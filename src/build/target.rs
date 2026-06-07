// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    ConfigForgeError, Result,
    generator::{GeneratorRegistry, GeneratorRequirements},
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
    pub(crate) requirements: GeneratorRequirements,
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
    let registry = GeneratorRegistry::with_builtin_generators();
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
        let requirements = registry.requirements(target_name, &export_task.type_id)?;
        validate_export_requirements(target_name, export_task, requirements)?;
        normalized.push(NormalizedExportTask {
            name,
            type_id: export_task.type_id.clone(),
            requirements,
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

fn validate_export_requirements(
    target_name: &str,
    export_task: &TargetExportConfig,
    requirements: GeneratorRequirements,
) -> Result<()> {
    validate_no_export_input_option(target_name, export_task)?;
    if !requirements.needs_data && export_task.options.contains_key("data") {
        return Err(ConfigForgeError::Validation(format!(
            "target `{target_name}` export task `{}` must not set `data`; `{}` only depends on Meta",
            export_task
                .name
                .as_deref()
                .unwrap_or(DEFAULT_EXPORT_TASK_NAME),
            export_task.type_id
        )));
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
        assert!(normalized.exports[0].requirements.needs_data);
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

        assert!(normalized.exports[0].requirements.needs_data);
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
            requirements: GeneratorRequirements::meta_only(),
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

        let codegen_task = crate::codegen::CodegenTaskConfig::from_export_task_parts(
            task.type_id.clone(),
            task.output.clone(),
            task.options.clone(),
        )
        .expect("codegen export task");

        assert_eq!(codegen_task.language, "cpp");
        assert_eq!(codegen_task.module_name.as_deref(), Some("GameConfig"));
        assert_eq!(codegen_task.namespace.as_deref(), Some("game::config"));
    }

    #[test]
    fn generator_requirements_classify_known_types() {
        let registry = GeneratorRegistry::with_builtin_generators();

        assert!(
            !registry
                .requirements("client", "rust")
                .expect("rust requirements")
                .needs_data
        );
        assert!(
            !registry
                .requirements("client", "ts")
                .expect("ts requirements")
                .needs_data
        );
        assert!(
            registry
                .requirements("client", "json")
                .expect("json requirements")
                .needs_data
        );
        assert!(
            registry
                .requirements("client", "lua")
                .expect("lua requirements")
                .needs_data
        );
        assert!(
            matches!(registry.requirements("client", "unknown"), Err(ConfigForgeError::Validation(message)) if message.contains("unknown export type `unknown`"))
        );
    }
}
