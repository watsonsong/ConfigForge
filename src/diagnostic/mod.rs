// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

mod error;

pub use error::{ConfigForgeError, Result};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    #[default]
    Error,
    Warning,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiagnosticLocation {
    pub source_path: Option<PathBuf>,
    pub table: Option<String>,
    pub field: Option<String>,
    pub row_index: Option<usize>,
    pub source_row_number: Option<usize>,
}

impl DiagnosticLocation {
    pub fn is_empty(&self) -> bool {
        self.source_path.is_none()
            && self.table.is_none()
            && self.field.is_none()
            && self.row_index.is_none()
            && self.source_row_number.is_none()
    }
}

impl fmt::Display for DiagnosticLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(source_path) = &self.source_path {
            parts.push(format!("source `{}`", source_path.display()));
        }
        if let Some(table) = &self.table {
            parts.push(format!("table `{table}`"));
        }
        if let Some(row_number) = self.source_row_number {
            parts.push(format!("row {row_number}"));
        } else if let Some(row_index) = self.row_index {
            parts.push(format!("row {}", row_index + 1));
        }
        if let Some(field) = &self.field {
            parts.push(format!("field `{field}`"));
        }
        write!(formatter, "{}", parts.join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub location: DiagnosticLocation,
    pub source: Option<String>,
    pub target: Option<String>,
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn new(
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
        location: DiagnosticLocation,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            location,
            source: None,
            target: None,
            hint: None,
        }
    }

    pub fn from_entry(entry: &impl DiagnosticEntry) -> Self {
        Self {
            code: entry.code(),
            severity: entry.severity(),
            message: entry.message().to_owned(),
            location: entry.location().clone(),
            source: entry.source().map(str::to_owned),
            target: entry.target().map(str::to_owned),
            hint: entry.hint().map(str::to_owned),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_diagnostic_parts(formatter, self)
    }
}

pub trait DiagnosticEntry {
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
}

pub fn format_diagnostic(entry: &impl DiagnosticEntry) -> String {
    (entry as &dyn DiagnosticEntry).to_string()
}

impl<T: DiagnosticEntry + ?Sized> DiagnosticEntry for &T {
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
}

impl DiagnosticEntry for Diagnostic {
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
}

impl fmt::Display for dyn DiagnosticEntry + '_ {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_diagnostic_parts(formatter, self)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticReport<T> {
    pub diagnostics: Vec<T>,
}

impl<T> DiagnosticReport<T> {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn push(&mut self, diagnostic: T) {
        self.diagnostics.push(diagnostic);
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
    Ok(())
}
