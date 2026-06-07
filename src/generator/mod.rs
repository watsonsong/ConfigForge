// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    ArtifactProjectInfo,
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
}
