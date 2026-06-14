// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::{CF_GENERATOR_INVALID_OPTION, CF_GENERATOR_UNSUPPORTED_OPTION},
};

pub(crate) struct GeneratorOptionParser {
    options: BTreeMap<String, toml::Value>,
    source: &'static str,
    target: Option<String>,
}

impl GeneratorOptionParser {
    pub(crate) fn new(options: BTreeMap<String, toml::Value>, source: &'static str) -> Self {
        Self {
            options,
            source,
            target: None,
        }
    }

    pub(crate) fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub(crate) fn take_string(
        &mut self,
        key: &str,
        message_prefix: &str,
    ) -> Result<Option<String>> {
        self.take_value_with(
            key,
            |key, source| format!("invalid {message_prefix} option `{key}`: {source}"),
            |key, _| key.to_owned(),
            "string value".to_owned(),
        )
    }

    pub(crate) fn take_bool(&mut self, key: &str, message_prefix: &str) -> Result<Option<bool>> {
        self.take_value_with(
            key,
            |key, source| format!("invalid {message_prefix} option `{key}`: {source}"),
            |key, _| key.to_owned(),
            "boolean value".to_owned(),
        )
    }

    pub(crate) fn take_value_with<T>(
        &mut self,
        key: &str,
        message: impl FnOnce(&str, toml::de::Error) -> String,
        current: impl FnOnce(&str, &toml::Value) -> String,
        expected: impl Into<String>,
    ) -> Result<Option<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        self.options
            .remove(key)
            .map(|value| {
                let current = current(key, &value);
                value.try_into().map_err(|source| {
                    self.option_error(
                        CF_GENERATOR_INVALID_OPTION,
                        message(key, source),
                        current,
                        expected.into(),
                    )
                })
            })
            .transpose()
    }

    pub(crate) fn reject_unknown(
        &self,
        message: impl FnOnce(&str) -> String,
        expected: impl Into<String>,
        hint: Option<String>,
    ) -> Result<()> {
        if let Some(key) = self.options.keys().next() {
            let mut diagnostic = Diagnostic::new(
                CF_GENERATOR_UNSUPPORTED_OPTION,
                DiagnosticSeverity::Error,
                message(key),
                DiagnosticLocation::default(),
            )
            .with_source(self.source)
            .with_current_value(key)
            .with_expected(expected);
            if let Some(target) = &self.target {
                diagnostic = diagnostic.with_target(target.clone());
            }
            if let Some(hint) = hint {
                diagnostic = diagnostic.with_hint(hint);
            }
            return Err(ConfigForgeError::from_diagnostic(diagnostic));
        }
        Ok(())
    }

    fn option_error(
        &self,
        code: &'static str,
        message: String,
        current: String,
        expected: String,
    ) -> ConfigForgeError {
        let mut diagnostic = Diagnostic::new(
            code,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(self.source)
        .with_current_value(current)
        .with_expected(expected);
        if let Some(target) = &self.target {
            diagnostic = diagnostic.with_target(target.clone());
        }
        ConfigForgeError::from_diagnostic(diagnostic)
    }
}
