// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticSeverity, Result,
    data::{DataResolver, DataRow, DataTable, ResolvedRow, ResolvedTable},
    diagnostic::{
        DiagnosticLocation,
        code::{CF_DATA_DUPLICATE_KEY, CF_DATA_INVALID_TYPE, CF_DATA_MISSING_REQUIRED_FIELD},
    },
    meta::{
        LinkedField, LinkedFieldMeta, LinkedIndexMeta, LinkedTableMeta, TableKind, TargetTags,
        VectorFormat,
    },
};

use super::diagnostic::{ValidationReport, error_issues_from_result, validation_error_message};
use super::key_policy::{raw_cell_key, resolved_path_value, scalar_export_value};
use super::struct_value::{
    validate_struct_child_path, validate_struct_field_value_for_resolved_row,
    validate_struct_field_value_for_row, validate_struct_occurrence_value,
    validate_supported_field_type, validate_value_type, validate_value_type_at_row_number,
    visible_required_struct_child_count,
};
use super::{ExportTableView, fragment_field_key};

#[cfg(test)]
pub(crate) fn validate_normalized_table(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
) -> Result<()> {
    validate_normalized_table_with_report(meta, table).into_result()
}

pub fn validate_normalized_table_with_report(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
) -> ValidationReport {
    let mut report = ValidationReport::new();
    let resolved_table = match DataResolver::resolve_table(meta, table, None) {
        Ok(resolved_table) => Some(resolved_table),
        Err(error) => {
            collect_table_check(
                &mut report,
                "table.resolve",
                "type",
                Err(error),
                DiagnosticLocation {
                    table: Some(meta.meta.name.clone()),
                    ..DiagnosticLocation::default()
                },
            );
            None
        }
    };

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
            validate_field_presence(meta, table, resolved_table.as_ref(), field),
            location.clone(),
        );
        collect_table_check(
            &mut report,
            "field.type",
            "type",
            validate_field_values(meta, table, resolved_table.as_ref(), field),
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
        validate_fragment_values(meta, table, resolved_table.as_ref(), None),
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

    if !report.has_error() && table_view.resolved_table().is_none() {
        let visible_row_indices = table_view
            .visible_rows
            .iter()
            .map(|row| row.row_index)
            .collect::<Vec<_>>();
        collect_table_check(
            &mut report,
            "table.resolve",
            "type",
            DataResolver::resolve_table_rows(
                meta,
                table_view.table,
                target_tags,
                visible_row_indices.iter().copied(),
            )
            .map(|_| ()),
            DiagnosticLocation {
                table: Some(meta.meta.name.clone()),
                ..DiagnosticLocation::default()
            },
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
        if let Some(target_tags) = target_tags
            && !index.meta.is_exported_for(target_tags)
        {
            continue;
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
                    Err(table_schema_error(
                        format!(
                            "table `{}` primary index `{}` references non-exported field `{}` for current target",
                            meta.meta.name, index.meta.name, field_name
                        ),
                        DiagnosticLocation {
                            table: Some(meta.meta.name.clone()),
                            field: Some(field_name.to_owned()),
                            ..DiagnosticLocation::default()
                        },
                        format!("{}.{}", index.meta.name, field_name),
                        "primary index fields exported for current target",
                    )),
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
    report.extend(error_issues_from_result(code, validator, result, location));
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
            location.source_path = Some(cell_source.source_path.clone());
            location.source_row_number = Some(cell_source.source_row_number);
            location.source_column_number = Some(cell_source.source_column_number);
            location.source_column_name = Some(cell_source.source_column_name.clone());
            location.column_index = Some(cell_source.column_index);
        }
    }
    location
}

fn data_validation_error(
    code: &'static str,
    source: &'static str,
    message: String,
    location: DiagnosticLocation,
    current_value: Option<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    let mut diagnostic = Diagnostic::new(code, DiagnosticSeverity::Error, message, location)
        .with_source(source)
        .with_expected(expected);
    if let Some(current_value) = current_value {
        diagnostic = diagnostic.with_current_value(current_value);
    }
    ConfigForgeError::from_diagnostic(diagnostic)
}

fn table_schema_error(
    message: String,
    location: DiagnosticLocation,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    super::validation_diagnostic_error(
        CF_DATA_INVALID_TYPE,
        "validation.table.schema",
        message,
        location,
        current_value,
        expected,
        "Check table kind, fragment layout, target visibility, and schema/data alignment.",
    )
}

fn fragment_value_error(
    message: String,
    location: DiagnosticLocation,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    super::validation_diagnostic_error(
        CF_DATA_INVALID_TYPE,
        "validation.fragment",
        message,
        location,
        current_value,
        expected,
        "Check active fragment metadata and row payload values.",
    )
}

fn missing_required_field_error(
    message: String,
    location: DiagnosticLocation,
    current_value: Option<String>,
) -> ConfigForgeError {
    data_validation_error(
        CF_DATA_MISSING_REQUIRED_FIELD,
        "validation.required",
        message,
        location,
        current_value,
        "non-empty required field value or configured default",
    )
}

fn invalid_field_type_error(
    message: String,
    location: DiagnosticLocation,
    current_value: String,
    field: &LinkedFieldMeta<'_>,
) -> ConfigForgeError {
    data_validation_error(
        CF_DATA_INVALID_TYPE,
        "validation.type",
        message,
        location,
        Some(current_value),
        expected_field_value(field),
    )
}

fn duplicate_key_error(
    message: String,
    location: DiagnosticLocation,
    current_value: String,
) -> ConfigForgeError {
    data_validation_error(
        CF_DATA_DUPLICATE_KEY,
        "validation.unique",
        message,
        location,
        Some(current_value),
        "unique field/index key value",
    )
}

fn expected_field_value(field: &LinkedFieldMeta<'_>) -> String {
    expected_linked_field_value(&field.linked_field)
}

fn expected_linked_field_value(field: &LinkedField<'_>) -> String {
    match field {
        LinkedField::Primitive(primitive) => primitive.as_str().to_owned(),
        LinkedField::Enum(enum_meta) => {
            let values = enum_meta
                .values
                .iter()
                .map(|value| value.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("one of enum `{}` values: {values}", enum_meta.name)
        }
        LinkedField::Struct(struct_type) => {
            format!("JSON object for struct `{}`", struct_type.meta.name)
        }
        LinkedField::List(element_type) => {
            format!(
                "JSON array of {}",
                expected_linked_field_value(element_type)
            )
        }
    }
}

fn validate_object_table_row_count(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()> {
    if table.rows.len() != 1 {
        return Err(table_schema_error(
            format!(
                "object table `{}` must normalize to exactly one logical row, got {}",
                meta.meta.name,
                table.rows.len()
            ),
            DiagnosticLocation {
                table: Some(meta.meta.name.clone()),
                ..DiagnosticLocation::default()
            },
            table.rows.len().to_string(),
            "exactly one logical row for object table",
        ));
    }
    Ok(())
}

fn validate_object_table_view_row_count(
    meta: &LinkedTableMeta<'_>,
    table_view: &ExportTableView<'_, '_>,
) -> Result<()> {
    if table_view.table.rows.len() != 1 {
        return Err(table_schema_error(
            format!(
                "object table `{}` must normalize to exactly one logical row, got {}",
                meta.meta.name,
                table_view.table.rows.len()
            ),
            DiagnosticLocation {
                table: Some(meta.meta.name.clone()),
                ..DiagnosticLocation::default()
            },
            table_view.table.rows.len().to_string(),
            "exactly one logical row for object table",
        ));
    }
    if table_view.visible_rows.len() > 1 {
        return Err(table_schema_error(
            format!(
                "object table `{}` export view must contain at most one logical row, got {}",
                meta.meta.name,
                table_view.visible_rows.len()
            ),
            DiagnosticLocation {
                table: Some(meta.meta.name.clone()),
                ..DiagnosticLocation::default()
            },
            table_view.visible_rows.len().to_string(),
            "at most one visible logical row for object table export view",
        ));
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

    validate_index_key_fields(meta, index)?;

    let mut seen = HashSet::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let key = raw_cell_key(row, &index.fields);
        if !seen.insert(key.clone()) {
            let message = format!(
                "table `{}` row {} index `{}` duplicates unique key `{:?}`",
                meta.meta.name,
                row_index + 1,
                index.meta.name,
                key
            );
            return Err(duplicate_key_error(
                message,
                DiagnosticLocation {
                    table: Some(meta.meta.name.clone()),
                    row_index: Some(row_index),
                    source_row_number: table
                        .row_source(row_index)
                        .map(|source| source.source_row_number),
                    ..DiagnosticLocation::default()
                },
                format!("{}={:?}", index.meta.name, key),
            ));
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

    validate_index_key_fields(meta, index)?;

    let mut seen = HashSet::new();
    for visible_row in &table_view.visible_rows {
        let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
            continue;
        };
        let key = raw_cell_key(row, &index.fields);
        if !seen.insert(key.clone()) {
            let row_number = visible_row
                .source_row_number
                .unwrap_or(visible_row.row_index + 1);
            let message = format!(
                "table `{}` row {} index `{}` duplicates unique key `{:?}`",
                meta.meta.name, row_number, index.meta.name, key
            );
            return Err(duplicate_key_error(
                message,
                DiagnosticLocation {
                    table: Some(meta.meta.name.clone()),
                    row_index: Some(visible_row.row_index),
                    source_row_number: Some(row_number),
                    ..DiagnosticLocation::default()
                },
                format!("{}={:?}", index.meta.name, key),
            ));
        }
    }

    Ok(())
}

fn validate_index_key_fields(
    meta: &LinkedTableMeta<'_>,
    index: &LinkedIndexMeta<'_>,
) -> Result<()> {
    for field in &index.fields {
        if field
            .primitive_type()
            .is_some_and(|primitive| primitive.is_builtin_value_type())
        {
            return Err(table_schema_error(
                format!(
                    "table `{}` index `{}` field `{}` uses unsupported builtin value key type `{}`",
                    meta.meta.name, index.meta.name, field.meta.name, field.meta.field_type
                ),
                DiagnosticLocation {
                    table: Some(meta.meta.name.clone()),
                    field: Some(field.meta.name.clone()),
                    ..DiagnosticLocation::default()
                },
                field.meta.field_type.clone(),
                "scalar field key type with stable equality semantics",
            ));
        }
    }
    Ok(())
}

fn validate_field_presence(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    resolved_table: Option<&ResolvedTable>,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    if matches!(&field.linked_field, LinkedField::Struct(_)) {
        return Ok(());
    }
    if !field.meta.required {
        return Ok(());
    }

    let present = table.rows.iter().enumerate().all(|(row_index, row)| {
        let resolved_value = resolved_table
            .and_then(|resolved_table| resolved_table.rows.get(row_index))
            .and_then(|row| row.get(&field.meta.name));
        field_has_source(row, resolved_value, field)
    });
    if !present {
        let message = format!(
            "table `{}` is missing field `{}`",
            meta.meta.name, field.meta.name
        );
        return Err(missing_required_field_error(
            message,
            field_location(meta, field, None),
            None,
        ));
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

    let present =
        table_view
            .visible_rows
            .iter()
            .enumerate()
            .all(|(visible_row_index, visible_row)| {
                table_view
                    .table
                    .rows
                    .get(visible_row.row_index)
                    .map(|row| {
                        field_has_source(
                            row,
                            table_view.resolved_value(visible_row_index, &field.meta.name),
                            field,
                        )
                    })
                    .unwrap_or(false)
            });
    if !present {
        let message = format!(
            "table `{}` is missing field `{}`",
            meta.meta.name, field.meta.name
        );
        return Err(missing_required_field_error(
            message,
            field_location(meta, field, None),
            None,
        ));
    }
    Ok(())
}

fn validate_field_values(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    resolved_table: Option<&ResolvedTable>,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    for (row_index, row) in table.rows.iter().enumerate() {
        if matches!(&field.linked_field, LinkedField::Struct(_)) {
            let resolved_value = resolved_table
                .and_then(|resolved_table| resolved_table.rows.get(row_index))
                .and_then(|row| row.get(&field.meta.name));
            if resolved_table.is_some() {
                validate_struct_field_value_for_resolved_row(
                    meta,
                    row_index + 1,
                    field,
                    resolved_value,
                    None,
                )?;
            } else {
                validate_struct_field_value_for_row(meta, row, row_index + 1, field, None)?;
            }
            continue;
        }
        let resolved_value = resolved_table
            .and_then(|resolved_table| resolved_table.rows.get(row_index))
            .and_then(|row| row.get(&field.meta.name));
        let value = row
            .get(&field.meta.name)
            .map(String::as_str)
            .unwrap_or_default();
        let has_value = field_has_effective_value(row, resolved_value, field);
        if field.meta.required && value.trim().is_empty() {
            if has_value {
                continue;
            }
            let message = format!(
                "table `{}` row {} field `{}` is required",
                meta.meta.name,
                row_index + 1,
                field.meta.name
            );
            return Err(missing_required_field_error(
                message,
                field_location(meta, field, Some((table, row_index))),
                Some(value.to_owned()),
            ));
        }

        if value.trim().is_empty() && !has_value {
            continue;
        }

        if field
            .primitive_type()
            .is_some_and(|primitive| primitive.is_builtin_value_type())
        {
            continue;
        }

        if resolved_value.is_some() {
            continue;
        }

        validate_value_type(meta, field, value, row_index).map_err(|error| {
            invalid_field_type_error(
                validation_error_message(error),
                field_location(meta, field, Some((table, row_index))),
                value.to_owned(),
                field,
            )
        })?;
    }
    Ok(())
}

fn validate_field_values_view(
    meta: &LinkedTableMeta<'_>,
    table_view: &ExportTableView<'_, '_>,
    field: &LinkedFieldMeta<'_>,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    for visible_row_index in 0..table_view.visible_rows.len() {
        let visible_row = &table_view.visible_rows[visible_row_index];
        let Some(row) = table_view.visible_raw_row(visible_row_index) else {
            continue;
        };
        let row_number = visible_row
            .source_row_number
            .unwrap_or(visible_row.row_index + 1);
        if matches!(&field.linked_field, LinkedField::Struct(_)) {
            let resolved_value = table_view.resolved_value(visible_row_index, &field.meta.name);
            if table_view.resolved_table().is_some() {
                validate_struct_field_value_for_resolved_row(
                    meta,
                    row_number,
                    field,
                    resolved_value,
                    target_tags,
                )?;
            } else {
                validate_struct_field_value_for_row(meta, row, row_number, field, target_tags)?;
            }
            continue;
        }
        let value = table_view
            .raw_cell(visible_row_index, &field.meta.name)
            .unwrap_or_default();
        let resolved_value = table_view.resolved_value(visible_row_index, &field.meta.name);
        let has_value = field_has_effective_value(row, resolved_value, field);
        if field.meta.required && value.trim().is_empty() {
            if has_value {
                continue;
            }
            let message = format!(
                "table `{}` row {} field `{}` is required",
                meta.meta.name, row_number, field.meta.name
            );
            return Err(missing_required_field_error(
                message,
                field_location(meta, field, Some((table_view.table, visible_row.row_index))),
                Some(value.to_owned()),
            ));
        }

        if value.trim().is_empty() && !has_value {
            continue;
        }

        if field
            .primitive_type()
            .is_some_and(|primitive| primitive.is_builtin_value_type())
        {
            continue;
        }

        if resolved_value.is_some() {
            continue;
        }

        validate_value_type_at_row_number(meta, field, value, row_number, target_tags).map_err(
            |error| {
                invalid_field_type_error(
                    validation_error_message(error),
                    field_location(meta, field, Some((table_view.table, visible_row.row_index))),
                    value.to_owned(),
                    field,
                )
            },
        )?;
    }
    Ok(())
}

fn field_has_source(
    row: &DataRow,
    resolved_value: Option<&crate::data::ResolvedValue>,
    field: &LinkedFieldMeta<'_>,
) -> bool {
    resolved_value.is_some()
        || row.contains_key(&field.meta.name)
        || field.meta.default.is_some()
        || expanded_vector_component_paths(field)
            .iter()
            .any(|path| row.contains_key(path))
}

fn field_has_effective_value(
    row: &DataRow,
    resolved_value: Option<&crate::data::ResolvedValue>,
    field: &LinkedFieldMeta<'_>,
) -> bool {
    resolved_value.is_some()
        || row
            .get(&field.meta.name)
            .is_some_and(|value| !value.trim().is_empty())
        || field.meta.default.is_some()
        || expanded_vector_component_paths(field)
            .iter()
            .any(|path| row.get(path).is_some_and(|value| !value.trim().is_empty()))
}

fn expanded_vector_component_paths(field: &LinkedFieldMeta<'_>) -> Vec<String> {
    if field.meta.vector_format != Some(VectorFormat::Expanded)
        || field
            .primitive_type()
            .is_none_or(|primitive| primitive.vector_dimension().is_none())
    {
        return Vec::new();
    }
    field
        .meta
        .vector_component_names()
        .into_iter()
        .map(|component| format!("{}.{}", field.meta.name, component))
        .collect()
}

fn is_expanded_vector_component(field: &LinkedFieldMeta<'_>, component: &str) -> bool {
    field.meta.vector_format == Some(VectorFormat::Expanded)
        && field
            .primitive_type()
            .is_some_and(|primitive| primitive.vector_dimension().is_some())
        && !component.contains('.')
        && field
            .meta
            .vector_component_names()
            .iter()
            .any(|name| name == component)
}

fn validate_fragment_values(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    resolved_table: Option<&ResolvedTable>,
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

        validate_fragment_values_for_row(
            meta,
            row,
            resolved_table.and_then(|resolved_table| resolved_table.rows.get(row_index)),
            row_number,
            active_fragments,
            target_tags,
        )?;
    }
    Ok(())
}

fn validate_fragment_values_view(
    meta: &LinkedTableMeta<'_>,
    table_view: &ExportTableView<'_, '_>,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    for (visible_row_index, visible_row) in table_view.visible_rows.iter().enumerate() {
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

        validate_fragment_values_for_row(
            meta,
            row,
            table_view.visible_resolved_row(visible_row_index),
            row_number,
            active_fragments,
            target_tags,
        )?;
    }
    Ok(())
}

fn validate_fragment_values_for_row(
    meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    resolved_row: Option<&ResolvedRow>,
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
        ) || meta
            .field(fragment_name)
            .is_some_and(|field| is_expanded_vector_component(field, field_name))
        {
            continue;
        }
        let Some(fragment) = meta.fragment(fragment_name) else {
            return Err(fragment_value_error(
                format!(
                    "table `{}` row {} field `{}` references unknown fragment `{}`",
                    meta.meta.name, row_number, key, fragment_name
                ),
                DiagnosticLocation {
                    table: Some(meta.meta.name.clone()),
                    field: Some(key.clone()),
                    source_row_number: Some(row_number),
                    ..DiagnosticLocation::default()
                },
                fragment_name,
                "declared fragment on this table",
            ));
        };
        if !active_fragments
            .iter()
            .any(|active| active == fragment_name)
        {
            return Err(fragment_value_error(
                format!(
                    "table `{}` row {} field `{}` belongs to inactive fragment `{}`",
                    meta.meta.name, row_number, key, fragment_name
                ),
                DiagnosticLocation {
                    table: Some(meta.meta.name.clone()),
                    field: Some(key.clone()),
                    source_row_number: Some(row_number),
                    ..DiagnosticLocation::default()
                },
                fragment_name,
                "active fragment for this row",
            ));
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
            return Err(fragment_value_error(
                format!(
                    "table `{}` row {} field `{}` references unknown fragment field `{}`",
                    meta.meta.name, row_number, key, field_name
                ),
                DiagnosticLocation {
                    table: Some(meta.meta.name.clone()),
                    field: Some(key.clone()),
                    source_row_number: Some(row_number),
                    ..DiagnosticLocation::default()
                },
                key,
                format!("declared field on fragment `{fragment_name}`"),
            ));
        }
    }

    for fragment_name in active_fragments {
        let Some(fragment) = meta.fragment(fragment_name) else {
            continue;
        };
        if let Some(target_tags) = target_tags
            && !fragment.meta.is_exported_for(target_tags)
        {
            continue;
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
                return Err(missing_required_field_error(
                    format!(
                        "table `{}` row {} fragment `{}` is missing required struct payload values",
                        meta.meta.name, row_number, fragment.meta.name
                    ),
                    DiagnosticLocation {
                        table: Some(meta.meta.name.clone()),
                        field: Some(fragment.meta.name.clone()),
                        source_row_number: Some(row_number),
                        ..DiagnosticLocation::default()
                    },
                    None,
                ));
            }
            continue;
        }
        for field in &fragment.fields {
            if let Some(target_tags) = target_tags
                && !field.meta.is_exported_for(target_tags)
            {
                continue;
            }
            validate_supported_field_type(meta, field)?;
            let field_path =
                fragment_field_key(fragment.meta.name.as_str(), field.meta.name.as_str());
            let value = scalar_export_value(
                resolved_path_value(resolved_row, &field_path),
                row.get(&field_path).map(String::as_str),
            );
            if field.meta.required && value.trim().is_empty() {
                return Err(missing_required_field_error(
                    format!(
                        "table `{}` row {} field `{}` is required",
                        meta.meta.name, row_number, field_path
                    ),
                    DiagnosticLocation {
                        table: Some(meta.meta.name.clone()),
                        field: Some(field_path),
                        source_row_number: Some(row_number),
                        ..DiagnosticLocation::default()
                    },
                    Some(value.clone()),
                ));
            }
            if value.trim().is_empty() {
                continue;
            }
            if field
                .primitive_type()
                .is_some_and(|primitive| primitive.is_builtin_value_type())
            {
                continue;
            }
            validate_value_type_at_row_number(meta, field, &value, row_number, target_tags)?;
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
            let message = format!(
                "table `{}` row {} field `{}` duplicates unique value `{}`",
                meta.meta.name,
                row_index + 1,
                field.meta.name,
                value
            );
            return Err(duplicate_key_error(
                message,
                field_location(meta, field, Some((table, row_index))),
                value.to_owned(),
            ));
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
            let row_number = visible_row
                .source_row_number
                .unwrap_or(visible_row.row_index + 1);
            let message = format!(
                "table `{}` row {} field `{}` duplicates unique value `{}`",
                meta.meta.name, row_number, field.meta.name, value
            );
            return Err(duplicate_key_error(
                message,
                field_location(meta, field, Some((table_view.table, visible_row.row_index))),
                value.to_owned(),
            ));
        }
    }

    Ok(())
}
