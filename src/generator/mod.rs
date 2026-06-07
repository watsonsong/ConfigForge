// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Result,
    codegen::{CodeGenerator, builtin_generators, find_generator},
    exporter::DataFormatRegistry,
    meta::{LinkedMetaModel, TargetTags},
    runtime_contract::ExportDependency,
};

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

pub(crate) struct GeneratorRegistry {
    data_registry: DataFormatRegistry,
    code_generators: Vec<Box<dyn CodeGenerator>>,
    direct_generators: Vec<DirectGeneratorRegistration>,
}

impl GeneratorRegistry {
    pub(crate) fn with_builtin_generators() -> Self {
        Self {
            data_registry: DataFormatRegistry::with_builtin_exporters(),
            code_generators: builtin_generators(),
            direct_generators: vec![DirectGeneratorRegistration::new(
                "validation_report",
                &[],
                GeneratorRequirements::meta_and_data().with_validation_report(),
            )],
        }
    }

    pub(crate) fn dependency(&self, target_name: &str, type_id: &str) -> Result<ExportDependency> {
        let mut matches = Vec::new();
        if let Some(dependency) = self.data_registry.dependency(type_id) {
            matches.push(dependency);
        }
        if let Some(dependency) =
            find_generator(&self.code_generators, type_id).map(|generator| generator.dependency())
        {
            matches.push(dependency);
        }
        if let Some(dependency) = self
            .direct_generators
            .iter()
            .find(|generator| generator.matches(type_id))
            .map(DirectGeneratorRegistration::dependency)
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

struct DirectGeneratorRegistration {
    type_id: &'static str,
    aliases: &'static [&'static str],
    requirements: GeneratorRequirements,
}

impl DirectGeneratorRegistration {
    const fn new(
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

    fn matches(&self, type_id: &str) -> bool {
        self.type_id == type_id || self.aliases.contains(&type_id)
    }

    fn dependency(&self) -> ExportDependency {
        self.requirements.dependency()
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
