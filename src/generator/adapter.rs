// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ConfigForgeError, Result,
    artifact::ArtifactSink,
    code_artifact::{
        CodeExportInput, CodeExportProject, CodeExportTarget, CodeGenerator, CodegenTaskConfig,
    },
    data_artifact::{AggregateDataExporter, DataFormatExporter, export_runtime_data_to_sink},
};

use super::{
    ArtifactGenerator, GeneratorCommonInput, GeneratorDataInput, GeneratorDescriptor,
    GeneratorRequirements,
};

pub(super) struct CodeArtifactGenerator(pub(super) Box<dyn CodeGenerator>);

impl ArtifactGenerator for CodeArtifactGenerator {
    fn type_id(&self) -> &'static str {
        self.0.language()
    }

    fn aliases(&self) -> &'static [&'static str] {
        self.0.language_aliases()
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_only()
    }

    fn descriptor(&self) -> GeneratorDescriptor {
        GeneratorDescriptor::new(self.type_id(), self.aliases(), self.requirements())
    }

    fn generate_to_sink(
        &self,
        input: &GeneratorCommonInput<'_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let code_input = build_code_export_input(input, self.0.as_ref())?;
        self.0.generate_to_sink(&code_input, sink)
    }
}

fn build_code_export_input<'meta>(
    input: &GeneratorCommonInput<'meta>,
    generator: &dyn CodeGenerator,
) -> Result<CodeExportInput<'meta>> {
    let task = CodegenTaskConfig::from_export_task_parts(
        input.type_id.clone(),
        input.output.clone(),
        input.options.clone(),
    )?;
    let code_target = CodeExportTarget::from_task(task, generator)?;
    Ok(CodeExportInput {
        meta: input.meta,
        target_tags: input.target_tags.clone(),
        project: CodeExportProject::from(&input.project),
        project_root: input.project_root.clone(),
        target_name: input.target_name.clone(),
        target: code_target,
        runtime_data_files: Vec::new(),
        generated_at: input.generated_at.clone(),
    })
}

pub(super) enum DataArtifactGenerator {
    Table(Box<dyn DataFormatExporter>),
    Aggregate(Box<dyn AggregateDataExporter>),
}

impl DataArtifactGenerator {
    pub(super) fn table(exporter: Box<dyn DataFormatExporter>) -> Self {
        Self::Table(exporter)
    }

    pub(super) fn aggregate(exporter: Box<dyn AggregateDataExporter>) -> Self {
        Self::Aggregate(exporter)
    }
}

impl ArtifactGenerator for DataArtifactGenerator {
    fn type_id(&self) -> &'static str {
        match self {
            Self::Table(exporter) => exporter.name(),
            Self::Aggregate(exporter) => exporter.name(),
        }
    }

    fn aliases(&self) -> &'static [&'static str] {
        match self {
            Self::Table(exporter) => exporter.aliases(),
            Self::Aggregate(exporter) => exporter.aliases(),
        }
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_and_data()
    }

    fn descriptor(&self) -> GeneratorDescriptor {
        GeneratorDescriptor::new(self.type_id(), self.aliases(), self.requirements())
    }

    fn generate_to_sink(
        &self,
        _input: &GeneratorCommonInput<'_>,
        _sink: &ArtifactSink,
    ) -> Result<()> {
        Err(ConfigForgeError::Validation(format!(
            "generator `{}` requires Data-backed artifact input",
            self.type_id()
        )))
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        match self {
            Self::Table(exporter) => input
                .meta
                .tables
                .iter()
                .filter(|table_meta| table_meta.meta.is_exported_for(&input.target_tags))
                .map(|table_meta| {
                    exporter
                        .plan_table_artifact(table_meta)
                        .map(|plan| input.output.join(plan.path))
                })
                .collect(),
            Self::Aggregate(exporter) => {
                let task = input.target_data_config();
                exporter.plan_artifacts(&input.output, &task, input.meta, &input.target_tags)
            }
        }
    }

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        match self {
            Self::Table(exporter) => export_runtime_data_to_sink(
                &input.common.output,
                input.common.meta,
                &input.common.target_tags,
                input.data_set,
                exporter.as_ref(),
                input.pool,
                sink,
            )
            .map(|_| ()),
            Self::Aggregate(exporter) => exporter.export_to_sink(
                &input.common.output,
                &input.common.target_data_config(),
                input.common.meta,
                &input.common.target_tags,
                input.data_set,
                sink,
            ),
        }
    }
}
