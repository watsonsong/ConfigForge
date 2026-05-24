use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::{ConfigForgeError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub path: PathBuf,
    pub content: ArtifactContent,
}

impl Artifact {
    pub fn text(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: ArtifactContent::Text(content.into()),
        }
    }

    pub fn binary(path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            content: ArtifactContent::Binary(content.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactContent {
    Text(String),
    Binary(Vec<u8>),
}

impl ArtifactContent {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(content) => content.as_bytes(),
            Self::Binary(content) => content,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Text(content) => content.into_bytes(),
            Self::Binary(content) => content,
        }
    }

    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactWriteMode {
    DryRun,
    Direct,
    TempCommit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactWriteResult {
    pub mode: ArtifactWriteMode,
    pub entries: Vec<ArtifactWriteEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactWriteEntry {
    pub path: PathBuf,
    pub bytes: usize,
    pub written: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactWriter {
    mode: ArtifactWriteMode,
}

impl ArtifactWriter {
    pub fn new(mode: ArtifactWriteMode) -> Self {
        Self { mode }
    }

    pub fn mode(self) -> ArtifactWriteMode {
        self.mode
    }

    pub fn write(self, artifacts: Vec<Artifact>) -> Result<ArtifactWriteResult> {
        let planned = plan_artifacts(artifacts)?;
        let entries = planned
            .iter()
            .map(|artifact| ArtifactWriteEntry {
                path: artifact.path.clone(),
                bytes: artifact.content.len(),
                written: self.mode != ArtifactWriteMode::DryRun,
            })
            .collect::<Vec<_>>();

        match self.mode {
            ArtifactWriteMode::DryRun => {}
            ArtifactWriteMode::Direct => write_direct(&planned)?,
            ArtifactWriteMode::TempCommit => write_temp_commit(&planned)?,
        }

        Ok(ArtifactWriteResult {
            mode: self.mode,
            entries,
        })
    }
}

fn plan_artifacts(artifacts: Vec<Artifact>) -> Result<Vec<Artifact>> {
    let mut by_path = BTreeMap::<PathBuf, Artifact>::new();
    for mut artifact in artifacts {
        artifact.path = lexical_normalize_path(&artifact.path);
        if by_path.contains_key(&artifact.path) {
            return Err(ConfigForgeError::Validation(format!(
                "multiple artifacts target `{}`",
                artifact.path.display(),
            )));
        }
        by_path.insert(artifact.path.clone(), artifact);
    }
    Ok(by_path.into_values().collect())
}

fn write_direct(artifacts: &[Artifact]) -> Result<()> {
    for artifact in artifacts {
        write_bytes(&artifact.path, artifact.content.as_bytes())?;
    }
    Ok(())
}

fn write_temp_commit(artifacts: &[Artifact]) -> Result<()> {
    let mut temp_paths = Vec::new();
    let result = (|| -> Result<()> {
        for (index, artifact) in artifacts.iter().enumerate() {
            let temp_path = temp_path_for(&artifact.path, index);
            write_bytes(&temp_path, artifact.content.as_bytes())?;
            temp_paths.push(temp_path);
        }

        for (artifact, temp_path) in artifacts.iter().zip(temp_paths.iter()) {
            fs::rename(temp_path, &artifact.path).map_err(|source| ConfigForgeError::WriteFile {
                path: artifact.path.clone(),
                source,
            })?;
        }
        Ok(())
    })();

    if result.is_err() {
        for temp_path in temp_paths {
            let _ = fs::remove_file(temp_path);
        }
    }

    result
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigForgeError::WriteFile {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, bytes).map_err(|source| ConfigForgeError::WriteFile {
        path: path.to_path_buf(),
        source,
    })
}

fn temp_path_for(path: &Path, index: usize) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "artifact".into());
    let temp_name = format!(".{file_name}.configforge-tmp-{}-{index}", std::process::id());
    path.with_file_name(temp_name)
}

fn lexical_normalize_path(path: &Path) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_returns_sorted_entries_without_writing() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let nested = temp_dir.path().join("out/./nested/../b.txt");
        let first = temp_dir.path().join("out/a.txt");

        let result = ArtifactWriter::new(ArtifactWriteMode::DryRun)
            .write(vec![
                Artifact::text(&nested, "b"),
                Artifact::text(&first, "a"),
            ])
            .expect("dry-run should succeed");

        assert_eq!(result.mode, ArtifactWriteMode::DryRun);
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].path, first);
        assert_eq!(result.entries[0].bytes, 1);
        assert!(!result.entries[0].written);
        assert!(!temp_dir.path().join("out").exists());
    }

    #[test]
    fn direct_writes_text_and_binary_content() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let text_path = temp_dir.path().join("text/generated.rs");
        let binary_path = temp_dir.path().join("data/items.msgpack");

        let result = ArtifactWriter::new(ArtifactWriteMode::Direct)
            .write(vec![
                Artifact::text(&text_path, "pub struct Item;"),
                Artifact::binary(&binary_path, vec![1, 2, 3]),
            ])
            .expect("direct write should succeed");

        assert_eq!(result.mode, ArtifactWriteMode::Direct);
        assert!(result.entries.iter().all(|entry| entry.written));
        assert_eq!(
            fs::read_to_string(text_path).expect("text artifact"),
            "pub struct Item;"
        );
        assert_eq!(
            fs::read(binary_path).expect("binary artifact"),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn temp_commit_writes_artifacts() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/generated.rs");

        ArtifactWriter::new(ArtifactWriteMode::TempCommit)
            .write(vec![Artifact::text(&path, "pub struct Generated;")])
            .expect("temp commit should succeed");

        assert_eq!(
            fs::read_to_string(path).expect("written artifact"),
            "pub struct Generated;"
        );
    }

    #[test]
    fn duplicate_normalized_paths_are_rejected_before_writing() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let first = temp_dir.path().join("out/data/items.json");
        let second = temp_dir.path().join("out/data/./nested/../items.json");

        let error = ArtifactWriter::new(ArtifactWriteMode::Direct)
            .write(vec![
                Artifact::binary(first, Vec::new()),
                Artifact::text(second, "conflict"),
            ])
            .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("multiple artifacts target"))
        );
        assert!(!temp_dir.path().join("out").exists());
    }
}
