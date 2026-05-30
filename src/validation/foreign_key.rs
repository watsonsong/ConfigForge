// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use crate::{
    ConfigForgeError, Result,
    data::{
        DataRow, DataSet, DataTable, ValueSourceKind,
        typed_value::parse_list_cell_value,
        value_conversion::{json_value_is_empty, json_value_to_cell, toml_default_to_json_value},
    },
    meta::{
        LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta, LinkedIndexMeta, LinkedMetaModel,
        LinkedTableMeta, TargetTags,
    },
};

use super::{ExportTableView, ExportValidationView};

pub(super) fn validate_foreign_keys_full(
    meta: &LinkedMetaModel<'_>,
    data_set: &DataSet,
) -> Result<()> {
    for source_table_meta in &meta.tables {
        let Some(source_table) = data_set.table(&source_table_meta.meta.name) else {
            continue;
        };
        for foreign_key in &source_table_meta.foreign_keys {
            let target_table_meta = meta.table(foreign_key.target_table_name).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "table `{}` foreign key `{}` references unknown target table `{}`",
                    source_table_meta.meta.name, foreign_key.name, foreign_key.target_table_name
                ))
            })?;
            let target_index = target_table_meta
                .index(foreign_key.target_index_name)
                .ok_or_else(|| {
                    ConfigForgeError::Validation(format!(
                        "table `{}` foreign key `{}` references unknown target index `{}.{}`",
                        source_table_meta.meta.name,
                        foreign_key.name,
                        target_table_meta.meta.name,
                        foreign_key.target_index_name
                    ))
                })?;
            let target_table = data_set
                .table(&target_table_meta.meta.name)
                .ok_or_else(|| {
                    ConfigForgeError::Validation(format!(
                        "data set is missing foreign key target table `{}`",
                        target_table_meta.meta.name
                    ))
                })?;
            let target_keys = build_foreign_key_target_keys(target_table, target_index, None);
            validate_foreign_key_source_rows(
                source_table_meta,
                source_table,
                foreign_key,
                &target_keys,
                None,
            )?;
        }
    }
    Ok(())
}

pub(super) fn validate_foreign_keys_for_target(
    meta: &LinkedMetaModel<'_>,
    export_view: &ExportValidationView<'_, '_>,
    target_tags: &TargetTags,
) -> Result<()> {
    for source_table_view in &export_view.tables {
        for foreign_key in &source_table_view.meta.foreign_keys {
            if !foreign_key.is_exported_for(target_tags) {
                continue;
            }
            validate_foreign_key_source_field_visibility(source_table_view, foreign_key)?;
            let target_table_meta = meta.table(foreign_key.target_table_name).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "table `{}` foreign key `{}` references unknown target table `{}`",
                    source_table_view.meta.meta.name,
                    foreign_key.name,
                    foreign_key.target_table_name
                ))
            })?;
            let Some(target_table_view) = export_view.table(foreign_key.target_table_name) else {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{}` foreign key `{}` references target table `{}` which is not exported for current target",
                    source_table_view.meta.meta.name,
                    foreign_key.name,
                    foreign_key.target_table_name
                )));
            };
            let target_index = target_table_meta
                .index(foreign_key.target_index_name)
                .ok_or_else(|| {
                    ConfigForgeError::Validation(format!(
                        "table `{}` foreign key `{}` references unknown target index `{}.{}`",
                        source_table_view.meta.meta.name,
                        foreign_key.name,
                        target_table_meta.meta.name,
                        foreign_key.target_index_name
                    ))
                })?;
            if !target_index.meta.is_exported_for(target_tags) {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{}` foreign key `{}` references target index `{}.{}` which is not exported for current target",
                    source_table_view.meta.meta.name,
                    foreign_key.name,
                    target_table_meta.meta.name,
                    target_index.meta.name
                )));
            }
            validate_foreign_key_target_field_visibility(
                source_table_view,
                foreign_key,
                target_table_view,
                target_index,
            )?;
            let target_keys = build_foreign_key_target_keys(
                target_table_view.table,
                target_index,
                Some(target_table_view),
            );
            validate_foreign_key_source_rows(
                source_table_view.meta,
                source_table_view.table,
                foreign_key,
                &target_keys,
                Some(source_table_view),
            )?;
        }
    }
    Ok(())
}

fn validate_foreign_key_source_field_visibility(
    source_table_view: &ExportTableView<'_, '_>,
    foreign_key: &LinkedForeignKeyMeta<'_>,
) -> Result<()> {
    for field in &foreign_key.fields {
        if !source_table_view.is_field_visible(field.meta.name.as_str()) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` foreign key `{}` references non-exported source field `{}` for current target",
                source_table_view.meta.meta.name, foreign_key.name, field.meta.name
            )));
        }
    }
    Ok(())
}

fn validate_foreign_key_target_field_visibility(
    source_table_view: &ExportTableView<'_, '_>,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    target_table_view: &ExportTableView<'_, '_>,
    target_index: &LinkedIndexMeta<'_>,
) -> Result<()> {
    for field in &target_index.fields {
        if !target_table_view.is_field_visible(field.meta.name.as_str()) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` foreign key `{}` references non-exported target index field `{}.{}` for current target",
                source_table_view.meta.meta.name,
                foreign_key.name,
                target_table_view.meta.meta.name,
                field.meta.name
            )));
        }
    }
    Ok(())
}

fn build_foreign_key_target_keys(
    target_table: &DataTable,
    target_index: &LinkedIndexMeta<'_>,
    target_table_view: Option<&ExportTableView<'_, '_>>,
) -> HashSet<Vec<String>> {
    let mut keys = HashSet::new();
    match target_table_view {
        Some(view) => {
            // Target-specific validation must reject references to rows filtered out for that target.
            for visible_row in &view.visible_rows {
                let Some(row) = target_table.rows.get(visible_row.row_index) else {
                    continue;
                };
                keys.insert(foreign_key_row_key(row, &target_index.fields));
            }
        }
        None => {
            for row in &target_table.rows {
                keys.insert(foreign_key_row_key(row, &target_index.fields));
            }
        }
    }
    keys
}

fn validate_foreign_key_source_rows(
    source_table_meta: &LinkedTableMeta<'_>,
    source_table: &DataTable,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    target_keys: &HashSet<Vec<String>>,
    source_table_view: Option<&ExportTableView<'_, '_>>,
) -> Result<()> {
    match source_table_view {
        Some(view) => {
            for visible_row in &view.visible_rows {
                let Some(row) = source_table.rows.get(visible_row.row_index) else {
                    continue;
                };
                let row_number = visible_row
                    .source_row_number
                    .unwrap_or(visible_row.row_index + 1);
                validate_foreign_key_row(
                    source_table_meta,
                    row,
                    row_number,
                    foreign_key,
                    target_keys,
                )?;
            }
        }
        None => {
            for (row_index, row) in source_table.rows.iter().enumerate() {
                let row_number = source_table
                    .row_source(row_index)
                    .map(|source| source.source_row_number)
                    .unwrap_or(row_index + 1);
                validate_foreign_key_row(
                    source_table_meta,
                    row,
                    row_number,
                    foreign_key,
                    target_keys,
                )?;
            }
        }
    }
    Ok(())
}

fn validate_foreign_key_row(
    source_table_meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    row_number: usize,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    target_keys: &HashSet<Vec<String>>,
) -> Result<()> {
    if let Some(list_field) = foreign_key_list_source_field(foreign_key) {
        return validate_list_foreign_key_row(
            source_table_meta,
            row,
            row_number,
            foreign_key,
            list_field,
            target_keys,
        );
    }

    let key = foreign_key_row_key(row, &foreign_key.fields);
    let empty_count = key.iter().filter(|value| value.trim().is_empty()).count();
    // All-empty keys can mean "no relationship"; partial composite keys are always ambiguous.
    if empty_count == key.len() {
        if foreign_key.allow_empty {
            return Ok(());
        }
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} foreign key `{}` is empty but `allow_empty = false`",
            source_table_meta.meta.name, row_number, foreign_key.name
        )));
    }
    if empty_count > 0 {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} foreign key `{}` has partial empty key `{:?}`",
            source_table_meta.meta.name, row_number, foreign_key.name, key
        )));
    }
    if !target_keys.contains(&key) {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} foreign key `{}` references missing `{}.{}` key `{:?}`",
            source_table_meta.meta.name,
            row_number,
            foreign_key.name,
            foreign_key.target_table_name,
            foreign_key.target_index_name,
            key
        )));
    }
    Ok(())
}

fn validate_list_foreign_key_row(
    source_table_meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    row_number: usize,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    list_field: &LinkedFieldMeta<'_>,
    target_keys: &HashSet<Vec<String>>,
) -> Result<()> {
    let raw_cell = row
        .get(&list_field.meta.name)
        .map(String::as_str)
        .unwrap_or_default();
    let default_raw;
    let raw = if raw_cell.trim().is_empty() {
        match list_field.meta.default.as_ref() {
            Some(default) => {
                let default_value = toml_default_to_json_value(default);
                if json_value_is_empty(&default_value) {
                    raw_cell
                } else {
                    default_raw = json_value_to_cell(&default_value).map_err(|_| {
                        ConfigForgeError::Validation(format!(
                            "table `{}` row {} foreign key `{}` source field `{}` default must be a JSON array",
                            source_table_meta.meta.name, row_number, foreign_key.name, list_field.meta.name
                        ))
                    })?;
                    default_raw.as_str()
                }
            }
            None => raw_cell,
        }
    } else {
        raw_cell
    };
    if raw.trim().is_empty() {
        if foreign_key.allow_empty {
            return Ok(());
        }
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} foreign key `{}` is empty but `allow_empty = false`",
            source_table_meta.meta.name, row_number, foreign_key.name
        )));
    }
    let LinkedField::List(element_type) = &list_field.linked_field else {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` foreign key `{}` expected list source field `{}`",
            source_table_meta.meta.name, foreign_key.name, list_field.meta.name
        )));
    };
    let elements = parse_list_cell_value(
        &source_table_meta.meta.name,
        row_number,
        &list_field.meta.name,
        element_type,
        raw,
        None,
        ValueSourceKind::ExplicitCell,
    )
    .map_err(|error| match error {
        ConfigForgeError::Validation(message) => ConfigForgeError::Validation(message.replace(
            " field `",
            &format!(" foreign key `{}` source field `", foreign_key.name),
        )),
        other => other,
    })?;
    if elements.is_empty() && !foreign_key.allow_empty {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} foreign key `{}` is empty but `allow_empty = false`",
            source_table_meta.meta.name, row_number, foreign_key.name
        )));
    }
    for (index, element) in elements.iter().enumerate() {
        let key_value = element.scalar_export_string().ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "table `{}` row {} foreign key `{}` source field `{}[{}]` must be a scalar key value",
                source_table_meta.meta.name, row_number, foreign_key.name, list_field.meta.name, index
            ))
        })?;
        let key = vec![key_value];
        if !target_keys.contains(&key) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} foreign key `{}` source field `{}[{}]` references missing `{}.{}` key `{:?}`",
                source_table_meta.meta.name,
                row_number,
                foreign_key.name,
                list_field.meta.name,
                index,
                foreign_key.target_table_name,
                foreign_key.target_index_name,
                key
            )));
        }
    }
    Ok(())
}

fn foreign_key_list_source_field<'a, 'meta>(
    foreign_key: &'a LinkedForeignKeyMeta<'meta>,
) -> Option<&'a LinkedFieldMeta<'meta>> {
    foreign_key
        .fields
        .iter()
        .find(|field| matches!(field.linked_field, LinkedField::List(_)))
}

fn foreign_key_row_key(row: &DataRow, fields: &[LinkedFieldMeta<'_>]) -> Vec<String> {
    // FK keys preserve raw cell strings to avoid target-dependent typed canonicalization surprises.
    fields
        .iter()
        .map(|field| {
            row.get(&field.meta.name)
                .map(String::as_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}
