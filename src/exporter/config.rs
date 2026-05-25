// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TargetDataConfig {
    #[serde(default)]
    pub name: Option<String>,
    pub input: PathBuf,
    pub output: PathBuf,
    #[serde(default = "default_data_format")]
    pub format: String,
}

fn default_data_format() -> String {
    "json".to_owned()
}
