// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

use crate::{
    ConfigForgeError, Result,
    meta::{
        AssetExtensionPolicy, AssetInputKind, AssetKindMeta, AssetPathStyle, AssetRegexRuleMeta,
        AssetRegexTarget, LinkedMetaModel, TargetTags, ValidationSeverity,
    },
    project::{AssetSearchPathConfig, AssetsConfig},
};

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
struct AssetSearchPath {
    name: String,
    root: PathBuf,
    mount: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssetCandidate {
    search_path_id: String,
    search_path_relative_path: PathBuf,
    resolved_filesystem_path: PathBuf,
}

pub struct AssetResolver {
    search_paths: BTreeMap<String, AssetSearchPath>,
    effective_kinds: BTreeMap<String, EffectiveAssetKind>,
    case_sensitive: Option<bool>,
    require_existing: Option<bool>,
}

impl AssetResolver {
    pub fn new(meta: &LinkedMetaModel<'_>, assets: &AssetsConfig) -> Result<Self> {
        let search_paths = assets
            .search_paths
            .iter()
            .map(|search_path| {
                (
                    search_path.name.clone(),
                    AssetSearchPath::from_config(search_path),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut effective_kinds = BTreeMap::new();
        for asset_kind in &meta.model.asset_kinds {
            let effective = effective_asset_kind(asset_kind, meta)?;
            effective_kinds.insert(effective.name.clone(), effective);
        }

        Ok(Self {
            search_paths,
            effective_kinds,
            case_sensitive: assets.case_sensitive,
            require_existing: assets.require_existing,
        })
    }

    pub fn effective_kind(&self, name: &str) -> Option<&EffectiveAssetKind> {
        self.effective_kinds.get(name)
    }

    pub fn resolve(&self, asset_kind: &str, raw_input: &str) -> Result<ResolvedAssetRef> {
        self.resolve_for_target(asset_kind, raw_input, &TargetTags::empty())
    }

    pub fn resolve_for_target(
        &self,
        asset_kind: &str,
        raw_input: &str,
        target_tags: &TargetTags,
    ) -> Result<ResolvedAssetRef> {
        let kind = self.effective_kind(asset_kind).ok_or_else(|| {
            ConfigForgeError::Validation(format!("unknown asset kind `{asset_kind}`"))
        })?;
        let resolved = self.resolve_with_kind(kind, raw_input)?;
        self.validate_resolved(kind, &resolved, target_tags)?;
        Ok(resolved)
    }

    fn resolve_with_kind(
        &self,
        kind: &EffectiveAssetKind,
        raw_input: &str,
    ) -> Result<ResolvedAssetRef> {
        if !matches!(kind.input, AssetInputKind::Path | AssetInputKind::String) {
            return Err(ConfigForgeError::Validation(format!(
                "asset kind `{}` uses unsupported input mode",
                kind.name
            )));
        }
        let stripped_input = strip_asset_wrappers(raw_input.trim(), &kind.strip_wrappers);
        match kind.path_style {
            AssetPathStyle::Relative => self.resolve_relative(kind, raw_input, &stripped_input),
            AssetPathStyle::Mounted => self.resolve_mounted(kind, raw_input, &stripped_input),
            AssetPathStyle::MountedObject => {
                self.resolve_mounted_object(kind, raw_input, &stripped_input)
            }
        }
    }

    fn resolve_relative(
        &self,
        kind: &EffectiveAssetKind,
        raw_input: &str,
        stripped_input: &str,
    ) -> Result<ResolvedAssetRef> {
        let relative_input = asset_relative_path(stripped_input)?;
        let candidates = self.generate_candidates(kind, &relative_input)?;
        let candidate = select_unique_candidate(&kind.name, stripped_input, candidates)?;
        let logical_ref = path_to_asset_string(&candidate.search_path_relative_path);
        Ok(ResolvedAssetRef {
            raw_input: raw_input.to_owned(),
            canonical_export_ref: logical_ref.clone(),
            logical_ref,
            search_path_id: candidate.search_path_id,
            search_path_relative_path: candidate.search_path_relative_path,
            resolved_filesystem_path: candidate.resolved_filesystem_path,
            object_name: None,
            completion_applied: false,
        })
    }

    fn resolve_mounted(
        &self,
        kind: &EffectiveAssetKind,
        raw_input: &str,
        stripped_input: &str,
    ) -> Result<ResolvedAssetRef> {
        let (search_path, relative_input) = self.mounted_relative(kind, stripped_input)?;
        let candidates =
            self.generate_candidates_for_search_path(kind, search_path, &relative_input)?;
        let candidate = select_unique_candidate(&kind.name, stripped_input, candidates)?;
        let logical_ref = mounted_ref(search_path, &candidate.search_path_relative_path);
        Ok(ResolvedAssetRef {
            raw_input: raw_input.to_owned(),
            canonical_export_ref: logical_ref.clone(),
            logical_ref,
            search_path_id: candidate.search_path_id,
            search_path_relative_path: candidate.search_path_relative_path,
            resolved_filesystem_path: candidate.resolved_filesystem_path,
            object_name: None,
            completion_applied: false,
        })
    }

    fn resolve_mounted_object(
        &self,
        kind: &EffectiveAssetKind,
        raw_input: &str,
        stripped_input: &str,
    ) -> Result<ResolvedAssetRef> {
        if let Some((package_ref, object_name)) = split_mounted_object(stripped_input, kind) {
            let (search_path, relative_input) = self.mounted_relative(kind, package_ref)?;
            let candidates =
                self.generate_candidates_for_search_path(kind, search_path, &relative_input)?;
            let candidate = select_unique_candidate(&kind.name, stripped_input, candidates)?;
            let canonical_export_ref = mounted_object_ref(
                search_path,
                &without_extension(&candidate.search_path_relative_path),
                object_name,
                &kind.object_separator,
            );
            Ok(ResolvedAssetRef {
                raw_input: raw_input.to_owned(),
                canonical_export_ref: canonical_export_ref.clone(),
                logical_ref: canonical_export_ref,
                search_path_id: candidate.search_path_id,
                search_path_relative_path: candidate.search_path_relative_path,
                resolved_filesystem_path: candidate.resolved_filesystem_path,
                object_name: Some(object_name.to_owned()),
                completion_applied: stripped_input != raw_input.trim(),
            })
        } else {
            if !kind.allow_auto_complete {
                return Err(ConfigForgeError::Validation(format!(
                    "asset kind `{}` requires a mounted object ref, but `{stripped_input}` is not one",
                    kind.name
                )));
            }
            let relative_input = asset_relative_path(stripped_input)?;
            let candidates = self.generate_candidates(kind, &relative_input)?;
            let candidate = select_unique_candidate(&kind.name, stripped_input, candidates)?;
            let package_relative = without_extension(&candidate.search_path_relative_path);
            let stem = package_relative
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    ConfigForgeError::Validation(format!(
                        "asset kind `{}` cannot derive object name for `{stripped_input}`",
                        kind.name
                    ))
                })?;
            let object_name = render_object_name(kind.object_name.as_deref(), stem);
            let search_path = self.search_path(&candidate.search_path_id)?;
            let canonical_export_ref = mounted_object_ref(
                search_path,
                &package_relative,
                &object_name,
                &kind.object_separator,
            );
            Ok(ResolvedAssetRef {
                raw_input: raw_input.to_owned(),
                canonical_export_ref: canonical_export_ref.clone(),
                logical_ref: canonical_export_ref,
                search_path_id: candidate.search_path_id,
                search_path_relative_path: candidate.search_path_relative_path,
                resolved_filesystem_path: candidate.resolved_filesystem_path,
                object_name: Some(object_name),
                completion_applied: true,
            })
        }
    }

    fn mounted_relative<'a>(
        &'a self,
        kind: &EffectiveAssetKind,
        input: &str,
    ) -> Result<(&'a AssetSearchPath, PathBuf)> {
        for search_path in self.search_paths_for_kind(kind)? {
            let Some(mount) = search_path.mount.as_deref() else {
                continue;
            };
            if let Some(relative) = strip_mount_prefix(input, mount) {
                return Ok((search_path, asset_relative_path(relative)?));
            }
        }

        Err(ConfigForgeError::Validation(format!(
            "asset kind `{}` input `{input}` does not match any configured mount",
            kind.name
        )))
    }

    fn generate_candidates(
        &self,
        kind: &EffectiveAssetKind,
        relative_input: &Path,
    ) -> Result<Vec<AssetCandidate>> {
        let mut candidates = Vec::new();
        for search_path in self.search_paths_for_kind(kind)? {
            candidates.extend(self.generate_candidates_for_search_path(
                kind,
                search_path,
                relative_input,
            )?);
        }
        Ok(candidates)
    }

    fn generate_candidates_for_search_path(
        &self,
        kind: &EffectiveAssetKind,
        search_path: &AssetSearchPath,
        relative_input: &Path,
    ) -> Result<Vec<AssetCandidate>> {
        let base_dirs = if kind.base_dirs.is_empty() {
            vec![PathBuf::new()]
        } else {
            kind.base_dirs
                .iter()
                .map(|base_dir| asset_relative_path(base_dir))
                .collect::<Result<Vec<_>>>()?
        };
        let candidate_inputs = candidate_relative_paths(kind, relative_input)?;
        let mut seen = BTreeSet::new();
        let mut candidates = Vec::new();
        for base_dir in base_dirs {
            for candidate_input in &candidate_inputs {
                let search_path_relative_path = if base_dir.as_os_str().is_empty() {
                    candidate_input.clone()
                } else {
                    base_dir.join(candidate_input)
                };
                ensure_relative_asset_path(&search_path_relative_path)?;
                if seen.insert((search_path.name.clone(), search_path_relative_path.clone())) {
                    candidates.push(AssetCandidate {
                        search_path_id: search_path.name.clone(),
                        resolved_filesystem_path: search_path.root.join(&search_path_relative_path),
                        search_path_relative_path,
                    });
                }
            }
        }
        Ok(candidates)
    }

    fn search_paths_for_kind(&self, kind: &EffectiveAssetKind) -> Result<Vec<&AssetSearchPath>> {
        if kind.search_paths.is_empty() {
            return Ok(self.search_paths.values().collect());
        }
        kind.search_paths
            .iter()
            .map(|name| self.search_path(name))
            .collect()
    }

    fn search_path(&self, name: &str) -> Result<&AssetSearchPath> {
        self.search_paths.get(name).ok_or_else(|| {
            ConfigForgeError::Validation(format!("asset search path `{name}` not found"))
        })
    }

    fn validate_resolved(
        &self,
        kind: &EffectiveAssetKind,
        resolved: &ResolvedAssetRef,
        target_tags: &TargetTags,
    ) -> Result<()> {
        let require_existing = kind
            .require_existing
            .or(self.require_existing)
            .unwrap_or(false);
        if require_existing && !resolved.resolved_filesystem_path.is_file() {
            return Err(ConfigForgeError::Validation(format!(
                "asset kind `{}` resolved `{}` to missing file `{}`",
                kind.name,
                resolved.raw_input,
                resolved.resolved_filesystem_path.display()
            )));
        }

        for sidecar_template in &kind.require_sidecars {
            let sidecar_path = sidecar_path_from_template(sidecar_template, resolved);
            if !sidecar_path.is_file() {
                return Err(ConfigForgeError::Validation(format!(
                    "asset kind `{}` requires missing sidecar `{}` for `{}`",
                    kind.name,
                    sidecar_path.display(),
                    resolved.raw_input
                )));
            }
        }

        let name = asset_rule_name(resolved);
        if !kind.name_prefixes.is_empty()
            && !kind
                .name_prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
        {
            return Err(ConfigForgeError::Validation(format!(
                "asset kind `{}` name `{name}` does not match required prefixes {:?}",
                kind.name, kind.name_prefixes
            )));
        }
        if !kind.name_suffixes.is_empty()
            && !kind
                .name_suffixes
                .iter()
                .any(|suffix| name.ends_with(suffix))
        {
            return Err(ConfigForgeError::Validation(format!(
                "asset kind `{}` name `{name}` does not match required suffixes {:?}",
                kind.name, kind.name_suffixes
            )));
        }

        if let Some(template) = kind.object_name.as_deref() {
            if let Some(object_name) = resolved.object_name.as_deref() {
                let stem = asset_stem(resolved);
                let expected = render_object_name(Some(template), &stem);
                if object_name != expected {
                    return Err(ConfigForgeError::Validation(format!(
                        "asset kind `{}` object name `{object_name}` must be `{expected}`",
                        kind.name
                    )));
                }
            }
        }

        if !kind.allow_path_prefixes.is_empty()
            && !kind
                .allow_path_prefixes
                .iter()
                .any(|prefix| resolved.logical_ref.starts_with(prefix))
        {
            return Err(ConfigForgeError::Validation(format!(
                "asset kind `{}` logical ref `{}` is outside allowed prefixes {:?}",
                kind.name, resolved.logical_ref, kind.allow_path_prefixes
            )));
        }
        let deny_rules_enabled = kind.deny_when_tags.is_empty()
            || kind
                .deny_when_tags
                .iter()
                .any(|tag| target_tags.contains(tag));
        if deny_rules_enabled {
            if let Some(prefix) = kind
                .deny_path_prefixes
                .iter()
                .find(|prefix| resolved.logical_ref.starts_with(prefix.as_str()))
            {
                return Err(ConfigForgeError::Validation(format!(
                    "asset kind `{}` logical ref `{}` is denied by prefix `{prefix}`",
                    kind.name, resolved.logical_ref
                )));
            }
            validate_simple_patterns(kind, resolved)?;
        } else {
            validate_allow_patterns(kind, resolved)?;
        }
        validate_regex_rules(kind, resolved, target_tags)?;

        let case_sensitive = kind.case_sensitive.or(self.case_sensitive).unwrap_or(false);
        if case_sensitive && resolved.resolved_filesystem_path.exists() {
            validate_case_sensitive_path(&resolved.resolved_filesystem_path)?;
        }

        Ok(())
    }
}

impl AssetSearchPath {
    fn from_config(config: &AssetSearchPathConfig) -> Self {
        Self {
            name: config.name.clone(),
            root: config.root.clone(),
            mount: config.mount.clone(),
        }
    }
}

fn effective_asset_kind(
    asset_kind: &AssetKindMeta,
    meta: &LinkedMetaModel<'_>,
) -> Result<EffectiveAssetKind> {
    let mut chain = Vec::new();
    let mut current = asset_kind;
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current.name.as_str()) {
            return Err(ConfigForgeError::Validation(format!(
                "asset kind extends cycle detected while resolving `{}`",
                asset_kind.name
            )));
        }
        chain.push(current);
        let Some(parent) = current.extends.as_deref() else {
            break;
        };
        current = meta.asset_kind(parent).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "asset kind `{}` extends unknown asset kind `{parent}`",
                current.name
            ))
        })?;
    }

    let mut effective =
        EffectiveAssetKind::from_raw(chain.pop().expect("asset kind chain is non-empty"));
    while let Some(child) = chain.pop() {
        effective.apply_child(child);
    }
    Ok(effective)
}

impl EffectiveAssetKind {
    fn from_raw(raw: &AssetKindMeta) -> Self {
        Self {
            name: raw.name.clone(),
            extends: raw.extends.clone(),
            search_paths: raw.search_paths.clone(),
            base_dirs: raw.base_dirs.clone(),
            extensions: raw.extensions.clone(),
            input: raw.input.unwrap_or(AssetInputKind::Path),
            path_style: raw.path_style.unwrap_or(AssetPathStyle::Relative),
            extension_policy: raw
                .extension_policy
                .unwrap_or(AssetExtensionPolicy::Required),
            strip_wrappers: raw.strip_wrappers.clone(),
            object_separator: raw
                .object_separator
                .clone()
                .unwrap_or_else(|| ".".to_owned()),
            object_name: raw.object_name.clone(),
            case_sensitive: raw.case_sensitive,
            require_existing: raw.require_existing,
            require_sidecars: raw.require_sidecars.clone(),
            allow_path_prefixes: raw.allow_path_prefixes.clone(),
            deny_path_prefixes: raw.deny_path_prefixes.clone(),
            deny_when_tags: raw.deny_when_tags.clone(),
            name_prefixes: raw.name_prefixes.clone(),
            name_suffixes: raw.name_suffixes.clone(),
            allow_patterns: raw.allow_patterns.clone(),
            deny_patterns: raw.deny_patterns.clone(),
            regex_rules: raw.regex_rules.clone(),
            allow_auto_complete: raw.allow_auto_complete,
        }
    }

    fn apply_child(&mut self, child: &AssetKindMeta) {
        self.name = child.name.clone();
        self.extends = child.extends.clone();
        if !child.search_paths.is_empty() {
            self.search_paths = child.search_paths.clone();
        }
        if !child.base_dirs.is_empty() {
            self.base_dirs = child.base_dirs.clone();
        }
        if !child.extensions.is_empty() {
            self.extensions = child.extensions.clone();
        }
        if let Some(input) = child.input {
            self.input = input;
        }
        if let Some(path_style) = child.path_style {
            self.path_style = path_style;
        }
        if let Some(extension_policy) = child.extension_policy {
            self.extension_policy = extension_policy;
        }
        if !child.strip_wrappers.is_empty() {
            self.strip_wrappers = child.strip_wrappers.clone();
        }
        if let Some(object_separator) = &child.object_separator {
            self.object_separator = object_separator.clone();
        }
        if let Some(object_name) = &child.object_name {
            self.object_name = Some(object_name.clone());
        }
        if child.case_sensitive.is_some() {
            self.case_sensitive = child.case_sensitive;
        }
        if child.require_existing.is_some() {
            self.require_existing = child.require_existing;
        }
        append_non_empty(&mut self.require_sidecars, &child.require_sidecars);
        append_non_empty(&mut self.allow_path_prefixes, &child.allow_path_prefixes);
        append_non_empty(&mut self.deny_path_prefixes, &child.deny_path_prefixes);
        append_non_empty(&mut self.deny_when_tags, &child.deny_when_tags);
        append_non_empty(&mut self.name_prefixes, &child.name_prefixes);
        append_non_empty(&mut self.name_suffixes, &child.name_suffixes);
        append_non_empty(&mut self.allow_patterns, &child.allow_patterns);
        append_non_empty(&mut self.deny_patterns, &child.deny_patterns);
        if !child.regex_rules.is_empty() {
            self.regex_rules.extend(child.regex_rules.clone());
        }
        self.allow_auto_complete |= child.allow_auto_complete;
    }
}

fn append_non_empty(target: &mut Vec<String>, source: &[String]) {
    if !source.is_empty() {
        target.extend(source.iter().cloned());
    }
}

fn strip_asset_wrappers<'a>(input: &'a str, wrappers: &[String]) -> String {
    for wrapper in wrappers {
        let Some((prefix, suffix)) = wrapper.split_once("{value}") else {
            continue;
        };
        if input.starts_with(prefix)
            && input.ends_with(suffix)
            && input.len() >= prefix.len() + suffix.len()
        {
            let end = input.len() - suffix.len();
            return input[prefix.len()..end].to_owned();
        }
    }
    input.to_owned()
}

fn asset_relative_path(path: &str) -> Result<PathBuf> {
    let normalized = path.replace('\\', "/");
    if Path::new(&normalized).is_absolute() || normalized.starts_with('/') {
        return Err(ConfigForgeError::Validation(format!(
            "asset path `{path}` must be relative"
        )));
    }
    let trimmed = normalized.trim_matches('/');
    let relative = PathBuf::from(trimmed);
    ensure_relative_asset_path(&relative)?;
    Ok(relative)
}

fn ensure_relative_asset_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(ConfigForgeError::Validation(
            "asset path must be non-empty".to_owned(),
        ));
    }
    if path.is_absolute() {
        return Err(ConfigForgeError::Validation(format!(
            "asset path `{}` must be relative",
            path.display()
        )));
    }
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir
        ) {
            return Err(ConfigForgeError::Validation(format!(
                "asset path `{}` must not escape its search path",
                path.display()
            )));
        }
    }
    Ok(())
}

fn candidate_relative_paths(
    kind: &EffectiveAssetKind,
    relative_input: &Path,
) -> Result<Vec<PathBuf>> {
    let has_extension = relative_input.extension().is_some();
    match kind.extension_policy {
        AssetExtensionPolicy::Required => {
            if has_extension || kind.extensions.is_empty() {
                Ok(vec![relative_input.to_path_buf()])
            } else {
                Ok(kind
                    .extensions
                    .iter()
                    .map(|extension| with_asset_extension(relative_input, extension))
                    .collect())
            }
        }
        AssetExtensionPolicy::Forbidden => {
            if has_extension {
                return Err(ConfigForgeError::Validation(format!(
                    "asset path `{}` must not include an extension",
                    relative_input.display()
                )));
            }
            if kind.extensions.is_empty() {
                Ok(vec![relative_input.to_path_buf()])
            } else {
                Ok(kind
                    .extensions
                    .iter()
                    .map(|extension| with_asset_extension(relative_input, extension))
                    .collect())
            }
        }
        AssetExtensionPolicy::Optional => {
            if has_extension || kind.extensions.is_empty() {
                Ok(vec![relative_input.to_path_buf()])
            } else {
                let mut candidates = vec![relative_input.to_path_buf()];
                candidates.extend(
                    kind.extensions
                        .iter()
                        .map(|extension| with_asset_extension(relative_input, extension)),
                );
                Ok(candidates)
            }
        }
    }
}

fn with_asset_extension(path: &Path, extension: &str) -> PathBuf {
    let mut extension = extension.trim_start_matches('.').to_owned();
    if extension.is_empty() {
        return path.to_path_buf();
    }
    let mut candidate = path.to_path_buf();
    candidate.set_extension(std::mem::take(&mut extension));
    candidate
}

fn select_unique_candidate(
    kind_name: &str,
    input: &str,
    candidates: Vec<AssetCandidate>,
) -> Result<AssetCandidate> {
    let existing = candidates
        .iter()
        .filter(|candidate| candidate.resolved_filesystem_path.exists())
        .cloned()
        .collect::<Vec<_>>();
    let candidates = if existing.is_empty() {
        candidates
    } else {
        existing
    };
    match candidates.as_slice() {
        [] => Err(ConfigForgeError::Validation(format!(
            "asset kind `{kind_name}` produced no candidates for `{input}`"
        ))),
        [candidate] => Ok(candidate.clone()),
        _ => Err(ConfigForgeError::Validation(format!(
            "asset kind `{kind_name}` resolved `{input}` ambiguously to {} candidates",
            candidates.len()
        ))),
    }
}

fn sidecar_path_from_template(template: &str, resolved: &ResolvedAssetRef) -> PathBuf {
    if template == "{path}" {
        return resolved.resolved_filesystem_path.clone();
    }
    if let Some(suffix) = template.strip_prefix("{path}") {
        return PathBuf::from(format!(
            "{}{}",
            resolved.resolved_filesystem_path.display(),
            suffix
        ));
    }
    PathBuf::from(template.replace(
        "{path}",
        &resolved.resolved_filesystem_path.display().to_string(),
    ))
}

fn asset_rule_name(resolved: &ResolvedAssetRef) -> String {
    resolved
        .object_name
        .clone()
        .unwrap_or_else(|| asset_stem(resolved))
}

fn asset_stem(resolved: &ResolvedAssetRef) -> String {
    resolved
        .search_path_relative_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned()
}

fn validate_simple_patterns(kind: &EffectiveAssetKind, resolved: &ResolvedAssetRef) -> Result<()> {
    validate_allow_patterns(kind, resolved)?;
    for pattern in &kind.deny_patterns {
        let regex = regex::Regex::new(pattern).map_err(|source| {
            ConfigForgeError::Validation(format!(
                "asset kind `{}` deny pattern `{pattern}` is invalid: {source}",
                kind.name
            ))
        })?;
        if regex.is_match(&resolved.logical_ref) {
            return Err(ConfigForgeError::Validation(format!(
                "asset kind `{}` logical ref `{}` is denied by pattern `{pattern}`",
                kind.name, resolved.logical_ref
            )));
        }
    }
    Ok(())
}

fn validate_allow_patterns(kind: &EffectiveAssetKind, resolved: &ResolvedAssetRef) -> Result<()> {
    if kind.allow_patterns.is_empty() {
        return Ok(());
    }
    for pattern in &kind.allow_patterns {
        let regex = regex::Regex::new(pattern).map_err(|source| {
            ConfigForgeError::Validation(format!(
                "asset kind `{}` allow pattern `{pattern}` is invalid: {source}",
                kind.name
            ))
        })?;
        if regex.is_match(&resolved.logical_ref) {
            return Ok(());
        }
    }
    Err(ConfigForgeError::Validation(format!(
        "asset kind `{}` logical ref `{}` does not match any allow pattern",
        kind.name, resolved.logical_ref
    )))
}

fn validate_regex_rules(
    kind: &EffectiveAssetKind,
    resolved: &ResolvedAssetRef,
    target_tags: &TargetTags,
) -> Result<()> {
    for rule in &kind.regex_rules {
        if !rule.when_tags.is_empty() && !rule.when_tags.iter().any(|tag| target_tags.contains(tag))
        {
            continue;
        }
        let pattern = if rule.case_sensitive == Some(false) {
            format!("(?i:{})", rule.pattern)
        } else {
            rule.pattern.clone()
        };
        let regex = regex::Regex::new(&pattern).map_err(|source| {
            ConfigForgeError::Validation(format!(
                "asset kind `{}` regex rule `{}` pattern is invalid: {source}",
                kind.name, rule.name
            ))
        })?;
        let value = regex_target_value(
            rule.target.unwrap_or(AssetRegexTarget::LogicalRef),
            resolved,
        );
        let matched = regex.is_match(&value);
        let passes = if rule.invert { !matched } else { matched };
        if !passes && rule.severity == ValidationSeverity::Error {
            return Err(ConfigForgeError::Validation(
                rule.message.clone().unwrap_or_else(|| {
                    format!(
                        "asset kind `{}` regex rule `{}` failed for `{value}`",
                        kind.name, rule.name
                    )
                }),
            ));
        }
    }
    Ok(())
}

fn regex_target_value(target: AssetRegexTarget, resolved: &ResolvedAssetRef) -> String {
    match target {
        AssetRegexTarget::RawInput => resolved.raw_input.clone(),
        AssetRegexTarget::LogicalRef => resolved.logical_ref.clone(),
        AssetRegexTarget::SearchPathRelative => {
            path_to_asset_string(&resolved.search_path_relative_path)
        }
        AssetRegexTarget::FileName => resolved
            .search_path_relative_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned(),
        AssetRegexTarget::Stem => asset_stem(resolved),
        AssetRegexTarget::ObjectName => resolved.object_name.clone().unwrap_or_default(),
        AssetRegexTarget::ExportRef => resolved.canonical_export_ref.clone(),
    }
}

fn validate_case_sensitive_path(path: &Path) -> Result<()> {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let entries = std::fs::read_dir(parent).map_err(|source| ConfigForgeError::ReadFile {
        path: parent.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ConfigForgeError::ReadFile {
            path: parent.to_path_buf(),
            source,
        })?;
        if entry.file_name().to_str() == Some(file_name) {
            return Ok(());
        }
    }
    Err(ConfigForgeError::Validation(format!(
        "asset path `{}` does not match filesystem case",
        path.display()
    )))
}

fn split_mounted_object<'a>(
    input: &'a str,
    kind: &EffectiveAssetKind,
) -> Option<(&'a str, &'a str)> {
    let (package, object) = input.rsplit_once(kind.object_separator.as_str())?;
    if package.is_empty() || object.is_empty() {
        return None;
    }
    Some((package, object))
}

fn strip_mount_prefix<'a>(input: &'a str, mount: &str) -> Option<&'a str> {
    if input == mount {
        return Some("");
    }
    input
        .strip_prefix(mount)
        .and_then(|relative| relative.strip_prefix('/').or(Some(relative)))
}

fn mounted_ref(search_path: &AssetSearchPath, relative_path: &Path) -> String {
    let relative = path_to_asset_string(relative_path);
    match search_path.mount.as_deref() {
        Some(mount) if !mount.is_empty() => format!("{}/{}", mount.trim_end_matches('/'), relative),
        _ => relative,
    }
}

fn mounted_object_ref(
    search_path: &AssetSearchPath,
    package_relative_path: &Path,
    object_name: &str,
    object_separator: &str,
) -> String {
    format!(
        "{}{}{}",
        mounted_ref(search_path, package_relative_path),
        object_separator,
        object_name
    )
}

fn render_object_name(template: Option<&str>, stem: &str) -> String {
    template
        .map(|template| template.replace("{stem}", stem))
        .unwrap_or_else(|| stem.to_owned())
}

fn without_extension(path: &Path) -> PathBuf {
    let mut path = path.to_path_buf();
    path.set_extension("");
    path
}

fn path_to_asset_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{meta::MetaModel, project::AssetsConfig};

    use super::*;

    fn linked_meta(source: &str) -> MetaModel {
        toml::from_str::<MetaModel>(source)
            .expect("meta should parse")
            .link()
            .expect("meta should link")
            .model
            .clone()
    }

    fn resolver(meta: &MetaModel, assets: AssetsConfig) -> AssetResolver {
        let linked = meta.link().expect("meta should link");
        AssetResolver::new(&linked, &assets).expect("resolver should build")
    }

    fn search_path(name: &str, root: PathBuf, mount: Option<&str>) -> AssetSearchPathConfig {
        AssetSearchPathConfig {
            name: name.to_owned(),
            root,
            kind: "filesystem".to_owned(),
            mount: mount.map(str::to_owned),
        }
    }

    #[test]
    fn resolves_relative_path_with_required_extension() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let asset_path = temp_dir.path().join("UI/Icon_Sword.png");
        fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
        fs::write(&asset_path, []).expect("write asset");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                base_dirs = ["UI"]
                extensions = [".png"]
                path_style = "relative"
                extension_policy = "required"
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
                ..AssetsConfig::default()
            },
        );

        let resolved = resolver
            .resolve("ui_icon", "Icon_Sword.png")
            .expect("asset should resolve");

        assert_eq!(resolved.canonical_export_ref, "UI/Icon_Sword.png");
        assert_eq!(resolved.search_path_id, "game");
        assert_eq!(
            resolved.search_path_relative_path,
            PathBuf::from("UI/Icon_Sword.png")
        );
        assert_eq!(resolved.resolved_filesystem_path, asset_path);
        assert!(!resolved.completion_applied);
    }

    #[test]
    fn resolves_unreal_mounted_object_path() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let asset_path = temp_dir.path().join("UI/WBP_Item.uasset");
        fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
        fs::write(&asset_path, []).expect("write asset");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "unreal_widget_class"
                search_paths = ["game"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                object_name = "{stem}_C"
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path(
                    "game",
                    temp_dir.path().to_path_buf(),
                    Some("/Game"),
                )],
                ..AssetsConfig::default()
            },
        );

        let resolved = resolver
            .resolve("unreal_widget_class", "/Game/UI/WBP_Item.WBP_Item_C")
            .expect("asset should resolve");

        assert_eq!(
            resolved.canonical_export_ref,
            "/Game/UI/WBP_Item.WBP_Item_C"
        );
        assert_eq!(
            resolved.search_path_relative_path,
            PathBuf::from("UI/WBP_Item.uasset")
        );
        assert_eq!(resolved.object_name.as_deref(), Some("WBP_Item_C"));
        assert_eq!(resolved.resolved_filesystem_path, asset_path);
    }

    #[test]
    fn strips_unreal_wrappers_before_resolving() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let asset_path = temp_dir.path().join("UI/WBP_Item.uasset");
        fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
        fs::write(&asset_path, []).expect("write asset");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "unreal_widget_class"
                search_paths = ["game"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                strip_wrappers = ["BlueprintGeneratedClass'{value}'"]
                object_name = "{stem}_C"
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path(
                    "game",
                    temp_dir.path().to_path_buf(),
                    Some("/Game"),
                )],
                ..AssetsConfig::default()
            },
        );

        let resolved = resolver
            .resolve(
                "unreal_widget_class",
                "BlueprintGeneratedClass'/Game/UI/WBP_Item.WBP_Item_C'",
            )
            .expect("asset should resolve");

        assert_eq!(
            resolved.canonical_export_ref,
            "/Game/UI/WBP_Item.WBP_Item_C"
        );
        assert!(resolved.completion_applied);
    }

    #[test]
    fn completes_unique_unreal_asset_candidate() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let asset_path = temp_dir.path().join("UI/WBP_Item.uasset");
        fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
        fs::write(&asset_path, []).expect("write asset");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "unreal_asset"
                search_paths = ["game"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                object_name = "{stem}_C"
                allow_auto_complete = true

                [[asset_kinds]]
                name = "ui_widget"
                extends = "unreal_asset"
                base_dirs = ["UI"]
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path(
                    "game",
                    temp_dir.path().to_path_buf(),
                    Some("/Game"),
                )],
                ..AssetsConfig::default()
            },
        );

        let resolved = resolver
            .resolve("ui_widget", "WBP_Item")
            .expect("asset should resolve");

        assert_eq!(
            resolved.canonical_export_ref,
            "/Game/UI/WBP_Item.WBP_Item_C"
        );
        assert_eq!(
            resolved.search_path_relative_path,
            PathBuf::from("UI/WBP_Item.uasset")
        );
        assert!(resolved.completion_applied);
        assert_eq!(resolved.object_name.as_deref(), Some("WBP_Item_C"));
    }

    #[test]
    fn reports_ambiguous_completion_candidates() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        for dir in ["UI", "DevUI"] {
            let asset_path = temp_dir.path().join(dir).join("WBP_Item.uasset");
            fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
            fs::write(&asset_path, []).expect("write asset");
        }
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "ui_widget"
                search_paths = ["game"]
                base_dirs = ["UI", "DevUI"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                object_name = "{stem}_C"
                allow_auto_complete = true
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path(
                    "game",
                    temp_dir.path().to_path_buf(),
                    Some("/Game"),
                )],
                ..AssetsConfig::default()
            },
        );

        let error = resolver.resolve("ui_widget", "WBP_Item").unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("ambiguously"))
        );
    }

    #[test]
    fn rejects_absolute_and_parent_escape_paths() {
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path("game", PathBuf::from("/tmp/assets"), None)],
                ..AssetsConfig::default()
            },
        );

        let absolute_error = resolver.resolve("ui_icon", "/tmp/Icon.png").unwrap_err();
        assert!(
            matches!(absolute_error, ConfigForgeError::Validation(message) if message.contains("must be relative") || message.contains("escape"))
        );

        let parent_error = resolver.resolve("ui_icon", "../Icon.png").unwrap_err();
        assert!(
            matches!(parent_error, ConfigForgeError::Validation(message) if message.contains("must not escape"))
        );
    }

    #[test]
    fn require_existing_reports_missing_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                require_existing = true
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
                ..AssetsConfig::default()
            },
        );

        let error = resolver.resolve("ui_icon", "Missing.png").unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("missing file"))
        );
    }

    #[test]
    fn require_sidecars_reports_missing_pair_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let asset_path = temp_dir.path().join("Icon_Sword.png");
        fs::write(&asset_path, []).expect("write asset");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                require_sidecars = ["{path}.meta"]
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
                ..AssetsConfig::default()
            },
        );

        let error = resolver.resolve("ui_icon", "Icon_Sword.png").unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("missing sidecar"))
        );
    }

    #[test]
    fn validates_name_prefixes_and_suffixes() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        fs::write(temp_dir.path().join("BadName.png"), []).expect("write asset");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                name_prefixes = ["Icon_"]
                name_suffixes = ["_UI"]
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
                ..AssetsConfig::default()
            },
        );

        let error = resolver.resolve("ui_icon", "BadName.png").unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("required prefixes"))
        );
    }

    #[test]
    fn validates_object_name_template() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let asset_path = temp_dir.path().join("UI/WBP_Item.uasset");
        fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
        fs::write(&asset_path, []).expect("write asset");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "unreal_widget_class"
                search_paths = ["game"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                object_name = "{stem}_C"
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path(
                    "game",
                    temp_dir.path().to_path_buf(),
                    Some("/Game"),
                )],
                ..AssetsConfig::default()
            },
        );

        let error = resolver
            .resolve("unreal_widget_class", "/Game/UI/WBP_Item.WBP_Item")
            .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("object name") && message.contains("WBP_Item_C"))
        );
    }

    #[test]
    fn validates_target_gated_deny_prefixes() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let asset_path = temp_dir.path().join("Dev/Icon_Debug.png");
        fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
        fs::write(&asset_path, []).expect("write asset");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                deny_path_prefixes = ["Dev/"]
                deny_when_tags = ["shipping"]
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
                ..AssetsConfig::default()
            },
        );

        resolver
            .resolve_for_target("ui_icon", "Dev/Icon_Debug.png", &TargetTags::new(["dev"]))
            .expect("dev target should allow dev asset");
        let error = resolver
            .resolve_for_target(
                "ui_icon",
                "Dev/Icon_Debug.png",
                &TargetTags::new(["shipping"]),
            )
            .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("denied by prefix"))
        );
    }

    #[test]
    fn validates_allow_and_deny_patterns() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        for file_name in ["Icon_Sword.png", "Tmp_Icon.png"] {
            fs::write(temp_dir.path().join(file_name), []).expect("write asset");
        }
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                allow_patterns = ["^Icon_"]
                deny_patterns = ["Tmp"]
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
                ..AssetsConfig::default()
            },
        );

        resolver
            .resolve("ui_icon", "Icon_Sword.png")
            .expect("allowed pattern should pass");
        let error = resolver.resolve("ui_icon", "Tmp_Icon.png").unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("does not match any allow pattern") || message.contains("denied by pattern"))
        );
    }

    #[test]
    fn validates_regex_rules_for_specific_targets() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        fs::write(temp_dir.path().join("Icon_Sword.png"), []).expect("write asset");
        let meta = linked_meta(
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]

                [[asset_kinds.regex_rules]]
                name = "shipping_icon_stem"
                target = "stem"
                pattern = "^Ship_"
                message = "shipping icon stem must start with Ship_"
                when_tags = ["shipping"]
            "#,
        );
        let resolver = resolver(
            &meta,
            AssetsConfig {
                search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
                ..AssetsConfig::default()
            },
        );

        resolver
            .resolve_for_target("ui_icon", "Icon_Sword.png", &TargetTags::new(["client"]))
            .expect("non-shipping target should skip regex rule");
        let error = resolver
            .resolve_for_target("ui_icon", "Icon_Sword.png", &TargetTags::new(["shipping"]))
            .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("shipping icon stem"))
        );
    }
}
