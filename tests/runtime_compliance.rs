// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unused_imports)]

use std::{fs, path::Path};

use config_forge_runtime_reader_core::{RuntimeDataError, RuntimeDataReader, RuntimeDataResult};
use serde_json::Value;

#[path = "../runtime/rust/serde_json_reader/src/runtime_reader.rs"]
mod json_runtime_reader;
#[path = "../runtime/rust/serde_msgpack_reader/src/runtime_reader.rs"]
mod msgpack_runtime_reader;

#[derive(Debug, Clone, Copy)]
enum PathStep {
    Field(&'static str),
    Index(usize),
}

struct ReaderAdapter<R> {
    name: &'static str,
    decode: fn(Value) -> R,
}

fn json_adapter() -> ReaderAdapter<json_runtime_reader::JsonValueReader> {
    ReaderAdapter {
        name: "rust-serde-json",
        decode: |value| {
            let bytes = serde_json::to_vec(&value).expect("canonical JSON should encode");
            let value = serde_json::from_slice(&bytes).expect("canonical JSON should decode");
            json_runtime_reader::JsonValueReader::new(value)
        },
    }
}

fn messagepack_adapter() -> ReaderAdapter<msgpack_runtime_reader::MessagePackValueReader> {
    ReaderAdapter {
        name: "rust-serde-messagepack",
        decode: |value| {
            let bytes = rmp_serde::to_vec_named(&value).expect("canonical value should encode");
            let value = rmp_serde::from_slice(&bytes).expect("canonical MessagePack should decode");
            msgpack_runtime_reader::MessagePackValueReader::new(value)
        },
    }
}

fn compliance_payload_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/compliance/payloads")
}

fn read_payload(name: &str) -> Value {
    let content = fs::read_to_string(compliance_payload_dir().join(name))
        .unwrap_or_else(|error| panic!("failed to read compliance payload `{name}`: {error}"));
    serde_json::from_str(&content)
        .unwrap_or_else(|error| panic!("failed to parse compliance payload `{name}`: {error}"))
}

fn read_expected_results() -> toml::Table {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("runtime/compliance/expected/scalar_results.toml");
    let content = fs::read_to_string(path).expect("scalar expected TOML should exist");
    content
        .parse::<toml::Table>()
        .expect("scalar expected TOML should parse")
}

fn read_error_cases() -> toml::Table {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/compliance/expected/error_cases.toml");
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

fn enter_path<R: RuntimeDataReader>(reader: &R, steps: &[PathStep]) -> RuntimeDataResult<usize> {
    let mut entered = 0;
    for step in steps {
        match step {
            PathStep::Field(name) => reader.enter_required_field(name)?,
            PathStep::Index(index) => reader.enter_index(*index)?,
        }
        entered += 1;
    }
    Ok(entered)
}

fn exit_path<R: RuntimeDataReader>(reader: &R, entered: usize) {
    for _ in 0..entered {
        reader.exit().expect("compliance path should exit cleanly");
    }
}

fn read_i64<R: RuntimeDataReader>(reader: &R, steps: &[PathStep]) -> i64 {
    let entered = enter_path(reader, steps).expect("path should be readable");
    let value = reader.as_i64().expect("value should be i64-compatible");
    exit_path(reader, entered);
    value
}

fn read_f64<R: RuntimeDataReader>(reader: &R, steps: &[PathStep]) -> f64 {
    let entered = enter_path(reader, steps).expect("path should be readable");
    let value = reader.as_f64().expect("value should be f64-compatible");
    exit_path(reader, entered);
    value
}

fn read_bool<R: RuntimeDataReader>(reader: &R, steps: &[PathStep]) -> bool {
    let entered = enter_path(reader, steps).expect("path should be readable");
    let value = reader.as_bool().expect("value should be bool-compatible");
    exit_path(reader, entered);
    value
}

fn read_string<R: RuntimeDataReader>(reader: &R, steps: &[PathStep]) -> String {
    let entered = enter_path(reader, steps).expect("path should be readable");
    let value = reader.as_string().expect("value should be string");
    exit_path(reader, entered);
    value
}

fn read_row_count<R: RuntimeDataReader>(reader: &R, steps: &[PathStep]) -> usize {
    let entered = enter_path(reader, steps).expect("path should be readable");
    let value = reader
        .read_row_count()
        .expect("value should have row count");
    exit_path(reader, entered);
    value
}

fn read_list_len<R: RuntimeDataReader>(reader: &R, steps: &[PathStep]) -> usize {
    let entered = enter_path(reader, steps).expect("path should be readable");
    let value = reader
        .read_list_len()
        .expect("value should have list length");
    exit_path(reader, entered);
    value
}

fn run_success_cases<R: RuntimeDataReader>(adapter: ReaderAdapter<R>) {
    let expected = read_expected_results();
    run_scalar_and_string_cases(&adapter, &expected);
    run_collection_object_nested_and_fragment_cases(&adapter, &expected);
}

fn reader_for<R>(adapter: &ReaderAdapter<R>, payload: &str) -> R {
    (adapter.decode)(read_payload(payload))
}

fn run_scalar_and_string_cases<R: RuntimeDataReader>(
    adapter: &ReaderAdapter<R>,
    expected: &toml::Table,
) {
    let scalar = expect_table(expected, "scalar_compat");
    let row = [PathStep::Field("rows"), PathStep::Index(0)];
    let reader = reader_for(adapter, "scalar_compat.json");

    assert_eq!(
        read_i64(&reader, &[&row[..], &[PathStep::Field("id")]].concat()),
        expect_i64(scalar, "id"),
        "{} should read integer scalar",
        adapter.name
    );
    assert_eq!(
        read_i64(&reader, &[&row[..], &[PathStep::Field("level")]].concat()),
        expect_i64(scalar, "level"),
        "{} should read numeric string as int",
        adapter.name
    );
    assert_eq!(
        read_f64(&reader, &[&row[..], &[PathStep::Field("ratio")]].concat()),
        expect_f64(scalar, "ratio"),
        "{} should read float scalar",
        adapter.name
    );
    assert_eq!(
        read_bool(
            &reader,
            &[&row[..], &[PathStep::Field("enabled_text")]].concat()
        ),
        expect_bool(scalar, "enabled_text"),
        "{} should read true-like string bool",
        adapter.name
    );
    assert_eq!(
        read_bool(
            &reader,
            &[&row[..], &[PathStep::Field("disabled_text")]].concat()
        ),
        expect_bool(scalar, "disabled_text"),
        "{} should read false-like string bool",
        adapter.name
    );
    assert_eq!(
        read_bool(
            &reader,
            &[&row[..], &[PathStep::Field("enabled_bool")]].concat()
        ),
        expect_bool(scalar, "enabled_bool"),
        "{} should read bool scalar",
        adapter.name
    );
    assert_eq!(
        read_string(&reader, &[&row[..], &[PathStep::Field("name")]].concat()),
        expect_str(scalar, "name"),
        "{} should read string scalar",
        adapter.name
    );

    let entered = enter_path(&reader, &row).expect("row should be readable");
    assert!(
        !reader
            .enter_optional_field("nullable")
            .expect("null optional field should be absent"),
        "{} should treat explicit null optional field as absent",
        adapter.name
    );
    exit_path(&reader, entered);
    assert!(expect_bool(scalar, "nullable_is_null"));

    let escaped = expect_table(expected, "escaped_string");
    let reader = reader_for(adapter, "escaped_string.json");
    assert_eq!(
        read_string(
            &reader,
            &[
                PathStep::Field("rows"),
                PathStep::Index(0),
                PathStep::Field("text"),
            ],
        ),
        expect_str(escaped, "text"),
        "{} should preserve escaped string content",
        adapter.name
    );
}

fn run_collection_object_nested_and_fragment_cases<R: RuntimeDataReader>(
    adapter: &ReaderAdapter<R>,
    expected: &toml::Table,
) {
    let collection = expect_table(expected, "collection_envelope");
    let reader = reader_for(adapter, "collection_envelope.json");
    assert_eq!(
        read_string(&reader, &[PathStep::Field("meta"), PathStep::Field("name")]),
        expect_str(collection, "table_name"),
        "{} should read collection metadata",
        adapter.name
    );
    assert_eq!(
        read_row_count(&reader, &[PathStep::Field("rows")]),
        usize::try_from(expect_i64(collection, "row_count")).expect("row count fits usize"),
        "{} should read collection row count",
        adapter.name
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
        expect_i64(collection, "first_id"),
        "{} should read first row id",
        adapter.name
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
        expect_str(collection, "second_name"),
        "{} should read second row name",
        adapter.name
    );
    assert_eq!(
        read_list_len(
            &reader,
            &[
                PathStep::Field("rows"),
                PathStep::Index(0),
                PathStep::Field("tags"),
            ],
        ),
        usize::try_from(expect_i64(collection, "tag_count")).expect("tag count fits usize"),
        "{} should read list length",
        adapter.name
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
        expect_str(collection, "first_tag"),
        "{} should read list item",
        adapter.name
    );

    let object = expect_table(expected, "object_table");
    let reader = reader_for(adapter, "object_table.json");
    assert_eq!(
        read_string(&reader, &[PathStep::Field("meta"), PathStep::Field("name")]),
        expect_str(object, "table_name"),
        "{} should read object table metadata",
        adapter.name
    );
    assert_eq!(
        read_i64(&reader, &[PathStep::Field("default_class_id")]),
        expect_i64(object, "default_class_id"),
        "{} should read object table int field",
        adapter.name
    );
    assert_eq!(
        read_bool(&reader, &[PathStep::Field("enabled")]),
        expect_bool(object, "enabled"),
        "{} should read object table bool field",
        adapter.name
    );
    assert_eq!(
        read_string(&reader, &[PathStep::Field("title")]),
        expect_str(object, "title"),
        "{} should read object table string field",
        adapter.name
    );

    let nested = expect_table(expected, "nested_struct");
    let reader = reader_for(adapter, "nested_struct.json");
    assert_eq!(
        read_i64(
            &reader,
            &[
                PathStep::Field("rows"),
                PathStep::Index(0),
                PathStep::Field("item_id"),
            ],
        ),
        expect_i64(nested, "item_id"),
        "{} should read nested case row id",
        adapter.name
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
        expect_i64(nested, "reward_count"),
        "{} should read nested struct count",
        adapter.name
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
        expect_i64(nested, "reward_item_id"),
        "{} should read nested struct item id",
        adapter.name
    );
    assert_eq!(
        read_list_len(
            &reader,
            &[
                PathStep::Field("rows"),
                PathStep::Index(0),
                PathStep::Field("attributes"),
            ],
        ),
        usize::try_from(expect_i64(nested, "attribute_count")).expect("attribute count fits usize"),
        "{} should read nested list length",
        adapter.name
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
        expect_str(nested, "first_attribute"),
        "{} should read nested list item",
        adapter.name
    );

    let fragment = expect_table(expected, "fragment");
    let reader = reader_for(adapter, "fragment.json");
    assert_eq!(
        read_i64(
            &reader,
            &[
                PathStep::Field("rows"),
                PathStep::Index(0),
                PathStep::Field("item_id"),
            ],
        ),
        expect_i64(fragment, "item_id"),
        "{} should read fragment row id",
        adapter.name
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
        expect_str(fragment, "fragment_kind"),
        "{} should read fragment kind",
        adapter.name
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
        expect_i64(fragment, "fragment_amount"),
        "{} should read fragment amount",
        adapter.name
    );
}

fn run_json_compatible_error_cases<R: RuntimeDataReader>(adapter: ReaderAdapter<R>) {
    let cases = read_error_cases();
    let cases = cases
        .get("cases")
        .and_then(toml::Value::as_array)
        .expect("error cases should be an array");
    let mut executed_cases = 0;
    for case in cases {
        let case = case.as_table().expect("case should be a table");
        if case.get("format").and_then(toml::Value::as_str).is_some() {
            continue;
        }
        let name = expect_str(case, "name");
        let payload = expect_str(case, "payload");
        let operation = expect_str(case, "operation");
        let expected_kind = expect_str(case, "expected_kind");
        let expected_message_contains = expect_str(case, "expected_message_contains");
        let reader = reader_for(&adapter, payload);
        let error = match name {
            "missing_field" => {
                let entered = enter_path(&reader, &[PathStep::Field("rows"), PathStep::Index(0)])
                    .expect("row should be readable");
                let error = reader
                    .enter_required_field("missing")
                    .expect_err("missing required field should fail");
                exit_path(&reader, entered);
                error
            }
            "explicit_null_required_field" => {
                let entered = enter_path(&reader, &[PathStep::Field("rows"), PathStep::Index(0)])
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
            other => panic!("unhandled compliance error case `{other}`"),
        };
        assert_eq!(
            format!("{:?}", error.kind),
            expected_kind,
            "{} case `{name}` operation `{operation}` should fail with expected kind",
            adapter.name
        );
        assert!(
            error.message.contains(expected_message_contains),
            "{} case `{name}` error `{}` should contain `{expected_message_contains}`",
            adapter.name,
            error.message
        );
        executed_cases += 1;
    }
    assert_eq!(
        executed_cases, 6,
        "{} should execute JSON-compatible errors",
        adapter.name
    );
}

fn run_scalar_error<R: RuntimeDataReader>(
    reader: &R,
    steps: &[PathStep],
    operation: impl FnOnce(&R) -> RuntimeDataResult<()>,
) -> RuntimeDataError {
    let entered = enter_path(reader, steps).expect("error case path should be readable");
    let error = operation(reader).expect_err("operation should fail");
    exit_path(reader, entered);
    error
}

#[test]
fn rust_serde_json_adapter_passes_compliance_success_cases() {
    run_success_cases(json_adapter());
}

#[test]
fn rust_serde_json_adapter_passes_compliance_error_cases() {
    run_json_compatible_error_cases(json_adapter());
}

#[test]
fn rust_serde_messagepack_adapter_passes_compliance_success_cases() {
    run_success_cases(messagepack_adapter());
}

#[test]
fn rust_serde_messagepack_adapter_passes_json_compatible_compliance_error_cases() {
    run_json_compatible_error_cases(messagepack_adapter());
}
