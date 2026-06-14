// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Component, Path, PathBuf};

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

pub(crate) fn path_is_within_root(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

pub(crate) fn filesystem_path_is_within_root(path: &Path, root: &Path) -> bool {
    let root_key = filesystem_aware_path_key(root).unwrap_or_else(|| root.to_path_buf());
    let path_key = filesystem_aware_path_key(path).unwrap_or_else(|| path.to_path_buf());
    path_is_within_root(&path_key, &root_key)
}

pub(crate) fn filesystem_aware_path_key(path: &Path) -> Option<PathBuf> {
    let mut suffix = Vec::new();
    let mut ancestor = path;
    while !ancestor.exists() {
        let file_name = ancestor.file_name()?;
        suffix.push(file_name.to_owned());
        ancestor = ancestor.parent()?;
    }
    let mut key = ancestor.canonicalize().ok()?;
    for component in suffix.iter().rev() {
        key.push(component);
    }
    Some(lexical_normalize_path(&key))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

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
}
