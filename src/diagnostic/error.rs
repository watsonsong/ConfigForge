// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use thiserror::Error;

use crate::{
    diagnostic::{Diagnostic, DiagnosticReport},
    meta::MetaDiagnostic,
};

pub type Result<T> = std::result::Result<T, ConfigForgeError>;

#[derive(Debug, Error)]
pub enum ConfigForgeError {
    #[error("failed to read `{path}`: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write `{path}`: {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse meta `{path}`: {source}")]
    ParseMeta {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to parse meta: {0}")]
    ParseMetaString(#[source] toml::de::Error),

    #[error("failed to parse project `{path}`: {source}")]
    ParseProject {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("meta validation failed: {}", format_meta_diagnostics(.0))]
    MetaValidation(Vec<MetaDiagnostic>),

    #[error("failed to parse csv `{path}`: {source}")]
    ParseCsv {
        path: PathBuf,
        #[source]
        source: csv::Error,
    },

    #[error("failed to serialize export output: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("failed to serialize messagepack export output: {source}")]
    SerializeMessagePack {
        #[source]
        source: rmp_serde::encode::Error,
    },

    #[error("validation failed: {0}")]
    /// Legacy unstructured validation fallback. Prefer `ValidationReport` for
    /// new ConfigForge-produced business validation errors.
    Validation(String),

    #[error("validation failed: {}", format_diagnostic_report(.0))]
    ValidationReport(DiagnosticReport<Diagnostic>),
}

impl ConfigForgeError {
    pub fn from_diagnostic(diagnostic: Diagnostic) -> Self {
        Self::ValidationReport(DiagnosticReport::from_one(diagnostic))
    }

    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        Self::ValidationReport(DiagnosticReport { diagnostics })
    }

    pub fn from_diagnostic_report(report: DiagnosticReport<Diagnostic>) -> Self {
        Self::ValidationReport(report)
    }
}

fn format_meta_diagnostics(diagnostics: &[MetaDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_diagnostic_report(report: &DiagnosticReport<Diagnostic>) -> String {
    report.error_messages().join("; ")
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::diagnostic::{DiagnosticLocation, DiagnosticSeverity};

    use super::*;

    const LEGACY_VALIDATION_FALLBACK_BASELINE: usize = 100;

    #[test]
    fn structured_error_helpers_create_validation_report_errors() {
        let diagnostic = Diagnostic::new(
            "custom.code",
            DiagnosticSeverity::Error,
            "invalid value",
            DiagnosticLocation::default(),
        );

        let error = ConfigForgeError::from_diagnostic(diagnostic.clone());
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected validation report error");
        };
        assert_eq!(report.diagnostics, vec![diagnostic.clone()]);

        let error = ConfigForgeError::from_diagnostics(vec![diagnostic]);
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected validation report error");
        };
        assert_eq!(report.diagnostics.len(), 1);
    }

    #[test]
    fn legacy_validation_fallback_debt_does_not_increase() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let count = count_legacy_validation_fallbacks(&source_root);

        assert!(
            count <= LEGACY_VALIDATION_FALLBACK_BASELINE,
            "legacy validation fallback count increased: {count} > {LEGACY_VALIDATION_FALLBACK_BASELINE}; use structured diagnostics for new ConfigForge-produced business errors"
        );
    }

    fn count_legacy_validation_fallbacks(path: &Path) -> usize {
        if path.is_dir() {
            return fs::read_dir(path)
                .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", path.display()))
                .map(|entry| {
                    let entry = entry.expect("source directory entry should be readable");
                    count_legacy_validation_fallbacks(&entry.path())
                })
                .sum();
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            return 0;
        }
        let content = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", path.display()));
        content
            .match_indices(concat!("ConfigForgeError", "::", "Validation", "("))
            .count()
    }
}
