// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    path::Path,
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
    exporter::ExportDependency,
    meta::TargetTags,
    project::ProjectConfig,
};

use runtime::{RuntimeConfigForgeResult, export_runtime_data_tasks};
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
    let linked_meta = meta_set.link().map_err(ConfigForgeError::MetaValidation)?;
    let target_tags = target.effective_target_tags(options.extra_tags);

    // Runtime data is exported before codegen so generated APIs can reference logical
    // runtime data file paths relative to the loader root.
    let data_exports = target.data_exports().collect::<Vec<_>>();
    let runtime_export = export_runtime_data_tasks(
        project.project.input.as_deref(),
        &data_exports,
        &linked_meta,
        &target_tags,
        &cpu_pool,
    )?;

    let mut artifacts = runtime_export.artifacts;
    artifacts.extend(collect_generated_code_artifacts(
        project_root,
        &CodeExportProject::from(&project.project),
        &target,
        &target_tags,
        &linked_meta,
        runtime_export.results,
        &cpu_pool,
    )?);

    let artifacts = ArtifactWriter::new(options.artifact_write_mode).write(artifacts)?;

    Ok(BuildOutput {
        warnings: runtime_export.warnings,
        artifacts,
    })
}

fn collect_generated_code_artifacts(
    project_root: &Path,
    project: &CodeExportProject,
    target: &NormalizedTargetConfig,
    target_tags: &TargetTags,
    linked_meta: &crate::meta::LinkedMetaModel<'_>,
    _runtime_data_results: Vec<RuntimeConfigForgeResult>,
    pool: &ThreadPool,
) -> Result<Vec<Artifact>> {
    let generated_at = generated_at();
    let generators = builtin_generators();
    let code_exports = target.code_exports().collect::<Vec<_>>();
    let context = CodegenTaskContext {
        project_root,
        project,
        target_tags,
        linked_meta,
        generated_at: &generated_at,
        generators: &generators,
    };
    let mut generated_by_task = pool.install(|| {
        code_exports
            .par_iter()
            .enumerate()
            .map(|(index, code_task)| (index, generate_code_task(&context, code_task)))
            .collect::<Vec<_>>()
    });
    generated_by_task.sort_by_key(|(index, _)| *index);
    let mut generated_files = Vec::new();
    for (_, result) in generated_by_task {
        generated_files.extend(result?);
    }

    Ok(generated_files
        .into_iter()
        .map(|file| Artifact::text(file.path, file.content))
        .collect())
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
    code_task: &NormalizedExportTask,
) -> Result<Vec<GeneratedFile>> {
    debug_assert_eq!(code_task.dependency, ExportDependency::MetaOnly);
    let task = codegen_task_config_from_export_task(code_task)?;
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

    use crate::codegen::default_module_name;
    use crate::data::DataTable;
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
}
