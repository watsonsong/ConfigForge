// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod asset;
mod builtins;
mod context;
mod defaults;
mod diagnostic;
mod entry;
mod foreign_key;
mod registry;
mod report;
mod struct_value;
mod table;

pub use context::{ExportTableView, ExportValidationView, ValidationContext, VisibleRow};
pub use defaults::{
    apply_field_defaults, normalize_data_set, normalize_data_set_with_pool, normalize_table,
};
#[allow(unused_imports)]
pub use diagnostic::ValidationOutcome;
pub use diagnostic::{ValidationIssue, ValidationIssueLocation, ValidationReport};
#[allow(unused_imports)]
pub use entry::validate_normalized_data_set_for_target_with_assets_outcome_and_pool;
#[cfg(test)]
pub(crate) use entry::validate_table;
pub use entry::{
    validate_data_set, validate_data_set_for_target, validate_data_set_for_target_with_report,
    validate_data_set_with_report, validate_normalized_data_set,
    validate_normalized_data_set_for_target,
    validate_normalized_data_set_for_target_with_assets_and_report,
    validate_normalized_data_set_for_target_with_assets_report_and_pool,
    validate_normalized_data_set_for_target_with_report,
    validate_normalized_data_set_for_target_with_report_and_pool,
    validate_normalized_data_set_with_assets_and_report,
    validate_normalized_data_set_with_report_and_pool,
};
pub use registry::{
    CompiledValidationPlan, CompiledValidator, ValidatorFactory, ValidatorRegistry,
};
pub use report::{
    DesignerReportFormat, DesignerReportOptions, DesignerValidationReport,
    ValidationReportExportConfig, render_designer_validation_report,
};

#[cfg(test)]
use crate::data::{DataSet, DataTable};
use crate::{ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity};

pub(super) fn fragment_field_key(fragment_name: &str, field_name: &str) -> String {
    format!("{fragment_name}.{field_name}")
}

pub(super) fn validation_diagnostic_error(
    code: &'static str,
    source: impl Into<String>,
    message: impl Into<String>,
    location: DiagnosticLocation,
    current: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(code, DiagnosticSeverity::Error, message, location)
            .with_source(source)
            .with_domain("validation")
            .with_current_value(current)
            .with_expected(expected)
            .with_hint(hint),
    )
}

#[cfg(test)]
mod tests;
