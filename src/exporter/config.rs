// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TargetDataConfig {
    #[serde(default)]
    pub name: Option<String>,
    pub input: PathBuf,
    pub output: PathBuf,
    #[serde(default = "default_data_format")]
    pub format: String,
    #[serde(flatten)]
    pub options: BTreeMap<String, toml::Value>,
}

fn default_data_format() -> String {
    "json".to_owned()
}
