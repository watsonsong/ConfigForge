// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::CF_DATA_SOURCE_SCHEMA_INVALID,
    meta::{MetaModel, SourceRef, TableKind, TableMeta, TargetTags},
};

use super::SourceRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataProvider {
    Csv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourcePlan {
    pub table_name: String,
    pub provider: DataProvider,
    pub path: PathBuf,
    pub header_row: usize,
    pub data_start_row: usize,
    pub fragments: Vec<String>,
    pub layout: DataSourceLayout,
    pub fields: Vec<String>,
    pub role: SourceRole,
    pub primary_key_fields: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSourceLayout {
    CollectionRecords,
    ObjectKeyValueRows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLoadPlan {
    pub source_read_plan: SourceReadPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceReadPlan {
    sources: Vec<DataSourcePlan>,
}

impl DataProvider {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Csv => "csv",
        }
    }
}

impl DataLoadPlan {
    pub fn new(source_read_plan: SourceReadPlan) -> Self {
        Self { source_read_plan }
    }

    pub fn from_meta_model(model: &MetaModel) -> Result<Self> {
        Self::from_tables(&model.tables, None)
    }

    pub fn from_tables<'a>(
        tables: impl IntoIterator<Item = &'a TableMeta>,
        target_tags: Option<&TargetTags>,
    ) -> Result<Self> {
        Ok(Self {
            source_read_plan: SourceReadPlan::from_tables(tables, target_tags)?,
        })
    }

    pub fn from_source_read_plan(source_read_plan: SourceReadPlan) -> Self {
        Self::new(source_read_plan)
    }

    pub fn source_read_plan(&self) -> &SourceReadPlan {
        &self.source_read_plan
    }

    pub fn sources(&self) -> &[DataSourcePlan] {
        self.source_read_plan.sources()
    }
}

impl SourceReadPlan {
    pub fn new(sources: Vec<DataSourcePlan>) -> Self {
        Self { sources }
    }

    pub fn from_meta_model(model: &MetaModel) -> Result<Self> {
        Self::from_tables(&model.tables, None)
    }

    pub fn from_tables<'a>(
        tables: impl IntoIterator<Item = &'a TableMeta>,
        target_tags: Option<&TargetTags>,
    ) -> Result<Self> {
        let mut sources = Vec::new();
        for table in tables {
            sources.extend(DataSourcePlan::from_table(table, target_tags)?);
        }
        Ok(Self { sources })
    }

    pub fn sources(&self) -> &[DataSourcePlan] {
        &self.sources
    }
}

impl DataSourcePlan {
    pub fn from_table(table: &TableMeta, target_tags: Option<&TargetTags>) -> Result<Vec<Self>> {
        if table.source.is_some() && !table.sources.is_empty() {
            return Err(source_plan_error(
                table,
                format!(
                    "table `{}` declares both legacy `source` and `sources`",
                    table.name
                ),
                "legacy `source` plus `sources`",
                "declare either `source` or `sources`, not both",
            ));
        }

        if table.kind == TableKind::Collection
            && table.sources.len() > 1
            && !table.is_variant_source_mode()
        {
            return Err(source_plan_error(
                table,
                format!(
                    "table `{}` declares multiple sources, but only polymorphic collection tables with fragments support variant source mode",
                    table.name
                ),
                "multiple sources",
                "polymorphic collection table with fragments",
            ));
        }

        for source in &table.sources {
            if source.export_when != crate::meta::ExportCondition::default() {
                return Err(source_plan_error(
                    table,
                    format!(
                        "table `{}` base source `{}` declares `export_when`, but source-level target filtering is currently supported only for `override_sources`",
                        table.name, source.path
                    ),
                    format!("base source `{}` export_when", source.path),
                    "export_when only on override_sources",
                ));
            }
        }

        let primary_key_fields = primary_key_fields_for_overrides(table)?;

        let mut plans = if !table.sources.is_empty() {
            table
                .sources
                .iter()
                .map(|source| {
                    Self::from_source_ref(
                        table,
                        source,
                        SourceRole::Base,
                        primary_key_fields.clone(),
                    )
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            let source = if let Some(source) = &table.source {
                source
            } else {
                return Err(source_plan_error(
                    table,
                    format!("table `{}` does not declare a source", table.name),
                    "<missing>",
                    "table source path",
                ));
            };

            let layout = layout_for_table(table);
            let data_start_row = if matches!(layout, DataSourceLayout::ObjectKeyValueRows)
                && source.data_start_row == 3
            {
                2
            } else {
                source.data_start_row
            };

            vec![Self {
                table_name: table.name.clone(),
                provider: DataProvider::parse(&source.provider).ok_or_else(|| {
                    unsupported_provider_error(&table.name, &source.path, &source.provider)
                })?,
                path: PathBuf::from(&source.path),
                header_row: source.header_row,
                data_start_row,
                fragments: Vec::new(),
                layout,
                fields: table
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect(),
                role: SourceRole::Base,
                primary_key_fields: primary_key_fields.clone(),
            }]
        };

        for source in &table.override_sources {
            if target_tags.is_some_and(|target_tags| !source.export_when.matches(target_tags)) {
                continue;
            }
            plans.push(Self::from_source_ref(
                table,
                source,
                SourceRole::Override,
                primary_key_fields.clone(),
            )?);
        }

        Ok(plans)
    }

    fn from_source_ref(
        table: &TableMeta,
        source: &SourceRef,
        role: SourceRole,
        primary_key_fields: Vec<String>,
    ) -> Result<Self> {
        let provider = DataProvider::parse(&source.provider).ok_or_else(|| {
            unsupported_provider_error(&table.name, &source.path, &source.provider)
        })?;
        let layout = layout_for_table(table);
        let data_start_row = if matches!(layout, DataSourceLayout::ObjectKeyValueRows)
            && source.data_start_row == 3
        {
            2
        } else {
            source.data_start_row
        };

        Ok(Self {
            table_name: table.name.clone(),
            provider,
            path: PathBuf::from(&source.path),
            header_row: source.header_row,
            data_start_row,
            fragments: source.fragments.clone(),
            layout,
            fields: table
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect(),
            role,
            primary_key_fields,
        })
    }
}

fn primary_key_fields_for_overrides(table: &TableMeta) -> Result<Vec<String>> {
    if table.override_sources.is_empty() {
        return Ok(Vec::new());
    }
    if table.kind == TableKind::Object {
        return Ok(Vec::new());
    }
    let primary_index = table.primary_index().ok_or_else(|| {
        source_plan_error(
            table,
            format!(
                "table `{}` declares override_sources, but collection table overrides require a primary index",
                table.name
            ),
            "override_sources without primary index",
            "primary index for collection table overrides",
        )
    })?;
    if primary_index.fields.is_empty() {
        return Err(source_plan_error(
            table,
            format!(
                "table `{}` declares override_sources, but primary index `{}` has no fields",
                table.name, primary_index.name
            ),
            format!("primary index `{}` with no fields", primary_index.name),
            "primary index with at least one field",
        ));
    }
    Ok(primary_index.fields.clone())
}

fn layout_for_table(table: &TableMeta) -> DataSourceLayout {
    match table.kind {
        TableKind::Object => DataSourceLayout::ObjectKeyValueRows,
        TableKind::Collection => DataSourceLayout::CollectionRecords,
    }
}

fn source_plan_error(
    table: &TableMeta,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_SOURCE_SCHEMA_INVALID,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                table: Some(table.name.clone()),
                ..DiagnosticLocation::default()
            },
        )
        .with_source("data.source_plan")
        .with_domain("data")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint("Check table source / sources / override_sources declarations in Meta."),
    )
}

fn unsupported_provider_error(
    table_name: &str,
    source_path: &str,
    provider: &str,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_SOURCE_SCHEMA_INVALID,
            DiagnosticSeverity::Error,
            format!(
                "table `{table_name}` source `{source_path}` uses unsupported provider `{provider}`; only `csv` is supported"
            ),
            DiagnosticLocation {
                table: Some(table_name.to_owned()),
                source_path: Some(PathBuf::from(source_path)),
                ..DiagnosticLocation::default()
            },
        )
        .with_source("data.source_plan")
        .with_domain("data")
        .with_current_value(provider)
        .with_expected("csv")
        .with_hint("Use provider = \"csv\" for current CSV-first source loading."),
    )
}
