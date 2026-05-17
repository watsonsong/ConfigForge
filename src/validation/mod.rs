use std::collections::{BTreeSet, HashSet};

use crate::{
    data::{DataSet, DataTable},
    meta::{
        LinkedField, LinkedFieldMeta, LinkedIndexMeta, LinkedMetaModel, LinkedTableMeta,
        PrimitiveFieldType, TargetTags,
    },
    ConfigForgeError, Result,
};

mod builtins;
mod context;
mod diagnostic;
mod registry;

pub use context::{ExportTableView, ExportValidationView, ValidationContext, VisibleRow};
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
    let table = filter_table_rows_for_target(meta, target_tags, table);
    let visible_fields = meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .map(|field| field.meta.name.as_str())
        .collect::<BTreeSet<_>>();
    let visible_rows = (0..table.rows.len())
        .map(|row_index| VisibleRow {
            row_index,
            source_row_number: Some(row_index + 1),
        })
        .collect();
    let table_view = ExportTableView::new(meta, &table, visible_fields, visible_rows);
    validate_normalized_table_view(&table_view, Some(target_tags))
}

fn validate_normalized_table_view(
    table_view: &ExportTableView<'_, '_>,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    let meta = table_view.meta;
    let visible_table = DataTable::new(
        table_view.table.name.clone(),
        table_view
            .visible_rows
            .iter()
            .filter_map(|visible_row| table_view.table.rows.get(visible_row.row_index).cloned())
            .collect(),
    );
    for field in meta
        .fields
        .iter()
        .filter(|field| table_view.is_field_visible(field.meta.name.as_str()))
    {
        validate_supported_field_type(meta, field)?;
        validate_field_presence(meta, &visible_table, field)?;
        validate_field_values(meta, &visible_table, field)?;
        validate_field_uniqueness(meta, &visible_table, field)?;
    }

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
        validate_index_uniqueness(meta, &visible_table, index)?;
    }

    Ok(())
}

fn filter_table_rows_for_target(
    meta: &LinkedTableMeta<'_>,
    target_tags: &TargetTags,
    table: &DataTable,
) -> DataTable {
    DataTable::new(
        table.name.clone(),
        table
            .rows
            .iter()
            .filter(|row| {
                meta.meta
                    .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
            })
            .cloned()
            .collect(),
    )
}

pub fn normalize_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<DataTable> {
    apply_field_defaults(meta, table)
}

pub fn apply_field_defaults(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<DataTable> {
    let mut table = table.clone();
    for row in &mut table.rows {
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
            .map(|field| row.get(&field.meta.name).cloned().unwrap_or_default())
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

fn validate_supported_field_type(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    if let LinkedField::Struct(_) = &field.linked_field {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` field `{}` uses struct type `{}`, but struct value validation is not implemented yet",
            meta.meta.name, field.meta.name, field.meta.field_type
        )));
    }

    Ok(())
}

fn validate_value_type(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
    value: &str,
    row_index: usize,
) -> Result<()> {
    let valid = match &field.linked_field {
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
        LinkedField::Struct(_) => {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` uses struct type `{}`, but struct value validation is not implemented yet",
                meta.meta.name,
                row_index + 1,
                field.meta.name,
                field.meta.field_type
            )));
        }
    };

    if !valid {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
            meta.meta.name,
            row_index + 1,
            field.meta.name,
            value,
            field.meta.field_type
        )));
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
        if !seen.insert(value.to_owned()) {
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

#[cfg(test)]
mod tests;
