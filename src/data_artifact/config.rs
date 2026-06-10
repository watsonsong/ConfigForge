// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, de::DeserializeOwned};

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::CF_GENERATOR_INVALID_OPTION,
};

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
                data_generator_option_error(
                    label,
                    format!("invalid {label} options: {error}"),
                    value.to_string(),
                    format!("valid {label} options"),
                    "Check the nested data generator option names, types, and values.",
                )
            });
        }
        if self.options.is_empty() {
            return Ok(T::default());
        }
        let value = toml::Value::Table(self.options.clone().into_iter().collect());
        value.clone().try_into().map_err(|error| {
            data_generator_option_error(
                label,
                format!("invalid {label} options: {error}"),
                value.to_string(),
                format!("valid {label} options"),
                "Check the data generator option names, types, and values.",
            )
        })
    }
}

fn data_generator_option_error(
    label: &str,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_INVALID_OPTION,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(format!("generator.{label}"))
        .with_domain("generator")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
}

fn default_data_format() -> String {
    "json".to_owned()
}
