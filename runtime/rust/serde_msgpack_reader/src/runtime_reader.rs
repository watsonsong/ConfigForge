// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::RefCell,
    fs,
    path::{Component, Path, PathBuf},
    rc::Rc,
};

use serde_json::Value;

pub use config_forge_runtime_reader_core::SUPPORTED_CONFIG_FORGE_READER_CONTRACT;
use config_forge_runtime_reader_core::{
    RuntimeDataError, RuntimeDataLoader, RuntimeDataReader, RuntimeDataResult,
};

#[derive(Debug)]
pub struct MessagePackValueReader {
    value: Rc<Value>,
    path: Rc<RefCell<Vec<MessagePackPathSegment>>>,
}

#[derive(Debug)]
enum MessagePackPathSegment {
    Field(&'static str),
    Index(usize),
}

impl MessagePackValueReader {
    pub fn new(value: Value) -> Self {
        Self {
            value: Rc::new(value),
            path: Rc::new(RefCell::new(Vec::with_capacity(8))),
        }
    }

    fn value_at<'a>(value: &'a Value, path: &[MessagePackPathSegment]) -> Option<&'a Value> {
        let mut current = value;
        for segment in path {
            current = match segment {
                MessagePackPathSegment::Field(name) => current.get(name)?,
                MessagePackPathSegment::Index(index) => current.as_array()?.get(*index)?,
            };
        }
        Some(current)
    }

    fn with_current<T>(&self, read: impl FnOnce(&Value) -> RuntimeDataResult<T>) -> RuntimeDataResult<T> {
        let path = self.path.borrow();
        let value = Self::value_at(&self.value, &path)
            .ok_or_else(|| RuntimeDataError::decode("runtime reader cursor is invalid"))?;
        read(value)
    }
}

impl RuntimeDataReader for MessagePackValueReader {
    fn enter_field(&self, name: &'static str) -> RuntimeDataResult<bool> {
        let found = {
            let path = self.path.borrow();
            Self::value_at(&self.value, &path)
                .and_then(|value| value.get(name))
                .is_some_and(|value| !value.is_null())
        };
        if found {
            self.path
                .borrow_mut()
                .push(MessagePackPathSegment::Field(name));
        }
        Ok(found)
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
            return Err(RuntimeDataError::not_found(format!("missing array index {index}")));
        }
        self.path
            .borrow_mut()
            .push(MessagePackPathSegment::Index(index));
        Ok(())
    }

    fn exit(&self) -> RuntimeDataResult<()> {
        self.path
            .borrow_mut()
            .pop()
            .map(|_| ())
            .ok_or_else(|| RuntimeDataError::decode("runtime reader cursor stack underflow"))
    }

    fn len(&self) -> RuntimeDataResult<usize> {
        self.with_current(|value| {
            value
                .as_array()
                .map(Vec::len)
                .or_else(|| value.as_object().map(|object| object.len()))
                .ok_or_else(|| RuntimeDataError::type_mismatch("reader value is not array/object"))
        })
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
        let parsed = self.with_current(|value| {
            match value {
                Value::Number(number) => number
                    .as_f64()
                    .ok_or_else(|| RuntimeDataError::type_mismatch("number is not f64")),
                Value::String(text) => text
                    .parse()
                    .map_err(|error| RuntimeDataError::decode(format!("invalid f64: {error}"))),
                other => Err(RuntimeDataError::type_mismatch(format!(
                    "expected f64-like value, got {other:?}"
                ))),
            }
        })?;
        if parsed.is_finite() {
            Ok(parsed)
        } else {
            Err(RuntimeDataError::type_mismatch("expected finite f64-like value"))
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
            Value::String(text) => Ok(text.clone()),
            other => Err(RuntimeDataError::type_mismatch(format!(
                "expected string value, got {other:?}"
            ))),
        })
    }

}

#[derive(Debug, Clone)]
pub struct FsMessagePackRuntimeDataLoader {
    data_dir: PathBuf,
}

impl FsMessagePackRuntimeDataLoader {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn open_reader(&self, path: &str) -> RuntimeDataResult<MessagePackValueReader> {
        let full_path = safe_join(&self.data_dir, &with_extension(path, "msgpack"))?;
        let bytes = fs::read(&full_path)
            .map_err(|error| RuntimeDataError::io(full_path.display().to_string(), error))?;
        let value = rmp_serde::from_slice(&bytes)
            .map_err(|error| RuntimeDataError::decode(error.to_string()))?;
        Ok(MessagePackValueReader::new(value))
    }
}

impl RuntimeDataLoader for FsMessagePackRuntimeDataLoader {
    type Reader = MessagePackValueReader;

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

fn with_extension(logical_path: &str, extension: &str) -> String {
    if Path::new(logical_path).extension().is_some() {
        logical_path.to_owned()
    } else {
        format!("{logical_path}.{extension}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, process};

    #[test]
    fn fs_messagepack_loader_opens_payload() {
        let dir = env::temp_dir().join(format!(
            "config_forge_msgpack_loader_test_{}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp runtime data dir should be created");

        let payload = serde_json::json!({ "rows": [{ "id": "7" }] });
        fs::write(
            dir.join("items.msgpack"),
            rmp_serde::to_vec_named(&payload).unwrap(),
        )
        .unwrap();

        let reader = FsMessagePackRuntimeDataLoader::new(&dir)
            .open("items.msgpack")
            .unwrap();
        assert!(reader.enter_field("rows").unwrap());
        reader.enter_index(0).unwrap();
        assert!(reader.enter_field("id").unwrap());
        assert_eq!(reader.as_i64().unwrap(), 7);
        reader.exit().unwrap();
        reader.exit().unwrap();
        reader.exit().unwrap();

        fs::remove_dir_all(&dir).expect("temp runtime data dir should be removed");
    }

    #[test]
    fn fs_messagepack_loader_rejects_path_traversal() {
        let loader = FsMessagePackRuntimeDataLoader::new("/tmp/config_forge_runtime_root");
        assert!(loader
            .open_reader("../secret.msgpack")
            .unwrap_err()
            .message
            .contains("escapes"));
        assert!(loader
            .open_reader("/tmp/secret.msgpack")
            .unwrap_err()
            .message
            .contains("relative"));
    }

    #[test]
    fn messagepack_reader_accepts_string_encoded_scalars() {
        let reader = MessagePackValueReader::new(serde_json::json!({
            "rows": [{ "id": "7", "enabled": "1" }]
        }));

        assert!(reader.enter_field("rows").unwrap());
        reader.enter_index(0).unwrap();
        assert!(reader.enter_field("id").unwrap());
        assert_eq!(reader.as_i64().unwrap(), 7);
        reader.exit().unwrap();
        assert!(reader.enter_field("enabled").unwrap());
        assert!(reader.as_bool().unwrap());
        reader.exit().unwrap();
        reader.exit().unwrap();
        reader.exit().unwrap();
    }
}
