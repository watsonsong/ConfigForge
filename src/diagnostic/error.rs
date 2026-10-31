// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use thiserror::Error;

use crate::diagnostic::{Diagnostic, DiagnosticLocation, DiagnosticReport, DiagnosticSeverity};

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

    #[error("meta validation failed: {}", format_diagnostic_report(.0))]
    MetaValidation(DiagnosticReport<Diagnostic>),

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
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        use super::code::*;
        let (code, path, domain) = match self {
            Self::MetaValidation(report) | Self::ValidationReport(report) => {
                return report.diagnostics.clone();
            }
            Self::ReadFile { path, .. } => (CF_IO_READ_FAILED, Some(path), "io"),
            Self::WriteFile { path, .. } => (CF_IO_WRITE_FAILED, Some(path), "io"),
            Self::ParseProject { path, .. } => (CF_PROJECT_PARSE_FAILED, Some(path), "project"),
            Self::ParseMeta { path, .. } => (CF_META_PARSE_FAILED, Some(path), "meta"),
            Self::ParseMetaString(_) => (CF_META_PARSE_FAILED, None, "meta"),
            Self::ParseCsv { path, .. } => (CF_DATA_PARSE_CSV_FAILED, Some(path), "data"),
            Self::Serialize(_) | Self::SerializeMessagePack { .. } => {
                (CF_OUTPUT_SERIALIZE_FAILED, None, "artifact")
            }
            Self::Validation(_) => (CF_BUILD_INVALID_CONFIG, None, "validation"),
        };
        let mut location = DiagnosticLocation {
            source_path: path.cloned(),
            ..Default::default()
        };
        if let Self::ParseCsv { source, .. } = self
            && let Some(position) = source.position()
        {
            location.source_row_number = usize::try_from(position.line()).ok();
        }
        let parse_error = match self {
            Self::ParseProject { source, .. }
            | Self::ParseMeta { source, .. }
            | Self::ParseMetaString(source) => Some(source),
            _ => None,
        };
        if let Some(source) = parse_error
            && let Some(span) = source.span()
            && let Some(content) = path.and_then(|path| std::fs::read_to_string(path).ok())
            && let Some(prefix) = content.get(..span.start)
        {
            location.source_row_number =
                Some(prefix.bytes().filter(|byte| *byte == b'\n').count() + 1);
            location.source_column_number = Some(
                prefix
                    .rsplit('\n')
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .count()
                    + 1,
            );
        }
        let metadata = metadata_for(code).expect("built-in error code metadata");
        let mut diagnostic =
            Diagnostic::new(code, DiagnosticSeverity::Error, self.to_string(), location)
                .with_source("error")
                .with_domain(domain)
                .with_explanation(metadata.explanation);
        if let Some(hint) = metadata.default_hint {
            diagnostic = diagnostic.with_hint(hint);
        }
        vec![diagnostic]
    }

    pub(crate) fn with_diagnostics(
        self,
        diagnostics: impl IntoIterator<Item = Diagnostic>,
    ) -> Self {
        let mut report = self.diagnostics();
        report.extend(diagnostics);
        Self::from_diagnostics(report)
    }

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

fn format_diagnostic_report(report: &DiagnosticReport<Diagnostic>) -> String {
    report.error_messages().join("; ")
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::diagnostic::{DiagnosticLocation, DiagnosticSeverity};

    use super::*;

    const LEGACY_VALIDATION_FALLBACK_BASELINE: usize = 3;

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

    #[test]
    fn diagnostic_module_does_not_depend_on_meta_module() {
        let diagnostic_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("diagnostic");
        let forbidden_markers = [
            concat!("crate", "::", "meta"),
            concat!("super", "::", "meta"),
            concat!("Meta", "Diagnostic"),
        ];

        assert_no_forbidden_markers(&diagnostic_root, &forbidden_markers);
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

    fn assert_no_forbidden_markers(path: &Path, forbidden_markers: &[&str]) {
        if path.is_dir() {
            for entry in fs::read_dir(path)
                .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", path.display()))
            {
                let entry = entry.expect("source directory entry should be readable");
                assert_no_forbidden_markers(&entry.path(), forbidden_markers);
            }
            return;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            return;
        }

        let content = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", path.display()));
        for marker in forbidden_markers {
            assert!(
                !content.contains(marker),
                "diagnostic module must not depend on meta marker `{marker}` in `{}`",
                path.display()
            );
        }
    }
}
