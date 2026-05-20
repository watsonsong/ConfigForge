use std::collections::{BTreeSet, HashSet};

use crate::{
    data::{DataRow, DataSet, DataTable},
    meta::{
        LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta, LinkedIndexMeta, LinkedMetaModel,
        LinkedStructType, LinkedTableMeta, PrimitiveFieldType, TargetTags, ValueLayout,
    },
    ConfigForgeError, Result,
};

mod builtins;
mod context;
mod diagnostic;
mod registry;

pub use context::{
    visible_row_for, ExportTableView, ExportValidationView, ValidationContext, VisibleRow,
};
pub use diagnostic::{ValidationIssue, ValidationIssueLocation, ValidationReport};
pub use registry::{
    CompiledValidationPlan, CompiledValidator, ValidatorFactory, ValidatorRegistry,
};

pub fn validate_data_set(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<()> {
    let data_set = normalize_data_set(meta, data_set)?;
    validate_normalized_data_set(meta, &data_set)
}

pub fn validate_data_set_with_report(
    meta: &LinkedMetaModel<'_>,
    data_set: &DataSet,
) -> Result<ValidationReport> {
    let data_set = normalize_data_set(meta, data_set)?;
    validate_normalized_data_set_with_report(meta, &data_set)
}

pub fn validate_data_set_for_target(
    meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<()> {
    let data_set = normalize_data_set(meta, data_set)?;
    validate_normalized_data_set_for_target(meta, target_tags, &data_set)
}

pub fn validate_data_set_for_target_with_report(
    meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<ValidationReport> {
    let data_set = normalize_data_set(meta, data_set)?;
    validate_normalized_data_set_for_target_with_report(meta, target_tags, &data_set)
}

pub fn normalize_data_set(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<DataSet> {
    let mut tables = Vec::with_capacity(data_set.tables().len());
    for table in data_set.tables() {
        if let Some(table_meta) = meta.table(&table.name) {
            tables.push(normalize_table(table_meta, table)?);
        } else {
            tables.push(table.clone());
        }
    }
    DataSet::try_new(tables)
}

pub fn validate_normalized_data_set(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<()> {
    validate_normalized_data_set_with_report(meta, data_set).map(|_| ())
}

pub fn validate_normalized_data_set_with_report(
    meta: &LinkedMetaModel<'_>,
    data_set: &DataSet,
) -> Result<ValidationReport> {
    for table_meta in &meta.tables {
        let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "data set is missing table `{}`",
                table_meta.meta.name
            ))
        })?;
        validate_normalized_table(table_meta, table)?;
    }

    validate_foreign_keys_full(meta, data_set)?;

    let registry = ValidatorRegistry::with_builtin_validators();
    let plan = CompiledValidationPlan::compile(meta, &registry)?;
    let context = ValidationContext::new(
        meta,
        None,
        data_set,
        ExportValidationView::full(meta, data_set),
    );
    report_into_result(plan.run(&context))
}

pub fn validate_normalized_data_set_for_target(
    meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<()> {
    validate_normalized_data_set_for_target_with_report(meta, target_tags, data_set).map(|_| ())
}

pub fn validate_normalized_data_set_for_target_with_report(
    meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<ValidationReport> {
    let export_view = ExportValidationView::for_target(meta, data_set, target_tags)?;
    for table_view in &export_view.tables {
        validate_normalized_table_view(table_view, Some(target_tags))?;
    }

    validate_foreign_keys_for_target(meta, &export_view, target_tags)?;

    let registry = ValidatorRegistry::with_builtin_validators();
    let plan = CompiledValidationPlan::compile(meta, &registry)?;
    let context = ValidationContext::new(meta, Some(target_tags.clone()), data_set, export_view);
    report_into_result(plan.run(&context))
}

fn report_into_result(report: ValidationReport) -> Result<ValidationReport> {
    if report.has_error() {
        return Err(ConfigForgeError::Validation(
            report.error_messages().join("; "),
        ));
    }
    Ok(report)
}

pub fn validate_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()> {
    let table = normalize_table(meta, table)?;
    validate_normalized_table(meta, &table)
}

pub fn validate_table_for_target(
    meta: &LinkedTableMeta<'_>,
    target_tags: &TargetTags,
    table: &DataTable,
) -> Result<()> {
    let table = normalize_table(meta, table)?;
    validate_normalized_table_for_target(meta, target_tags, &table)
}

pub fn validate_normalized_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()> {
    for field in &meta.fields {
        validate_supported_field_type(meta, field)?;
        validate_field_presence(meta, table, field)?;
        validate_field_values(meta, table, field)?;
        validate_field_uniqueness(meta, table, field)?;
    }

    validate_fragment_values(meta, table, None)?;

    for index in &meta.indexes {
        validate_index_uniqueness(meta, table, index)?;
    }

    Ok(())
}

pub fn validate_normalized_table_for_target(
    meta: &LinkedTableMeta<'_>,
    target_tags: &TargetTags,
    table: &DataTable,
) -> Result<()> {
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
        .collect();
    let table_view = ExportTableView::new(meta, table, visible_fields, visible_rows);
    validate_normalized_table_view(&table_view, Some(target_tags))
}

fn validate_normalized_table_view(
    table_view: &ExportTableView<'_, '_>,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    let meta = table_view.meta;
    for field in meta
        .fields
        .iter()
        .filter(|field| table_view.is_field_visible(field.meta.name.as_str()))
    {
        validate_supported_field_type(meta, field)?;
        validate_field_presence_view(meta, table_view, field)?;
        validate_field_values_view(meta, table_view, field, target_tags)?;
        validate_field_uniqueness_view(meta, table_view, field)?;
    }

    validate_fragment_values_view(meta, table_view, target_tags)?;

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
                return Err(ConfigForgeError::Validation(format!(
                    "table `{}` primary index `{}` references non-exported field `{}` for current target",
                    meta.meta.name, index.meta.name, field_name
                )));
            }
            continue;
        }
        validate_index_uniqueness_view(meta, table_view, index)?;
    }

    Ok(())
}

pub fn normalize_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<DataTable> {
    apply_field_defaults(meta, table)
}

pub fn apply_field_defaults(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<DataTable> {
    let mut table = table.clone();
    for row_index in 0..table.rows.len() {
        let active_fragments = table
            .row_source(row_index)
            .map(|source| source.fragments.clone())
            .unwrap_or_default();
        let row = &mut table.rows[row_index];
        for field in &meta.fields {
            let needs_default = row
                .get(&field.meta.name)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true);
            if !needs_default {
                continue;
            }

            if let Some(default) = &field.meta.default {
                row.insert(
                    field.meta.name.clone(),
                    default_value_to_cell(meta, field, default)?,
                );
            }
        }

        for fragment_name in active_fragments {
            let Some(fragment) = meta.fragment(&fragment_name) else {
                continue;
            };
            for field in &fragment.fields {
                let key = fragment_field_key(fragment.meta.name.as_str(), field.meta.name.as_str());
                let needs_default = row
                    .get(&key)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true);
                if !needs_default {
                    continue;
                }

                if let Some(default) = &field.meta.default {
                    row.insert(key, default_value_to_cell(meta, field, default)?);
                }
            }
        }
    }

    Ok(table)
}

fn default_value_to_cell(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
    default: &toml::Value,
) -> Result<String> {
    if let Some(value) = default.as_str() {
        return Ok(value.to_owned());
    }
    if let Some(value) = default.as_integer() {
        return Ok(value.to_string());
    }
    if let Some(value) = default.as_float() {
        return Ok(value.to_string());
    }
    if let Some(value) = default.as_bool() {
        return Ok(value.to_string());
    }

    Err(ConfigForgeError::Validation(format!(
        "table `{}` field `{}` has unsupported default value `{}`",
        meta.meta.name, field.meta.name, default
    )))
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

fn validate_foreign_keys_full(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<()> {
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

fn validate_foreign_keys_for_target(
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
                source_table_view.meta.meta.name,
                foreign_key.name,
                field.meta.name
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

fn validate_field_presence(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
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
        if fragment.field(field_name).is_none() {
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

fn validate_supported_field_type(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    match (&field.linked_field, field.meta.layout) {
        (LinkedField::Struct(_), None | Some(ValueLayout::JsonCell)) => Ok(()),
        (LinkedField::Struct(_), Some(ValueLayout::ScalarCell | ValueLayout::ExpandedValue)) => {
            Err(ConfigForgeError::Validation(format!(
                "table `{}` field `{}` uses struct type `{}` with unsupported layout; only json_cell is supported in this phase",
                meta.meta.name, field.meta.name, field.meta.field_type
            )))
        }
        (_, None | Some(ValueLayout::ScalarCell)) => Ok(()),
        (_, Some(ValueLayout::JsonCell | ValueLayout::ExpandedValue)) => Err(
            ConfigForgeError::Validation(format!(
                "table `{}` field `{}` uses unsupported layout; non-struct fields only support scalar_cell in this phase",
                meta.meta.name, field.meta.name
            )),
        ),
    }
}

fn validate_value_type(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
    value: &str,
    row_index: usize,
) -> Result<()> {
    validate_value_type_at_row_number(meta, field, value, row_index + 1, None)
}

fn validate_value_type_at_row_number(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
    value: &str,
    row_number: usize,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    if let LinkedField::Struct(struct_type) = &field.linked_field {
        return validate_struct_json_cell(
            meta.meta.name.as_str(),
            row_number,
            field.meta.name.as_str(),
            struct_type,
            value,
            target_tags,
        );
    }

    if !validate_scalar_value(field, value) {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
            meta.meta.name, row_number, field.meta.name, value, field.meta.field_type
        )));
    }

    Ok(())
}

fn validate_scalar_value(field: &LinkedFieldMeta<'_>, value: &str) -> bool {
    match &field.linked_field {
        LinkedField::Primitive(PrimitiveFieldType::Int) => value.parse::<i64>().is_ok(),
        LinkedField::Primitive(PrimitiveFieldType::Float) => value.parse::<f64>().is_ok(),
        LinkedField::Primitive(PrimitiveFieldType::Bool) => {
            matches!(value, "true" | "false" | "1" | "0")
        }
        LinkedField::Primitive(
            PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
        ) => true,
        LinkedField::Enum(enum_meta) => enum_meta
            .values
            .iter()
            .any(|enum_value| enum_value.name == value),
        LinkedField::Struct(_) => false,
    }
}

fn validate_struct_json_cell(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
    raw: &str,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|source| {
        ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` has invalid struct JSON: {}",
            table_name, row_number, field_path, source
        ))
    })?;
    let object = value.as_object().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` must be a JSON object for struct `{}`",
            table_name, row_number, field_path, struct_type.meta.name
        ))
    })?;

    for key in object.keys() {
        if !struct_type
            .fields
            .iter()
            .any(|field| field.meta.name == key.as_str())
        {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` contains unknown struct key `{}`",
                table_name, row_number, field_path, key
            )));
        }
    }

    for child in &struct_type.fields {
        if let Some(target_tags) = target_tags {
            if !child.meta.is_exported_for(target_tags) {
                continue;
            }
        }
        let child_path = format!("{field_path}.{}", child.meta.name);
        let child_value = object.get(&child.meta.name);
        let default_value = child.meta.default.as_ref().map(toml_default_to_json_value);
        let effective_value = child_value.or(default_value.as_ref());
        let is_empty = effective_value.map(json_value_is_empty).unwrap_or(true);

        if child.meta.required && is_empty {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` is required",
                table_name, row_number, child_path
            )));
        }
        if is_empty {
            continue;
        }

        match &child.linked_field {
            LinkedField::Struct(nested_struct) => {
                let raw = effective_value.expect("checked non-empty").to_string();
                validate_struct_json_cell(
                    table_name,
                    row_number,
                    &child_path,
                    nested_struct,
                    &raw,
                    target_tags,
                )?;
            }
            _ => {
                let scalar = json_value_to_scalar_cell(
                    effective_value.expect("checked non-empty"),
                    table_name,
                    row_number,
                    &child_path,
                )?;
                if !validate_scalar_value(child, &scalar) {
                    return Err(ConfigForgeError::Validation(format!(
                        "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
                        table_name, row_number, child_path, scalar, child.meta.field_type
                    )));
                }
            }
        }
    }

    Ok(())
}

fn toml_default_to_json_value(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(value) => serde_json::Value::String(value.clone()),
        toml::Value::Integer(value) => serde_json::Value::Number((*value).into()),
        toml::Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(value) => serde_json::Value::Bool(*value),
        _ => serde_json::Value::Null,
    }
}

fn json_value_is_empty(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::Null)
        || value.as_str().map(str::trim).is_some_and(str::is_empty)
}

fn json_value_to_scalar_cell(
    value: &serde_json::Value,
    table_name: &str,
    row_number: usize,
    field_path: &str,
) -> Result<String> {
    match value {
        serde_json::Value::String(value) => Ok(value.clone()),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        serde_json::Value::Bool(value) => Ok(value.to_string()),
        _ => Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` must be a scalar value",
            table_name, row_number, field_path
        ))),
    }
}

fn fragment_field_key(fragment_name: &str, field_name: &str) -> String {
    format!("{fragment_name}.{field_name}")
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

#[cfg(test)]
mod tests;
