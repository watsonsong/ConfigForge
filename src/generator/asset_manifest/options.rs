// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

use crate::{Result, data_artifact::TargetDataConfig};

use super::FORMAT;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(super) struct AssetManifestOptions {
    pub(super) file_name: Option<String>,
    pub(super) format: AssetManifestFormat,
    pub(super) csv_mode: AssetManifestCsvMode,
    pub(super) deduplicate: bool,
    pub(super) include_referenced_by: bool,
    pub(super) include_source_location: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AssetManifestFormat {
    Json,
    Csv,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AssetManifestCsvMode {
    UnrealDatatable,
    Simple,
}

impl Default for AssetManifestOptions {
    fn default() -> Self {
        Self {
            file_name: None,
            format: AssetManifestFormat::Json,
            csv_mode: AssetManifestCsvMode::UnrealDatatable,
            deduplicate: true,
            include_referenced_by: true,
            include_source_location: false,
        }
    }
}

impl AssetManifestOptions {
    pub(super) fn file_name(&self) -> String {
        self.file_name.clone().unwrap_or_else(|| match self.format {
            AssetManifestFormat::Json => "asset_manifest.json".to_owned(),
            AssetManifestFormat::Csv => "asset_manifest.csv".to_owned(),
        })
    }
}

pub(super) fn options_from_task(task: &TargetDataConfig) -> Result<AssetManifestOptions> {
    task.parse_options(FORMAT, FORMAT)
}
