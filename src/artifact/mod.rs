// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPlan {
    pub path: PathBuf,
}

impl ArtifactPlan {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
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
        Self { mode }
    }

    pub fn mode(self) -> ArtifactWriteMode {
        self.mode
    }

    pub fn open_sink(self) -> ArtifactSink {
        ArtifactSink::new(self.mode)
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
    fn new(mode: ArtifactWriteMode) -> Self {
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
                .filter(|(_, status)| **status == ArtifactPathState::Open)
                .map(|(path, _)| path.display().to_string())
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
                .filter(|(_, status)| **status == ArtifactPathState::Failed)
                .map(|(path, _)| path.display().to_string())
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
            self.inner.clear_temp_files();
            return Err(error);
        }

        if self.mode == ArtifactWriteMode::TempCommit {
            let result = commit_temp_files(&temp_files);
            if result.is_err() {
                cleanup_temp_files(temp_files.values());
                self.inner.clear_temp_files();
            }
            result?;
            self.inner.clear_temp_files();
        }

        Ok(ArtifactWriteResult {
            mode: self.mode,
            entries,
        })
    }
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
        if let Err(source) = self.target.flush() {
            let path = self.write_error_path();
            self.sink.fail_path_with_message(
                &self.path,
                format!("failed to flush `{}`: {source}", path.display()),
            );
            return Err(ConfigForgeError::WriteFile { path, source });
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
            #[cfg(test)]
            ArtifactWriteTarget::FailFlush => Ok(buf.len()),
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
        self.target.flush()
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
    #[cfg(test)]
    FailFlush,
}

impl ArtifactWriteTarget {
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Count => Ok(()),
            Self::File(file) => file.flush(),
            #[cfg(test)]
            Self::FailFlush => Err(io::Error::other("injected flush failure")),
        }
    }
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

    fn clear_temp_files(&self) {
        self.lock_state().temp_files.clear();
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
        replace_with_temp_file(temp_path, final_path)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_with_temp_file(temp_path: &Path, final_path: &Path) -> Result<()> {
    fs::rename(temp_path, final_path).map_err(|source| ConfigForgeError::WriteFile {
        path: final_path.to_path_buf(),
        source,
    })
}

#[cfg(windows)]
fn replace_with_temp_file(temp_path: &Path, final_path: &Path) -> Result<()> {
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
    fn temp_commit_replaces_existing_final_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/generated.rs");
        fs::create_dir_all(path.parent().expect("out dir")).expect("create out dir");
        fs::write(&path, "old content").expect("seed existing artifact");

        let result = ArtifactWriter::new(ArtifactWriteMode::TempCommit)
            .write(vec![Artifact::text(&path, "new content")])
            .expect("temp commit should replace existing final file");

        assert_eq!(result.mode, ArtifactWriteMode::TempCommit);
        assert_eq!(result.entries[0].path, path);
        assert_eq!(result.entries[0].bytes, "new content".len());
        assert_eq!(
            fs::read_to_string(&result.entries[0].path).expect("written artifact"),
            "new content"
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
            plan: ArtifactPlan::new(&path),
            content: &[1, 2, 3, 4],
        };

        let entry = sink.write_producer(producer).expect("producer write");
        let result = sink.finish().expect("finish sink");

        assert_eq!(entry.bytes, 4);
        assert_eq!(result.entries[0].path, path);
        assert_eq!(fs::read(path).expect("producer content"), vec![1, 2, 3, 4]);
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
    fn sink_finish_preserves_flush_error_after_ignored_stream_finish_error() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/generated.rs");
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

        let mut stream = sink.create(&path).expect("create stream");
        stream.target = ArtifactWriteTarget::FailFlush;
        stream.write_all(b"content").expect("write content");

        let immediate_error = stream.finish().unwrap_err();
        assert!(matches!(
            immediate_error,
            ConfigForgeError::WriteFile { path: error_path, source }
                if error_path == path && source.to_string().contains("injected flush failure")
        ));

        let finish_error = sink.finish().unwrap_err();

        assert!(matches!(
            finish_error,
            ConfigForgeError::Validation(message)
                if message.contains("failed to flush")
                    && message.contains("injected flush failure")
                    && !message.contains("dropped before finish")
        ));
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
    fn sink_temp_commit_finish_clears_temp_registry_before_late_clone_drop() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/generated.rs");
        let temp_path = temp_path_for(&path, 0);

        let retained_sink_clone = {
            let sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();
            let retained_sink_clone = sink.clone();
            sink.write_text(&path, "first")
                .expect("first temp artifact should write");
            sink.finish().expect("first sink should commit");
            retained_sink_clone
        };
        assert_eq!(fs::read_to_string(&path).expect("first artifact"), "first");

        let second_sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();
        let mut second_stream = second_sink.create(&path).expect("create second stream");
        second_stream.write_all(b"second").expect("write second");
        second_stream.finish().expect("finish second stream");
        assert!(temp_path.exists());

        drop(retained_sink_clone);

        assert!(
            temp_path.exists(),
            "late drop of a previously finished sink clone must not remove a new sink temp file"
        );
        second_sink.finish().expect("second sink should commit");
        assert_eq!(
            fs::read_to_string(&path).expect("second artifact"),
            "second"
        );
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
    fn direct_stream_writes_large_artifact_through_standard_writer() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("out/large.bin");

        let result = ArtifactWriter::new(ArtifactWriteMode::Direct)
            .write(vec![Artifact::binary(&path, vec![1, 2, 3, 4])])
            .expect("large artifact should write");

        assert_eq!(result.entries[0].bytes, 4);
        assert_eq!(fs::read(path).expect("written artifact"), vec![1, 2, 3, 4]);
    }

    #[test]
    fn temp_commit_writes_multiple_artifacts_through_standard_writer() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let first = temp_dir.path().join("out/a.txt");
        let second = temp_dir.path().join("out/b.txt");

        let result = ArtifactWriter::new(ArtifactWriteMode::TempCommit)
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
