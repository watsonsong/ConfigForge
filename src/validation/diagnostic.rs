use crate::{ConfigForgeError, Result, meta::ValidationSeverity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub location: ValidationIssueLocation,
    pub validator: String,
    pub target: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationIssueLocation {
    pub table: Option<String>,
    pub field: Option<String>,
    pub row_index: Option<usize>,
    pub source_row_number: Option<usize>,
}

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

    pub fn has_error(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error)
    }

    pub fn warning_messages(&self) -> Vec<String> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == ValidationSeverity::Warning)
            .map(format_issue)
            .collect()
    }

    pub fn error_messages(&self) -> Vec<String> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == ValidationSeverity::Error)
            .map(|issue| issue.message.clone())
            .collect()
    }

    pub fn into_result(self) -> Result<()> {
        if !self.has_error() {
            return Ok(());
        }

        let messages = self.error_messages().join("; ");
        Err(ConfigForgeError::Validation(messages))
    }
}

fn format_issue(issue: &ValidationIssue) -> String {
    let mut parts = Vec::new();
    if let Some(table) = &issue.location.table {
        parts.push(format!("table `{table}`"));
    }
    if let Some(row_number) = issue.location.source_row_number {
        parts.push(format!("row {row_number}"));
    }
    if let Some(field) = &issue.location.field {
        parts.push(format!("field `{field}`"));
    }
    if let Some(target) = &issue.target {
        parts.push(format!("target `{target}`"));
    }

    if parts.is_empty() {
        format!("{}: {}", issue.validator, issue.message)
    } else {
        format!(
            "{}: {} ({})",
            issue.validator,
            issue.message,
            parts.join(", ")
        )
    }
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
}
