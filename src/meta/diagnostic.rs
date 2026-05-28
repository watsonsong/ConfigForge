// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use crate::diagnostic::{
    format_diagnostic, DiagnosticEntry, DiagnosticLocation, DiagnosticSeverity,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MetaDiagnostic {
    pub code: MetaDiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub location: DiagnosticLocation,
}

impl MetaDiagnostic {
    pub fn new(code: MetaDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            location: DiagnosticLocation::default(),
        }
    }
}

impl fmt::Display for MetaDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&format_diagnostic(self))
    }
}

impl DiagnosticEntry for MetaDiagnostic {
    fn code(&self) -> String {
        format!("{:?}", self.code)
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MetaDiagnosticCode {
    DuplicateTableName,
    DuplicateEnumName,
    DuplicateStructName,
    DuplicateEnumValueName,
    DuplicateFieldName,
    DuplicateIndexName,
    DuplicateTypeName,
    ReservedTypeName,
    MissingFieldReference,
    InvalidPrimaryIndex,
    PrimaryIndexMismatch,
    EmptyIndexFields,
    UnknownFieldType,
    InvalidDefaultValue,
    InvalidSourceDeclaration,
    InvalidValidatorConfig,
    InvalidForeignKeyDeclaration,
    UnsupportedFeature,
}
