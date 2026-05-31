// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::path::PathBuf;

use rayon::ThreadPool;

use crate::{
    ConfigForgeError, Result,
    artifact::Artifact,
    data::{DataLoadPlan, DataLoader},
    exporter::{
        DataExporterRef, DataFormatRegistry, RuntimeDataFile, TargetDataConfig, export_runtime_data,
    },
    meta::{LinkedMetaModel, TargetTags},
    project::lexical_normalize_path,
    validation::{normalize_data_set, validate_normalized_data_set_for_target_with_report},
};

use super::target::NormalizedTargetDataConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeConfigForgeResult {
    pub(super) data_name: String,
    pub(super) files: Vec<RuntimeDataFile>,
}

pub(super) struct RuntimeExportOutput {
    pub(super) warnings: Vec<String>,
    pub(super) results: Vec<RuntimeConfigForgeResult>,
    pub(super) artifacts: Vec<Artifact>,
}

pub(super) fn export_runtime_data_tasks(
    data_tasks: &[NormalizedTargetDataConfig],
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    pool: &ThreadPool,
) -> Result<RuntimeExportOutput> {
    if data_tasks.is_empty() {
        return Ok(RuntimeExportOutput {
            warnings: Vec::new(),
            results: Vec::new(),
            artifacts: Vec::new(),
        });
    }

    let registry = DataFormatRegistry::with_builtin_exporters();
    ensure_planned_runtime_output_paths(data_tasks, linked_meta, target_tags, &registry)?;

    let data_plan = DataLoadPlan::from_tables(
        linked_meta
            .tables
            .iter()
            .filter(|table| table.meta.is_exported_for(target_tags))
            .map(|table| table.meta),
        Some(target_tags),
    )?;
    let mut normalized_data_sets: BTreeMap<PathBuf, crate::data::DataSet> = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut results = Vec::new();
    let mut artifacts = Vec::new();

    for data_task in data_tasks {
        let format_exporter = registry.require_any(&data_task.format)?;
        let data_set = match normalized_data_sets.entry(data_task.input.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let loaded_data_set =
                    DataLoader::new(&data_task.input).load_with_pool(&data_plan, pool)?;
                let data_set = normalize_data_set(linked_meta, &loaded_data_set)?;
                let validation_report = validate_normalized_data_set_for_target_with_report(
                    linked_meta,
                    target_tags,
                    &data_set,
                )?;
                warnings.extend(validation_report.warning_messages());
                entry.insert(data_set)
            }
        };
        match format_exporter {
            DataExporterRef::Table(exporter) => {
                let runtime_artifacts = export_runtime_data(
                    &data_task.output,
                    linked_meta,
                    target_tags,
                    data_set,
                    exporter,
                    pool,
                )?;
                let files = runtime_artifacts
                    .iter()
                    .map(|artifact| artifact.file.clone())
                    .collect::<Vec<_>>();
                artifacts.extend(
                    runtime_artifacts
                        .into_iter()
                        .map(|artifact| artifact.artifact),
                );
                results.push(RuntimeConfigForgeResult {
                    data_name: data_task.name.clone(),
                    files,
                });
            }
            DataExporterRef::Aggregate(exporter) => {
                artifacts.extend(exporter.export(
                    &data_task.output,
                    &target_data_config_from_normalized(data_task),
                    linked_meta,
                    target_tags,
                    data_set,
                )?);
            }
        }
    }

    Ok(RuntimeExportOutput {
        warnings,
        results,
        artifacts,
    })
}

fn ensure_planned_runtime_output_paths(
    data_tasks: &[NormalizedTargetDataConfig],
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    registry: &DataFormatRegistry,
) -> Result<()> {
    let mut paths = BTreeSet::new();
    for data_task in data_tasks {
        match registry.require_any(&data_task.format)? {
            DataExporterRef::Table(exporter) => {
                for table_meta in &linked_meta.tables {
                    if !table_meta.meta.is_exported_for(target_tags) {
                        continue;
                    }
                    let plan = exporter.plan_table_artifact(table_meta)?;
                    let path = lexical_normalize_path(&data_task.output.join(&plan.path));
                    if !paths.insert(path.clone()) {
                        return Err(ConfigForgeError::Validation(format!(
                            "multiple data artifacts target `{}`",
                            path.display()
                        )));
                    }
                }
            }
            DataExporterRef::Aggregate(exporter) => {
                let task = target_data_config_from_normalized(data_task);
                for path in
                    exporter.plan_artifacts(&data_task.output, &task, linked_meta, target_tags)?
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

fn target_data_config_from_normalized(data_task: &NormalizedTargetDataConfig) -> TargetDataConfig {
    TargetDataConfig {
        name: Some(data_task.name.clone()),
        input: data_task.input.clone(),
        output: data_task.output.clone(),
        format: data_task.format.clone(),
        options: data_task.options.clone(),
    }
}
