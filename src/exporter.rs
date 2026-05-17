use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde::{Deserialize, Deserializer};

use crate::{
    codegen::{
        builtin_generators, CodeExportInput, CodeExportProject, CodeExportTarget,
        CodegenTaskConfig, RuntimeDataFile, RuntimeDataFormat,
    },
    data::{DataLoadPlan, DataLoader, DataRow, DataTable},
    meta::{
        link_meta_model, validate_meta_model, EnumMeta, MetaDiagnostic, MetaModel, StructMeta,
        TableMeta,
    },
    validation::{normalize_data_set, validate_normalized_data_set},
    ConfigForgeError, Result,
};

#[derive(Debug, Clone, Copy)]
pub struct BuildOptions<'a> {
    pub project_path: &'a Path,
    pub target_name: &'a str,
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
    pub data: Option<TargetDataConfig>,
    #[serde(default)]
    pub code: Vec<CodegenTaskConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TargetDataConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    #[serde(default = "default_data_format")]
    pub format: TargetDataFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetDataFormat {
    Json,
    MessagePack,
}

impl<'de> Deserialize<'de> for TargetDataFormat {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "json" => Ok(Self::Json),
            "messagepack" | "msgpack" => Ok(Self::MessagePack),
            _ => Err(serde::de::Error::custom(format!(
                "unsupported target data format `{value}`"
            ))),
        }
    }
}

impl From<TargetDataFormat> for RuntimeDataFormat {
    fn from(format: TargetDataFormat) -> Self {
        match format {
            TargetDataFormat::Json => RuntimeDataFormat::Json,
            TargetDataFormat::MessagePack => RuntimeDataFormat::MessagePack,
        }
    }
}

fn default_data_format() -> TargetDataFormat {
    TargetDataFormat::Json
}

#[derive(Debug, Clone, Serialize)]
struct ExportedTable {
    meta: ExportedTableMeta,
    rows: Vec<DataRow>,
}

#[derive(Debug, Clone, Serialize)]
struct ExportedTableMeta {
    name: String,
}

pub fn build_target(options: BuildOptions<'_>) -> Result<()> {
    let project = ProjectConfig::from_path(options.project_path)?;
    let project_root = options
        .project_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let target = project.target(options.target_name)?;
    let meta_set = project.meta_model()?;
    let linked_meta = meta_set.link().map_err(ConfigForgeError::MetaValidation)?;

    let runtime_data_format = target
        .data
        .as_ref()
        .map(|data_config| RuntimeDataFormat::from(data_config.format))
        .unwrap_or(RuntimeDataFormat::Json);

    let runtime_data_files = if let Some(data_config) = &target.data {
        let data_plan = DataLoadPlan::from_meta_model(&meta_set)?;
        let data_set = DataLoader::new(project_root.join(&data_config.input)).load(&data_plan)?;
        let data_set = normalize_data_set(&linked_meta, &data_set)?;
        validate_normalized_data_set(&linked_meta, &data_set)?;
        write_runtime_data(
            &project_root.join(&data_config.output),
            &linked_meta,
            &data_set,
            data_config.format,
        )?
    } else {
        Vec::new()
    };

    write_generated_code(
        project_root,
        &CodeExportProject::from(&project.project),
        target,
        &linked_meta,
        runtime_data_files,
        runtime_data_format,
    )
}

impl ProjectConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let mut visited = BTreeSet::new();
        let mut fragments = Vec::new();
        load_project_config_recursive(path.as_ref(), &mut visited, &mut fragments)?;
        Ok(merge_project_configs(fragments))
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

fn write_runtime_data(
    out_dir: &Path,
    linked_meta: &crate::meta::LinkedMetaModel<'_>,
    data_set: &crate::data::DataSet,
    format: TargetDataFormat,
) -> Result<Vec<RuntimeDataFile>> {
    fs::create_dir_all(out_dir).map_err(|source| ConfigForgeError::WriteFile {
        path: out_dir.to_path_buf(),
        source,
    })?;

    let mut runtime_data_files = Vec::new();
    for table_meta in &linked_meta.tables {
        let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "data set is missing table `{}`",
                table_meta.meta.name
            ))
        })?;
        let path = match format {
            TargetDataFormat::Json => write_table_json(out_dir, table_meta, table)?,
            TargetDataFormat::MessagePack => write_table_messagepack(out_dir, table_meta, table)?,
        };
        runtime_data_files.push(RuntimeDataFile {
            table_name: table_meta.meta.name.clone(),
            path,
        });
    }
    Ok(runtime_data_files)
}

fn write_table_messagepack(
    out_dir: &Path,
    table_meta: &crate::meta::LinkedTableMeta<'_>,
    table: &DataTable,
) -> Result<String> {
    let payload = build_exported_table_payload(table_meta, table);
    let content = rmp_serde::to_vec_named(&payload)
        .map_err(|source| ConfigForgeError::SerializeMessagePack { source })?;
    let file_name = format!("{}.msgpack", table_meta.meta.name);
    let path = out_dir.join(&file_name);
    fs::write(&path, content).map_err(|source| ConfigForgeError::WriteFile { path, source })?;
    Ok(file_name)
}

fn write_table_json(
    out_dir: &Path,
    table_meta: &crate::meta::LinkedTableMeta<'_>,
    table: &DataTable,
) -> Result<String> {
    let payload = build_exported_table_payload(table_meta, table);
    let content = serde_json::to_string_pretty(&payload)?;
    let file_name = format!("{}.json", table_meta.meta.name);
    let path = out_dir.join(&file_name);
    fs::write(&path, content).map_err(|source| ConfigForgeError::WriteFile { path, source })?;
    Ok(file_name)
}

fn build_exported_table_payload(
    table_meta: &crate::meta::LinkedTableMeta<'_>,
    table: &DataTable,
) -> ExportedTable {
    let exported_fields = table_meta
        .fields
        .iter()
        .filter(|field| field.meta.export && !field.meta.tool_only && !field.meta.validator_only)
        .map(|field| field.meta.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .filter(|(field, _)| exported_fields.contains(field.as_str()))
                .map(|(field, value)| (field.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .collect();

    ExportedTable {
        meta: ExportedTableMeta {
            name: table_meta.meta.name.clone(),
        },
        rows,
    }
}

fn write_generated_code(
    project_root: &Path,
    project: &CodeExportProject,
    target: &TargetConfig,
    linked_meta: &crate::meta::LinkedMetaModel<'_>,
    runtime_data_files: Vec<RuntimeDataFile>,
    runtime_data_format: RuntimeDataFormat,
) -> Result<()> {
    let mut generated_files = Vec::new();
    for code_task in &target.code {
        let generator = builtin_generators()
            .into_iter()
            .find(|generator| generator.language() == code_task.language)
            .ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "no code generator registered for `{}`",
                    code_task.language.as_str()
                ))
            })?;
        let mut code_target = CodeExportTarget::from_task(code_task.clone());
        code_target.out_dir = project_root.join(&code_target.out_dir);
        let input = CodeExportInput {
            meta: linked_meta,
            project: project.clone(),
            target: code_target,
            runtime_data_files: runtime_data_files.clone(),
            runtime_data_format,
            generated_at: generated_at(),
        };
        generated_files.extend(generator.generate(&input)?);
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
        assert_eq!(config.format, TargetDataFormat::MessagePack);

        let alias_config: TargetDataConfig = toml::from_str(
            r#"
                input = "Data"
                output = "Generated/Data"
                format = "msgpack"
            "#,
        )
        .expect("msgpack alias should parse");
        assert_eq!(alias_config.format, TargetDataFormat::MessagePack);
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

        assert_eq!(config.format, TargetDataFormat::Json);
    }

    #[test]
    fn converts_target_data_format_to_runtime_data_format() {
        assert_eq!(
            RuntimeDataFormat::from(TargetDataFormat::Json),
            RuntimeDataFormat::Json
        );
        assert_eq!(
            RuntimeDataFormat::from(TargetDataFormat::MessagePack),
            RuntimeDataFormat::MessagePack
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
                tool_only = true

                [[tables.fields]]
                name = "validator_flag"
                type = "bool"
                validator_only = true

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

        let payload = build_exported_table_payload(table_meta, &table);
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert_eq!(value["meta"]["name"], "items");
        assert!(value["meta"].get("fields").is_none());
        assert_eq!(value["rows"][0]["id"], "1");
        assert!(value["rows"][0].get("debug_note").is_none());
        assert!(value["rows"][0].get("validator_flag").is_none());
        assert!(value["rows"][0].get("internal_value").is_none());
    }
}
