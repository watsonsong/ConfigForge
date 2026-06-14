// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

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
    pub(super) mode: ArtifactWriteMode,
    pub(super) output_root: Option<PathBuf>,
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
