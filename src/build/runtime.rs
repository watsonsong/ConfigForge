// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use rayon::{ThreadPool, prelude::*};
use std::collections::BTreeSet;

use crate::{
    ConfigForgeError, Result,
    artifact::ArtifactSink,
    asset::AssetResolver,
    data::{DataRow, DataSet, DataTable, RawSourceData},
    diagnostic::code::{
        CF_ARTIFACT_PATH_COLLISION, CF_BUILD_EXPORT_TASK_FAILED, CF_BUILD_INPUT_REQUIRED,
        CF_BUILD_INVALID_CONFIG,
    },
    diagnostic::{Diagnostic, DiagnosticReport},
    generator::{
        GeneratorCommonInput, GeneratorRegistry, GeneratorTaskInput, PreparedGeneratorInputs,
    },
    meta::{LinkedField, LinkedFieldMeta, LinkedMetaModel, LinkedStructType, TargetTags},
    project::AssetsConfig,
    project::lexical_normalize_path,
    validation::{
        DesignerReportFormat, DesignerReportOptions, DesignerValidationReport,
        ValidationReportExportConfig, normalize_data_set_with_pool,
        render_designer_validation_report,
        validate_normalized_data_set_for_target_with_assets_report_and_pool,
    },
};

use super::{build_diagnostic_error, target::NormalizedExportTask};

#[derive(Debug)]
pub(super) struct RuntimeExportOutput {
    pub(super) warnings: Vec<String>,
    pub(super) task_outputs: Vec<RuntimeExportTaskOutput>,
}

#[derive(Debug)]
pub(super) struct RuntimeExportTaskOutput {
    pub(super) export_name: String,
}

pub(super) fn export_meta_and_data_tasks(
    raw_sources: Option<&RawSourceData>,
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    generator_registry: &GeneratorRegistry,
    pool: &ThreadPool,
    sink: &ArtifactSink,
) -> Result<RuntimeExportOutput> {
    export_meta_and_data_tasks_with_generator_registry(
        raw_sources,
        export_tasks,
        linked_meta,
        assets,
        target_tags,
        generator_registry,
        pool,
        sink,
    )
}

fn export_meta_and_data_tasks_with_generator_registry(
    raw_sources: Option<&RawSourceData>,
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    generator_registry: &GeneratorRegistry,
    pool: &ThreadPool,
    sink: &ArtifactSink,
) -> Result<RuntimeExportOutput> {
    if export_tasks.is_empty() {
        return Ok(RuntimeExportOutput {
            warnings: Vec::new(),
            task_outputs: Vec::new(),
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

    ensure_planned_runtime_output_paths(
        export_tasks,
        linked_meta,
        target_tags,
        generator_registry,
        pool,
    )?;

    let data_set = normalize_data_set_with_pool(linked_meta, raw_sources.as_data_set(), pool)?;
    let validation_result = validate_normalized_data_set_for_target_with_assets_report_and_pool(
        linked_meta,
        assets,
        target_tags,
        &data_set,
        pool,
    );
    let validation_report = match validation_result {
        Ok(report) => report,
        Err(ConfigForgeError::ValidationReport(diagnostic_report)) => {
            export_validation_report_tasks_from_diagnostics(
                export_tasks,
                linked_meta,
                target_tags,
                &diagnostic_report,
                sink,
            )?;
            return Err(ConfigForgeError::ValidationReport(diagnostic_report));
        }
        Err(error) => return Err(error),
    };
    let mut warnings = Vec::new();
    warnings.extend(validation_report.warning_messages());
    let export_data_set = canonicalize_asset_refs(linked_meta, assets, target_tags, &data_set)?;
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
                        &export_data_set,
                        &validation_report,
                        pool,
                        sink,
                        generator_registry,
                    ),
                )
            })
            .collect::<Vec<_>>()
    });
    task_outputs.sort_by_key(|(index, _)| *index);

    for (_, task_output) in task_outputs {
        task_output?;
    }

    let stable_task_outputs = export_tasks
        .iter()
        .map(|task| RuntimeExportTaskOutput {
            export_name: task.name.clone(),
        })
        .collect();

    Ok(RuntimeExportOutput {
        warnings,
        task_outputs: stable_task_outputs,
    })
}

fn export_validation_report_tasks_from_diagnostics(
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    diagnostic_report: &DiagnosticReport<Diagnostic>,
    sink: &ArtifactSink,
) -> Result<()> {
    let report_tasks = export_tasks
        .iter()
        .copied()
        .filter(|task| task.type_id == "validation_report")
        .collect::<Vec<_>>();
    if report_tasks.is_empty() {
        return Ok(());
    }
    let report = DesignerValidationReport::from_diagnostic_report(
        diagnostic_report,
        designer_report_options(linked_meta, target_tags),
    );
    for task in report_tasks {
        write_report_task(task, &report, sink)?;
    }
    Ok(())
}

fn designer_report_options(
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
) -> DesignerReportOptions {
    let _ = linked_meta;
    DesignerReportOptions {
        title: None,
        project_name: None,
        project_path: None,
        target: None,
        target_tags: target_tags.clone(),
        generated_at: super::generated_at(),
        input_root: None,
    }
}

fn write_report_task(
    task: &NormalizedExportTask,
    report: &DesignerValidationReport,
    sink: &ArtifactSink,
) -> Result<()> {
    let config = validation_report_export_config_from_task(task)?;
    let mut report = report.clone();
    if let Some(title) = &config.title {
        report.metadata.title = title.clone();
    }
    let content = render_designer_validation_report(&report, config.format)?;
    sink.write_text(config.output, content)
}

fn validation_report_export_config_from_task(
    task: &NormalizedExportTask,
) -> Result<ValidationReportExportConfig> {
    let mut options = task.options.clone();
    let format = take_report_format(&mut options)?.unwrap_or(DesignerReportFormat::Html);
    let file_name = take_report_string(&mut options, "file_name")?
        .unwrap_or_else(|| format!("validation-report.{}", format.extension()));
    let title = take_report_string(&mut options, "title")?;
    let unsupported = options.keys().cloned().collect::<Vec<_>>();
    if let Some(key) = unsupported.first() {
        return Err(build_diagnostic_error(
            CF_BUILD_INVALID_CONFIG,
            "build.validation_report.options",
            format!(
                "validation_report export task `{}` has unknown option `{key}`",
                task.name
            ),
            key,
            "supported validation_report export task option",
            "Remove unknown validation_report options or update the exporter schema.",
        ));
    }
    Ok(ValidationReportExportConfig {
        format,
        output: task.output.join(file_name),
        title,
    })
}

fn take_report_format(
    options: &mut std::collections::BTreeMap<String, toml::Value>,
) -> Result<Option<DesignerReportFormat>> {
    take_report_string(options, "format")?
        .map(|value| value.parse())
        .transpose()
}

fn take_report_string(
    options: &mut std::collections::BTreeMap<String, toml::Value>,
    key: &str,
) -> Result<Option<String>> {
    options
        .remove(key)
        .map(|value| {
            let current_value = format!("{key}={value}");
            value.try_into().map_err(|source| {
                build_diagnostic_error(
                    CF_BUILD_INVALID_CONFIG,
                    "build.validation_report.options",
                    format!("invalid validation_report option `{key}`: {source}"),
                    current_value,
                    format!("valid `{key}` option value"),
                    "Check validation_report export task option types.",
                )
            })
        })
        .transpose()
}

fn canonicalize_asset_refs(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<DataSet> {
    if meta.model.asset_kinds.is_empty() {
        return Ok(data_set.clone());
    }

    let resolver = AssetResolver::new(meta, assets)?;
    let mut tables = Vec::with_capacity(data_set.tables.len());
    for table in &data_set.tables {
        let Some(table_meta) = meta.table(&table.name) else {
            tables.push(table.clone());
            continue;
        };
        let mut rows = Vec::with_capacity(table.rows.len());
        for (row_index, row) in table.rows.iter().enumerate() {
            let mut row = row.clone();
            if table_meta
                .meta
                .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
            {
                for field in table_meta
                    .fields
                    .iter()
                    .filter(|field| field.meta.is_exported_for(target_tags))
                {
                    canonicalize_asset_field_path(
                        &resolver,
                        target_tags,
                        field.meta.name.as_str(),
                        field,
                        &mut row,
                    )?;
                }

                let active_fragments = table
                    .row_source(row_index)
                    .map(|source| source.fragments.as_slice())
                    .unwrap_or(&[]);
                for fragment_name in active_fragments {
                    let Some(fragment) = table_meta.fragment(fragment_name) else {
                        continue;
                    };
                    if !fragment.meta.is_exported_for(target_tags) {
                        continue;
                    }
                    if let Some(struct_type) = fragment.struct_type() {
                        canonicalize_struct_asset_fields(
                            &resolver,
                            target_tags,
                            fragment.meta.name.as_str(),
                            struct_type,
                            &mut row,
                        )?;
                    } else {
                        for field in &fragment.fields {
                            if !field.meta.is_exported_for(target_tags) {
                                continue;
                            }
                            let field_path = format!(
                                "{}.{}",
                                fragment.meta.name.as_str(),
                                field.meta.name.as_str()
                            );
                            canonicalize_asset_field_path(
                                &resolver,
                                target_tags,
                                &field_path,
                                field,
                                &mut row,
                            )?;
                        }
                    }
                }
            }
            rows.push(row);
        }
        tables.push(DataTable::new_with_sources_and_cells(
            table.name.clone(),
            rows,
            table.row_sources.clone(),
            table.cell_sources.clone(),
        )?);
    }

    DataSet::try_new(tables)
}

fn canonicalize_struct_asset_fields(
    resolver: &AssetResolver,
    target_tags: &TargetTags,
    parent_path: &str,
    struct_type: &LinkedStructType<'_>,
    row: &mut DataRow,
) -> Result<()> {
    for field in &struct_type.fields {
        if !field.meta.is_exported_for(target_tags) {
            continue;
        }
        let field_path = format!("{parent_path}.{}", field.meta.name);
        match &field.linked_field {
            LinkedField::Struct(child_struct) => canonicalize_struct_asset_fields(
                resolver,
                target_tags,
                &field_path,
                child_struct,
                row,
            )?,
            _ => canonicalize_asset_field_path(resolver, target_tags, &field_path, field, row)?,
        }
    }
    Ok(())
}

fn canonicalize_asset_field_path(
    resolver: &AssetResolver,
    target_tags: &TargetTags,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    row: &mut DataRow,
) -> Result<()> {
    if !matches!(
        field.linked_field,
        LinkedField::Primitive(crate::meta::PrimitiveFieldType::Asset)
    ) {
        return Ok(());
    }
    let Some(asset_kind) = field.meta.asset_kind.as_deref() else {
        return Ok(());
    };
    let Some(value) = row.get(field_path).cloned() else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Ok(());
    }
    let resolved = resolver.resolve_for_target(asset_kind, &value, target_tags)?;
    row.insert(field_path.to_owned(), resolved.canonical_export_ref);
    Ok(())
}

fn export_meta_and_data_task(
    export_task: &NormalizedExportTask,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &crate::data::DataSet,
    validation_report: &crate::validation::ValidationReport,
    pool: &ThreadPool,
    sink: &ArtifactSink,
    generator_registry: &GeneratorRegistry,
) -> Result<RuntimeExportTaskOutput> {
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
    generator.generate(
        &GeneratorTaskInput {
            common: generator_common_input(export_task, linked_meta, target_tags),
            prepared: PreparedGeneratorInputs::MetaAndData {
                data_set,
                validation_report,
                pool,
            },
        },
        sink,
    )?;
    Ok(RuntimeExportTaskOutput {
        export_name: export_task.name.clone(),
    })
}

fn ensure_planned_runtime_output_paths(
    export_tasks: &[&NormalizedExportTask],
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    generator_registry: &GeneratorRegistry,
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
                        generator_registry,
                    ),
                )
            })
            .collect::<Vec<_>>()
    });
    planned_by_task.sort_by_key(|(index, _)| *index);

    for (_, task_paths) in planned_by_task {
        for path in task_paths? {
            if !paths.insert(path.clone()) {
                return Err(build_diagnostic_error(
                    CF_ARTIFACT_PATH_COLLISION,
                    "build.runtime.output",
                    format!("multiple data artifacts target `{}`", path.display()),
                    path.display().to_string(),
                    "unique planned data artifact output path",
                    "Adjust export task output or file_name so runtime data artifacts do not collide.",
                ));
            }
        }
    }
    Ok(())
}

fn plan_runtime_output_paths_for_task(
    export_task: &NormalizedExportTask,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    generator_registry: &GeneratorRegistry,
) -> Result<Vec<std::path::PathBuf>> {
    generator_registry
        .require_generator(&export_task.type_id)?
        .plan_task_artifacts(&GeneratorTaskInput {
            common: generator_common_input(export_task, linked_meta, target_tags),
            prepared: PreparedGeneratorInputs::Planning,
        })?
        .into_iter()
        .map(|path| Ok(lexical_normalize_path(&path)))
        .collect()
}

fn generator_common_input<'meta>(
    export_task: &NormalizedExportTask,
    linked_meta: &'meta LinkedMetaModel<'meta>,
    target_tags: &TargetTags,
) -> GeneratorCommonInput<'meta> {
    GeneratorCommonInput {
        meta: linked_meta,
        target_tags: target_tags.clone(),
        project: crate::ArtifactProjectInfo::default(),
        project_root: std::path::PathBuf::new(),
        target_name: String::new(),
        task_name: export_task.name.clone(),
        type_id: export_task.type_id.clone(),
        output: export_task.output.clone(),
        options: export_task.options.clone(),
        generated_at: super::generated_at(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    use crate::{
        artifact::{ArtifactWriteMode, ArtifactWriter},
        data::{DataSet, DataTable, RawSourceData},
        generator::{
            ArtifactGenerator, GeneratorCommonInput, GeneratorDataInput, GeneratorRegistry,
            GeneratorRequirements,
        },
        meta::{MetaModel, TargetTags},
    };

    use super::*;

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

        fn generate_to_sink(
            &self,
            _input: &GeneratorCommonInput<'_>,
            _sink: &ArtifactSink,
        ) -> Result<()> {
            Err(build_diagnostic_error(
                CF_BUILD_EXPORT_TASK_FAILED,
                "build.test.generator",
                "instrumented aggregate test generator requires data input".to_owned(),
                "meta-only generation call",
                "data-backed generation call",
                "test failure",
            ))
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

        fn generate_with_data_to_sink(
            &self,
            input: &GeneratorDataInput<'_, '_>,
            sink: &ArtifactSink,
        ) -> Result<()> {
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

        let output = export_meta_and_data_tasks_with_generator_registry(
            Some(&raw_sources),
            &export_tasks,
            &linked_meta,
            &AssetsConfig::default(),
            &TargetTags::empty(),
            &generator_registry,
            &pool,
            &sink,
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
        let artifact_result = sink.finish().expect("sink should finish");
        assert_eq!(
            artifact_result
                .entries
                .into_iter()
                .map(|entry| entry.path)
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("out/fast/fast.txt"),
                PathBuf::from("out/slow/slow.txt")
            ]
        );
    }

    struct DuplicatePathGenerator;

    impl ArtifactGenerator for DuplicatePathGenerator {
        fn type_id(&self) -> &'static str {
            "duplicate_path"
        }

        fn requirements(&self) -> GeneratorRequirements {
            GeneratorRequirements::meta_and_data()
        }

        fn generate_to_sink(
            &self,
            _input: &GeneratorCommonInput<'_>,
            _sink: &ArtifactSink,
        ) -> Result<()> {
            Err(build_diagnostic_error(
                CF_BUILD_EXPORT_TASK_FAILED,
                "build.test.generator",
                "duplicate path test generator requires data input".to_owned(),
                "meta-only generation call",
                "data-backed generation call",
                "test failure",
            ))
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

        fn generate_with_data_to_sink(
            &self,
            _input: &GeneratorDataInput<'_, '_>,
            _sink: &ArtifactSink,
        ) -> Result<()> {
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

        let error = export_meta_and_data_tasks_with_generator_registry(
            Some(&raw_sources),
            &export_tasks,
            &linked_meta,
            &AssetsConfig::default(),
            &TargetTags::empty(),
            &generator_registry,
            &pool,
            &sink,
        )
        .expect_err("duplicate path should fail during planning");
        let diagnostic = expect_single_diagnostic(error);

        assert_eq!(diagnostic.code, CF_ARTIFACT_PATH_COLLISION);
        assert_eq!(
            diagnostic.current_value.as_deref(),
            Some("out/same-file.dup")
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
