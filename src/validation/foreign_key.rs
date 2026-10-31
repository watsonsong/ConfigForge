// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    data::{
        DataRow, DataSet, DataTable, ValueSourceKind,
        typed_value::parse_list_cell_value,
        value_conversion::{json_value_is_empty, json_value_to_cell, toml_default_to_json_value},
    },
    diagnostic::code::{
        CF_DATA_INVALID_FOREIGN_KEY, CF_DATA_INVALID_TYPE, CF_VALIDATION_INVALID_CONFIG,
    },
    meta::{
        LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta, LinkedIndexMeta, LinkedMetaModel,
        LinkedTableMeta, TargetTags,
    },
};

use super::{
    ExportTableView, ExportValidationView,
    key_policy::{raw_cell_key, raw_cell_value},
};

fn foreign_key_error(
    source_table_meta: &LinkedTableMeta<'_>,
    row_number: usize,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    message: String,
    current_value: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_INVALID_FOREIGN_KEY,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                table: Some(source_table_meta.meta.name.clone()),
                field: foreign_key
                    .fields
                    .first()
                    .map(|field| field.meta.name.clone()),
                source_row_number: Some(row_number),
                ..DiagnosticLocation::default()
            },
        )
        .with_source("validation.foreign_key")
        .with_current_value(current_value)
        .with_expected(format!(
            "existing `{}`.`{}` key; allow_empty = {}",
            foreign_key.target_table_name, foreign_key.target_index_name, foreign_key.allow_empty
        )),
    )
}

fn foreign_key_config_error(
    source_table_meta: &LinkedTableMeta<'_>,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    super::validation_diagnostic_error(
        CF_VALIDATION_INVALID_CONFIG,
        "validation.foreign_key.config",
        message,
        DiagnosticLocation {
            table: Some(source_table_meta.meta.name.clone()),
            field: foreign_key
                .fields
                .first()
                .map(|field| field.meta.name.clone()),
            ..DiagnosticLocation::default()
        },
        current_value,
        expected,
        "Check foreign key target table/index and target-aware export visibility.",
    )
}

fn foreign_key_source_type_error(
    source_table_meta: &LinkedTableMeta<'_>,
    row_number: usize,
    _foreign_key: &LinkedForeignKeyMeta<'_>,
    field_name: impl Into<String>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    super::validation_diagnostic_error(
        CF_DATA_INVALID_TYPE,
        "validation.foreign_key.source",
        message,
        DiagnosticLocation {
            table: Some(source_table_meta.meta.name.clone()),
            field: Some(field_name.into()),
            source_row_number: Some(row_number),
            ..DiagnosticLocation::default()
        },
        current_value,
        expected,
        "Check foreign key source field type and cell/default value shape.",
    )
}

pub(super) fn validate_foreign_keys_full(
    meta: &LinkedMetaModel<'_>,
    data_set: &DataSet,
) -> Result<()> {
    let mut target_key_cache = ForeignKeyTargetKeyCache::new(ForeignKeyTargetViewScope::Full);
    for source_table_meta in &meta.tables {
        let Some(source_table) = data_set.table(&source_table_meta.meta.name) else {
            continue;
        };
        for foreign_key in &source_table_meta.foreign_keys {
            let target_table_meta = meta.table(foreign_key.target_table_name).ok_or_else(|| {
                foreign_key_config_error(
                    source_table_meta,
                    foreign_key,
                    format!(
                        "table `{}` foreign key `{}` references unknown target table `{}`",
                        source_table_meta.meta.name,
                        foreign_key.name,
                        foreign_key.target_table_name
                    ),
                    foreign_key.target_table_name,
                    "declared foreign key target table",
                )
            })?;
            let target_index = target_table_meta
                .index(foreign_key.target_index_name)
                .ok_or_else(|| {
                    foreign_key_config_error(
                        source_table_meta,
                        foreign_key,
                        format!(
                            "table `{}` foreign key `{}` references unknown target index `{}.{}`",
                            source_table_meta.meta.name,
                            foreign_key.name,
                            target_table_meta.meta.name,
                            foreign_key.target_index_name
                        ),
                        format!(
                            "{}.{}",
                            target_table_meta.meta.name, foreign_key.target_index_name
                        ),
                        "declared target table index",
                    )
                })?;
            let target_table = data_set
                .table(&target_table_meta.meta.name)
                .ok_or_else(|| {
                    foreign_key_config_error(
                        source_table_meta,
                        foreign_key,
                        format!(
                            "data set is missing foreign key target table `{}`",
                            target_table_meta.meta.name
                        ),
                        "missing target table data",
                        format!("data table `{}`", target_table_meta.meta.name),
                    )
                })?;
            let target_keys = target_key_cache.get_or_build(
                &target_table_meta.meta.name,
                target_table,
                target_index,
                None,
            );
            validate_foreign_key_source_rows(
                source_table_meta,
                source_table,
                foreign_key,
                target_keys,
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
    let mut target_key_cache = ForeignKeyTargetKeyCache::new(ForeignKeyTargetViewScope::Target);
    for source_table_view in &export_view.tables {
        for foreign_key in &source_table_view.meta.foreign_keys {
            if !foreign_key.is_exported_for(target_tags) {
                continue;
            }
            validate_foreign_key_source_field_visibility(source_table_view, foreign_key)?;
            let target_table_meta = meta.table(foreign_key.target_table_name).ok_or_else(|| {
                foreign_key_config_error(
                    source_table_view.meta,
                    foreign_key,
                    format!(
                        "table `{}` foreign key `{}` references unknown target table `{}`",
                        source_table_view.meta.meta.name,
                        foreign_key.name,
                        foreign_key.target_table_name
                    ),
                    foreign_key.target_table_name,
                    "declared foreign key target table",
                )
            })?;
            let Some(target_table_view) = export_view.table(foreign_key.target_table_name) else {
                return Err(foreign_key_config_error(
                    source_table_view.meta,
                    foreign_key,
                    format!(
                        "table `{}` foreign key `{}` references target table `{}` which is not exported for current target",
                        source_table_view.meta.meta.name,
                        foreign_key.name,
                        foreign_key.target_table_name
                    ),
                    foreign_key.target_table_name,
                    "target table exported for current target",
                ));
            };
            let target_index = target_table_meta
                .index(foreign_key.target_index_name)
                .ok_or_else(|| {
                    foreign_key_config_error(
                        source_table_view.meta,
                        foreign_key,
                        format!(
                            "table `{}` foreign key `{}` references unknown target index `{}.{}`",
                            source_table_view.meta.meta.name,
                            foreign_key.name,
                            target_table_meta.meta.name,
                            foreign_key.target_index_name
                        ),
                        format!(
                            "{}.{}",
                            target_table_meta.meta.name, foreign_key.target_index_name
                        ),
                        "declared target table index",
                    )
                })?;
            if !target_index.meta.is_exported_for(target_tags) {
                return Err(foreign_key_config_error(
                    source_table_view.meta,
                    foreign_key,
                    format!(
                        "table `{}` foreign key `{}` references target index `{}.{}` which is not exported for current target",
                        source_table_view.meta.meta.name,
                        foreign_key.name,
                        target_table_meta.meta.name,
                        target_index.meta.name
                    ),
                    format!("{}.{}", target_table_meta.meta.name, target_index.meta.name),
                    "target index exported for current target",
                ));
            }
            validate_foreign_key_target_field_visibility(
                source_table_view,
                foreign_key,
                target_table_view,
                target_index,
            )?;
            let target_keys = target_key_cache.get_or_build(
                &target_table_meta.meta.name,
                target_table_view.table,
                target_index,
                Some(target_table_view),
            );
            validate_foreign_key_source_rows(
                source_table_view.meta,
                source_table_view.table,
                foreign_key,
                target_keys,
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
            return Err(foreign_key_config_error(
                source_table_view.meta,
                foreign_key,
                format!(
                    "table `{}` foreign key `{}` references non-exported source field `{}` for current target",
                    source_table_view.meta.meta.name, foreign_key.name, field.meta.name
                ),
                field.meta.name.as_str(),
                "source field exported for current target",
            ));
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
            return Err(foreign_key_config_error(
                source_table_view.meta,
                foreign_key,
                format!(
                    "table `{}` foreign key `{}` references non-exported target index field `{}.{}` for current target",
                    source_table_view.meta.meta.name,
                    foreign_key.name,
                    target_table_view.meta.meta.name,
                    field.meta.name
                ),
                format!("{}.{}", target_table_view.meta.meta.name, field.meta.name),
                "target index field exported for current target",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ForeignKeyTargetViewScope {
    Full,
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ForeignKeyTargetKeyCacheKey {
    target_table_name: String,
    target_index_name: String,
    view_scope: ForeignKeyTargetViewScope,
}

struct ForeignKeyTargetKeyCache {
    view_scope: ForeignKeyTargetViewScope,
    keys_by_target: HashMap<ForeignKeyTargetKeyCacheKey, ForeignKeyTargetKeySet>,
    #[cfg(test)]
    build_count: usize,
}

impl ForeignKeyTargetKeyCache {
    fn new(view_scope: ForeignKeyTargetViewScope) -> Self {
        Self {
            view_scope,
            keys_by_target: HashMap::new(),
            #[cfg(test)]
            build_count: 0,
        }
    }

    fn get_or_build(
        &mut self,
        target_table_name: &str,
        target_table: &DataTable,
        target_index: &LinkedIndexMeta<'_>,
        target_table_view: Option<&ExportTableView<'_, '_>>,
    ) -> &ForeignKeyTargetKeySet {
        let cache_key = ForeignKeyTargetKeyCacheKey {
            target_table_name: target_table_name.to_owned(),
            target_index_name: target_index.meta.name.clone(),
            view_scope: self.view_scope,
        };
        if !self.keys_by_target.contains_key(&cache_key) {
            let target_keys =
                ForeignKeyTargetKeySet::build(target_table, target_index, target_table_view);
            self.keys_by_target.insert(cache_key.clone(), target_keys);
            #[cfg(test)]
            {
                self.build_count += 1;
            }
        }
        self.keys_by_target
            .get(&cache_key)
            .expect("target key cache entry should exist")
    }

    #[cfg(test)]
    fn build_count(&self) -> usize {
        self.build_count
    }
}

enum ForeignKeyTargetKeySet {
    Single(HashSet<String>),
    Composite(HashSet<Vec<String>>),
}

impl ForeignKeyTargetKeySet {
    fn build(
        target_table: &DataTable,
        target_index: &LinkedIndexMeta<'_>,
        target_table_view: Option<&ExportTableView<'_, '_>>,
    ) -> Self {
        if target_index.fields.len() == 1 {
            let field = &target_index.fields[0];
            let mut keys = HashSet::new();
            for_each_target_row(target_table, target_table_view, |row| {
                keys.insert(raw_cell_value(row, &field.meta.name));
            });
            return Self::Single(keys);
        }

        let mut keys = HashSet::new();
        for_each_target_row(target_table, target_table_view, |row| {
            keys.insert(raw_cell_key(row, &target_index.fields));
        });
        Self::Composite(keys)
    }

    fn contains_single(&self, key: &str) -> bool {
        match self {
            Self::Single(keys) => keys.contains(key),
            Self::Composite(_) => false,
        }
    }

    fn contains_composite(&self, key: &[String]) -> bool {
        match self {
            Self::Single(_) => false,
            Self::Composite(keys) => keys.contains(key),
        }
    }
}

fn for_each_target_row(
    target_table: &DataTable,
    target_table_view: Option<&ExportTableView<'_, '_>>,
    mut visit: impl FnMut(&DataRow),
) {
    match target_table_view {
        Some(view) => {
            // Target-specific validation must reject references to rows filtered out for that target.
            for visible_row in &view.visible_rows {
                if let Some(row) = target_table.rows.get(visible_row.row_index) {
                    visit(row);
                }
            }
        }
        None => {
            for row in &target_table.rows {
                visit(row);
            }
        }
    }
}

fn validate_foreign_key_source_rows(
    source_table_meta: &LinkedTableMeta<'_>,
    source_table: &DataTable,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    target_keys: &ForeignKeyTargetKeySet,
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
                )
                .map_err(|error| {
                    source_table.enrich_error_location(visible_row.row_index, error)
                })?;
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
                )
                .map_err(|error| source_table.enrich_error_location(row_index, error))?;
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
    target_keys: &ForeignKeyTargetKeySet,
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

    if foreign_key.fields.len() == 1 {
        return validate_single_foreign_key_row(
            source_table_meta,
            row,
            row_number,
            foreign_key,
            target_keys,
        );
    }

    validate_composite_foreign_key_row(source_table_meta, row, row_number, foreign_key, target_keys)
}

fn validate_single_foreign_key_row(
    source_table_meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    row_number: usize,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    target_keys: &ForeignKeyTargetKeySet,
) -> Result<()> {
    let key = raw_cell_value(row, &foreign_key.fields[0].meta.name);
    if key.trim().is_empty() {
        if foreign_key.allow_empty {
            return Ok(());
        }
        let message = format!(
            "table `{}` row {} foreign key `{}` is empty but `allow_empty = false`",
            source_table_meta.meta.name, row_number, foreign_key.name
        );
        return Err(foreign_key_error(
            source_table_meta,
            row_number,
            foreign_key,
            message,
            key,
        ));
    }
    if !target_keys.contains_single(&key) {
        let current_value = format!("{:?}", vec![key.as_str()]);
        let message = format!(
            "table `{}` row {} foreign key `{}` references missing `{}.{}` key `{:?}`",
            source_table_meta.meta.name,
            row_number,
            foreign_key.name,
            foreign_key.target_table_name,
            foreign_key.target_index_name,
            vec![key.as_str()]
        );
        return Err(foreign_key_error(
            source_table_meta,
            row_number,
            foreign_key,
            message,
            current_value,
        ));
    }
    Ok(())
}

fn validate_composite_foreign_key_row(
    source_table_meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    row_number: usize,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    target_keys: &ForeignKeyTargetKeySet,
) -> Result<()> {
    let key = raw_cell_key(row, &foreign_key.fields);
    let empty_count = key.iter().filter(|value| value.trim().is_empty()).count();
    // All-empty keys can mean "no relationship"; partial composite keys are always ambiguous.
    if empty_count == key.len() {
        if foreign_key.allow_empty {
            return Ok(());
        }
        let message = format!(
            "table `{}` row {} foreign key `{}` is empty but `allow_empty = false`",
            source_table_meta.meta.name, row_number, foreign_key.name
        );
        return Err(foreign_key_error(
            source_table_meta,
            row_number,
            foreign_key,
            message,
            format!("{:?}", key),
        ));
    }
    if empty_count > 0 {
        let message = format!(
            "table `{}` row {} foreign key `{}` has partial empty key `{:?}`",
            source_table_meta.meta.name, row_number, foreign_key.name, key
        );
        return Err(foreign_key_error(
            source_table_meta,
            row_number,
            foreign_key,
            message,
            format!("{:?}", key),
        ));
    }
    if !target_keys.contains_composite(&key) {
        let message = format!(
            "table `{}` row {} foreign key `{}` references missing `{}.{}` key `{:?}`",
            source_table_meta.meta.name,
            row_number,
            foreign_key.name,
            foreign_key.target_table_name,
            foreign_key.target_index_name,
            key
        );
        return Err(foreign_key_error(
            source_table_meta,
            row_number,
            foreign_key,
            message,
            format!("{:?}", key),
        ));
    }
    Ok(())
}

fn validate_list_foreign_key_row(
    source_table_meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    row_number: usize,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    list_field: &LinkedFieldMeta<'_>,
    target_keys: &ForeignKeyTargetKeySet,
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
                        foreign_key_source_type_error(
                            source_table_meta,
                            row_number,
                            foreign_key,
                            list_field.meta.name.as_str(),
                            format!(
                                "table `{}` row {} foreign key `{}` source field `{}` default must be a JSON array",
                                source_table_meta.meta.name, row_number, foreign_key.name, list_field.meta.name
                            ),
                            "non-array default value",
                            "JSON array default for list foreign key source field",
                        )
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
        let message = format!(
            "table `{}` row {} foreign key `{}` is empty but `allow_empty = false`",
            source_table_meta.meta.name, row_number, foreign_key.name
        );
        return Err(foreign_key_error(
            source_table_meta,
            row_number,
            foreign_key,
            message,
            raw,
        ));
    }
    let LinkedField::List(element_type) = &list_field.linked_field else {
        return Err(foreign_key_source_type_error(
            source_table_meta,
            row_number,
            foreign_key,
            list_field.meta.name.as_str(),
            format!(
                "table `{}` foreign key `{}` expected list source field `{}`",
                source_table_meta.meta.name, foreign_key.name, list_field.meta.name
            ),
            list_field.meta.name.as_str(),
            "list source field for list foreign key",
        ));
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
        ConfigForgeError::Validation(message) => foreign_key_source_type_error(
            source_table_meta,
            row_number,
            foreign_key,
            list_field.meta.name.as_str(),
            message.replace(
                " field `",
                &format!(" foreign key `{}` source field `", foreign_key.name),
            ),
            raw,
            "list value containing scalar foreign key elements",
        ),
        other => other,
    })?;
    if elements.is_empty() && !foreign_key.allow_empty {
        let message = format!(
            "table `{}` row {} foreign key `{}` is empty but `allow_empty = false`",
            source_table_meta.meta.name, row_number, foreign_key.name
        );
        return Err(foreign_key_error(
            source_table_meta,
            row_number,
            foreign_key,
            message,
            raw,
        ));
    }
    for (index, element) in elements.iter().enumerate() {
        let key_value = element.scalar_export_string().ok_or_else(|| {
            foreign_key_source_type_error(
                source_table_meta,
                row_number,
                foreign_key,
                format!("{}[{index}]", list_field.meta.name),
                format!(
                    "table `{}` row {} foreign key `{}` source field `{}[{}]` must be a scalar key value",
                    source_table_meta.meta.name, row_number, foreign_key.name, list_field.meta.name, index
                ),
                "non-scalar list element",
                "scalar foreign key element value",
            )
        })?;
        if !target_keys.contains_single(&key_value) {
            let current_value = format!("{:?}", vec![key_value.as_str()]);
            let message = format!(
                "table `{}` row {} foreign key `{}` source field `{}[{}]` references missing `{}.{}` key `{:?}`",
                source_table_meta.meta.name,
                row_number,
                foreign_key.name,
                list_field.meta.name,
                index,
                foreign_key.target_table_name,
                foreign_key.target_index_name,
                vec![key_value.as_str()]
            );
            return Err(foreign_key_error(
                source_table_meta,
                row_number,
                foreign_key,
                message,
                current_value,
            ));
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::{data::DataTable, meta::MetaModel};

    use super::*;
    use crate::validation::{ExportTableView, VisibleRow};

    fn linked_items_meta() -> MetaModel {
        MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "region"
                type = "string"

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true

                [[tables.indexes]]
                name = "by_id_region"
                fields = ["id", "region"]
                unique = true
            "#,
        )
        .expect("meta should load")
    }

    fn items_table() -> DataTable {
        DataTable::new(
            "items",
            vec![
                BTreeMap::from([
                    ("id".to_owned(), "1".to_owned()),
                    ("region".to_owned(), "cn".to_owned()),
                ]),
                BTreeMap::from([
                    ("id".to_owned(), "2".to_owned()),
                    ("region".to_owned(), "us".to_owned()),
                ]),
            ],
        )
    }

    #[test]
    fn target_key_cache_reuses_single_index_builds_and_keeps_composite_separate() {
        let meta = linked_items_meta();
        let linked_meta = meta.link().expect("meta should link");
        let table_meta = linked_meta.table("items").expect("table should link");
        let by_id = table_meta.index("by_id").expect("single index should link");
        let by_id_region = table_meta
            .index("by_id_region")
            .expect("composite index should link");
        let table = items_table();
        let mut cache = ForeignKeyTargetKeyCache::new(ForeignKeyTargetViewScope::Full);

        {
            let keys = cache.get_or_build("items", &table, by_id, None);
            assert!(keys.contains_single("1"));
            assert!(keys.contains_single("2"));
            assert!(!keys.contains_composite(&["1".to_owned()]));
        }
        assert_eq!(cache.build_count(), 1);

        {
            let keys = cache.get_or_build("items", &table, by_id, None);
            assert!(keys.contains_single("1"));
        }
        assert_eq!(cache.build_count(), 1);

        {
            let keys = cache.get_or_build("items", &table, by_id_region, None);
            assert!(keys.contains_composite(&["1".to_owned(), "cn".to_owned()]));
            assert!(keys.contains_composite(&["2".to_owned(), "us".to_owned()]));
            assert!(!keys.contains_single("1"));
        }
        assert_eq!(cache.build_count(), 2);
    }

    #[test]
    fn target_key_cache_respects_target_view_scope_rows() {
        let meta = linked_items_meta();
        let linked_meta = meta.link().expect("meta should link");
        let table_meta = linked_meta.table("items").expect("table should link");
        let by_id = table_meta.index("by_id").expect("single index should link");
        let table = items_table();
        let visible_fields = BTreeSet::from(["id", "region"]);
        let visible_rows = vec![VisibleRow {
            row_index: 0,
            source_row_number: Some(1),
        }];
        let table_view =
            ExportTableView::new(table_meta, &table, visible_fields, visible_rows, None);
        let mut cache = ForeignKeyTargetKeyCache::new(ForeignKeyTargetViewScope::Target);

        let keys = cache.get_or_build("items", &table, by_id, Some(&table_view));

        assert!(keys.contains_single("1"));
        assert!(!keys.contains_single("2"));
        assert_eq!(cache.build_count(), 1);
    }
}
