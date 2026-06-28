// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Serialize;

use crate::{Result, runtime_contract::RuntimeDataFile};

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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    runtime_readers: Vec<RuntimeReaderAdapterReference>,
    #[serde(flatten)]
    language_fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RuntimeReaderAdapterReference {
    format: &'static str,
    adapter: &'static str,
    parser_backend: &'static str,
    parser_backend_version: &'static str,
    adapter_support_level: &'static str,
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
            runtime_readers: Vec::new(),
            language_fields: BTreeMap::new(),
        }
    }

    pub(crate) fn with_runtime_readers(
        mut self,
        readers: impl IntoIterator<Item = RuntimeReaderAdapterReference>,
    ) -> Self {
        self.runtime_readers.extend(readers);
        self
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

pub(crate) fn recommended_runtime_reader_adapters(
    language: &str,
    runtime_data_files: &[RuntimeDataFile],
) -> Vec<RuntimeReaderAdapterReference> {
    let mut formats = BTreeSet::new();
    for file in runtime_data_files {
        formats.insert(canonical_runtime_format(&file.format));
    }
    formats
        .into_iter()
        .filter_map(|format| recommended_runtime_reader_adapter(language, format))
        .collect()
}

fn canonical_runtime_format(format: &str) -> &str {
    match format {
        "bin" | "binary" => "binary",
        "msgpack" | "messagepack" => "messagepack",
        "json" => "json",
        other => other,
    }
}

fn recommended_runtime_reader_adapter(
    language: &str,
    format: &str,
) -> Option<RuntimeReaderAdapterReference> {
    match (language, format) {
        ("rust", "json") => Some(RuntimeReaderAdapterReference {
            format: "json",
            adapter: "runtime/rust/serde_json_reader",
            parser_backend: "serde_json",
            parser_backend_version: "1.0",
            adapter_support_level: "stable",
        }),
        ("rust", "messagepack") => Some(RuntimeReaderAdapterReference {
            format: "messagepack",
            adapter: "runtime/rust/serde_msgpack_reader",
            parser_backend: "rmp-serde",
            parser_backend_version: "1.3",
            adapter_support_level: "stable",
        }),
        ("rust", "binary") => Some(RuntimeReaderAdapterReference {
            format: "binary",
            adapter: "runtime/rust/binary_reader",
            parser_backend: "configforge-binary",
            parser_backend_version: "0.1",
            adapter_support_level: "stable",
        }),
        ("cpp", "json") => Some(RuntimeReaderAdapterReference {
            format: "json",
            adapter: "runtime/cpp/rapidjson_reader",
            parser_backend: "RapidJSON",
            parser_backend_version: "1.1",
            adapter_support_level: "stable",
        }),
        ("cpp", "messagepack") => Some(RuntimeReaderAdapterReference {
            format: "messagepack",
            adapter: "runtime/cpp/msgpackc_reader",
            parser_backend: "msgpack-cxx",
            parser_backend_version: "cpp-6.1.1",
            adapter_support_level: "stable",
        }),
        ("cpp", "binary") => Some(RuntimeReaderAdapterReference {
            format: "binary",
            adapter: "runtime/cpp/binary_reader",
            parser_backend: "configforge-binary",
            parser_backend_version: "0.1",
            adapter_support_level: "stable",
        }),
        _ => None,
    }
}
