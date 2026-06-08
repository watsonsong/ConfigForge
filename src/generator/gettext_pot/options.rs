// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

use crate::{Result, data_artifact::TargetDataConfig};

use super::FORMAT;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(super) struct GettextPotOptions {
    pub(super) domain: Option<String>,
    pub(super) file_name: Option<String>,
    pub(super) context: PotContextMode,
    pub(super) source_reference: PotSourceReferenceMode,
    pub(super) include_translator_comments: bool,
    pub(super) deduplicate: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum PotContextMode {
    None,
    TableField,
    Field,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum PotSourceReferenceMode {
    SourceLocation,
    RecordKey,
    Both,
    None,
}

impl Default for GettextPotOptions {
    fn default() -> Self {
        Self {
            domain: None,
            file_name: None,
            context: PotContextMode::TableField,
            source_reference: PotSourceReferenceMode::SourceLocation,
            include_translator_comments: true,
            deduplicate: true,
        }
    }
}

impl GettextPotOptions {
    pub(super) fn file_name(&self) -> String {
        self.file_name.clone().unwrap_or_else(|| {
            format!(
                "{}.pot",
                self.domain
                    .as_deref()
                    .unwrap_or("messages")
                    .replace('/', "_")
            )
        })
    }
}

pub(super) fn options_from_task(task: &TargetDataConfig) -> Result<GettextPotOptions> {
    task.parse_options(FORMAT, FORMAT)
}
