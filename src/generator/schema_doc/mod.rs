// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod build;
mod format;
mod html;
mod markdown;
mod model;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use crate::{
    Result,
    artifact::ArtifactSink,
    generator::{ArtifactGenerator, GeneratorCommonInput, GeneratorRequirements},
};

use format::{
    schema_doc_export_config_from_options, schema_doc_output_paths, write_schema_doc_to_sink,
};

pub use format::{SchemaDocExportConfig, SchemaDocFormat};
pub use markdown::{render_schema_doc_index_markdown, render_schema_table_markdown};
pub use model::{SchemaDoc, SchemaDocOptions};

pub(crate) struct SchemaDocGenerator;

impl ArtifactGenerator for SchemaDocGenerator {
    fn type_id(&self) -> &'static str {
        "schema_doc"
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_only()
    }

    fn generate_to_sink(
        &self,
        input: &GeneratorCommonInput<'_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let config =
            schema_doc_export_config_from_options(input.output.clone(), input.options.clone())?;
        let doc = schema_doc_from_input(input, config.title.clone());
        write_schema_doc_to_sink(&doc, &config, sink)
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        let config =
            schema_doc_export_config_from_options(input.output.clone(), input.options.clone())?;
        let doc = schema_doc_from_input(input, config.title.clone());
        let paths = schema_doc_output_paths(&doc, &config)?;
        Ok(std::iter::once(paths.index_path)
            .chain(paths.table_paths)
            .collect())
    }
}

fn schema_doc_from_input(input: &GeneratorCommonInput<'_>, title: Option<String>) -> SchemaDoc {
    SchemaDoc::from_meta(
        input.meta,
        &input.project,
        SchemaDocOptions {
            title,
            project_name: input.project.name.clone(),
            project_path: Some(input.project_root.clone()),
            target: Some(input.target_name.clone()),
            target_tags: input.target_tags.clone(),
            generated_at: input.generated_at.clone(),
            include_hidden: false,
            include_internal: false,
            include_struct_details: true,
            include_asset_rules: true,
            include_i18n: true,
        },
    )
}
