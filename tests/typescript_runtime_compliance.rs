// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path, process::Command};

#[path = "cli_integration/support/commands.rs"]
mod commands;

use commands::{
    assert_command_success, assert_optional_skips_within_limit, command_exists,
    current_test_profile, network_allowed, require_any_tool_or_skip, require_tool_or_skip,
    skip_optional_test, skipped_optional_test_count,
};

const TEST_NAME: &str = "TypeScript runtime compliance runner";

#[test]
fn typescript_json_and_messagepack_readers_pass_compliance_cases() {
    let skips_before = skipped_optional_test_count();
    let mut executed = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_typescript_runtime_compliance_cases()
    }));
    if result.is_err() {
        failed += 1;
    } else if skipped_optional_test_count() > skips_before {
        skipped += 1;
    } else {
        executed += 1;
    }

    eprintln!(
        "runtime compliance summary: suite=typescript, executed={executed}, skipped={skipped}, failed={failed}"
    );
    assert_optional_skips_within_limit("typescript runtime compliance", skipped);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn run_typescript_runtime_compliance_cases() {
    if require_tool_or_skip(TEST_NAME, "node").is_none() {
        return;
    }

    let has_tsc = command_exists("tsc");
    let has_npx = command_exists("npx");
    if !has_tsc && current_test_profile().requires_optional_tools() {
        let _ = require_tool_or_skip(TEST_NAME, "tsc");
        return;
    }
    if !has_tsc && has_npx && !network_allowed() {
        skip_optional_test(
            TEST_NAME,
            "npx fallback requires CONFIG_FORGE_ALLOW_NETWORK=1",
        );
        return;
    }
    if !has_tsc && !has_npx && require_any_tool_or_skip(TEST_NAME, &["tsc", "npx"]).is_none() {
        return;
    }

    let temp_dir = tempfile::tempdir().expect("typescript compliance temp dir should be created");
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("typescript compliance src dir should be created");

    copy_runtime_sources(&src_dir);
    write_compliance_data(&src_dir);
    fs::write(src_dir.join("runner.ts"), runner_source())
        .expect("typescript compliance runner should be written");
    fs::write(temp_dir.path().join("tsconfig.json"), tsconfig_source())
        .expect("typescript compliance tsconfig should be written");

    let tsc_output = if has_tsc {
        Command::new("tsc")
            .args(["--project"])
            .arg(temp_dir.path().join("tsconfig.json"))
            .output()
            .expect("typescript compliance tsc should run")
    } else {
        Command::new("npx")
            .args(["--yes", "-p", "typescript", "tsc", "--project"])
            .arg(temp_dir.path().join("tsconfig.json"))
            .output()
            .expect("typescript compliance npx tsc should run")
    };
    assert_command_success(tsc_output, "TypeScript runtime compliance tsc");

    let node_output = Command::new("node")
        .arg(temp_dir.path().join("dist/runner.js"))
        .output()
        .expect("typescript compliance node runner should run");
    assert_command_success(node_output, "TypeScript runtime compliance node runner");
}

fn copy_runtime_sources(src_dir: &Path) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    fs::copy(
        root.join("runtime/typescript/reader_core/src/runtime_reader.ts"),
        src_dir.join("reader_core.ts"),
    )
    .expect("TypeScript reader core source should be copied");

    let json_reader =
        fs::read_to_string(root.join("runtime/typescript/json_reader/src/runtime_loader.ts"))
            .expect("TypeScript JSON reader source should be read");
    let json_reader = json_reader.replace(
        "} from \"@config-forge/runtime-reader-core\";",
        "} from \"./reader_core\";",
    );
    fs::write(src_dir.join("json_reader.ts"), json_reader)
        .expect("TypeScript JSON reader source should be written");

    let messagepack_reader = fs::read_to_string(
        root.join("runtime/typescript/messagepack_reader/src/runtime_loader.ts"),
    )
    .expect("TypeScript MessagePack reader source should be read");
    let messagepack_reader = messagepack_reader.replace(
        "} from \"@config-forge/runtime-reader-core\";",
        "} from \"./reader_core\";",
    );
    fs::write(src_dir.join("messagepack_reader.ts"), messagepack_reader)
        .expect("TypeScript MessagePack reader source should be written");
}

fn write_compliance_data(src_dir: &Path) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let payload_dir = root.join("tests/fixtures/runtime_reader_compliance/payloads");
    let mut payloads = BTreeMap::new();
    for name in [
        "scalar_compat.json",
        "escaped_string.json",
        "collection_envelope.json",
        "object_table.json",
        "nested_struct.json",
        "fragment.json",
        "missing_vs_null.json",
    ] {
        let content = fs::read_to_string(payload_dir.join(name))
            .unwrap_or_else(|error| panic!("failed to read compliance payload `{name}`: {error}"));
        payloads.insert(name.to_owned(), content);
    }

    let expected = read_toml_as_json(
        root.join("tests/fixtures/runtime_reader_compliance/expected/scalar_results.toml"),
    );
    let errors = read_toml_as_json(
        root.join("tests/fixtures/runtime_reader_compliance/expected/error_cases.toml"),
    );

    let payloads_json =
        serde_json::to_string_pretty(&payloads).expect("compliance payload map should serialize");
    let expected_json = serde_json::to_string_pretty(&expected)
        .expect("compliance expected results should serialize");
    let errors_json =
        serde_json::to_string_pretty(&errors).expect("compliance error cases should serialize");

    fs::write(
        src_dir.join("compliance_data.ts"),
        format!(
            "// Generated by tests/typescript_runtime_compliance.rs\n\
             export const payloads = {payloads_json} as const;\n\
             export const expected = {expected_json} as const;\n\
             export const errorCases = {errors_json} as const;\n"
        ),
    )
    .expect("TypeScript compliance data should be written");
}

fn read_toml_as_json(path: impl AsRef<Path>) -> serde_json::Value {
    let content = fs::read_to_string(path.as_ref()).unwrap_or_else(|error| {
        panic!("failed to read TOML `{}`: {error}", path.as_ref().display())
    });
    let value = content.parse::<toml::Table>().unwrap_or_else(|error| {
        panic!(
            "failed to parse TOML `{}`: {error}",
            path.as_ref().display()
        )
    });
    serde_json::to_value(value).expect("TOML value should serialize to JSON")
}

fn tsconfig_source() -> &'static str {
    r#"
{
  "compilerOptions": {
    "strict": true,
    "target": "ES2020",
    "module": "Node16",
    "moduleResolution": "Node16",
    "rootDir": "src",
    "outDir": "dist",
    "skipLibCheck": true
  },
  "include": ["src/**/*.ts"]
}
"#
}

fn runner_source() -> &'static str {
    r#"
import { decodeJsonValue } from "./json_reader";
import { decodeMessagePackValue } from "./messagepack_reader";
import { RuntimeDataReaderLike } from "./reader_core";
import { payloads, expected, errorCases } from "./compliance_data";

type Step = { kind: "field"; name: string } | { kind: "index"; index: number };
type ExpectedTable = Record<string, string | number | boolean>;
type ReaderFactory = (payload: keyof typeof payloads) => RuntimeDataReaderLike;
type Adapter = { name: string; readerFor: ReaderFactory };

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder("utf-8", { fatal: true });

const adapters: readonly Adapter[] = [
  { name: "typescript-json", readerFor: (payload) => decodeJsonValue(payloads[payload]) },
  {
    name: "typescript-messagepack",
    readerFor: (payload) => decodeMessagePackValue(encodeMessagePack(JSON.parse(payloads[payload])), decodeMessagePack),
  },
];

function field(name: string): Step { return { kind: "field", name }; }
function index(value: number): Step { return { kind: "index", index: value }; }

function enterPath(reader: RuntimeDataReaderLike, steps: readonly Step[]): Step[] {
  const entered: Step[] = [];
  try {
    for (const step of steps) {
      if (step.kind === "field") {
        reader.enterRequiredField(step.name);
      } else {
        reader.enterIndex(step.index);
      }
      entered.push(step);
    }
    return entered;
  } catch (error) {
    exitPath(reader, entered);
    throw error;
  }
}

function exitPath(reader: RuntimeDataReaderLike, entered: readonly Step[]): void {
  for (let i = entered.length - 1; i >= 0; i -= 1) {
    if (entered[i].kind === "field") {
      reader.exitField();
    } else {
      reader.exit();
    }
  }
}

function withPath<T>(reader: RuntimeDataReaderLike, steps: readonly Step[], read: () => T): T {
  const entered = enterPath(reader, steps);
  try {
    return read();
  } finally {
    exitPath(reader, entered);
  }
}

function assertEqual<T>(actual: T, expectedValue: T, label: string): void {
  if (actual !== expectedValue) {
    throw new Error(`${label}: expected ${String(expectedValue)}, got ${String(actual)}`);
  }
}

function assertMessageContains(error: unknown, keyword: string, label: string): void {
  const message = error instanceof Error ? error.message : String(error);
  if (!message.includes(keyword)) {
    throw new Error(`${label}: error message ${JSON.stringify(message)} does not contain ${JSON.stringify(keyword)}`);
  }
}

function table(name: keyof typeof expected): ExpectedTable {
  return expected[name] as ExpectedTable;
}

function readInt(reader: RuntimeDataReaderLike, steps: readonly Step[]): number {
  return withPath(reader, steps, () => reader.asInt64());
}

function readFloat(reader: RuntimeDataReaderLike, steps: readonly Step[]): number {
  return withPath(reader, steps, () => reader.asFloat64());
}

function readBool(reader: RuntimeDataReaderLike, steps: readonly Step[]): boolean {
  return withPath(reader, steps, () => reader.asBool());
}

function readString(reader: RuntimeDataReaderLike, steps: readonly Step[]): string {
  return withPath(reader, steps, () => reader.asString());
}

function readRowCount(reader: RuntimeDataReaderLike, steps: readonly Step[]): number {
  return withPath(reader, steps, () => reader.readRowCount());
}

function readListLength(reader: RuntimeDataReaderLike, steps: readonly Step[]): number {
  return withPath(reader, steps, () => reader.readListLength());
}

function runScalarAndStringCases(adapter: Adapter): void {
  const scalar = table("scalar_compat");
  const row = [field("rows"), index(0)] as const;
  const reader = adapter.readerFor("scalar_compat.json");
  assertEqual(readInt(reader, [...row, field("id")]), scalar.id as number, `${adapter.name} int scalar`);
  assertEqual(readInt(reader, [...row, field("level")]), scalar.level as number, `${adapter.name} numeric string int scalar`);
  assertEqual(readFloat(reader, [...row, field("ratio")]), scalar.ratio as number, `${adapter.name} float scalar`);
  assertEqual(readBool(reader, [...row, field("enabled_text")]), scalar.enabled_text as boolean, `${adapter.name} true-like bool string`);
  assertEqual(readBool(reader, [...row, field("disabled_text")]), scalar.disabled_text as boolean, `${adapter.name} false-like bool string`);
  assertEqual(readBool(reader, [...row, field("enabled_bool")]), scalar.enabled_bool as boolean, `${adapter.name} bool scalar`);
  assertEqual(readString(reader, [...row, field("name")]), scalar.name as string, `${adapter.name} string scalar`);

  const entered = enterPath(reader, row);
  try {
    assertEqual(reader.enterOptionalField("nullable"), false, `${adapter.name} null optional field should be absent`);
  } finally {
    exitPath(reader, entered);
  }

  const escaped = table("escaped_string");
  const escapedReader = adapter.readerFor("escaped_string.json");
  assertEqual(
    readString(escapedReader, [field("rows"), index(0), field("text")]),
    escaped.text as string,
    `${adapter.name} escaped string`,
  );
}

function runCollectionObjectNestedAndFragmentCases(adapter: Adapter): void {
  const collection = table("collection_envelope");
  const collectionReader = adapter.readerFor("collection_envelope.json");
  assertEqual(readString(collectionReader, [field("meta"), field("name")]), collection.table_name as string, `${adapter.name} collection metadata`);
  assertEqual(readRowCount(collectionReader, [field("rows")]), collection.row_count as number, `${adapter.name} collection row count`);
  assertEqual(readInt(collectionReader, [field("rows"), index(0), field("id")]), collection.first_id as number, `${adapter.name} first row id`);
  assertEqual(readString(collectionReader, [field("rows"), index(1), field("name")]), collection.second_name as string, `${adapter.name} second row name`);
  assertEqual(readListLength(collectionReader, [field("rows"), index(0), field("tags")]), collection.tag_count as number, `${adapter.name} tag count`);
  assertEqual(readString(collectionReader, [field("rows"), index(0), field("tags"), index(0)]), collection.first_tag as string, `${adapter.name} first tag`);

  const object = table("object_table");
  const objectReader = adapter.readerFor("object_table.json");
  assertEqual(readString(objectReader, [field("meta"), field("name")]), object.table_name as string, `${adapter.name} object table metadata`);
  assertEqual(readInt(objectReader, [field("default_class_id")]), object.default_class_id as number, `${adapter.name} object table int field`);
  assertEqual(readBool(objectReader, [field("enabled")]), object.enabled as boolean, `${adapter.name} object table bool field`);
  assertEqual(readString(objectReader, [field("title")]), object.title as string, `${adapter.name} object table string field`);

  const nested = table("nested_struct");
  const nestedReader = adapter.readerFor("nested_struct.json");
  assertEqual(readInt(nestedReader, [field("rows"), index(0), field("item_id")]), nested.item_id as number, `${adapter.name} nested row id`);
  assertEqual(readInt(nestedReader, [field("rows"), index(0), field("reward"), field("count")]), nested.reward_count as number, `${adapter.name} nested reward count`);
  assertEqual(readInt(nestedReader, [field("rows"), index(0), field("reward"), field("item_id")]), nested.reward_item_id as number, `${adapter.name} nested reward item id`);
  assertEqual(readListLength(nestedReader, [field("rows"), index(0), field("attributes")]), nested.attribute_count as number, `${adapter.name} nested attribute count`);
  assertEqual(readString(nestedReader, [field("rows"), index(0), field("attributes"), index(0)]), nested.first_attribute as string, `${adapter.name} nested first attribute`);

  const fragment = table("fragment");
  const fragmentReader = adapter.readerFor("fragment.json");
  assertEqual(readInt(fragmentReader, [field("rows"), index(0), field("item_id")]), fragment.item_id as number, `${adapter.name} fragment row id`);
  assertEqual(readString(fragmentReader, [field("rows"), index(0), field("effect"), field("kind")]), fragment.fragment_kind as string, `${adapter.name} fragment kind`);
  assertEqual(readInt(fragmentReader, [field("rows"), index(0), field("effect"), field("amount")]), fragment.fragment_amount as number, `${adapter.name} fragment amount`);
}

function expectFailure(label: string, operation: () => void, keyword: string): void {
  try {
    operation();
  } catch (error) {
    assertMessageContains(error, keyword, label);
    return;
  }
  throw new Error(`${label}: expected failure`);
}

function expectedKeyword(caseName: string, fallback: string): string {
  if (caseName === "int_overflow") {
    return "safe integer";
  }
  if (caseName === "invalid_bool_string") {
    return "bool-like";
  }
  return fallback;
}

function runErrorCases(adapter: Adapter): void {
  const cases = ((errorCases as unknown as { cases: ReadonlyArray<Record<string, string>> }).cases)
    .filter((testCase) => testCase.format === undefined || testCase.format === adapter.name.replace("typescript-", ""));
  let executed = 0;
  for (const testCase of cases) {
    if (testCase.operation === "decode") {
      if (testCase.name === "messagepack_non_string_map_key") {
        expectFailure(`${adapter.name} ${testCase.name}`, () => decodeMessagePackValue(new Uint8Array([0x81, 0x01, 0x01]), decodeMessagePack), testCase.expected_message_contains);
      } else if (testCase.name === "messagepack_binary_ext") {
        expectFailure(`${adapter.name} ${testCase.name}`, () => decodeMessagePackValue(new Uint8Array([0xc4, 0x01, 0x00]), decodeMessagePack), testCase.expected_message_contains);
      } else {
        throw new Error(`unhandled ${adapter.name} compliance decode error case ${testCase.name}`);
      }
      executed += 1;
      continue;
    }
    const reader = adapter.readerFor(testCase.payload as keyof typeof payloads);
    const keyword = expectedKeyword(testCase.name, testCase.expected_message_contains);
    switch (testCase.name) {
      case "missing_field": {
        const entered = enterPath(reader, [field("rows"), index(0)]);
        try {
          expectFailure(testCase.name, () => reader.enterRequiredField("missing"), keyword);
        } finally {
          exitPath(reader, entered);
        }
        break;
      }
      case "explicit_null_required_field": {
        const entered = enterPath(reader, [field("rows"), index(0)]);
        try {
          expectFailure(testCase.name, () => reader.enterRequiredField("explicit_null"), keyword);
        } finally {
          exitPath(reader, entered);
        }
        break;
      }
      case "array_index_out_of_bounds": {
        const entered = enterPath(reader, [field("rows")]);
        try {
          expectFailure(testCase.name, () => reader.enterIndex(99), keyword);
        } finally {
          exitPath(reader, entered);
        }
        break;
      }
      case "int_overflow":
        expectFailure(testCase.name, () => {
          withPath(reader, [field("rows"), index(0), field("overflow_int")], () => reader.asInt64());
        }, keyword);
        break;
      case "invalid_bool_string":
        expectFailure(testCase.name, () => {
          withPath(reader, [field("rows"), index(0), field("invalid_bool")], () => reader.asBool());
        }, keyword);
        break;
      case "non_finite_float_string":
        expectFailure(testCase.name, () => {
          withPath(reader, [field("rows"), index(0), field("non_finite_float")], () => reader.asFloat64());
        }, keyword);
        break;
      default:
        throw new Error(`unhandled ${adapter.name} compliance error case ${testCase.name}`);
    }
    executed += 1;
  }
  assertEqual(executed, adapter.name === "typescript-messagepack" ? 8 : 6, `${adapter.name} error case count`);
}

function encodeMessagePack(value: unknown): Uint8Array {
  const bytes: number[] = [];
  writeMessagePack(value, bytes);
  return new Uint8Array(bytes);
}

function writeMessagePack(value: unknown, bytes: number[]): void {
  if (value === null) {
    bytes.push(0xc0);
  } else if (typeof value === "boolean") {
    bytes.push(value ? 0xc3 : 0xc2);
  } else if (typeof value === "number") {
    if (Number.isSafeInteger(value)) {
      writeInteger(value, bytes);
    } else {
      bytes.push(0xcb);
      const buffer = new ArrayBuffer(8);
      new DataView(buffer).setFloat64(0, value, false);
      bytes.push(...new Uint8Array(buffer));
    }
  } else if (typeof value === "string") {
    writeString(value, bytes);
  } else if (Array.isArray(value)) {
    writeArrayHeader(value.length, bytes);
    for (const element of value) {
      writeMessagePack(element, bytes);
    }
  } else if (typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    writeMapHeader(entries.length, bytes);
    for (const [key, element] of entries) {
      writeString(key, bytes);
      writeMessagePack(element, bytes);
    }
  } else {
    throw new Error(`unsupported MessagePack encode value ${String(value)}`);
  }
}

function writeInteger(value: number, bytes: number[]): void {
  if (value >= 0 && value <= 0x7f) {
    bytes.push(value);
  } else if (value >= 0 && value <= 0xff) {
    bytes.push(0xcc, value);
  } else if (value >= 0 && value <= 0xffff) {
    bytes.push(0xcd, (value >>> 8) & 0xff, value & 0xff);
  } else if (value >= 0 && value <= 0xffffffff) {
    bytes.push(0xce, (value / 0x1000000) & 0xff, (value >>> 16) & 0xff, (value >>> 8) & 0xff, value & 0xff);
  } else if (value >= -32 && value < 0) {
    bytes.push(0x100 + value);
  } else if (value >= -0x80 && value < 0) {
    bytes.push(0xd0, value & 0xff);
  } else if (value >= -0x8000 && value < 0) {
    bytes.push(0xd1, (value >> 8) & 0xff, value & 0xff);
  } else {
    bytes.push(0xcb);
    const buffer = new ArrayBuffer(8);
    new DataView(buffer).setFloat64(0, value, false);
    bytes.push(...new Uint8Array(buffer));
  }
}

function writeString(value: string, bytes: number[]): void {
  const encoded = textEncoder.encode(value);
  if (encoded.length <= 31) {
    bytes.push(0xa0 | encoded.length);
  } else if (encoded.length <= 0xff) {
    bytes.push(0xd9, encoded.length);
  } else if (encoded.length <= 0xffff) {
    bytes.push(0xda, (encoded.length >>> 8) & 0xff, encoded.length & 0xff);
  } else {
    throw new Error("test MessagePack string is too large");
  }
  bytes.push(...encoded);
}

function writeArrayHeader(length: number, bytes: number[]): void {
  if (length <= 15) {
    bytes.push(0x90 | length);
  } else if (length <= 0xffff) {
    bytes.push(0xdc, (length >>> 8) & 0xff, length & 0xff);
  } else {
    throw new Error("test MessagePack array is too large");
  }
}

function writeMapHeader(length: number, bytes: number[]): void {
  if (length <= 15) {
    bytes.push(0x80 | length);
  } else if (length <= 0xffff) {
    bytes.push(0xde, (length >>> 8) & 0xff, length & 0xff);
  } else {
    throw new Error("test MessagePack map is too large");
  }
}

function decodeMessagePack(bytes: Uint8Array): unknown {
  const decoder = new MessagePackDecoder(bytes);
  const value = decoder.readValue();
  decoder.ensureExhausted();
  return value;
}

class MessagePackDecoder {
  private offset = 0;

  constructor(private readonly bytes: Uint8Array) {}

  readValue(): unknown {
    const tag = this.readU8();
    if (tag <= 0x7f) return tag;
    if (tag >= 0xe0) return tag - 0x100;
    if ((tag & 0xe0) === 0xa0) return this.readString(tag & 0x1f);
    if ((tag & 0xf0) === 0x90) return this.readArray(tag & 0x0f);
    if ((tag & 0xf0) === 0x80) return this.readMap(tag & 0x0f);
    switch (tag) {
      case 0xc0: return null;
      case 0xc2: return false;
      case 0xc3: return true;
      case 0xc4:
      case 0xc5:
      case 0xc6:
      case 0xc7:
      case 0xc8:
      case 0xc9:
        throw new Error("unsupported binary/ext value");
      case 0xca: return this.readFloat32();
      case 0xcb: return this.readFloat64();
      case 0xcc: return this.readU8();
      case 0xcd: return this.readU16();
      case 0xce: return this.readU32();
      case 0xd0: return this.readI8();
      case 0xd1: return this.readI16();
      case 0xd9: return this.readString(this.readU8());
      case 0xda: return this.readString(this.readU16());
      case 0xdc: return this.readArray(this.readU16());
      case 0xde: return this.readMap(this.readU16());
      default:
        throw new Error(`unsupported MessagePack tag 0x${tag.toString(16)}`);
    }
  }

  ensureExhausted(): void {
    if (this.offset !== this.bytes.length) {
      throw new Error("trailing MessagePack bytes");
    }
  }

  private readArray(length: number): unknown[] {
    return Array.from({ length }, () => this.readValue());
  }

  private readMap(length: number): Record<string, unknown> {
    const value: Record<string, unknown> = {};
    for (let index = 0; index < length; index += 1) {
      const key = this.readValue();
      if (typeof key !== "string") {
        throw new Error("unsupported non-string map key");
      }
      value[key] = this.readValue();
    }
    return value;
  }

  private readString(length: number): string {
    return textDecoder.decode(this.readExact(length));
  }

  private readFloat32(): number {
    const bytes = this.readExact(4);
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getFloat32(0, false);
  }

  private readFloat64(): number {
    const bytes = this.readExact(8);
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getFloat64(0, false);
  }

  private readU8(): number { return this.readExact(1)[0]; }
  private readU16(): number { const bytes = this.readExact(2); return (bytes[0] << 8) | bytes[1]; }
  private readU32(): number { const bytes = this.readExact(4); return bytes[0] * 0x1000000 + ((bytes[1] << 16) | (bytes[2] << 8) | bytes[3]); }
  private readI8(): number { const value = this.readU8(); return value & 0x80 ? value - 0x100 : value; }
  private readI16(): number { const value = this.readU16(); return value & 0x8000 ? value - 0x10000 : value; }

  private readExact(length: number): Uint8Array {
    if (this.offset + length > this.bytes.length) {
      throw new Error("truncated MessagePack payload");
    }
    const value = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return value;
  }
}

for (const adapter of adapters) {
  runScalarAndStringCases(adapter);
  runCollectionObjectNestedAndFragmentCases(adapter);
  runErrorCases(adapter);
}
"#
}
