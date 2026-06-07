// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::Path,
    path::PathBuf,
    time::Instant,
    time::{SystemTime, UNIX_EPOCH},
};

use rayon::ThreadPool;
use rayon::prelude::*;

mod options;
mod runtime;
mod target;

pub use options::{
    BuildErrorWithTrace, BuildOptions, BuildOutput, BuildTrace, BuildTraceStage,
    BuildTraceStageStatus, ExecutionOptions,
};

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Result,
    artifact::{ArtifactSink, ArtifactWriter},
    data::{DataLoader, RawSourceData, SourceReadPlan},
    generator::{
        GeneratorCommonInput, GeneratorRegistry, GeneratorRequirements, GeneratorTaskInput,
        PreparedGeneratorInputs,
    },
    meta::{LinkedMetaModel, MetaModel, TargetTags},
    project::ProjectConfig,
};

use runtime::export_meta_and_data_tasks;
use target::{NormalizedExportTask, NormalizedTargetConfig};

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
                ConfigForgeError::Validation(format!("failed to create CPU worker pool: {source}"))
            })
    })?;
    let project = trace.record("load_project", None, || {
        ProjectConfig::from_path(options.project_path)
    })?;
    let project_root = options
        .project_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let mut target = trace.record("normalize_target", None, || {
        project.target(options.target_name)?.normalize()
    })?;
    if let Some(report_config) = options.validation_report {
        add_cli_validation_report_task(&mut target, report_config)?;
    }
    let meta_set = project.raw_meta_model();
    let target_tags = target.effective_target_tags(options.extra_tags);

    let sink = ArtifactWriter::new(options.artifact_write_mode).open_sink();
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

fn add_cli_validation_report_task(
    target: &mut NormalizedTargetConfig,
    report_config: crate::validation::ValidationReportExportConfig,
) -> Result<()> {
    if target
        .exports
        .iter()
        .any(|task| task.name == "cli-validation-report")
    {
        return Err(ConfigForgeError::Validation(format!(
            "target `{}` already declares export task `cli-validation-report`",
            target.name
        )));
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
            ConfigForgeError::Validation(format!(
                "validation report output `{}` must include a valid file name",
                report_config.output.display()
            ))
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

struct TargetPipelineExecutionContext<'a, 'meta> {
    project_root: &'a Path,
    project_info: &'a ArtifactProjectInfo,
    assets: &'a crate::project::AssetsConfig,
    target: &'a NormalizedTargetConfig,
    target_tags: &'a TargetTags,
    raw_meta: &'meta MetaModel,
    data_root: Option<&'a Path>,
    pool: &'a ThreadPool,
    sink: &'a ArtifactSink,
    trace: &'a mut BuildTraceRecorder,
}

struct BuildInputPreparation<'a> {
    read_sources: Option<Box<dyn FnOnce() -> Result<RawSourceData> + Send + 'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildDependencyPlan {
    targets: Vec<TargetBuildPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TargetBuildPlan {
    target_name: String,
    effective_tags: Vec<String>,
    requires_meta: bool,
    data_dependency: Option<DataDependencyKey>,
    meta_only_exports: Vec<usize>,
    meta_and_data_exports: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DataDependencyKey {
    input: Option<PathBuf>,
    effective_tags: Vec<String>,
}

struct TargetPipelineOutput {
    warnings: Vec<String>,
}

struct ExportBranchOutput {
    warnings: Vec<String>,
}

fn execute_target_pipeline(
    context: TargetPipelineExecutionContext<'_, '_>,
) -> Result<TargetPipelineOutput> {
    let dependency_plan = context.trace.record("plan_dependencies", None, || {
        Ok(BuildDependencyPlan::for_target(
            context.target,
            context.data_root,
            context.target_tags,
        ))
    })?;
    let target_plan = dependency_plan.single_target_plan()?;
    let execution_view_detail = build_execution_view_detail(context.target, target_plan);
    context
        .trace
        .record("build_execution_view", Some(execution_view_detail), || {
            Ok(())
        })?;
    let meta_and_data_exports = target_plan.meta_and_data_export_tasks(context.target);
    let meta_only_exports = target_plan.meta_only_export_tasks(context.target);
    let mut prepared_inputs = context.trace.record("prepare_build_inputs", None, || {
        prepare_build_inputs(
            context.raw_meta,
            target_plan,
            context.target_tags,
            context.data_root,
            context.pool,
        )
    })?;

    let generated_at = generated_at();
    let generator_registry = GeneratorRegistry::with_builtin_generators();

    let (runtime_output, meta_only_output) = context.trace.record("run_exporters", None, || {
        execute_export_pipeline(ExportPipelineContext {
            raw_meta: context.raw_meta,
            read_sources: prepared_inputs.read_sources.take(),
            runtime_exports: &meta_and_data_exports,
            meta_only_exports: &meta_only_exports,
            project_root: context.project_root,
            project_info: context.project_info,
            target_name: &context.target.name,
            assets: context.assets,
            target_tags: context.target_tags,
            generated_at: &generated_at,
            generator_registry: &generator_registry,
            pool: context.pool,
            sink: context.sink,
        })
    })?;
    let warnings = context
        .trace
        .record("collect_export_diagnostics", None, || {
            let mut warnings = runtime_output.warnings;
            warnings.extend(meta_only_output.warnings);
            Ok(warnings)
        })?;

    Ok(TargetPipelineOutput { warnings })
}

struct BuildTraceRecorder {
    started_at: Instant,
    stages: Vec<BuildTraceStage>,
}

impl BuildTraceRecorder {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            stages: Vec::new(),
        }
    }

    fn record<T, F>(&mut self, name: &str, detail: Option<String>, operation: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let started_at_ms = self.started_at.elapsed().as_millis();
        let stage_started = Instant::now();
        let result = operation();
        let duration_ms = stage_started.elapsed().as_millis();
        let status = if result.is_ok() {
            BuildTraceStageStatus::Succeeded
        } else {
            BuildTraceStageStatus::Failed
        };
        self.stages.push(BuildTraceStage {
            name: name.to_owned(),
            started_at_ms,
            duration_ms,
            status,
            detail,
        });
        result
    }

    fn finish(self, mut trace: BuildTrace) -> BuildTrace {
        trace.total_duration_ms = self.started_at.elapsed().as_millis();
        trace.stages = self.stages;
        trace
    }
}

fn prepare_build_inputs<'a>(
    raw_meta: &MetaModel,
    target_plan: &TargetBuildPlan,
    target_tags: &TargetTags,
    data_root: Option<&Path>,
    pool: &'a ThreadPool,
) -> Result<BuildInputPreparation<'a>> {
    let read_sources = target_plan
        .requires_data()
        .then(|| source_reader_for_target(raw_meta, data_root, target_tags, pool))
        .transpose()?;
    Ok(BuildInputPreparation { read_sources })
}

struct ExportPipelineContext<'a, 'meta> {
    raw_meta: &'meta MetaModel,
    read_sources: Option<Box<dyn FnOnce() -> Result<RawSourceData> + Send + 'a>>,
    runtime_exports: &'a [(usize, &'a NormalizedExportTask)],
    meta_only_exports: &'a [(usize, &'a NormalizedExportTask)],
    project_root: &'a Path,
    project_info: &'a ArtifactProjectInfo,
    target_name: &'a str,
    assets: &'a crate::project::AssetsConfig,
    target_tags: &'a TargetTags,
    generated_at: &'a str,
    generator_registry: &'a GeneratorRegistry,
    pool: &'a ThreadPool,
    sink: &'a ArtifactSink,
}

fn execute_export_pipeline<'meta>(
    context: ExportPipelineContext<'_, 'meta>,
) -> Result<(ExportBranchOutput, ExportBranchOutput)> {
    let ExportPipelineContext {
        raw_meta,
        read_sources,
        runtime_exports,
        meta_only_exports,
        project_root,
        project_info,
        target_name,
        assets,
        target_tags,
        generated_at,
        generator_registry,
        pool,
        sink,
    } = context;
    let ((linked_meta, meta_only_output), raw_sources) = join_link_meta_only_and_source_read(
        || {
            let linked_meta = raw_meta.link().map_err(ConfigForgeError::MetaValidation)?;
            let meta_only_context = MetaOnlyGeneratorContext {
                project_root,
                project_info,
                target_name,
                target_tags,
                linked_meta: &linked_meta,
                generated_at,
                generator_registry,
                sink,
            };
            let meta_only_output =
                execute_meta_only_generator_nodes(&meta_only_context, meta_only_exports, pool)?;
            Ok((linked_meta, meta_only_output))
        },
        read_sources,
        pool,
    )?;

    let runtime_output = execute_runtime_export_nodes(
        raw_sources.as_ref(),
        runtime_exports,
        &linked_meta,
        assets,
        target_tags,
        generator_registry,
        pool,
        sink,
    )?;
    Ok((runtime_output, meta_only_output))
}

fn join_link_meta_only_and_source_read<'meta, LinkMetaOnlyFn, SourceFn>(
    link_and_generate_meta_only: LinkMetaOnlyFn,
    read_sources: Option<SourceFn>,
    pool: &ThreadPool,
) -> Result<(
    (LinkedMetaModel<'meta>, ExportBranchOutput),
    Option<RawSourceData>,
)>
where
    LinkMetaOnlyFn: FnOnce() -> Result<(LinkedMetaModel<'meta>, ExportBranchOutput)> + Send,
    SourceFn: FnOnce() -> Result<RawSourceData> + Send,
{
    if let Some(read_sources) = read_sources {
        let (link_meta_only_result, source_result) =
            pool.install(|| rayon::join(link_and_generate_meta_only, read_sources));
        let link_meta_only = link_meta_only_result?;
        let raw_sources = source_result?;
        Ok((link_meta_only, Some(raw_sources)))
    } else {
        Ok((link_and_generate_meta_only()?, None))
    }
}

fn source_reader_for_target<'a>(
    raw_meta: &MetaModel,
    data_root: Option<&Path>,
    target_tags: &TargetTags,
    pool: &'a ThreadPool,
) -> Result<Box<dyn FnOnce() -> Result<RawSourceData> + Send + 'a>> {
    let data_root = data_root.ok_or_else(|| {
        ConfigForgeError::Validation(
            "`project.input` must be set when target declares Data-dependent export tasks"
                .to_owned(),
        )
    })?;
    let source_read_plan = SourceReadPlan::from_tables(
        raw_meta
            .tables
            .iter()
            .filter(|table| table.is_exported_for(target_tags)),
        Some(target_tags),
    )?;
    let data_root = data_root.to_path_buf();
    Ok(Box::new(move || {
        DataLoader::new(&data_root).read_sources_with_pool(&source_read_plan, pool)
    }))
}

fn build_execution_view_detail(target: &NormalizedTargetConfig, plan: &TargetBuildPlan) -> String {
    let data_mode = if plan.requires_data() {
        "with_data"
    } else {
        "meta_only"
    };
    let exports = plan
        .export_indices_in_declaration_order()
        .iter()
        .map(|index| target.exports[*index].name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "target={}, mode={}, meta_only_exports={}, meta_and_data_exports={}, exports=[{}]",
        plan.target_name,
        data_mode,
        plan.meta_only_exports.len(),
        plan.meta_and_data_exports.len(),
        exports
    )
}

impl BuildDependencyPlan {
    fn for_target(
        target: &NormalizedTargetConfig,
        data_root: Option<&Path>,
        target_tags: &TargetTags,
    ) -> Self {
        Self {
            targets: vec![TargetBuildPlan::new(target, data_root, target_tags)],
        }
    }

    fn single_target_plan(&self) -> Result<&TargetBuildPlan> {
        match self.targets.as_slice() {
            [target] => Ok(target),
            _ => Err(ConfigForgeError::Validation(
                "expected exactly one target in build dependency plan".to_owned(),
            )),
        }
    }
}

impl TargetBuildPlan {
    fn new(
        target: &NormalizedTargetConfig,
        data_root: Option<&Path>,
        target_tags: &TargetTags,
    ) -> Self {
        let effective_tags = target_tags.iter().map(str::to_owned).collect::<Vec<_>>();
        let mut meta_only_exports = Vec::new();
        let mut meta_and_data_exports = Vec::new();
        for (index, task) in target.exports.iter().enumerate() {
            if task.requirements.needs_data {
                meta_and_data_exports.push(index);
            } else {
                meta_only_exports.push(index);
            }
        }
        let requires_meta = !target.exports.is_empty();
        let data_dependency = (!meta_and_data_exports.is_empty()).then(|| DataDependencyKey {
            input: data_root.map(Path::to_path_buf),
            effective_tags: effective_tags.clone(),
        });
        Self {
            target_name: target.name.clone(),
            effective_tags,
            requires_meta,
            data_dependency,
            meta_only_exports,
            meta_and_data_exports,
        }
    }

    fn requires_data(&self) -> bool {
        self.data_dependency.is_some()
    }

    fn meta_only_export_tasks<'a>(
        &'a self,
        target: &'a NormalizedTargetConfig,
    ) -> Vec<(usize, &'a NormalizedExportTask)> {
        self.export_tasks_by_indices(target, &self.meta_only_exports)
    }

    fn meta_and_data_export_tasks<'a>(
        &'a self,
        target: &'a NormalizedTargetConfig,
    ) -> Vec<(usize, &'a NormalizedExportTask)> {
        self.export_tasks_by_indices(target, &self.meta_and_data_exports)
    }

    fn export_tasks_by_indices<'a>(
        &'a self,
        target: &'a NormalizedTargetConfig,
        indices: &[usize],
    ) -> Vec<(usize, &'a NormalizedExportTask)> {
        indices
            .iter()
            .map(|index| (*index, &target.exports[*index]))
            .collect()
    }

    fn export_indices_in_declaration_order(&self) -> Vec<usize> {
        let mut indices =
            Vec::with_capacity(self.meta_only_exports.len() + self.meta_and_data_exports.len());
        indices.extend(self.meta_only_exports.iter().copied());
        indices.extend(self.meta_and_data_exports.iter().copied());
        indices.sort_unstable();
        indices
    }
}

fn execute_runtime_export_nodes(
    raw_sources: Option<&RawSourceData>,
    export_tasks: &[(usize, &NormalizedExportTask)],
    linked_meta: &crate::meta::LinkedMetaModel<'_>,
    assets: &crate::project::AssetsConfig,
    target_tags: &TargetTags,
    generator_registry: &GeneratorRegistry,
    pool: &ThreadPool,
    sink: &ArtifactSink,
) -> Result<ExportBranchOutput> {
    let meta_and_data_tasks = export_tasks
        .iter()
        .map(|(_, task)| *task)
        .collect::<Vec<_>>();
    let output = export_meta_and_data_tasks(
        raw_sources,
        &meta_and_data_tasks,
        linked_meta,
        assets,
        target_tags,
        generator_registry,
        pool,
        sink,
    )?;
    if output.task_outputs.len() != export_tasks.len() {
        return Err(ConfigForgeError::Validation(
            "runtime exporter returned an unexpected number of task outputs".to_owned(),
        ));
    }

    for ((_, task), task_output) in export_tasks.iter().copied().zip(output.task_outputs) {
        if task_output.export_name != task.name {
            return Err(ConfigForgeError::Validation(format!(
                "runtime exporter returned output for `{}` while `{}` was expected",
                task_output.export_name, task.name
            )));
        }
    }

    Ok(ExportBranchOutput {
        warnings: output.warnings,
    })
}

fn execute_meta_only_generator_nodes(
    context: &MetaOnlyGeneratorContext<'_, '_>,
    export_tasks: &[(usize, &NormalizedExportTask)],
    pool: &ThreadPool,
) -> Result<ExportBranchOutput> {
    let mut generated_by_task = pool.install(|| {
        export_tasks
            .par_iter()
            .map(|(index, export_task)| {
                (
                    *index,
                    generate_and_write_meta_only_task(context, export_task),
                )
            })
            .collect::<Vec<_>>()
    });
    generated_by_task.sort_by_key(|(index, _)| *index);
    for (_, result) in generated_by_task {
        result?;
    }

    Ok(ExportBranchOutput {
        warnings: Vec::new(),
    })
}

struct MetaOnlyGeneratorContext<'a, 'meta> {
    project_root: &'a Path,
    project_info: &'a ArtifactProjectInfo,
    target_name: &'a str,
    target_tags: &'a TargetTags,
    linked_meta: &'meta crate::meta::LinkedMetaModel<'meta>,
    generated_at: &'a str,
    generator_registry: &'a GeneratorRegistry,
    sink: &'a ArtifactSink,
}

fn generate_and_write_meta_only_task(
    context: &MetaOnlyGeneratorContext<'_, '_>,
    export_task: &NormalizedExportTask,
) -> Result<()> {
    debug_assert!(!export_task.requirements.needs_data);
    let generator = context
        .generator_registry
        .require_generator(&export_task.type_id)?;
    if generator.requirements().needs_data {
        return Err(ConfigForgeError::Validation(format!(
            "target `{}` export task `{}` requires Data but was scheduled in the Meta-only branch",
            context.target_name, export_task.name
        )));
    }
    let input = GeneratorTaskInput {
        common: build_generator_common_input(context, export_task),
        prepared: PreparedGeneratorInputs::MetaOnly,
    };
    generator.generate(&input, context.sink)
}

fn build_generator_common_input<'meta>(
    context: &MetaOnlyGeneratorContext<'_, 'meta>,
    task: &NormalizedExportTask,
) -> GeneratorCommonInput<'meta> {
    GeneratorCommonInput {
        meta: context.linked_meta,
        target_tags: context.target_tags.clone(),
        project: context.project_info.clone(),
        project_root: context.project_root.to_path_buf(),
        target_name: context.target_name.to_owned(),
        task_name: task.name.clone(),
        type_id: task.type_id.clone(),
        output: context.project_root.join(&task.output),
        options: task.options.clone(),
        generated_at: context.generated_at.to_owned(),
    }
}

fn generated_at() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_unix_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = seconds_of_day % 3_600 / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_unix_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_piece = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_piece + 2) / 5 + 1;
    let month = month_piece + if month_piece < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
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

    use crate::artifact::{ArtifactWriteMode, ArtifactWriter};
    use crate::codegen::{CodeExportInput, CodeGenerator, default_module_name};
    use crate::data::{DataSet, DataTable, RawSourceData};
    use crate::exporter::build_exported_table_payload;
    use crate::meta::{MetaModel, TargetTags};

    use super::*;

    #[test]
    fn maps_language_to_default_module_name() {
        assert_eq!(default_module_name("rust"), Some("config_forge_generated"));
        assert_eq!(default_module_name("cpp"), Some("config_forge_generated"));
        assert_eq!(
            default_module_name("typescript"),
            Some("configForgeGenerated")
        );
    }

    #[test]
    fn converts_unix_days_to_civil_date() {
        assert_eq!(civil_from_unix_days(0), (1970, 1, 1));
        assert_eq!(civil_from_unix_days(20_590), (2026, 5, 17));
    }

    #[test]
    fn exported_table_payload_omits_non_runtime_fields_and_full_meta() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "debug_note"
                type = "string"
                export_when = { any = ["debug"] }

                [[tables.fields]]
                name = "validator_flag"
                type = "bool"
                export_when = { any = ["validator"] }

                [[tables.fields]]
                name = "internal_value"
                type = "string"
                export = false
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("items").expect("linked table");
        let table = DataTable::new(
            "items",
            vec![BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                ("debug_note".to_owned(), "debug".to_owned()),
                ("validator_flag".to_owned(), "true".to_owned()),
                ("internal_value".to_owned(), "secret".to_owned()),
            ])],
        );

        let payload = build_exported_table_payload(table_meta, &table, &TargetTags::empty())
            .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert_eq!(value["meta"]["name"], "items");
        assert!(value["meta"].get("fields").is_none());
        assert_eq!(value["rows"][0]["id"], "1");
        assert!(value["rows"][0].get("debug_note").is_none());
        assert!(value["rows"][0].get("validator_flag").is_none());
        assert!(value["rows"][0].get("internal_value").is_none());
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
                    meta.link().map_err(ConfigForgeError::MetaValidation)?,
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
                let linked_meta = meta.link().map_err(ConfigForgeError::MetaValidation)?;
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
            || Err(ConfigForgeError::Validation("meta failed".to_owned())),
            Some(|| Err(ConfigForgeError::Validation("source failed".to_owned()))),
            &pool,
        );
        let error = match result {
            Ok(_) => panic!("parallel preparation should fail"),
            Err(error) => error,
        };

        assert!(matches!(error, ConfigForgeError::Validation(message) if message == "meta failed"));
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
            Err(ConfigForgeError::Validation("boom".to_owned()))
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

        assert!(matches!(
            result,
            Err(ConfigForgeError::Validation(message))
                if message.contains("no artifact generator registered")
        ));
        let run_exporters_stage = trace
            .stages
            .iter()
            .find(|stage| stage.name == "run_exporters")
            .expect("run_exporters stage should be recorded");
        assert_eq!(run_exporters_stage.status, BuildTraceStageStatus::Failed);
    }

    #[test]
    fn meta_only_code_generator_writes_via_sink_first_override() {
        struct SinkOnlyGenerator {
            called: Arc<AtomicBool>,
        }

        impl CodeGenerator for SinkOnlyGenerator {
            fn language(&self) -> &'static str {
                "sink_only"
            }

            fn default_naming(&self) -> crate::codegen::NamingConfig {
                crate::codegen::NamingConfig {
                    type_name: crate::codegen::CaseStyle::PascalCase,
                    enum_value: crate::codegen::CaseStyle::ScreamingSnakeCase,
                    field: crate::codegen::CaseStyle::SnakeCase,
                    function: crate::codegen::CaseStyle::SnakeCase,
                    constant: crate::codegen::CaseStyle::ScreamingSnakeCase,
                }
            }

            fn generate(
                &self,
                _input: &CodeExportInput<'_>,
            ) -> Result<Vec<crate::codegen::GeneratedFile>> {
                panic!("build must call generate_to_sink instead of legacy generate fallback")
            }

            fn generate_to_sink(
                &self,
                input: &CodeExportInput<'_>,
                sink: &ArtifactSink,
            ) -> Result<()> {
                self.called.store(true, Ordering::SeqCst);
                sink.write_text(input.target.out_dir.join("generated.txt"), "sink-first")
            }
        }

        let meta = minimal_meta_model();
        let linked_meta = meta.link().expect("minimal meta should link");
        let target_tags = TargetTags::empty();
        let project_info = ArtifactProjectInfo::default();
        let called = Arc::new(AtomicBool::new(false));
        let generators = vec![Box::new(SinkOnlyGenerator {
            called: called.clone(),
        }) as Box<dyn CodeGenerator>];
        let generator_registry = GeneratorRegistry::new(
            crate::exporter::DataFormatRegistry::new(),
            generators,
            Vec::new(),
        );
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
