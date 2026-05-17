use crate::{meta::ValidationSeverity, ConfigForgeError, Result};

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

    pub fn into_result(self) -> Result<()> {
        if !self.has_error() {
            return Ok(());
        }

        let messages = self
            .issues
            .into_iter()
            .filter(|issue| issue.severity == ValidationSeverity::Error)
            .map(|issue| issue.message)
            .collect::<Vec<_>>()
            .join("; ");
        Err(ConfigForgeError::Validation(messages))
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
