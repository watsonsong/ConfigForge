// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::codegen::CodeWriter;

pub(super) fn render_runtime_reader_imports(content: &mut CodeWriter) {
    content.raw(
        r#"pub use config_forge_runtime_reader_core::{
    CONFIG_FORGE_READER_CONTRACT_VERSION, RuntimeDataError, RuntimeDataLoader, RuntimeDataReader,
    RuntimeDataResult,
};

"#,
    );
}

pub(super) fn render_runtime_reader_helpers(content: &mut CodeWriter) {
    content.raw(
        r#"fn with_required_field<R: RuntimeDataReader, T>(
    reader: &R,
    name: &'static str,
    read: impl FnOnce(&R) -> RuntimeDataResult<T>,
) -> RuntimeDataResult<T> {
    if !reader.enter_field(name)? {
        return Err(RuntimeDataError::not_found(format!("missing required field `{name}`")));
    }
    let result = if reader.is_null() {
        Err(RuntimeDataError::not_found(format!("missing required field `{name}`")))
    } else {
        read(reader)
    };
    match (result, reader.exit()) {
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
    if !reader.enter_field(name)? {
        return Ok(None);
    }
    let result = if reader.is_null() { Ok(None) } else { read(reader).map(Some) };
    match (result, reader.exit()) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn read_required_i64<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<i64> {
    if reader.is_positional() {
        return reader.as_i64();
    }
    with_required_field(reader, name, |value| value.as_i64())
}

fn read_required_f64<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<f64> {
    if reader.is_positional() {
        return reader.as_f64();
    }
    with_required_field(reader, name, |value| value.as_f64())
}

fn read_required_bool<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<bool> {
    if reader.is_positional() {
        return reader.as_bool();
    }
    with_required_field(reader, name, |value| value.as_bool())
}

fn read_required_string<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<String> {
    if reader.is_positional() {
        return reader.as_string();
    }
    with_required_field(reader, name, |value| value.as_string())
}

fn read_optional_i64<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<Option<i64>> {
    if reader.is_positional() {
        return if reader.read_optional_present()? { Ok(Some(reader.as_i64()?)) } else { Ok(None) };
    }
    with_optional_field(reader, name, |value| value.as_i64())
}

fn read_optional_f64<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<Option<f64>> {
    if reader.is_positional() {
        return if reader.read_optional_present()? { Ok(Some(reader.as_f64()?)) } else { Ok(None) };
    }
    with_optional_field(reader, name, |value| value.as_f64())
}

fn read_optional_bool<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<Option<bool>> {
    if reader.is_positional() {
        return if reader.read_optional_present()? { Ok(Some(reader.as_bool()?)) } else { Ok(None) };
    }
    with_optional_field(reader, name, |value| value.as_bool())
}

fn read_optional_string<R: RuntimeDataReader>(reader: &R, name: &'static str) -> RuntimeDataResult<Option<String>> {
    if reader.is_positional() {
        return if reader.read_optional_present()? { Ok(Some(reader.as_string()?)) } else { Ok(None) };
    }
    with_optional_field(reader, name, |value| value.as_string())
}

"#,
    );
}
