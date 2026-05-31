// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    path::Path,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use rayon::ThreadPool;
use rayon::prelude::*;

mod options;
mod runtime;
mod target;

pub use options::{BuildOptions, BuildOutput, ExecutionOptions};

use crate::{
    ConfigForgeError, Result,
    artifact::{Artifact, ArtifactWriter},
    codegen::{
        CodeExportInput, CodeExportProject, CodeExportTarget, CodeGenerator, GeneratedFile,
        builtin_generators,
    },
    data::{DataLoader, RawSourceData, SourceReadPlan},
    meta::{LinkedMetaModel, MetaModel, TargetTags},
    project::ProjectConfig,
    runtime_contract::ExportDependency,
};

use runtime::{RuntimeConfigForgeResult, export_meta_and_data_tasks};
use target::{NormalizedExportTask, NormalizedTargetConfig, codegen_task_config_from_export_task};

pub fn build_target(options: BuildOptions<'_>) -> Result<BuildOutput> {
    let cpu_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.execution.jobs)
        .thread_name(|index| format!("config-forge-cpu-{index}"))
        .build()
        .map_err(|source| {
            ConfigForgeError::Validation(format!("failed to create CPU worker pool: {source}"))
        })?;
    let project = ProjectConfig::from_path(options.project_path)?;
    let project_root = options
        .project_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let target = project.target(options.target_name)?;
    let target = target.normalize()?;
    let meta_set = project.raw_meta_model();
    let target_tags = target.effective_target_tags(options.extra_tags);

    let dag_output = execute_target_dag(TargetDagExecutionContext {
        project_root,
        project: &CodeExportProject::from(&project.project),
        target: &target,
        target_tags: &target_tags,
        raw_meta: &meta_set,
        data_root: project.project.input.as_deref(),
        pool: &cpu_pool,
    })?;

    let artifacts = ArtifactWriter::new(options.artifact_write_mode).write(dag_output.artifacts)?;

    Ok(BuildOutput {
        warnings: dag_output.warnings,
        artifacts,
    })
}

struct TargetDagExecutionContext<'a, 'meta> {
    project_root: &'a Path,
    project: &'a CodeExportProject,
    target: &'a NormalizedTargetConfig,
    target_tags: &'a TargetTags,
    raw_meta: &'meta MetaModel,
    data_root: Option<&'a Path>,
    pool: &'a ThreadPool,
}

struct BuildInputPreparation<'meta> {
    linked_meta: LinkedMetaModel<'meta>,
    raw_sources: Option<RawSourceData>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum BuildDagNodeKey {
    LinkMeta,
    EffectiveTargetTags {
        target_name: String,
        effective_tags: Vec<String>,
    },
    DataLoadPlan {
        input: Option<PathBuf>,
        effective_tags: Vec<String>,
    },
    NormalizeAndValidate {
        input: Option<PathBuf>,
        effective_tags: Vec<String>,
    },
    RunExporter {
        target_name: String,
        export_name: String,
    },
    WriteArtifacts,
}

struct TargetDagOutput {
    warnings: Vec<String>,
    artifacts: Vec<Artifact>,
}

struct TargetDagExportOutput {
    declaration_index: usize,
    artifacts: Vec<Artifact>,
}

struct DagBranchOutput {
    warnings: Vec<String>,
    export_outputs: Vec<TargetDagExportOutput>,
    #[allow(dead_code)]
    runtime_results: Vec<RuntimeConfigForgeResult>,
}

fn execute_target_dag(context: TargetDagExecutionContext<'_, '_>) -> Result<TargetDagOutput> {
    let dependency_plan =
        BuildDependencyPlan::for_target(context.target, context.data_root, context.target_tags);
    let target_plan = dependency_plan.single_target_plan()?;
    // This node list is an execution-view skeleton for tests and future tracing/executor work.
    // The current MVP below still executes two dependency branches directly instead of scheduling
    // these node keys through a generic DAG executor.
    let _execution_view_nodes = build_target_local_dag(context.target, target_plan);
    let meta_and_data_exports =
        target_plan.export_tasks(context.target, ExportDependency::MetaAndData);
    let meta_only_exports = target_plan.export_tasks(context.target, ExportDependency::MetaOnly);
    let prepared_inputs = prepare_build_inputs(
        context.raw_meta,
        target_plan,
        context.target_tags,
        context.data_root,
        context.pool,
    )?;

    let generated_at = generated_at();
    let generators = builtin_generators();
    let codegen_context = CodegenTaskContext {
        project_root: context.project_root,
        project: context.project,
        target_tags: context.target_tags,
        linked_meta: &prepared_inputs.linked_meta,
        generated_at: &generated_at,
        generators: &generators,
    };

    let (runtime_output, code_output) = context.pool.install(|| {
        rayon::join(
            || {
                execute_runtime_export_nodes(
                    prepared_inputs.raw_sources.as_ref(),
                    &meta_and_data_exports,
                    &prepared_inputs.linked_meta,
                    context.target_tags,
                    context.pool,
                )
            },
            || execute_codegen_export_nodes(&codegen_context, &meta_only_exports, context.pool),
        )
    });

    let runtime_output = runtime_output?;
    let code_output = code_output?;
    let mut export_outputs = runtime_output.export_outputs;
    export_outputs.extend(code_output.export_outputs);
    let artifacts = collect_export_artifacts_in_declaration_order(export_outputs);

    let mut warnings = runtime_output.warnings;
    warnings.extend(code_output.warnings);

    Ok(TargetDagOutput {
        warnings,
        artifacts,
    })
}

fn prepare_build_inputs<'meta>(
    raw_meta: &'meta MetaModel,
    target_plan: &TargetBuildPlan,
    target_tags: &TargetTags,
    data_root: Option<&Path>,
    pool: &ThreadPool,
) -> Result<BuildInputPreparation<'meta>> {
    let read_sources = target_plan
        .requires_data()
        .then_some(|| read_sources_for_target(raw_meta, data_root, target_tags, pool));
    let (linked_meta, raw_sources) = join_meta_link_and_source_read(
        || raw_meta.link().map_err(ConfigForgeError::MetaValidation),
        read_sources,
        pool,
    )?;
    Ok(BuildInputPreparation {
        linked_meta,
        raw_sources,
    })
}

fn join_meta_link_and_source_read<'meta, LinkFn, SourceFn>(
    link_meta: LinkFn,
    read_sources: Option<SourceFn>,
    pool: &ThreadPool,
) -> Result<(LinkedMetaModel<'meta>, Option<RawSourceData>)>
where
    LinkFn: FnOnce() -> Result<LinkedMetaModel<'meta>> + Send,
    SourceFn: FnOnce() -> Result<RawSourceData> + Send,
{
    if let Some(read_sources) = read_sources {
        let (meta_result, source_result) = pool.install(|| rayon::join(link_meta, read_sources));
        let linked_meta = meta_result?;
        let raw_sources = source_result?;
        Ok((linked_meta, Some(raw_sources)))
    } else {
        Ok((link_meta()?, None))
    }
}

fn read_sources_for_target(
    raw_meta: &MetaModel,
    data_root: Option<&Path>,
    target_tags: &TargetTags,
    pool: &ThreadPool,
) -> Result<RawSourceData> {
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
    DataLoader::new(data_root).read_sources_with_pool(&source_read_plan, pool)
}

fn build_target_local_dag(
    target: &NormalizedTargetConfig,
    plan: &TargetBuildPlan,
) -> Vec<BuildDagNodeKey> {
    let mut nodes = Vec::new();
    if plan.requires_meta {
        nodes.push(BuildDagNodeKey::LinkMeta);
    }
    nodes.push(BuildDagNodeKey::EffectiveTargetTags {
        target_name: plan.target_name.clone(),
        effective_tags: plan.effective_tags.clone(),
    });
    if plan.requires_data() {
        let data_dependency = plan
            .data_dependency
            .as_ref()
            .expect("requires_data implies a data dependency key");
        nodes.push(BuildDagNodeKey::DataLoadPlan {
            input: data_dependency.input.clone(),
            effective_tags: data_dependency.effective_tags.clone(),
        });
        nodes.push(BuildDagNodeKey::NormalizeAndValidate {
            input: data_dependency.input.clone(),
            effective_tags: data_dependency.effective_tags.clone(),
        });
    }
    nodes.extend(
        plan.export_indices_in_declaration_order()
            .iter()
            .map(|index| BuildDagNodeKey::RunExporter {
                target_name: plan.target_name.clone(),
                export_name: target.exports[*index].name.clone(),
            }),
    );
    nodes.push(BuildDagNodeKey::WriteArtifacts);
    nodes
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
            match task.dependency {
                ExportDependency::MetaOnly => meta_only_exports.push(index),
                ExportDependency::MetaAndData => meta_and_data_exports.push(index),
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

    fn export_tasks<'a>(
        &'a self,
        target: &'a NormalizedTargetConfig,
        dependency: ExportDependency,
    ) -> Vec<(usize, &'a NormalizedExportTask)> {
        let indices = match dependency {
            ExportDependency::MetaOnly => &self.meta_only_exports,
            ExportDependency::MetaAndData => &self.meta_and_data_exports,
        };
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
    target_tags: &TargetTags,
    pool: &ThreadPool,
) -> Result<DagBranchOutput> {
    let meta_and_data_tasks = export_tasks
        .iter()
        .map(|(_, task)| *task)
        .collect::<Vec<_>>();
    let output = export_meta_and_data_tasks(
        raw_sources,
        &meta_and_data_tasks,
        linked_meta,
        target_tags,
        pool,
    )?;
    if output.task_outputs.len() != export_tasks.len() {
        return Err(ConfigForgeError::Validation(
            "runtime exporter returned an unexpected number of task outputs".to_owned(),
        ));
    }

    let mut runtime_results = Vec::new();
    let mut export_outputs = Vec::new();
    for ((declaration_index, task), task_output) in
        export_tasks.iter().copied().zip(output.task_outputs)
    {
        if task_output.export_name != task.name {
            return Err(ConfigForgeError::Validation(format!(
                "runtime exporter returned output for `{}` while `{}` was expected",
                task_output.export_name, task.name
            )));
        }
        if let Some(result) = task_output.result {
            runtime_results.push(result);
        }
        export_outputs.push(TargetDagExportOutput {
            declaration_index,
            artifacts: task_output.artifacts,
        });
    }

    Ok(DagBranchOutput {
        warnings: output.warnings,
        export_outputs,
        runtime_results,
    })
}

fn execute_codegen_export_nodes(
    context: &CodegenTaskContext<'_, '_>,
    export_tasks: &[(usize, &NormalizedExportTask)],
    pool: &ThreadPool,
) -> Result<DagBranchOutput> {
    let mut generated_by_task = pool.install(|| {
        export_tasks
            .par_iter()
            .map(|(index, export_task)| (*index, generate_code_task(context, export_task)))
            .collect::<Vec<_>>()
    });
    generated_by_task.sort_by_key(|(index, _)| *index);
    let mut export_outputs = Vec::new();
    for (declaration_index, result) in generated_by_task {
        let artifacts = result?
            .into_iter()
            .map(|file| Artifact::text(file.path, file.content))
            .collect();
        export_outputs.push(TargetDagExportOutput {
            declaration_index,
            artifacts,
        });
    }

    Ok(DagBranchOutput {
        warnings: Vec::new(),
        export_outputs,
        runtime_results: Vec::new(),
    })
}

fn collect_export_artifacts_in_declaration_order(
    mut export_outputs: Vec<TargetDagExportOutput>,
) -> Vec<Artifact> {
    export_outputs.sort_by_key(|output| output.declaration_index);
    export_outputs
        .into_iter()
        .flat_map(|output| output.artifacts)
        .collect()
}

struct CodegenTaskContext<'a, 'meta> {
    project_root: &'a Path,
    project: &'a CodeExportProject,
    target_tags: &'a TargetTags,
    linked_meta: &'meta crate::meta::LinkedMetaModel<'meta>,
    generated_at: &'a str,
    generators: &'a [Box<dyn CodeGenerator>],
}

fn generate_code_task(
    context: &CodegenTaskContext<'_, '_>,
    export_task: &NormalizedExportTask,
) -> Result<Vec<GeneratedFile>> {
    debug_assert_eq!(export_task.dependency, ExportDependency::MetaOnly);
    let task = codegen_task_config_from_export_task(export_task)?;
    let generator =
        crate::codegen::find_generator(context.generators, &task.language).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "no code generator registered for `{}`",
                task.language
            ))
        })?;
    let mut code_target = CodeExportTarget::from_task(task, generator)?;
    code_target.out_dir = context.project_root.join(&code_target.out_dir);
    let input = CodeExportInput {
        meta: context.linked_meta,
        target_tags: context.target_tags.clone(),
        project: context.project.clone(),
        target: code_target,
        runtime_data_files: Vec::new(),
        generated_at: context.generated_at.to_owned(),
    };
    generator.generate(&input)
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

    use crate::artifact::ArtifactContent;
    use crate::codegen::default_module_name;
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
    fn target_local_dag_includes_shared_data_nodes_once() {
        let target = NormalizedTargetConfig {
            name: "client".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![
                NormalizedExportTask {
                    name: "rust".to_owned(),
                    type_id: "rust".to_owned(),
                    dependency: ExportDependency::MetaOnly,
                    output: PathBuf::from("out/code"),
                    options: BTreeMap::new(),
                },
                NormalizedExportTask {
                    name: "json".to_owned(),
                    type_id: "json".to_owned(),
                    dependency: ExportDependency::MetaAndData,
                    output: PathBuf::from("out/json"),
                    options: BTreeMap::new(),
                },
                NormalizedExportTask {
                    name: "messagepack".to_owned(),
                    type_id: "messagepack".to_owned(),
                    dependency: ExportDependency::MetaAndData,
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

        let nodes = build_target_local_dag(&target, &plan);

        assert_eq!(
            nodes
                .iter()
                .filter(|node| matches!(node, BuildDagNodeKey::DataLoadPlan { .. }))
                .count(),
            1
        );
        assert_eq!(
            nodes
                .iter()
                .filter(|node| matches!(node, BuildDagNodeKey::NormalizeAndValidate { .. }))
                .count(),
            1
        );
        assert_eq!(
            nodes
                .iter()
                .filter(|node| matches!(node, BuildDagNodeKey::RunExporter { .. }))
                .count(),
            3
        );
    }

    #[test]
    fn target_local_dag_skips_data_nodes_for_meta_only_exports() {
        let target = NormalizedTargetConfig {
            name: "client".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![NormalizedExportTask {
                name: "rust".to_owned(),
                type_id: "rust".to_owned(),
                dependency: ExportDependency::MetaOnly,
                output: PathBuf::from("out/code"),
                options: BTreeMap::new(),
            }],
        };
        let plan = TargetBuildPlan::new(&target, None, &TargetTags::new(["client"]));

        let nodes = build_target_local_dag(&target, &plan);

        assert!(
            nodes
                .iter()
                .all(|node| !matches!(node, BuildDagNodeKey::DataLoadPlan { .. }))
        );
        assert!(
            nodes
                .iter()
                .all(|node| !matches!(node, BuildDagNodeKey::NormalizeAndValidate { .. }))
        );
    }

    #[test]
    fn target_build_plan_classifies_rust_only_as_meta_only() {
        let target = NormalizedTargetConfig {
            name: "code_only".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![export_task("rust", ExportDependency::MetaOnly)],
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
            exports: vec![export_task("json", ExportDependency::MetaAndData)],
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
                export_task("json", ExportDependency::MetaAndData),
                export_task("rust", ExportDependency::MetaOnly),
                export_task("lua", ExportDependency::MetaAndData),
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
            plan.export_tasks(&target, ExportDependency::MetaOnly)
                .into_iter()
                .map(|(index, task)| (index, task.name.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "rust")]
        );
        assert_eq!(
            plan.export_tasks(&target, ExportDependency::MetaAndData)
                .into_iter()
                .map(|(index, task)| (index, task.name.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "json"), (2, "lua")]
        );
    }

    #[test]
    fn target_local_dag_follows_target_build_plan_order() {
        let target = NormalizedTargetConfig {
            name: "client".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![
                export_task("json", ExportDependency::MetaAndData),
                export_task("rust", ExportDependency::MetaOnly),
                export_task("lua", ExportDependency::MetaAndData),
            ],
        };
        let plan = TargetBuildPlan::new(
            &target,
            Some(Path::new("Data")),
            &TargetTags::new(["client"]),
        );

        let nodes = build_target_local_dag(&target, &plan);

        assert_eq!(
            nodes
                .iter()
                .filter_map(|node| match node {
                    BuildDagNodeKey::RunExporter { export_name, .. } => Some(export_name.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec!["json", "rust", "lua"]
        );
    }

    #[test]
    fn meta_link_and_source_read_overlap_before_normalize_join() {
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

        let (linked_meta, raw_sources) = join_meta_link_and_source_read(
            || {
                thread::sleep(Duration::from_millis(40));
                if source_started_for_link.load(Ordering::SeqCst) {
                    link_observed_source_for_link.store(true, Ordering::SeqCst);
                }
                thread::sleep(Duration::from_millis(40));
                meta.link().map_err(ConfigForgeError::MetaValidation)
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
        assert!(raw_sources.is_some());
        assert!(link_observed_source.load(Ordering::SeqCst));
        assert!(started_at.elapsed() < Duration::from_millis(140));
    }

    #[test]
    fn meta_link_error_wins_when_parallel_source_read_also_fails() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool should build");
        let result = join_meta_link_and_source_read(
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
            exports: vec![export_task("rust", ExportDependency::MetaOnly)],
        };
        let plan = TargetBuildPlan::new(&target, None, &TargetTags::new(["client"]));

        let prepared =
            prepare_build_inputs(&meta, &plan, &TargetTags::new(["client"]), None, &pool)
                .expect("code-only preparation should not require project.input");

        assert!(prepared.linked_meta.table("items").is_some());
        assert!(prepared.raw_sources.is_none());
    }

    #[test]
    fn collect_export_artifacts_preserves_declaration_order() {
        let artifacts = collect_export_artifacts_in_declaration_order(vec![
            TargetDagExportOutput {
                declaration_index: 2,
                artifacts: vec![Artifact::text("c.txt", "c")],
            },
            TargetDagExportOutput {
                declaration_index: 0,
                artifacts: vec![Artifact::text("a.txt", "a1"), Artifact::text("b.txt", "a2")],
            },
            TargetDagExportOutput {
                declaration_index: 1,
                artifacts: vec![Artifact::binary("bin.dat", [1_u8, 2])],
            },
        ]);

        assert_eq!(
            artifacts
                .iter()
                .map(|artifact| artifact.path.as_path())
                .collect::<Vec<_>>(),
            vec![
                Path::new("a.txt"),
                Path::new("b.txt"),
                Path::new("bin.dat"),
                Path::new("c.txt"),
            ]
        );
        assert!(matches!(artifacts[2].content, ArtifactContent::Binary(_)));
    }

    #[test]
    fn collect_export_artifacts_preserves_mixed_export_declaration_order() {
        let artifacts = collect_export_artifacts_in_declaration_order(vec![
            TargetDagExportOutput {
                declaration_index: 3,
                artifacts: vec![Artifact::text("lua/init.lua", "return {}")],
            },
            TargetDagExportOutput {
                declaration_index: 1,
                artifacts: vec![Artifact::text("code/generated.rs", "pub struct GameData;")],
            },
            TargetDagExportOutput {
                declaration_index: 0,
                artifacts: vec![
                    Artifact::text("data/items.json", "{}"),
                    Artifact::text("data/game_rules.json", "{}"),
                ],
            },
            TargetDagExportOutput {
                declaration_index: 2,
                artifacts: vec![Artifact::text("i18n/messages.pot", "")],
            },
        ]);

        assert_eq!(
            artifacts
                .iter()
                .map(|artifact| artifact.path.as_path())
                .collect::<Vec<_>>(),
            vec![
                Path::new("data/items.json"),
                Path::new("data/game_rules.json"),
                Path::new("code/generated.rs"),
                Path::new("i18n/messages.pot"),
                Path::new("lua/init.lua"),
            ]
        );
    }

    fn export_task(name: &str, dependency: ExportDependency) -> NormalizedExportTask {
        NormalizedExportTask {
            name: name.to_owned(),
            type_id: name.to_owned(),
            dependency,
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
}
