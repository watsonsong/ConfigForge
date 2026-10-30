// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::path_util::lexical_normalize_path;
use crate::{ConfigForgeError, Result};

use super::{
    model::{
        Artifact, ArtifactContent, ArtifactPlan, ArtifactWriteEntry, ArtifactWriteMode,
        ArtifactWriteResult, ArtifactWriter,
    },
    path::{
        artifact_collision_code, artifact_collision_error, artifact_collision_message,
        artifact_conflict_key, artifact_state_error, cleanup_temp_files, commit_temp_files,
        normalize_artifact_path, open_file_for_write, temp_path_for,
    },
};

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
            output_root: None,
        }
    }

    pub fn with_output_root(mut self, output_root: impl Into<PathBuf>) -> Self {
        self.output_root = Some(output_root.into());
        self
    }

    pub fn mode(self) -> ArtifactWriteMode {
        self.mode
    }

    pub fn open_sink(self) -> ArtifactSink {
        ArtifactSink::new(self.mode, self.output_root)
    }

    pub fn write(self, artifacts: Vec<Artifact>) -> Result<ArtifactWriteResult> {
        let planned = super::path::plan_artifacts(artifacts)?;
        let sink = self.open_sink();
        sink.reserve_paths(planned.iter().map(|artifact| artifact.path.clone()))?;
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
    output_root: Option<PathBuf>,
    inner: Arc<ArtifactSinkInner>,
}

impl ArtifactSink {
    fn new(mode: ArtifactWriteMode, output_root: Option<PathBuf>) -> Self {
        Self {
            mode,
            output_root: output_root.map(|path| lexical_normalize_path(&path)),
            inner: Arc::new(ArtifactSinkInner {
                state: Mutex::new(ArtifactSinkState::default()),
            }),
        }
    }

    pub(crate) fn reserve_paths(&self, paths: impl IntoIterator<Item = PathBuf>) -> Result<()> {
        let mut state = self.inner.lock_state();
        if state.closed || state.planned || !state.paths.is_empty() || state.first_error.is_some() {
            return Err(artifact_state_error(
                "artifact paths must be reserved before writing".to_owned(),
                "artifact sink already used",
                "unused artifact sink",
            ));
        }
        state.planned = true;
        state.first_error = Some(artifact_state_error(
            "artifact path reservation did not complete".to_owned(),
            "incomplete artifact plan",
            "all artifact paths successfully reserved",
        ));
        let mut reserved = BTreeMap::<PathBuf, ArtifactPathState>::new();
        let mut conflict_keys = BTreeMap::<PathBuf, PathBuf>::new();
        for path in paths {
            let path = normalize_artifact_path(self.output_root.as_deref(), &path)?;
            let key = artifact_conflict_key(self.output_root.as_deref(), &path)?;
            if let Some(existing) = conflict_keys
                .get(&key)
                .or_else(|| reserved.get_key_value(&path).map(|(path, _)| path))
            {
                let error = artifact_collision_error(
                    artifact_collision_code(&path, existing, &key),
                    &artifact_collision_message(&path, existing),
                    &path,
                    existing,
                );
                state.first_error = Some(artifact_collision_error(
                    artifact_collision_code(&path, existing, &key),
                    &artifact_collision_message(&path, existing),
                    &path,
                    existing,
                ));
                return Err(error);
            }
            reserved.insert(path.clone(), ArtifactPathState::Reserved);
            conflict_keys.insert(key, path);
        }
        state.paths = reserved;
        state.conflict_keys = conflict_keys;
        state.first_error = None;
        Ok(())
    }

    pub fn create(&self, path: impl Into<PathBuf>) -> Result<ArtifactWriteStream> {
        let path = path.into();
        let normalized_path = normalize_artifact_path(self.output_root.as_deref(), &path)?;
        let conflict_key = artifact_conflict_key(self.output_root.as_deref(), &normalized_path)?;
        let (index, temp_path) = {
            let mut state = self.inner.lock_state();
            if state.closed {
                return Err(artifact_state_error(
                    "artifact sink is already closed".to_owned(),
                    normalized_path.display().to_string(),
                    "open artifact sink",
                ));
            }
            let reserved = state.paths.get(&normalized_path) == Some(&ArtifactPathState::Reserved);
            if (state.paths.contains_key(&normalized_path) && !reserved)
                || state
                    .conflict_keys
                    .get(&conflict_key)
                    .is_some_and(|existing| !reserved || existing != &normalized_path)
            {
                let existing = state
                    .conflict_keys
                    .get(&conflict_key)
                    .unwrap_or(&normalized_path)
                    .clone();
                let code = artifact_collision_code(&normalized_path, &existing, &conflict_key);
                let message = artifact_collision_message(&normalized_path, &existing);
                if state.first_error.is_none() {
                    state.first_error = Some(artifact_collision_error(
                        code,
                        &message,
                        &normalized_path,
                        &existing,
                    ));
                }
                return Err(artifact_collision_error(
                    code,
                    &message,
                    &normalized_path,
                    &existing,
                ));
            }
            if state.first_error.is_some() {
                return Err(artifact_state_error(
                    "artifact sink is already failed".to_owned(),
                    normalized_path.display().to_string(),
                    "artifact sink without prior write failure",
                ));
            }

            if state.planned && !reserved {
                let error = artifact_state_error(
                    "generator attempted to write an unplanned artifact".to_owned(),
                    normalized_path.display().to_string(),
                    "artifact path declared during planning",
                );
                state.first_error = Some(artifact_state_error(
                    "generator attempted to write an unplanned artifact".to_owned(),
                    normalized_path.display().to_string(),
                    "artifact path declared during planning",
                ));
                return Err(error);
            }

            let index = state.next_index;
            state.next_index += 1;
            state
                .paths
                .insert(normalized_path.clone(), ArtifactPathState::Open);
            state
                .conflict_keys
                .insert(conflict_key, normalized_path.clone());
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
                let Some(temp_path) = temp_path.clone() else {
                    return Err(artifact_state_error(
                        format!(
                            "artifact `{}` is missing a temporary path in temp-commit mode",
                            normalized_path.display()
                        ),
                        "missing temp path",
                        "temp-commit artifact streams must have an allocated temporary path",
                    ));
                };
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
                .filter(|(_, status)| {
                    matches!(
                        status,
                        ArtifactPathState::Open | ArtifactPathState::Reserved
                    )
                })
                .map(|(path, _)| path.display().to_string())
                .collect::<Vec<_>>();
            if !open_paths.is_empty() && state.first_error.is_none() {
                state.first_error = Some(artifact_state_error(
                    format!("planned artifact `{}` was not finished", open_paths[0]),
                    &open_paths[0],
                    "all artifact streams finished before sink.finish",
                ));
            }
            let failed_paths = state
                .paths
                .iter()
                .filter(|(_, status)| **status == ArtifactPathState::Failed)
                .map(|(path, _)| path.display().to_string())
                .collect::<Vec<_>>();
            if !failed_paths.is_empty() && state.first_error.is_none() {
                state.first_error = Some(artifact_state_error(
                    format!("artifact `{}` failed before finish", failed_paths[0]),
                    &failed_paths[0],
                    "all artifact writes successful before sink.finish",
                ));
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
    pub(super) target: ArtifactWriteTarget,
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
pub(super) enum ArtifactWriteTarget {
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
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
            state.first_error = Some(artifact_state_error(
                message,
                path.display().to_string(),
                "artifact path can be opened for writing",
            ));
        }
    }

    fn fail_path_with_message(&self, path: &Path, message: String) {
        let mut state = self.lock_state();
        state
            .paths
            .insert(path.to_path_buf(), ArtifactPathState::Failed);
        if state.first_error.is_none() {
            state.first_error = Some(artifact_state_error(
                message,
                path.display().to_string(),
                "artifact stream write and finish without IO errors",
            ));
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
    planned: bool,
    paths: BTreeMap<PathBuf, ArtifactPathState>,
    conflict_keys: BTreeMap<PathBuf, PathBuf>,
    entries: BTreeMap<PathBuf, ArtifactWriteEntry>,
    temp_files: BTreeMap<PathBuf, PathBuf>,
    first_error: Option<ConfigForgeError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactPathState {
    Reserved,
    Open,
    Finished,
    Failed,
}
