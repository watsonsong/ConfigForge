// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data::{DataSet, DataTable},
    meta::{LinkedMetaModel, LinkedTableMeta, TargetTags},
    ConfigForgeError, Result,
};

use super::{
    context::{ExportValidationView, ValidationContext},
    defaults::{normalize_data_set, normalize_table},
    diagnostic::ValidationReport,
    registry::{CompiledValidationPlan, ValidatorRegistry},
    table::{validate_normalized_table, validate_normalized_table_for_target},
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

    super::foreign_key::validate_foreign_keys_full(meta, data_set)?;

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
        super::table::validate_normalized_table_view(table_view, Some(target_tags))?;
    }

    super::foreign_key::validate_foreign_keys_for_target(meta, &export_view, target_tags)?;

    let registry = ValidatorRegistry::with_builtin_validators();
    let plan = CompiledValidationPlan::compile(meta, &registry)?;
    let context = ValidationContext::new(meta, Some(target_tags.clone()), data_set, export_view);
    report_into_result(plan.run(&context))
}

fn report_into_result(report: ValidationReport) -> Result<ValidationReport> {
    if report.has_error() {
        return Err(ConfigForgeError::ValidationReport(
            report.clone().into_diagnostic_report(),
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
