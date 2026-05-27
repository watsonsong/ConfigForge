// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use serde_json::Value;

pub use config_forge_runtime_reader_core::SUPPORTED_CONFIG_FORGE_READER_CONTRACT;
use config_forge_runtime_reader_core::{
    RuntimeDataError, RuntimeDataLoader, RuntimeDataReader, RuntimeDataResult,
};

#[derive(Debug, Clone)]
pub struct JsonValueReader {
    value: Value,
}

impl JsonValueReader {
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

impl RuntimeDataReader for JsonValueReader {
    fn field(&self, name: &str) -> RuntimeDataResult<Option<Self>> {
        Ok(self.value.get(name).cloned().map(Self::new))
    }

    fn index(&self, index: usize) -> RuntimeDataResult<Self> {
        self.value
            .as_array()
            .and_then(|values| values.get(index))
            .cloned()
            .map(Self::new)
            .ok_or_else(|| RuntimeDataError::not_found(format!("missing array index {index}")))
    }

    fn len(&self) -> RuntimeDataResult<usize> {
        self.value
            .as_array()
            .map(Vec::len)
            .or_else(|| self.value.as_object().map(|object| object.len()))
            .ok_or_else(|| RuntimeDataError::type_mismatch("reader value is not array/object"))
    }

    fn as_i64(&self) -> RuntimeDataResult<i64> {
        match &self.value {
            Value::Number(number) => number
                .as_i64()
                .ok_or_else(|| RuntimeDataError::type_mismatch("number is not i64")),
            Value::String(text) => text
                .parse()
                .map_err(|error| RuntimeDataError::decode(format!("invalid i64: {error}"))),
            other => Err(RuntimeDataError::type_mismatch(format!(
                "expected i64-like value, got {other:?}"
            ))),
        }
    }

    fn as_f64(&self) -> RuntimeDataResult<f64> {
        let parsed = match &self.value {
            Value::Number(number) => number
                .as_f64()
                .ok_or_else(|| RuntimeDataError::type_mismatch("number is not f64"))?,
            Value::String(text) => text
                .parse()
                .map_err(|error| RuntimeDataError::decode(format!("invalid f64: {error}")))?,
            other => {
                return Err(RuntimeDataError::type_mismatch(format!(
                    "expected f64-like value, got {other:?}"
                )));
            }
        };
        if parsed.is_finite() {
            Ok(parsed)
        } else {
            Err(RuntimeDataError::type_mismatch(format!(
                "expected finite f64-like value, got {:?}",
                self.value
            )))
        }
    }

    fn as_bool(&self) -> RuntimeDataResult<bool> {
        match &self.value {
            Value::Bool(value) => Ok(*value),
            Value::String(text) => match text.as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(RuntimeDataError::decode(format!("invalid bool: {text}"))),
            },
            other => Err(RuntimeDataError::type_mismatch(format!(
                "expected bool-like value, got {other:?}"
            ))),
        }
    }

    fn as_string(&self) -> RuntimeDataResult<String> {
        match &self.value {
            Value::String(text) => Ok(text.clone()),
            other => Err(RuntimeDataError::type_mismatch(format!(
                "expected string value, got {other:?}"
            ))),
        }
    }

    fn is_null(&self) -> bool {
        self.value.is_null()
    }
}

#[derive(Debug, Clone)]
pub struct FsJsonRuntimeDataLoader {
    data_dir: PathBuf,
}

impl FsJsonRuntimeDataLoader {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn open_reader(&self, path: &str) -> RuntimeDataResult<JsonValueReader> {
        let full_path = safe_join(&self.data_dir, path)?;
        let bytes = fs::read(&full_path)
            .map_err(|error| RuntimeDataError::io(full_path.display().to_string(), error))?;
        let value = serde_json::from_slice(&bytes)
            .map_err(|error| RuntimeDataError::decode(error.to_string()))?;
        Ok(JsonValueReader::new(value))
    }
}

impl RuntimeDataLoader for FsJsonRuntimeDataLoader {
    type Reader = JsonValueReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader> {
        self.open_reader(path)
    }
}

fn safe_join(root: &Path, logical_path: &str) -> RuntimeDataResult<PathBuf> {
    let path = Path::new(logical_path);
    if path.is_absolute() {
        return Err(RuntimeDataError::invalid_path(format!(
            "runtime data path must be relative: {logical_path}"
        )));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(RuntimeDataError::invalid_path(format!(
                        "runtime data path escapes data root: {logical_path}"
                    )));
                }
            }
            _ => {
                return Err(RuntimeDataError::invalid_path(format!(
                    "unsupported runtime data path: {logical_path}"
                )));
            }
        }
    }

    Ok(root.join(normalized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, process};

    #[test]
    fn fs_json_loader_opens_payload() {
        let dir = env::temp_dir().join(format!(
            "config_forge_json_loader_test_{}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp runtime data dir should be created");

        let payload = serde_json::json!({ "rows": [{ "id": "7" }] });
        fs::write(dir.join("items.json"), serde_json::to_vec(&payload).unwrap()).unwrap();

        let reader = FsJsonRuntimeDataLoader::new(&dir).open("items.json").unwrap();
        let row = reader
            .field("rows")
            .unwrap()
            .expect("rows")
            .index(0)
            .unwrap();
        assert_eq!(row.field("id").unwrap().unwrap().as_i64().unwrap(), 7);

        fs::remove_dir_all(&dir).expect("temp runtime data dir should be removed");
    }

    #[test]
    fn fs_json_loader_rejects_path_traversal() {
        let loader = FsJsonRuntimeDataLoader::new("/tmp/config_forge_runtime_root");
        assert!(loader
            .open_reader("../secret.json")
            .unwrap_err()
            .message
            .contains("escapes"));
        assert!(loader
            .open_reader("/tmp/secret.json")
            .unwrap_err()
            .message
            .contains("relative"));
    }

    #[test]
    fn json_reader_accepts_scalar_conversions() {
        let reader = JsonValueReader::new(serde_json::json!({
            "rows": [{ "id": "1", "ratio": "1.5", "enabled": "true", "name": "Potion" }]
        }));

        let rows = reader.field("rows").unwrap().expect("rows field");
        assert_eq!(rows.len().unwrap(), 1);
        let row = rows.index(0).unwrap();
        assert_eq!(row.field("id").unwrap().unwrap().as_i64().unwrap(), 1);
        assert_eq!(row.field("ratio").unwrap().unwrap().as_f64().unwrap(), 1.5);
        assert!(row.field("enabled").unwrap().unwrap().as_bool().unwrap());
        assert_eq!(
            row.field("name").unwrap().unwrap().as_string().unwrap(),
            "Potion"
        );
        assert!(row.field("missing").unwrap().is_none());
    }

    #[test]
    fn json_reader_rejects_non_finite_number_strings() {
        let reader = JsonValueReader::new(Value::String("NaN".to_owned()));
        assert!(reader.as_f64().unwrap_err().message.contains("finite"));
    }
}
