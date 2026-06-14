// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use serde_json::Value;

pub use config_forge_runtime_reader_core::SUPPORTED_CONFIG_FORGE_READER_CONTRACT;
use config_forge_runtime_reader_core::{
    RuntimeDataError, RuntimeDataLoader, RuntimeDataReader, RuntimeDataResult, RuntimeDecodeLimits,
    read_runtime_file,
};

#[derive(Debug)]
pub struct JsonValueReader {
    value: Rc<Value>,
    path: Rc<RefCell<Vec<JsonPathSegment>>>,
    limits: RuntimeDecodeLimits,
}

#[derive(Debug)]
enum JsonPathSegment {
    Field(&'static str),
    Index(usize),
}

impl JsonValueReader {
    pub fn new(value: Value) -> Self {
        Self::with_limits(value, RuntimeDecodeLimits::default())
    }

    pub fn with_limits(value: Value, limits: RuntimeDecodeLimits) -> Self {
        Self {
            value: Rc::new(value),
            path: Rc::new(RefCell::new(Vec::with_capacity(8))),
            limits,
        }
    }

    fn value_at<'a>(value: &'a Value, path: &[JsonPathSegment]) -> Option<&'a Value> {
        let mut current = value;
        for segment in path {
            current = match segment {
                JsonPathSegment::Field(name) => current.get(name)?,
                JsonPathSegment::Index(index) => current.as_array()?.get(*index)?,
            };
        }
        Some(current)
    }

    fn with_current<T>(
        &self,
        read: impl FnOnce(&Value) -> RuntimeDataResult<T>,
    ) -> RuntimeDataResult<T> {
        let path = self.path.borrow();
        let value = Self::value_at(&self.value, &path)
            .ok_or_else(|| RuntimeDataError::decode("runtime reader cursor is invalid"))?;
        read(value)
    }

    pub fn enter_field(&self, name: &'static str) -> RuntimeDataResult<bool> {
        let found = {
            let path = self.path.borrow();
            Self::value_at(&self.value, &path)
                .and_then(|value| value.get(name))
                .is_some_and(|value| !value.is_null())
        };
        if found {
            let mut path = self.path.borrow_mut();
            self.limits.check_nesting_depth(path.len() + 1)?;
            path.push(JsonPathSegment::Field(name));
        }
        Ok(found)
    }

    pub fn len(&self) -> RuntimeDataResult<usize> {
        self.with_current(|value| {
            value
                .as_array()
                .map(Vec::len)
                .or_else(|| value.as_object().map(|object| object.len()))
                .ok_or_else(|| RuntimeDataError::type_mismatch("reader value is not array/object"))
        })
    }
}

impl RuntimeDataReader for JsonValueReader {
    fn enter_required_field(&self, name: &'static str) -> RuntimeDataResult<()> {
        if !self.enter_field(name)? {
            return Err(RuntimeDataError::not_found(format!(
                "missing required field `{name}`"
            )));
        }
        Ok(())
    }

    fn enter_optional_field(&self, name: &'static str) -> RuntimeDataResult<bool> {
        self.enter_field(name)
    }

    fn exit_field(&self) -> RuntimeDataResult<()> {
        self.exit()
    }

    fn enter_index(&self, index: usize) -> RuntimeDataResult<()> {
        let found = {
            let path = self.path.borrow();
            Self::value_at(&self.value, &path)
                .and_then(Value::as_array)
                .and_then(|values| values.get(index))
                .is_some()
        };
        if !found {
            return Err(RuntimeDataError::not_found(format!(
                "missing array index {index}"
            )));
        }
        let mut path = self.path.borrow_mut();
        self.limits.check_nesting_depth(path.len() + 1)?;
        path.push(JsonPathSegment::Index(index));
        Ok(())
    }

    fn exit(&self) -> RuntimeDataResult<()> {
        self.path
            .borrow_mut()
            .pop()
            .map(|_| ())
            .ok_or_else(|| RuntimeDataError::decode("runtime reader cursor stack underflow"))
    }

    fn read_row_count(&self) -> RuntimeDataResult<usize> {
        let rows = self.len()?;
        self.limits.check_rows(rows)?;
        Ok(rows)
    }

    fn read_list_len(&self) -> RuntimeDataResult<usize> {
        let length = self.len()?;
        self.limits.check_list_length(length)?;
        Ok(length)
    }

    fn array_elements_are_positional(&self) -> bool {
        false
    }

    fn begin_record(&self, _optional_slot_count: usize) -> RuntimeDataResult<()> {
        Ok(())
    }

    fn end_record(&self) -> RuntimeDataResult<()> {
        Ok(())
    }

    fn read_enum_index(&self, variants: &[&str]) -> RuntimeDataResult<usize> {
        let value = self.as_string()?;
        variants
            .iter()
            .position(|variant| *variant == value.as_str())
            .ok_or_else(|| RuntimeDataError::decode(format!("unknown enum value `{value}`")))
    }

    fn as_i64(&self) -> RuntimeDataResult<i64> {
        self.with_current(|value| match value {
            Value::Number(number) => number
                .as_i64()
                .ok_or_else(|| RuntimeDataError::type_mismatch("number is not i64")),
            Value::String(text) => text
                .parse()
                .map_err(|error| RuntimeDataError::decode(format!("invalid i64: {error}"))),
            other => Err(RuntimeDataError::type_mismatch(format!(
                "expected i64-like value, got {other:?}"
            ))),
        })
    }

    fn as_f64(&self) -> RuntimeDataResult<f64> {
        let parsed = self.with_current(|value| match value {
            Value::Number(number) => number
                .as_f64()
                .ok_or_else(|| RuntimeDataError::type_mismatch("number is not f64")),
            Value::String(text) => text
                .parse()
                .map_err(|error| RuntimeDataError::decode(format!("invalid f64: {error}"))),
            other => Err(RuntimeDataError::type_mismatch(format!(
                "expected f64-like value, got {other:?}"
            ))),
        })?;
        if parsed.is_finite() {
            Ok(parsed)
        } else {
            Err(RuntimeDataError::type_mismatch(
                "expected finite f64-like value",
            ))
        }
    }

    fn as_bool(&self) -> RuntimeDataResult<bool> {
        self.with_current(|value| match value {
            Value::Bool(value) => Ok(*value),
            Value::String(text) => match text.as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(RuntimeDataError::decode(format!("invalid bool: {text}"))),
            },
            other => Err(RuntimeDataError::type_mismatch(format!(
                "expected bool-like value, got {other:?}"
            ))),
        })
    }

    fn as_string(&self) -> RuntimeDataResult<String> {
        self.with_current(|value| match value {
            Value::String(text) => {
                self.limits.check_string_bytes(text.len())?;
                Ok(text.clone())
            }
            other => Err(RuntimeDataError::type_mismatch(format!(
                "expected string value, got {other:?}"
            ))),
        })
    }

    fn schema_revision(&self) -> Option<u64> {
        None
    }

    fn ensure_exhausted(&self) -> RuntimeDataResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FsJsonRuntimeDataLoader {
    data_dir: PathBuf,
    limits: RuntimeDecodeLimits,
}

impl FsJsonRuntimeDataLoader {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            limits: RuntimeDecodeLimits::default(),
        }
    }

    pub fn with_limits(data_dir: impl Into<PathBuf>, limits: RuntimeDecodeLimits) -> Self {
        Self {
            data_dir: data_dir.into(),
            limits,
        }
    }

    pub fn open_reader(&self, path: &str) -> RuntimeDataResult<JsonValueReader> {
        let bytes = read_runtime_file(&self.data_dir, path, "json", self.limits)?;
        let value = serde_json::from_slice(&bytes)
            .map_err(|error| RuntimeDataError::decode(error.to_string()))?;
        Ok(JsonValueReader::with_limits(value, self.limits))
    }
}

impl RuntimeDataLoader for FsJsonRuntimeDataLoader {
    type Reader = JsonValueReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader> {
        self.open_reader(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, process};

    #[test]
    fn fs_json_loader_opens_payload() {
        let dir = env::temp_dir().join(format!("config_forge_json_loader_test_{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp runtime data dir should be created");

        let payload = serde_json::json!({ "rows": [{ "id": "7" }] });
        fs::write(
            dir.join("items.json"),
            serde_json::to_vec(&payload).unwrap(),
        )
        .unwrap();

        let reader = FsJsonRuntimeDataLoader::new(&dir)
            .open("items.json")
            .unwrap();
        assert!(reader.enter_field("rows").unwrap());
        reader.enter_index(0).unwrap();
        assert!(reader.enter_field("id").unwrap());
        assert_eq!(reader.as_i64().unwrap(), 7);
        reader.exit().unwrap();
        reader.exit().unwrap();
        reader.exit().unwrap();

        let reader = FsJsonRuntimeDataLoader::new(&dir)
            .open("nested/../items")
            .unwrap();
        assert!(reader.enter_field("rows").unwrap());

        fs::remove_dir_all(&dir).expect("temp runtime data dir should be removed");
    }

    #[test]
    fn fs_json_loader_rejects_path_traversal() {
        let loader = FsJsonRuntimeDataLoader::new("/tmp/config_forge_runtime_root");
        assert!(
            loader
                .open_reader("../secret.json")
                .unwrap_err()
                .message
                .contains("escapes")
        );
        assert!(
            loader
                .open_reader("/tmp/secret.json")
                .unwrap_err()
                .message
                .contains("relative")
        );
    }

    #[test]
    fn json_reader_accepts_scalar_conversions() {
        let reader = JsonValueReader::new(serde_json::json!({
            "rows": [{ "id": "1", "ratio": "1.5", "enabled": "true", "name": "Potion" }]
        }));

        assert!(reader.enter_field("rows").unwrap());
        assert_eq!(reader.len().unwrap(), 1);
        reader.enter_index(0).unwrap();
        assert!(reader.enter_field("id").unwrap());
        assert_eq!(reader.as_i64().unwrap(), 1);
        reader.exit().unwrap();
        assert!(reader.enter_field("ratio").unwrap());
        assert_eq!(reader.as_f64().unwrap(), 1.5);
        reader.exit().unwrap();
        assert!(reader.enter_field("enabled").unwrap());
        assert!(reader.as_bool().unwrap());
        reader.exit().unwrap();
        assert!(reader.enter_field("name").unwrap());
        assert_eq!(reader.as_string().unwrap(), "Potion");
        reader.exit().unwrap();
        assert!(!reader.enter_field("missing").unwrap());
        reader.exit().unwrap();
        reader.exit().unwrap();
    }

    #[test]
    fn json_reader_rejects_non_finite_number_strings() {
        let reader = JsonValueReader::new(Value::String("NaN".to_owned()));
        assert!(reader.as_f64().unwrap_err().message.contains("finite"));
    }

    #[test]
    fn json_reader_enforces_decode_limits() {
        let reader = JsonValueReader::with_limits(
            serde_json::json!([{}, {}]),
            RuntimeDecodeLimits {
                max_rows: 1,
                ..RuntimeDecodeLimits::default()
            },
        );
        assert!(
            reader
                .read_row_count()
                .unwrap_err()
                .message
                .contains("row count")
        );

        let reader = JsonValueReader::with_limits(
            serde_json::json!([1, 2]),
            RuntimeDecodeLimits {
                max_array_length: 1,
                ..RuntimeDecodeLimits::default()
            },
        );
        assert!(
            reader
                .read_list_len()
                .unwrap_err()
                .message
                .contains("list length")
        );

        let reader = JsonValueReader::with_limits(
            Value::String("abcd".to_owned()),
            RuntimeDecodeLimits {
                max_string_bytes: 3,
                ..RuntimeDecodeLimits::default()
            },
        );
        assert!(
            reader
                .as_string()
                .unwrap_err()
                .message
                .contains("string byte length")
        );

        let reader = JsonValueReader::with_limits(
            serde_json::json!({ "a": { "b": 1 } }),
            RuntimeDecodeLimits {
                max_nesting_depth: 1,
                ..RuntimeDecodeLimits::default()
            },
        );
        assert!(reader.enter_field("a").unwrap());
        assert!(
            reader
                .enter_field("b")
                .unwrap_err()
                .message
                .contains("nesting depth")
        );
    }

    #[test]
    fn fs_json_loader_rejects_oversized_file_before_parsing_payload() {
        let dir = env::temp_dir().join(format!(
            "config_forge_json_limit_loader_test_{}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp runtime data dir should be created");

        fs::write(dir.join("items.json"), b"[]").unwrap();
        let loader = FsJsonRuntimeDataLoader::with_limits(
            &dir,
            RuntimeDecodeLimits {
                max_file_bytes: 1,
                ..RuntimeDecodeLimits::default()
            },
        );
        assert!(
            loader
                .open_reader("items.json")
                .unwrap_err()
                .message
                .contains("runtime file bytes")
        );

        fs::remove_dir_all(&dir).expect("temp runtime data dir should be removed");
    }
}
