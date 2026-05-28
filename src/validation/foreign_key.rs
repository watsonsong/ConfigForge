// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use crate::{
    data::{DataRow, DataSet, DataTable},
    meta::{
        LinkedFieldMeta, LinkedForeignKeyMeta, LinkedIndexMeta, LinkedMetaModel, LinkedTableMeta,
        TargetTags,
    },
    ConfigForgeError, Result,
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
