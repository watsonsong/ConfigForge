// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactWriter {
    mode: ArtifactWriteMode,
    output_root: Option<PathBuf>,
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
            if state.paths.contains_key(&normalized_path)
                || state.conflict_keys.contains_key(&conflict_key)
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
                state.first_error = Some(artifact_state_error(
                    format!("artifact stream `{}` was not finished", open_paths[0]),
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
    paths: BTreeMap<PathBuf, ArtifactPathState>,
    conflict_keys: BTreeMap<PathBuf, PathBuf>,
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

fn normalize_artifact_path(output_root: Option<&Path>, path: &Path) -> Result<PathBuf> {
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

fn filesystem_path_is_within_root(path: &Path, root: &Path) -> bool {
    let root_key = filesystem_aware_path_key(root).unwrap_or_else(|| root.to_path_buf());
    let path_key = filesystem_aware_path_key(path).unwrap_or_else(|| path.to_path_buf());
    let root_key = case_fold_conflict_key(root_key);
    let path_key = case_fold_conflict_key(path_key);
    path_is_within_root(&path_key, &root_key)
}

fn artifact_conflict_key(output_root: Option<&Path>, normalized_path: &Path) -> Result<PathBuf> {
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

fn artifact_collision_message(path: &Path, existing: &Path) -> String {
    format!(
        "multiple artifacts target `{}` (conflicts with `{}`)",
        path.display(),
        existing.display()
    )
}

fn artifact_collision_code(path: &Path, existing: &Path, conflict_key: &Path) -> &'static str {
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

fn artifact_collision_error(
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

fn artifact_path_error(
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

fn artifact_state_error(
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
fn existing_file_identity_key(path: &Path) -> Option<PathBuf> {
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
fn existing_file_identity_key(_path: &Path) -> Option<PathBuf> {
    None
}

#[cfg(any(target_os = "macos", windows))]
fn case_fold_conflict_key(path: PathBuf) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_ascii_lowercase())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn case_fold_conflict_key(path: PathBuf) -> PathBuf {
    path
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

#[cfg(test)]
mod tests {
    use std::{env, io::Write as _, sync::Mutex};

    use crate::diagnostic::code::{
        CF_ARTIFACT_CASE_INSENSITIVE_COLLISION, CF_ARTIFACT_HARDLINK_ALIAS_COLLISION,
        CF_ARTIFACT_PATH_COLLISION, CF_ARTIFACT_PATH_ESCAPE, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION,
        CF_ARTIFACT_WRITE_STATE,
    };

    use super::*;

    static CURRENT_DIR_LOCK: Mutex<()> = Mutex::new(());

    fn assert_artifact_diagnostic(error: ConfigForgeError, code: &str, current_contains: &str) {
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected structured artifact diagnostic");
        };
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.code, code);
        assert_eq!(diagnostic.source.as_deref(), Some("artifact.writer"));
        assert!(
            diagnostic
                .current_value
                .as_deref()
                .is_some_and(|current| current.contains(current_contains)),
            "expected current_value to contain `{current_contains}`, got {:?}",
            diagnostic.current_value
        );
        assert!(diagnostic.expected.is_some());
    }

    fn assert_artifact_state_diagnostic(error: ConfigForgeError, current_contains: &str) {
        assert_artifact_diagnostic(error, CF_ARTIFACT_WRITE_STATE, current_contains);
    }

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

        assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_COLLISION, "items.json");
        stream.finish().expect("finish first stream");
        let error = sink.finish().unwrap_err();
        assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_COLLISION, "items.json");
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

        assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_COLLISION, "items.json");
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

        assert_artifact_state_diagnostic(error, "generated.rs");
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

        assert_artifact_state_diagnostic(finish_error, "generated.rs");
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

        assert_artifact_state_diagnostic(error, "generated.rs");
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

        assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_COLLISION, "items.json");
        assert!(!temp_dir.path().join("out").exists());
    }

    #[test]
    fn output_root_rejects_absolute_path_escape() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join("project");
        let outside = temp_dir.path().join("outside/generated.rs");

        let error = ArtifactWriter::new(ArtifactWriteMode::Direct)
            .with_output_root(&root)
            .write(vec![Artifact::text(&outside, "escape")])
            .unwrap_err();

        assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_ESCAPE, "outside/generated.rs");
        assert!(!outside.exists());
    }

    #[test]
    fn output_root_rejects_parent_escape() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join("project");

        let error = ArtifactWriter::new(ArtifactWriteMode::DryRun)
            .with_output_root(&root)
            .write(vec![Artifact::text("../outside/generated.rs", "escape")])
            .unwrap_err();

        assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_ESCAPE, "../outside/generated.rs");
    }

    #[cfg(unix)]
    #[test]
    fn output_root_detects_symlink_alias_conflict() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join("project");
        let out = root.join("out");
        let alias = root.join("out_alias");
        fs::create_dir_all(&out).expect("create out dir");
        std::os::unix::fs::symlink(&out, &alias).expect("create symlink alias");

        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
            .with_output_root(&root)
            .open_sink();
        sink.write_text(out.join("items.json"), "first")
            .expect("first path should plan");
        let error = sink
            .write_text(alias.join("items.json"), "second")
            .unwrap_err();

        assert_artifact_diagnostic(error, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION, "items.json");
    }

    #[cfg(unix)]
    #[test]
    fn output_root_rejects_existing_symlink_target_escape() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join("project");
        let outside = temp_dir.path().join("outside.txt");
        let symlink_path = root.join("out/generated.rs");
        fs::create_dir_all(symlink_path.parent().expect("out dir")).expect("create out dir");
        fs::write(&outside, "outside").expect("write outside file");
        std::os::unix::fs::symlink(&outside, &symlink_path).expect("create symlink file");

        let error = ArtifactWriter::new(ArtifactWriteMode::Direct)
            .with_output_root(&root)
            .write(vec![Artifact::text(&symlink_path, "new")])
            .unwrap_err();

        assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_ESCAPE, "generated.rs");
        assert_eq!(
            fs::read_to_string(outside).expect("outside file"),
            "outside"
        );
    }

    #[cfg(unix)]
    #[test]
    fn output_root_detects_existing_hardlink_alias_conflict() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join("project");
        let first = root.join("out/a.txt");
        let second = root.join("out/b.txt");
        fs::create_dir_all(first.parent().expect("out dir")).expect("create out dir");
        fs::write(&first, "same inode").expect("write first hardlink target");
        fs::hard_link(&first, &second).expect("create hardlink alias");

        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
            .with_output_root(&root)
            .open_sink();
        sink.write_text(&first, "first")
            .expect("first path should plan");
        let error = sink.write_text(&second, "second").unwrap_err();

        assert_artifact_diagnostic(error, CF_ARTIFACT_HARDLINK_ALIAS_COLLISION, "b.txt");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn output_root_detects_case_insensitive_collision_on_default_macos_volume() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        if !directory_is_case_insensitive(temp_dir.path()) {
            eprintln!(
                "skipping macOS case-insensitive collision test: temp volume is case-sensitive"
            );
            return;
        }
        assert_case_alias_conflict(temp_dir.path().join("project"));
    }

    #[cfg(windows)]
    #[test]
    fn output_root_detects_case_insensitive_collision_on_windows() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        assert_case_alias_conflict(temp_dir.path().join("project"));
    }

    #[cfg(any(target_os = "macos", windows))]
    fn assert_case_alias_conflict(root: PathBuf) {
        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
            .with_output_root(&root)
            .open_sink();
        sink.write_text("Out/items.json", "first")
            .expect("first path should plan");
        let error = sink.write_text("out/items.json", "second").unwrap_err();

        assert_artifact_diagnostic(error, CF_ARTIFACT_CASE_INSENSITIVE_COLLISION, "items.json");
    }

    #[cfg(target_os = "macos")]
    fn directory_is_case_insensitive(directory: &Path) -> bool {
        let probe = directory.join("configforge_case_probe");
        let alias = directory.join("CONFIGFORGE_CASE_PROBE");
        fs::write(&probe, "probe").expect("write case probe");
        alias.exists()
    }

    #[cfg(windows)]
    #[test]
    fn output_root_detects_windows_symlink_alias_conflict() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join("project");
        let out = root.join("out");
        let alias = root.join("out_alias");
        fs::create_dir_all(&out).expect("create out dir");
        if !try_create_windows_symlink_dir(&out, &alias) {
            return;
        }

        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
            .with_output_root(&root)
            .open_sink();
        sink.write_text(out.join("items.json"), "first")
            .expect("first path should plan");
        let error = sink
            .write_text(alias.join("items.json"), "second")
            .unwrap_err();

        assert_artifact_diagnostic(error, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION, "items.json");
    }

    #[cfg(windows)]
    #[test]
    fn output_root_detects_windows_junction_alias_conflict() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join("project");
        let out = root.join("out");
        let alias = root.join("out_junction");
        fs::create_dir_all(&out).expect("create out dir");
        if !try_create_windows_junction(&out, &alias) {
            return;
        }

        let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
            .with_output_root(&root)
            .open_sink();
        sink.write_text(out.join("items.json"), "first")
            .expect("first path should plan");
        let error = sink
            .write_text(alias.join("items.json"), "second")
            .unwrap_err();

        assert_artifact_diagnostic(error, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION, "items.json");
    }

    #[cfg(windows)]
    #[test]
    fn windows_lexical_normalization_preserves_drive_verbatim_and_unc_prefixes() {
        let cases = [
            (
                r"C:\Project\out\..\Data\items.json",
                r"C:\Project\Data\items.json",
            ),
            (
                r"\\?\C:\Project\out\..\Data\items.json",
                r"\\?\C:\Project\Data\items.json",
            ),
            (
                r"\\server\share\Project\out\..\Data\items.json",
                r"\\server\share\Project\Data\items.json",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(
                lexical_normalize_path(Path::new(input)),
                PathBuf::from(expected)
            );
        }
    }

    #[cfg(windows)]
    fn try_create_windows_symlink_dir(target: &Path, link: &Path) -> bool {
        match std::os::windows::fs::symlink_dir(target, link) {
            Ok(()) => true,
            Err(error) => {
                eprintln!(
                    "skipping Windows symlink alias test: could not create directory symlink `{}` -> `{}`: {error}",
                    link.display(),
                    target.display()
                );
                false
            }
        }
    }

    #[cfg(windows)]
    fn try_create_windows_junction(target: &Path, link: &Path) -> bool {
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output();
        match output {
            Ok(output) if output.status.success() => true,
            Ok(output) => {
                eprintln!(
                    "skipping Windows junction alias test: mklink /J failed with status {}: {}{}",
                    output.status,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                false
            }
            Err(error) => {
                eprintln!("skipping Windows junction alias test: could not run mklink /J: {error}");
                false
            }
        }
    }

    #[test]
    fn temp_commit_reports_rename_failure_and_cleans_temp_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join("project");
        let path = root.join("out/generated.rs");
        fs::create_dir_all(&path).expect("create final path as directory");

        let error = ArtifactWriter::new(ArtifactWriteMode::TempCommit)
            .with_output_root(&root)
            .write(vec![Artifact::text(&path, "content")])
            .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::WriteFile { path: error_path, .. } if error_path == path)
        );
        let temp_files = fs::read_dir(path.parent().expect("out dir"))
            .expect("read out dir")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect out dir");
        assert!(temp_files.iter().all(|entry| entry.path() == path));
    }
}
