// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use rayon::{ThreadPool, prelude::*};

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Result,
    artifact::ArtifactSink,
    data::RawSourceData,
    diagnostic::code::{CF_BUILD_EXPORT_TASK_FAILED, CF_BUILD_INPUT_REQUIRED},
    generator::{
        GeneratorCommonInput, GeneratorCommonInputContext, GeneratorRegistry, GeneratorTaskInput,
        PreparedGeneratorInputs,
    },
    meta::{LinkedMetaModel, TargetTags},
    project::AssetsConfig,
    validation::{
        normalize_data_set_with_pool,
        validate_normalized_data_set_for_target_with_assets_outcome_and_pool,
    },
};

use super::{
    asset_refs::canonicalize_asset_refs, build_diagnostic_error, target::NormalizedExportTask,
};

#[derive(Debug)]
pub(super) struct RuntimeExportOutput {
    pub(super) warnings: Vec<String>,
    pub(super) diagnostics: Vec<crate::Diagnostic>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn export_meta_and_data_tasks(
    raw_sources: Option<&RawSourceData>,
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    project_root: &std::path::Path,
    project_info: &ArtifactProjectInfo,
    target_name: &str,
    generated_at: &str,
    generator_registry: &GeneratorRegistry,
    pool: &ThreadPool,
    sink: &ArtifactSink,
) -> Result<RuntimeExportOutput> {
    if export_tasks.is_empty() {
        return Ok(RuntimeExportOutput {
            warnings: Vec::new(),
            diagnostics: Vec::new(),
        });
    }
    let raw_sources = raw_sources.ok_or_else(|| {
        build_diagnostic_error(
            CF_BUILD_INPUT_REQUIRED,
            "build.runtime.input",
            "raw source data must be prepared when target declares Data-dependent export tasks"
                .to_owned(),
            "missing raw source data",
            "raw source data for Data-dependent export tasks",
            "Check build input preparation and generator requirements before runtime export.",
        )
    })?;

    let data_set = normalize_data_set_with_pool(linked_meta, raw_sources.as_data_set(), pool)?;
    let validation_outcome = validate_normalized_data_set_for_target_with_assets_outcome_and_pool(
        linked_meta,
        assets,
        target_tags,
        &data_set,
        pool,
    )?;
    if validation_outcome.is_blocking() {
        let report_result = export_validation_report_tasks(
            export_tasks,
            linked_meta,
            target_tags,
            project_root,
            project_info,
            target_name,
            generated_at,
            validation_outcome.report(),
            sink,
            generator_registry,
        );
        let error = ConfigForgeError::ValidationReport(
            validation_outcome.into_report().into_diagnostic_report(),
        );
        return Err(match report_result {
            Ok(()) => error,
            Err(report_error) => error.with_diagnostics(report_error.diagnostics()),
        });
    }
    let validation_report = validation_outcome.into_report();
    let mut warnings = Vec::new();
    warnings.extend(validation_report.warning_messages());
    let diagnostics = validation_report
        .clone()
        .into_diagnostic_report()
        .diagnostics;
    let export_data_set = canonicalize_asset_refs(linked_meta, assets, target_tags, data_set)
        .map_err(|error| error.with_diagnostics(diagnostics.clone()))?;
    let task_results = pool.install(|| {
        export_tasks
            .par_iter()
            .map(|export_task| {
                export_meta_and_data_task(
                    export_task,
                    linked_meta,
                    target_tags,
                    project_root,
                    project_info,
                    target_name,
                    generated_at,
                    &export_data_set,
                    &validation_report,
                    pool,
                    sink,
                    generator_registry,
                )
            })
            .collect::<Vec<_>>()
    });

    for task_result in task_results {
        task_result.map_err(|error| error.with_diagnostics(diagnostics.clone()))?;
    }

    Ok(RuntimeExportOutput {
        warnings,
        diagnostics,
    })
}

#[allow(clippy::too_many_arguments)]
fn export_validation_report_tasks(
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    project_root: &std::path::Path,
    project_info: &ArtifactProjectInfo,
    target_name: &str,
    generated_at: &str,
    validation_report: &crate::validation::ValidationReport,
    sink: &ArtifactSink,
    generator_registry: &GeneratorRegistry,
) -> Result<()> {
    let mut errors = Vec::new();
    for task in export_tasks
        .iter()
        .copied()
        .filter(|task| task.requirements.supports_report_only)
    {
        let generator = generator_registry.require_generator(&task.type_id)?;
        if let Err(error) = generator.generate(
            &GeneratorTaskInput {
                common: generator_common_input(
                    task,
                    linked_meta,
                    target_tags,
                    project_root,
                    project_info,
                    target_name,
                    generated_at,
                ),
                prepared: PreparedGeneratorInputs::ValidationReportOnly { validation_report },
            },
            sink,
        ) {
            errors.extend(error.diagnostics());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ConfigForgeError::from_diagnostics(errors))
    }
}

#[allow(clippy::too_many_arguments)]
fn export_meta_and_data_task(
    export_task: &NormalizedExportTask,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    project_root: &std::path::Path,
    project_info: &ArtifactProjectInfo,
    target_name: &str,
    generated_at: &str,
    data_set: &crate::data::DataSet,
    validation_report: &crate::validation::ValidationReport,
    pool: &ThreadPool,
    sink: &ArtifactSink,
    generator_registry: &GeneratorRegistry,
) -> Result<()> {
    let generator = generator_registry.require_generator(&export_task.type_id)?;
    if !generator.requirements().needs_data {
        return Err(build_diagnostic_error(
            CF_BUILD_EXPORT_TASK_FAILED,
            "build.runtime.scheduler",
            format!(
                "export task `{}` was scheduled with Data, but generator `{}` does not require Data",
                export_task.name, export_task.type_id
            ),
            format!("{}:{}", export_task.name, export_task.type_id),
            "Meta-only generator scheduled in meta-only branch",
            "Check generator requirements and target build plan classification.",
        ));
    }
    let common = generator_common_input(
        export_task,
        linked_meta,
        target_tags,
        project_root,
        project_info,
        target_name,
        generated_at,
    );
    generator.generate(
        &GeneratorTaskInput {
            common,
            prepared: PreparedGeneratorInputs::MetaAndData {
                data_set,
                validation_report,
                pool,
            },
        },
        sink,
    )
}

fn generator_common_input<'meta>(
    export_task: &NormalizedExportTask,
    linked_meta: &'meta LinkedMetaModel<'meta>,
    target_tags: &TargetTags,
    project_root: &std::path::Path,
    project_info: &ArtifactProjectInfo,
    target_name: &str,
    generated_at: &str,
) -> GeneratorCommonInput<'meta> {
    GeneratorCommonInput::from_task_context(
        GeneratorCommonInputContext {
            linked_meta,
            target_tags,
            project_root,
            project_info,
            target_name,
            generated_at,
        },
        &export_task.name,
        &export_task.type_id,
        &export_task.output,
        &export_task.options,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

    use crate::{
        Diagnostic,
        artifact::{ArtifactWriteMode, ArtifactWriter},
        data::{DataSet, DataTable, RawSourceData},
        generator::{
            ArtifactGenerator, GeneratorCommonInput, GeneratorRegistry, GeneratorRequirements,
        },
        meta::{MetaModel, TargetTags},
    };

    use super::*;
    use crate::{
        build::artifact_plan::plan_target_artifacts, diagnostic::code::CF_ARTIFACT_PATH_COLLISION,
    };

    struct InstrumentedAggregateGenerator {
        active_plans: Arc<AtomicUsize>,
        max_active_plans: Arc<AtomicUsize>,
        active_exports: Arc<AtomicUsize>,
        max_active_exports: Arc<AtomicUsize>,
    }

    impl ArtifactGenerator for InstrumentedAggregateGenerator {
        fn type_id(&self) -> &'static str {
            "instrumented_aggregate"
        }

        fn requirements(&self) -> GeneratorRequirements {
            GeneratorRequirements::meta_and_data()
        }

        fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
            let active_now = self.active_plans.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_plans
                .fetch_max(active_now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(20));
            self.active_plans.fetch_sub(1, Ordering::SeqCst);
            let task = input.target_data_config();
            Ok(vec![input.output.join(format!(
                "{}.txt",
                task.name.as_deref().unwrap_or("default")
            ))])
        }

        fn generate(
            &self,
            input: &crate::generator::GeneratorTaskInput<'_, '_>,
            sink: &ArtifactSink,
        ) -> Result<()> {
            let input = input.data_input(self.type_id())?;
            let active_now = self.active_exports.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_exports
                .fetch_max(active_now, Ordering::SeqCst);
            let task = input.common.target_data_config();
            if task.name.as_deref() == Some("slow") {
                thread::sleep(Duration::from_millis(30));
            } else {
                thread::sleep(Duration::from_millis(10));
            }
            self.active_exports.fetch_sub(1, Ordering::SeqCst);
            let task_name = task.name.as_deref().unwrap_or("default");
            sink.write_text(
                input.common.output.join(format!("{task_name}.txt")),
                task_name,
            )
        }
    }

    #[test]
    fn meta_and_data_artifact_planning_and_generation_run_export_tasks_concurrently() {
        let active_plans = Arc::new(AtomicUsize::new(0));
        let max_active_plans = Arc::new(AtomicUsize::new(0));
        let active_exports = Arc::new(AtomicUsize::new(0));
        let max_active_exports = Arc::new(AtomicUsize::new(0));
        let generator_registry =
            GeneratorRegistry::new(vec![Box::new(InstrumentedAggregateGenerator {
                active_plans: active_plans.clone(),
                max_active_plans: max_active_plans.clone(),
                active_exports: active_exports.clone(),
                max_active_exports: max_active_exports.clone(),
            })]);
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
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

        plan_target_artifacts(
            &export_tasks,
            GeneratorCommonInputContext {
                linked_meta: &linked_meta,
                target_tags: &TargetTags::empty(),
                project_root: std::path::Path::new(""),
                project_info: &ArtifactProjectInfo::default(),
                target_name: "",
                generated_at: &crate::build::generated_at(),
            },
            &generator_registry,
            &pool,
            &sink,
        )
        .expect("target artifacts should be planned");

        let output = export_meta_and_data_tasks(
            Some(&raw_sources),
            &export_tasks,
            &linked_meta,
            &AssetsConfig::default(),
            &TargetTags::empty(),
            std::path::Path::new(""),
            &ArtifactProjectInfo::default(),
            "",
            &crate::build::generated_at(),
            &generator_registry,
            &pool,
            &sink,
        )
        .expect("runtime export should succeed");

        assert!(max_active_plans.load(Ordering::SeqCst) > 1);
        assert!(max_active_exports.load(Ordering::SeqCst) > 1);
        assert!(output.warnings.is_empty());
        assert!(output.diagnostics.is_empty());
        let artifact_result = sink.finish().expect("sink should finish");
        assert_eq!(
            artifact_result
                .entries
                .into_iter()
                .map(|entry| (entry.path, entry.bytes, entry.written))
                .collect::<Vec<_>>(),
            vec![
                (PathBuf::from("out/fast/fast.txt"), 4, false),
                (PathBuf::from("out/slow/slow.txt"), 4, false)
            ]
        );
    }

    struct FailingGenerator {
        second_failed: Mutex<mpsc::Receiver<()>>,
        notify_second_failed: mpsc::Sender<()>,
        completed: Arc<Mutex<Vec<String>>>,
    }

    impl ArtifactGenerator for FailingGenerator {
        fn type_id(&self) -> &'static str {
            "failing"
        }

        fn requirements(&self) -> GeneratorRequirements {
            GeneratorRequirements::meta_and_data()
        }

        fn plan_artifacts(&self, _input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
            Ok(Vec::new())
        }

        fn generate(&self, input: &GeneratorTaskInput<'_, '_>, _sink: &ArtifactSink) -> Result<()> {
            let task_name = input.common.task_name.as_str();
            if task_name == "first" {
                self.second_failed
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .expect("second task should fail before first task");
            }
            self.completed.lock().unwrap().push(task_name.to_owned());
            if task_name == "second" {
                self.notify_second_failed.send(()).unwrap();
            }
            Err(build_diagnostic_error(
                CF_BUILD_EXPORT_TASK_FAILED,
                "test.generator",
                format!("{task_name} failed"),
                task_name,
                "successful generation",
                "Test generator failure propagation.",
            ))
        }
    }

    #[test]
    fn runtime_export_waits_for_all_tasks_and_returns_first_declared_failure() {
        let (sender, receiver) = mpsc::channel();
        let completed = Arc::new(Mutex::new(Vec::new()));
        let registry = GeneratorRegistry::new(vec![Box::new(FailingGenerator {
            second_failed: Mutex::new(receiver),
            notify_second_failed: sender,
            completed: completed.clone(),
        })]);
        let meta = minimal_meta_model();
        let linked_meta = meta.link().unwrap();
        let tasks = ["first", "second", "third"].map(|name| NormalizedExportTask {
            type_id: "failing".to_owned(),
            ..export_task(name, PathBuf::from("out"))
        });
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

        let error = export_meta_and_data_tasks(
            Some(&raw_sources()),
            &tasks.iter().collect::<Vec<_>>(),
            &linked_meta,
            &AssetsConfig::default(),
            &TargetTags::empty(),
            std::path::Path::new(""),
            &ArtifactProjectInfo::default(),
            "",
            &crate::build::generated_at(),
            &registry,
            &pool,
            &sink,
        )
        .expect_err("generator failures should propagate");

        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, CF_BUILD_EXPORT_TASK_FAILED);
        assert_eq!(diagnostic.message, "first failed");
        let completed = completed.lock().unwrap();
        assert_eq!(completed.len(), 3);
        let position = |name| completed.iter().position(|task| task == name).unwrap();
        assert!(position("second") < position("first"));
        assert!(completed.iter().any(|task| task == "third"));
    }

    struct DuplicatePathGenerator;

    impl ArtifactGenerator for DuplicatePathGenerator {
        fn type_id(&self) -> &'static str {
            "duplicate_path"
        }

        fn requirements(&self) -> GeneratorRequirements {
            GeneratorRequirements::meta_and_data()
        }

        fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
            input
                .meta
                .tables
                .iter()
                .filter(|table_meta| table_meta.meta.is_exported_for(&input.target_tags))
                .map(|_table_meta| Ok(input.output.join("same-file.dup")))
                .collect()
        }

        fn generate(
            &self,
            _input: &crate::generator::GeneratorTaskInput<'_, '_>,
            _sink: &ArtifactSink,
        ) -> Result<()> {
            let _input = _input.data_input(self.type_id())?;
            panic!("duplicate path planning should fail before serialization");
        }
    }

    #[test]
    fn meta_and_data_parallel_planning_rejects_path_conflicts_before_generation() {
        let generator_registry = GeneratorRegistry::new(vec![Box::new(DuplicatePathGenerator)]);
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
        let task = NormalizedExportTask {
            name: "dup".to_owned(),
            type_id: "duplicate_path".to_owned(),
            requirements: GeneratorRequirements::meta_and_data(),
            output: PathBuf::from("out"),
            options: BTreeMap::new(),
        };
        let export_tasks = vec![&task];
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool should build");
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

        let error = plan_target_artifacts(
            &export_tasks,
            GeneratorCommonInputContext {
                linked_meta: &linked_meta,
                target_tags: &TargetTags::empty(),
                project_root: std::path::Path::new(""),
                project_info: &ArtifactProjectInfo::default(),
                target_name: "",
                generated_at: &crate::build::generated_at(),
            },
            &generator_registry,
            &pool,
            &sink,
        )
        .expect_err("duplicate path should fail during planning");
        let diagnostic = expect_single_diagnostic(error);

        assert_eq!(diagnostic.code, CF_ARTIFACT_PATH_COLLISION);
        assert_eq!(
            std::path::Path::new(diagnostic.current_value.as_deref().expect("collision path")),
            std::path::Path::new("out/same-file.dup")
        );
    }

    fn expect_single_diagnostic(error: ConfigForgeError) -> Diagnostic {
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected structured diagnostic report");
        };
        assert_eq!(report.diagnostics.len(), 1);
        report.diagnostics.into_iter().next().unwrap()
    }

    fn export_task(name: &str, output: PathBuf) -> NormalizedExportTask {
        NormalizedExportTask {
            name: name.to_owned(),
            type_id: "instrumented_aggregate".to_owned(),
            requirements: GeneratorRequirements::meta_and_data(),
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
