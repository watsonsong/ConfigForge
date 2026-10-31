// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use crate::diagnostic::{
    Diagnostic, DiagnosticEntry, DiagnosticLocation, DiagnosticReport, DiagnosticSeverity,
    code::{self, *},
    format_diagnostic,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MetaDiagnostic {
    pub related_locations: Vec<DiagnosticLocation>,
    pub code: MetaDiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub location: DiagnosticLocation,
}

impl MetaDiagnostic {
    pub fn new(code: MetaDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            related_locations: Vec::new(),
            code,
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            location: DiagnosticLocation::default(),
        }
    }
}

pub(crate) fn meta_diagnostics_to_report(
    diagnostics: Vec<MetaDiagnostic>,
) -> DiagnosticReport<Diagnostic> {
    DiagnosticReport {
        diagnostics: diagnostics
            .into_iter()
            .map(|diagnostic| Diagnostic::from_entry(&diagnostic))
            .collect(),
    }
}

impl fmt::Display for MetaDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&format_diagnostic(self))
    }
}

impl DiagnosticEntry for MetaDiagnostic {
    fn related_locations(&self) -> &[DiagnosticLocation] {
        &self.related_locations
    }
    fn code(&self) -> String {
        self.code.stable_code().to_owned()
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
        Some("meta.validate")
    }

    fn hint(&self) -> Option<&str> {
        code::metadata_for(self.code.stable_code()).and_then(|metadata| metadata.default_hint)
    }

    fn explanation(&self) -> Option<&str> {
        code::metadata_for(self.code.stable_code()).map(|metadata| metadata.explanation)
    }

    fn domain(&self) -> Option<&str> {
        Some("meta")
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
    DuplicateAssetKindName,
    InvalidAssetKindDeclaration,
    UnsupportedFeature,
}

impl MetaDiagnosticCode {
    pub fn stable_code(self) -> &'static str {
        match self {
            Self::DuplicateTableName => CF_META_DUPLICATE_TABLE_NAME,
            Self::DuplicateEnumName => CF_META_DUPLICATE_ENUM_NAME,
            Self::DuplicateStructName => CF_META_DUPLICATE_STRUCT_NAME,
            Self::DuplicateEnumValueName => CF_META_DUPLICATE_ENUM_VALUE_NAME,
            Self::DuplicateFieldName => CF_META_DUPLICATE_FIELD_NAME,
            Self::DuplicateIndexName => CF_META_DUPLICATE_INDEX_NAME,
            Self::DuplicateTypeName => CF_META_DUPLICATE_TYPE_NAME,
            Self::ReservedTypeName => CF_META_RESERVED_TYPE_NAME,
            Self::MissingFieldReference => CF_META_MISSING_FIELD_REFERENCE,
            Self::InvalidPrimaryIndex => CF_META_INVALID_PRIMARY_INDEX,
            Self::PrimaryIndexMismatch => CF_META_PRIMARY_INDEX_MISMATCH,
            Self::EmptyIndexFields => CF_META_EMPTY_INDEX_FIELDS,
            Self::UnknownFieldType => CF_META_UNKNOWN_FIELD_TYPE,
            Self::InvalidDefaultValue => CF_META_INVALID_DEFAULT_VALUE,
            Self::InvalidSourceDeclaration => CF_META_INVALID_SOURCE_DECLARATION,
            Self::InvalidValidatorConfig => CF_META_INVALID_VALIDATOR_CONFIG,
            Self::InvalidForeignKeyDeclaration => CF_META_INVALID_FOREIGN_KEY_DECLARATION,
            Self::DuplicateAssetKindName => CF_META_DUPLICATE_ASSET_KIND_NAME,
            Self::InvalidAssetKindDeclaration => CF_META_INVALID_ASSET_KIND_DECLARATION,
            Self::UnsupportedFeature => CF_META_UNSUPPORTED_FEATURE,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::code::metadata_for;

    use super::*;

    const ALL_META_DIAGNOSTIC_CODES: &[MetaDiagnosticCode] = &[
        MetaDiagnosticCode::DuplicateTableName,
        MetaDiagnosticCode::DuplicateEnumName,
        MetaDiagnosticCode::DuplicateStructName,
        MetaDiagnosticCode::DuplicateEnumValueName,
        MetaDiagnosticCode::DuplicateFieldName,
        MetaDiagnosticCode::DuplicateIndexName,
        MetaDiagnosticCode::DuplicateTypeName,
        MetaDiagnosticCode::ReservedTypeName,
        MetaDiagnosticCode::MissingFieldReference,
        MetaDiagnosticCode::InvalidPrimaryIndex,
        MetaDiagnosticCode::PrimaryIndexMismatch,
        MetaDiagnosticCode::EmptyIndexFields,
        MetaDiagnosticCode::UnknownFieldType,
        MetaDiagnosticCode::InvalidDefaultValue,
        MetaDiagnosticCode::InvalidSourceDeclaration,
        MetaDiagnosticCode::InvalidValidatorConfig,
        MetaDiagnosticCode::InvalidForeignKeyDeclaration,
        MetaDiagnosticCode::DuplicateAssetKindName,
        MetaDiagnosticCode::InvalidAssetKindDeclaration,
        MetaDiagnosticCode::UnsupportedFeature,
    ];

    #[test]
    fn all_meta_diagnostic_codes_have_catalog_entries() {
        for code in ALL_META_DIAGNOSTIC_CODES {
            let stable_code = code.stable_code();
            let metadata = metadata_for(stable_code)
                .unwrap_or_else(|| panic!("missing diagnostic code metadata for `{stable_code}`"));
            assert_eq!(metadata.domain, "meta");
            assert!(!metadata.title.trim().is_empty());
            assert!(!metadata.explanation.trim().is_empty());
        }
    }
}
