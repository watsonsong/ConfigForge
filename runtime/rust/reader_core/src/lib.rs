// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub const CONFIG_FORGE_READER_CONTRACT_VERSION: &str = "1.0";
pub const SUPPORTED_CONFIG_FORGE_READER_CONTRACT: &str = "^1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDecodeLimits {
    pub max_file_bytes: usize,
    pub max_rows: usize,
    pub max_array_length: usize,
    pub max_string_bytes: usize,
    pub max_nesting_depth: usize,
}

impl RuntimeDecodeLimits {
    pub const DEFAULT_MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
    pub const DEFAULT_MAX_ROWS: usize = 1_000_000;
    pub const DEFAULT_MAX_ARRAY_LENGTH: usize = 1_000_000;
    pub const DEFAULT_MAX_STRING_BYTES: usize = 16 * 1024 * 1024;
    pub const DEFAULT_MAX_NESTING_DEPTH: usize = 128;

    pub const fn default_limits() -> Self {
        Self {
            max_file_bytes: Self::DEFAULT_MAX_FILE_BYTES,
            max_rows: Self::DEFAULT_MAX_ROWS,
            max_array_length: Self::DEFAULT_MAX_ARRAY_LENGTH,
            max_string_bytes: Self::DEFAULT_MAX_STRING_BYTES,
            max_nesting_depth: Self::DEFAULT_MAX_NESTING_DEPTH,
        }
    }

    pub fn check_file_bytes(&self, bytes: usize) -> RuntimeDataResult<()> {
        check_limit("runtime file bytes", bytes, self.max_file_bytes)
    }

    pub fn check_rows(&self, rows: usize) -> RuntimeDataResult<()> {
        check_limit("row count", rows, self.max_rows)
    }

    pub fn check_list_length(&self, length: usize) -> RuntimeDataResult<()> {
        check_limit("list length", length, self.max_array_length)
    }
    pub fn check_array_length(&self, length: usize) -> RuntimeDataResult<()> {
        self.check_list_length(length)
    }

    pub fn check_string_bytes(&self, bytes: usize) -> RuntimeDataResult<()> {
        check_limit("string byte length", bytes, self.max_string_bytes)
    }

    pub fn check_nesting_depth(&self, depth: usize) -> RuntimeDataResult<()> {
        check_limit("nesting depth", depth, self.max_nesting_depth)
    }
}

impl Default for RuntimeDecodeLimits {
    fn default() -> Self {
        Self::default_limits()
    }
}

fn check_limit(label: &str, actual: usize, limit: usize) -> RuntimeDataResult<()> {
    if actual <= limit {
        return Ok(());
    }
    Err(RuntimeDataError::decode(format!(
        "{label} `{actual}` exceeds decode limit `{limit}`"
    )))
}

pub type RuntimeDataResult<T> = Result<T, RuntimeDataError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeDataErrorKind {
    Io,
    Decode,
    SchemaMismatch,
    TypeMismatch,
    NotFound,
    Unsupported,
    InvalidPath,
}

impl RuntimeDataErrorKind {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io => "CF_RT_IO",
            Self::Decode => "CF_RT_DECODE",
            Self::SchemaMismatch => "CF_RT_SCHEMA_MISMATCH",
            Self::TypeMismatch => "CF_RT_TYPE_MISMATCH",
            Self::NotFound => "CF_RT_NOT_FOUND",
            Self::Unsupported => "CF_RT_UNSUPPORTED",
            Self::InvalidPath => "CF_RT_INVALID_PATH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDataError {
    pub kind: RuntimeDataErrorKind,
    pub message: String,
    pub path: Option<String>,
    pub source: Option<String>,
}

impl RuntimeDataError {
    pub fn new(kind: RuntimeDataErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            path: None,
            source: None,
        }
    }

    pub fn io(path: impl Into<String>, source: impl ToString) -> Self {
        Self {
            kind: RuntimeDataErrorKind::Io,
            message: "failed to read runtime data".to_owned(),
            path: Some(path.into()),
            source: Some(source.to_string()),
        }
    }

    pub fn decode(message: impl Into<String>) -> Self {
        Self::new(RuntimeDataErrorKind::Decode, message)
    }

    pub fn schema_mismatch(message: impl Into<String>) -> Self {
        Self::new(RuntimeDataErrorKind::SchemaMismatch, message)
    }

    pub fn type_mismatch(message: impl Into<String>) -> Self {
        Self::new(RuntimeDataErrorKind::TypeMismatch, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(RuntimeDataErrorKind::NotFound, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(RuntimeDataErrorKind::Unsupported, message)
    }

    pub fn invalid_path(message: impl Into<String>) -> Self {
        Self::new(RuntimeDataErrorKind::InvalidPath, message)
    }

    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
}

impl std::fmt::Display for RuntimeDataError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)?;
        if let Some(path) = &self.path {
            write!(formatter, " (path `{path}`)")?;
        }
        if let Some(source) = &self.source {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeDataError {}

pub fn read_runtime_file(
    root: &Path,
    logical_path: &str,
    default_extension: &str,
    limits: RuntimeDecodeLimits,
) -> RuntimeDataResult<Vec<u8>> {
    let full_path = safe_join_runtime_path(
        root,
        &logical_path_with_extension(logical_path, default_extension),
    )?;
    check_runtime_file_size(&full_path, limits)?;
    let bytes = fs::read(&full_path)
        .map_err(|source| RuntimeDataError::io(full_path.display().to_string(), source))?;
    limits.check_file_bytes(bytes.len())?;
    Ok(bytes)
}

pub fn check_runtime_file_size(path: &Path, limits: RuntimeDecodeLimits) -> RuntimeDataResult<()> {
    let metadata = fs::metadata(path)
        .map_err(|source| RuntimeDataError::io(path.display().to_string(), source))?;
    let size = usize::try_from(metadata.len())
        .map_err(|_| RuntimeDataError::decode("runtime file bytes exceed usize"))?;
    limits.check_file_bytes(size)
}

pub fn safe_join_runtime_path(root: &Path, logical_path: &str) -> RuntimeDataResult<PathBuf> {
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
            Component::RootDir | Component::Prefix(_) => {
                return Err(RuntimeDataError::invalid_path(format!(
                    "unsupported runtime data path: {logical_path}"
                )));
            }
        }
    }

    Ok(root.join(normalized))
}

pub fn logical_path_with_extension(logical_path: &str, extension: &str) -> String {
    if Path::new(logical_path).extension().is_some() {
        logical_path.to_owned()
    } else {
        format!("{logical_path}.{extension}")
    }
}

impl From<String> for RuntimeDataError {
    fn from(message: String) -> Self {
        Self::decode(message)
    }
}

impl From<&str> for RuntimeDataError {
    fn from(message: &str) -> Self {
        Self::decode(message)
    }
}

pub trait RuntimeDataReader {
    fn enter_required_field(&self, name: &'static str) -> RuntimeDataResult<()>;
    fn enter_optional_field(&self, name: &'static str) -> RuntimeDataResult<bool>;
    fn exit_field(&self) -> RuntimeDataResult<()>;
    fn enter_index(&self, index: usize) -> RuntimeDataResult<()>;
    fn exit(&self) -> RuntimeDataResult<()>;
    fn read_row_count(&self) -> RuntimeDataResult<usize>;
    fn read_list_len(&self) -> RuntimeDataResult<usize>;
    fn array_elements_are_positional(&self) -> bool;
    fn begin_record(&self, optional_slot_count: usize) -> RuntimeDataResult<()>;
    fn end_record(&self) -> RuntimeDataResult<()>;
    fn read_enum_index(&self, variants: &[&str]) -> RuntimeDataResult<usize>;
    fn as_i64(&self) -> RuntimeDataResult<i64>;
    fn as_f64(&self) -> RuntimeDataResult<f64>;
    fn as_bool(&self) -> RuntimeDataResult<bool>;
    fn as_string(&self) -> RuntimeDataResult<String>;
    fn schema_revision(&self) -> Option<u64>;
    fn ensure_exhausted(&self) -> RuntimeDataResult<()>;
}

pub trait RuntimeDataLoader {
    type Reader: RuntimeDataReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::{cell::RefCell, env, fs, path::Path, process, rc::Rc};

    struct LenOnlyReader {
        len: usize,
        limits: RuntimeDecodeLimits,
    }

    impl LenOnlyReader {
        fn new(len: usize, max_rows: usize, max_array_length: usize) -> Self {
            Self {
                len,
                limits: RuntimeDecodeLimits {
                    max_rows,
                    max_array_length,
                    ..RuntimeDecodeLimits::default()
                },
            }
        }
    }

    impl RuntimeDataReader for LenOnlyReader {
        fn enter_required_field(&self, _name: &'static str) -> RuntimeDataResult<()> {
            Ok(())
        }

        fn enter_optional_field(&self, _name: &'static str) -> RuntimeDataResult<bool> {
            Ok(false)
        }

        fn exit_field(&self) -> RuntimeDataResult<()> {
            Ok(())
        }

        fn enter_index(&self, _index: usize) -> RuntimeDataResult<()> {
            Ok(())
        }

        fn exit(&self) -> RuntimeDataResult<()> {
            Ok(())
        }

        fn read_row_count(&self) -> RuntimeDataResult<usize> {
            self.limits.check_rows(self.len)?;
            Ok(self.len)
        }

        fn read_list_len(&self) -> RuntimeDataResult<usize> {
            self.limits.check_list_length(self.len)?;
            Ok(self.len)
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

        fn read_enum_index(&self, _variants: &[&str]) -> RuntimeDataResult<usize> {
            Err(RuntimeDataError::unsupported("not used in test"))
        }

        fn as_i64(&self) -> RuntimeDataResult<i64> {
            Err(RuntimeDataError::unsupported("not used in test"))
        }

        fn as_f64(&self) -> RuntimeDataResult<f64> {
            Err(RuntimeDataError::unsupported("not used in test"))
        }

        fn as_bool(&self) -> RuntimeDataResult<bool> {
            Err(RuntimeDataError::unsupported("not used in test"))
        }

        fn as_string(&self) -> RuntimeDataResult<String> {
            Err(RuntimeDataError::unsupported("not used in test"))
        }

        fn schema_revision(&self) -> Option<u64> {
            None
        }

        fn ensure_exhausted(&self) -> RuntimeDataResult<()> {
            Ok(())
        }
    }

    #[test]
    fn row_count_and_list_length_have_distinct_limits() {
        let reader = LenOnlyReader::new(3, 2, 4);

        let row_error = reader
            .read_row_count()
            .expect_err("row count should be limited");
        assert!(row_error.message.contains("row count"));
        assert_eq!(reader.read_list_len().expect("list length should pass"), 3);
    }

    #[test]
    fn runtime_path_helper_normalizes_without_escaping_root() {
        let root = Path::new("/runtime/root");

        assert_eq!(
            safe_join_runtime_path(root, "nested/../items.json").unwrap(),
            root.join("items.json")
        );
        assert!(
            safe_join_runtime_path(root, "../items.json")
                .unwrap_err()
                .message
                .contains("escapes")
        );
        assert_eq!(
            safe_join_runtime_path(root, "../items.json")
                .unwrap_err()
                .code(),
            "CF_RT_INVALID_PATH"
        );
        assert!(
            safe_join_runtime_path(root, "/tmp/items.json")
                .unwrap_err()
                .message
                .contains("relative")
        );
    }

    #[test]
    fn runtime_file_reader_applies_extension_and_size_limit() {
        let dir = env::temp_dir().join(format!(
            "config_forge_reader_core_file_test_{}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp runtime data dir should be created");
        fs::write(dir.join("items.json"), b"{}").expect("payload should be written");

        assert_eq!(
            read_runtime_file(
                &dir,
                "nested/../items",
                "json",
                RuntimeDecodeLimits::default()
            )
            .unwrap(),
            b"{}"
        );
        assert!(
            read_runtime_file(
                &dir,
                "items",
                "json",
                RuntimeDecodeLimits {
                    max_file_bytes: 1,
                    ..RuntimeDecodeLimits::default()
                }
            )
            .unwrap_err()
            .message
            .contains("runtime file bytes")
        );

        fs::remove_dir_all(&dir).expect("temp runtime data dir should be removed");
    }

    #[derive(Debug)]
    struct ComplianceJsonReader {
        value: Rc<Value>,
        path: Rc<RefCell<Vec<CompliancePathSegment>>>,
    }

    #[derive(Debug)]
    enum CompliancePathSegment {
        Field(&'static str),
        Index(usize),
    }

    impl ComplianceJsonReader {
        fn new(value: Value) -> Self {
            Self {
                value: Rc::new(value),
                path: Rc::new(RefCell::new(Vec::new())),
            }
        }

        fn value_at<'a>(value: &'a Value, path: &[CompliancePathSegment]) -> Option<&'a Value> {
            let mut current = value;
            for segment in path {
                current = match segment {
                    CompliancePathSegment::Field(name) => current.get(name)?,
                    CompliancePathSegment::Index(index) => current.as_array()?.get(*index)?,
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
                    .push(CompliancePathSegment::Field(name));
            }
            Ok(found)
        }

        fn len(&self) -> RuntimeDataResult<usize> {
            self.with_current(|value| {
                value
                    .as_array()
                    .map(Vec::len)
                    .or_else(|| value.as_object().map(|object| object.len()))
                    .ok_or_else(|| {
                        RuntimeDataError::type_mismatch("reader value is not array/object")
                    })
            })
        }
    }

    impl RuntimeDataReader for ComplianceJsonReader {
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
            self.path
                .borrow_mut()
                .push(CompliancePathSegment::Index(index));
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
            self.len()
        }

        fn read_list_len(&self) -> RuntimeDataResult<usize> {
            self.len()
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
                Value::String(text) => Ok(text.clone()),
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
    enum PathStep {
        Field(&'static str),
        Index(usize),
    }

    fn enter_path(reader: &ComplianceJsonReader, steps: &[PathStep]) -> RuntimeDataResult<usize> {
        let mut entered = 0;
        for step in steps {
            match step {
                PathStep::Field(name) => {
                    if !reader.enter_field(name)? {
                        return Err(RuntimeDataError::not_found(format!(
                            "missing field `{name}`"
                        )));
                    }
                }
                PathStep::Index(index) => reader.enter_index(*index)?,
            }
            entered += 1;
        }
        Ok(entered)
    }

    fn exit_path(reader: &ComplianceJsonReader, entered: usize) {
        for _ in 0..entered {
            reader.exit().expect("compliance path should exit cleanly");
        }
    }

    fn read_payload(name: &str) -> Value {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/runtime_reader_compliance/payloads");
        let content = fs::read_to_string(root.join(name)).expect("compliance payload should exist");
        serde_json::from_str(&content).expect("compliance payload should be valid JSON")
    }

    fn read_expected_results() -> toml::Table {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/runtime_reader_compliance/expected/scalar_results.toml");
        let content = fs::read_to_string(path).expect("scalar expected TOML should exist");
        content
            .parse::<toml::Table>()
            .expect("scalar expected TOML should parse")
    }

    fn read_error_cases() -> toml::Table {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/runtime_reader_compliance/expected/error_cases.toml");
        let content = fs::read_to_string(path).expect("error expected TOML should exist");
        content
            .parse::<toml::Table>()
            .expect("error expected TOML should parse")
    }

    fn expect_table<'a>(table: &'a toml::Table, key: &str) -> &'a toml::Table {
        table
            .get(key)
            .and_then(toml::Value::as_table)
            .unwrap_or_else(|| panic!("missing expected table `{key}`"))
    }

    fn expect_i64(table: &toml::Table, key: &str) -> i64 {
        table
            .get(key)
            .and_then(toml::Value::as_integer)
            .unwrap_or_else(|| panic!("missing expected integer `{key}`"))
    }

    fn expect_f64(table: &toml::Table, key: &str) -> f64 {
        table
            .get(key)
            .and_then(toml::Value::as_float)
            .unwrap_or_else(|| panic!("missing expected float `{key}`"))
    }

    fn expect_bool(table: &toml::Table, key: &str) -> bool {
        table
            .get(key)
            .and_then(toml::Value::as_bool)
            .unwrap_or_else(|| panic!("missing expected bool `{key}`"))
    }

    fn expect_str<'a>(table: &'a toml::Table, key: &str) -> &'a str {
        table
            .get(key)
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("missing expected string `{key}`"))
    }

    fn read_i64(reader: &ComplianceJsonReader, steps: &[PathStep]) -> i64 {
        let entered = enter_path(reader, steps).expect("path should be readable");
        let value = reader.as_i64().expect("value should be i64-compatible");
        exit_path(reader, entered);
        value
    }

    fn read_f64(reader: &ComplianceJsonReader, steps: &[PathStep]) -> f64 {
        let entered = enter_path(reader, steps).expect("path should be readable");
        let value = reader.as_f64().expect("value should be f64-compatible");
        exit_path(reader, entered);
        value
    }

    fn read_bool(reader: &ComplianceJsonReader, steps: &[PathStep]) -> bool {
        let entered = enter_path(reader, steps).expect("path should be readable");
        let value = reader.as_bool().expect("value should be bool-compatible");
        exit_path(reader, entered);
        value
    }

    fn read_string(reader: &ComplianceJsonReader, steps: &[PathStep]) -> String {
        let entered = enter_path(reader, steps).expect("path should be readable");
        let value = reader.as_string().expect("value should be string");
        exit_path(reader, entered);
        value
    }

    fn read_len(reader: &ComplianceJsonReader, steps: &[PathStep]) -> usize {
        let entered = enter_path(reader, steps).expect("path should be readable");
        let value = reader.read_list_len().expect("value should have length");
        exit_path(reader, entered);
        value
    }

    #[test]
    fn rust_runtime_compliance_scalar_and_string_cases_pass() {
        let expected = read_expected_results();
        let scalar = expect_table(&expected, "scalar_compat");
        let row = [PathStep::Field("rows"), PathStep::Index(0)];
        let reader = ComplianceJsonReader::new(read_payload("scalar_compat.json"));

        assert_eq!(
            read_i64(&reader, &[&row[..], &[PathStep::Field("id")]].concat()),
            expect_i64(scalar, "id")
        );
        assert_eq!(
            read_i64(&reader, &[&row[..], &[PathStep::Field("level")]].concat()),
            expect_i64(scalar, "level")
        );
        assert_eq!(
            read_f64(&reader, &[&row[..], &[PathStep::Field("ratio")]].concat()),
            expect_f64(scalar, "ratio")
        );
        assert_eq!(
            read_bool(
                &reader,
                &[&row[..], &[PathStep::Field("enabled_text")]].concat()
            ),
            expect_bool(scalar, "enabled_text")
        );
        assert_eq!(
            read_bool(
                &reader,
                &[&row[..], &[PathStep::Field("disabled_text")]].concat()
            ),
            expect_bool(scalar, "disabled_text")
        );
        assert_eq!(
            read_bool(
                &reader,
                &[&row[..], &[PathStep::Field("enabled_bool")]].concat()
            ),
            expect_bool(scalar, "enabled_bool")
        );
        assert_eq!(
            read_string(&reader, &[&row[..], &[PathStep::Field("name")]].concat()),
            expect_str(scalar, "name")
        );
        let entered = enter_path(&reader, &row).expect("row should be readable");
        assert!(
            !reader
                .enter_optional_field("nullable")
                .expect("null optional field should be absent")
        );
        exit_path(&reader, entered);
        assert!(expect_bool(scalar, "nullable_is_null"));

        let escaped = expect_table(&expected, "escaped_string");
        let reader = ComplianceJsonReader::new(read_payload("escaped_string.json"));
        assert_eq!(
            read_string(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("text"),
                ],
            ),
            expect_str(escaped, "text")
        );
    }

    #[test]
    fn rust_runtime_compliance_collection_object_nested_and_fragment_cases_pass() {
        let expected = read_expected_results();

        let collection = expect_table(&expected, "collection_envelope");
        let reader = ComplianceJsonReader::new(read_payload("collection_envelope.json"));
        assert_eq!(
            read_string(&reader, &[PathStep::Field("meta"), PathStep::Field("name")]),
            expect_str(collection, "table_name")
        );
        assert_eq!(
            read_len(&reader, &[PathStep::Field("rows")]),
            usize::try_from(expect_i64(collection, "row_count")).expect("row count fits usize")
        );
        assert_eq!(
            read_i64(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("id"),
                ],
            ),
            expect_i64(collection, "first_id")
        );
        assert_eq!(
            read_string(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(1),
                    PathStep::Field("name"),
                ],
            ),
            expect_str(collection, "second_name")
        );
        assert_eq!(
            read_len(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("tags"),
                ],
            ),
            usize::try_from(expect_i64(collection, "tag_count")).expect("tag count fits usize")
        );
        assert_eq!(
            read_string(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("tags"),
                    PathStep::Index(0),
                ],
            ),
            expect_str(collection, "first_tag")
        );

        let object = expect_table(&expected, "object_table");
        let reader = ComplianceJsonReader::new(read_payload("object_table.json"));
        assert_eq!(
            read_string(&reader, &[PathStep::Field("meta"), PathStep::Field("name")]),
            expect_str(object, "table_name")
        );
        assert_eq!(
            read_i64(&reader, &[PathStep::Field("default_class_id")]),
            expect_i64(object, "default_class_id")
        );
        assert_eq!(
            read_bool(&reader, &[PathStep::Field("enabled")]),
            expect_bool(object, "enabled")
        );
        assert_eq!(
            read_string(&reader, &[PathStep::Field("title")]),
            expect_str(object, "title")
        );

        let nested = expect_table(&expected, "nested_struct");
        let reader = ComplianceJsonReader::new(read_payload("nested_struct.json"));
        assert_eq!(
            read_i64(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("item_id"),
                ],
            ),
            expect_i64(nested, "item_id")
        );
        assert_eq!(
            read_i64(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("reward"),
                    PathStep::Field("count"),
                ],
            ),
            expect_i64(nested, "reward_count")
        );
        assert_eq!(
            read_i64(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("reward"),
                    PathStep::Field("item_id"),
                ],
            ),
            expect_i64(nested, "reward_item_id")
        );
        assert_eq!(
            read_len(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("attributes"),
                ],
            ),
            usize::try_from(expect_i64(nested, "attribute_count"))
                .expect("attribute count fits usize")
        );
        assert_eq!(
            read_string(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("attributes"),
                    PathStep::Index(0),
                ],
            ),
            expect_str(nested, "first_attribute")
        );

        let fragment = expect_table(&expected, "fragment");
        let reader = ComplianceJsonReader::new(read_payload("fragment.json"));
        assert_eq!(
            read_i64(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("item_id"),
                ],
            ),
            expect_i64(fragment, "item_id")
        );
        assert_eq!(
            read_string(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("effect"),
                    PathStep::Field("kind"),
                ],
            ),
            expect_str(fragment, "fragment_kind")
        );
        assert_eq!(
            read_i64(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("effect"),
                    PathStep::Field("amount"),
                ],
            ),
            expect_i64(fragment, "fragment_amount")
        );
    }

    #[test]
    fn rust_runtime_compliance_error_cases_are_executable() {
        let cases = read_error_cases();
        let cases = cases
            .get("cases")
            .and_then(toml::Value::as_array)
            .expect("error cases should be an array");
        let mut executed_json_cases = 0;
        for case in cases {
            let case = case.as_table().expect("case should be a table");
            if case
                .get("format")
                .and_then(toml::Value::as_str)
                .is_some_and(|format| format != "json")
            {
                continue;
            }
            let name = expect_str(case, "name");
            let payload = expect_str(case, "payload");
            let operation = expect_str(case, "operation");
            let expected_kind = expect_str(case, "expected_kind");
            let expected_message_contains = expect_str(case, "expected_message_contains");
            let reader = ComplianceJsonReader::new(read_payload(payload));
            let error = match name {
                "missing_field" => {
                    let entered =
                        enter_path(&reader, &[PathStep::Field("rows"), PathStep::Index(0)])
                            .expect("row should be readable");
                    let error = reader
                        .enter_required_field("missing")
                        .expect_err("missing required field should fail");
                    exit_path(&reader, entered);
                    error
                }
                "explicit_null_required_field" => {
                    let entered =
                        enter_path(&reader, &[PathStep::Field("rows"), PathStep::Index(0)])
                            .expect("row should be readable");
                    let error = reader
                        .enter_required_field("explicit_null")
                        .expect_err("null required field should fail");
                    exit_path(&reader, entered);
                    error
                }
                "array_index_out_of_bounds" => {
                    let entered = enter_path(&reader, &[PathStep::Field("rows")])
                        .expect("rows should be readable");
                    let error = reader
                        .enter_index(99)
                        .expect_err("out-of-bounds index should fail");
                    exit_path(&reader, entered);
                    error
                }
                "int_overflow" => run_scalar_error(
                    &reader,
                    &[
                        PathStep::Field("rows"),
                        PathStep::Index(0),
                        PathStep::Field("overflow_int"),
                    ],
                    |reader| reader.as_i64().map(|_| ()),
                ),
                "invalid_bool_string" => run_scalar_error(
                    &reader,
                    &[
                        PathStep::Field("rows"),
                        PathStep::Index(0),
                        PathStep::Field("invalid_bool"),
                    ],
                    |reader| reader.as_bool().map(|_| ()),
                ),
                "non_finite_float_string" => run_scalar_error(
                    &reader,
                    &[
                        PathStep::Field("rows"),
                        PathStep::Index(0),
                        PathStep::Field("non_finite_float"),
                    ],
                    |reader| reader.as_f64().map(|_| ()),
                ),
                other => panic!("unhandled JSON compliance error case `{other}`"),
            };
            assert_eq!(
                format!("{:?}", error.kind),
                expected_kind,
                "case `{name}` operation `{operation}` should fail with expected kind"
            );
            assert!(
                error.message.contains(expected_message_contains),
                "case `{name}` error `{}` should contain `{expected_message_contains}`",
                error.message
            );
            executed_json_cases += 1;
        }
        assert_eq!(executed_json_cases, 6);
    }

    fn run_scalar_error(
        reader: &ComplianceJsonReader,
        steps: &[PathStep],
        operation: impl FnOnce(&ComplianceJsonReader) -> RuntimeDataResult<()>,
    ) -> RuntimeDataError {
        let entered = enter_path(reader, steps).expect("error case path should be readable");
        let error = operation(reader).expect_err("operation should fail");
        exit_path(reader, entered);
        error
    }
}
