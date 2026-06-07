// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::codegen::{CodeWriterLike, RuntimeInterfaceMode};

use super::RustCodegenOptions;

pub(super) fn render_runtime_reader_contract(
    content: &mut impl CodeWriterLike,
    options: &RustCodegenOptions,
) {
    match options.runtime_interface {
        RuntimeInterfaceMode::Inline => render_inline_runtime_reader_contract(content),
        RuntimeInterfaceMode::External => render_external_runtime_reader_contract(content, options),
    }
}

fn render_external_runtime_reader_contract(
    content: &mut impl CodeWriterLike,
    options: &RustCodegenOptions,
) {
    let runtime_core_path = options
        .runtime_core_path
        .as_deref()
        .unwrap_or("config_forge_runtime_reader_core");
    content.raw_fmt(format_args!(
        r#"pub use {runtime_core_path}::{{
    CONFIG_FORGE_READER_CONTRACT_VERSION, RuntimeDataError, RuntimeDataErrorKind,
    RuntimeDataLoader, RuntimeDataReader, RuntimeDataResult,
}};

"#
    ));
}

fn render_inline_runtime_reader_contract(content: &mut impl CodeWriterLike) {
    content.raw(
        r#"pub const CONFIG_FORGE_READER_CONTRACT_VERSION: &str = "1.0";

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

"#,
    );
}

pub(super) fn render_runtime_reader_helpers(content: &mut impl CodeWriterLike) {
    content.raw(
        r#"fn with_required_field<R: RuntimeDataReader, T>(
    reader: &R,
    name: &'static str,
    read: impl FnOnce(&R) -> RuntimeDataResult<T>,
) -> RuntimeDataResult<T> {
    reader.enter_required_field(name)?;
    let result = read(reader);
    match (result, reader.exit_field()) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn with_optional_field<R: RuntimeDataReader, T>(
    reader: &R,
    name: &'static str,
    read: impl FnOnce(&R) -> RuntimeDataResult<T>,
) -> RuntimeDataResult<Option<T>> {
    if !reader.enter_optional_field(name)? {
        return Ok(None);
    }
    let result = read(reader).map(Some);
    match (result, reader.exit_field()) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn read_required_i64<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<i64> {
    with_required_field(reader, name, |value| value.as_i64())
}

fn read_required_f64<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<f64> {
    with_required_field(reader, name, |value| value.as_f64())
}

fn read_required_bool<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<bool> {
    with_required_field(reader, name, |value| value.as_bool())
}

fn read_required_string<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<String> {
    with_required_field(reader, name, |value| value.as_string())
}

fn read_optional_i64<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<Option<i64>> {
    with_optional_field(reader, name, |value| value.as_i64())
}

fn read_optional_f64<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<Option<f64>> {
    with_optional_field(reader, name, |value| value.as_f64())
}

fn read_optional_bool<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<Option<bool>> {
    with_optional_field(reader, name, |value| value.as_bool())
}

fn read_optional_string<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<Option<String>> {
    with_optional_field(reader, name, |value| value.as_string())
}

fn read_list<R: RuntimeDataReader, T>(reader: &R, read: impl Fn(&R) -> RuntimeDataResult<T>) -> RuntimeDataResult<Vec<T>> {
    let len = reader.read_list_len()?;
    let positional = reader.array_elements_are_positional();
    let mut values = Vec::new();
    for index in 0..len {
        if positional {
            values.push(read(reader)?);
        } else {
            reader.enter_index(index)?;
            let value = read(reader);
            let exit = reader.exit();
            match (value, exit) {
                (Ok(value), Ok(())) => values.push(value),
                (Err(error), _) => return Err(error),
                (Ok(_), Err(error)) => return Err(error),
            }
        }
    }
    Ok(values)
}

fn read_required_list<R: RuntimeDataReader, T>(
    reader: &R,
    name: &'static str,
    read: impl Fn(&R) -> RuntimeDataResult<T>,
) -> RuntimeDataResult<Vec<T>> {
    with_required_field(reader, name, |value| read_list(value, read))
}

fn read_optional_list<R: RuntimeDataReader, T>(
    reader: &R,
    name: &'static str,
    read: impl Fn(&R) -> RuntimeDataResult<T>,
) -> RuntimeDataResult<Option<Vec<T>>> {
    with_optional_field(reader, name, |value| read_list(value, read))
}

"#,
    );
}
