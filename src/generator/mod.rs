// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Result,
    artifact::ArtifactSink,
    codegen::{CodeGenerator, builtin_generators, find_generator},
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

    pub(crate) fn code_generator(&self, type_id: &str) -> Option<&dyn CodeGenerator> {
        find_generator(&self.code_generators, type_id)
    }

    pub(crate) fn data_generator(&self, type_id: &str) -> Option<DataArtifactGeneratorRef<'_>> {
        self.data_registry
            .require_any(type_id)
            .ok()
            .map(DataArtifactGeneratorRef)
    }

    pub(crate) fn require_data_generator(
        &self,
        type_id: &str,
    ) -> Result<DataArtifactGeneratorRef<'_>> {
        self.data_generator(type_id).ok_or_else(|| {
            ConfigForgeError::Validation(format!("no data exporter registered for `{type_id}`"))
        })
    }

    pub(crate) fn direct_generator(&self, type_id: &str) -> Option<&dyn ArtifactGenerator> {
        self.direct_generators
            .iter()
            .find(|generator| generator.matches(type_id))
            .map(Box::as_ref)
    }

    pub(crate) fn dependency(&self, target_name: &str, type_id: &str) -> Result<ExportDependency> {
        let mut matches = Vec::new();
        if let Some(generator) = self.data_generator(type_id) {
            matches.push(generator.requirements().dependency());
        }
        if let Some(dependency) =
            find_generator(&self.code_generators, type_id).map(|generator| generator.dependency())
        {
            matches.push(dependency);
        }
        if let Some(dependency) = self
            .direct_generator(type_id)
            .map(|generator| generator.requirements().dependency())
        {
            matches.push(dependency);
        }

        let Some(first) = matches.first().copied() else {
            return Err(ConfigForgeError::Validation(format!(
                "target `{target_name}` declares unknown export type `{type_id}`"
            )));
        };
        if matches.iter().all(|dependency| *dependency == first) {
            Ok(first)
        } else {
            Err(ConfigForgeError::Validation(format!(
                "target `{target_name}` declares ambiguous export type `{type_id}`; multiple generator contracts with different dependencies are registered"
            )))
        }
    }
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
