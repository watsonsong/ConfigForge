// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

pub const CONFIG_FORGE_READER_CONTRACT_VERSION: &str = "1.0";
pub const SUPPORTED_CONFIG_FORGE_READER_CONTRACT: &str = "^1.0";

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
    fn is_null(&self) -> bool;
    fn schema_revision(&self) -> Option<u64> {
        None
    }
    fn is_positional(&self) -> bool {
        false
    }
    fn read_row_count(&self) -> RuntimeDataResult<usize> {
        Err(RuntimeDataError::unsupported(
            "positional runtime reader is not supported",
        ))
    }
    fn begin_record(&self, _optional_slot_count: usize) -> RuntimeDataResult<()> {
        Ok(())
    }
    fn end_record(&self) -> RuntimeDataResult<()> {
        Ok(())
    }
    fn read_optional_present(&self, _slot_index: usize) -> RuntimeDataResult<bool> {
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
        Err(RuntimeDataError::unsupported(
            "positional runtime reader is not supported",
        ))
    }
}

pub trait RuntimeDataLoader {
    type Reader: RuntimeDataReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader>;
}
