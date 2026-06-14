// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::CF_DATA_INVALID_ASSET_REF,
    meta::{
        AssetExtensionPolicy, AssetInputKind, AssetKindMeta, AssetPathStyle, AssetRegexTarget,
        LinkedMetaModel, TargetTags, ValidationSeverity,
    },
    project::{AssetSearchPathConfig, AssetsConfig},
};

use super::model::{AssetCandidate, AssetSearchPath, EffectiveAssetKind, ResolvedAssetRef};

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
            asset_error(
                format!("unknown asset kind `{asset_kind}`"),
                asset_kind,
                "declared asset kind",
                "Declare the asset kind in Meta or update the field asset_kind reference.",
            )
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
            return Err(asset_error(
                format!("asset kind `{}` uses unsupported input mode", kind.name),
                format!("{:?}", kind.input),
                "path or string input mode",
                "Use path/string asset input for resolver-backed asset references.",
            ));
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
                return Err(asset_kind_error(
                    kind,
                    format!(
                        "asset kind `{}` requires a mounted object ref, but `{stripped_input}` is not one",
                        kind.name
                    ),
                    stripped_input,
                    "mounted object ref",
                    "Use the configured mounted object reference format or enable auto-completion.",
                ));
            }
            let relative_input = asset_relative_path(stripped_input)?;
            let candidates = self.generate_candidates(kind, &relative_input)?;
            let candidate = select_unique_candidate(&kind.name, stripped_input, candidates)?;
            let package_relative = without_extension(&candidate.search_path_relative_path);
            let stem = package_relative
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    asset_kind_error(
                        kind,
                        format!(
                            "asset kind `{}` cannot derive object name for `{stripped_input}`",
                            kind.name
                        ),
                        stripped_input,
                        "path with file stem",
                        "Use an asset path that contains a file name stem.",
                    )
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

        Err(asset_kind_error(
            kind,
            format!(
                "asset kind `{}` input `{input}` does not match any configured mount",
                kind.name
            ),
            input,
            "input under one configured mount",
            "Check the asset mount prefix and search path configuration.",
        ))
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
            asset_error(
                format!("asset search path `{name}` not found"),
                name,
                "declared asset search path",
                "Declare the search path in project assets.search_paths or remove the reference.",
            )
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
            return Err(resolved_asset_error(
                kind,
                resolved,
                format!(
                    "asset kind `{}` resolved `{}` to missing file `{}`",
                    kind.name,
                    resolved.raw_input,
                    resolved.resolved_filesystem_path.display()
                ),
                resolved.raw_input.as_str(),
                "existing asset file",
                "Create the asset file or disable require_existing for this asset kind/search.",
            ));
        }

        for sidecar_template in &kind.require_sidecars {
            let sidecar_path = sidecar_path_from_template(sidecar_template, resolved);
            if !sidecar_path.is_file() {
                return Err(resolved_asset_error(
                    kind,
                    resolved,
                    format!(
                        "asset kind `{}` requires missing sidecar `{}` for `{}`",
                        kind.name,
                        sidecar_path.display(),
                        resolved.raw_input
                    ),
                    resolved.raw_input.as_str(),
                    "existing sidecar file",
                    "Create the required sidecar file or update require_sidecars.",
                ));
            }
        }

        let name = asset_rule_name(resolved);
        if !kind.name_prefixes.is_empty()
            && !kind
                .name_prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
        {
            return Err(resolved_asset_error(
                kind,
                resolved,
                format!(
                    "asset kind `{}` name `{name}` does not match required prefixes {:?}",
                    kind.name, kind.name_prefixes
                ),
                name.clone(),
                format!("prefix in {:?}", kind.name_prefixes),
                "Rename the asset to use an allowed prefix or update name_prefixes.",
            ));
        }
        if !kind.name_suffixes.is_empty()
            && !kind
                .name_suffixes
                .iter()
                .any(|suffix| name.ends_with(suffix))
        {
            return Err(resolved_asset_error(
                kind,
                resolved,
                format!(
                    "asset kind `{}` name `{name}` does not match required suffixes {:?}",
                    kind.name, kind.name_suffixes
                ),
                name.clone(),
                format!("suffix in {:?}", kind.name_suffixes),
                "Rename the asset to use an allowed suffix or update name_suffixes.",
            ));
        }

        if let Some(template) = kind.object_name.as_deref() {
            if let Some(object_name) = resolved.object_name.as_deref() {
                let stem = asset_stem(resolved);
                let expected = render_object_name(Some(template), &stem);
                if object_name != expected {
                    return Err(resolved_asset_error(
                        kind,
                        resolved,
                        format!(
                            "asset kind `{}` object name `{object_name}` must be `{expected}`",
                            kind.name
                        ),
                        object_name,
                        expected,
                        "Use the configured object_name template output.",
                    ));
                }
            }
        }

        if !kind.allow_path_prefixes.is_empty()
            && !kind
                .allow_path_prefixes
                .iter()
                .any(|prefix| resolved.logical_ref.starts_with(prefix))
        {
            return Err(resolved_asset_error(
                kind,
                resolved,
                format!(
                    "asset kind `{}` logical ref `{}` is outside allowed prefixes {:?}",
                    kind.name, resolved.logical_ref, kind.allow_path_prefixes
                ),
                resolved.logical_ref.as_str(),
                format!("prefix in {:?}", kind.allow_path_prefixes),
                "Move the asset under an allowed logical prefix or update allow_path_prefixes.",
            ));
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
                return Err(resolved_asset_error(
                    kind,
                    resolved,
                    format!(
                        "asset kind `{}` logical ref `{}` is denied by prefix `{prefix}`",
                        kind.name, resolved.logical_ref
                    ),
                    resolved.logical_ref.as_str(),
                    format!("not under denied prefix `{prefix}`"),
                    "Move the asset outside the denied prefix or adjust deny_path_prefixes.",
                ));
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
            return Err(asset_error(
                format!(
                    "asset kind extends cycle detected while resolving `{}`",
                    asset_kind.name
                ),
                asset_kind.name.as_str(),
                "acyclic asset kind extends chain",
                "Remove the cycle from asset kind extends declarations.",
            ));
        }
        chain.push(current);
        let Some(parent) = current.extends.as_deref() else {
            break;
        };
        current = meta.asset_kind(parent).ok_or_else(|| {
            asset_error(
                format!(
                    "asset kind `{}` extends unknown asset kind `{parent}`",
                    current.name
                ),
                parent,
                "declared parent asset kind",
                "Declare the parent asset kind or remove the extends reference.",
            )
        })?;
    }

    let Some(root) = chain.pop() else {
        return Err(asset_error(
            format!("asset kind `{}` could not be resolved", asset_kind.name),
            asset_kind.name.as_str(),
            "asset kind inheritance chain",
            "Declare the asset kind before using it.",
        ));
    };
    let mut effective = EffectiveAssetKind::from_raw(root);
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

fn strip_asset_wrappers(input: &str, wrappers: &[String]) -> String {
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
        return Err(asset_error(
            format!("asset path `{path}` must be relative"),
            path,
            "relative asset path",
            "Use an asset path relative to its configured search path.",
        ));
    }
    let trimmed = normalized.trim_matches('/');
    let relative = PathBuf::from(trimmed);
    ensure_relative_asset_path(&relative)?;
    Ok(relative)
}

fn ensure_relative_asset_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(asset_error(
            "asset path must be non-empty".to_owned(),
            "empty path",
            "non-empty relative asset path",
            "Provide a non-empty asset reference value.",
        ));
    }
    if path.is_absolute() {
        return Err(asset_error(
            format!("asset path `{}` must be relative", path.display()),
            path.display().to_string(),
            "relative asset path",
            "Use an asset path relative to its configured search path.",
        ));
    }
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir
        ) {
            return Err(asset_error(
                format!(
                    "asset path `{}` must not escape its search path",
                    path.display()
                ),
                path.display().to_string(),
                "relative path without parent/root components",
                "Remove parent directory or root components from the asset path.",
            ));
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
                return Err(asset_error(
                    format!(
                        "asset path `{}` must not include an extension",
                        relative_input.display()
                    ),
                    relative_input.display().to_string(),
                    "asset path without extension",
                    "Remove the extension from the asset reference for this asset kind.",
                ));
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
        [] => Err(asset_error(
            format!("asset kind `{kind_name}` produced no candidates for `{input}`"),
            input,
            "at least one asset resolution candidate",
            "Check asset kind search_paths, base_dirs, extensions, and input path.",
        )),
        [candidate] => Ok(candidate.clone()),
        _ => Err(asset_error(
            format!(
                "asset kind `{kind_name}` resolved `{input}` ambiguously to {} candidates",
                candidates.len()
            ),
            input,
            "exactly one asset resolution candidate",
            "Narrow asset kind search_paths/base_dirs/extensions or provide a more specific path.",
        )),
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
            asset_kind_error(
                kind,
                format!(
                    "asset kind `{}` deny pattern `{pattern}` is invalid: {source}",
                    kind.name
                ),
                pattern.as_str(),
                "valid regex pattern",
                "Fix the deny pattern regex in Meta.",
            )
        })?;
        if regex.is_match(&resolved.logical_ref) {
            return Err(resolved_asset_error(
                kind,
                resolved,
                format!(
                    "asset kind `{}` logical ref `{}` is denied by pattern `{pattern}`",
                    kind.name, resolved.logical_ref
                ),
                resolved.logical_ref.as_str(),
                format!("not matching deny pattern `{pattern}`"),
                "Move or rename the asset so it does not match the deny pattern.",
            ));
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
            asset_kind_error(
                kind,
                format!(
                    "asset kind `{}` allow pattern `{pattern}` is invalid: {source}",
                    kind.name
                ),
                pattern.as_str(),
                "valid regex pattern",
                "Fix the allow pattern regex in Meta.",
            )
        })?;
        if regex.is_match(&resolved.logical_ref) {
            return Ok(());
        }
    }
    Err(resolved_asset_error(
        kind,
        resolved,
        format!(
            "asset kind `{}` logical ref `{}` does not match any allow pattern",
            kind.name, resolved.logical_ref
        ),
        resolved.logical_ref.as_str(),
        format!("one allow pattern in {:?}", kind.allow_patterns),
        "Move or rename the asset so it matches an allow pattern.",
    ))
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
            asset_kind_error(
                kind,
                format!(
                    "asset kind `{}` regex rule `{}` pattern is invalid: {source}",
                    kind.name, rule.name
                ),
                pattern.as_str(),
                "valid regex pattern",
                "Fix the regex rule pattern in Meta.",
            )
        })?;
        let value = regex_target_value(
            rule.target.unwrap_or(AssetRegexTarget::LogicalRef),
            resolved,
        );
        let matched = regex.is_match(&value);
        let passes = if rule.invert { !matched } else { matched };
        if !passes && rule.severity == ValidationSeverity::Error {
            return Err(resolved_asset_error(
                kind,
                resolved,
                rule.message.clone().unwrap_or_else(|| {
                    format!(
                        "asset kind `{}` regex rule `{}` failed for `{value}`",
                        kind.name, rule.name
                    )
                }),
                value,
                format!("regex rule `{}` passes", rule.name),
                "Update the asset reference or relax the regex rule.",
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
    Err(asset_error(
        format!(
            "asset path `{}` does not match filesystem case",
            path.display()
        ),
        path.display().to_string(),
        "path casing matching filesystem entry",
        "Update the asset reference casing to match the filesystem.",
    ))
}

fn asset_kind_error(
    kind: &EffectiveAssetKind,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    asset_diagnostic_error(
        message,
        Some(kind.name.as_str()),
        None,
        current_value,
        expected,
        hint,
    )
}

fn resolved_asset_error(
    kind: &EffectiveAssetKind,
    resolved: &ResolvedAssetRef,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    asset_diagnostic_error(
        message,
        Some(kind.name.as_str()),
        Some(resolved.logical_ref.as_str()),
        current_value,
        expected,
        hint,
    )
}

fn asset_error(
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    asset_diagnostic_error(message, None, None, current_value, expected, hint)
}

fn asset_diagnostic_error(
    message: String,
    _asset_kind: Option<&str>,
    target: Option<&str>,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    let diagnostic = Diagnostic::new(
        CF_DATA_INVALID_ASSET_REF,
        DiagnosticSeverity::Error,
        message,
        DiagnosticLocation::default(),
    )
    .with_source("asset.resolver")
    .with_domain("data")
    .with_current_value(current_value)
    .with_expected(expected)
    .with_hint(hint);
    let diagnostic = if let Some(target) = target {
        diagnostic.with_target(target)
    } else {
        diagnostic
    };
    ConfigForgeError::from_diagnostic(diagnostic)
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
