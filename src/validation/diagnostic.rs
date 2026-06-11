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

    pub fn into_result(self) -> Result<()> {
        if !self.has_error() {
            return Ok(());
        }

        Err(ConfigForgeError::ValidationReport(
            self.into_diagnostic_report(),
        ))
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
    let location = if diagnostic.location.is_empty() {
        fallback_location.clone()
    } else {
        diagnostic.location
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
