use std::path::PathBuf;

use thiserror::Error;

use crate::meta::MetaDiagnostic;

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

    #[error("validation failed: {0}")]
    Validation(String),
}

fn format_meta_diagnostics(diagnostics: &[MetaDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}
