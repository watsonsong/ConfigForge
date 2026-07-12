// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use rayon::{ThreadPool, prelude::*};

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Result,
    artifact::ArtifactSink,
    data::{DataLoader, RawSourceData, SourceReadPlan},
    diagnostic::code::CF_BUILD_EXPORT_TASK_FAILED,
    generator::{
        GeneratorCommonInput, GeneratorCommonInputContext, GeneratorRegistry, GeneratorTaskInput,
        PreparedGeneratorInputs,
    },
    meta::{LinkedMetaModel, MetaModel, TargetTags},
};

use super::{
    build_diagnostic_error, input_required_error,
    plan::{BuildDependencyPlan, TargetBuildPlan, build_execution_view_detail},
    runtime::export_meta_and_data_tasks,
    target::{NormalizedExportTask, NormalizedTargetConfig},
    trace::{BuildTraceRecorder, generated_at},
};

pub(super) struct TargetPipelineExecutionContext<'a, 'meta> {
    pub(super) project_root: &'a Path,
    pub(super) project_info: &'a ArtifactProjectInfo,
    pub(super) assets: &'a crate::project::AssetsConfig,
    pub(super) target: &'a NormalizedTargetConfig,
    pub(super) target_tags: &'a TargetTags,
    pub(super) raw_meta: &'meta MetaModel,
    pub(super) data_root: Option<&'a Path>,
    pub(super) pool: &'a ThreadPool,
    pub(super) sink: &'a ArtifactSink,
    pub(super) trace: &'a mut BuildTraceRecorder,
}

pub(super) struct BuildInputPreparation<'a> {
    pub(super) read_sources: Option<Box<dyn FnOnce() -> Result<RawSourceData> + Send + 'a>>,
}

pub(super) struct TargetPipelineOutput {
    pub(super) warnings: Vec<String>,
}

pub(super) struct ExportBranchOutput {
    pub(super) warnings: Vec<String>,
}

pub(super) fn execute_target_pipeline(
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

pub(super) fn prepare_build_inputs<'a>(
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
            let linked_meta = raw_meta
                .link()
                .map_err(crate::meta::meta_diagnostics_to_report)
                .map_err(ConfigForgeError::MetaValidation)?;
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
        project_root,
        project_info,
        target_name,
        generated_at,
        generator_registry,
        pool,
        sink,
    )?;
    Ok((runtime_output, meta_only_output))
}

pub(super) fn join_link_meta_only_and_source_read<'meta, LinkMetaOnlyFn, SourceFn>(
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
    let data_root = data_root.ok_or_else(|| input_required_error(target_tags))?;
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

#[allow(clippy::too_many_arguments)]
fn execute_runtime_export_nodes(
    raw_sources: Option<&RawSourceData>,
    export_tasks: &[(usize, &NormalizedExportTask)],
    linked_meta: &crate::meta::LinkedMetaModel<'_>,
    assets: &crate::project::AssetsConfig,
    target_tags: &TargetTags,
    project_root: &Path,
    project_info: &ArtifactProjectInfo,
    target_name: &str,
    generated_at: &str,
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
        project_root,
        project_info,
        target_name,
        generated_at,
        generator_registry,
        pool,
        sink,
    )?;
    if output.task_outputs.len() != export_tasks.len() {
        return Err(build_diagnostic_error(
            CF_BUILD_EXPORT_TASK_FAILED,
            "build.runtime.output",
            "runtime exporter returned an unexpected number of task outputs".to_owned(),
            output.task_outputs.len().to_string(),
            format!("{} runtime export task outputs", export_tasks.len()),
            "Check runtime export task scheduling and output aggregation.",
        ));
    }

    for ((_, task), task_output) in export_tasks.iter().copied().zip(output.task_outputs) {
        if task_output.export_name != task.name {
            return Err(build_diagnostic_error(
                CF_BUILD_EXPORT_TASK_FAILED,
                "build.runtime.output",
                format!(
                    "runtime exporter returned output for `{}` while `{}` was expected",
                    task_output.export_name, task.name
                ),
                task_output.export_name,
                task.name.clone(),
                "Check runtime export task output ordering and names.",
            ));
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

pub(super) struct MetaOnlyGeneratorContext<'a, 'meta> {
    pub(super) project_root: &'a Path,
    pub(super) project_info: &'a ArtifactProjectInfo,
    pub(super) target_name: &'a str,
    pub(super) target_tags: &'a TargetTags,
    pub(super) linked_meta: &'meta crate::meta::LinkedMetaModel<'meta>,
    pub(super) generated_at: &'a str,
    pub(super) generator_registry: &'a GeneratorRegistry,
    pub(super) sink: &'a ArtifactSink,
}

pub(super) fn generate_and_write_meta_only_task(
    context: &MetaOnlyGeneratorContext<'_, '_>,
    export_task: &NormalizedExportTask,
) -> Result<()> {
    debug_assert!(!export_task.requirements.needs_data);
    let generator = context
        .generator_registry
        .require_generator(&export_task.type_id)?;
    if generator.requirements().needs_data {
        return Err(build_diagnostic_error(
            CF_BUILD_EXPORT_TASK_FAILED,
            "build.meta_only.scheduler",
            format!(
                "target `{}` export task `{}` requires Data but was scheduled in the Meta-only branch",
                context.target_name, export_task.name
            ),
            format!("{}:{}", context.target_name, export_task.name),
            "Data-dependent export task scheduled in runtime branch",
            "Check generator requirements and target build plan classification.",
        ));
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
    GeneratorCommonInput::from_task_context(
        GeneratorCommonInputContext {
            linked_meta: context.linked_meta,
            target_tags: context.target_tags,
            project_root: context.project_root,
            project_info: context.project_info,
            target_name: context.target_name,
            generated_at: context.generated_at,
        },
        &task.name,
        &task.type_id,
        &task.output,
        &task.options,
    )
}
