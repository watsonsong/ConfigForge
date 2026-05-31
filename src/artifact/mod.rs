// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
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
    limits: ArtifactWriteLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactWriteLimits {
    pub max_in_flight_bytes: usize,
    pub max_in_flight_artifacts: usize,
}

impl ArtifactWriter {
    pub fn new(mode: ArtifactWriteMode) -> Self {
        Self {
            mode,
            limits: ArtifactWriteLimits::default(),
        }
    }

    pub fn mode(self) -> ArtifactWriteMode {
        self.mode
    }

    pub fn limits(self) -> ArtifactWriteLimits {
        self.limits
    }

    pub fn with_limits(mut self, limits: ArtifactWriteLimits) -> Self {
        self.limits = limits.normalized();
        self
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
            ArtifactWriteMode::Direct => write_direct_bounded(planned, self.limits)?,
            ArtifactWriteMode::TempCommit => write_temp_commit_bounded(planned, self.limits)?,
        }

        Ok(ArtifactWriteResult {
            mode: self.mode,
            entries,
        })
    }
}

impl ArtifactWriteLimits {
    pub const DEFAULT_MAX_IN_FLIGHT_BYTES: usize = 256 * 1024 * 1024;
    pub const DEFAULT_MAX_IN_FLIGHT_ARTIFACTS: usize = 16;

    pub fn new(max_in_flight_bytes: usize, max_in_flight_artifacts: usize) -> Result<Self> {
        let limits = Self {
            max_in_flight_bytes,
            max_in_flight_artifacts,
        };
        if limits.max_in_flight_bytes == 0 {
            return Err(ConfigForgeError::Validation(
                "artifact memory limit must be greater than 0".to_owned(),
            ));
        }
        if limits.max_in_flight_artifacts == 0 {
            return Err(ConfigForgeError::Validation(
                "artifact in-flight limit must be greater than 0".to_owned(),
            ));
        }
        Ok(limits)
    }

    fn normalized(self) -> Self {
        Self {
            max_in_flight_bytes: self.max_in_flight_bytes.max(1),
            max_in_flight_artifacts: self.max_in_flight_artifacts.max(1),
        }
    }
}

impl Default for ArtifactWriteLimits {
    fn default() -> Self {
        Self {
            max_in_flight_bytes: Self::DEFAULT_MAX_IN_FLIGHT_BYTES,
            max_in_flight_artifacts: Self::DEFAULT_MAX_IN_FLIGHT_ARTIFACTS,
        }
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

fn write_direct_bounded(artifacts: Vec<Artifact>, limits: ArtifactWriteLimits) -> Result<()> {
    write_artifacts_bounded(artifacts, limits, |artifact, _index| {
        write_bytes(&artifact.path, artifact.content.as_bytes())?;
        Ok(())
    })
    .map(|_| ())
}

fn write_temp_commit_bounded(artifacts: Vec<Artifact>, limits: ArtifactWriteLimits) -> Result<()> {
    let temp_writes = write_artifacts_bounded(artifacts, limits, |artifact, index| {
        let temp_path = temp_path_for(&artifact.path, index);
        write_bytes(&temp_path, artifact.content.as_bytes())?;
        Ok(TempCommitArtifact {
            final_path: artifact.path,
            temp_path,
        })
    })?;
    let result = (|| -> Result<()> {
        for temp_write in &temp_writes {
            fs::rename(&temp_write.temp_path, &temp_write.final_path).map_err(|source| {
                ConfigForgeError::WriteFile {
                    path: temp_write.final_path.clone(),
                    source,
                }
            })?;
        }
        Ok(())
    })();

    if result.is_err() {
        for temp_write in temp_writes {
            let _ = fs::remove_file(temp_write.temp_path);
        }
    }

    result
}

#[derive(Debug)]
struct TempCommitArtifact {
    final_path: PathBuf,
    temp_path: PathBuf,
}

fn write_artifacts_bounded<WriteFn, WriteResult>(
    artifacts: Vec<Artifact>,
    limits: ArtifactWriteLimits,
    mut write_artifact: WriteFn,
) -> Result<Vec<WriteResult>>
where
    WriteFn: FnMut(Artifact, usize) -> Result<WriteResult> + Send + 'static,
    WriteResult: Send + 'static,
{
    let limits = limits.normalized();
    let byte_semaphore = Arc::new(ByteSemaphore::new(limits.max_in_flight_bytes));
    let (sender, receiver) = mpsc::sync_channel::<QueuedArtifact>(limits.max_in_flight_artifacts);
    let worker_semaphore = byte_semaphore.clone();
    let worker = thread::spawn(move || {
        let mut first_error = None;
        let mut write_results = BTreeMap::<usize, WriteResult>::new();
        for queued in receiver {
            let QueuedArtifact {
                index,
                bytes,
                artifact,
            } = queued;
            if first_error.is_none() {
                match write_artifact(artifact, index) {
                    Ok(write_result) => {
                        write_results.insert(index, write_result);
                    }
                    Err(error) => {
                        first_error = Some(error);
                    }
                }
            }
            worker_semaphore.release(bytes);
        }
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(write_results.into_values().collect::<Vec<_>>())
        }
    });

    for (index, artifact) in artifacts.into_iter().enumerate() {
        let bytes = artifact.content.len();
        byte_semaphore.acquire(bytes);
        if sender
            .send(QueuedArtifact {
                index,
                bytes,
                artifact,
            })
            .is_err()
        {
            byte_semaphore.release(bytes);
            break;
        }
    }
    drop(sender);

    worker
        .join()
        .map_err(|_| ConfigForgeError::Validation("artifact writer worker panicked".to_owned()))?
}

struct QueuedArtifact {
    index: usize,
    bytes: usize,
    artifact: Artifact,
}

#[derive(Debug)]
struct ByteSemaphore {
    capacity: usize,
    state: Mutex<ByteSemaphoreState>,
    capacity_available: Condvar,
}

#[derive(Debug)]
struct ByteSemaphoreState {
    in_flight: usize,
}

impl ByteSemaphore {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(ByteSemaphoreState { in_flight: 0 }),
            capacity_available: Condvar::new(),
        }
    }

    fn acquire(&self, bytes: usize) {
        let mut state = self
            .state
            .lock()
            .expect("byte semaphore lock should not be poisoned");
        while state.in_flight != 0 && state.in_flight.saturating_add(bytes) > self.capacity {
            state = self
                .capacity_available
                .wait(state)
                .expect("byte semaphore lock should not be poisoned");
        }
        state.in_flight = state.in_flight.saturating_add(bytes);
    }

    fn release(&self, bytes: usize) {
        let mut state = self
            .state
            .lock()
            .expect("byte semaphore lock should not be poisoned");
        state.in_flight = state.in_flight.saturating_sub(bytes);
        self.capacity_available.notify_all();
    }
}
fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = parent_dir_to_create(path) {
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

fn parent_dir_to_create(path: &Path) -> Option<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

fn temp_path_for(path: &Path, index: usize) -> PathBuf {
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
    use std::{env, sync::Mutex};

    use super::*;

    static CURRENT_DIR_LOCK: Mutex<()> = Mutex::new(());

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
    fn pure_file_name_path_does_not_try_to_create_empty_parent_dir() {
        let _guard = CURRENT_DIR_LOCK.lock().expect("current dir lock");
        let original_dir = env::current_dir().expect("current dir");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        env::set_current_dir(temp_dir.path()).expect("set current dir");

        let result = ArtifactWriter::new(ArtifactWriteMode::Direct).write(vec![Artifact::text(
            "generated.rs",
            "pub struct Generated;",
        )]);

        env::set_current_dir(original_dir).expect("restore current dir");
        result.expect("pure file name artifact should write");
        assert_eq!(
            fs::read_to_string(temp_dir.path().join("generated.rs")).expect("written artifact"),
            "pub struct Generated;"
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
    fn bounded_direct_write_allows_single_artifact_larger_than_byte_limit() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/large.bin");
        let limits = ArtifactWriteLimits::new(1, 1).expect("limits should be valid");

        let result = ArtifactWriter::new(ArtifactWriteMode::Direct)
            .with_limits(limits)
            .write(vec![Artifact::binary(&path, vec![1, 2, 3, 4])])
            .expect("single oversized artifact should write");

        assert_eq!(result.entries[0].bytes, 4);
        assert_eq!(fs::read(path).expect("written artifact"), vec![1, 2, 3, 4]);
    }

    #[test]
    fn bounded_temp_commit_writes_multiple_artifacts_with_small_queue() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let first = temp_dir.path().join("out/a.txt");
        let second = temp_dir.path().join("out/b.txt");
        let limits = ArtifactWriteLimits::new(1, 1).expect("limits should be valid");

        let result = ArtifactWriter::new(ArtifactWriteMode::TempCommit)
            .with_limits(limits)
            .write(vec![
                Artifact::text(&second, "bb"),
                Artifact::text(&first, "aa"),
            ])
            .expect("bounded temp commit should succeed");

        assert_eq!(result.entries[0].path, first);
        assert_eq!(result.entries[1].path, second);
        assert_eq!(fs::read_to_string(first).expect("first artifact"), "aa");
        assert_eq!(fs::read_to_string(second).expect("second artifact"), "bb");
    }

    #[test]
    fn artifact_write_limits_reject_zero_values() {
        assert!(matches!(
            ArtifactWriteLimits::new(0, 1),
            Err(ConfigForgeError::Validation(message))
                if message.contains("artifact memory limit")
        ));
        assert!(matches!(
            ArtifactWriteLimits::new(1, 0),
            Err(ConfigForgeError::Validation(message))
                if message.contains("artifact in-flight limit")
        ));
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
