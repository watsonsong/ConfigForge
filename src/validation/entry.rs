// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    data::{DataSet, DataTable},
    diagnostic::DiagnosticLocation,
    meta::{LinkedMetaModel, LinkedTableMeta, TargetTags},
    project::AssetsConfig,
};

use rayon::prelude::*;

use super::{
    context::{ExportValidationView, ValidationContext},
    defaults::{normalize_data_set, normalize_table},
    diagnostic::{ValidationReport, error_issue_from_message},
    registry::{CompiledValidationPlan, ValidatorRegistry},
    table::{
        validate_normalized_table, validate_normalized_table_for_target,
        validate_normalized_table_view_with_report, validate_normalized_table_with_report,
    },
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
    let mut report = ValidationReport::new();
    for table_meta in &meta.tables {
        let Some(table) = data_set.table(&table_meta.meta.name) else {
            report.push(error_issue_from_message(
                "table.missing",
                "required",
                format!("data set is missing table `{}`", table_meta.meta.name),
                DiagnosticLocation {
                    table: Some(table_meta.meta.name.clone()),
                    ..DiagnosticLocation::default()
                },
            ));
            continue;
        };
        report.append(&mut validate_normalized_table_with_report(
            table_meta, table,
        ));
    }

    finish_full_data_set_validation(meta, data_set, report)
}

pub fn validate_normalized_data_set_with_assets_and_report(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    data_set: &DataSet,
) -> Result<ValidationReport> {
    let mut report = ValidationReport::new();
    for table_meta in &meta.tables {
        let Some(table) = data_set.table(&table_meta.meta.name) else {
            report.push(error_issue_from_message(
                "table.missing",
                "required",
                format!("data set is missing table `{}`", table_meta.meta.name),
                DiagnosticLocation {
                    table: Some(table_meta.meta.name.clone()),
                    ..DiagnosticLocation::default()
                },
            ));
            continue;
        };
        report.append(&mut validate_normalized_table_with_report(
            table_meta, table,
        ));
    }

    finish_full_data_set_validation_with_assets(meta, assets, data_set, report)
}

pub fn validate_normalized_data_set_with_report_and_pool(
    meta: &LinkedMetaModel<'_>,
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
) -> Result<ValidationReport> {
    validate_normalized_data_set_with_table_validator_and_pool(
        meta,
        data_set,
        pool,
        validate_normalized_table_with_report,
    )
}

pub(crate) fn validate_normalized_data_set_with_table_validator_and_pool(
    meta: &LinkedMetaModel<'_>,
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
    table_validator: impl Fn(&LinkedTableMeta<'_>, &DataTable) -> ValidationReport + Sync,
) -> Result<ValidationReport> {
    let mut table_reports = pool.install(|| {
        meta.tables
            .par_iter()
            .enumerate()
            .map(|(index, table_meta)| {
                let mut report = ValidationReport::new();
                let Some(table) = data_set.table(&table_meta.meta.name) else {
                    report.push(missing_table_issue(table_meta));
                    return (index, report);
                };
                report.append(&mut table_validator(table_meta, table));
                (index, report)
            })
            .collect::<Vec<_>>()
    });
    table_reports.sort_by_key(|(index, _)| *index);

    let mut report = ValidationReport::new();
    for (_, mut table_report) in table_reports {
        report.append(&mut table_report);
    }

    finish_full_data_set_validation(meta, data_set, report)
}

fn finish_full_data_set_validation(
    meta: &LinkedMetaModel<'_>,
    data_set: &DataSet,
    mut report: ValidationReport,
) -> Result<ValidationReport> {
    if let Some(issue) = super::diagnostic::error_issue_from_result(
        "foreign_key.integrity",
        "foreign_key",
        super::foreign_key::validate_foreign_keys_full(meta, data_set),
        DiagnosticLocation::default(),
    ) {
        report.push(issue);
    }

    let registry = ValidatorRegistry::with_builtin_validators();
    let plan = CompiledValidationPlan::compile(meta, &registry)?;
    let context = ValidationContext::new(
        meta,
        None,
        data_set,
        ExportValidationView::full(meta, data_set),
    );
    report.append(&mut plan.run(&context));
    report.sort_by_stable_key();
    report_into_result(report)
}

fn finish_full_data_set_validation_with_assets(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    data_set: &DataSet,
    mut report: ValidationReport,
) -> Result<ValidationReport> {
    if let Some(issue) = super::diagnostic::error_issue_from_result(
        "foreign_key.integrity",
        "foreign_key",
        super::foreign_key::validate_foreign_keys_full(meta, data_set),
        DiagnosticLocation::default(),
    ) {
        report.push(issue);
    }

    report.append(&mut super::asset::validate_assets_full(
        meta, assets, data_set,
    ));

    let registry = ValidatorRegistry::with_builtin_validators();
    let plan = CompiledValidationPlan::compile(meta, &registry)?;
    let context = ValidationContext::new(
        meta,
        None,
        data_set,
        ExportValidationView::full(meta, data_set),
    );
    report.append(&mut plan.run(&context));
    report.sort_by_stable_key();
    report_into_result(report)
}

fn missing_table_issue(table_meta: &LinkedTableMeta<'_>) -> super::diagnostic::ValidationIssue {
    error_issue_from_message(
        "table.missing",
        "required",
        format!("data set is missing table `{}`", table_meta.meta.name),
        DiagnosticLocation {
            table: Some(table_meta.meta.name.clone()),
            ..DiagnosticLocation::default()
        },
    )
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
    let mut report = ValidationReport::new();
    for table_view in &export_view.tables {
        report.append(&mut validate_normalized_table_view_with_report(
            table_view,
            Some(target_tags),
        ));
    }

    finish_target_data_set_validation(meta, target_tags, data_set, export_view, None, report)
}

pub fn validate_normalized_data_set_for_target_with_assets_and_report(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<ValidationReport> {
    let export_view = ExportValidationView::for_target(meta, data_set, target_tags)?;
    let mut report = ValidationReport::new();
    for table_view in &export_view.tables {
        report.append(&mut validate_normalized_table_view_with_report(
            table_view,
            Some(target_tags),
        ));
    }

    finish_target_data_set_validation(
        meta,
        target_tags,
        data_set,
        export_view,
        Some(assets),
        report,
    )
}

pub fn validate_normalized_data_set_for_target_with_report_and_pool(
    meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
) -> Result<ValidationReport> {
    validate_normalized_data_set_for_target_with_table_view_validator_and_pool(
        meta,
        target_tags,
        data_set,
        pool,
        |table_view| validate_normalized_table_view_with_report(table_view, Some(target_tags)),
    )
}

pub fn validate_normalized_data_set_for_target_with_assets_report_and_pool(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
) -> Result<ValidationReport> {
    validate_normalized_data_set_for_target_with_table_view_validator_assets_and_pool(
        meta,
        assets,
        target_tags,
        data_set,
        pool,
        |table_view| validate_normalized_table_view_with_report(table_view, Some(target_tags)),
    )
}

pub(crate) fn validate_normalized_data_set_for_target_with_table_view_validator_and_pool(
    meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
    table_view_validator: impl Fn(&super::ExportTableView<'_, '_>) -> ValidationReport + Sync,
) -> Result<ValidationReport> {
    let export_view =
        ExportValidationView::for_target_with_pool(meta, data_set, target_tags, pool)?;
    let mut table_reports = pool.install(|| {
        export_view
            .tables
            .par_iter()
            .enumerate()
            .map(|(index, table_view)| (index, table_view_validator(table_view)))
            .collect::<Vec<_>>()
    });
    table_reports.sort_by_key(|(index, _)| *index);

    let mut report = ValidationReport::new();
    for (_, mut table_report) in table_reports {
        report.append(&mut table_report);
    }

    finish_target_data_set_validation(meta, target_tags, data_set, export_view, None, report)
}

pub(crate) fn validate_normalized_data_set_for_target_with_table_view_validator_assets_and_pool(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
    table_view_validator: impl Fn(&super::ExportTableView<'_, '_>) -> ValidationReport + Sync,
) -> Result<ValidationReport> {
    let export_view =
        ExportValidationView::for_target_with_pool(meta, data_set, target_tags, pool)?;
    let mut table_reports = pool.install(|| {
        export_view
            .tables
            .par_iter()
            .enumerate()
            .map(|(index, table_view)| (index, table_view_validator(table_view)))
            .collect::<Vec<_>>()
    });
    table_reports.sort_by_key(|(index, _)| *index);

    let mut report = ValidationReport::new();
    for (_, mut table_report) in table_reports {
        report.append(&mut table_report);
    }

    finish_target_data_set_validation(
        meta,
        target_tags,
        data_set,
        export_view,
        Some(assets),
        report,
    )
}

fn finish_target_data_set_validation(
    meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
    export_view: ExportValidationView<'_, '_>,
    assets: Option<&AssetsConfig>,
    mut report: ValidationReport,
) -> Result<ValidationReport> {
    if let Some(issue) = super::diagnostic::error_issue_from_result(
        "foreign_key.integrity",
        "foreign_key",
        super::foreign_key::validate_foreign_keys_for_target(meta, &export_view, target_tags),
        DiagnosticLocation::default(),
    ) {
        report.push(issue.with_target(target_tags.iter().collect::<Vec<_>>().join(",")));
    }

    if let Some(assets) = assets {
        report.append(&mut super::asset::validate_assets_for_target(
            meta,
            assets,
            target_tags,
            &export_view,
        ));
    }

    let registry = ValidatorRegistry::with_builtin_validators();
    let plan = CompiledValidationPlan::compile(meta, &registry)?;
    let context = ValidationContext::new(meta, Some(target_tags.clone()), data_set, export_view);
    report.append(&mut plan.run(&context));
    report.sort_by_stable_key();
    report_into_result(report)
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
