// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPlan {
    pub path: PathBuf,
    pub estimated_generation_bytes: usize,
}

impl ArtifactPlan {
    pub fn new(path: impl Into<PathBuf>, estimated_generation_bytes: usize) -> Self {
        Self {
            path: path.into(),
            estimated_generation_bytes,
        }
    }
}

pub trait ArtifactProducer {
    fn plan(&self) -> &ArtifactPlan;

    fn produce(self, stream: &mut ArtifactWriteStream) -> Result<()>
    where
        Self: Sized;
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

    pub fn open_sink(self) -> ArtifactSink {
        ArtifactSink::new(self.mode, self.limits)
    }

    pub fn write(self, artifacts: Vec<Artifact>) -> Result<ArtifactWriteResult> {
        let planned = plan_artifacts(artifacts)?;
        let sink = self.open_sink();
        for artifact in planned {
            match artifact.content {
                ArtifactContent::Text(content) => sink.write_text(artifact.path, &content)?,
                ArtifactContent::Binary(content) => sink.write_binary(artifact.path, &content)?,
            }
        }
        sink.finish()
    }
}

#[derive(Debug, Clone)]
pub struct ArtifactSink {
    mode: ArtifactWriteMode,
    inner: Arc<ArtifactSinkInner>,
}

impl ArtifactSink {
    fn new(mode: ArtifactWriteMode, _limits: ArtifactWriteLimits) -> Self {
        Self {
            mode,
            inner: Arc::new(ArtifactSinkInner {
                state: Mutex::new(ArtifactSinkState::default()),
            }),
        }
    }

    pub fn create(&self, path: impl Into<PathBuf>) -> Result<ArtifactWriteStream> {
        let path = path.into();
        let normalized_path = lexical_normalize_path(&path);
        let (index, temp_path) = {
            let mut state = self.inner.lock_state();
            if state.closed {
                return Err(ConfigForgeError::Validation(
                    "artifact sink is already closed".to_owned(),
                ));
            }
            if state.paths.contains_key(&normalized_path) {
                let message = format!("multiple artifacts target `{}`", normalized_path.display());
                if state.first_error.is_none() {
                    state.first_error = Some(ConfigForgeError::Validation(message.clone()));
                }
                return Err(ConfigForgeError::Validation(message));
            }
            if state.first_error.is_some() {
                return Err(ConfigForgeError::Validation(
                    "artifact sink is already failed".to_owned(),
                ));
            }

            let index = state.next_index;
            state.next_index += 1;
            state
                .paths
                .insert(normalized_path.clone(), ArtifactPathState::Open);
            let temp_path = (self.mode == ArtifactWriteMode::TempCommit)
                .then(|| temp_path_for(&normalized_path, index));
            (index, temp_path)
        };

        let target = match self.mode {
            ArtifactWriteMode::DryRun => ArtifactWriteTarget::Count,
            ArtifactWriteMode::Direct => match open_file_for_write(&normalized_path) {
                Ok(file) => ArtifactWriteTarget::File(file),
                Err(error) => {
                    self.inner.record_path_error(
                        &normalized_path,
                        format!("failed to open artifact `{}`", normalized_path.display()),
                    );
                    return Err(error);
                }
            },
            ArtifactWriteMode::TempCommit => {
                let temp_path = temp_path
                    .clone()
                    .expect("temp path should exist in temp-commit mode");
                match open_file_for_write(&temp_path) {
                    Ok(file) => ArtifactWriteTarget::File(file),
                    Err(error) => {
                        self.inner.record_path_error(
                            &normalized_path,
                            format!("failed to open artifact `{}`", temp_path.display()),
                        );
                        return Err(error);
                    }
                }
            }
        };

        Ok(ArtifactWriteStream {
            sink: self.inner.clone(),
            mode: self.mode,
            path: normalized_path,
            temp_path,
            target,
            bytes: 0,
            finished: false,
            _index: index,
        })
    }

    pub fn write_text(&self, path: impl Into<PathBuf>, content: impl AsRef<str>) -> Result<()> {
        let mut stream = self.create(path)?;
        stream.write_all_artifact(content.as_ref().as_bytes())?;
        stream.finish().map(|_| ())
    }

    pub fn write_binary(&self, path: impl Into<PathBuf>, content: impl AsRef<[u8]>) -> Result<()> {
        let mut stream = self.create(path)?;
        stream.write_all_artifact(content.as_ref())?;
        stream.finish().map(|_| ())
    }

    pub fn write_producer<P>(&self, producer: P) -> Result<ArtifactWriteEntry>
    where
        P: ArtifactProducer,
    {
        let path = producer.plan().path.clone();
        let mut stream = self.create(path)?;
        producer.produce(&mut stream)?;
        stream.finish()
    }

    pub fn finish(self) -> Result<ArtifactWriteResult> {
        let (entries, temp_files, first_error) = {
            let mut state = self.inner.lock_state();
            state.closed = true;
            let open_paths = state
                .paths
                .iter()
                .filter_map(|(path, status)| {
                    (*status == ArtifactPathState::Open).then(|| path.display().to_string())
                })
                .collect::<Vec<_>>();
            if !open_paths.is_empty() && state.first_error.is_none() {
                state.first_error = Some(ConfigForgeError::Validation(format!(
                    "artifact stream `{}` was not finished",
                    open_paths[0]
                )));
            }
            let failed_paths = state
                .paths
                .iter()
                .filter_map(|(path, status)| {
                    (*status == ArtifactPathState::Failed).then(|| path.display().to_string())
                })
                .collect::<Vec<_>>();
            if !failed_paths.is_empty() && state.first_error.is_none() {
                state.first_error = Some(ConfigForgeError::Validation(format!(
                    "artifact `{}` failed before finish",
                    failed_paths[0]
                )));
            }
            (
                state.entries.values().cloned().collect::<Vec<_>>(),
                state.temp_files.clone(),
                state.first_error.take(),
            )
        };

        if let Some(error) = first_error {
            cleanup_temp_files(temp_files.values());
            return Err(error);
        }

        if self.mode == ArtifactWriteMode::TempCommit {
            let result = commit_temp_files(&temp_files);
            if result.is_err() {
                cleanup_temp_files(temp_files.values());
            }
            result?;
        }

        Ok(ArtifactWriteResult {
            mode: self.mode,
            entries,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ArtifactGenerationLimiter {
    inner: Arc<ArtifactGenerationLimiterInner>,
}

impl ArtifactGenerationLimiter {
    pub fn new(max_in_flight_bytes: usize, unknown_generation_bytes: usize) -> Self {
        Self {
            inner: Arc::new(ArtifactGenerationLimiterInner {
                byte_semaphore: ByteSemaphore::new(max_in_flight_bytes.max(1)),
                unknown_generation_bytes: unknown_generation_bytes.max(1),
            }),
        }
    }

    pub fn from_limits(limits: ArtifactWriteLimits) -> Self {
        let limits = limits.normalized();
        Self::new(
            limits.max_in_flight_bytes,
            limits
                .max_in_flight_bytes
                .checked_div(limits.max_in_flight_artifacts)
                .unwrap_or(1)
                .max(1),
        )
    }

    pub fn acquire(&self, estimated_bytes: usize) -> ArtifactGenerationPermit {
        let bytes = estimated_bytes.max(1);
        self.inner.byte_semaphore.acquire(bytes);
        ArtifactGenerationPermit {
            limiter: self.inner.clone(),
            bytes,
            released: false,
        }
    }

    pub fn acquire_unknown(&self) -> ArtifactGenerationPermit {
        self.acquire(self.inner.unknown_generation_bytes)
    }

    pub fn run_unknown<T>(&self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        let _permit = self.acquire_unknown();
        operation()
    }
}

#[derive(Debug)]
pub struct ArtifactGenerationPermit {
    limiter: Arc<ArtifactGenerationLimiterInner>,
    bytes: usize,
    released: bool,
}

impl ArtifactGenerationPermit {
    fn release_once(&mut self) {
        if !self.released {
            self.limiter.byte_semaphore.release(self.bytes);
            self.released = true;
        }
    }
}

impl Drop for ArtifactGenerationPermit {
    fn drop(&mut self) {
        self.release_once();
    }
}

#[derive(Debug)]
struct ArtifactGenerationLimiterInner {
    byte_semaphore: ByteSemaphore,
    unknown_generation_bytes: usize,
}

#[derive(Debug)]
pub struct ArtifactWriteStream {
    sink: Arc<ArtifactSinkInner>,
    mode: ArtifactWriteMode,
    path: PathBuf,
    temp_path: Option<PathBuf>,
    target: ArtifactWriteTarget,
    bytes: usize,
    finished: bool,
    _index: usize,
}

impl ArtifactWriteStream {
    pub fn write_all_artifact(&mut self, bytes: &[u8]) -> Result<()> {
        self.write_all(bytes)
            .map_err(|source| ConfigForgeError::WriteFile {
                path: self.write_error_path(),
                source,
            })
    }

    pub fn finish(mut self) -> Result<ArtifactWriteEntry> {
        if let ArtifactWriteTarget::File(file) = &mut self.target {
            file.flush().map_err(|source| ConfigForgeError::WriteFile {
                path: self.write_error_path(),
                source,
            })?;
        }

        let entry = ArtifactWriteEntry {
            path: self.path.clone(),
            bytes: self.bytes,
            written: self.mode != ArtifactWriteMode::DryRun,
        };
        self.sink
            .mark_path_finished(self.path.clone(), self.temp_path.clone(), entry.clone());
        self.finished = true;
        Ok(entry)
    }

    fn write_error_path(&self) -> PathBuf {
        self.temp_path.clone().unwrap_or_else(|| self.path.clone())
    }
}

impl Write for ArtifactWriteStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let result = match &mut self.target {
            ArtifactWriteTarget::Count => Ok(buf.len()),
            ArtifactWriteTarget::File(file) => file.write(buf),
        };

        match result {
            Ok(written) => {
                self.bytes = self.bytes.saturating_add(written);
                Ok(written)
            }
            Err(error) => {
                self.sink.fail_path_with_message(
                    &self.path,
                    format!(
                        "failed to write `{}`: {error}",
                        self.write_error_path().display()
                    ),
                );
                Err(error)
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.target {
            ArtifactWriteTarget::Count => Ok(()),
            ArtifactWriteTarget::File(file) => file.flush(),
        }
    }
}

impl Drop for ArtifactWriteStream {
    fn drop(&mut self) {
        if !self.finished {
            self.sink.fail_path_with_message(
                &self.path,
                format!(
                    "artifact stream `{}` was dropped before finish",
                    self.path.display()
                ),
            );
            if let Some(temp_path) = &self.temp_path {
                let _ = fs::remove_file(temp_path);
            }
        }
    }
}

#[derive(Debug)]
enum ArtifactWriteTarget {
    Count,
    File(File),
}

#[derive(Debug)]
struct ArtifactSinkInner {
    state: Mutex<ArtifactSinkState>,
}

impl Drop for ArtifactSinkInner {
    fn drop(&mut self) {
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cleanup_temp_files(state.temp_files.values());
        state.temp_files.clear();
    }
}

impl ArtifactSinkInner {
    fn lock_state(&self) -> std::sync::MutexGuard<'_, ArtifactSinkState> {
        self.state
            .lock()
            .expect("artifact sink state lock should not be poisoned")
    }

    fn mark_path_finished(
        &self,
        path: PathBuf,
        temp_path: Option<PathBuf>,
        entry: ArtifactWriteEntry,
    ) {
        let mut state = self.lock_state();
        state
            .paths
            .insert(path.clone(), ArtifactPathState::Finished);
        if let Some(temp_path) = temp_path {
            state.temp_files.insert(path.clone(), temp_path);
        }
        state.entries.insert(path, entry);
    }

    fn record_path_error(&self, path: &Path, message: String) {
        let mut state = self.lock_state();
        state
            .paths
            .insert(path.to_path_buf(), ArtifactPathState::Failed);
        if state.first_error.is_none() {
            state.first_error = Some(ConfigForgeError::Validation(message));
        }
    }

    fn fail_path_with_message(&self, path: &Path, message: String) {
        let mut state = self.lock_state();
        state
            .paths
            .insert(path.to_path_buf(), ArtifactPathState::Failed);
        if state.first_error.is_none() {
            state.first_error = Some(ConfigForgeError::Validation(message));
        }
    }
}

#[derive(Debug, Default)]
struct ArtifactSinkState {
    next_index: usize,
    closed: bool,
    paths: BTreeMap<PathBuf, ArtifactPathState>,
    entries: BTreeMap<PathBuf, ArtifactWriteEntry>,
    temp_files: BTreeMap<PathBuf, PathBuf>,
    first_error: Option<ConfigForgeError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactPathState {
    Open,
    Finished,
    Failed,
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

fn open_file_for_write(path: &Path) -> Result<File> {
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

fn commit_temp_files(temp_files: &BTreeMap<PathBuf, PathBuf>) -> Result<()> {
    for (final_path, temp_path) in temp_files {
        fs::rename(temp_path, final_path).map_err(|source| ConfigForgeError::WriteFile {
            path: final_path.clone(),
            source,
        })?;
    }
    Ok(())
}

fn cleanup_temp_files<'a>(temp_files: impl IntoIterator<Item = &'a PathBuf>) {
    for temp_path in temp_files {
        let _ = fs::remove_file(temp_path);
    }
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
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::thread;
    use std::time::Duration;
    use std::{env, io::Write as _, sync::Mutex};

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
    fn sink_direct_stream_writes_content_and_entries() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let first = temp_dir.path().join("out/b.txt");
        let second = temp_dir.path().join("out/a.txt");
        let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();

        let mut stream = sink.create(&first).expect("create first stream");
        stream.write_all(b"b").expect("write first");
        let first_entry = stream.finish().expect("finish first");
        assert_eq!(first_entry.bytes, 1);
        sink.write_text(&second, "aa").expect("write second");

        let result = sink.finish().expect("finish sink");

        assert_eq!(result.mode, ArtifactWriteMode::Direct);
        assert_eq!(result.entries[0].path, second);
        assert_eq!(result.entries[0].bytes, 2);
        assert_eq!(result.entries[1].path, first);
        assert_eq!(fs::read_to_string(first).expect("first artifact"), "b");
        assert_eq!(fs::read_to_string(second).expect("second artifact"), "aa");
    }

    #[test]
    fn sink_dry_run_counts_stream_bytes_without_writing() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/generated.rs");
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

        let mut stream = sink.create(&path).expect("create dry-run stream");
        stream.write_all(b"abc").expect("write dry-run stream");
        stream.write_all(b"de").expect("write dry-run stream");
        stream.finish().expect("finish dry-run stream");
        let result = sink.finish().expect("finish sink");

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].path, path);
        assert_eq!(result.entries[0].bytes, 5);
        assert!(!result.entries[0].written);
        assert!(!temp_dir.path().join("out").exists());
    }

    struct TestProducer {
        plan: ArtifactPlan,
        content: &'static [u8],
    }

    impl ArtifactProducer for TestProducer {
        fn plan(&self) -> &ArtifactPlan {
            &self.plan
        }

        fn produce(self, stream: &mut ArtifactWriteStream) -> Result<()> {
            stream.write_all_artifact(self.content)
        }
    }

    #[test]
    fn sink_writes_artifact_producer() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/producer.bin");
        let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
        let producer = TestProducer {
            plan: ArtifactPlan::new(&path, 4),
            content: &[1, 2, 3, 4],
        };

        let entry = sink.write_producer(producer).expect("producer write");
        let result = sink.finish().expect("finish sink");

        assert_eq!(entry.bytes, 4);
        assert_eq!(result.entries[0].path, path);
        assert_eq!(fs::read(path).expect("producer content"), vec![1, 2, 3, 4]);
    }

    #[test]
    fn generation_limiter_blocks_until_generation_permit_is_released() {
        let limiter = ArtifactGenerationLimiter::new(4, 4);
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for _ in 0..2 {
            let limiter = limiter.clone();
            let active = active.clone();
            let max_active = max_active.clone();
            handles.push(thread::spawn(move || {
                let _permit = limiter.acquire(4);
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_active.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(20));
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }

        for handle in handles {
            handle.join().expect("thread should join");
        }

        assert_eq!(max_active.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn sink_rejects_duplicate_path_when_stream_is_open() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let first = temp_dir.path().join("out/data/items.json");
        let duplicate = temp_dir.path().join("out/data/./nested/../items.json");
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

        let stream = sink.create(&first).expect("create first stream");
        let error = sink.create(&duplicate).unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("multiple artifacts target"))
        );
        stream.finish().expect("finish first stream");
        let error = sink.finish().unwrap_err();
        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("multiple artifacts target"))
        );
    }

    #[test]
    fn sink_finish_fails_after_ignored_duplicate_create_error() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/data/items.json");
        let duplicate = temp_dir.path().join("out/data/./items.json");
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

        let stream = sink.create(&path).expect("create first stream");
        let _ignored = sink.create(&duplicate).unwrap_err();
        stream.finish().expect("finish first stream");

        let error = sink.finish().unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("multiple artifacts target"))
        );
    }

    #[test]
    fn sink_finish_fails_after_ignored_open_file_error() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let blocked_parent = temp_dir.path().join("out-as-file");
        fs::write(&blocked_parent, "not a directory").expect("blocked parent file");
        let path = blocked_parent.join("generated.rs");
        let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();

        let _ignored = sink.create(&path).unwrap_err();

        let error = sink.finish().unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("failed to open artifact"))
        );
    }

    #[test]
    fn sink_temp_commit_defers_visibility_until_finish() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/generated.rs");
        let sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();

        let mut stream = sink.create(&path).expect("create temp stream");
        stream
            .write_all(b"pub struct Generated;")
            .expect("write temp");
        stream.finish().expect("finish temp stream");
        assert!(!path.exists());

        let result = sink.finish().expect("finish temp sink");

        assert_eq!(result.entries[0].path, path);
        assert_eq!(
            fs::read_to_string(path).expect("written artifact"),
            "pub struct Generated;"
        );
    }

    #[test]
    fn sink_temp_commit_drop_cleans_finished_temp_files_without_finish() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/generated.rs");

        {
            let sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();
            sink.write_text(&path, "pub struct Generated;")
                .expect("temp artifact should write");
            assert!(!path.exists());

            let temp_files = fs::read_dir(path.parent().expect("out dir"))
                .expect("out dir should exist")
                .collect::<std::result::Result<Vec<_>, _>>()
                .expect("read temp dir");
            assert_eq!(temp_files.len(), 1);
        }

        assert!(!path.exists());
        let temp_files = fs::read_dir(path.parent().expect("out dir"))
            .expect("out dir should exist")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("read temp dir");
        assert!(temp_files.is_empty());
    }

    #[test]
    fn sink_temp_commit_fails_when_stream_is_dropped_without_finish() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/generated.rs");
        let sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();

        {
            let mut stream = sink.create(&path).expect("create temp stream");
            stream.write_all(b"partial").expect("write temp stream");
        }

        let error = sink.finish().unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("dropped before finish"))
        );
        assert!(!path.exists());
        let out_dir = temp_dir.path().join("out");
        let temp_files = fs::read_dir(out_dir)
            .expect("out dir")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("read temp dir");
        assert!(temp_files.is_empty());
    }

    #[test]
    fn writer_vec_api_uses_sink_and_matches_manual_sink_result() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let writer_path = temp_dir.path().join("writer/out.txt");
        let sink_path = temp_dir.path().join("sink/out.txt");

        let writer_result = ArtifactWriter::new(ArtifactWriteMode::Direct)
            .write(vec![Artifact::text(&writer_path, "same")])
            .expect("writer api");

        let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
        sink.write_text(&sink_path, "same").expect("sink write");
        let sink_result = sink.finish().expect("sink finish");

        assert_eq!(writer_result.entries[0].bytes, sink_result.entries[0].bytes);
        assert_eq!(
            writer_result.entries[0].written,
            sink_result.entries[0].written
        );
        assert_eq!(
            fs::read_to_string(writer_path).expect("writer file"),
            "same"
        );
        assert_eq!(fs::read_to_string(sink_path).expect("sink file"), "same");
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
