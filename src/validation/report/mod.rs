// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod model;
mod render;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use model::{
    DesignerReportFormat, DesignerReportOptions, DesignerValidationIssue, DesignerValidationReport,
    IssueGroup, ReportMetadata, ReportSummary, ValidationReportExportConfig,
};
pub use render::render_designer_validation_report;
