// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod builtins;
mod context;
mod defaults;
mod diagnostic;
mod entry;
mod foreign_key;
mod registry;
mod struct_value;
mod table;

pub use context::{
    ExportTableView, ExportValidationView, ValidationContext, VisibleRow, visible_row_for,
};
pub use defaults::{
    apply_field_defaults, normalize_data_set, normalize_data_set_with_pool, normalize_table,
};
pub use diagnostic::{ValidationIssue, ValidationIssueLocation, ValidationReport};
pub use entry::{
    validate_data_set, validate_data_set_for_target, validate_data_set_for_target_with_report,
    validate_data_set_with_report, validate_normalized_data_set,
    validate_normalized_data_set_for_target, validate_normalized_data_set_for_target_with_report,
    validate_table, validate_table_for_target,
};
pub use registry::{
    CompiledValidationPlan, CompiledValidator, ValidatorFactory, ValidatorRegistry,
};
pub use table::{validate_normalized_table, validate_normalized_table_for_target};

#[cfg(test)]
use crate::data::{DataSet, DataTable};

pub(super) fn fragment_field_key(fragment_name: &str, field_name: &str) -> String {
    format!("{fragment_name}.{field_name}")
}

#[cfg(test)]
mod tests;
