// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    artifact::{Artifact, ArtifactSink},
    data::DataSet,
    data_artifact::{AggregateDataExporter, TargetDataConfig},
    diagnostic::code::{
        CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH, CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    },
    meta::{LinkedMetaModel, TargetTags},
};

use super::{
    ArtifactGenerator, GeneratorCommonInput, GeneratorDataInput, GeneratorRequirements,
    artifact_generator::generator_contract_error,
};

mod extract;
mod options;
mod render;

#[cfg(test)]
mod tests;

use extract::{build_manifest_entries, extract_table_asset_references};
use options::{AssetManifestFormat, options_from_task};
use render::{render_csv_manifest, render_json_manifest};

pub(super) const FORMAT: &str = "asset_manifest";

pub(super) struct AssetManifestExporter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AssetManifestEntry {
    pub(super) kind: String,
    pub(super) path: String,
    pub(super) referenced_by: Vec<AssetReferenceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(super) struct AssetReferenceEntry {
    pub(super) table: String,
    pub(super) row: String,
    pub(super) field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) source: Option<String>,
}

impl AggregateDataExporter for AssetManifestExporter {
    fn name(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["assets"]
    }

    fn plan_artifacts(
        &self,
        out_dir: &Path,
        task: &TargetDataConfig,
        _linked_meta: &LinkedMetaModel<'_>,
        _target_tags: &TargetTags,
    ) -> Result<Vec<PathBuf>> {
        let options = options_from_task(task)?;
        Ok(vec![out_dir.join(options.file_name())])
    }

    fn export(
        &self,
        out_dir: &Path,
        task: &TargetDataConfig,
        linked_meta: &LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
    ) -> Result<Vec<Artifact>> {
        let (file_name, content) = render_asset_manifest(task, linked_meta, target_tags, data_set)?;
        Ok(vec![Artifact::text(out_dir.join(file_name), content)])
    }

    fn export_to_sink(
        &self,
        out_dir: &Path,
        task: &TargetDataConfig,
        linked_meta: &LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let (file_name, content) = render_asset_manifest(task, linked_meta, target_tags, data_set)?;
        sink.write_text(out_dir.join(file_name), content)
    }
}

impl ArtifactGenerator for AssetManifestExporter {
    fn type_id(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["assets"]
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_and_data().with_assets()
    }

    fn generate_to_sink(
        &self,
        _input: &GeneratorCommonInput<'_>,
        _sink: &ArtifactSink,
    ) -> Result<()> {
        Err(generator_contract_error(
            FORMAT,
            format!("generator `{FORMAT}` requires Data-backed artifact input"),
            "meta-only generation call",
            "data-backed generation input",
            "Use the generator registry data-backed pipeline for asset_manifest exports.",
        ))
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        let task = input.target_data_config();
        <Self as AggregateDataExporter>::plan_artifacts(
            self,
            &input.output,
            &task,
            input.meta,
            &input.target_tags,
        )
    }

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let task = input.common.target_data_config();
        <Self as AggregateDataExporter>::export_to_sink(
            self,
            &input.common.output,
            &task,
            input.common.meta,
            &input.common.target_tags,
            input.data_set,
            sink,
        )
    }
}

fn render_asset_manifest(
    task: &TargetDataConfig,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<(String, String)> {
    let options = options_from_task(task)?;
    let mut references = Vec::new();
    for table_meta in &linked_meta.tables {
        if !table_meta.meta.is_exported_for(target_tags) {
            continue;
        }
        let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
            asset_manifest_input_error(
                format!("data set is missing table `{}`", table_meta.meta.name),
                table_meta.meta.name.clone(),
                "visible table present in prepared data set",
                "Ensure data loading completed for all target-visible asset manifest tables.",
            )
        })?;
        extract_table_asset_references(&mut references, table_meta, table, target_tags, &options);
    }

    let entries = build_manifest_entries(references, &options);
    let content = match options.format {
        AssetManifestFormat::Json => render_json_manifest(&entries, target_tags, &options)?,
        AssetManifestFormat::Csv => render_csv_manifest(&entries, &options)?,
    };
    Ok((options.file_name(), content))
}

fn asset_manifest_input_error(
    message: impl Into<String>,
    current: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source("generator.asset_manifest.input")
        .with_domain("generator")
        .with_current_value(current)
        .with_expected(expected)
        .with_hint(hint),
    )
}

pub(super) fn asset_manifest_render_error(
    message: impl Into<String>,
    current: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source("generator.asset_manifest.render")
        .with_domain("generator")
        .with_current_value(current)
        .with_expected(expected)
        .with_hint(hint),
    )
}
