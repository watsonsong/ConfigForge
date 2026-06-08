// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, de::DeserializeOwned};

use crate::{ConfigForgeError, Result};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TargetDataConfig {
    #[serde(default)]
    pub name: Option<String>,
    pub output: PathBuf,
    #[serde(default = "default_data_format")]
    pub format: String,
    #[serde(flatten)]
    pub options: BTreeMap<String, toml::Value>,
}

impl TargetDataConfig {
    pub fn parse_options<T>(&self, nested_key: &str, label: &str) -> Result<T>
    where
        T: Default + DeserializeOwned,
    {
        if let Some(value) = self.options.get(nested_key) {
            return value.clone().try_into().map_err(|error| {
                ConfigForgeError::Validation(format!("invalid {label} options: {error}"))
            });
        }
        if self.options.is_empty() {
            return Ok(T::default());
        }
        toml::Value::Table(self.options.clone().into_iter().collect())
            .try_into()
            .map_err(|error| {
                ConfigForgeError::Validation(format!("invalid {label} options: {error}"))
            })
    }
}

fn default_data_format() -> String {
    "json".to_owned()
}
