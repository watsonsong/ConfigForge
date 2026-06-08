// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use rayon::ThreadPool;

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Result,
    code_artifact::{
        CodeExportInput, CodeExportProject, CodeExportTarget, CodeGenerator, CodegenTaskConfig,
    },
    data::DataSet,
    data_artifact::TargetDataConfig,
    meta::{LinkedMetaModel, TargetTags},
    validation::ValidationReport,
};

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

impl GeneratorCommonInput<'_> {
    pub(crate) fn target_data_config(&self) -> TargetDataConfig {
        TargetDataConfig {
            name: Some(self.task_name.clone()),
            format: self.type_id.clone(),
            output: self.output.clone(),
            options: self.options.clone(),
        }
    }
}

impl<'meta> GeneratorCommonInput<'meta> {
    pub(crate) fn code_export_input(
        &self,
        generator: &dyn CodeGenerator,
    ) -> Result<CodeExportInput<'meta>> {
        let task = CodegenTaskConfig::from_export_task_parts(
            self.type_id.clone(),
            self.output.clone(),
            self.options.clone(),
        )?;
        let code_target = CodeExportTarget::from_task(task, generator)?;
        Ok(CodeExportInput {
            meta: self.meta,
            target_tags: self.target_tags.clone(),
            project: CodeExportProject::from(&self.project),
            project_root: self.project_root.clone(),
            target_name: self.target_name.clone(),
            target: code_target,
            runtime_data_files: Vec::new(),
            generated_at: self.generated_at.clone(),
        })
    }
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
    pub(crate) fn data_set(&self, generator_type: &str) -> Result<&'a DataSet> {
        match self {
            Self::MetaAndData { data_set, .. } => Ok(*data_set),
            Self::Planning | Self::MetaOnly => Err(ConfigForgeError::Validation(format!(
                "generator `{generator_type}` requires Data-backed artifact input"
            ))),
        }
    }

    pub(crate) fn validation_report(&self, generator_type: &str) -> Result<&'a ValidationReport> {
        match self {
            Self::MetaAndData {
                validation_report, ..
            } => Ok(*validation_report),
            Self::Planning | Self::MetaOnly => Err(ConfigForgeError::Validation(format!(
                "generator `{generator_type}` requires validation report input"
            ))),
        }
    }

    pub(crate) fn pool(&self, generator_type: &str) -> Result<&'a ThreadPool> {
        match self {
            Self::MetaAndData { pool, .. } => Ok(*pool),
            Self::Planning | Self::MetaOnly => Err(ConfigForgeError::Validation(format!(
                "generator `{generator_type}` requires Data-backed artifact input"
            ))),
        }
    }
}
