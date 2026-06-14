// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
};

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::{
        CF_ARTIFACT_CASE_INSENSITIVE_COLLISION, CF_ARTIFACT_HARDLINK_ALIAS_COLLISION,
        CF_ARTIFACT_PATH_COLLISION, CF_ARTIFACT_PATH_ESCAPE, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION,
        CF_ARTIFACT_WRITE_STATE,
    },
    path_util::{filesystem_aware_path_key, lexical_normalize_path, path_is_within_root},
};

use super::Artifact;

pub(super) fn plan_artifacts(artifacts: Vec<Artifact>) -> Result<Vec<Artifact>> {
    let mut by_path = BTreeMap::<PathBuf, Artifact>::new();
    for mut artifact in artifacts {
        artifact.path = lexical_normalize_path(&artifact.path);
        if by_path.contains_key(&artifact.path) {
            let message = format!("multiple artifacts target `{}`", artifact.path.display());
            return Err(artifact_path_error(
                CF_ARTIFACT_PATH_COLLISION,
                message,
                &artifact.path,
                "output root内唯一 normalized artifact path",
            ));
        }
        by_path.insert(artifact.path.clone(), artifact);
    }
    Ok(by_path.into_values().collect())
}

pub(super) fn normalize_artifact_path(output_root: Option<&Path>, path: &Path) -> Result<PathBuf> {
    let normalized_path = if let Some(output_root) = output_root {
        let normalized_root = lexical_normalize_path(output_root);
        let candidate = if path.is_absolute() {
            lexical_normalize_path(path)
        } else {
            lexical_normalize_path(&normalized_root.join(path))
        };
        if !path_is_within_root(&candidate, &normalized_root)
            && !filesystem_path_is_within_root(&candidate, &normalized_root)
        {
            return Err(artifact_path_error(
                CF_ARTIFACT_PATH_ESCAPE,
                format!(
                    "artifact path `{}` escapes output root `{}`",
                    path.display(),
                    normalized_root.display()
                ),
                path,
                format!("path inside output root `{}`", normalized_root.display()),
            ));
        }
        candidate
    } else {
        lexical_normalize_path(path)
    };
    Ok(normalized_path)
}

pub(super) fn filesystem_path_is_within_root(path: &Path, root: &Path) -> bool {
    let root_key = filesystem_aware_path_key(root).unwrap_or_else(|| root.to_path_buf());
    let path_key = filesystem_aware_path_key(path).unwrap_or_else(|| path.to_path_buf());
    let root_key = case_fold_conflict_key(root_key);
    let path_key = case_fold_conflict_key(path_key);
    path_is_within_root(&path_key, &root_key)
}

pub(super) fn artifact_conflict_key(
    output_root: Option<&Path>,
    normalized_path: &Path,
) -> Result<PathBuf> {
    let alias_key =
        filesystem_aware_path_key(normalized_path).unwrap_or_else(|| normalized_path.to_path_buf());
    if let Some(output_root) = output_root {
        let root_key = filesystem_aware_path_key(&lexical_normalize_path(output_root))
            .unwrap_or_else(|| lexical_normalize_path(output_root));
        let root_key = case_fold_conflict_key(root_key);
        if !path_is_within_root(&case_fold_conflict_key(alias_key.clone()), &root_key) {
            return Err(artifact_path_error(
                CF_ARTIFACT_PATH_ESCAPE,
                format!(
                    "artifact path `{}` escapes output root `{}` through filesystem alias",
                    normalized_path.display(),
                    output_root.display()
                ),
                normalized_path,
                format!(
                    "filesystem-resolved path inside output root `{}`",
                    output_root.display()
                ),
            ));
        }
    }
    Ok(existing_file_identity_key(normalized_path)
        .unwrap_or_else(|| case_fold_conflict_key(alias_key)))
}

pub(super) fn artifact_collision_message(path: &Path, existing: &Path) -> String {
    format!(
        "multiple artifacts target `{}` (conflicts with `{}`)",
        path.display(),
        existing.display()
    )
}

pub(super) fn artifact_collision_code(
    path: &Path,
    existing: &Path,
    conflict_key: &Path,
) -> &'static str {
    if existing_file_identity_key(path).is_some_and(|key| key == conflict_key) {
        return CF_ARTIFACT_HARDLINK_ALIAS_COLLISION;
    }
    if path != existing
        && case_fold_conflict_key(path.to_path_buf())
            == case_fold_conflict_key(existing.to_path_buf())
    {
        return CF_ARTIFACT_CASE_INSENSITIVE_COLLISION;
    }
    if path != existing {
        return CF_ARTIFACT_SYMLINK_ALIAS_COLLISION;
    }
    CF_ARTIFACT_PATH_COLLISION
}

pub(super) fn artifact_collision_error(
    code: &'static str,
    message: &str,
    path: &Path,
    existing: &Path,
) -> ConfigForgeError {
    artifact_path_error(
        code,
        message.to_owned(),
        path,
        format!(
            "output root 内唯一 normalized artifact path; conflicts with `{}`",
            existing.display()
        ),
    )
}

pub(super) fn artifact_path_error(
    code: &'static str,
    message: String,
    path: &Path,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            code,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source("artifact.writer")
        .with_current_value(path.display().to_string())
        .with_expected(expected),
    )
}

pub(super) fn artifact_state_error(
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_ARTIFACT_WRITE_STATE,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source("artifact.writer")
        .with_current_value(current)
        .with_expected(expected)
        .with_hint("Finish each artifact stream exactly once and stop using a sink after failure or finish."),
    )
}

#[cfg(unix)]
pub(super) fn existing_file_identity_key(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path).ok()?;
    metadata.is_file().then(|| {
        PathBuf::from(format!(
            "configforge-inode-key/{}/{}",
            metadata.dev(),
            metadata.ino()
        ))
    })
}

#[cfg(not(unix))]
pub(super) fn existing_file_identity_key(_path: &Path) -> Option<PathBuf> {
    None
}

#[cfg(any(target_os = "macos", windows))]
pub(super) fn case_fold_conflict_key(path: PathBuf) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_ascii_lowercase())
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(super) fn case_fold_conflict_key(path: PathBuf) -> PathBuf {
    path
}

pub(super) fn open_file_for_write(path: &Path) -> Result<File> {
    if let Some(parent) = parent_dir_to_create(path) {
        fs::create_dir_all(parent).map_err(|source| ConfigForgeError::WriteFile {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    File::create(path).map_err(|source| ConfigForgeError::WriteFile {
        path: path.to_path_buf(),
        source,
    })
}

pub(super) fn commit_temp_files(temp_files: &BTreeMap<PathBuf, PathBuf>) -> Result<()> {
    for (final_path, temp_path) in temp_files {
        replace_with_temp_file(temp_path, final_path)?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn replace_with_temp_file(temp_path: &Path, final_path: &Path) -> Result<()> {
    fs::rename(temp_path, final_path).map_err(|source| ConfigForgeError::WriteFile {
        path: final_path.to_path_buf(),
        source,
    })
}

#[cfg(windows)]
pub(super) fn replace_with_temp_file(temp_path: &Path, final_path: &Path) -> Result<()> {
    match fs::remove_file(final_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(ConfigForgeError::WriteFile {
                path: final_path.to_path_buf(),
                source,
            });
        }
    }

    fs::rename(temp_path, final_path).map_err(|source| ConfigForgeError::WriteFile {
        path: final_path.to_path_buf(),
        source,
    })
}

pub(super) fn cleanup_temp_files<'a>(temp_files: impl IntoIterator<Item = &'a PathBuf>) {
    for temp_path in temp_files {
        let _ = fs::remove_file(temp_path);
    }
}

pub(super) fn parent_dir_to_create(path: &Path) -> Option<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

pub(super) fn temp_path_for(path: &Path, index: usize) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "artifact".into());
    let temp_name = format!(
        ".{file_name}.configforge-tmp-{}-{index}",
        std::process::id()
    );
    path.with_file_name(temp_name)
}
