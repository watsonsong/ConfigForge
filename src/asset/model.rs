// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::meta::{AssetExtensionPolicy, AssetInputKind, AssetPathStyle, AssetRegexRuleMeta};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveAssetKind {
    pub name: String,
    pub extends: Option<String>,
    pub search_paths: Vec<String>,
    pub base_dirs: Vec<String>,
    pub extensions: Vec<String>,
    pub input: AssetInputKind,
    pub path_style: AssetPathStyle,
    pub extension_policy: AssetExtensionPolicy,
    pub strip_wrappers: Vec<String>,
    pub object_separator: String,
    pub object_name: Option<String>,
    pub case_sensitive: Option<bool>,
    pub require_existing: Option<bool>,
    pub require_sidecars: Vec<String>,
    pub allow_path_prefixes: Vec<String>,
    pub deny_path_prefixes: Vec<String>,
    pub deny_when_tags: Vec<String>,
    pub name_prefixes: Vec<String>,
    pub name_suffixes: Vec<String>,
    pub allow_patterns: Vec<String>,
    pub deny_patterns: Vec<String>,
    pub regex_rules: Vec<AssetRegexRuleMeta>,
    pub allow_auto_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAssetRef {
    pub raw_input: String,
    pub canonical_export_ref: String,
    pub logical_ref: String,
    pub search_path_id: String,
    pub search_path_relative_path: PathBuf,
    pub resolved_filesystem_path: PathBuf,
    pub object_name: Option<String>,
    pub completion_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AssetSearchPath {
    pub(super) name: String,
    pub(super) root: PathBuf,
    pub(super) mount: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AssetCandidate {
    pub(super) search_path_id: String,
    pub(super) search_path_relative_path: PathBuf,
    pub(super) resolved_filesystem_path: PathBuf,
}
