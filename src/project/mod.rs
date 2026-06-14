// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity,
    Result,
    diagnostic::code::{CF_BUILD_TARGET_NOT_FOUND, CF_PROJECT_INVALID_CONFIG},
    meta::{AssetKindMeta, EnumMeta, MetaModel, StructMeta, TableMeta},
    path_util::lexical_normalize_path,
};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ProjectConfig {
    #[serde(default)]
    pub imports: Vec<PathBuf>,
    #[serde(default)]
    pub project: ProjectConfigMeta,
    #[serde(default)]
    pub assets: AssetsConfig,
    #[serde(default)]
    pub enums: Vec<EnumMeta>,
    #[serde(default)]
    pub structs: Vec<StructMeta>,
    #[serde(default)]
    pub tables: Vec<TableMeta>,
    #[serde(default)]
    pub asset_kinds: Vec<AssetKindMeta>,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct AssetsConfig {
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    #[serde(default)]
    pub require_existing: Option<bool>,
    #[serde(default)]
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub search_paths: Vec<AssetSearchPathConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AssetSearchPathConfig {
    pub name: String,
    pub root: PathBuf,
    #[serde(default = "default_asset_search_path_kind")]
    pub kind: String,
    #[serde(default)]
    pub mount: Option<String>,
}

fn default_asset_search_path_kind() -> String {
    "filesystem".to_owned()
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct ProjectConfigMeta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub input: Option<PathBuf>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub copyright: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub exports: Vec<TargetExportConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TargetExportConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub type_id: String,
    pub output: PathBuf,
    #[serde(flatten)]
    pub options: std::collections::BTreeMap<String, toml::Value>,
}

impl ProjectConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let mut visited = BTreeSet::new();
        let mut fragments = Vec::new();
        load_project_config_recursive(path.as_ref(), &mut visited, &mut fragments)?;
        let merged = merge_project_configs(fragments);
        validate_unique_target_names(&merged.targets)?;
        validate_asset_search_paths(&merged.assets.search_paths)?;
        validate_asset_kind_search_path_refs(&merged)?;
        Ok(merged)
    }

    pub(crate) fn target(&self, target_name: &str) -> Result<&TargetConfig> {
        self.targets
            .iter()
            .find(|target| target.name == target_name)
            .ok_or_else(|| target_not_found_error(target_name))
    }

    /// Extracts the project-embedded Meta declarations as an unvalidated raw model.
    ///
    /// Project config loading validates project-level shape and paths, but build orchestration
    /// must still call `MetaModel::link()` on this value before passing it to data, validation,
    /// exporter, or codegen modules.
    pub(crate) fn raw_meta_model(&self) -> MetaModel {
        MetaModel {
            enums: self.enums.clone(),
            structs: self.structs.clone(),
            tables: self.tables.clone(),
            asset_kinds: self.asset_kinds.clone(),
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
    if let Some(input) = &mut config.project.input {
        *input = normalize_config_path(base_dir, input);
    }
    if let Some(root) = &mut config.assets.root {
        *root = normalize_config_path(base_dir, root);
        if !config
            .assets
            .search_paths
            .iter()
            .any(|search_path| search_path.name == "default")
        {
            config.assets.search_paths.push(AssetSearchPathConfig {
                name: "default".to_owned(),
                root: root.clone(),
                kind: default_asset_search_path_kind(),
                mount: None,
            });
        }
    }
    for search_path in &mut config.assets.search_paths {
        search_path.root = normalize_config_path(base_dir, &search_path.root);
    }
    for target in &mut config.targets {
        for export in &mut target.exports {
            export.output = normalize_config_path(base_dir, &export.output);
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

fn merge_project_configs(fragments: Vec<ProjectConfig>) -> ProjectConfig {
    let mut merged = ProjectConfig {
        imports: Vec::new(),
        project: ProjectConfigMeta::default(),
        assets: AssetsConfig::default(),
        enums: Vec::new(),
        structs: Vec::new(),
        tables: Vec::new(),
        asset_kinds: Vec::new(),
        targets: Vec::new(),
    };

    for mut fragment in fragments {
        if fragment.project.name.is_some() {
            merged.project.name = fragment.project.name.take();
        }
        if fragment.project.input.is_some() {
            merged.project.input = fragment.project.input.take();
        }
        if fragment.project.desc.is_some() {
            merged.project.desc = fragment.project.desc.take();
        }
        if fragment.project.copyright.is_some() {
            merged.project.copyright = fragment.project.copyright.take();
        }
        if fragment.assets.case_sensitive.is_some() {
            merged.assets.case_sensitive = fragment.assets.case_sensitive.take();
        }
        if fragment.assets.require_existing.is_some() {
            merged.assets.require_existing = fragment.assets.require_existing.take();
        }
        if fragment.assets.root.is_some() {
            merged.assets.root = fragment.assets.root.take();
        }
        merged
            .assets
            .search_paths
            .append(&mut fragment.assets.search_paths);
        merged.enums.append(&mut fragment.enums);
        merged.structs.append(&mut fragment.structs);
        merged.tables.append(&mut fragment.tables);
        merged.asset_kinds.append(&mut fragment.asset_kinds);
        merged.targets.append(&mut fragment.targets);
    }

    merged
}

pub(crate) fn validate_asset_search_paths(search_paths: &[AssetSearchPathConfig]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for search_path in search_paths {
        if !seen.insert(search_path.name.as_str()) {
            return Err(project_config_error(
                format!("duplicate asset search path `{}`", search_path.name),
                "project.assets",
                search_path.name.clone(),
                "unique assets.search_paths.name",
            ));
        }
        if search_path.kind != "filesystem" {
            return Err(project_config_error(
                format!(
                    "unsupported asset search path kind `{}` for `{}`; only `filesystem` is supported",
                    search_path.kind, search_path.name
                ),
                "project.assets",
                search_path.kind.clone(),
                "filesystem",
            ));
        }
    }

    Ok(())
}

pub(crate) fn validate_asset_kind_search_path_refs(config: &ProjectConfig) -> Result<()> {
    let search_path_names = config
        .assets
        .search_paths
        .iter()
        .map(|search_path| search_path.name.as_str())
        .collect::<BTreeSet<_>>();
    for asset_kind in &config.asset_kinds {
        for search_path in &asset_kind.search_paths {
            if !search_path_names.contains(search_path.as_str()) {
                return Err(project_config_error(
                    format!(
                        "unknown asset search path `{search_path}` referenced by asset kind `{}`",
                        asset_kind.name
                    ),
                    "project.asset_kinds",
                    search_path.clone(),
                    "declared assets.search_paths.name",
                ));
            }
        }
    }

    Ok(())
}

pub(crate) fn validate_unique_target_names(targets: &[TargetConfig]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for target in targets {
        if !seen.insert(target.name.as_str()) {
            return Err(project_config_error(
                format!("duplicate target `{}`", target.name),
                "project.targets",
                target.name.clone(),
                "unique targets.name",
            ));
        }
    }

    Ok(())
}

impl From<&ProjectConfigMeta> for ArtifactProjectInfo {
    fn from(project: &ProjectConfigMeta) -> Self {
        Self {
            name: project.name.clone(),
            desc: project.desc.clone(),
            copyright: project.copyright.clone(),
        }
    }
}

fn target_not_found_error(target_name: &str) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_BUILD_TARGET_NOT_FOUND,
            DiagnosticSeverity::Error,
            format!("target `{target_name}` not found"),
            DiagnosticLocation::default(),
        )
        .with_source("project.target")
        .with_domain("build")
        .with_target(target_name)
        .with_current_value(target_name)
        .with_expected("declared target name")
        .with_hint("Check the CLI --target value and [[targets]].name in ConfigForge.toml."),
    )
}

fn project_config_error(
    message: String,
    source: &'static str,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_PROJECT_INVALID_CONFIG,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(source)
        .with_domain("project")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(
            "Check ConfigForge.toml project, assets, asset_kinds, and targets declarations.",
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;

    fn expect_single_diagnostic(error: ConfigForgeError) -> Diagnostic {
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected structured diagnostic report");
        };
        assert_eq!(report.diagnostics.len(), 1);
        report.diagnostics.into_iter().next().unwrap()
    }

    #[test]
    fn parses_messagepack_export_type_and_alias() {
        let config: TargetExportConfig = toml::from_str(
            r#"
                output = "Generated/Data"
                type = "messagepack"
            "#,
        )
        .expect("messagepack export should parse");
        assert_eq!(config.type_id, "messagepack");

        let alias_config: TargetExportConfig = toml::from_str(
            r#"
                output = "Generated/Data"
                type = "msgpack"
            "#,
        )
        .expect("msgpack alias should parse");
        assert_eq!(alias_config.type_id, "msgpack");
    }

    #[test]
    fn project_config_rejects_duplicate_target_names() {
        let error = validate_unique_target_names(&[
            TargetConfig {
                name: "client".to_owned(),
                tags: Vec::new(),
                exports: Vec::new(),
            },
            TargetConfig {
                name: "client".to_owned(),
                tags: Vec::new(),
                exports: Vec::new(),
            },
        ])
        .unwrap_err();

        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, CF_PROJECT_INVALID_CONFIG);
        assert!(diagnostic.message.contains("duplicate target `client`"));
        assert_eq!(diagnostic.source.as_deref(), Some("project.targets"));
        assert_eq!(diagnostic.domain.as_deref(), Some("project"));
        assert_eq!(diagnostic.current_value.as_deref(), Some("client"));
        assert_eq!(diagnostic.expected.as_deref(), Some("unique targets.name"));
    }

    #[test]
    fn project_config_normalizes_asset_search_path_roots() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("ConfigForge.toml");
        fs::write(
            &project_path,
            r#"
                [assets]
                case_sensitive = true
                require_existing = true

                [[assets.search_paths]]
                name = "game"
                root = "Content/./UI/.."
                mount = "/Game"
            "#,
        )
        .expect("write project config");

        let config = ProjectConfig::from_path(&project_path).expect("project config should load");

        assert_eq!(config.assets.case_sensitive, Some(true));
        assert_eq!(config.assets.require_existing, Some(true));
        assert_eq!(config.assets.search_paths.len(), 1);
        let search_path = &config.assets.search_paths[0];
        assert_eq!(search_path.name, "game");
        assert_eq!(search_path.kind, "filesystem");
        assert_eq!(search_path.mount.as_deref(), Some("/Game"));
        assert_eq!(
            search_path.root,
            fs::canonicalize(temp_dir.path())
                .expect("canonical temp dir")
                .join("Content")
        );
    }

    #[test]
    fn project_config_assets_root_adds_default_search_path() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("ConfigForge.toml");
        fs::write(
            &project_path,
            r#"
                [assets]
                root = "Assets/../Content"
            "#,
        )
        .expect("write project config");

        let config = ProjectConfig::from_path(&project_path).expect("project config should load");

        let expected_root = fs::canonicalize(temp_dir.path())
            .expect("canonical temp dir")
            .join("Content");
        assert_eq!(config.assets.root, Some(expected_root.clone()));
        assert_eq!(config.assets.search_paths.len(), 1);
        let search_path = &config.assets.search_paths[0];
        assert_eq!(search_path.name, "default");
        assert_eq!(search_path.root, expected_root);
    }

    #[test]
    fn project_config_rejects_duplicate_asset_search_path_names() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("ConfigForge.toml");
        fs::write(
            &project_path,
            r#"
                [[assets.search_paths]]
                name = "game"
                root = "Content"

                [[assets.search_paths]]
                name = "game"
                root = "Plugins"
            "#,
        )
        .expect("write project config");

        let error = ProjectConfig::from_path(&project_path).unwrap_err();

        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, CF_PROJECT_INVALID_CONFIG);
        assert!(
            diagnostic
                .message
                .contains("duplicate asset search path `game`")
        );
        assert_eq!(diagnostic.source.as_deref(), Some("project.assets"));
        assert_eq!(diagnostic.domain.as_deref(), Some("project"));
        assert_eq!(diagnostic.current_value.as_deref(), Some("game"));
        assert_eq!(
            diagnostic.expected.as_deref(),
            Some("unique assets.search_paths.name")
        );
    }

    #[test]
    fn project_config_rejects_unsupported_asset_search_path_kind() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("ConfigForge.toml");
        fs::write(
            &project_path,
            r#"
                [[assets.search_paths]]
                name = "registry"
                root = "Content"
                kind = "unreal_registry"
            "#,
        )
        .expect("write project config");

        let error = ProjectConfig::from_path(&project_path).unwrap_err();

        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, CF_PROJECT_INVALID_CONFIG);
        assert!(
            diagnostic
                .message
                .contains("unsupported asset search path kind `unreal_registry`")
        );
        assert_eq!(diagnostic.source.as_deref(), Some("project.assets"));
        assert_eq!(diagnostic.domain.as_deref(), Some("project"));
        assert_eq!(diagnostic.current_value.as_deref(), Some("unreal_registry"));
        assert_eq!(diagnostic.expected.as_deref(), Some("filesystem"));
    }

    #[test]
    fn project_config_rejects_unknown_asset_kind_search_path_ref() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("ConfigForge.toml");
        fs::write(
            &project_path,
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["missing"]
            "#,
        )
        .expect("write project config");

        let error = ProjectConfig::from_path(&project_path).unwrap_err();

        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, CF_PROJECT_INVALID_CONFIG);
        assert!(
            diagnostic
                .message
                .contains("unknown asset search path `missing` referenced by asset kind `ui_icon`")
        );
        assert_eq!(diagnostic.source.as_deref(), Some("project.asset_kinds"));
        assert_eq!(diagnostic.domain.as_deref(), Some("project"));
        assert_eq!(diagnostic.current_value.as_deref(), Some("missing"));
        assert_eq!(
            diagnostic.expected.as_deref(),
            Some("declared assets.search_paths.name")
        );
    }

    #[test]
    fn target_config_rejects_legacy_data_and_code_fields() {
        let data_error = toml::from_str::<TargetConfig>(
            r#"
                name = "client"

                [[data]]
                output = "Generated/Data"
                type = "json"
            "#,
        )
        .unwrap_err();
        assert!(data_error.to_string().contains("unknown field `data`"));

        let code_error = toml::from_str::<TargetConfig>(
            r#"
                name = "client"

                [[code]]
                output = "Generated/Code"
                type = "rust"
            "#,
        )
        .unwrap_err();
        assert!(code_error.to_string().contains("unknown field `code`"));
    }

    #[test]
    fn standard_project_config_includes_java_client_target() {
        let config = ProjectConfig::from_path(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/standard_project/ConfigForge.toml"),
        )
        .expect("standard project config should load");
        let client = config.target("client").expect("client target");
        let java = client
            .exports
            .iter()
            .find(|export| export.type_id == "java")
            .expect("java client target should exist");

        assert_eq!(
            java.output,
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/standard_project/out/client_code/java")
        );
        assert_eq!(
            java.options.get("file_name").and_then(toml::Value::as_str),
            Some("ReferenceGameConfig")
        );
        let java_options = java
            .options
            .get("java")
            .and_then(toml::Value::as_table)
            .expect("java options");
        assert_eq!(
            java_options
                .get("root_class_name")
                .and_then(toml::Value::as_str),
            Some("ReferenceGameConfig")
        );
    }
}
