// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};

use serde::Serialize;

use crate::Result;

pub(crate) const RUNTIME_INTERFACE_MANIFEST_FILE_NAME: &str = "config_forge_runtime_interface.json";
pub(crate) const RUNTIME_READER_CONTRACT: &str = "config_forge_runtime_reader";
pub(crate) const RUNTIME_READER_CONTRACT_VERSION: &str = "1.0";
pub(crate) const REQUIRED_RUNTIME_READER_CONTRACT: &str = "^1.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RuntimeInterfaceManifest {
    contract: &'static str,
    contract_version: &'static str,
    required_contract: &'static str,
    language: String,
    emit: &'static str,
    runtime_interface: &'static str,
    #[serde(flatten)]
    language_fields: BTreeMap<String, String>,
}

impl RuntimeInterfaceManifest {
    pub(crate) fn reference(language: impl Into<String>) -> Self {
        Self {
            contract: RUNTIME_READER_CONTRACT,
            contract_version: RUNTIME_READER_CONTRACT_VERSION,
            required_contract: REQUIRED_RUNTIME_READER_CONTRACT,
            language: language.into(),
            emit: "reference",
            runtime_interface: "external",
            language_fields: BTreeMap::new(),
        }
    }

    pub(crate) fn with_language_field(mut self, key: &str, value: impl Into<String>) -> Self {
        self.language_fields.insert(key.to_owned(), value.into());
        self
    }

    pub(crate) fn to_json_pretty(&self) -> Result<String> {
        let mut content = serde_json::to_string_pretty(self)?;
        content.push('\n');
        Ok(content)
    }
}

pub(crate) fn runtime_interface_manifest_path(out_dir: &Path) -> std::path::PathBuf {
    out_dir.join(RUNTIME_INTERFACE_MANIFEST_FILE_NAME)
}
