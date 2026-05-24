use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rayon::ThreadPool;

use crate::{
    ConfigForgeError, Result,
    artifact::Artifact,
    data::{DataLoadPlan, DataLoader},
    exporter::{DataFormatRegistry, RuntimeDataFile, export_runtime_data_artifacts},
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
    )?;
    let mut normalized_data_sets: BTreeMap<PathBuf, crate::data::DataSet> = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut results = Vec::new();
    let mut artifacts = Vec::new();

    for data_task in data_tasks {
        let data_exporter = registry.require(&data_task.format)?;
        if !normalized_data_sets.contains_key(&data_task.input) {
            let loaded_data_set =
                DataLoader::new(&data_task.input).load_with_pool(&data_plan, pool)?;
            let data_set = normalize_data_set(linked_meta, &loaded_data_set)?;
            let validation_report = validate_normalized_data_set_for_target_with_report(
                linked_meta,
                target_tags,
                &data_set,
            )?;
            warnings.extend(validation_report.warning_messages());
            normalized_data_sets.insert(data_task.input.clone(), data_set);
        }
        let data_set = normalized_data_sets
            .get(&data_task.input)
            .expect("data set should be loaded before export");
        let runtime_artifacts = export_runtime_data_artifacts(
            &data_task.output,
            linked_meta,
            target_tags,
            data_set,
            data_exporter,
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
        let exporter = registry.require(&data_task.format)?;
        for table_meta in &linked_meta.tables {
            if !table_meta.meta.is_exported_for(target_tags) {
                continue;
            }
            let plan = exporter.plan_table_artifact(table_meta)?;
            let path = lexical_normalize_path(&data_task.output.join(&plan.path));
            if !paths.insert(path.clone()) {
                return Err(ConfigForgeError::Validation(format!(
                    "multiple runtime data artifacts target `{}`",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}
