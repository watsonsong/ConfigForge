// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use crate::{
    ConfigForgeError, Result,
    artifact::{Artifact, ArtifactSink},
    data::DataSet,
    data_artifact::{AggregateDataExporter, TargetDataConfig},
    meta::{LinkedMetaModel, TargetTags},
};

use super::{ArtifactGenerator, GeneratorCommonInput, GeneratorDataInput, GeneratorRequirements};

mod extract;
mod options;
mod render;

#[cfg(test)]
mod tests;

use extract::extract_table_entries;
use options::options_from_task;
use render::render_pot;

pub(super) const FORMAT: &str = "gettext_pot";

pub(super) struct GettextPotExporter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TextExtractionEntry {
    pub(super) msgid: String,
    pub(super) msgctxt: Option<String>,
    pub(super) table_name: String,
    pub(super) field_path: String,
    pub(super) record_key: Option<String>,
    pub(super) source_reference: Option<String>,
    pub(super) translator_comments: Vec<String>,
}

impl AggregateDataExporter for GettextPotExporter {
    fn name(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["pot"]
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
        let (file_name, content) = render_pot_export(task, linked_meta, target_tags, data_set)?;
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
        let (file_name, content) = render_pot_export(task, linked_meta, target_tags, data_set)?;
        sink.write_text(out_dir.join(file_name), content)
    }
}

impl ArtifactGenerator for GettextPotExporter {
    fn type_id(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["pot"]
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_and_data()
    }

    fn generate_to_sink(
        &self,
        _input: &GeneratorCommonInput<'_>,
        _sink: &ArtifactSink,
    ) -> Result<()> {
        Err(ConfigForgeError::Validation(format!(
            "generator `{}` requires Data-backed artifact input",
            FORMAT
        )))
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

fn render_pot_export(
    task: &TargetDataConfig,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<(String, String)> {
    let options = options_from_task(task)?;
    let mut entries = Vec::new();
    for table_meta in &linked_meta.tables {
        if !table_meta.meta.is_exported_for(target_tags) {
            continue;
        }
        let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "data set is missing table `{}`",
                table_meta.meta.name
            ))
        })?;
        entries.extend(extract_table_entries(
            table_meta,
            table,
            target_tags,
            &options,
        )?);
    }
    let file_name = options.file_name();
    let content = render_pot(&entries, &options);
    Ok((file_name, content))
}
