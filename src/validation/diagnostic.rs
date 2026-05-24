use crate::{
    ConfigForgeError, Result,
    diagnostic::{DiagnosticEntry, DiagnosticLocation, DiagnosticReport, DiagnosticSeverity},
    meta::ValidationSeverity,
};

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

        let messages = self.error_messages().join("; ");
        Err(ConfigForgeError::Validation(messages))
    }

    fn as_diagnostic_report(&self) -> DiagnosticReport<&ValidationIssue> {
        DiagnosticReport {
            diagnostics: self.issues.iter().collect(),
        }
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
}
