// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use rayon::{ThreadPool, prelude::*};
use std::collections::BTreeSet;

use crate::{
    ConfigForgeError, Result,
    artifact::Artifact,
    data::RawSourceData,
    exporter::{DataExporterRef, DataFormatRegistry, export_runtime_data},
    meta::{LinkedMetaModel, TargetTags},
    project::lexical_normalize_path,
    runtime_contract::RuntimeDataFile,
    validation::{
        normalize_data_set_with_pool, validate_normalized_data_set_for_target_with_report_and_pool,
    },
};

use super::target::{NormalizedExportTask, target_data_config_from_export_task};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeConfigForgeResult {
    pub(super) export_name: String,
    pub(super) files: Vec<RuntimeDataFile>,
}

#[derive(Debug)]
pub(super) struct RuntimeExportOutput {
    pub(super) warnings: Vec<String>,
    pub(super) task_outputs: Vec<RuntimeExportTaskOutput>,
}

#[derive(Debug)]
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
    let registry = DataFormatRegistry::with_builtin_exporters();
    export_meta_and_data_tasks_with_registry(
        raw_sources,
        export_tasks,
        linked_meta,
        target_tags,
        pool,
        &registry,
    )
}

fn export_meta_and_data_tasks_with_registry(
    raw_sources: Option<&RawSourceData>,
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    pool: &ThreadPool,
    registry: &DataFormatRegistry,
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

    ensure_planned_runtime_output_paths(export_tasks, linked_meta, target_tags, registry, pool)?;

    let data_set = normalize_data_set_with_pool(linked_meta, raw_sources.as_data_set(), pool)?;
    let validation_report = validate_normalized_data_set_for_target_with_report_and_pool(
        linked_meta,
        target_tags,
        &data_set,
        pool,
    )?;
    let mut warnings = Vec::new();
    warnings.extend(validation_report.warning_messages());
    let mut task_outputs = pool.install(|| {
        export_tasks
            .par_iter()
            .enumerate()
            .map(|(index, export_task)| {
                (
                    index,
                    export_meta_and_data_task(
                        export_task,
                        linked_meta,
                        target_tags,
                        &data_set,
                        pool,
                        registry,
                    ),
                )
            })
            .collect::<Vec<_>>()
    });
    task_outputs.sort_by_key(|(index, _)| *index);

    let mut stable_task_outputs = Vec::with_capacity(task_outputs.len());
    for (_, task_output) in task_outputs {
        stable_task_outputs.push(task_output?);
    }

    Ok(RuntimeExportOutput {
        warnings,
        task_outputs: stable_task_outputs,
    })
}

fn export_meta_and_data_task(
    export_task: &NormalizedExportTask,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &crate::data::DataSet,
    pool: &ThreadPool,
    registry: &DataFormatRegistry,
) -> Result<RuntimeExportTaskOutput> {
    let format_exporter = registry.require_any(&export_task.type_id)?;
    match format_exporter {
        DataExporterRef::Table(exporter) => {
            let runtime_artifacts = export_runtime_data(
                &export_task.output,
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
            Ok(RuntimeExportTaskOutput {
                export_name: export_task.name.clone(),
                result: Some(RuntimeConfigForgeResult {
                    export_name: export_task.name.clone(),
                    files,
                }),
                artifacts: runtime_artifacts
                    .into_iter()
                    .map(|artifact| artifact.artifact)
                    .collect(),
            })
        }
        DataExporterRef::Aggregate(exporter) => Ok(RuntimeExportTaskOutput {
            export_name: export_task.name.clone(),
            result: None,
            artifacts: exporter.export(
                &export_task.output,
                &target_data_config_from_export_task(export_task),
                linked_meta,
                target_tags,
                data_set,
            )?,
        }),
    }
}

fn ensure_planned_runtime_output_paths(
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    registry: &DataFormatRegistry,
    pool: &ThreadPool,
) -> Result<()> {
    let mut paths = BTreeSet::new();
    let mut planned_by_task = pool.install(|| {
        export_tasks
            .par_iter()
            .enumerate()
            .map(|(index, export_task)| {
                (
                    index,
                    plan_runtime_output_paths_for_task(
                        export_task,
                        linked_meta,
                        target_tags,
                        registry,
                    ),
                )
            })
            .collect::<Vec<_>>()
    });
    planned_by_task.sort_by_key(|(index, _)| *index);

    for (_, task_paths) in planned_by_task {
        for path in task_paths? {
            if !paths.insert(path.clone()) {
                return Err(ConfigForgeError::Validation(format!(
                    "multiple data artifacts target `{}`",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn plan_runtime_output_paths_for_task(
    export_task: &NormalizedExportTask,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    registry: &DataFormatRegistry,
) -> Result<Vec<std::path::PathBuf>> {
    match registry.require_any(&export_task.type_id)? {
        DataExporterRef::Table(exporter) => linked_meta
            .tables
            .iter()
            .filter(|table_meta| table_meta.meta.is_exported_for(target_tags))
            .map(|table_meta| {
                exporter
                    .plan_table_artifact(table_meta)
                    .map(|plan| lexical_normalize_path(&export_task.output.join(&plan.path)))
            })
            .collect(),
        DataExporterRef::Aggregate(exporter) => {
            let task = target_data_config_from_export_task(export_task);
            exporter
                .plan_artifacts(&export_task.output, &task, linked_meta, target_tags)?
                .into_iter()
                .map(|path| Ok(lexical_normalize_path(&path)))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    use crate::{
        artifact::Artifact,
        data::{DataSet, DataTable, RawSourceData},
        exporter::{AggregateDataExporter, DataFormatExporter, RuntimeArtifactPlan},
        meta::{LinkedTableMeta, MetaModel, TargetTags},
        runtime_contract::ExportDependency,
    };

    use super::*;

    struct InstrumentedAggregateExporter {
        active_plans: Arc<AtomicUsize>,
        max_active_plans: Arc<AtomicUsize>,
        active_exports: Arc<AtomicUsize>,
        max_active_exports: Arc<AtomicUsize>,
    }

    impl AggregateDataExporter for InstrumentedAggregateExporter {
        fn name(&self) -> &'static str {
            "instrumented_aggregate"
        }

        fn plan_artifacts(
            &self,
            out_dir: &Path,
            task: &crate::exporter::TargetDataConfig,
            _linked_meta: &LinkedMetaModel<'_>,
            _target_tags: &TargetTags,
        ) -> Result<Vec<PathBuf>> {
            let active_now = self.active_plans.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_plans
                .fetch_max(active_now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(20));
            self.active_plans.fetch_sub(1, Ordering::SeqCst);
            Ok(vec![out_dir.join(format!(
                "{}.txt",
                task.name.as_deref().unwrap_or("default")
            ))])
        }

        fn export(
            &self,
            out_dir: &Path,
            task: &crate::exporter::TargetDataConfig,
            _linked_meta: &LinkedMetaModel<'_>,
            _target_tags: &TargetTags,
            _data_set: &DataSet,
        ) -> Result<Vec<Artifact>> {
            let active_now = self.active_exports.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_exports
                .fetch_max(active_now, Ordering::SeqCst);
            if task.name.as_deref() == Some("slow") {
                thread::sleep(Duration::from_millis(30));
            } else {
                thread::sleep(Duration::from_millis(10));
            }
            self.active_exports.fetch_sub(1, Ordering::SeqCst);
            let task_name = task.name.as_deref().unwrap_or("default");
            Ok(vec![Artifact::text(
                out_dir.join(format!("{task_name}.txt")),
                task_name,
            )])
        }
    }

    #[test]
    fn meta_and_data_artifact_planning_and_generation_run_export_tasks_concurrently() {
        let active_plans = Arc::new(AtomicUsize::new(0));
        let max_active_plans = Arc::new(AtomicUsize::new(0));
        let active_exports = Arc::new(AtomicUsize::new(0));
        let max_active_exports = Arc::new(AtomicUsize::new(0));
        let mut registry = DataFormatRegistry::new();
        registry.register_aggregate(InstrumentedAggregateExporter {
            active_plans: active_plans.clone(),
            max_active_plans: max_active_plans.clone(),
            active_exports: active_exports.clone(),
            max_active_exports: max_active_exports.clone(),
        });
        let meta = minimal_meta_model();
        let linked_meta = meta.link().expect("meta should link");
        let raw_sources = raw_sources();
        let first_task = export_task("slow", PathBuf::from("out/slow"));
        let second_task = export_task("fast", PathBuf::from("out/fast"));
        let export_tasks = vec![&first_task, &second_task];
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool should build");

        let output = export_meta_and_data_tasks_with_registry(
            Some(&raw_sources),
            &export_tasks,
            &linked_meta,
            &TargetTags::empty(),
            &pool,
            &registry,
        )
        .expect("runtime export should succeed");

        assert!(max_active_plans.load(Ordering::SeqCst) > 1);
        assert!(max_active_exports.load(Ordering::SeqCst) > 1);
        assert_eq!(
            output
                .task_outputs
                .iter()
                .map(|output| output.export_name.as_str())
                .collect::<Vec<_>>(),
            vec!["slow", "fast"]
        );
        assert_eq!(
            output
                .task_outputs
                .into_iter()
                .flat_map(|output| output.artifacts)
                .map(|artifact| artifact.path)
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("out/slow/slow.txt"),
                PathBuf::from("out/fast/fast.txt")
            ]
        );
    }

    struct DuplicatePathExporter;

    impl DataFormatExporter for DuplicatePathExporter {
        fn name(&self) -> &'static str {
            "duplicate_path"
        }

        fn extension(&self) -> &'static str {
            "dup"
        }

        fn plan_table_artifact(
            &self,
            table_meta: &LinkedTableMeta<'_>,
        ) -> Result<RuntimeArtifactPlan> {
            Ok(RuntimeArtifactPlan {
                table_name: table_meta.meta.name.clone(),
                path: PathBuf::from("same-file.dup"),
            })
        }

        fn serialize_table(
            &self,
            _table_meta: &LinkedTableMeta<'_>,
            _table: &DataTable,
            _target_tags: &TargetTags,
        ) -> Result<Vec<u8>> {
            panic!("duplicate path planning should fail before serialization");
        }
    }

    #[test]
    fn meta_and_data_parallel_planning_rejects_path_conflicts_before_generation() {
        let mut registry = DataFormatRegistry::new();
        registry.register(DuplicatePathExporter);
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "levels"
                source = "levels.csv"
                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        )
        .expect("meta should load");
        let linked_meta = meta.link().expect("meta should link");
        let raw_sources = RawSourceData::new(DataSet::new(vec![
            DataTable::new(
                "items",
                vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
            ),
            DataTable::new(
                "levels",
                vec![BTreeMap::from([("id".to_owned(), "2".to_owned())])],
            ),
        ]));
        let task = NormalizedExportTask {
            name: "dup".to_owned(),
            type_id: "duplicate_path".to_owned(),
            dependency: ExportDependency::MetaAndData,
            output: PathBuf::from("out"),
            options: BTreeMap::new(),
        };
        let export_tasks = vec![&task];
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool should build");

        let error = export_meta_and_data_tasks_with_registry(
            Some(&raw_sources),
            &export_tasks,
            &linked_meta,
            &TargetTags::empty(),
            &pool,
            &registry,
        )
        .expect_err("duplicate path should fail during planning");

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("multiple data artifacts target `out/same-file.dup`"))
        );
    }

    fn export_task(name: &str, output: PathBuf) -> NormalizedExportTask {
        NormalizedExportTask {
            name: name.to_owned(),
            type_id: "instrumented_aggregate".to_owned(),
            dependency: ExportDependency::MetaAndData,
            output,
            options: BTreeMap::new(),
        }
    }

    fn minimal_meta_model() -> MetaModel {
        MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        )
        .expect("minimal meta should load")
    }

    fn raw_sources() -> RawSourceData {
        RawSourceData::new(DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        )]))
    }
}
