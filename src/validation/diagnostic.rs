// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    diagnostic::{
        Diagnostic, DiagnosticEntry, DiagnosticLocation, DiagnosticReport, DiagnosticSeverity,
    },
    meta::ValidationSeverity,
};

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub related_locations: Vec<DiagnosticLocation>,
    pub record_key: Option<String>,
    pub context: std::collections::BTreeMap<String, String>,
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub location: ValidationIssueLocation,
    pub validator: String,
    pub target: Option<String>,
    pub hint: Option<String>,
    pub current_value: Option<String>,
    pub expected: Option<String>,
    pub explanation: Option<String>,
    pub domain: Option<String>,
    pub owner: Option<String>,
}

pub type ValidationIssueLocation = DiagnosticLocation;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationOutcome {
    pub report: ValidationReport,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self { issues: Vec::new() }
    }

    pub fn push(&mut self, issue: ValidationIssue) {
        self.issues.push(issue);
    }

    pub fn extend(&mut self, issues: impl IntoIterator<Item = ValidationIssue>) {
        self.issues.extend(issues);
    }

    pub fn append(&mut self, other: &mut ValidationReport) {
        self.issues.append(&mut other.issues);
    }

    pub fn sort_by_stable_key(&mut self) {
        self.issues.sort_by(compare_issues_by_stable_key);
    }

    pub fn has_error(&self) -> bool {
        self.as_diagnostic_report().has_error()
    }

    pub fn warning_messages(&self) -> Vec<String> {
        self.as_diagnostic_report().warning_messages()
    }

    pub fn error_messages(&self) -> Vec<String> {
        self.as_diagnostic_report().error_messages()
    }

    pub fn into_outcome(self) -> ValidationOutcome {
        ValidationOutcome { report: self }
    }

    pub fn into_result(self) -> Result<()> {
        self.into_outcome().into_unit_result()
    }

    pub fn into_diagnostic_report(self) -> DiagnosticReport<Diagnostic> {
        DiagnosticReport {
            diagnostics: self
                .issues
                .into_iter()
                .map(|issue| Diagnostic::from_entry(&issue))
                .collect(),
        }
    }

    fn as_diagnostic_report(&self) -> DiagnosticReport<&ValidationIssue> {
        DiagnosticReport {
            diagnostics: self.issues.iter().collect(),
        }
    }
}

impl ValidationOutcome {
    pub fn is_blocking(&self) -> bool {
        self.report.has_error()
    }

    pub fn report(&self) -> &ValidationReport {
        &self.report
    }

    pub fn into_report(self) -> ValidationReport {
        self.report
    }

    pub fn into_result(self) -> Result<ValidationReport> {
        if !self.is_blocking() {
            return Ok(self.report);
        }

        Err(ConfigForgeError::ValidationReport(
            self.report.into_diagnostic_report(),
        ))
    }

    pub fn into_unit_result(self) -> Result<()> {
        self.into_result().map(|_| ())
    }
}

fn compare_issues_by_stable_key(left: &ValidationIssue, right: &ValidationIssue) -> Ordering {
    (
        left.severity,
        left.target.as_deref(),
        left.location.table.as_deref(),
        left.location.row_index,
        left.location.source_row_number,
        left.location.field.as_deref(),
        left.validator.as_str(),
        left.code.as_str(),
        left.message.as_str(),
    )
        .cmp(&(
            right.severity,
            right.target.as_deref(),
            right.location.table.as_deref(),
            right.location.row_index,
            right.location.source_row_number,
            right.location.field.as_deref(),
            right.validator.as_str(),
            right.code.as_str(),
            right.message.as_str(),
        ))
}

impl ValidationIssue {
    pub fn new(
        code: impl Into<String>,
        severity: ValidationSeverity,
        validator: impl Into<String>,
        message: impl Into<String>,
        location: ValidationIssueLocation,
    ) -> Self {
        Self {
            related_locations: Vec::new(),
            record_key: None,
            context: Default::default(),
            code: code.into(),
            severity,
            message: message.into(),
            location,
            validator: validator.into(),
            target: None,
            hint: None,
            current_value: None,
            expected: None,
            explanation: None,
            domain: None,
            owner: None,
        }
    }

    pub fn error(
        code: impl Into<String>,
        validator: impl Into<String>,
        message: impl Into<String>,
        location: ValidationIssueLocation,
    ) -> Self {
        Self::new(
            code,
            ValidationSeverity::Error,
            validator,
            message,
            location,
        )
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_current_value(mut self, current_value: impl Into<String>) -> Self {
        self.current_value = Some(current_value.into());
        self
    }

    pub fn with_expected(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }

    pub fn with_explanation(mut self, explanation: impl Into<String>) -> Self {
        self.explanation = Some(explanation.into());
        self
    }

    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }
}

pub(super) fn error_issue_from_message(
    code: impl Into<String>,
    validator: impl Into<String>,
    message: impl Into<String>,
    location: ValidationIssueLocation,
) -> ValidationIssue {
    ValidationIssue::error(code, validator, message, location)
}

pub(super) fn error_issues_from_result(
    code: impl Into<String>,
    validator: impl Into<String>,
    result: Result<()>,
    location: ValidationIssueLocation,
) -> Vec<ValidationIssue> {
    let code = code.into();
    let validator = validator.into();
    let Err(error) = result else {
        return Vec::new();
    };
    if let ConfigForgeError::ValidationReport(report) = error {
        return report
            .diagnostics
            .into_iter()
            .map(|diagnostic| validation_issue_from_diagnostic(diagnostic, &validator, &location))
            .collect();
    }
    vec![error_issue_from_message(
        code,
        validator,
        validation_error_message(error),
        location,
    )]
}

fn validation_issue_from_diagnostic(
    diagnostic: Diagnostic,
    fallback_validator: &str,
    fallback_location: &ValidationIssueLocation,
) -> ValidationIssue {
    let primary = diagnostic.location;
    let location = DiagnosticLocation {
        source_path: primary
            .source_path
            .or_else(|| fallback_location.source_path.clone()),
        source_sheet: primary
            .source_sheet
            .or_else(|| fallback_location.source_sheet.clone()),
        table: primary.table.or_else(|| fallback_location.table.clone()),
        field: primary.field.or_else(|| fallback_location.field.clone()),
        row_index: primary.row_index.or(fallback_location.row_index),
        source_row_number: primary
            .source_row_number
            .or(fallback_location.source_row_number),
        source_column_number: primary
            .source_column_number
            .or(fallback_location.source_column_number),
        source_column_name: primary
            .source_column_name
            .or_else(|| fallback_location.source_column_name.clone()),
        column_index: primary.column_index.or(fallback_location.column_index),
    };
    let mut issue = ValidationIssue::new(
        diagnostic.code,
        diagnostic.severity,
        diagnostic
            .source
            .unwrap_or_else(|| fallback_validator.to_owned()),
        diagnostic.message,
        location,
    );
    issue.target = diagnostic.target;
    issue.hint = diagnostic.hint;
    issue.current_value = diagnostic.current_value;
    issue.expected = diagnostic.expected;
    issue.explanation = diagnostic.explanation;
    issue.domain = diagnostic.domain;
    issue.owner = diagnostic.owner;
    issue.related_locations = diagnostic.related_locations;
    issue.record_key = diagnostic.record_key;
    issue.context = diagnostic.context;
    issue
}

pub(super) fn validation_error_message(error: ConfigForgeError) -> String {
    match error {
        ConfigForgeError::Validation(message) => message,
        ConfigForgeError::ValidationReport(report) => report.error_messages().join("; "),
        other => other.to_string(),
    }
}

impl DiagnosticEntry for ValidationIssue {
    fn related_locations(&self) -> &[DiagnosticLocation] {
        &self.related_locations
    }
    fn record_key(&self) -> Option<&str> {
        self.record_key.as_deref()
    }
    fn context(&self) -> Option<&std::collections::BTreeMap<String, String>> {
        Some(&self.context)
    }
    fn code(&self) -> String {
        self.code.clone()
    }

    fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    fn message(&self) -> &str {
        &self.message
    }

    fn location(&self) -> &DiagnosticLocation {
        &self.location
    }

    fn source(&self) -> Option<&str> {
        Some(&self.validator)
    }

    fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    fn current_value(&self) -> Option<&str> {
        self.current_value.as_deref()
    }

    fn expected(&self) -> Option<&str> {
        self.expected.as_deref()
    }

    fn explanation(&self) -> Option<&str> {
        self.explanation.as_deref()
    }

    fn domain(&self) -> Option<&str> {
        self.domain.as_deref()
    }

    fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_diagnostic_keeps_primary_coordinates_and_fills_logical_context() {
        let diagnostic = Diagnostic::new(
            "custom.code",
            DiagnosticSeverity::Warning,
            "invalid value",
            DiagnosticLocation {
                source_row_number: Some(42),
                ..Default::default()
            },
        );
        let fallback = DiagnosticLocation {
            table: Some("items".to_owned()),
            field: Some("weight".to_owned()),
            row_index: Some(7),
            source_row_number: Some(9),
            ..Default::default()
        };
        let issue = validation_issue_from_diagnostic(diagnostic, "custom", &fallback);
        assert_eq!(issue.location.source_row_number, Some(42));
        assert_eq!(issue.location.row_index, Some(7));
        assert_eq!(issue.location.table.as_deref(), Some("items"));
        assert_eq!(issue.location.field.as_deref(), Some("weight"));
        assert_eq!(issue.severity, DiagnosticSeverity::Warning);
    }
}
