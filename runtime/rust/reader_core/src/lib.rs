// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

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

    pub fn check_array_length(&self, length: usize) -> RuntimeDataResult<()> {
        check_limit("array length", length, self.max_array_length)
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
    fn enter_field(&self, name: &'static str) -> RuntimeDataResult<bool>;
    fn enter_index(&self, index: usize) -> RuntimeDataResult<()>;
    fn exit(&self) -> RuntimeDataResult<()>;
    fn len(&self) -> RuntimeDataResult<usize>;
    fn as_i64(&self) -> RuntimeDataResult<i64>;
    fn as_f64(&self) -> RuntimeDataResult<f64>;
    fn as_bool(&self) -> RuntimeDataResult<bool>;
    fn as_string(&self) -> RuntimeDataResult<String>;
    fn schema_revision(&self) -> Option<u64> {
        None
    }
    fn decode_limits(&self) -> RuntimeDecodeLimits {
        RuntimeDecodeLimits::default()
    }
    fn enter_required_field(&self, name: &'static str) -> RuntimeDataResult<()> {
        if !self.enter_field(name)? {
            return Err(RuntimeDataError::not_found(format!(
                "missing required field `{name}`"
            )));
        }
        Ok(())
    }
    fn enter_optional_field(&self, name: &'static str) -> RuntimeDataResult<bool> {
        if !self.enter_field(name)? {
            return Ok(false);
        }
        Ok(true)
    }
    fn exit_field(&self) -> RuntimeDataResult<()> {
        self.exit()
    }
    fn read_enum_index(&self, variants: &[&str]) -> RuntimeDataResult<usize> {
        let value = self.as_string()?;
        variants
            .iter()
            .position(|variant| *variant == value.as_str())
            .ok_or_else(|| RuntimeDataError::decode(format!("unknown enum value `{value}`")))
    }
    fn read_row_count(&self) -> RuntimeDataResult<usize> {
        let rows = self.len()?;
        self.decode_limits().check_rows(rows)?;
        Ok(rows)
    }
    fn read_array_len(&self) -> RuntimeDataResult<usize> {
        let length = self.len()?;
        self.decode_limits().check_array_length(length)?;
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
    fn read_optional_present(&self) -> RuntimeDataResult<bool> {
        Err(RuntimeDataError::unsupported(
            "positional runtime reader is not supported",
        ))
    }
    fn read_next_i64(&self) -> RuntimeDataResult<i64> {
        Err(RuntimeDataError::unsupported(
            "positional runtime reader is not supported",
        ))
    }
    fn read_next_f64(&self) -> RuntimeDataResult<f64> {
        Err(RuntimeDataError::unsupported(
            "positional runtime reader is not supported",
        ))
    }
    fn read_next_bool(&self) -> RuntimeDataResult<bool> {
        Err(RuntimeDataError::unsupported(
            "positional runtime reader is not supported",
        ))
    }
    fn read_next_string(&self) -> RuntimeDataResult<String> {
        Err(RuntimeDataError::unsupported(
            "positional runtime reader is not supported",
        ))
    }
    fn read_enum_ordinal(&self) -> RuntimeDataResult<u64> {
        Err(RuntimeDataError::unsupported(
            "positional runtime reader is not supported",
        ))
    }
    fn ensure_exhausted(&self) -> RuntimeDataResult<()> {
        Ok(())
    }
}

pub trait RuntimeDataLoader {
    type Reader: RuntimeDataReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader>;
}
