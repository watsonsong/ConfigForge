// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path, path::PathBuf};

mod options;
mod pipeline;
mod plan;
mod runtime;
mod target;
mod trace;

pub use options::{
    BuildErrorWithTrace, BuildOptions, BuildOutput, BuildTrace, BuildTraceStage,
    BuildTraceStageStatus, ExecutionOptions,
};

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity,
    Result,
    artifact::ArtifactWriter,
    diagnostic::code::{CF_BUILD_INPUT_REQUIRED, CF_BUILD_INVALID_CONFIG, CF_BUILD_INVALID_OPTION},
    generator::GeneratorRequirements,
    meta::TargetTags,
    project::ProjectConfig,
};

use pipeline::{TargetPipelineExecutionContext, execute_target_pipeline};
use target::{NormalizedExportTask, NormalizedTargetConfig};
use trace::BuildTraceRecorder;
#[cfg(test)]
use trace::generated_at;

pub fn build_target(options: BuildOptions<'_>) -> Result<BuildOutput> {
    build_target_with_trace(options).map_err(|error| error.error)
}

pub fn build_target_with_trace(
    options: BuildOptions<'_>,
) -> std::result::Result<BuildOutput, Box<BuildErrorWithTrace>> {
    let mut trace = BuildTraceRecorder::new();
    let execution = options.execution;
    let target_name = options.target_name.to_owned();

    let result = build_target_inner(options, &mut trace);
    let trace = trace.finish(BuildTrace::new(&target_name, execution));

    match result {
        Ok(output) => Ok(output.with_trace(trace)),
        Err(error) => Err(Box::new(BuildErrorWithTrace { error, trace })),
    }
}

fn build_target_inner(
    options: BuildOptions<'_>,
    trace: &mut BuildTraceRecorder,
) -> Result<BuildOutputWithoutTrace> {
    let cpu_pool = trace.record("create_cpu_pool", None, || {
        rayon::ThreadPoolBuilder::new()
            .num_threads(options.execution.jobs)
            .thread_name(|index| format!("config-forge-cpu-{index}"))
            .build()
            .map_err(|source| {
                build_diagnostic_error(
                    CF_BUILD_INVALID_OPTION,
                    "build.execution.cpu_pool",
                    format!("failed to create CPU worker pool: {source}"),
                    options.execution.jobs.to_string(),
                    "valid CPU worker pool configuration",
                    "Check --jobs and local thread pool resource limits.",
                )
            })
    })?;
    let project = trace.record("load_project", None, || {
        ProjectConfig::from_path(options.project_path)
    })?;
    let project_root = options
        .project_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let artifact_output_root = artifact_output_root(options.project_path)?;
    let mut target = trace.record("normalize_target", None, || {
        project.target(options.target_name)?.normalize()
    })?;
    if let Some(report_config) = options.validation_report {
        add_cli_validation_report_task(&mut target, report_config)?;
    }
    let meta_set = project.raw_meta_model();
    let target_tags = target.effective_target_tags(options.extra_tags);

    let sink = ArtifactWriter::new(options.artifact_write_mode)
        .with_output_root(artifact_output_root)
        .open_sink();
    let project_info = ArtifactProjectInfo::from(&project.project);

    let pipeline_output = execute_target_pipeline(TargetPipelineExecutionContext {
        project_root,
        project_info: &project_info,
        assets: &project.assets,
        target: &target,
        target_tags: &target_tags,
        raw_meta: &meta_set,
        data_root: project.project.input.as_deref(),
        pool: &cpu_pool,
        sink: &sink,
        trace,
    })?;

    let artifacts = trace.record("finish_artifacts", None, || sink.finish())?;

    Ok(BuildOutputWithoutTrace {
        warnings: pipeline_output.warnings,
        artifacts,
    })
}

fn artifact_output_root(project_path: &Path) -> Result<PathBuf> {
    let canonical_project_path =
        fs::canonicalize(project_path).map_err(|source| ConfigForgeError::ReadFile {
            path: project_path.to_path_buf(),
            source,
        })?;
    Ok(canonical_project_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf())
}

fn add_cli_validation_report_task(
    target: &mut NormalizedTargetConfig,
    report_config: crate::validation::ValidationReportExportConfig,
) -> Result<()> {
    if target
        .exports
        .iter()
        .any(|task| task.name == "cli-validation-report")
    {
        return Err(build_config_error(
            &target.name,
            format!(
                "target `{}` already declares export task `cli-validation-report`",
                target.name
            ),
            "build.validation_report.task",
            "cli-validation-report",
            "no existing export task named `cli-validation-report`",
        ));
    }
    let output_dir = report_config
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let file_name = report_config
        .output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            build_config_error(
                &target.name,
                format!(
                    "validation report output `{}` must include a valid file name",
                    report_config.output.display()
                ),
                "build.validation_report.output",
                report_config.output.display().to_string(),
                "path with a valid file name",
            )
        })?
        .to_owned();
    let mut options = BTreeMap::from([
        (
            "format".to_owned(),
            toml::Value::String(report_config.format.to_string()),
        ),
        ("file_name".to_owned(), toml::Value::String(file_name)),
    ]);
    if let Some(title) = report_config.title {
        options.insert("title".to_owned(), toml::Value::String(title));
    }
    target.exports.push(NormalizedExportTask {
        name: "cli-validation-report".to_owned(),
        type_id: "validation_report".to_owned(),
        requirements: GeneratorRequirements::meta_and_data().with_validation_report(),
        output: output_dir,
        options,
    });
    Ok(())
}

struct BuildOutputWithoutTrace {
    warnings: Vec<String>,
    artifacts: crate::artifact::ArtifactWriteResult,
}

impl BuildOutputWithoutTrace {
    fn with_trace(self, trace: BuildTrace) -> BuildOutput {
        BuildOutput {
            warnings: self.warnings,
            artifacts: self.artifacts,
            trace,
        }
    }
}

fn build_config_error(
    target_name: &str,
    message: String,
    source: &'static str,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_BUILD_INVALID_CONFIG,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(source)
        .with_domain("build")
        .with_target(target_name)
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint("Check target export tasks and build validation report options."),
    )
}

pub(super) fn build_diagnostic_error(
    code: &'static str,
    source: impl Into<String>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            code,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(source)
        .with_domain("build")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
}

pub(super) fn input_required_error(target_tags: &TargetTags) -> ConfigForgeError {
    let tags = target_tags.iter().collect::<Vec<_>>().join(",");
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_BUILD_INPUT_REQUIRED,
            DiagnosticSeverity::Error,
            "`project.input` must be set when target declares Data-dependent export tasks",
            DiagnosticLocation::default(),
        )
        .with_source("project.input")
        .with_domain("build")
        .with_current_value("<unset>")
        .with_expected("project.input path for Data-dependent export tasks")
        .with_hint("Set [project].input in ConfigForge.toml or remove Data-dependent export tasks.")
        .with_target(tags),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::artifact::{ArtifactSink, ArtifactWriteMode, ArtifactWriter};
    use crate::data::{DataSet, RawSourceData};
    use crate::diagnostic::code::CF_BUILD_EXPORT_TASK_FAILED;
    use crate::diagnostic::code::CF_GENERATOR_UNKNOWN_TYPE;
    use crate::generator::{ArtifactGenerator, GeneratorCommonInput, GeneratorRegistry};
    use crate::meta::MetaModel;

    use super::pipeline::{
        ExportBranchOutput, MetaOnlyGeneratorContext, generate_and_write_meta_only_task,
        join_link_meta_only_and_source_read, prepare_build_inputs,
    };
    use super::plan::{DataDependencyKey, TargetBuildPlan, build_execution_view_detail};
    use super::trace::civil_from_unix_days;
    use super::*;

    #[test]
    fn production_build_code_does_not_dispatch_through_artifact_toolkits() {
        for (path, source) in [
            ("src/build/mod.rs", include_str!("mod.rs")),
            ("src/build/runtime.rs", include_str!("runtime.rs")),
            ("src/build/target.rs", include_str!("target.rs")),
        ] {
            let production_source = source
                .split("\n#[cfg(test)]")
                .next()
                .expect("source split should always yield production section");

            assert!(
                !production_source.contains("crate::code_artifact"),
                "{path} production code should route code artifacts through GeneratorRegistry"
            );
            assert!(
                !production_source.contains("crate::data_artifact"),
                "{path} production code should route data artifacts through GeneratorRegistry"
            );
        }
    }

    #[test]
    fn converts_unix_days_to_civil_date() {
        assert_eq!(civil_from_unix_days(0), (1970, 1, 1));
        assert_eq!(civil_from_unix_days(20_590), (2026, 5, 17));
    }

    #[test]
    fn target_build_plan_shares_data_dependency_for_data_exports() {
        let target = NormalizedTargetConfig {
            name: "client".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![
                NormalizedExportTask {
                    name: "rust".to_owned(),
                    type_id: "rust".to_owned(),
                    requirements: GeneratorRequirements::meta_only(),
                    output: PathBuf::from("out/code"),
                    options: BTreeMap::new(),
                },
                NormalizedExportTask {
                    name: "json".to_owned(),
                    type_id: "json".to_owned(),
                    requirements: GeneratorRequirements::meta_and_data(),
                    output: PathBuf::from("out/json"),
                    options: BTreeMap::new(),
                },
                NormalizedExportTask {
                    name: "messagepack".to_owned(),
                    type_id: "messagepack".to_owned(),
                    requirements: GeneratorRequirements::meta_and_data(),
                    output: PathBuf::from("out/msgpack"),
                    options: BTreeMap::new(),
                },
            ],
        };
        let plan = TargetBuildPlan::new(
            &target,
            Some(Path::new("Data")),
            &TargetTags::new(["client", "shipping"]),
        );

        assert!(plan.requires_data());
        assert_eq!(plan.meta_only_exports, vec![0]);
        assert_eq!(plan.meta_and_data_exports, vec![1, 2]);
        assert_eq!(
            plan.data_dependency,
            Some(DataDependencyKey {
                input: Some(PathBuf::from("Data")),
                effective_tags: vec!["client".to_owned(), "shipping".to_owned()],
            })
        );
    }

    #[test]
    fn target_build_plan_skips_data_dependency_for_meta_only_exports() {
        let target = NormalizedTargetConfig {
            name: "client".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![NormalizedExportTask {
                name: "rust".to_owned(),
                type_id: "rust".to_owned(),
                requirements: GeneratorRequirements::meta_only(),
                output: PathBuf::from("out/code"),
                options: BTreeMap::new(),
            }],
        };
        let plan = TargetBuildPlan::new(&target, None, &TargetTags::new(["client"]));

        assert!(!plan.requires_data());
        assert!(plan.data_dependency.is_none());
        assert_eq!(plan.meta_only_exports, vec![0]);
        assert!(plan.meta_and_data_exports.is_empty());
    }

    #[test]
    fn target_build_plan_classifies_rust_only_as_meta_only() {
        let target = NormalizedTargetConfig {
            name: "code_only".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![export_task("rust", false)],
        };

        let plan = TargetBuildPlan::new(
            &target,
            Some(Path::new("missing_data")),
            &TargetTags::new(["client", "shipping"]),
        );

        assert!(plan.requires_meta);
        assert!(!plan.requires_data());
        assert_eq!(plan.effective_tags, vec!["client", "shipping"]);
        assert_eq!(plan.meta_only_exports, vec![0]);
        assert!(plan.meta_and_data_exports.is_empty());
        assert!(plan.data_dependency.is_none());
        assert_eq!(plan.export_indices_in_declaration_order(), vec![0]);
    }

    #[test]
    fn target_build_plan_classifies_json_only_as_data_backed() {
        let target = NormalizedTargetConfig {
            name: "data_only".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![export_task("json", true)],
        };

        let plan = TargetBuildPlan::new(
            &target,
            Some(Path::new("Data")),
            &TargetTags::new(["client"]),
        );

        assert!(plan.requires_meta);
        assert!(plan.requires_data());
        assert!(plan.meta_only_exports.is_empty());
        assert_eq!(plan.meta_and_data_exports, vec![0]);
        assert_eq!(
            plan.data_dependency,
            Some(DataDependencyKey {
                input: Some(PathBuf::from("Data")),
                effective_tags: vec!["client".to_owned()],
            })
        );
    }

    #[test]
    fn target_build_plan_preserves_mixed_export_indices() {
        let target = NormalizedTargetConfig {
            name: "client".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![
                export_task("json", true),
                export_task("rust", false),
                export_task("lua", true),
            ],
        };

        let plan = TargetBuildPlan::new(
            &target,
            Some(Path::new("Data")),
            &TargetTags::new(["client"]),
        );

        assert_eq!(plan.meta_only_exports, vec![1]);
        assert_eq!(plan.meta_and_data_exports, vec![0, 2]);
        assert_eq!(plan.export_indices_in_declaration_order(), vec![0, 1, 2]);
        assert_eq!(
            plan.meta_only_export_tasks(&target)
                .into_iter()
                .map(|(index, task)| (index, task.name.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "rust")]
        );
        assert_eq!(
            plan.meta_and_data_export_tasks(&target)
                .into_iter()
                .map(|(index, task)| (index, task.name.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "json"), (2, "lua")]
        );
    }

    #[test]
    fn execution_view_detail_reports_plan_summary_without_exposing_node_shape() {
        let target = NormalizedTargetConfig {
            name: "client".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![
                export_task("json", true),
                export_task("rust", false),
                export_task("lua", true),
            ],
        };
        let plan = TargetBuildPlan::new(
            &target,
            Some(Path::new("Data")),
            &TargetTags::new(["client"]),
        );

        let detail = build_execution_view_detail(&target, &plan);

        assert!(detail.contains("target=client"));
        assert!(detail.contains("mode=with_data"));
        assert!(detail.contains("meta_only_exports=1"));
        assert!(detail.contains("meta_and_data_exports=2"));
        assert!(detail.contains("exports=[json,rust,lua]"));
    }

    #[test]
    fn link_meta_only_and_source_read_overlap_before_normalize_join() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool should build");
        let meta = minimal_meta_model();
        let source_started = Arc::new(AtomicBool::new(false));
        let link_observed_source = Arc::new(AtomicBool::new(false));
        let source_started_for_link = source_started.clone();
        let link_observed_source_for_link = link_observed_source.clone();
        let source_started_for_source = source_started.clone();
        let started_at = Instant::now();

        let ((linked_meta, meta_only_output), raw_sources) = join_link_meta_only_and_source_read(
            || {
                thread::sleep(Duration::from_millis(40));
                if source_started_for_link.load(Ordering::SeqCst) {
                    link_observed_source_for_link.store(true, Ordering::SeqCst);
                }
                thread::sleep(Duration::from_millis(40));
                Ok((
                    meta.link()
                        .map_err(crate::meta::meta_diagnostics_to_report)
                        .map_err(ConfigForgeError::MetaValidation)?,
                    empty_branch_output(),
                ))
            },
            Some(|| {
                source_started_for_source.store(true, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(80));
                Ok(empty_raw_sources())
            }),
            &pool,
        )
        .expect("meta link and source read should finish");

        assert!(linked_meta.table("items").is_some());
        assert!(meta_only_output.warnings.is_empty());
        assert!(raw_sources.is_some());
        assert!(link_observed_source.load(Ordering::SeqCst));
        assert!(started_at.elapsed() < Duration::from_millis(140));
    }

    #[test]
    fn meta_only_branch_can_start_before_source_read_finishes() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool should build");
        let meta = minimal_meta_model();
        let source_finished = Arc::new(AtomicBool::new(false));
        let meta_only_observed_source_unfinished = Arc::new(AtomicBool::new(false));
        let source_finished_for_meta_only = source_finished.clone();
        let meta_only_observed_source_unfinished_for_meta_only =
            meta_only_observed_source_unfinished.clone();
        let source_finished_for_source = source_finished.clone();

        let ((linked_meta, _meta_only_output), raw_sources) = join_link_meta_only_and_source_read(
            || {
                thread::sleep(Duration::from_millis(20));
                let linked_meta = meta
                    .link()
                    .map_err(crate::meta::meta_diagnostics_to_report)
                    .map_err(ConfigForgeError::MetaValidation)?;
                if !source_finished_for_meta_only.load(Ordering::SeqCst) {
                    meta_only_observed_source_unfinished_for_meta_only
                        .store(true, Ordering::SeqCst);
                }
                Ok((linked_meta, empty_branch_output()))
            },
            Some(|| {
                thread::sleep(Duration::from_millis(100));
                source_finished_for_source.store(true, Ordering::SeqCst);
                Ok(empty_raw_sources())
            }),
            &pool,
        )
        .expect("pipeline branches should finish");

        assert!(linked_meta.table("items").is_some());
        assert!(raw_sources.is_some());
        assert!(source_finished.load(Ordering::SeqCst));
        assert!(meta_only_observed_source_unfinished.load(Ordering::SeqCst));
    }

    #[test]
    fn meta_link_error_wins_when_parallel_source_read_also_fails() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool should build");
        let result = join_link_meta_only_and_source_read(
            || {
                Err(build_diagnostic_error(
                    CF_BUILD_EXPORT_TASK_FAILED,
                    "build.test",
                    "meta failed".to_owned(),
                    "meta branch",
                    "successful meta branch",
                    "test failure",
                ))
            },
            Some(|| {
                Err(build_diagnostic_error(
                    CF_BUILD_EXPORT_TASK_FAILED,
                    "build.test",
                    "source failed".to_owned(),
                    "source branch",
                    "successful source branch",
                    "test failure",
                ))
            }),
            &pool,
        );
        let error = match result {
            Ok(_) => panic!("parallel preparation should fail"),
            Err(error) => error,
        };

        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, CF_BUILD_EXPORT_TASK_FAILED);
        assert_eq!(diagnostic.message, "meta failed");
    }

    #[test]
    fn code_only_target_preparation_does_not_start_source_read() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool should build");
        let meta = minimal_meta_model();
        let target = NormalizedTargetConfig {
            name: "code_only".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![export_task("rust", false)],
        };
        let plan = TargetBuildPlan::new(&target, None, &TargetTags::new(["client"]));

        let prepared =
            prepare_build_inputs(&meta, &plan, &TargetTags::new(["client"]), None, &pool)
                .expect("code-only preparation should not require project.input");

        assert!(prepared.read_sources.is_none());
    }

    #[test]
    fn build_trace_recorder_collects_stage_timing_and_failure_status() {
        let mut recorder = BuildTraceRecorder::new();

        let ok_value = recorder
            .record("successful_stage", Some("detail".to_owned()), || Ok(7))
            .expect("successful stage should return value");
        let failed = recorder.record::<(), _>("failed_stage", None, || {
            Err(build_diagnostic_error(
                CF_BUILD_EXPORT_TASK_FAILED,
                "build.test",
                "boom".to_owned(),
                "failed stage",
                "successful stage",
                "test failure",
            ))
        });
        let trace = recorder.finish(BuildTrace::new(
            "client",
            ExecutionOptions::from_cli(Some(2), Some(4), Some(2), false)
                .expect("execution options"),
        ));

        assert_eq!(ok_value, 7);
        assert!(failed.is_err());
        assert_eq!(trace.target, "client");
        assert_eq!(trace.jobs, 2);
        assert_eq!(trace.stages.len(), 2);
        assert_eq!(trace.stages[0].name, "successful_stage");
        assert_eq!(trace.stages[0].status, BuildTraceStageStatus::Succeeded);
        assert_eq!(trace.stages[0].detail.as_deref(), Some("detail"));
        assert_eq!(trace.stages[1].name, "failed_stage");
        assert_eq!(trace.stages[1].status, BuildTraceStageStatus::Failed);
    }

    fn expect_single_diagnostic(error: ConfigForgeError) -> Diagnostic {
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected structured diagnostic report");
        };
        assert_eq!(report.diagnostics.len(), 1);
        report.diagnostics.into_iter().next().unwrap()
    }

    #[test]
    fn run_exporters_trace_marks_nested_branch_error_as_failed() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool should build");
        let meta = minimal_meta_model();
        let target = NormalizedTargetConfig {
            name: "bad_meta_only".to_owned(),
            tags: Vec::new(),
            exports: vec![NormalizedExportTask {
                name: "unknown".to_owned(),
                type_id: "unknown_language".to_owned(),
                requirements: GeneratorRequirements::meta_only(),
                output: PathBuf::from("out/code"),
                options: BTreeMap::new(),
            }],
        };
        let target_tags = TargetTags::empty();
        let project_info = ArtifactProjectInfo::default();
        let assets = crate::project::AssetsConfig::default();
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();
        let mut trace = BuildTraceRecorder::new();

        let result = execute_target_pipeline(TargetPipelineExecutionContext {
            project_root: Path::new("."),
            project_info: &project_info,
            assets: &assets,
            target: &target,
            target_tags: &target_tags,
            raw_meta: &meta,
            data_root: None,
            pool: &pool,
            sink: &sink,
            trace: &mut trace,
        });

        let Err(ConfigForgeError::ValidationReport(report)) = result else {
            panic!("expected structured unknown generator diagnostic");
        };
        assert_eq!(report.diagnostics[0].code, CF_GENERATOR_UNKNOWN_TYPE);
        assert_eq!(
            report.diagnostics[0].current_value.as_deref(),
            Some("unknown_language")
        );
        let run_exporters_stage = trace
            .stages
            .iter()
            .find(|stage| stage.name == "run_exporters")
            .expect("run_exporters stage should be recorded");
        assert_eq!(run_exporters_stage.status, BuildTraceStageStatus::Failed);
    }

    #[test]
    fn meta_only_generator_writes_via_shared_sink() {
        struct SinkOnlyGenerator {
            called: Arc<AtomicBool>,
        }

        impl ArtifactGenerator for SinkOnlyGenerator {
            fn type_id(&self) -> &'static str {
                "sink_only"
            }

            fn requirements(&self) -> GeneratorRequirements {
                GeneratorRequirements::meta_only()
            }

            fn generate_to_sink(
                &self,
                input: &GeneratorCommonInput<'_>,
                sink: &ArtifactSink,
            ) -> Result<()> {
                self.called.store(true, Ordering::SeqCst);
                sink.write_text(input.output.join("generated.txt"), "sink-first")
            }
        }

        let meta = minimal_meta_model();
        let linked_meta = meta.link().expect("minimal meta should link");
        let target_tags = TargetTags::empty();
        let project_info = ArtifactProjectInfo::default();
        let called = Arc::new(AtomicBool::new(false));
        let generators = vec![Box::new(SinkOnlyGenerator {
            called: called.clone(),
        }) as Box<dyn ArtifactGenerator>];
        let generator_registry = GeneratorRegistry::new(generators);
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();
        let context = MetaOnlyGeneratorContext {
            project_root: Path::new("."),
            project_info: &project_info,
            target_name: "default",
            target_tags: &target_tags,
            linked_meta: &linked_meta,
            generated_at: "2026-06-03T00:00:00Z",
            generator_registry: &generator_registry,
            sink: &sink,
        };
        let export_task = NormalizedExportTask {
            name: "code".to_owned(),
            type_id: "sink_only".to_owned(),
            requirements: GeneratorRequirements::meta_only(),
            output: PathBuf::from("out/sink_only"),
            options: BTreeMap::new(),
        };

        generate_and_write_meta_only_task(&context, &export_task)
            .expect("sink-first generator should write successfully");
        let result = sink.finish().expect("sink should finish");

        assert!(called.load(Ordering::SeqCst));
        assert_eq!(result.entries.len(), 1);
        assert_eq!(
            result.entries[0].path,
            PathBuf::from("out/sink_only/generated.txt")
        );
        assert_eq!(result.entries[0].bytes, "sink-first".len());
    }

    fn export_task(name: &str, needs_data: bool) -> NormalizedExportTask {
        NormalizedExportTask {
            name: name.to_owned(),
            type_id: name.to_owned(),
            requirements: if needs_data {
                GeneratorRequirements::meta_and_data()
            } else {
                GeneratorRequirements::meta_only()
            },
            output: PathBuf::from(format!("out/{name}")),
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

    fn empty_raw_sources() -> RawSourceData {
        RawSourceData::new(DataSet::try_new(Vec::new()).expect("empty data set should build"))
    }

    fn empty_branch_output() -> ExportBranchOutput {
        ExportBranchOutput {
            warnings: Vec::new(),
        }
    }
}
