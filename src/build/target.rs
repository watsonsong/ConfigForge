// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    ConfigForgeError, Result,
    codegen::{CodegenTaskConfig, builtin_generators, find_generator},
    exporter::{DataFormatRegistry, ExportDependency, TargetDataConfig},
    meta::TargetTags,
    project::{TargetConfig, TargetExportConfig},
};

pub(crate) const DEFAULT_EXPORT_TASK_NAME: &str = "default";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NormalizedTargetConfig {
    pub(crate) name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) exports: Vec<NormalizedExportTask>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NormalizedExportTask {
    pub(crate) name: String,
    pub(crate) type_id: String,
    pub(crate) dependency: ExportDependency,
    pub(crate) output: PathBuf,
    pub(crate) options: BTreeMap<String, toml::Value>,
}

impl TargetConfig {
    pub fn target_tags(&self) -> TargetTags {
        TargetTags::new(self.tags.clone())
    }

    pub(crate) fn normalize(&self) -> Result<NormalizedTargetConfig> {
        let exports = normalize_export_tasks(&self.name, &self.exports)?;
        Ok(NormalizedTargetConfig {
            name: self.name.clone(),
            tags: self.tags.clone(),
            exports,
        })
    }
}

impl NormalizedTargetConfig {
    pub(crate) fn effective_target_tags(&self, extra_tags: &[String]) -> TargetTags {
        TargetTags::new(self.tags.iter().chain(extra_tags.iter()).cloned())
    }
}

fn normalize_export_tasks(
    target_name: &str,
    export_tasks: &[TargetExportConfig],
) -> Result<Vec<NormalizedExportTask>> {
    let mut normalized = Vec::with_capacity(export_tasks.len());
    let mut seen = std::collections::BTreeSet::new();
    let registry = ExporterRegistry::with_builtin_exporters();
    for export_task in export_tasks {
        let name = match (&export_task.name, export_tasks.len()) {
            (Some(name), _) => {
                validate_export_task_name(target_name, name)?;
                name.clone()
            }
            (None, 1) => DEFAULT_EXPORT_TASK_NAME.to_owned(),
            (None, _) => {
                validate_export_task_name(target_name, &export_task.type_id)?;
                export_task.type_id.clone()
            }
        };
        if !seen.insert(name.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "target `{target_name}` declares duplicate export task `{name}`"
            )));
        }
        let dependency = registry.dependency(target_name, &export_task.type_id)?;
        validate_export_dependency(target_name, export_task, dependency)?;
        normalized.push(NormalizedExportTask {
            name,
            type_id: export_task.type_id.clone(),
            dependency,
            output: export_task.output.clone(),
            options: export_task.options.clone(),
        });
    }
    Ok(normalized)
}

fn validate_export_task_name(target_name: &str, name: &str) -> Result<()> {
    let is_valid = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        && name
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && name
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric());
    if is_valid {
        Ok(())
    } else {
        Err(ConfigForgeError::Validation(format!(
            "target `{target_name}` declares invalid export task name `{name}`; export task names must use ASCII letters, digits, `_` or `-`, and start and end with a letter or digit"
        )))
    }
}

struct ExporterRegistry {
    data_registry: DataFormatRegistry,
    code_generators: Vec<Box<dyn crate::codegen::CodeGenerator>>,
}

impl ExporterRegistry {
    fn with_builtin_exporters() -> Self {
        Self {
            data_registry: DataFormatRegistry::with_builtin_exporters(),
            code_generators: builtin_generators(),
        }
    }

    fn dependency(&self, target_name: &str, type_id: &str) -> Result<ExportDependency> {
        let data_dependency = self.data_registry.dependency(type_id);
        let code_dependency =
            find_generator(&self.code_generators, type_id).map(|generator| generator.dependency());

        match (data_dependency, code_dependency) {
            (Some(data_dependency), None) => Ok(data_dependency),
            (None, Some(code_dependency)) => Ok(code_dependency),
            (Some(data_dependency), Some(code_dependency))
                if data_dependency == code_dependency =>
            {
                Ok(data_dependency)
            }
            (Some(_), Some(_)) => Err(ConfigForgeError::Validation(format!(
                "target `{target_name}` declares ambiguous export type `{type_id}`; multiple exporter contracts with different dependencies are registered"
            ))),
            (None, None) => Err(ConfigForgeError::Validation(format!(
                "target `{target_name}` declares unknown export type `{type_id}`"
            ))),
        }
    }
}

fn validate_export_dependency(
    target_name: &str,
    export_task: &TargetExportConfig,
    dependency: ExportDependency,
) -> Result<()> {
    match dependency {
        ExportDependency::MetaOnly => {
            validate_no_export_input_option(target_name, export_task)?;
            if export_task.options.contains_key("data") {
                return Err(ConfigForgeError::Validation(format!(
                    "target `{target_name}` export task `{}` must not set `data`; `{}` only depends on Meta",
                    export_task
                        .name
                        .as_deref()
                        .unwrap_or(DEFAULT_EXPORT_TASK_NAME),
                    export_task.type_id
                )));
            }
        }
        ExportDependency::MetaAndData => validate_no_export_input_option(target_name, export_task)?,
        ExportDependency::DataOnly => {}
    }
    Ok(())
}

fn validate_no_export_input_option(
    target_name: &str,
    export_task: &TargetExportConfig,
) -> Result<()> {
    if export_task.options.contains_key("input") {
        return Err(ConfigForgeError::Validation(format!(
            "target `{target_name}` export task `{}` must not set `input`; source data root is configured once at `project.input`",
            export_task
                .name
                .as_deref()
                .unwrap_or(DEFAULT_EXPORT_TASK_NAME)
        )));
    }
    Ok(())
}

pub(crate) fn target_data_config_from_export_task(task: &NormalizedExportTask) -> TargetDataConfig {
    TargetDataConfig {
        name: Some(task.name.clone()),
        output: task.output.clone(),
        format: task.type_id.clone(),
        options: task.options.clone(),
    }
}

pub(crate) fn codegen_task_config_from_export_task(
    task: &NormalizedExportTask,
) -> Result<CodegenTaskConfig> {
    let mut options = task.options.clone();
    Ok(CodegenTaskConfig {
        language: task.type_id.clone(),
        output: task.output.clone(),
        file_name: take_string(&mut options, "file_name")?,
        data: take_string(&mut options, "data")?,
        module_name: take_string(&mut options, "module_name")?,
        namespace: take_string(&mut options, "namespace")?,
        file_header: take_string(&mut options, "file_header")?,
        keyword_conflict: take_optional_value(&mut options, "keyword_conflict")?,
        i18n: take_value(&mut options, "i18n")?.unwrap_or_default(),
        naming: take_value(&mut options, "naming")?.unwrap_or_default(),
        formatting: take_value(&mut options, "formatting")?.unwrap_or_default(),
        language_options: options.into_iter().collect(),
    })
}

fn take_string(options: &mut BTreeMap<String, toml::Value>, key: &str) -> Result<Option<String>> {
    take_optional_value(options, key)
}

fn take_optional_value<T>(
    options: &mut BTreeMap<String, toml::Value>,
    key: &str,
) -> Result<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    take_value(options, key)
}

fn take_value<T>(options: &mut BTreeMap<String, toml::Value>, key: &str) -> Result<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    options
        .remove(key)
        .map(|value| {
            value.try_into().map_err(|source| {
                ConfigForgeError::Validation(format!(
                    "invalid export task option `{key}`: {source}"
                ))
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn export_task(type_id: &str) -> TargetExportConfig {
        TargetExportConfig {
            name: None,
            type_id: type_id.to_owned(),
            output: PathBuf::from("Generated/Data"),
            options: BTreeMap::new(),
        }
    }

    #[test]
    fn normalizes_single_unnamed_export_task_to_default() {
        let target = TargetConfig {
            name: "client".to_owned(),
            tags: vec!["client".to_owned()],
            exports: vec![export_task("json")],
        };

        let normalized = target.normalize().expect("target should normalize");

        assert_eq!(normalized.exports[0].name, DEFAULT_EXPORT_TASK_NAME);
        assert_eq!(
            normalized.exports[0].dependency,
            ExportDependency::MetaAndData
        );
    }

    #[test]
    fn rejects_meta_only_export_input() {
        let target = TargetConfig {
            name: "client".to_owned(),
            tags: Vec::new(),
            exports: vec![TargetExportConfig {
                name: None,
                type_id: "rust".to_owned(),
                output: PathBuf::from("Generated/Rust"),
                options: BTreeMap::from([(
                    "input".to_owned(),
                    toml::Value::String("Data".to_owned()),
                )]),
            }],
        };

        let error = target.normalize().unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("must not set `input`"))
        );
    }

    #[test]
    fn allows_meta_and_data_export_without_task_input() {
        let target = TargetConfig {
            name: "client".to_owned(),
            tags: Vec::new(),
            exports: vec![TargetExportConfig {
                name: None,
                type_id: "json".to_owned(),
                output: PathBuf::from("Generated/Data"),
                options: BTreeMap::new(),
            }],
        };

        let normalized = target.normalize().expect("target should normalize");

        assert_eq!(
            normalized.exports[0].dependency,
            ExportDependency::MetaAndData
        );
    }

    #[test]
    fn rejects_meta_and_data_export_input() {
        let target = TargetConfig {
            name: "client".to_owned(),
            tags: Vec::new(),
            exports: vec![TargetExportConfig {
                name: None,
                type_id: "json".to_owned(),
                output: PathBuf::from("Generated/Data"),
                options: BTreeMap::from([(
                    "input".to_owned(),
                    toml::Value::String("Data".to_owned()),
                )]),
            }],
        };

        let error = target.normalize().unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("must not set `input`"))
        );
    }

    #[test]
    fn rejects_invalid_export_task_names() {
        let error = normalize_export_tasks(
            "client",
            &[TargetExportConfig {
                name: Some("bad name".to_owned()),
                ..export_task("json")
            }],
        )
        .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("invalid export task name"))
        );
    }

    #[test]
    fn rejects_duplicate_export_task_names() {
        let error = normalize_export_tasks(
            "client",
            &[
                TargetExportConfig {
                    name: Some("json".to_owned()),
                    ..export_task("json")
                },
                TargetExportConfig {
                    name: Some("json".to_owned()),
                    type_id: "messagepack".to_owned(),
                    output: PathBuf::from("Generated/MessagePack"),
                    options: BTreeMap::new(),
                },
            ],
        )
        .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("duplicate export task `json`"))
        );
    }

    #[test]
    fn converts_export_task_to_codegen_task() {
        let task = NormalizedExportTask {
            name: "cpp".to_owned(),
            type_id: "cpp".to_owned(),
            dependency: ExportDependency::MetaOnly,
            output: PathBuf::from("Generated/Cpp"),
            options: BTreeMap::from([
                (
                    "module_name".to_owned(),
                    toml::Value::String("GameConfig".to_owned()),
                ),
                (
                    "namespace".to_owned(),
                    toml::Value::String("game::config".to_owned()),
                ),
            ]),
        };

        let code_task = codegen_task_config_from_export_task(&task).expect("code task");

        assert_eq!(code_task.language, "cpp");
        assert_eq!(code_task.module_name.as_deref(), Some("GameConfig"));
        assert_eq!(code_task.namespace.as_deref(), Some("game::config"));
    }

    #[test]
    fn export_dependency_classifies_known_types() {
        let registry = ExporterRegistry::with_builtin_exporters();

        assert_eq!(
            registry
                .dependency("client", "rust")
                .expect("rust dependency"),
            ExportDependency::MetaOnly
        );
        assert_eq!(
            registry.dependency("client", "ts").expect("ts dependency"),
            ExportDependency::MetaOnly
        );
        assert_eq!(
            registry
                .dependency("client", "json")
                .expect("json dependency"),
            ExportDependency::MetaAndData
        );
        assert_eq!(
            registry
                .dependency("client", "lua")
                .expect("lua dependency"),
            ExportDependency::MetaAndData
        );
        assert!(
            matches!(registry.dependency("client", "unknown"), Err(ConfigForgeError::Validation(message)) if message.contains("unknown export type `unknown`"))
        );
        let _ = ExportDependency::DataOnly;
    }
}
