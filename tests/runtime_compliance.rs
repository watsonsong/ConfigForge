// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unused_imports)]

use std::{fs, path::Path};

use config_forge_runtime_reader_core::{
    RuntimeDataError, RuntimeDataLoader, RuntimeDataReader, RuntimeDataResult,
};
use serde_json::Value;

#[path = "../runtime/rust/binary_reader/src/runtime_reader.rs"]
mod binary_runtime_reader;
#[path = "../runtime/rust/serde_json_reader/src/runtime_reader.rs"]
mod json_runtime_reader;
#[path = "../runtime/rust/serde_msgpack_reader/src/runtime_reader.rs"]
mod msgpack_runtime_reader;

const BINARY_FORMAT_VERSION: u16 = 1;
const BINARY_HEADER_SIZE: u16 = 16;
const JS_SAFE_INTEGER_MAX: i64 = (1_i64 << 53) - 1;
const JS_SAFE_INTEGER_MIN: i64 = -JS_SAFE_INTEGER_MAX;

#[derive(Debug, Clone, Copy)]
enum PathStep {
    Field(&'static str),
    Index(usize),
}

#[derive(Debug, Clone, Copy)]
enum BinaryTableKind {
    Collection,
    Object,
}

#[derive(Debug, Clone)]
struct BinaryCase {
    name: &'static str,
    payload: &'static str,
    kind: BinaryTableKind,
    layout: BinaryRecordLayout,
}

#[derive(Debug, Clone)]
struct BinaryRecordLayout {
    fields: Vec<BinaryFieldLayout>,
}

#[derive(Debug, Clone)]
struct BinaryFieldLayout {
    name: &'static str,
    optional: bool,
    value: BinaryValueLayout,
}

#[derive(Debug, Clone)]
enum BinaryValueLayout {
    Int,
    Float,
    Bool,
    String,
    Struct(BinaryRecordLayout),
    List(Box<BinaryValueLayout>),
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

fn int_field(name: &'static str) -> BinaryFieldLayout {
    BinaryFieldLayout {
        name,
        optional: false,
        value: BinaryValueLayout::Int,
    }
}

fn float_field(name: &'static str) -> BinaryFieldLayout {
    BinaryFieldLayout {
        name,
        optional: false,
        value: BinaryValueLayout::Float,
    }
}

fn bool_field(name: &'static str) -> BinaryFieldLayout {
    BinaryFieldLayout {
        name,
        optional: false,
        value: BinaryValueLayout::Bool,
    }
}

fn string_field(name: &'static str) -> BinaryFieldLayout {
    BinaryFieldLayout {
        name,
        optional: false,
        value: BinaryValueLayout::String,
    }
}

fn optional_string_field(name: &'static str) -> BinaryFieldLayout {
    BinaryFieldLayout {
        name,
        optional: true,
        value: BinaryValueLayout::String,
    }
}

fn struct_field(name: &'static str, fields: Vec<BinaryFieldLayout>) -> BinaryFieldLayout {
    BinaryFieldLayout {
        name,
        optional: false,
        value: BinaryValueLayout::Struct(BinaryRecordLayout { fields }),
    }
}

fn string_list_field(name: &'static str) -> BinaryFieldLayout {
    BinaryFieldLayout {
        name,
        optional: false,
        value: BinaryValueLayout::List(Box::new(BinaryValueLayout::String)),
    }
}

fn binary_success_cases() -> Vec<BinaryCase> {
    vec![
        BinaryCase {
            name: "scalar_compat",
            payload: "scalar_compat.json",
            kind: BinaryTableKind::Collection,
            layout: BinaryRecordLayout {
                fields: vec![
                    int_field("id"),
                    int_field("level"),
                    float_field("ratio"),
                    bool_field("enabled_text"),
                    bool_field("disabled_text"),
                    bool_field("enabled_bool"),
                    string_field("name"),
                    optional_string_field("nullable"),
                ],
            },
        },
        BinaryCase {
            name: "escaped_string",
            payload: "escaped_string.json",
            kind: BinaryTableKind::Collection,
            layout: BinaryRecordLayout {
                fields: vec![string_field("text")],
            },
        },
        BinaryCase {
            name: "collection_envelope",
            payload: "collection_envelope.json",
            kind: BinaryTableKind::Collection,
            layout: BinaryRecordLayout {
                fields: vec![
                    int_field("id"),
                    string_field("name"),
                    string_list_field("tags"),
                ],
            },
        },
        BinaryCase {
            name: "object_table",
            payload: "object_table.json",
            kind: BinaryTableKind::Object,
            layout: BinaryRecordLayout {
                fields: vec![
                    int_field("default_class_id"),
                    bool_field("enabled"),
                    string_field("title"),
                ],
            },
        },
        BinaryCase {
            name: "nested_struct",
            payload: "nested_struct.json",
            kind: BinaryTableKind::Collection,
            layout: BinaryRecordLayout {
                fields: vec![
                    int_field("item_id"),
                    struct_field("reward", vec![int_field("item_id"), int_field("count")]),
                    string_list_field("attributes"),
                ],
            },
        },
        BinaryCase {
            name: "fragment",
            payload: "fragment.json",
            kind: BinaryTableKind::Collection,
            layout: BinaryRecordLayout {
                fields: vec![
                    int_field("item_id"),
                    struct_field("effect", vec![string_field("kind"), int_field("amount")]),
                ],
            },
        },
    ]
}

fn compliance_payload_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime_reader_compliance/payloads")
}

fn read_payload(name: &str) -> Value {
    let content = fs::read_to_string(compliance_payload_dir().join(name))
        .unwrap_or_else(|error| panic!("failed to read compliance payload `{name}`: {error}"));
    serde_json::from_str(&content)
        .unwrap_or_else(|error| panic!("failed to parse compliance payload `{name}`: {error}"))
}

fn read_expected_results() -> toml::Table {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runtime_reader_compliance/expected/scalar_results.toml");
    let content = fs::read_to_string(path).expect("scalar expected TOML should exist");
    content
        .parse::<toml::Table>()
        .expect("scalar expected TOML should parse")
}

fn binary_schema_revision(case: &BinaryCase) -> u64 {
    // Compliance fixtures do not depend on production Meta files, but the
    // materializer still derives a stable revision from the logical schema so
    // tests exercise the same exporter/reader contract boundary as generated
    // code: the expected revision changes when table kind or layout changes.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"config-forge-runtime-compliance-bin-v1");
    bytes.extend_from_slice(case.name.as_bytes());
    bytes.push(match case.kind {
        BinaryTableKind::Collection => 1,
        BinaryTableKind::Object => 2,
    });
    write_layout_signature(&mut bytes, &case.layout);
    fnv1a64(&bytes)
}

fn write_layout_signature(bytes: &mut Vec<u8>, layout: &BinaryRecordLayout) {
    write_varuint(bytes, layout.fields.len() as u64);
    for field in &layout.fields {
        write_varuint(bytes, field.name.len() as u64);
        bytes.extend_from_slice(field.name.as_bytes());
        bytes.push(u8::from(field.optional));
        write_value_signature(bytes, &field.value);
    }
}

fn write_value_signature(bytes: &mut Vec<u8>, layout: &BinaryValueLayout) {
    match layout {
        BinaryValueLayout::Int => bytes.push(1),
        BinaryValueLayout::Float => bytes.push(2),
        BinaryValueLayout::Bool => bytes.push(3),
        BinaryValueLayout::String => bytes.push(4),
        BinaryValueLayout::Struct(record) => {
            bytes.push(5);
            write_layout_signature(bytes, record);
        }
        BinaryValueLayout::List(element) => {
            bytes.push(6);
            write_value_signature(bytes, element);
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

fn materialize_binary_case(case: &BinaryCase) -> Vec<u8> {
    let payload = read_payload(case.payload);
    let mut bytes = Vec::new();
    write_binary_header(&mut bytes, binary_schema_revision(case));
    match case.kind {
        BinaryTableKind::Collection => {
            let rows = payload
                .get("rows")
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("binary case `{}` should contain rows array", case.name));
            write_varuint(&mut bytes, rows.len() as u64);
            for row in rows {
                materialize_record(&mut bytes, &case.layout, row, case.name);
            }
        }
        BinaryTableKind::Object => {
            materialize_record(&mut bytes, &case.layout, &payload, case.name)
        }
    }
    bytes
}

fn materialize_record(
    bytes: &mut Vec<u8>,
    layout: &BinaryRecordLayout,
    object: &Value,
    case_name: &str,
) {
    let optional_count = layout.fields.iter().filter(|field| field.optional).count();
    let bitmap_offset = bytes.len();
    bytes.resize(bytes.len() + optional_count.div_ceil(8), 0);
    let mut optional_slot = 0usize;
    for field in &layout.fields {
        let value = object.get(field.name);
        if field.optional {
            let present = value.is_some_and(|value| !value.is_null());
            if present {
                bytes[bitmap_offset + optional_slot / 8] |= 1 << (optional_slot % 8);
                materialize_value(
                    bytes,
                    &field.value,
                    value.expect("present optional value should exist"),
                    case_name,
                    field.name,
                );
            }
            optional_slot += 1;
        } else {
            let value = value.unwrap_or_else(|| {
                panic!(
                    "binary case `{case_name}` missing required field `{}`",
                    field.name
                )
            });
            assert!(
                !value.is_null(),
                "binary case `{case_name}` required field `{}` is null",
                field.name
            );
            materialize_value(bytes, &field.value, value, case_name, field.name);
        }
    }
}

fn materialize_value(
    bytes: &mut Vec<u8>,
    layout: &BinaryValueLayout,
    value: &Value,
    case_name: &str,
    field_name: &str,
) {
    match layout {
        BinaryValueLayout::Int => {
            write_zigzag(bytes, parse_i64_value(value, case_name, field_name))
        }
        BinaryValueLayout::Float => {
            bytes.extend_from_slice(&parse_f64_value(value, case_name, field_name).to_le_bytes())
        }
        BinaryValueLayout::Bool => {
            bytes.push(u8::from(parse_bool_value(value, case_name, field_name)))
        }
        BinaryValueLayout::String => {
            write_string(bytes, parse_string_value(value, case_name, field_name))
        }
        BinaryValueLayout::Struct(record) => materialize_record(bytes, record, value, case_name),
        BinaryValueLayout::List(element) => {
            let array = value.as_array().unwrap_or_else(|| {
                panic!("binary case `{case_name}` field `{field_name}` should be an array")
            });
            write_varuint(bytes, array.len() as u64);
            for element_value in array {
                materialize_value(bytes, element, element_value, case_name, field_name);
            }
        }
    }
}

fn parse_i64_value(value: &Value, case_name: &str, field_name: &str) -> i64 {
    let parsed = if let Some(value) = value.as_i64() {
        value
    } else if let Some(value) = value.as_str() {
        value.parse::<i64>().unwrap_or_else(|error| {
            panic!("binary case `{case_name}` field `{field_name}` invalid int `{value}`: {error}")
        })
    } else {
        panic!("binary case `{case_name}` field `{field_name}` should be int-compatible")
    };
    assert!(
        (JS_SAFE_INTEGER_MIN..=JS_SAFE_INTEGER_MAX).contains(&parsed),
        "binary case `{case_name}` field `{field_name}` int `{parsed}` exceeds JavaScript safe integer range"
    );
    parsed
}

fn parse_f64_value(value: &Value, case_name: &str, field_name: &str) -> f64 {
    let parsed = if let Some(value) = value.as_f64() {
        value
    } else if let Some(value) = value.as_str() {
        value.parse::<f64>().unwrap_or_else(|error| {
            panic!(
                "binary case `{case_name}` field `{field_name}` invalid float `{value}`: {error}"
            )
        })
    } else {
        panic!("binary case `{case_name}` field `{field_name}` should be float-compatible")
    };
    assert!(
        parsed.is_finite(),
        "binary case `{case_name}` field `{field_name}` float must be finite"
    );
    parsed
}

fn parse_bool_value(value: &Value, case_name: &str, field_name: &str) -> bool {
    if let Some(value) = value.as_bool() {
        return value;
    }
    match value.as_str() {
        Some("true" | "1") => true,
        Some("false" | "0") => false,
        Some(other) => panic!(
            "binary case `{case_name}` field `{field_name}` invalid bool-compatible string `{other}`"
        ),
        None => panic!("binary case `{case_name}` field `{field_name}` should be bool-compatible"),
    }
}

fn parse_string_value<'a>(value: &'a Value, case_name: &str, field_name: &str) -> &'a str {
    value.as_str().unwrap_or_else(|| {
        panic!("binary case `{case_name}` field `{field_name}` should be string")
    })
}

fn write_binary_header(bytes: &mut Vec<u8>, revision: u64) {
    bytes.extend_from_slice(b"CFGB");
    bytes.extend_from_slice(&BINARY_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&BINARY_HEADER_SIZE.to_le_bytes());
    bytes.extend_from_slice(&revision.to_le_bytes());
}

fn write_string(bytes: &mut Vec<u8>, value: &str) {
    write_varuint(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn write_zigzag(bytes: &mut Vec<u8>, value: i64) {
    write_varuint(bytes, ((value << 1) ^ (value >> 63)) as u64);
}

fn write_varuint(bytes: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn read_error_cases() -> toml::Table {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runtime_reader_compliance/expected/error_cases.toml");
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

fn with_binary_case_reader(
    case_name: &str,
    operation: impl FnOnce(&binary_runtime_reader::BinaryRuntimeDataReader, &BinaryCase),
) {
    let case = binary_success_cases()
        .into_iter()
        .find(|case| case.name == case_name)
        .unwrap_or_else(|| panic!("unknown binary compliance case `{case_name}`"));
    let bytes = materialize_binary_case(&case);
    let reader = binary_runtime_reader::BinaryRuntimeDataReader::from_bytes(bytes)
        .expect("materialized binary case should decode");
    assert_eq!(
        reader.schema_revision(),
        Some(binary_schema_revision(&case)),
        "binary case `{case_name}` should expose derived schema revision"
    );
    operation(&reader, &case);
    reader
        .ensure_exhausted()
        .unwrap_or_else(|error| panic!("binary case `{case_name}` should be exhausted: {error:?}"));
}

fn enter_binary_row(reader: &binary_runtime_reader::BinaryRuntimeDataReader, index: usize) {
    reader
        .enter_index(index)
        .expect("binary row should be read sequentially");
}

fn begin_binary_record(
    reader: &binary_runtime_reader::BinaryRuntimeDataReader,
    layout: &BinaryRecordLayout,
) {
    reader
        .begin_record(layout.fields.iter().filter(|field| field.optional).count())
        .expect("binary record should begin");
}

fn end_binary_record(reader: &binary_runtime_reader::BinaryRuntimeDataReader) {
    reader.end_record().expect("binary record should end");
}

fn binary_optional_present(
    reader: &binary_runtime_reader::BinaryRuntimeDataReader,
    field_name: &'static str,
) -> bool {
    reader
        .enter_optional_field(field_name)
        .expect("binary optional slot should be readable")
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
            "list_length_on_scalar" => run_scalar_error(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("id"),
                ],
                |reader| reader.read_list_len().map(|_| ()),
            ),
            "string_type_mismatch" => run_scalar_error(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("enabled_bool"),
                ],
                |reader| reader.as_string().map(|_| ()),
            ),
            "unknown_enum_value" => run_scalar_error(
                &reader,
                &[
                    PathStep::Field("rows"),
                    PathStep::Index(0),
                    PathStep::Field("name"),
                ],
                |reader| reader.read_enum_index(&["Sword", "Shield"]).map(|_| ()),
            ),
            other => panic!("unhandled compliance error case `{other}`"),
        };
        assert_eq!(
            format!("{:?}", error.kind),
            expected_kind,
            "{} case `{name}` operation `{operation}` should fail with expected kind",
            adapter.name
        );
        assert_eq!(
            error.code(),
            expected_error_code(expected_kind),
            "{} case `{name}` operation `{operation}` should expose expected code",
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
        executed_cases, 9,
        "{} should execute JSON-compatible errors",
        adapter.name
    );
}

fn expected_error_code(kind: &str) -> &'static str {
    match kind {
        "Io" => "CF_RT_IO",
        "Decode" => "CF_RT_DECODE",
        "SchemaMismatch" => "CF_RT_SCHEMA_MISMATCH",
        "TypeMismatch" => "CF_RT_TYPE_MISMATCH",
        "NotFound" => "CF_RT_NOT_FOUND",
        "Unsupported" => "CF_RT_UNSUPPORTED",
        "InvalidPath" => "CF_RT_INVALID_PATH",
        other => panic!("unknown expected error kind `{other}`"),
    }
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

fn run_binary_success_cases() {
    let expected = read_expected_results();
    let scalar = expect_table(&expected, "scalar_compat");
    with_binary_case_reader("scalar_compat", |reader, case| {
        assert_eq!(reader.read_row_count().unwrap(), 1);
        enter_binary_row(reader, 0);
        begin_binary_record(reader, &case.layout);
        assert_eq!(reader.as_i64().unwrap(), expect_i64(scalar, "id"));
        assert_eq!(reader.as_i64().unwrap(), expect_i64(scalar, "level"));
        assert_eq!(reader.as_f64().unwrap(), expect_f64(scalar, "ratio"));
        assert_eq!(
            reader.as_bool().unwrap(),
            expect_bool(scalar, "enabled_text")
        );
        assert_eq!(
            reader.as_bool().unwrap(),
            expect_bool(scalar, "disabled_text")
        );
        assert_eq!(
            reader.as_bool().unwrap(),
            expect_bool(scalar, "enabled_bool")
        );
        assert_eq!(reader.as_string().unwrap(), expect_str(scalar, "name"));
        assert!(
            !binary_optional_present(reader, "nullable"),
            "binary should encode null optional field as absent"
        );
        end_binary_record(reader);
    });

    let escaped = expect_table(&expected, "escaped_string");
    with_binary_case_reader("escaped_string", |reader, case| {
        assert_eq!(reader.read_row_count().unwrap(), 1);
        enter_binary_row(reader, 0);
        begin_binary_record(reader, &case.layout);
        assert_eq!(reader.as_string().unwrap(), expect_str(escaped, "text"));
        end_binary_record(reader);
    });

    let collection = expect_table(&expected, "collection_envelope");
    with_binary_case_reader("collection_envelope", |reader, case| {
        assert_eq!(
            reader.read_row_count().unwrap(),
            usize::try_from(expect_i64(collection, "row_count")).unwrap()
        );
        enter_binary_row(reader, 0);
        begin_binary_record(reader, &case.layout);
        assert_eq!(reader.as_i64().unwrap(), expect_i64(collection, "first_id"));
        assert_eq!(reader.as_string().unwrap(), "Sword");
        assert_eq!(
            reader.read_list_len().unwrap(),
            usize::try_from(expect_i64(collection, "tag_count")).unwrap()
        );
        assert_eq!(
            reader.as_string().unwrap(),
            expect_str(collection, "first_tag")
        );
        assert_eq!(reader.as_string().unwrap(), "starter");
        end_binary_record(reader);

        enter_binary_row(reader, 1);
        begin_binary_record(reader, &case.layout);
        assert_eq!(reader.as_i64().unwrap(), 1002);
        assert_eq!(
            reader.as_string().unwrap(),
            expect_str(collection, "second_name")
        );
        assert_eq!(reader.read_list_len().unwrap(), 1);
        assert_eq!(reader.as_string().unwrap(), "armor");
        end_binary_record(reader);
    });

    let object = expect_table(&expected, "object_table");
    with_binary_case_reader("object_table", |reader, case| {
        begin_binary_record(reader, &case.layout);
        assert_eq!(
            reader.as_i64().unwrap(),
            expect_i64(object, "default_class_id")
        );
        assert_eq!(reader.as_bool().unwrap(), expect_bool(object, "enabled"));
        assert_eq!(reader.as_string().unwrap(), expect_str(object, "title"));
        end_binary_record(reader);
    });

    let nested = expect_table(&expected, "nested_struct");
    with_binary_case_reader("nested_struct", |reader, case| {
        assert_eq!(reader.read_row_count().unwrap(), 1);
        enter_binary_row(reader, 0);
        begin_binary_record(reader, &case.layout);
        assert_eq!(reader.as_i64().unwrap(), expect_i64(nested, "item_id"));
        let BinaryValueLayout::Struct(reward_layout) = &case.layout.fields[1].value else {
            panic!("nested reward should be struct");
        };
        begin_binary_record(reader, reward_layout);
        assert_eq!(
            reader.as_i64().unwrap(),
            expect_i64(nested, "reward_item_id")
        );
        assert_eq!(reader.as_i64().unwrap(), expect_i64(nested, "reward_count"));
        end_binary_record(reader);
        assert_eq!(
            reader.read_list_len().unwrap(),
            usize::try_from(expect_i64(nested, "attribute_count")).unwrap()
        );
        assert_eq!(
            reader.as_string().unwrap(),
            expect_str(nested, "first_attribute")
        );
        assert_eq!(reader.as_string().unwrap(), "rare");
        end_binary_record(reader);
    });

    let fragment = expect_table(&expected, "fragment");
    with_binary_case_reader("fragment", |reader, case| {
        assert_eq!(reader.read_row_count().unwrap(), 1);
        enter_binary_row(reader, 0);
        begin_binary_record(reader, &case.layout);
        assert_eq!(reader.as_i64().unwrap(), expect_i64(fragment, "item_id"));
        let BinaryValueLayout::Struct(effect_layout) = &case.layout.fields[1].value else {
            panic!("fragment effect should be struct");
        };
        begin_binary_record(reader, effect_layout);
        assert_eq!(
            reader.as_string().unwrap(),
            expect_str(fragment, "fragment_kind")
        );
        assert_eq!(
            reader.as_i64().unwrap(),
            expect_i64(fragment, "fragment_amount")
        );
        end_binary_record(reader);
        end_binary_record(reader);
    });
}

fn run_binary_malformed_cases() {
    let scalar_case = binary_success_cases()
        .into_iter()
        .find(|case| case.name == "scalar_compat")
        .expect("scalar binary case should exist");

    expect_binary_decode_failure(
        "bad magic",
        {
            let mut bytes = materialize_binary_case(&scalar_case);
            bytes[0] = b'X';
            bytes
        },
        "magic",
        |_| Ok(()),
    );
    expect_binary_decode_failure(
        "unsupported version",
        {
            let mut bytes = materialize_binary_case(&scalar_case);
            bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
            bytes
        },
        "version",
        |_| Ok(()),
    );
    expect_binary_decode_failure(
        "wrong header size",
        {
            let mut bytes = materialize_binary_case(&scalar_case);
            bytes[6..8].copy_from_slice(&12u16.to_le_bytes());
            bytes
        },
        "header size",
        |_| Ok(()),
    );
    expect_binary_decode_failure("truncated header", b"CFGB".to_vec(), "header", |_| Ok(()));
    expect_binary_operation_failure(
        "schema revision mismatch",
        materialize_binary_case(&scalar_case),
        "schema revision",
        |reader| {
            require_binary_schema(reader, binary_schema_revision(&scalar_case).wrapping_add(1))
        },
    );
    expect_binary_operation_failure(
        "truncated varuint",
        {
            let mut bytes = Vec::new();
            write_binary_header(&mut bytes, 1);
            bytes.push(0x80);
            bytes
        },
        "truncated",
        |reader| reader.read_row_count().map(|_| ()),
    );
    expect_binary_operation_failure(
        "varuint overflow",
        {
            let mut bytes = Vec::new();
            write_binary_header(&mut bytes, 1);
            bytes.extend_from_slice(&[0x80; 9]);
            bytes.push(0x02);
            bytes
        },
        "u64",
        |reader| reader.read_row_count().map(|_| ()),
    );
    expect_binary_operation_failure(
        "invalid bool",
        {
            let mut bytes = Vec::new();
            write_binary_header(&mut bytes, 1);
            bytes.push(0x02);
            bytes
        },
        "bool",
        |reader| reader.as_bool().map(|_| ()),
    );
    expect_binary_operation_failure(
        "invalid utf8",
        {
            let mut bytes = Vec::new();
            write_binary_header(&mut bytes, 1);
            write_varuint(&mut bytes, 1);
            bytes.push(0xff);
            bytes
        },
        "UTF-8",
        |reader| reader.as_string().map(|_| ()),
    );
    expect_binary_operation_failure(
        "invalid optional bitmap",
        {
            let mut bytes = Vec::new();
            write_binary_header(&mut bytes, 1);
            write_varuint(&mut bytes, 1);
            bytes.push(0x80);
            bytes
        },
        "bitmap",
        |reader| {
            let _ = reader.read_row_count()?;
            reader.begin_record(1)
        },
    );
    expect_binary_operation_failure(
        "enum ordinal out of range",
        {
            let mut bytes = Vec::new();
            write_binary_header(&mut bytes, 1);
            write_varuint(&mut bytes, 2);
            bytes
        },
        "ordinal",
        |reader| reader.read_enum_index(&["a", "b"]).map(|_| ()),
    );
    expect_binary_operation_failure(
        "trailing bytes",
        {
            let mut bytes = Vec::new();
            write_binary_header(&mut bytes, 1);
            write_varuint(&mut bytes, 0);
            bytes.push(0);
            bytes
        },
        "trailing",
        |reader| {
            let _ = reader.read_row_count()?;
            reader.ensure_exhausted()
        },
    );
}

fn require_binary_schema(
    reader: &binary_runtime_reader::BinaryRuntimeDataReader,
    expected_revision: u64,
) -> RuntimeDataResult<()> {
    if reader.schema_revision() == Some(expected_revision) {
        Ok(())
    } else {
        Err(RuntimeDataError::decode(format!(
            "schema revision mismatch: expected `{expected_revision}`, got `{:?}`",
            reader.schema_revision()
        )))
    }
}

fn expect_binary_decode_failure(
    label: &str,
    bytes: Vec<u8>,
    keyword: &str,
    operation: impl FnOnce(&binary_runtime_reader::BinaryRuntimeDataReader) -> RuntimeDataResult<()>,
) {
    match binary_runtime_reader::BinaryRuntimeDataReader::from_bytes(bytes) {
        Ok(reader) => {
            let error = operation(&reader).expect_err("binary operation should fail");
            assert!(
                error.message.contains(keyword),
                "binary malformed case `{label}` error `{}` should contain `{keyword}`",
                error.message
            );
        }
        Err(error) => assert!(
            error.message.contains(keyword),
            "binary malformed case `{label}` error `{}` should contain `{keyword}`",
            error.message
        ),
    }
}

fn expect_binary_operation_failure(
    label: &str,
    bytes: Vec<u8>,
    keyword: &str,
    operation: impl FnOnce(&binary_runtime_reader::BinaryRuntimeDataReader) -> RuntimeDataResult<()>,
) {
    let reader = binary_runtime_reader::BinaryRuntimeDataReader::from_bytes(bytes)
        .expect("malformed operation case should have valid header");
    let error = operation(&reader).expect_err("binary operation should fail");
    assert!(
        error.message.contains(keyword),
        "binary malformed case `{label}` error `{}` should contain `{keyword}`",
        error.message
    );
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

#[test]
fn rust_binary_adapter_passes_schema_derived_success_cases() {
    run_binary_success_cases();
}

#[test]
fn rust_binary_adapter_passes_malformed_corpus() {
    run_binary_malformed_cases();
}
