// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::codegen::CodeWriterLike;

pub(super) fn render_runtime_reader_imports(content: &mut impl CodeWriterLike) {
    content.raw(
        r#"pub use config_forge_runtime_reader_core::{
    CONFIG_FORGE_READER_CONTRACT_VERSION, RuntimeDataError, RuntimeDataLoader, RuntimeDataReader,
    RuntimeDataResult,
};

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
