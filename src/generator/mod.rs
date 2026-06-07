// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Result,
    artifact::ArtifactSink,
    codegen::{
        CodeExportInput, CodeExportProject, CodeExportTarget, CodeGenerator, CodegenTaskConfig,
        builtin_generators, find_generator,
    },
    data::DataSet,
    exporter::{
        DataExporterRef, DataFormatRegistry, TargetDataConfig, export_runtime_data_to_sink,
    },
    meta::{LinkedMetaModel, TargetTags},
    runtime_contract::ExportDependency,
    schema_doc::SchemaDocGenerator,
};

use rayon::ThreadPool;

/// Input requirements declared by a build-facing artifact generator.
///
/// Requirements describe which shared build inputs must be prepared before a
/// generator can run. They are scheduling metadata, not a trait inheritance
/// hierarchy: a data-backed generator may still produce code-like artifacts,
/// and a Meta-only generator may produce docs or manifests rather than code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GeneratorRequirements {
    pub(crate) needs_meta: bool,
    pub(crate) needs_data: bool,
    pub(crate) needs_assets: bool,
    pub(crate) needs_validation_report: bool,
}

impl GeneratorRequirements {
    pub(crate) const fn meta_only() -> Self {
        Self {
            needs_meta: true,
            needs_data: false,
            needs_assets: false,
            needs_validation_report: false,
        }
    }

    pub(crate) const fn meta_and_data() -> Self {
        Self {
            needs_meta: true,
            needs_data: true,
            needs_assets: false,
            needs_validation_report: false,
        }
    }

    pub(crate) const fn with_assets(mut self) -> Self {
        self.needs_assets = true;
        self
    }

    pub(crate) const fn with_validation_report(mut self) -> Self {
        self.needs_validation_report = true;
        self
    }

    pub(crate) const fn dependency(self) -> ExportDependency {
        if self.needs_data {
            ExportDependency::MetaAndData
        } else {
            ExportDependency::MetaOnly
        }
    }
}

impl From<GeneratorRequirements> for ExportDependency {
    fn from(requirements: GeneratorRequirements) -> Self {
        requirements.dependency()
    }
}

/// Shared, task-local context for artifact generators.
///
/// This is the common subset of information needed by current codegen,
/// schema-doc, and data-backed exporters. Stepwise refactors will adapt this
/// into concrete execution inputs without changing the public Project Config
/// shape.
pub(crate) struct GeneratorCommonInput<'meta> {
    pub(crate) meta: &'meta LinkedMetaModel<'meta>,
    pub(crate) target_tags: TargetTags,
    pub(crate) project: ArtifactProjectInfo,
    pub(crate) project_root: PathBuf,
    pub(crate) target_name: String,
    pub(crate) task_name: String,
    pub(crate) type_id: String,
    pub(crate) output: PathBuf,
    pub(crate) options: BTreeMap<String, toml::Value>,
    pub(crate) generated_at: String,
}

pub(crate) struct GeneratorDataInput<'a, 'meta> {
    pub(crate) common: GeneratorCommonInput<'meta>,
    pub(crate) target_data_config: TargetDataConfig,
    pub(crate) data_set: &'a DataSet,
    pub(crate) pool: &'a ThreadPool,
}

pub(crate) trait ArtifactGenerator: Send + Sync {
    fn type_id(&self) -> &'static str;

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn requirements(&self) -> GeneratorRequirements;

    fn generate_to_sink(&self, input: &GeneratorCommonInput<'_>, sink: &ArtifactSink)
    -> Result<()>;

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        let _ = input;
        Ok(Vec::new())
    }

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let _ = (input, sink);
        Err(ConfigForgeError::Validation(format!(
            "generator `{}` does not support Data-backed artifact generation",
            self.type_id()
        )))
    }

    fn matches(&self, type_id: &str) -> bool {
        self.type_id() == type_id || self.aliases().contains(&type_id)
    }
}

pub(crate) struct GeneratorRegistry {
    data_registry: DataFormatRegistry,
    code_generators: Vec<Box<dyn CodeGenerator>>,
    direct_generators: Vec<Box<dyn ArtifactGenerator>>,
}

impl GeneratorRegistry {
    pub(crate) fn new(
        data_registry: DataFormatRegistry,
        code_generators: Vec<Box<dyn CodeGenerator>>,
        direct_generators: Vec<Box<dyn ArtifactGenerator>>,
    ) -> Self {
        Self {
            data_registry,
            code_generators,
            direct_generators,
        }
    }

    pub(crate) fn with_builtin_generators() -> Self {
        Self::new(
            DataFormatRegistry::with_builtin_exporters(),
            builtin_generators(),
            vec![
                Box::new(SchemaDocGenerator),
                Box::new(ValidationReportGenerator),
            ],
        )
    }

    fn data_generator(&self, type_id: &str) -> Option<DataArtifactGeneratorRef<'_>> {
        self.data_registry
            .require_any(type_id)
            .ok()
            .map(DataArtifactGeneratorRef)
    }

    fn code_generator(&self, type_id: &str) -> Option<CodeArtifactGeneratorRef<'_>> {
        find_generator(&self.code_generators, type_id).map(CodeArtifactGeneratorRef)
    }

    fn direct_generator(&self, type_id: &str) -> Option<&dyn ArtifactGenerator> {
        self.direct_generators
            .iter()
            .find(|generator| generator.matches(type_id))
            .map(Box::as_ref)
    }

    fn matching_generators(&self, type_id: &str) -> Vec<ArtifactGeneratorRef<'_>> {
        let mut generators = Vec::new();
        if let Some(generator) = self.direct_generator(type_id) {
            generators.push(ArtifactGeneratorRef::Direct(generator));
        }
        if let Some(generator) = self.code_generator(type_id) {
            generators.push(ArtifactGeneratorRef::Code(generator));
        }
        if let Some(generator) = self.data_generator(type_id) {
            generators.push(ArtifactGeneratorRef::Data(generator));
        }
        generators
    }

    pub(crate) fn require_generator(&self, type_id: &str) -> Result<ArtifactGeneratorRef<'_>> {
        let mut generators = self.matching_generators(type_id).into_iter();
        let Some(generator) = generators.next() else {
            return Err(ConfigForgeError::Validation(format!(
                "no artifact generator registered for `{type_id}`"
            )));
        };
        if generators.next().is_some() {
            return Err(ConfigForgeError::Validation(format!(
                "ambiguous artifact generator registration for `{type_id}`"
            )));
        }
        Ok(generator)
    }

    pub(crate) fn dependency(&self, target_name: &str, type_id: &str) -> Result<ExportDependency> {
        match self.require_generator(type_id) {
            Ok(generator) => Ok(generator.requirements().dependency()),
            Err(ConfigForgeError::Validation(message))
                if message.contains("no artifact generator registered") =>
            {
                Err(ConfigForgeError::Validation(format!(
                    "target `{target_name}` declares unknown export type `{type_id}`"
                )))
            }
            Err(error) => Err(error),
        }
    }
}

pub(crate) enum ArtifactGeneratorRef<'a> {
    Direct(&'a dyn ArtifactGenerator),
    Code(CodeArtifactGeneratorRef<'a>),
    Data(DataArtifactGeneratorRef<'a>),
}

impl ArtifactGenerator for ArtifactGeneratorRef<'_> {
    fn type_id(&self) -> &'static str {
        match self {
            Self::Direct(generator) => generator.type_id(),
            Self::Code(generator) => generator.type_id(),
            Self::Data(generator) => generator.type_id(),
        }
    }

    fn aliases(&self) -> &'static [&'static str] {
        match self {
            Self::Direct(generator) => generator.aliases(),
            Self::Code(generator) => generator.aliases(),
            Self::Data(generator) => generator.aliases(),
        }
    }

    fn requirements(&self) -> GeneratorRequirements {
        match self {
            Self::Direct(generator) => generator.requirements(),
            Self::Code(generator) => generator.requirements(),
            Self::Data(generator) => generator.requirements(),
        }
    }

    fn generate_to_sink(
        &self,
        input: &GeneratorCommonInput<'_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        match self {
            Self::Direct(generator) => generator.generate_to_sink(input, sink),
            Self::Code(generator) => generator.generate_to_sink(input, sink),
            Self::Data(generator) => generator.generate_to_sink(input, sink),
        }
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        match self {
            Self::Direct(generator) => generator.plan_artifacts(input),
            Self::Code(generator) => generator.plan_artifacts(input),
            Self::Data(generator) => generator.plan_artifacts(input),
        }
    }

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        match self {
            Self::Direct(generator) => generator.generate_with_data_to_sink(input, sink),
            Self::Code(generator) => generator.generate_with_data_to_sink(input, sink),
            Self::Data(generator) => generator.generate_with_data_to_sink(input, sink),
        }
    }
}

pub(crate) struct CodeArtifactGeneratorRef<'a>(&'a dyn CodeGenerator);

impl ArtifactGenerator for CodeArtifactGeneratorRef<'_> {
    fn type_id(&self) -> &'static str {
        self.0.language()
    }

    fn aliases(&self) -> &'static [&'static str] {
        self.0.language_aliases()
    }

    fn requirements(&self) -> GeneratorRequirements {
        match self.0.dependency() {
            ExportDependency::MetaOnly => GeneratorRequirements::meta_only(),
            ExportDependency::MetaAndData => GeneratorRequirements::meta_and_data(),
        }
    }

    fn generate_to_sink(
        &self,
        input: &GeneratorCommonInput<'_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let code_input = build_code_export_input(input, self.0)?;
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

pub(crate) struct DataArtifactGeneratorRef<'a>(DataExporterRef<'a>);

impl ArtifactGenerator for DataArtifactGeneratorRef<'_> {
    fn type_id(&self) -> &'static str {
        match self.0 {
            DataExporterRef::Table(exporter) => exporter.name(),
            DataExporterRef::Aggregate(exporter) => exporter.name(),
        }
    }

    fn aliases(&self) -> &'static [&'static str] {
        match self.0 {
            DataExporterRef::Table(exporter) => exporter.aliases(),
            DataExporterRef::Aggregate(exporter) => exporter.aliases(),
        }
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
            self.type_id()
        )))
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        match self.0 {
            DataExporterRef::Table(exporter) => input
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
            DataExporterRef::Aggregate(exporter) => {
                let task = TargetDataConfig {
                    name: Some(input.task_name.clone()),
                    format: input.type_id.clone(),
                    output: input.output.clone(),
                    options: input.options.clone(),
                };
                exporter.plan_artifacts(&input.output, &task, input.meta, &input.target_tags)
            }
        }
    }

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        match self.0 {
            DataExporterRef::Table(exporter) => export_runtime_data_to_sink(
                &input.common.output,
                input.common.meta,
                &input.common.target_tags,
                input.data_set,
                exporter,
                input.pool,
                sink,
            )
            .map(|_| ()),
            DataExporterRef::Aggregate(exporter) => exporter.export_to_sink(
                &input.common.output,
                &input.target_data_config,
                input.common.meta,
                &input.common.target_tags,
                input.data_set,
                sink,
            ),
        }
    }
}

struct ValidationReportGenerator;

impl ArtifactGenerator for ValidationReportGenerator {
    fn type_id(&self) -> &'static str {
        "validation_report"
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_and_data().with_validation_report()
    }

    fn generate_to_sink(
        &self,
        _input: &GeneratorCommonInput<'_>,
        _sink: &ArtifactSink,
    ) -> Result<()> {
        Err(ConfigForgeError::Validation(
            "validation_report is generated by the data-backed validation report pipeline"
                .to_owned(),
        ))
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        let mut options = input.options.clone();
        let format = take_optional_string(&mut options, "format")?
            .map(|value| value.parse())
            .transpose()?
            .unwrap_or(crate::validation::DesignerReportFormat::Html);
        let file_name = take_optional_string(&mut options, "file_name")?
            .unwrap_or_else(|| format!("validation-report.{}", format.extension()));
        let _title = take_optional_string(&mut options, "title")?;
        let unsupported = options.keys().cloned().collect::<Vec<_>>();
        if let Some(key) = unsupported.first() {
            return Err(ConfigForgeError::Validation(format!(
                "validation_report export task `{}` has unknown option `{key}`",
                input.task_name
            )));
        }
        Ok(vec![input.output.join(file_name)])
    }
}

fn take_optional_string(
    options: &mut BTreeMap<String, toml::Value>,
    key: &str,
) -> Result<Option<String>> {
    options
        .remove(key)
        .map(|value| {
            value.try_into().map_err(|source| {
                ConfigForgeError::Validation(format!(
                    "invalid validation_report option `{key}`: {source}"
                ))
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_without_data_are_meta_only() {
        let requirements = GeneratorRequirements::meta_only();

        assert!(requirements.needs_meta);
        assert!(!requirements.needs_data);
        assert_eq!(requirements.dependency(), ExportDependency::MetaOnly);
        assert_eq!(
            ExportDependency::from(requirements),
            ExportDependency::MetaOnly
        );
    }

    #[test]
    fn requirements_with_data_are_meta_and_data() {
        let requirements = GeneratorRequirements::meta_and_data();

        assert!(requirements.needs_meta);
        assert!(requirements.needs_data);
        assert_eq!(requirements.dependency(), ExportDependency::MetaAndData);
        assert_eq!(
            ExportDependency::from(requirements),
            ExportDependency::MetaAndData
        );
    }

    #[test]
    fn auxiliary_requirements_do_not_force_data_dependency() {
        let meta_with_assets = GeneratorRequirements::meta_only().with_assets();
        let meta_with_report = GeneratorRequirements::meta_only().with_validation_report();

        assert_eq!(meta_with_assets.dependency(), ExportDependency::MetaOnly);
        assert!(meta_with_assets.needs_assets);
        assert_eq!(meta_with_report.dependency(), ExportDependency::MetaOnly);
        assert!(meta_with_report.needs_validation_report);
    }

    #[test]
    fn registry_classifies_known_generator_types() {
        let registry = GeneratorRegistry::with_builtin_generators();

        assert_eq!(
            registry
                .dependency("client", "rust")
                .expect("rust dependency"),
            ExportDependency::MetaOnly
        );
        assert_eq!(
            registry.dependency("client", "ts").expect("ts dependency"),
            ExportDependency::MetaOnly
        );
        assert_eq!(
            registry
                .dependency("client", "schema_doc")
                .expect("schema_doc dependency"),
            ExportDependency::MetaOnly
        );
        assert_eq!(
            registry
                .dependency("client", "json")
                .expect("json dependency"),
            ExportDependency::MetaAndData
        );
        assert_eq!(
            registry
                .dependency("client", "lua")
                .expect("lua dependency"),
            ExportDependency::MetaAndData
        );
        assert_eq!(
            registry
                .dependency("client", "validation_report")
                .expect("validation_report dependency"),
            ExportDependency::MetaAndData
        );
    }

    #[test]
    fn registry_reports_unknown_generator_type() {
        let registry = GeneratorRegistry::with_builtin_generators();

        assert!(
            matches!(registry.dependency("client", "unknown"), Err(ConfigForgeError::Validation(message)) if message.contains("unknown export type `unknown`"))
        );
    }
}
