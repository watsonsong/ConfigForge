// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn network_allowed() -> bool {
    std::env::var("CONFIG_FORGE_ALLOW_NETWORK")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn assert_command_success(output: std::process::Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn typescript_json_reader_passes_compliance_cases() {
    if !command_exists("node") {
        eprintln!("skipping TypeScript JSON compliance runner: node not found");
        return;
    }
    if !command_exists("tsc") && !command_exists("npx") {
        eprintln!("skipping TypeScript JSON compliance runner: tsc/npx not found");
        return;
    }
    if !command_exists("tsc") && command_exists("npx") && !network_allowed() {
        eprintln!(
            "skipping TypeScript JSON compliance runner: npx fallback requires CONFIG_FORGE_ALLOW_NETWORK=1"
        );
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

    let tsc_output = if command_exists("tsc") {
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
    assert_command_success(tsc_output, "TypeScript JSON compliance tsc");

    let node_output = Command::new("node")
        .arg(temp_dir.path().join("dist/runner.js"))
        .output()
        .expect("typescript compliance node runner should run");
    assert_command_success(node_output, "TypeScript JSON compliance node runner");
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
import { RuntimeDataReaderLike } from "./reader_core";
import { payloads, expected, errorCases } from "./compliance_data";

type Step = { kind: "field"; name: string } | { kind: "index"; index: number };
type ExpectedTable = Record<string, string | number | boolean>;

function readerFor(payload: keyof typeof payloads): RuntimeDataReaderLike {
  return decodeJsonValue(payloads[payload]);
}

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

function runScalarAndStringCases(): void {
  const scalar = table("scalar_compat");
  const row = [field("rows"), index(0)] as const;
  const reader = readerFor("scalar_compat.json");
  assertEqual(readInt(reader, [...row, field("id")]), scalar.id as number, "int scalar");
  assertEqual(readInt(reader, [...row, field("level")]), scalar.level as number, "numeric string int scalar");
  assertEqual(readFloat(reader, [...row, field("ratio")]), scalar.ratio as number, "float scalar");
  assertEqual(readBool(reader, [...row, field("enabled_text")]), scalar.enabled_text as boolean, "true-like bool string");
  assertEqual(readBool(reader, [...row, field("disabled_text")]), scalar.disabled_text as boolean, "false-like bool string");
  assertEqual(readBool(reader, [...row, field("enabled_bool")]), scalar.enabled_bool as boolean, "bool scalar");
  assertEqual(readString(reader, [...row, field("name")]), scalar.name as string, "string scalar");

  const entered = enterPath(reader, row);
  try {
    assertEqual(reader.enterOptionalField("nullable"), false, "null optional field should be absent");
  } finally {
    exitPath(reader, entered);
  }

  const escaped = table("escaped_string");
  const escapedReader = readerFor("escaped_string.json");
  assertEqual(
    readString(escapedReader, [field("rows"), index(0), field("text")]),
    escaped.text as string,
    "escaped string",
  );
}

function runCollectionObjectNestedAndFragmentCases(): void {
  const collection = table("collection_envelope");
  const collectionReader = readerFor("collection_envelope.json");
  assertEqual(readString(collectionReader, [field("meta"), field("name")]), collection.table_name as string, "collection metadata");
  assertEqual(readRowCount(collectionReader, [field("rows")]), collection.row_count as number, "collection row count");
  assertEqual(readInt(collectionReader, [field("rows"), index(0), field("id")]), collection.first_id as number, "first row id");
  assertEqual(readString(collectionReader, [field("rows"), index(1), field("name")]), collection.second_name as string, "second row name");
  assertEqual(readListLength(collectionReader, [field("rows"), index(0), field("tags")]), collection.tag_count as number, "tag count");
  assertEqual(readString(collectionReader, [field("rows"), index(0), field("tags"), index(0)]), collection.first_tag as string, "first tag");

  const object = table("object_table");
  const objectReader = readerFor("object_table.json");
  assertEqual(readString(objectReader, [field("meta"), field("name")]), object.table_name as string, "object table metadata");
  assertEqual(readInt(objectReader, [field("default_class_id")]), object.default_class_id as number, "object table int field");
  assertEqual(readBool(objectReader, [field("enabled")]), object.enabled as boolean, "object table bool field");
  assertEqual(readString(objectReader, [field("title")]), object.title as string, "object table string field");

  const nested = table("nested_struct");
  const nestedReader = readerFor("nested_struct.json");
  assertEqual(readInt(nestedReader, [field("rows"), index(0), field("item_id")]), nested.item_id as number, "nested row id");
  assertEqual(readInt(nestedReader, [field("rows"), index(0), field("reward"), field("count")]), nested.reward_count as number, "nested reward count");
  assertEqual(readInt(nestedReader, [field("rows"), index(0), field("reward"), field("item_id")]), nested.reward_item_id as number, "nested reward item id");
  assertEqual(readListLength(nestedReader, [field("rows"), index(0), field("attributes")]), nested.attribute_count as number, "nested attribute count");
  assertEqual(readString(nestedReader, [field("rows"), index(0), field("attributes"), index(0)]), nested.first_attribute as string, "nested first attribute");

  const fragment = table("fragment");
  const fragmentReader = readerFor("fragment.json");
  assertEqual(readInt(fragmentReader, [field("rows"), index(0), field("item_id")]), fragment.item_id as number, "fragment row id");
  assertEqual(readString(fragmentReader, [field("rows"), index(0), field("effect"), field("kind")]), fragment.fragment_kind as string, "fragment kind");
  assertEqual(readInt(fragmentReader, [field("rows"), index(0), field("effect"), field("amount")]), fragment.fragment_amount as number, "fragment amount");
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

function runErrorCases(): void {
  const cases = ((errorCases as unknown as { cases: ReadonlyArray<Record<string, string>> }).cases)
    .filter((testCase) => testCase.format === undefined || testCase.format === "json");
  let executed = 0;
  for (const testCase of cases) {
    if (testCase.format !== undefined) {
      continue;
    }
    const reader = readerFor(testCase.payload as keyof typeof payloads);
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
        throw new Error(`unhandled TypeScript JSON compliance error case ${testCase.name}`);
    }
    executed += 1;
  }
  assertEqual(executed, 6, "JSON-compatible error case count");
}

runScalarAndStringCases();
runCollectionObjectNestedAndFragmentCases();
runErrorCases();
"#
}
