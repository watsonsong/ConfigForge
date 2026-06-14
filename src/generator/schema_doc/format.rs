// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt, path::PathBuf, str::FromStr};

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    artifact::ArtifactSink,
    diagnostic::code::{
        CF_ARTIFACT_PATH_COLLISION, CF_GENERATOR_INVALID_OPTION, CF_GENERATOR_UNSUPPORTED_OPTION,
    },
    path_util::lexical_normalize_path,
};

use super::{
    html::{render_schema_doc_index_html, render_schema_table_html},
    markdown::{render_schema_doc_index_markdown, render_schema_table_markdown},
    model::{SchemaDoc, SchemaTableDoc},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaDocFormat {
    Markdown,
    Html,
}

impl SchemaDocFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Html => "html",
        }
    }
}

impl fmt::Display for SchemaDocFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Markdown => "markdown",
            Self::Html => "html",
        })
    }
}

impl FromStr for SchemaDocFormat {
    type Err = ConfigForgeError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "markdown" | "md" => Ok(Self::Markdown),
            "html" => Ok(Self::Html),
            other => Err(schema_doc_option_error(
                CF_GENERATOR_UNSUPPORTED_OPTION,
                "format",
                format!(
                    "unsupported schema_doc format `{other}`; supported formats: markdown, html"
                ),
                other,
                "markdown or html",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDocExportConfig {
    pub format: SchemaDocFormat,
    pub output: PathBuf,
    pub index_file: String,
    pub title: Option<String>,
}

pub fn write_schema_doc_to_sink(
    doc: &SchemaDoc,
    config: &SchemaDocExportConfig,
    sink: &ArtifactSink,
) -> Result<()> {
    let artifacts = render_schema_doc_artifacts(doc, config)?;
    for artifact in artifacts {
        sink.write_text(artifact.path, artifact.content)?;
    }
    Ok(())
}

pub fn render_schema_doc_artifacts(
    doc: &SchemaDoc,
    config: &SchemaDocExportConfig,
) -> Result<Vec<SchemaDocRenderedArtifact>> {
    let paths = schema_doc_output_paths(doc, config)?;
    match config.format {
        SchemaDocFormat::Markdown => {
            let mut artifacts = Vec::with_capacity(1 + doc.tables.len());
            artifacts.push(SchemaDocRenderedArtifact {
                path: paths.index_path,
                content: render_schema_doc_index_markdown(doc),
            });
            for (table, path) in doc.tables.iter().zip(paths.table_paths) {
                artifacts.push(SchemaDocRenderedArtifact {
                    path,
                    content: render_schema_table_markdown(doc, table),
                });
            }
            Ok(artifacts)
        }
        SchemaDocFormat::Html => {
            let mut artifacts = Vec::with_capacity(1 + doc.tables.len());
            artifacts.push(SchemaDocRenderedArtifact {
                path: paths.index_path,
                content: render_schema_doc_index_html(doc, &paths.table_file_names),
            });
            for (table, path) in doc.tables.iter().zip(paths.table_paths) {
                artifacts.push(SchemaDocRenderedArtifact {
                    path,
                    content: render_schema_table_html(doc, table, &config.index_file),
                });
            }
            Ok(artifacts)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDocRenderedArtifact {
    pub path: PathBuf,
    pub content: String,
}

pub fn schema_doc_output_paths(
    doc: &SchemaDoc,
    config: &SchemaDocExportConfig,
) -> Result<SchemaDocOutputPaths> {
    validate_index_file_name(&config.index_file, config.format)?;
    let index_path = lexical_normalize_path(&config.output.join(&config.index_file));
    let mut seen = std::collections::BTreeSet::new();
    if !seen.insert(index_path.clone()) {
        return Err(schema_doc_path_collision_error(&index_path));
    }
    let mut table_paths = Vec::with_capacity(doc.tables.len());
    let mut table_file_names = Vec::with_capacity(doc.tables.len());
    for table in &doc.tables {
        let file_name = table_file_name(table, config.format);
        validate_file_name(&file_name, "schema_doc table file")?;
        let path = lexical_normalize_path(&config.output.join(&file_name));
        if !seen.insert(path.clone()) {
            return Err(schema_doc_path_collision_error(&path));
        }
        table_file_names.push(file_name);
        table_paths.push(path);
    }
    Ok(SchemaDocOutputPaths {
        index_path,
        table_paths,
        table_file_names,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDocOutputPaths {
    pub index_path: PathBuf,
    pub table_paths: Vec<PathBuf>,
    pub table_file_names: Vec<String>,
}

pub(super) fn table_file_name(table: &SchemaTableDoc, format: SchemaDocFormat) -> String {
    format!("{}.{}", table.name, format.extension())
}

pub(super) fn validate_file_name(value: &str, label: &str) -> Result<()> {
    let path = std::path::Path::new(value);
    let valid = !value.is_empty()
        && path.file_name().and_then(|name| name.to_str()) == Some(value)
        && path.components().count() == 1
        && !matches!(
            path.components().next(),
            Some(std::path::Component::ParentDir | std::path::Component::CurDir)
        );
    if valid {
        Ok(())
    } else {
        Err(schema_doc_option_error(
            CF_GENERATOR_INVALID_OPTION,
            label,
            format!("{label} `{value}` must be a plain file name"),
            value,
            "plain file name without path separators",
        ))
    }
}

fn validate_index_file_name(value: &str, format: SchemaDocFormat) -> Result<()> {
    validate_file_name(value, "schema_doc index_file")?;
    if format == SchemaDocFormat::Html && !value.ends_with(".html") {
        return Err(schema_doc_option_error(
            CF_GENERATOR_INVALID_OPTION,
            "index_file",
            format!("schema_doc html index_file `{value}` must use the .html extension"),
            value,
            "plain .html file name",
        ));
    }
    Ok(())
}

pub fn schema_doc_export_config_from_options(
    output: PathBuf,
    mut options: BTreeMap<String, toml::Value>,
) -> Result<SchemaDocExportConfig> {
    let format = take_schema_doc_string(&mut options, "format")?
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(SchemaDocFormat::Markdown);
    let index_file = take_schema_doc_string(&mut options, "index_file")?
        .unwrap_or_else(|| format!("index.{}", format.extension()));
    validate_index_file_name(&index_file, format)?;
    let title = take_schema_doc_string(&mut options, "title")?;
    let unsupported = options.keys().cloned().collect::<Vec<_>>();
    if let Some(key) = unsupported.first() {
        return Err(schema_doc_option_error(
            CF_GENERATOR_UNSUPPORTED_OPTION,
            key,
            format!("schema_doc export task has unknown option `{key}`"),
            key,
            "supported schema_doc option",
        ));
    }
    Ok(SchemaDocExportConfig {
        format,
        output,
        index_file,
        title,
    })
}

fn take_schema_doc_string(
    options: &mut BTreeMap<String, toml::Value>,
    key: &str,
) -> Result<Option<String>> {
    options
        .remove(key)
        .map(|value| {
            value.try_into().map_err(|source| {
                schema_doc_option_error(
                    CF_GENERATOR_INVALID_OPTION,
                    key,
                    format!("invalid schema_doc option `{key}`: {source}"),
                    key,
                    "string value",
                )
            })
        })
        .transpose()
}

fn schema_doc_option_error(
    code: &'static str,
    option: impl Into<String>,
    message: impl Into<String>,
    current: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            code,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source("generator.schema_doc.options")
        .with_current_value(current)
        .with_expected(expected)
        .with_hint(format!(
            "Check the schema_doc export task option `{}`.",
            option.into()
        )),
    )
}

fn schema_doc_path_collision_error(path: &std::path::Path) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_ARTIFACT_PATH_COLLISION,
            DiagnosticSeverity::Error,
            format!("schema_doc output path `{}` is duplicated", path.display()),
            DiagnosticLocation::default(),
        )
        .with_source("generator.schema_doc.output")
        .with_current_value(path.display().to_string())
        .with_expected("unique schema_doc output paths")
        .with_hint("Rename the schema_doc index_file or table so generated files do not collide."),
    )
}
