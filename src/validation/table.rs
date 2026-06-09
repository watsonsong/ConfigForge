// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashSet};

use crate::{
    ConfigForgeError, Result,
    data::{DataResolver, DataRow, DataTable},
    diagnostic::DiagnosticLocation,
    meta::{LinkedField, LinkedFieldMeta, LinkedIndexMeta, LinkedTableMeta, TableKind, TargetTags},
};

use super::diagnostic::{ValidationReport, error_issue_from_result};
use super::struct_value::{
    validate_struct_child_path, validate_struct_field_value_for_row,
    validate_struct_occurrence_value, validate_supported_field_type, validate_value_type,
    validate_value_type_at_row_number, visible_required_struct_child_count,
};
use super::{ExportTableView, fragment_field_key, visible_row_for};

pub fn validate_normalized_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()> {
    validate_normalized_table_with_report(meta, table).into_result()
}

pub fn validate_normalized_table_with_report(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
) -> ValidationReport {
    let mut report = ValidationReport::new();
    collect_table_check(
        &mut report,
        "table.resolve",
        "type",
        DataResolver::resolve_table(meta, table, None).map(|_| ()),
        DiagnosticLocation {
            table: Some(meta.meta.name.clone()),
            ..DiagnosticLocation::default()
        },
    );

    for field in &meta.fields {
        let location = field_location(meta, field, None);
        collect_table_check(
            &mut report,
            "field.unsupported_type",
            "type",
            validate_supported_field_type(meta, field),
            location.clone(),
        );
        collect_table_check(
            &mut report,
            "field.required",
            "required",
            validate_field_presence(meta, table, field),
            location.clone(),
        );
        collect_table_check(
            &mut report,
            "field.type",
            "type",
            validate_field_values(meta, table, field),
            location.clone(),
        );
        collect_table_check(
            &mut report,
            "field.unique",
            "unique",
            validate_field_uniqueness(meta, table, field),
            location,
        );
    }

    if meta.meta.kind == TableKind::Object {
        collect_table_check(
            &mut report,
            "object_table.row_count",
            "object_table",
            validate_object_table_row_count(meta, table),
            DiagnosticLocation {
                table: Some(meta.meta.name.clone()),
                ..DiagnosticLocation::default()
            },
        );
        report.sort_by_stable_key();
        return report;
    }

    collect_table_check(
        &mut report,
        "fragment.value",
        "fragment",
        validate_fragment_values(meta, table, None),
        DiagnosticLocation {
            table: Some(meta.meta.name.clone()),
            ..DiagnosticLocation::default()
        },
    );

    for index in &meta.indexes {
        collect_table_check(
            &mut report,
            "index.unique",
            "unique",
            validate_index_uniqueness(meta, table, index),
            DiagnosticLocation {
                table: Some(meta.meta.name.clone()),
                ..DiagnosticLocation::default()
            },
        );
    }

    report.sort_by_stable_key();
    report
}

pub fn validate_normalized_table_for_target(
    meta: &LinkedTableMeta<'_>,
    target_tags: &TargetTags,
    table: &DataTable,
) -> Result<()> {
    validate_normalized_table_for_target_with_report(meta, target_tags, table).into_result()
}

pub fn validate_normalized_table_for_target_with_report(
    meta: &LinkedTableMeta<'_>,
    target_tags: &TargetTags,
    table: &DataTable,
) -> ValidationReport {
    let visible_fields = meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .map(|field| field.meta.name.as_str())
        .collect::<BTreeSet<_>>();
    let visible_rows = table
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            meta.meta
                .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
        })
        .map(|(row_index, _)| visible_row_for(table, row_index))
        .collect::<Vec<_>>();
    let mut report = ValidationReport::new();
    collect_table_check(
        &mut report,
        "table.resolve",
        "type",
        DataResolver::resolve_table_rows(
            meta,
            table,
            Some(target_tags),
            visible_rows.iter().map(|row| row.row_index),
        )
        .map(|_| ()),
        DiagnosticLocation {
            table: Some(meta.meta.name.clone()),
            ..DiagnosticLocation::default()
        },
    );
    let table_view = ExportTableView::new(meta, table, visible_fields, visible_rows);
    report.append(&mut validate_normalized_table_view_with_report(
        &table_view,
        Some(target_tags),
    ));
    report.sort_by_stable_key();
    report
}

pub(super) fn validate_normalized_table_view_with_report(
    table_view: &ExportTableView<'_, '_>,
    target_tags: Option<&TargetTags>,
) -> ValidationReport {
    let mut report = ValidationReport::new();
    let meta = table_view.meta;
    for field in meta
        .fields
        .iter()
        .filter(|field| table_view.is_field_visible(field.meta.name.as_str()))
    {
        let location = field_location(meta, field, None);
        collect_table_check(
            &mut report,
            "field.unsupported_type",
            "type",
            validate_supported_field_type(meta, field),
            location.clone(),
        );
        collect_table_check(
            &mut report,
            "field.required",
            "required",
            validate_field_presence_view(meta, table_view, field),
            location.clone(),
        );
        collect_table_check(
            &mut report,
            "field.type",
            "type",
            validate_field_values_view(meta, table_view, field, target_tags),
            location.clone(),
        );
        collect_table_check(
            &mut report,
            "field.unique",
            "unique",
            validate_field_uniqueness_view(meta, table_view, field),
            location,
        );
    }

    if meta.meta.kind == TableKind::Object {
        collect_table_check(
            &mut report,
            "object_table.row_count",
            "object_table",
            validate_object_table_view_row_count(meta, table_view),
            DiagnosticLocation {
                table: Some(meta.meta.name.clone()),
                ..DiagnosticLocation::default()
            },
        );
        report.sort_by_stable_key();
        return report;
    }

    collect_table_check(
        &mut report,
        "fragment.value",
        "fragment",
        validate_fragment_values_view(meta, table_view, target_tags),
        DiagnosticLocation {
            table: Some(meta.meta.name.clone()),
            ..DiagnosticLocation::default()
        },
    );

    let primary_index_name = meta.primary_index().map(|index| index.meta.name.as_str());
    for index in &meta.indexes {
        if let Some(target_tags) = target_tags {
            if !index.meta.is_exported_for(target_tags) {
                continue;
            }
        }
        let is_primary_index =
            index.meta.primary || primary_index_name == Some(index.meta.name.as_str());
        if let Some(field_name) = index
            .fields
            .iter()
            .find(|field| !table_view.is_field_visible(field.meta.name.as_str()))
            .map(|field| field.meta.name.as_str())
        {
            if is_primary_index {
                collect_table_check(
                    &mut report,
                    "index.visibility",
                    "unique",
                    Err(ConfigForgeError::Validation(format!(
                        "table `{}` primary index `{}` references non-exported field `{}` for current target",
                        meta.meta.name, index.meta.name, field_name
                    ))),
                    DiagnosticLocation {
                        table: Some(meta.meta.name.clone()),
                        field: Some(field_name.to_owned()),
                        ..DiagnosticLocation::default()
                    },
                );
            }
            continue;
        }
        collect_table_check(
            &mut report,
            "index.unique",
            "unique",
            validate_index_uniqueness_view(meta, table_view, index),
            DiagnosticLocation {
                table: Some(meta.meta.name.clone()),
                ..DiagnosticLocation::default()
            },
        );
    }

    report.sort_by_stable_key();
    report
}

fn collect_table_check(
    report: &mut ValidationReport,
    code: &'static str,
    validator: &'static str,
    result: Result<()>,
    location: DiagnosticLocation,
) {
    if let Some(issue) = error_issue_from_result(code, validator, result, location) {
        report.push(issue);
    }
}

fn field_location(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
    row: Option<(&DataTable, usize)>,
) -> DiagnosticLocation {
    let mut location = DiagnosticLocation {
        table: Some(meta.meta.name.clone()),
        field: Some(field.meta.name.clone()),
        ..DiagnosticLocation::default()
    };
    if let Some((table, row_index)) = row {
        location.row_index = Some(row_index);
        if let Some(row_source) = table.row_source(row_index) {
            location.source_path = Some(row_source.source_path.clone());
            location.source_row_number = Some(row_source.source_row_number);
        }
        if let Some(cell_source) = table.cell_source(row_index, &field.meta.name) {
            location.source_row_number = Some(cell_source.source_row_number);
            location.source_column_number = Some(cell_source.source_column_number);
            location.source_column_name = Some(cell_source.source_column_name.clone());
            location.column_index = Some(cell_source.column_index);
        }
    }
    location
}

fn validate_object_table_row_count(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()> {
    if table.rows.len() != 1 {
        return Err(ConfigForgeError::Validation(format!(
            "object table `{}` must normalize to exactly one logical row, got {}",
            meta.meta.name,
            table.rows.len()
        )));
    }
    Ok(())
}

fn validate_object_table_view_row_count(
    meta: &LinkedTableMeta<'_>,
    table_view: &ExportTableView<'_, '_>,
) -> Result<()> {
    if table_view.table.rows.len() != 1 {
        return Err(ConfigForgeError::Validation(format!(
            "object table `{}` must normalize to exactly one logical row, got {}",
            meta.meta.name,
            table_view.table.rows.len()
        )));
    }
    if table_view.visible_rows.len() > 1 {
        return Err(ConfigForgeError::Validation(format!(
            "object table `{}` export view must contain at most one logical row, got {}",
            meta.meta.name,
            table_view.visible_rows.len()
        )));
    }
    Ok(())
}

fn validate_index_uniqueness(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    index: &LinkedIndexMeta<'_>,
) -> Result<()> {
    if !index.meta.unique {
        return Ok(());
    }

    let mut seen = HashSet::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let key = index
            .fields
            .iter()
            .map(|field| {
                row.get(&field.meta.name)
                    .map(String::as_str)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        if !seen.insert(key.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} index `{}` duplicates unique key `{:?}`",
                meta.meta.name,
                row_index + 1,
                index.meta.name,
                key
            )));
        }
    }

    Ok(())
}

fn validate_index_uniqueness_view(
    meta: &LinkedTableMeta<'_>,
    table_view: &ExportTableView<'_, '_>,
    index: &LinkedIndexMeta<'_>,
) -> Result<()> {
    if !index.meta.unique {
        return Ok(());
    }

    let mut seen = HashSet::new();
    for visible_row in &table_view.visible_rows {
        let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
            continue;
        };
        let key = index
            .fields
            .iter()
            .map(|field| {
                row.get(&field.meta.name)
                    .map(String::as_str)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        if !seen.insert(key.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} index `{}` duplicates unique key `{:?}`",
                meta.meta.name,
                visible_row
                    .source_row_number
                    .unwrap_or(visible_row.row_index + 1),
                index.meta.name,
                key
            )));
        }
    }

    Ok(())
}

fn validate_field_presence(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    if matches!(&field.linked_field, LinkedField::Struct(_)) {
        return Ok(());
    }
    if !field.meta.required {
        return Ok(());
    }

    let present = table
        .rows
        .iter()
        .all(|row| row.contains_key(&field.meta.name));
    if !present {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` is missing field `{}`",
            meta.meta.name, field.meta.name
        )));
    }
    Ok(())
}

fn validate_field_presence_view(
    meta: &LinkedTableMeta<'_>,
    table_view: &ExportTableView<'_, '_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    if matches!(&field.linked_field, LinkedField::Struct(_)) {
        return Ok(());
    }
    if !field.meta.required {
        return Ok(());
    }

    let present = table_view.visible_rows.iter().all(|visible_row| {
        table_view
            .table
            .rows
            .get(visible_row.row_index)
            .map(|row| row.contains_key(&field.meta.name))
            .unwrap_or(false)
    });
    if !present {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` is missing field `{}`",
            meta.meta.name, field.meta.name
        )));
    }
    Ok(())
}

fn validate_field_values(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    for (row_index, row) in table.rows.iter().enumerate() {
        if matches!(&field.linked_field, LinkedField::Struct(_)) {
            validate_struct_field_value_for_row(meta, row, row_index + 1, field, None)?;
            continue;
        }
        let value = row
            .get(&field.meta.name)
            .map(String::as_str)
            .unwrap_or_default();
        if field.meta.required && value.trim().is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` is required",
                meta.meta.name,
                row_index + 1,
                field.meta.name
            )));
        }

        if value.trim().is_empty() {
            continue;
        }

        validate_value_type(meta, field, value, row_index)?;
    }
    Ok(())
}

fn validate_field_values_view(
    meta: &LinkedTableMeta<'_>,
    table_view: &ExportTableView<'_, '_>,
    field: &LinkedFieldMeta<'_>,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    for visible_row in &table_view.visible_rows {
        let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
            continue;
        };
        let row_number = visible_row
            .source_row_number
            .unwrap_or(visible_row.row_index + 1);
        if matches!(&field.linked_field, LinkedField::Struct(_)) {
            validate_struct_field_value_for_row(meta, row, row_number, field, target_tags)?;
            continue;
        }
        let value = row
            .get(&field.meta.name)
            .map(String::as_str)
            .unwrap_or_default();
        if field.meta.required && value.trim().is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` is required",
                meta.meta.name, row_number, field.meta.name
            )));
        }

        if value.trim().is_empty() {
            continue;
        }

        validate_value_type_at_row_number(meta, field, value, row_number, target_tags)?;
    }
    Ok(())
}

fn validate_fragment_values(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    for (row_index, row) in table.rows.iter().enumerate() {
        let row_number = table
            .row_source(row_index)
            .map(|source| source.source_row_number)
            .unwrap_or(row_index + 1);
        let active_fragments = table
            .row_source(row_index)
            .map(|source| source.fragments.as_slice())
            .unwrap_or(&[]);

        validate_fragment_values_for_row(meta, row, row_number, active_fragments, target_tags)?;
    }
    Ok(())
}

fn validate_fragment_values_view(
    meta: &LinkedTableMeta<'_>,
    table_view: &ExportTableView<'_, '_>,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    for visible_row in &table_view.visible_rows {
        let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
            continue;
        };
        let row_number = visible_row
            .source_row_number
            .unwrap_or(visible_row.row_index + 1);
        let active_fragments = table_view
            .table
            .row_source(visible_row.row_index)
            .map(|source| source.fragments.as_slice())
            .unwrap_or(&[]);

        validate_fragment_values_for_row(meta, row, row_number, active_fragments, target_tags)?;
    }
    Ok(())
}

fn validate_fragment_values_for_row(
    meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    row_number: usize,
    active_fragments: &[String],
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    for key in row.keys().filter(|key| key.contains('.')) {
        let Some((fragment_name, field_name)) = key.split_once('.') else {
            continue;
        };
        if matches!(
            meta.field(fragment_name).map(|field| &field.linked_field),
            Some(LinkedField::Struct(_))
        ) {
            continue;
        }
        let Some(fragment) = meta.fragment(fragment_name) else {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` references unknown fragment `{}`",
                meta.meta.name, row_number, key, fragment_name
            )));
        };
        if !active_fragments
            .iter()
            .any(|active| active == fragment_name)
        {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` belongs to inactive fragment `{}`",
                meta.meta.name, row_number, key, fragment_name
            )));
        }
        if let Some(struct_type) = fragment.struct_type() {
            validate_struct_child_path(
                meta.meta.name.as_str(),
                row_number,
                key,
                struct_type,
                field_name,
            )?;
        } else if fragment.field(field_name).is_none() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` references unknown fragment field `{}`",
                meta.meta.name, row_number, key, field_name
            )));
        }
    }

    for fragment_name in active_fragments {
        let Some(fragment) = meta.fragment(fragment_name) else {
            continue;
        };
        if let Some(target_tags) = target_tags {
            if !fragment.meta.is_exported_for(target_tags) {
                continue;
            }
        }
        if let Some(struct_type) = fragment.struct_type() {
            let has_effective_value = validate_struct_occurrence_value(
                meta.meta.name.as_str(),
                row,
                row_number,
                fragment.meta.name.as_str(),
                struct_type,
                target_tags,
            )?;
            if !has_effective_value
                && visible_required_struct_child_count(struct_type, target_tags) > 0
            {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{}` row {} fragment `{}` is missing required struct payload values",
                    meta.meta.name, row_number, fragment.meta.name
                )));
            }
            continue;
        }
        for field in &fragment.fields {
            if let Some(target_tags) = target_tags {
                if !field.meta.is_exported_for(target_tags) {
                    continue;
                }
            }
            validate_supported_field_type(meta, field)?;
            let field_path =
                fragment_field_key(fragment.meta.name.as_str(), field.meta.name.as_str());
            let value = row.get(&field_path).map(String::as_str).unwrap_or_default();
            if field.meta.required && value.trim().is_empty() {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{}` row {} field `{}` is required",
                    meta.meta.name, row_number, field_path
                )));
            }
            if value.trim().is_empty() {
                continue;
            }
            validate_value_type_at_row_number(meta, field, value, row_number, target_tags)?;
        }
    }
    Ok(())
}

fn validate_field_uniqueness(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    if !field.meta.unique {
        return Ok(());
    }

    let mut seen = HashSet::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let value = row
            .get(&field.meta.name)
            .map(String::as_str)
            .unwrap_or_default();
        if !seen.insert(value) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` duplicates unique value `{}`",
                meta.meta.name,
                row_index + 1,
                field.meta.name,
                value
            )));
        }
    }

    Ok(())
}

fn validate_field_uniqueness_view(
    meta: &LinkedTableMeta<'_>,
    table_view: &ExportTableView<'_, '_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    if !field.meta.unique {
        return Ok(());
    }

    let mut seen = HashSet::new();
    for visible_row in &table_view.visible_rows {
        let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
            continue;
        };
        let value = row
            .get(&field.meta.name)
            .map(String::as_str)
            .unwrap_or_default();
        if !seen.insert(value) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` duplicates unique value `{}`",
                meta.meta.name,
                visible_row
                    .source_row_number
                    .unwrap_or(visible_row.row_index + 1),
                field.meta.name,
                value
            )));
        }
    }

    Ok(())
}
