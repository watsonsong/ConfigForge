// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    artifact::ArtifactSink,
    data::DataSet,
    data_artifact::TargetDataConfig,
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    meta::{LinkedMetaModel, TargetTags},
};

use super::{ArtifactGenerator, GeneratorCommonInput, GeneratorRequirements};

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

impl GettextPotExporter {
    fn plan_paths(
        &self,
        out_dir: &Path,
        task: &TargetDataConfig,
        _linked_meta: &LinkedMetaModel<'_>,
        _target_tags: &TargetTags,
    ) -> Result<Vec<PathBuf>> {
        let options = options_from_task(task)?;
        Ok(vec![out_dir.join(options.file_name())])
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

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        let task = input.target_data_config();
        self.plan_paths(&input.output, &task, input.meta, &input.target_tags)
    }

    fn generate(
        &self,
        input: &crate::generator::GeneratorTaskInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let input = input.data_input(self.type_id())?;
        let task = input.common.target_data_config();
        self.export_to_sink(
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
            gettext_pot_extraction_error(
                format!("data set is missing table `{}`", table_meta.meta.name),
                table_meta.meta.name.clone(),
                "visible table present in prepared data set",
                "Ensure data loading completed for all target-visible gettext_pot tables.",
            )
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

pub(super) fn gettext_pot_extraction_error(
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
        .with_source("generator.gettext_pot.input")
        .with_domain("generator")
        .with_current_value(current)
        .with_expected(expected)
        .with_hint(hint),
    )
}
