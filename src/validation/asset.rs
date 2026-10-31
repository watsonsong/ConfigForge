// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError,
    asset::AssetResolver,
    data::{DataResolver, DataRow, DataTable, ResolvedRow},
    diagnostic::{DiagnosticLocation, code},
    meta::{
        AssetsConfig, LinkedField, LinkedFieldMeta, LinkedMetaModel, LinkedStructType, TargetTags,
    },
};

use super::{
    ExportTableView, ValidationIssue, ValidationReport,
    diagnostic::error_issues_from_result,
    fragment_field_key,
    key_policy::{resolved_path_value, scalar_export_value},
};

pub(super) fn validate_assets_for_target(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    export_view: &super::ExportValidationView<'_, '_>,
) -> ValidationReport {
    let mut report = ValidationReport::new();
    let resolver = match AssetResolver::new(meta, assets) {
        Ok(resolver) => resolver,
        Err(error) => {
            report.extend(asset_issues_from_error(
                error,
                DiagnosticLocation::default(),
                target_tags,
                None,
                Some("valid asset resolver configuration".to_owned()),
            ));
            return report;
        }
    };

    for table_view in &export_view.tables {
        validate_table_assets(table_view, &resolver, target_tags, &mut report);
    }
    report.sort_by_stable_key();
    report
}

pub(super) fn validate_assets_full(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    data_set: &crate::data::DataSet,
) -> ValidationReport {
    let target_tags = TargetTags::empty();
    let mut report = ValidationReport::new();
    let resolver = match AssetResolver::new(meta, assets) {
        Ok(resolver) => resolver,
        Err(error) => {
            report.extend(asset_issues_from_error(
                error,
                DiagnosticLocation::default(),
                &target_tags,
                None,
                Some("valid asset resolver configuration".to_owned()),
            ));
            return report;
        }
    };

    for table_meta in &meta.tables {
        let Some(table) = data_set.table(&table_meta.meta.name) else {
            continue;
        };
        let resolved_table = DataResolver::resolve_table(table_meta, table, None).ok();
        for (row_index, row) in table.rows.iter().enumerate() {
            let row_number = row_number(table, row_index);
            let resolved_row = resolved_table
                .as_ref()
                .and_then(|resolved_table| resolved_table.rows.get(row_index));
            for field in &table_meta.fields {
                validate_asset_field_path(
                    &resolver,
                    &target_tags,
                    table_meta.meta.name.as_str(),
                    row,
                    resolved_row,
                    row_number,
                    field.meta.name.as_str(),
                    field,
                    &mut report,
                );
            }
            let active_fragments = table
                .row_source(row_index)
                .map(|source| source.fragments.as_slice())
                .unwrap_or(&[]);
            for fragment_name in active_fragments {
                let Some(fragment) = table_meta.fragment(fragment_name) else {
                    continue;
                };
                if let Some(struct_type) = fragment.struct_type() {
                    validate_struct_asset_fields(
                        &resolver,
                        &target_tags,
                        table_meta.meta.name.as_str(),
                        row,
                        resolved_row,
                        row_number,
                        fragment.meta.name.as_str(),
                        struct_type,
                        &mut report,
                    );
                } else {
                    for field in &fragment.fields {
                        let field_path = fragment_field_key(
                            fragment.meta.name.as_str(),
                            field.meta.name.as_str(),
                        );
                        validate_asset_field_path(
                            &resolver,
                            &target_tags,
                            table_meta.meta.name.as_str(),
                            row,
                            resolved_row,
                            row_number,
                            &field_path,
                            field,
                            &mut report,
                        );
                    }
                }
            }
        }
    }
    report.sort_by_stable_key();
    report
}

fn validate_table_assets(
    table_view: &ExportTableView<'_, '_>,
    resolver: &AssetResolver,
    target_tags: &TargetTags,
    report: &mut ValidationReport,
) {
    for (visible_row_index, visible_row) in table_view.visible_rows.iter().enumerate() {
        let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
            continue;
        };
        let resolved_row = table_view.visible_resolved_row(visible_row_index);
        let row_number = visible_row
            .source_row_number
            .unwrap_or(visible_row.row_index + 1);

        for field in table_view
            .meta
            .fields
            .iter()
            .filter(|field| table_view.is_field_visible(field.meta.name.as_str()))
        {
            validate_asset_field_path(
                resolver,
                target_tags,
                table_view.meta.meta.name.as_str(),
                row,
                resolved_row,
                row_number,
                field.meta.name.as_str(),
                field,
                report,
            );
        }

        let active_fragments = table_view
            .table
            .row_source(visible_row.row_index)
            .map(|source| source.fragments.as_slice())
            .unwrap_or(&[]);
        for fragment_name in active_fragments {
            let Some(fragment) = table_view.meta.fragment(fragment_name) else {
                continue;
            };
            if !fragment.meta.is_exported_for(target_tags) {
                continue;
            }
            if let Some(struct_type) = fragment.struct_type() {
                validate_struct_asset_fields(
                    resolver,
                    target_tags,
                    table_view.meta.meta.name.as_str(),
                    row,
                    resolved_row,
                    row_number,
                    fragment.meta.name.as_str(),
                    struct_type,
                    report,
                );
            } else {
                for field in &fragment.fields {
                    if !field.meta.is_exported_for(target_tags) {
                        continue;
                    }
                    let field_path =
                        fragment_field_key(fragment.meta.name.as_str(), field.meta.name.as_str());
                    validate_asset_field_path(
                        resolver,
                        target_tags,
                        table_view.meta.meta.name.as_str(),
                        row,
                        resolved_row,
                        row_number,
                        &field_path,
                        field,
                        report,
                    );
                }
            }
        }
    }
}

fn row_number(table: &DataTable, row_index: usize) -> usize {
    table
        .row_source(row_index)
        .map(|source| source.source_row_number)
        .unwrap_or(row_index + 1)
}

#[allow(clippy::too_many_arguments)]
fn validate_struct_asset_fields(
    resolver: &AssetResolver,
    target_tags: &TargetTags,
    table_name: &str,
    row: &DataRow,
    resolved_row: Option<&ResolvedRow>,
    row_number: usize,
    parent_path: &str,
    struct_type: &LinkedStructType<'_>,
    report: &mut ValidationReport,
) {
    for field in &struct_type.fields {
        if !field.meta.is_exported_for(target_tags) {
            continue;
        }
        let field_path = format!("{parent_path}.{}", field.meta.name);
        match &field.linked_field {
            LinkedField::Struct(child_struct) => validate_struct_asset_fields(
                resolver,
                target_tags,
                table_name,
                row,
                resolved_row,
                row_number,
                &field_path,
                child_struct,
                report,
            ),
            _ => validate_asset_field_path(
                resolver,
                target_tags,
                table_name,
                row,
                resolved_row,
                row_number,
                &field_path,
                field,
                report,
            ),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_asset_field_path(
    resolver: &AssetResolver,
    target_tags: &TargetTags,
    table_name: &str,
    row: &DataRow,
    resolved_row: Option<&ResolvedRow>,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    report: &mut ValidationReport,
) {
    if !matches!(
        field.linked_field,
        LinkedField::Primitive(crate::meta::PrimitiveFieldType::Asset)
    ) {
        return;
    }
    let Some(asset_kind) = field.meta.asset_kind.as_deref() else {
        return;
    };
    let value = scalar_export_value(
        resolved_path_value(resolved_row, field_path),
        raw_cell(row, field_path),
    );
    if value.trim().is_empty() {
        return;
    }
    if let Err(error) = resolver.resolve_for_target(asset_kind, &value, target_tags) {
        report.extend(asset_issues_from_error(
            error,
            DiagnosticLocation {
                table: Some(table_name.to_owned()),
                field: Some(field_path.to_owned()),
                source_row_number: Some(row_number),
                ..DiagnosticLocation::default()
            },
            target_tags,
            Some(value),
            Some(format!("valid `{asset_kind}` asset reference")),
        ));
    }
}

fn raw_cell<'a>(row: &'a DataRow, field_path: &str) -> Option<&'a str> {
    row.get(field_path).map(String::as_str)
}

fn asset_issues_from_error(
    error: ConfigForgeError,
    location: DiagnosticLocation,
    target_tags: &TargetTags,
    current_value: Option<String>,
    expected: Option<String>,
) -> Vec<ValidationIssue> {
    let mut issues = error_issues_from_result(
        code::CF_DATA_INVALID_ASSET_REF,
        "asset",
        Err(error),
        location,
    );
    if issues.is_empty() {
        issues.push(ValidationIssue::error(
            code::CF_DATA_INVALID_ASSET_REF,
            "asset",
            "asset reference validation failed",
            DiagnosticLocation::default(),
        ));
    }
    let target = target_tags.iter().collect::<Vec<_>>().join(",");
    issues
        .into_iter()
        .map(|mut issue| {
            if !target.is_empty() {
                issue = issue.with_target(target.clone());
            }
            if issue.current_value.is_none()
                && let Some(current_value) = &current_value
            {
                issue = issue.with_current_value(current_value.clone());
            }
            if issue.expected.is_none()
                && let Some(expected) = &expected
            {
                issue = issue.with_expected(expected.clone());
            }
            if issue.domain.is_none() {
                issue = issue.with_domain("data");
            }
            if issue.hint.is_none() {
                issue = issue.with_hint(
                    "Check the asset kind configuration, search paths, and referenced asset value.",
                );
            }
            issue
        })
        .collect()
}
