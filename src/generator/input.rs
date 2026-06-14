// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use rayon::ThreadPool;

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity,
    Result,
    code_artifact::{
        CodeExportInput, CodeExportProject, CodeExportTarget, CodeGenerator, CodegenTaskConfig,
    },
    data::DataSet,
    data_artifact::TargetDataConfig,
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
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

pub(crate) struct GeneratorValidationReportInput<'a, 'meta> {
    pub(crate) common: GeneratorCommonInput<'meta>,
    pub(crate) validation_report: &'a ValidationReport,
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
    pub(crate) fn from_task_context(
        context: GeneratorCommonInputContext<'_, 'meta>,
        task_name: &str,
        type_id: &str,
        output: &std::path::Path,
        options: &BTreeMap<String, toml::Value>,
    ) -> Self {
        Self {
            meta: context.linked_meta,
            target_tags: context.target_tags.clone(),
            project: context.project_info.clone(),
            project_root: context.project_root.to_path_buf(),
            target_name: context.target_name.to_owned(),
            task_name: task_name.to_owned(),
            type_id: type_id.to_owned(),
            output: context.project_root.join(output),
            options: options.clone(),
            generated_at: context.generated_at.to_owned(),
        }
    }

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

#[derive(Clone, Copy)]
pub(crate) struct GeneratorCommonInputContext<'a, 'meta> {
    pub(crate) linked_meta: &'meta LinkedMetaModel<'meta>,
    pub(crate) target_tags: &'a TargetTags,
    pub(crate) project_root: &'a std::path::Path,
    pub(crate) project_info: &'a ArtifactProjectInfo,
    pub(crate) target_name: &'a str,
    pub(crate) generated_at: &'a str,
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
    ValidationReportOnly {
        validation_report: &'a ValidationReport,
    },
}

impl<'a> PreparedGeneratorInputs<'a> {
    pub(crate) fn data_set(&self, generator_type: &str) -> Result<&'a DataSet> {
        match self {
            Self::MetaAndData { data_set, .. } => Ok(*data_set),
            Self::Planning | Self::MetaOnly | Self::ValidationReportOnly { .. } => {
                Err(unsupported_data_input_error(
                    generator_type,
                    self.kind_label(),
                    "Data-backed artifact input",
                ))
            }
        }
    }

    pub(crate) fn validation_report(&self, generator_type: &str) -> Result<&'a ValidationReport> {
        match self {
            Self::MetaAndData {
                validation_report, ..
            } => Ok(*validation_report),
            Self::ValidationReportOnly { validation_report } => Ok(*validation_report),
            Self::Planning | Self::MetaOnly => Err(unsupported_data_input_error(
                generator_type,
                self.kind_label(),
                "validation report input",
            )),
        }
    }

    pub(crate) fn pool(&self, generator_type: &str) -> Result<&'a ThreadPool> {
        match self {
            Self::MetaAndData { pool, .. } => Ok(*pool),
            Self::Planning | Self::MetaOnly | Self::ValidationReportOnly { .. } => {
                Err(unsupported_data_input_error(
                    generator_type,
                    self.kind_label(),
                    "Data-backed artifact input",
                ))
            }
        }
    }

    pub(crate) fn is_validation_report_only(&self) -> bool {
        matches!(self, Self::ValidationReportOnly { .. })
    }

    fn kind_label(&self) -> &'static str {
        match self {
            Self::Planning => "Planning",
            Self::MetaOnly => "MetaOnly",
            Self::MetaAndData { .. } => "MetaAndData",
            Self::ValidationReportOnly { .. } => "ValidationReportOnly",
        }
    }
}

fn unsupported_data_input_error(
    generator_type: &str,
    current_input: &str,
    expected_input: &str,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
            DiagnosticSeverity::Error,
            format!("generator `{generator_type}` requires {expected_input}"),
            DiagnosticLocation::default(),
        )
        .with_source("generator.input")
        .with_current_value(current_input)
        .with_expected(expected_input),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_inputs_report_structured_data_requirement_errors() {
        let error = PreparedGeneratorInputs::MetaOnly
            .data_set("json")
            .unwrap_err();
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected structured diagnostic report");
        };
        assert_eq!(
            report.diagnostics[0].code,
            CF_GENERATOR_UNSUPPORTED_DATA_INPUT
        );
        assert_eq!(
            report.diagnostics[0].source.as_deref(),
            Some("generator.input")
        );
        assert_eq!(
            report.diagnostics[0].current_value.as_deref(),
            Some("MetaOnly")
        );
    }
}
