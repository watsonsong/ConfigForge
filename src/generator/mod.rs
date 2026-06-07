// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Result,
    artifact::ArtifactSink,
    codegen::{
        CodeExportInput, CodeExportProject, CodeExportTarget, CodeGenerator, CodegenTaskConfig,
        builtin_generators,
    },
    data::DataSet,
    exporter::{
        AggregateDataExporter, DataFormatExporter, DataFormatRegistry, TargetDataConfig,
        export_runtime_data_to_sink,
    },
    meta::{LinkedMetaModel, TargetTags},
    runtime_contract::ExportDependency,
    schema_doc::SchemaDocGenerator,
    validation::{DesignerReportFormat, DesignerValidationReport, ValidationReport},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GeneratorDescriptor {
    pub(crate) type_id: &'static str,
    pub(crate) aliases: &'static [&'static str],
    pub(crate) requirements: GeneratorRequirements,
}

impl GeneratorDescriptor {
    pub(crate) const fn new(
        type_id: &'static str,
        aliases: &'static [&'static str],
        requirements: GeneratorRequirements,
    ) -> Self {
        Self {
            type_id,
            aliases,
            requirements,
        }
    }

    pub(crate) fn matches(&self, type_id: &str) -> bool {
        self.type_id == type_id || self.aliases.contains(&type_id)
    }
}

/// Shared, task-local context for artifact generators.
///
/// This is the common subset of information needed by current codegen,
/// schema-doc, and data-backed exporters. Stepwise refactors will adapt this
/// into concrete execution inputs without changing the public Project Config
/// shape.
#[derive(Clone)]
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
    pub(crate) data_set: &'a DataSet,
    pub(crate) validation_report: &'a ValidationReport,
    pub(crate) pool: &'a ThreadPool,
}

pub(crate) struct GeneratorTaskInput<'a, 'meta> {
    pub(crate) common: GeneratorCommonInput<'meta>,
    pub(crate) prepared: PreparedGeneratorInputs<'a>,
}

pub(crate) enum PreparedGeneratorInputs<'a> {
    Planning,
    MetaOnly,
    MetaAndData {
        data_set: &'a DataSet,
        validation_report: &'a ValidationReport,
        pool: &'a ThreadPool,
    },
}

impl<'a> PreparedGeneratorInputs<'a> {
    fn data_set(&self, generator_type: &str) -> Result<&'a DataSet> {
        match self {
            Self::MetaAndData { data_set, .. } => Ok(*data_set),
            Self::Planning | Self::MetaOnly => Err(ConfigForgeError::Validation(format!(
                "generator `{generator_type}` requires Data-backed artifact input"
            ))),
        }
    }

    fn validation_report(&self, generator_type: &str) -> Result<&'a ValidationReport> {
        match self {
            Self::MetaAndData {
                validation_report, ..
            } => Ok(*validation_report),
            Self::Planning | Self::MetaOnly => Err(ConfigForgeError::Validation(format!(
                "generator `{generator_type}` requires validation report input"
            ))),
        }
    }

    fn pool(&self, generator_type: &str) -> Result<&'a ThreadPool> {
        match self {
            Self::MetaAndData { pool, .. } => Ok(*pool),
            Self::Planning | Self::MetaOnly => Err(ConfigForgeError::Validation(format!(
                "generator `{generator_type}` requires Data-backed artifact input"
            ))),
        }
    }
}

pub(crate) trait ArtifactGenerator: Send + Sync {
    fn type_id(&self) -> &'static str;

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn requirements(&self) -> GeneratorRequirements;

    fn descriptor(&self) -> GeneratorDescriptor {
        GeneratorDescriptor::new(self.type_id(), self.aliases(), self.requirements())
    }

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

    fn plan_task_artifacts(&self, input: &GeneratorTaskInput<'_, '_>) -> Result<Vec<PathBuf>> {
        self.plan_artifacts(&input.common)
    }

    fn generate(&self, input: &GeneratorTaskInput<'_, '_>, sink: &ArtifactSink) -> Result<()> {
        if self.requirements().needs_data {
            let data_input = GeneratorDataInput {
                common: input.common.clone(),
                data_set: input.prepared.data_set(self.type_id())?,
                validation_report: input.prepared.validation_report(self.type_id())?,
                pool: input.prepared.pool(self.type_id())?,
            };
            self.generate_with_data_to_sink(&data_input, sink)
        } else {
            self.generate_to_sink(&input.common, sink)
        }
    }
}

pub(crate) struct GeneratorRegistry {
    generators: Vec<Box<dyn ArtifactGenerator>>,
}

impl GeneratorRegistry {
    pub(crate) fn new(
        data_registry: DataFormatRegistry,
        code_generators: Vec<Box<dyn CodeGenerator>>,
        direct_generators: Vec<Box<dyn ArtifactGenerator>>,
    ) -> Self {
        let mut generators = Vec::new();
        let (table_exporters, aggregate_exporters) = data_registry.into_exporters();
        generators.extend(table_exporters.into_iter().map(|exporter| {
            Box::new(DataArtifactGenerator::Table(exporter)) as Box<dyn ArtifactGenerator>
        }));
        generators.extend(aggregate_exporters.into_iter().map(|exporter| {
            Box::new(DataArtifactGenerator::Aggregate(exporter)) as Box<dyn ArtifactGenerator>
        }));
        generators.extend(code_generators.into_iter().map(|generator| {
            Box::new(CodeArtifactGenerator(generator)) as Box<dyn ArtifactGenerator>
        }));
        generators.extend(direct_generators);
        Self { generators }
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

    fn matching_generators(&self, type_id: &str) -> Vec<&dyn ArtifactGenerator> {
        self.generators
            .iter()
            .map(Box::as_ref)
            .filter(|generator| generator.descriptor().matches(type_id))
            .collect()
    }

    pub(crate) fn require_generator(&self, type_id: &str) -> Result<&dyn ArtifactGenerator> {
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

pub(crate) struct CodeArtifactGenerator(Box<dyn CodeGenerator>);

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

enum DataArtifactGenerator {
    Table(Box<dyn DataFormatExporter>),
    Aggregate(Box<dyn AggregateDataExporter>),
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
                &TargetDataConfig {
                    name: Some(input.common.task_name.clone()),
                    output: input.common.output.clone(),
                    format: input.common.type_id.clone(),
                    options: input.common.options.clone(),
                },
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

    fn descriptor(&self) -> GeneratorDescriptor {
        GeneratorDescriptor::new(self.type_id(), self.aliases(), self.requirements())
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

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let config = validation_report_export_config_from_input(&input.common)?;
        let mut report = DesignerValidationReport::from_validation_report(
            input.validation_report,
            crate::validation::DesignerReportOptions {
                title: None,
                project_name: input.common.project.name.clone(),
                project_path: Some(input.common.project_root.clone()),
                target: Some(input.common.target_name.clone()),
                target_tags: input.common.target_tags.clone(),
                generated_at: input.common.generated_at.clone(),
                input_root: None,
            },
        );
        if let Some(title) = config.title {
            report.metadata.title = title;
        }
        let content = crate::validation::render_designer_validation_report(&report, config.format)?;
        sink.write_text(config.output, content)
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        Ok(vec![
            validation_report_export_config_from_input(input)?.output,
        ])
    }
}

struct ValidationReportGeneratorConfig {
    format: DesignerReportFormat,
    output: PathBuf,
    title: Option<String>,
}

fn validation_report_export_config_from_input(
    input: &GeneratorCommonInput<'_>,
) -> Result<ValidationReportGeneratorConfig> {
    let mut options = input.options.clone();
    let format = take_optional_string(&mut options, "format")?
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(DesignerReportFormat::Html);
    let file_name = take_optional_string(&mut options, "file_name")?
        .unwrap_or_else(|| format!("validation-report.{}", format.extension()));
    let title = take_optional_string(&mut options, "title")?;
    let unsupported = options.keys().cloned().collect::<Vec<_>>();
    if let Some(key) = unsupported.first() {
        return Err(ConfigForgeError::Validation(format!(
            "validation_report export task `{}` has unknown option `{key}`",
            input.task_name
        )));
    }
    Ok(ValidationReportGeneratorConfig {
        format,
        output: input.output.join(file_name),
        title,
    })
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

    struct StubGenerator {
        descriptor: GeneratorDescriptor,
    }

    impl ArtifactGenerator for StubGenerator {
        fn type_id(&self) -> &'static str {
            self.descriptor.type_id
        }

        fn aliases(&self) -> &'static [&'static str] {
            self.descriptor.aliases
        }

        fn requirements(&self) -> GeneratorRequirements {
            self.descriptor.requirements
        }

        fn descriptor(&self) -> GeneratorDescriptor {
            self.descriptor
        }

        fn generate_to_sink(
            &self,
            _input: &GeneratorCommonInput<'_>,
            _sink: &ArtifactSink,
        ) -> Result<()> {
            Ok(())
        }
    }

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
    fn descriptor_matches_type_id_and_aliases() {
        let descriptor = GeneratorDescriptor::new(
            "messagepack",
            &["msgpack", "mpack"],
            GeneratorRequirements::meta_and_data(),
        );

        assert!(descriptor.matches("messagepack"));
        assert!(descriptor.matches("msgpack"));
        assert!(descriptor.matches("mpack"));
        assert!(!descriptor.matches("json"));
        assert_eq!(
            descriptor.requirements.dependency(),
            ExportDependency::MetaAndData
        );
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

    #[test]
    fn registry_reports_ambiguous_generator_registration() {
        let registry = GeneratorRegistry::new(
            DataFormatRegistry::new(),
            Vec::new(),
            vec![
                Box::new(StubGenerator {
                    descriptor: GeneratorDescriptor::new(
                        "custom",
                        &[],
                        GeneratorRequirements::meta_only(),
                    ),
                }),
                Box::new(StubGenerator {
                    descriptor: GeneratorDescriptor::new(
                        "other",
                        &["custom"],
                        GeneratorRequirements::meta_only(),
                    ),
                }),
            ],
        );

        assert!(
            matches!(registry.require_generator("custom"), Err(ConfigForgeError::Validation(message)) if message.contains("ambiguous artifact generator registration for `custom`"))
        );
    }
}
