// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    data::{DataSet, DataTable},
    diagnostic::DiagnosticLocation,
    meta::{AssetsConfig, LinkedMetaModel, LinkedTableMeta, TargetTags},
};

use rayon::prelude::*;

use super::{
    context::{ExportValidationView, ValidationContext},
    defaults::normalize_data_set,
    diagnostic::{ValidationOutcome, ValidationReport, error_issue_from_message},
    registry::{CompiledValidationPlan, ValidatorRegistry},
    table::{validate_normalized_table_view_with_report, validate_normalized_table_with_report},
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
    validate_full_data_set(
        meta,
        data_set,
        None,
        None,
        validate_normalized_table_with_report,
    )
}

pub fn validate_normalized_data_set_with_assets_and_report(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    data_set: &DataSet,
) -> Result<ValidationReport> {
    validate_full_data_set(
        meta,
        data_set,
        Some(assets),
        None,
        validate_normalized_table_with_report,
    )
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
    validate_full_data_set(meta, data_set, None, Some(pool), table_validator)
}

fn collect_table_reports<T: Sync>(
    tables: &[T],
    pool: Option<&rayon::ThreadPool>,
    validate: impl Fn(&T) -> ValidationReport + Sync,
) -> ValidationReport {
    let mut report = ValidationReport::new();
    if let Some(pool) = pool {
        let reports = pool.install(|| tables.par_iter().map(&validate).collect::<Vec<_>>());
        for mut table_report in reports {
            report.append(&mut table_report);
        }
    } else {
        for table in tables {
            report.append(&mut validate(table));
        }
    }
    report
}

fn validate_full_data_set(
    meta: &LinkedMetaModel<'_>,
    data_set: &DataSet,
    assets: Option<&AssetsConfig>,
    pool: Option<&rayon::ThreadPool>,
    table_validator: impl Fn(&LinkedTableMeta<'_>, &DataTable) -> ValidationReport + Sync,
) -> Result<ValidationReport> {
    let mut report = collect_table_reports(&meta.tables, pool, |table_meta| {
        match data_set.table(&table_meta.meta.name) {
            Some(table) => table_validator(table_meta, table),
            None => {
                let mut report = ValidationReport::new();
                report.push(missing_table_issue(table_meta));
                report
            }
        }
    });
    report.extend(super::diagnostic::error_issues_from_result(
        "foreign_key.integrity",
        "foreign_key",
        super::foreign_key::validate_foreign_keys_full(meta, data_set),
        DiagnosticLocation::default(),
    ));
    if let Some(assets) = assets {
        report.append(&mut super::asset::validate_assets_full(
            meta, assets, data_set,
        ));
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
    enrich_source_locations(&mut report, data_set);
    enrich_record_keys(&mut report, meta, data_set);
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
    validate_target_data_set(meta, target_tags, data_set, None, None, |view| {
        validate_normalized_table_view_with_report(view, Some(target_tags))
    })
    .and_then(ValidationOutcome::into_result)
}

pub fn validate_normalized_data_set_for_target_with_assets_and_report(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<ValidationReport> {
    validate_target_data_set(meta, target_tags, data_set, Some(assets), None, |view| {
        validate_normalized_table_view_with_report(view, Some(target_tags))
    })
    .and_then(ValidationOutcome::into_result)
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
    validate_normalized_data_set_for_target_with_assets_outcome_and_pool(
        meta,
        assets,
        target_tags,
        data_set,
        pool,
    )
    .and_then(ValidationOutcome::into_result)
}

pub fn validate_normalized_data_set_for_target_with_assets_outcome_and_pool(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
) -> Result<ValidationOutcome> {
    validate_normalized_data_set_for_target_with_table_view_validator_assets_outcome_and_pool(
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
    validate_target_data_set(
        meta,
        target_tags,
        data_set,
        None,
        Some(pool),
        table_view_validator,
    )
    .and_then(ValidationOutcome::into_result)
}

pub(crate) fn validate_normalized_data_set_for_target_with_table_view_validator_assets_outcome_and_pool(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
    table_view_validator: impl Fn(&super::ExportTableView<'_, '_>) -> ValidationReport + Sync,
) -> Result<ValidationOutcome> {
    validate_target_data_set(
        meta,
        target_tags,
        data_set,
        Some(assets),
        Some(pool),
        table_view_validator,
    )
}

fn validate_target_data_set(
    meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
    assets: Option<&AssetsConfig>,
    pool: Option<&rayon::ThreadPool>,
    table_view_validator: impl Fn(&super::ExportTableView<'_, '_>) -> ValidationReport + Sync,
) -> Result<ValidationOutcome> {
    let export_view = match pool {
        Some(pool) => {
            ExportValidationView::for_target_with_pool(meta, data_set, target_tags, pool)?
        }
        None => ExportValidationView::for_target(meta, data_set, target_tags)?,
    };
    let report = collect_table_reports(&export_view.tables, pool, table_view_validator);
    finish_target_data_set_validation_outcome(
        meta,
        target_tags,
        data_set,
        export_view,
        assets,
        report,
    )
}

fn finish_target_data_set_validation_outcome(
    meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
    export_view: ExportValidationView<'_, '_>,
    assets: Option<&AssetsConfig>,
    mut report: ValidationReport,
) -> Result<ValidationOutcome> {
    report.extend(
        super::diagnostic::error_issues_from_result(
            "foreign_key.integrity",
            "foreign_key",
            super::foreign_key::validate_foreign_keys_for_target(meta, &export_view, target_tags),
            DiagnosticLocation::default(),
        )
        .into_iter()
        .map(|issue| issue.with_target(target_tags.iter().collect::<Vec<_>>().join(","))),
    );

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
    enrich_source_locations(&mut report, data_set);
    enrich_record_keys(&mut report, meta, data_set);
    report.sort_by_stable_key();
    Ok(report.into_outcome())
}

fn report_into_result(report: ValidationReport) -> Result<ValidationReport> {
    report.into_outcome().into_result()
}

#[cfg(test)]
pub(crate) fn validate_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()> {
    let table = super::defaults::normalize_table(meta, table)?;
    super::table::validate_normalized_table(meta, &table)
}

fn enrich_source_locations(report: &mut ValidationReport, data_set: &DataSet) {
    for issue in &mut report.issues {
        let location = &mut issue.location;
        let Some(table) = location
            .table
            .as_deref()
            .and_then(|name| data_set.table(name))
        else {
            continue;
        };
        let row_index = location.row_index.or_else(|| {
            location.source_row_number.and_then(|number| {
                let mut matches = table
                    .row_sources
                    .iter()
                    .enumerate()
                    .filter(|(_, source)| {
                        source.source_row_number == number
                            && location
                                .source_path
                                .as_ref()
                                .is_none_or(|path| path == &source.source_path)
                            && location
                                .source_sheet
                                .as_ref()
                                .is_none_or(|sheet| Some(sheet) == source.source_sheet.as_ref())
                    })
                    .map(|(index, _)| index);
                let first = matches.next();
                first.filter(|_| matches.next().is_none())
            })
        });
        let Some(row_index) = row_index else {
            continue;
        };
        table.enrich_location(row_index, location);
    }
}

fn enrich_record_keys(report: &mut ValidationReport, meta: &LinkedMetaModel<'_>, data: &DataSet) {
    for issue in &mut report.issues {
        if issue.code == crate::diagnostic::code::CF_DATA_INVALID_FOREIGN_KEY
            && let Some(table) = &issue.location.table
        {
            let key = if let Some(field) = &issue.location.field {
                format!("tables/{table}/fields/{field}")
            } else {
                format!("tables/{table}")
            };
            if let Some(source) = meta
                .model
                .declaration_sources
                .get(&key)
                .and_then(|sources| sources.first())
            {
                let mut source = source.clone();
                source.table = issue.location.table.clone();
                source.field = issue.location.field.clone();
                issue.related_locations.push(source);
            }
        }
        for related in &mut issue.related_locations {
            if related.source_path.is_none()
                && let Some(name) = related.table.as_deref()
                && let Some(source) = meta
                    .model
                    .declaration_sources
                    .get(&format!("tables/{name}"))
                    .and_then(|sources| sources.first())
            {
                related.source_path = source.source_path.clone();
                related.source_row_number = source.source_row_number;
                related.source_column_number = source.source_column_number;
            }
        }
        let Some(name) = issue.location.table.as_deref() else {
            continue;
        };
        let (Some(table), Some(schema)) = (data.table(name), meta.table(name)) else {
            continue;
        };
        if let Some(index) = issue.location.row_index {
            issue.record_key = super::key_policy::raw_record_key(schema, table.rows.get(index));
        }
    }
}
