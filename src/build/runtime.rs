// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use rayon::ThreadPool;
use std::collections::BTreeSet;

use crate::{
    ConfigForgeError, Result,
    artifact::Artifact,
    data::RawSourceData,
    exporter::{DataExporterRef, DataFormatRegistry, export_runtime_data},
    meta::{LinkedMetaModel, TargetTags},
    project::lexical_normalize_path,
    runtime_contract::RuntimeDataFile,
    validation::{normalize_data_set, validate_normalized_data_set_for_target_with_report},
};

use super::target::{NormalizedExportTask, target_data_config_from_export_task};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeConfigForgeResult {
    pub(super) export_name: String,
    pub(super) files: Vec<RuntimeDataFile>,
}

pub(super) struct RuntimeExportOutput {
    pub(super) warnings: Vec<String>,
    pub(super) task_outputs: Vec<RuntimeExportTaskOutput>,
}

pub(super) struct RuntimeExportTaskOutput {
    pub(super) export_name: String,
    pub(super) result: Option<RuntimeConfigForgeResult>,
    pub(super) artifacts: Vec<Artifact>,
}

pub(super) fn export_meta_and_data_tasks(
    raw_sources: Option<&RawSourceData>,
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    pool: &ThreadPool,
) -> Result<RuntimeExportOutput> {
    if export_tasks.is_empty() {
        return Ok(RuntimeExportOutput {
            warnings: Vec::new(),
            task_outputs: Vec::new(),
        });
    }
    let raw_sources = raw_sources.ok_or_else(|| {
        ConfigForgeError::Validation(
            "raw source data must be prepared when target declares Data-dependent export tasks"
                .to_owned(),
        )
    })?;

    let registry = DataFormatRegistry::with_builtin_exporters();
    ensure_planned_runtime_output_paths(export_tasks, linked_meta, target_tags, &registry)?;

    let data_set = normalize_data_set(linked_meta, raw_sources.as_data_set())?;
    let validation_report =
        validate_normalized_data_set_for_target_with_report(linked_meta, target_tags, &data_set)?;
    let mut warnings = Vec::new();
    warnings.extend(validation_report.warning_messages());
    let mut task_outputs = Vec::new();

    for export_task in export_tasks {
        let format_exporter = registry.require_any(&export_task.type_id)?;
        match format_exporter {
            DataExporterRef::Table(exporter) => {
                let runtime_artifacts = export_runtime_data(
                    &export_task.output,
                    linked_meta,
                    target_tags,
                    &data_set,
                    exporter,
                    pool,
                )?;
                let files = runtime_artifacts
                    .iter()
                    .map(|artifact| artifact.file.clone())
                    .collect::<Vec<_>>();
                task_outputs.push(RuntimeExportTaskOutput {
                    export_name: export_task.name.clone(),
                    result: Some(RuntimeConfigForgeResult {
                        export_name: export_task.name.clone(),
                        files,
                    }),
                    artifacts: runtime_artifacts
                        .into_iter()
                        .map(|artifact| artifact.artifact)
                        .collect(),
                });
            }
            DataExporterRef::Aggregate(exporter) => {
                task_outputs.push(RuntimeExportTaskOutput {
                    export_name: export_task.name.clone(),
                    result: None,
                    artifacts: exporter.export(
                        &export_task.output,
                        &target_data_config_from_export_task(export_task),
                        linked_meta,
                        target_tags,
                        &data_set,
                    )?,
                });
            }
        }
    }

    Ok(RuntimeExportOutput {
        warnings,
        task_outputs,
    })
}

fn ensure_planned_runtime_output_paths(
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    registry: &DataFormatRegistry,
) -> Result<()> {
    let mut paths = BTreeSet::new();
    for export_task in export_tasks {
        match registry.require_any(&export_task.type_id)? {
            DataExporterRef::Table(exporter) => {
                for table_meta in &linked_meta.tables {
                    if !table_meta.meta.is_exported_for(target_tags) {
                        continue;
                    }
                    let plan = exporter.plan_table_artifact(table_meta)?;
                    let path = lexical_normalize_path(&export_task.output.join(&plan.path));
                    if !paths.insert(path.clone()) {
                        return Err(ConfigForgeError::Validation(format!(
                            "multiple data artifacts target `{}`",
                            path.display()
                        )));
                    }
                }
            }
            DataExporterRef::Aggregate(exporter) => {
                let task = target_data_config_from_export_task(export_task);
                for path in
                    exporter.plan_artifacts(&export_task.output, &task, linked_meta, target_tags)?
                {
                    let path = lexical_normalize_path(&path);
                    if !paths.insert(path.clone()) {
                        return Err(ConfigForgeError::Validation(format!(
                            "multiple data artifacts target `{}`",
                            path.display()
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}
