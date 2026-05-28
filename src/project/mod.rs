// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    codegen::{CodeExportProject, CodegenTaskConfig},
    exporter::TargetDataConfig,
    meta::{EnumMeta, MetaModel, StructMeta, TableMeta},
    ConfigForgeError, Result,
};

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

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TargetConfig {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub data: Vec<TargetDataConfig>,
    #[serde(default)]
    pub code: Vec<CodegenTaskConfig>,
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

    pub(crate) fn target(&self, target_name: &str) -> Result<&TargetConfig> {
        self.targets
            .iter()
            .find(|target| target.name == target_name)
            .ok_or_else(|| {
                ConfigForgeError::Validation(format!("target `{target_name}` not found"))
            })
    }

    pub(crate) fn raw_meta_model(&self) -> MetaModel {
        MetaModel {
            enums: self.enums.clone(),
            structs: self.structs.clone(),
            tables: self.tables.clone(),
        }
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
        for data in &mut target.data {
            data.input = normalize_config_path(base_dir, &data.input);
            data.output = normalize_config_path(base_dir, &data.output);
        }
        for code in &mut target.code {
            code.output = normalize_config_path(base_dir, &code.output);
        }
    }
}

fn normalize_config_path(base_dir: &Path, path: &Path) -> PathBuf {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };
    lexical_normalize_path(&resolved)
}

pub(crate) fn lexical_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::ParentDir) | None => {
                    if !path.has_root() {
                        normalized.push(component.as_os_str());
                    }
                }
                Some(Component::Prefix(_)) | Some(Component::RootDir) | Some(Component::CurDir) => {
                }
            },
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
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

pub(crate) fn validate_unique_target_names(targets: &[TargetConfig]) -> Result<()> {
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

impl From<&ProjectConfigMeta> for CodeExportProject {
    fn from(project: &ProjectConfigMeta) -> Self {
        Self {
            name: project.name.clone(),
            desc: project.desc.clone(),
            copyright: project.copyright.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::exporter::TargetDataConfig;

    use super::*;

    #[test]
    fn lexical_normalize_path_collapses_current_and_parent_components() {
        assert_eq!(
            lexical_normalize_path(Path::new("out/./data/sub/../items.json")),
            PathBuf::from("out/data/items.json")
        );
    }

    #[test]
    fn lexical_normalize_path_preserves_leading_parent_components() {
        assert_eq!(
            lexical_normalize_path(Path::new("../../data/../meta")),
            PathBuf::from("../../meta")
        );
    }

    #[test]
    fn lexical_normalize_path_keeps_absolute_root_when_parent_escapes() {
        assert_eq!(
            lexical_normalize_path(Path::new("/../tmp/./data/..")),
            PathBuf::from("/tmp")
        );
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
    fn project_config_rejects_duplicate_target_names() {
        let error = validate_unique_target_names(&[
            TargetConfig {
                name: "client".to_owned(),
                tags: Vec::new(),
                data: Vec::new(),
                code: Vec::new(),
            },
            TargetConfig {
                name: "client".to_owned(),
                tags: Vec::new(),
                data: Vec::new(),
                code: Vec::new(),
            },
        ])
        .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("duplicate target `client`"))
        );
    }
}
