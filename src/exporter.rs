use std::{
    collections::BTreeSet,
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rayon::prelude::*;
use rayon::ThreadPool;
use serde::Deserialize;

use crate::{
    codegen::{
        builtin_generators, CodeExportInput, CodeExportProject, CodeExportTarget, CodeGenerator,
        CodegenTaskConfig, GeneratedFile, RuntimeDataFile,
    },
    data::{DataLoadPlan, DataLoader},
    config_forge::{export_runtime_data, DataFormatRegistry},
    meta::{
        link_meta_model, validate_meta_model, EnumMeta, MetaDiagnostic, MetaModel, StructMeta,
        TableMeta, TargetTags,
    },
    validation::{normalize_data_set, validate_normalized_data_set_for_target_with_report},
    ConfigForgeError, Result,
};

#[derive(Debug, Clone, Copy)]
pub struct BuildOptions<'a> {
    pub project_path: &'a Path,
    pub target_name: &'a str,
    pub execution: ExecutionOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionOptions {
    pub jobs: usize,
    pub io_concurrency: usize,
    pub network_concurrency: usize,
    pub serial: bool,
}

const MAX_JOBS: usize = 128;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildOutput {
    pub warnings: Vec<String>,
}

impl ExecutionOptions {
    pub fn from_cli(
        jobs: Option<usize>,
        io_concurrency: Option<usize>,
        network_concurrency: Option<usize>,
        serial: bool,
    ) -> Result<Self> {
        let mut options = Self::default();
        if let Some(jobs) = jobs {
            options.jobs = jobs;
        }
        if let Some(io_concurrency) = io_concurrency {
            options.io_concurrency = io_concurrency;
        }
        if let Some(network_concurrency) = network_concurrency {
            options.network_concurrency = network_concurrency;
        }
        options.serial = serial;
        options.normalize()
    }

    fn normalize(mut self) -> Result<Self> {
        if self.serial {
            self.jobs = 1;
            self.io_concurrency = 1;
            self.network_concurrency = 1;
        }
        if self.jobs == 0 {
            return Err(ConfigForgeError::Validation(
                "--jobs must be greater than 0".to_owned(),
            ));
        }
        if self.jobs > MAX_JOBS {
            return Err(ConfigForgeError::Validation(format!(
                "--jobs must be no greater than {MAX_JOBS}"
            )));
        }
        if self.io_concurrency == 0 {
            return Err(ConfigForgeError::Validation(
                "--io-concurrency must be greater than 0".to_owned(),
            ));
        }
        if self.network_concurrency == 0 {
            return Err(ConfigForgeError::Validation(
                "--network-concurrency must be greater than 0".to_owned(),
            ));
        }
        Ok(self)
    }
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        let available = std::thread::available_parallelism()
            .unwrap_or_else(|_| NonZeroUsize::new(1).expect("1 is non-zero"))
            .get();
        let jobs = available.clamp(1, 4);
        let io_concurrency = (jobs * 2).clamp(4, 16);
        let network_concurrency = io_concurrency.min(8);
        Self {
            jobs,
            io_concurrency,
            network_concurrency,
            serial: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ProjectConfig {
    #[serde(default)]
    pub imports: Vec<PathBuf>,
    #[serde(default)]
    pub project: ProjectConfigMeta,
    #[serde(default)]
    pub enums: Vec<EnumMeta>,
    #[serde(default)]
    pub structs: Vec<StructMeta>,
    #[serde(default)]
    pub tables: Vec<TableMeta>,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct ProjectConfigMeta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub copyright: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TargetConfig {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub data: Option<TargetDataConfig>,
    #[serde(default)]
    pub code: Vec<CodegenTaskConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TargetDataConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    #[serde(default = "default_data_format")]
    pub format: String,
}

fn default_data_format() -> String {
    "json".to_owned()
}

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
    let meta_set = project.meta_model()?;
    let linked_meta = meta_set.link().map_err(ConfigForgeError::MetaValidation)?;
    let target_tags = target.target_tags();

    let mut warnings = Vec::new();
    let runtime_data_files = if let Some(data_config) = &target.data {
        let registry = DataFormatRegistry::with_builtin_exporters();
        let data_exporter = registry.require(&data_config.format)?;
        let data_plan = DataLoadPlan::from_tables(
            linked_meta
                .tables
                .iter()
                .filter(|table| table.meta.is_exported_for(&target_tags))
                .map(|table| table.meta),
        )?;
        let data_set = DataLoader::new(project_root.join(&data_config.input))
            .load_with_pool(&data_plan, &cpu_pool)?;
        let data_set = normalize_data_set(&linked_meta, &data_set)?;
        let validation_report = validate_normalized_data_set_for_target_with_report(
            &linked_meta,
            &target_tags,
            &data_set,
        )?;
        warnings.extend(validation_report.warning_messages());
        export_runtime_data(
            &project_root.join(&data_config.output),
            &linked_meta,
            &target_tags,
            &data_set,
            data_exporter,
            &cpu_pool,
        )?
    } else {
        Vec::new()
    };

    write_generated_code(
        project_root,
        &CodeExportProject::from(&project.project),
        target,
        &target_tags,
        &linked_meta,
        runtime_data_files,
        &cpu_pool,
    )?;

    Ok(BuildOutput { warnings })
}

impl TargetConfig {
    pub fn target_tags(&self) -> TargetTags {
        TargetTags::new(self.tags.clone())
    }
}

impl ProjectConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let mut visited = BTreeSet::new();
        let mut fragments = Vec::new();
        load_project_config_recursive(path.as_ref(), &mut visited, &mut fragments)?;
        let merged = merge_project_configs(fragments);
        validate_unique_target_names(&merged.targets)?;
        Ok(merged)
    }

    fn target(&self, target_name: &str) -> Result<&TargetConfig> {
        self.targets
            .iter()
            .find(|target| target.name == target_name)
            .ok_or_else(|| ConfigForgeError::Validation(format!("target `{target_name}` not found")))
    }

    fn meta_model(&self) -> Result<MetaModel> {
        let model = MetaModel {
            enums: self.enums.clone(),
            structs: self.structs.clone(),
            tables: self.tables.clone(),
        };
        finish_loading_project_meta(&model)?;
        Ok(model)
    }
}

fn load_project_config_recursive(
    path: &Path,
    visited: &mut BTreeSet<PathBuf>,
    fragments: &mut Vec<ProjectConfig>,
) -> Result<()> {
    let canonical_path = fs::canonicalize(path).map_err(|source| ConfigForgeError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    if !visited.insert(canonical_path.clone()) {
        return Ok(());
    }

    let content =
        fs::read_to_string(&canonical_path).map_err(|source| ConfigForgeError::ReadFile {
            path: canonical_path.clone(),
            source,
        })?;
    let mut fragment: ProjectConfig =
        toml::from_str(&content).map_err(|source| ConfigForgeError::ParseProject {
            path: canonical_path.clone(),
            source,
        })?;
    let base_dir = canonical_path.parent().unwrap_or_else(|| Path::new("."));
    for import in &fragment.imports {
        load_project_config_recursive(&base_dir.join(import), visited, fragments)?;
    }
    normalize_project_config_paths(&mut fragment, base_dir);
    fragments.push(fragment);
    Ok(())
}

fn normalize_project_config_paths(config: &mut ProjectConfig, base_dir: &Path) {
    for target in &mut config.targets {
        if let Some(data) = &mut target.data {
            data.input = normalize_config_path(base_dir, &data.input);
            data.output = normalize_config_path(base_dir, &data.output);
        }
        for code in &mut target.code {
            code.output = normalize_config_path(base_dir, &code.output);
        }
    }
}

fn normalize_config_path(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

fn merge_project_configs(fragments: Vec<ProjectConfig>) -> ProjectConfig {
    let mut merged = ProjectConfig {
        imports: Vec::new(),
        project: ProjectConfigMeta::default(),
        enums: Vec::new(),
        structs: Vec::new(),
        tables: Vec::new(),
        targets: Vec::new(),
    };

    for mut fragment in fragments {
        if fragment.project.name.is_some() {
            merged.project.name = fragment.project.name.take();
        }
        if fragment.project.desc.is_some() {
            merged.project.desc = fragment.project.desc.take();
        }
        if fragment.project.copyright.is_some() {
            merged.project.copyright = fragment.project.copyright.take();
        }
        merged.enums.append(&mut fragment.enums);
        merged.structs.append(&mut fragment.structs);
        merged.tables.append(&mut fragment.tables);
        merged.targets.append(&mut fragment.targets);
    }

    merged
}

fn validate_unique_target_names(targets: &[TargetConfig]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for target in targets {
        if !seen.insert(target.name.as_str()) {
            return Err(ConfigForgeError::Validation(format!(
                "duplicate target `{}`",
                target.name
            )));
        }
    }

    Ok(())
}

fn finish_loading_project_meta(model: &MetaModel) -> Result<()> {
    let (_, mut diagnostics): (
        Option<crate::meta::LinkedMetaModel<'_>>,
        Vec<MetaDiagnostic>,
    ) = link_meta_model(model);
    diagnostics.extend(validate_meta_model(model));
    diagnostics.sort();
    diagnostics.dedup();

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(ConfigForgeError::MetaValidation(diagnostics))
    }
}

impl From<&ProjectConfigMeta> for CodeExportProject {
    fn from(project: &ProjectConfigMeta) -> Self {
        Self {
            name: project.name.clone(),
            desc: project.desc.clone(),
            copyright: project.copyright.clone(),
        }
    }
}

fn write_generated_code(
    project_root: &Path,
    project: &CodeExportProject,
    target: &TargetConfig,
    target_tags: &TargetTags,
    linked_meta: &crate::meta::LinkedMetaModel<'_>,
    runtime_data_files: Vec<RuntimeDataFile>,
    pool: &ThreadPool,
) -> Result<()> {
    let generated_at = generated_at();
    let generators = builtin_generators();
    let context = CodegenTaskContext {
        project_root,
        project,
        target_tags,
        linked_meta,
        runtime_data_files: &runtime_data_files,
        generated_at: &generated_at,
        generators: &generators,
    };
    let mut generated_by_task = pool.install(|| {
        target
            .code
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

    let mut planned_paths = BTreeSet::new();
    for file in &generated_files {
        if !planned_paths.insert(file.path.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "multiple generated code artifacts target `{}`",
                file.path.display()
            )));
        }
    }

    for file in generated_files {
        if let Some(parent) = file.path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigForgeError::WriteFile {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&file.path, file.content).map_err(|source| ConfigForgeError::WriteFile {
            path: file.path,
            source,
        })?;
    }

    Ok(())
}

struct CodegenTaskContext<'a, 'meta> {
    project_root: &'a Path,
    project: &'a CodeExportProject,
    target_tags: &'a TargetTags,
    linked_meta: &'meta crate::meta::LinkedMetaModel<'meta>,
    runtime_data_files: &'a [RuntimeDataFile],
    generated_at: &'a str,
    generators: &'a [Box<dyn CodeGenerator>],
}

fn generate_code_task(
    context: &CodegenTaskContext<'_, '_>,
    code_task: &CodegenTaskConfig,
) -> Result<Vec<GeneratedFile>> {
    let generator = context
        .generators
        .iter()
        .find(|generator| generator.language() == code_task.language)
        .ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "no code generator registered for `{}`",
                code_task.language.as_str()
            ))
        })?;
    let mut code_target = CodeExportTarget::from_task(code_task.clone());
    code_target.out_dir = context.project_root.join(&code_target.out_dir);
    let input = CodeExportInput {
        meta: context.linked_meta,
        target_tags: context.target_tags.clone(),
        project: context.project.clone(),
        target: code_target,
        runtime_data_files: context.runtime_data_files.to_vec(),
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

    use crate::codegen::{default_module_name, CodeExportLanguage};
    use crate::data::DataTable;
    use crate::config_forge::build_exported_table_payload;

    use super::*;

    #[test]
    fn maps_language_to_default_module_name() {
        assert_eq!(
            default_module_name(CodeExportLanguage::Rust),
            "config_forge_generated"
        );
        assert_eq!(
            default_module_name(CodeExportLanguage::Cpp),
            "config_forge_generated"
        );
        assert_eq!(
            default_module_name(CodeExportLanguage::TypeScript),
            "dataExportGenerated"
        );
    }

    #[test]
    fn converts_unix_days_to_civil_date() {
        assert_eq!(civil_from_unix_days(0), (1970, 1, 1));
        assert_eq!(civil_from_unix_days(20_590), (2026, 5, 17));
    }

    #[test]
    fn parses_messagepack_data_format_and_alias() {
        let config: TargetDataConfig = toml::from_str(
            r#"
                input = "Data"
                output = "Generated/Data"
                format = "messagepack"
            "#,
        )
        .expect("messagepack format should parse");
        assert_eq!(config.format, "messagepack");

        let alias_config: TargetDataConfig = toml::from_str(
            r#"
                input = "Data"
                output = "Generated/Data"
                format = "msgpack"
            "#,
        )
        .expect("msgpack alias should parse");
        assert_eq!(alias_config.format, "msgpack");
    }

    #[test]
    fn defaults_data_format_to_json() {
        let config: TargetDataConfig = toml::from_str(
            r#"
                input = "Data"
                output = "Generated/Data"
            "#,
        )
        .expect("data config should parse");

        assert_eq!(config.format, "json");
    }

    #[test]
    fn execution_options_apply_defaults_overrides_and_serial() {
        let defaults = ExecutionOptions::default();
        assert!(defaults.jobs >= 1);
        assert!(defaults.jobs <= 4);
        assert!(defaults.io_concurrency >= defaults.jobs);
        assert!(defaults.network_concurrency <= defaults.io_concurrency);

        let custom = ExecutionOptions::from_cli(Some(2), Some(5), Some(3), false)
            .expect("custom execution options should be valid");
        assert_eq!(custom.jobs, 2);
        assert_eq!(custom.io_concurrency, 5);
        assert_eq!(custom.network_concurrency, 3);
        assert!(!custom.serial);

        let serial = ExecutionOptions::from_cli(Some(8), Some(8), Some(8), true)
            .expect("serial should override explicit concurrency");
        assert_eq!(serial.jobs, 1);
        assert_eq!(serial.io_concurrency, 1);
        assert_eq!(serial.network_concurrency, 1);
        assert!(serial.serial);
    }

    #[test]
    fn execution_options_reject_zero_limits() {
        let cases = [
            ExecutionOptions::from_cli(Some(0), None, None, false),
            ExecutionOptions::from_cli(None, Some(0), None, false),
            ExecutionOptions::from_cli(None, None, Some(0), false),
        ];

        for error in cases.into_iter().map(Result::unwrap_err) {
            assert!(
                matches!(error, ConfigForgeError::Validation(message) if message.contains("greater than 0"))
            );
        }
    }

    #[test]
    fn execution_options_reject_excessive_jobs() {
        let error = ExecutionOptions::from_cli(Some(MAX_JOBS + 1), None, None, false).unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("--jobs must be no greater than"))
        );
    }

    #[test]
    fn project_config_rejects_duplicate_target_names() {
        let error = validate_unique_target_names(&[
            TargetConfig {
                name: "client".to_owned(),
                tags: Vec::new(),
                data: None,
                code: Vec::new(),
            },
            TargetConfig {
                name: "client".to_owned(),
                tags: Vec::new(),
                data: None,
                code: Vec::new(),
            },
        ])
        .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("duplicate target `client`"))
        );
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
