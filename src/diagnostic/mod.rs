// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

pub mod code;
mod error;

pub use error::{ConfigForgeError, Result};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    #[default]
    Error,
    Warning,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct DiagnosticLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_sheet: Option<String>,
    pub source_path: Option<PathBuf>,
    pub table: Option<String>,
    pub field: Option<String>,
    pub row_index: Option<usize>,
    pub source_row_number: Option<usize>,
    pub source_column_number: Option<usize>,
    pub source_column_name: Option<String>,
    pub column_index: Option<usize>,
}

impl DiagnosticLocation {
    pub fn is_empty(&self) -> bool {
        self.source_sheet.is_none()
            && self.source_path.is_none()
            && self.table.is_none()
            && self.field.is_none()
            && self.row_index.is_none()
            && self.source_row_number.is_none()
            && self.source_column_number.is_none()
            && self.source_column_name.is_none()
            && self.column_index.is_none()
    }
}

impl fmt::Display for DiagnosticLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(source_path) = &self.source_path {
            parts.push(format!("source `{}`", source_path.display()));
        }
        if let Some(sheet) = &self.source_sheet {
            parts.push(format!("sheet `{sheet}`"));
        }
        if let Some(table) = &self.table {
            parts.push(format!("table `{table}`"));
        }
        if let Some(row_number) = self.source_row_number {
            parts.push(format!("row {row_number}"));
        } else if let Some(row_index) = self.row_index {
            parts.push(format!("row {}", row_index + 1));
        }
        if let Some(column_name) = &self.source_column_name {
            parts.push(format!("column `{column_name}`"));
        } else if let Some(column_number) = self.source_column_number {
            parts.push(format!("column {column_number}"));
        } else if let Some(column_index) = self.column_index {
            parts.push(format!("column {}", column_index + 1));
        }
        if let Some(field) = &self.field {
            parts.push(format!("field `{field}`"));
        }
        write!(formatter, "{}", parts.join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub related_locations: Vec<DiagnosticLocation>,
    pub record_key: Option<String>,
    pub context: BTreeMap<String, String>,
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub location: DiagnosticLocation,
    pub source: Option<String>,
    pub target: Option<String>,
    pub hint: Option<String>,
    pub current_value: Option<String>,
    pub expected: Option<String>,
    pub explanation: Option<String>,
    pub domain: Option<String>,
    pub owner: Option<String>,
}

impl Diagnostic {
    pub fn new(
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
        location: DiagnosticLocation,
    ) -> Self {
        Self {
            related_locations: Vec::new(),
            record_key: None,
            context: BTreeMap::new(),
            code: code.into(),
            severity,
            message: message.into(),
            location,
            source: None,
            target: None,
            hint: None,
            current_value: None,
            expected: None,
            explanation: None,
            domain: None,
            owner: None,
        }
    }

    pub fn from_entry(entry: &impl DiagnosticEntry) -> Self {
        Self {
            related_locations: entry.related_locations().to_vec(),
            record_key: entry.record_key().map(str::to_owned),
            context: entry.context().cloned().unwrap_or_default(),
            code: entry.code(),
            severity: entry.severity(),
            message: entry.message().to_owned(),
            location: entry.location().clone(),
            source: entry.source().map(str::to_owned),
            target: entry.target().map(str::to_owned),
            hint: entry.hint().map(str::to_owned),
            current_value: entry.current_value().map(str::to_owned),
            expected: entry.expected().map(str::to_owned),
            explanation: entry.explanation().map(str::to_owned),
            domain: entry.domain().map(str::to_owned),
            owner: entry.owner().map(str::to_owned),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
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

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_diagnostic_parts(formatter, self)
    }
}

pub trait DiagnosticEntry {
    fn related_locations(&self) -> &[DiagnosticLocation] {
        &[]
    }
    fn record_key(&self) -> Option<&str> {
        None
    }
    fn context(&self) -> Option<&BTreeMap<String, String>> {
        None
    }
    fn code(&self) -> String;
    fn severity(&self) -> DiagnosticSeverity;
    fn message(&self) -> &str;
    fn location(&self) -> &DiagnosticLocation;
    fn source(&self) -> Option<&str> {
        None
    }
    fn target(&self) -> Option<&str> {
        None
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn current_value(&self) -> Option<&str> {
        None
    }
    fn expected(&self) -> Option<&str> {
        None
    }
    fn explanation(&self) -> Option<&str> {
        None
    }
    fn domain(&self) -> Option<&str> {
        None
    }
    fn owner(&self) -> Option<&str> {
        None
    }
}

pub fn format_diagnostic(entry: &impl DiagnosticEntry) -> String {
    (entry as &dyn DiagnosticEntry).to_string()
}

impl<T: DiagnosticEntry + ?Sized> DiagnosticEntry for &T {
    fn related_locations(&self) -> &[DiagnosticLocation] {
        (*self).related_locations()
    }
    fn record_key(&self) -> Option<&str> {
        (*self).record_key()
    }
    fn context(&self) -> Option<&BTreeMap<String, String>> {
        (*self).context()
    }
    fn code(&self) -> String {
        (*self).code()
    }

    fn severity(&self) -> DiagnosticSeverity {
        (*self).severity()
    }

    fn message(&self) -> &str {
        (*self).message()
    }

    fn location(&self) -> &DiagnosticLocation {
        (*self).location()
    }

    fn source(&self) -> Option<&str> {
        (*self).source()
    }

    fn target(&self) -> Option<&str> {
        (*self).target()
    }

    fn hint(&self) -> Option<&str> {
        (*self).hint()
    }

    fn current_value(&self) -> Option<&str> {
        (*self).current_value()
    }

    fn expected(&self) -> Option<&str> {
        (*self).expected()
    }

    fn explanation(&self) -> Option<&str> {
        (*self).explanation()
    }

    fn domain(&self) -> Option<&str> {
        (*self).domain()
    }

    fn owner(&self) -> Option<&str> {
        (*self).owner()
    }
}

impl DiagnosticEntry for Diagnostic {
    fn related_locations(&self) -> &[DiagnosticLocation] {
        &self.related_locations
    }
    fn record_key(&self) -> Option<&str> {
        self.record_key.as_deref()
    }
    fn context(&self) -> Option<&BTreeMap<String, String>> {
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
        self.source.as_deref()
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

impl fmt::Display for dyn DiagnosticEntry + '_ {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_diagnostic_parts(formatter, self)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DiagnosticReport<T> {
    pub diagnostics: Vec<T>,
}

impl<T> DiagnosticReport<T> {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn from_one(diagnostic: T) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }

    pub fn push(&mut self, diagnostic: T) {
        self.diagnostics.push(diagnostic);
    }

    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = T>) {
        self.diagnostics.extend(diagnostics);
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

impl<T: DiagnosticEntry> DiagnosticReport<T> {
    pub fn has_error(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == DiagnosticSeverity::Error)
    }

    pub fn warning_messages(&self) -> Vec<String> {
        self.messages_for(DiagnosticSeverity::Warning)
    }

    pub fn error_messages(&self) -> Vec<String> {
        self.messages_for(DiagnosticSeverity::Error)
    }

    pub fn messages_for(&self, severity: DiagnosticSeverity) -> Vec<String> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity() == severity)
            .map(|diagnostic| (diagnostic as &dyn DiagnosticEntry).to_string())
            .collect()
    }
}

fn format_diagnostic_parts(
    formatter: &mut fmt::Formatter<'_>,
    diagnostic: &dyn DiagnosticEntry,
) -> fmt::Result {
    if let Some(source) = diagnostic.source() {
        write!(formatter, "{source}: ")?;
    }
    write!(
        formatter,
        "{:?} {}: {}",
        diagnostic.severity(),
        diagnostic.code(),
        diagnostic.message()
    )?;

    let mut context = Vec::new();
    if !diagnostic.location().is_empty() {
        context.push(diagnostic.location().to_string());
    }
    if let Some(target) = diagnostic.target() {
        context.push(format!("target `{target}`"));
    }
    if !context.is_empty() {
        write!(formatter, " ({})", context.join(", "))?;
    }
    if let Some(hint) = diagnostic.hint() {
        write!(formatter, " [{hint}]")?;
    }
    for related in diagnostic.related_locations() {
        write!(formatter, " [related: {related}]")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_from_entry_preserves_enrichment_fields() {
        let source = Diagnostic::new(
            "custom.code",
            DiagnosticSeverity::Error,
            "invalid value",
            DiagnosticLocation::default(),
        )
        .with_source("validation.custom")
        .with_target("client")
        .with_hint("fix it")
        .with_current_value("bad")
        .with_expected("good")
        .with_explanation("the value is invalid")
        .with_domain("items")
        .with_owner("systems");

        let diagnostic = Diagnostic::from_entry(&source);

        assert_eq!(diagnostic.code, "custom.code");
        assert_eq!(diagnostic.source.as_deref(), Some("validation.custom"));
        assert_eq!(diagnostic.target.as_deref(), Some("client"));
        assert_eq!(diagnostic.hint.as_deref(), Some("fix it"));
        assert_eq!(diagnostic.current_value.as_deref(), Some("bad"));
        assert_eq!(diagnostic.expected.as_deref(), Some("good"));
        assert_eq!(
            diagnostic.explanation.as_deref(),
            Some("the value is invalid")
        );
        assert_eq!(diagnostic.domain.as_deref(), Some("items"));
        assert_eq!(diagnostic.owner.as_deref(), Some("systems"));
    }

    #[test]
    fn diagnostic_location_display_includes_column_context() {
        let location = DiagnosticLocation {
            source_sheet: None,
            source_path: Some(PathBuf::from("items.csv")),
            table: Some("items".to_owned()),
            field: Some("name".to_owned()),
            row_index: Some(0),
            source_row_number: Some(3),
            source_column_number: Some(2),
            source_column_name: Some("name".to_owned()),
            column_index: Some(1),
        };

        assert_eq!(
            location.to_string(),
            "source `items.csv`, table `items`, row 3, column `name`, field `name`"
        );
    }

    #[test]
    fn diagnostic_report_helpers_manage_diagnostics() {
        let first = Diagnostic::new(
            "first",
            DiagnosticSeverity::Error,
            "first message",
            DiagnosticLocation::default(),
        );
        let second = Diagnostic::new(
            "second",
            DiagnosticSeverity::Warning,
            "second message",
            DiagnosticLocation::default(),
        );

        let mut report = DiagnosticReport::from_one(first);
        assert!(!report.is_empty());
        assert_eq!(report.diagnostics.len(), 1);

        report.extend([second]);
        assert_eq!(report.diagnostics.len(), 2);
    }
}
