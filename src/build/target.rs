// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    codegen::CodegenTaskConfig, exporter::TargetDataConfig, meta::TargetTags,
    project::TargetConfig, ConfigForgeError, Result,
};

pub(crate) const DEFAULT_DATA_TASK_NAME: &str = "default";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NormalizedTargetConfig {
    pub(crate) name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) data: Vec<NormalizedTargetDataConfig>,
    pub(crate) code: Vec<NormalizedCodegenTaskConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedTargetDataConfig {
    pub(crate) name: String,
    pub(crate) input: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) format: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NormalizedCodegenTaskConfig {
    pub(crate) task: CodegenTaskConfig,
    pub(crate) selected_data_name: Option<String>,
}

impl TargetConfig {
    pub fn target_tags(&self) -> TargetTags {
        TargetTags::new(self.tags.clone())
    }

    pub(crate) fn normalize(&self) -> Result<NormalizedTargetConfig> {
        let data = normalize_target_data_tasks(&self.name, &self.data)?;
        let code = normalize_codegen_tasks(&self.name, &data, &self.code)?;
        Ok(NormalizedTargetConfig {
            name: self.name.clone(),
            tags: self.tags.clone(),
            data,
            code,
        })
    }
}

impl NormalizedTargetConfig {
    pub(crate) fn target_tags(&self) -> TargetTags {
        TargetTags::new(self.tags.clone())
    }
}

pub(crate) fn normalize_target_data_tasks(
    target_name: &str,
    data_tasks: &[TargetDataConfig],
) -> Result<Vec<NormalizedTargetDataConfig>> {
    let mut normalized = Vec::with_capacity(data_tasks.len());
    let mut seen = std::collections::BTreeSet::new();
    for data_task in data_tasks {
        let name = match (&data_task.name, data_tasks.len()) {
            (Some(name), _) => {
                validate_data_task_name(target_name, name)?;
                name.clone()
            }
            (None, 1) => DEFAULT_DATA_TASK_NAME.to_owned(),
            (None, _) => {
                return Err(ConfigForgeError::Validation(format!(
                    "target `{target_name}` declares multiple data tasks, but one is missing `name`"
                )));
            }
        };
        if !seen.insert(name.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "target `{target_name}` declares duplicate data task `{name}`"
            )));
        }
        normalized.push(NormalizedTargetDataConfig {
            name,
            input: data_task.input.clone(),
            output: data_task.output.clone(),
            format: data_task.format.clone(),
        });
    }
    Ok(normalized)
}

fn validate_data_task_name(target_name: &str, name: &str) -> Result<()> {
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
            "target `{target_name}` declares invalid data task name `{name}`; data task names must use ASCII letters, digits, `_` or `-`, and start and end with a letter or digit"
        )))
    }
}

fn normalize_codegen_tasks(
    target_name: &str,
    data_tasks: &[NormalizedTargetDataConfig],
    code_tasks: &[CodegenTaskConfig],
) -> Result<Vec<NormalizedCodegenTaskConfig>> {
    code_tasks
        .iter()
        .map(|task| {
            let selected_data_name = match (&task.data, data_tasks.len()) {
                (Some(name), _) => {
                    if data_tasks.iter().any(|data_task| data_task.name == *name) {
                        Some(name.clone())
                    } else {
                        return Err(ConfigForgeError::Validation(format!(
                            "target `{target_name}` code task for `{}` references unknown data task `{name}`",
                            task.language
                        )));
                    }
                }
                (None, 0) => None,
                (None, 1) => Some(data_tasks[0].name.clone()),
                (None, _) => {
                    return Err(ConfigForgeError::Validation(format!(
                        "target `{target_name}` code task for `{}` must set `data` when multiple data tasks are configured",
                        task.language
                    )));
                }
            };
            Ok(NormalizedCodegenTaskConfig {
                task: task.clone(),
                selected_data_name,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_single_unnamed_data_task_to_default() {
        let target = TargetConfig {
            name: "client".to_owned(),
            tags: vec!["client".to_owned()],
            data: vec![TargetDataConfig {
                name: None,
                input: PathBuf::from("Data"),
                output: PathBuf::from("Generated/Data"),
                format: "json".to_owned(),
            }],
            code: vec![CodegenTaskConfig {
                language: "typescript".to_owned(),
                output: PathBuf::from("Generated/TypeScript"),
                data: None,
                module_name: None,
                namespace: None,
                file_header: None,
                keyword_conflict: None,
                naming: Default::default(),
                formatting: Default::default(),
                language_options: Default::default(),
            }],
        };

        let normalized = target.normalize().expect("target should normalize");

        assert_eq!(normalized.data[0].name, DEFAULT_DATA_TASK_NAME);
        assert_eq!(
            normalized.code[0].selected_data_name.as_deref(),
            Some(DEFAULT_DATA_TASK_NAME)
        );
    }

    #[test]
    fn explicit_default_data_binding_matches_single_unnamed_data_task() {
        let target = TargetConfig {
            name: "client".to_owned(),
            tags: Vec::new(),
            data: vec![TargetDataConfig {
                name: None,
                input: PathBuf::from("Data"),
                output: PathBuf::from("Generated/Data"),
                format: "json".to_owned(),
            }],
            code: vec![CodegenTaskConfig {
                language: "rust".to_owned(),
                output: PathBuf::from("Generated/Rust"),
                data: Some(DEFAULT_DATA_TASK_NAME.to_owned()),
                module_name: None,
                namespace: None,
                file_header: None,
                keyword_conflict: None,
                naming: Default::default(),
                formatting: Default::default(),
                language_options: Default::default(),
            }],
        };

        let normalized = target.normalize().expect("target should normalize");

        assert_eq!(
            normalized.code[0].selected_data_name.as_deref(),
            Some(DEFAULT_DATA_TASK_NAME)
        );
    }

    #[test]
    fn rejects_ambiguous_codegen_data_binding() {
        let target = TargetConfig {
            name: "client".to_owned(),
            tags: Vec::new(),
            data: vec![
                TargetDataConfig {
                    name: Some("json".to_owned()),
                    input: PathBuf::from("Data"),
                    output: PathBuf::from("Generated/Json"),
                    format: "json".to_owned(),
                },
                TargetDataConfig {
                    name: Some("messagepack".to_owned()),
                    input: PathBuf::from("Data"),
                    output: PathBuf::from("Generated/MessagePack"),
                    format: "messagepack".to_owned(),
                },
            ],
            code: vec![CodegenTaskConfig {
                language: "cpp".to_owned(),
                output: PathBuf::from("Generated/Cpp"),
                data: None,
                module_name: None,
                namespace: None,
                file_header: None,
                keyword_conflict: None,
                naming: Default::default(),
                formatting: Default::default(),
                language_options: Default::default(),
            }],
        };

        let error = target.normalize().unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("must set `data`"))
        );
    }

    #[test]
    fn rejects_invalid_data_task_names() {
        let error = normalize_target_data_tasks(
            "client",
            &[TargetDataConfig {
                name: Some("bad name".to_owned()),
                input: PathBuf::from("Data"),
                output: PathBuf::from("Generated/Data"),
                format: "json".to_owned(),
            }],
        )
        .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("invalid data task name"))
        );
    }

    #[test]
    fn rejects_duplicate_data_task_names() {
        let error = normalize_target_data_tasks(
            "client",
            &[
                TargetDataConfig {
                    name: Some("json".to_owned()),
                    input: PathBuf::from("Data"),
                    output: PathBuf::from("Generated/DataA"),
                    format: "json".to_owned(),
                },
                TargetDataConfig {
                    name: Some("json".to_owned()),
                    input: PathBuf::from("Data"),
                    output: PathBuf::from("Generated/DataB"),
                    format: "messagepack".to_owned(),
                },
            ],
        )
        .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("duplicate data task `json`"))
        );
    }
}
