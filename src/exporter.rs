use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::Path};

use serde::Serialize;

use crate::{
    codegen::{
        builtin_generators, default_module_name, CodeExportInput, CodeExportTarget, CodegenOptions,
        RuntimeDataFile, RuntimeDataFormat,
    },
    data::{DataLoadPlan, DataLoader, DataRow, DataTable},
    meta::{MetaModel, TableMeta},
    validation::{normalize_data_set, validate_data_set, validate_normalized_data_set},
    ConfigForgeError, Result,
};

#[derive(Debug, Clone, Copy)]
pub struct ExportOptions<'a> {
    pub meta_path: &'a Path,
    pub data_dir: &'a Path,
    pub out_dir: &'a Path,
}

#[derive(Debug, Clone, Serialize)]
struct ExportedTable<'a> {
    meta: &'a TableMeta,
    rows: &'a [DataRow],
}

pub fn validate_project(meta_path: &Path, data_dir: &Path) -> Result<()> {
    let meta_set = MetaModel::from_path(meta_path)?;
    let data_plan = DataLoadPlan::from_meta_model(&meta_set)?;
    let linked_meta = meta_set.link().map_err(ConfigForgeError::MetaValidation)?;
    let data_set = DataLoader::new(data_dir).load(&data_plan)?;
    validate_data_set(&linked_meta, &data_set)
}

pub fn export_project(options: ExportOptions<'_>) -> Result<()> {
    let meta_set = MetaModel::from_path(options.meta_path)?;
    let data_plan = DataLoadPlan::from_meta_model(&meta_set)?;
    let linked_meta = meta_set.link().map_err(ConfigForgeError::MetaValidation)?;
    let data_set = DataLoader::new(options.data_dir).load(&data_plan)?;
    let data_set = normalize_data_set(&linked_meta, &data_set)?;
    validate_normalized_data_set(&linked_meta, &data_set)?;

    fs::create_dir_all(options.out_dir).map_err(|source| ConfigForgeError::WriteFile {
        path: options.out_dir.to_path_buf(),
        source,
    })?;

    for table_meta in &linked_meta.tables {
        let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "data set is missing table `{}`",
                table_meta.meta.name
            ))
        })?;
        write_table_json(options.out_dir, table_meta.meta, table)?;
    }

    write_generated_code(options.out_dir, &linked_meta)?;

    Ok(())
}

fn write_table_json(out_dir: &Path, table_meta: &TableMeta, table: &DataTable) -> Result<()> {
    let payload = ExportedTable {
        meta: table_meta,
        rows: &table.rows,
    };
    let content = serde_json::to_string_pretty(&payload)?;
    let path = out_dir.join(format!("{}.json", table_meta.name));
    fs::write(&path, content).map_err(|source| ConfigForgeError::WriteFile { path, source })
}

fn write_generated_code(
    out_dir: &Path,
    linked_meta: &crate::meta::LinkedMetaModel<'_>,
) -> Result<()> {
    let runtime_data_files = linked_meta
        .tables
        .iter()
        .map(|table| RuntimeDataFile {
            table_name: table.meta.name.clone(),
            path: format!("{}.json", table.meta.name),
        })
        .collect::<Vec<_>>();

    for generator in builtin_generators() {
        let target = CodeExportTarget {
            language: generator.language(),
            out_dir: out_dir.to_path_buf(),
            module_name: default_module_name(generator.language()).to_owned(),
            namespace: None,
            options: CodegenOptions::for_language(generator.language()),
        };
        let input = CodeExportInput {
            meta: linked_meta,
            target,
            runtime_data_files: runtime_data_files.clone(),
            runtime_data_format: RuntimeDataFormat::Json,
            generated_at: generated_at(),
        };
        for file in generator.generate(&input)? {
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
    use crate::codegen::CodeExportLanguage;

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
}
