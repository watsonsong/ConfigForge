// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::Path, process::Command};

#[path = "cli_integration/support/commands.rs"]
mod commands;

use commands::{
    assert_command_success, assert_optional_skips_within_limit, require_tool_or_skip,
    skipped_optional_test_count,
};

const CSHARP_TEST_NAME: &str = "C# runtime compliance runner";
const JAVA_TEST_NAME: &str = "Java runtime compliance runner";

#[test]
fn csharp_standard_readers_pass_runtime_compliance_cases() {
    run_compliance_case_with_summary("csharp", CSHARP_TEST_NAME, || {
        if require_tool_or_skip(CSHARP_TEST_NAME, "dotnet").is_none() {
            return;
        }
        run_csharp_runtime_compliance_cases();
    });
}

fn run_csharp_runtime_compliance_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().expect("C# compliance temp dir should be created");
    let project_dir = temp_dir.path();
    for (source, target) in [
        (
            "runtime/csharp/json_reader/JsonRuntimeDataReader.cs",
            "JsonRuntimeDataReader.cs",
        ),
        (
            "runtime/csharp/messagepack_reader/MessagePackRuntimeDataReader.cs",
            "MessagePackRuntimeDataReader.cs",
        ),
        (
            "runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs",
            "BinaryRuntimeDataReader.cs",
        ),
    ] {
        fs::copy(root.join(source), project_dir.join(target))
            .unwrap_or_else(|error| panic!("failed to copy {source}: {error}"));
    }
    fs::write(
        project_dir.join("ConfigForgeCSharpCompliance.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net6.0</TargetFramework>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
  </PropertyGroup>
</Project>
"#,
    )
    .expect("C# compliance csproj should be written");
    fs::write(project_dir.join("Program.cs"), csharp_runner_source())
        .expect("C# compliance Program.cs should be written");

    let output = Command::new("dotnet")
        .args(["run", "--project"])
        .arg(project_dir.join("ConfigForgeCSharpCompliance.csproj"))
        .env(
            "FIXTURE_DIR",
            root.join("tests/fixtures/runtime_reader_compliance/payloads"),
        )
        .output()
        .expect("C# compliance runner should run");
    assert_command_success(output, "C# runtime compliance runner");
}

#[test]
fn java_standard_readers_pass_runtime_compliance_cases() {
    run_compliance_case_with_summary("java", JAVA_TEST_NAME, || {
        if require_tool_or_skip(JAVA_TEST_NAME, "javac").is_none()
            || require_tool_or_skip(JAVA_TEST_NAME, "java").is_none()
        {
            return;
        }
        run_java_runtime_compliance_cases();
    });
}

fn run_java_runtime_compliance_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().expect("Java compliance temp dir should be created");
    let source_root = temp_dir.path().join("src");
    let class_dir = temp_dir.path().join("classes");
    let json_dir = source_root.join("configforge/runtime/json");
    let msgpack_dir = source_root.join("configforge/runtime/messagepack");
    let binary_dir = source_root.join("configforge/runtime/binary");
    fs::create_dir_all(&json_dir).expect("Java JSON dir should be created");
    fs::create_dir_all(&msgpack_dir).expect("Java MessagePack dir should be created");
    fs::create_dir_all(&binary_dir).expect("Java binary dir should be created");
    fs::create_dir_all(&class_dir).expect("Java class dir should be created");

    for file in [
        "ConfigForgeJsonParser.java",
        "ConfigForgeJsonRuntimeDataReader.java",
    ] {
        fs::copy(
            root.join("runtime/java/json_reader/src/configforge/runtime/json")
                .join(file),
            json_dir.join(file),
        )
        .expect("Java JSON source should be copied");
    }
    for file in [
        "ConfigForgeMessagePackParser.java",
        "ConfigForgeMessagePackRuntimeDataReader.java",
    ] {
        fs::copy(
            root.join("runtime/java/messagepack_reader/src/configforge/runtime/messagepack")
                .join(file),
            msgpack_dir.join(file),
        )
        .expect("Java MessagePack source should be copied");
    }
    fs::copy(
        root.join(
            "runtime/java/binary_reader/src/configforge/runtime/binary/ConfigForgeBinaryRuntimeDataReader.java",
        ),
        binary_dir.join("ConfigForgeBinaryRuntimeDataReader.java"),
    )
    .expect("Java binary source should be copied");
    fs::write(
        source_root.join("RuntimeCompliance.java"),
        java_runner_source(),
    )
    .expect("Java compliance source should be written");

    let output = Command::new("javac")
        .arg("-d")
        .arg(&class_dir)
        .arg(source_root.join("RuntimeCompliance.java"))
        .arg(json_dir.join("ConfigForgeJsonParser.java"))
        .arg(json_dir.join("ConfigForgeJsonRuntimeDataReader.java"))
        .arg(msgpack_dir.join("ConfigForgeMessagePackParser.java"))
        .arg(msgpack_dir.join("ConfigForgeMessagePackRuntimeDataReader.java"))
        .arg(binary_dir.join("ConfigForgeBinaryRuntimeDataReader.java"))
        .output()
        .expect("Java compliance javac should run");
    assert_command_success(output, "Java runtime compliance javac");

    let output = Command::new("java")
        .arg("-cp")
        .arg(&class_dir)
        .arg("RuntimeCompliance")
        .env(
            "FIXTURE_DIR",
            root.join("tests/fixtures/runtime_reader_compliance/payloads"),
        )
        .output()
        .expect("Java compliance runner should run");
    assert_command_success(output, "Java runtime compliance runner");
}

fn run_compliance_case_with_summary(
    suite: &str,
    scope: &str,
    runner: impl FnOnce() + std::panic::UnwindSafe,
) {
    let skips_before = skipped_optional_test_count();
    let mut executed = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    let result = std::panic::catch_unwind(runner);
    if result.is_err() {
        failed += 1;
    } else if skipped_optional_test_count() > skips_before {
        skipped += 1;
    } else {
        executed += 1;
    }

    eprintln!(
        "runtime compliance summary: suite={suite}, executed={executed}, skipped={skipped}, failed={failed}"
    );
    assert_optional_skips_within_limit(scope, skipped);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn csharp_runner_source() -> &'static str {
    r#"
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using System.Text.Json;
using ConfigForge.Runtime.BinaryReader;
using ConfigForge.Runtime.JsonReader;
using ConfigForge.Runtime.MessagePackReader;

var fixtureDir = Environment.GetEnvironmentVariable("FIXTURE_DIR") ?? throw new Exception("FIXTURE_DIR missing");
var adapters = new (string Name, Func<string, dynamic> ReaderFor)[] {
    ("csharp-json", name => new JsonRuntimeDataReader(JsonDocument.Parse(File.ReadAllText(Path.Combine(fixtureDir, name))))),
    ("csharp-messagepack", name => new MessagePackRuntimeDataReader(EncodeMessagePack(JsonDocument.Parse(File.ReadAllText(Path.Combine(fixtureDir, name))).RootElement))),
};
foreach (var adapter in adapters) {
    RunNamedCompliance(adapter.Name, adapter.ReaderFor);
}
RunBinaryCompliance();

static void Check(bool condition, string label) { if (!condition) throw new Exception(label); }
static void Equal<T>(T actual, T expected, string label) { if (!EqualityComparer<T>.Default.Equals(actual, expected)) throw new Exception($"{label}: expected {expected}, got {actual}"); }
static void ExpectFailure(string label, Action operation, string keyword) { try { operation(); } catch (Exception error) { if (keyword.Length == 0 || error.Message.Contains(keyword)) return; throw new Exception($"{label}: expected {keyword}, got {error.Message}"); } throw new Exception(label + ": expected failure"); }
static void Enter(dynamic reader, params object[] steps) { foreach (var step in steps) { if (step is string field) reader.EnterRequiredField(field); else reader.EnterListElement((int)step); } }
static void Exit(dynamic reader, params object[] steps) { for (var i = steps.Length - 1; i >= 0; i--) { if (steps[i] is string) reader.ExitField(); else reader.ExitListElement(); } }
static T WithPath<T>(dynamic reader, Func<dynamic, T> read, params object[] steps) { Enter(reader, steps); try { return read(reader); } finally { Exit(reader, steps); } }

static void RunNamedCompliance(string adapter, Func<string, dynamic> readerFor) {
    dynamic scalar = readerFor("scalar_compat.json");
    Equal(WithPath<long>((object)scalar, r => r.ReadInt(), "rows", 0, "id"), 7L, adapter + " int scalar");
    Equal(WithPath<long>((object)scalar, r => r.ReadInt(), "rows", 0, "level"), 12L, adapter + " numeric string int");
    Check(Math.Abs(WithPath<double>((object)scalar, r => r.ReadFloat(), "rows", 0, "ratio") - 1.5) < 0.000001, adapter + " float scalar");
    Equal(WithPath<bool>((object)scalar, r => r.ReadBool(), "rows", 0, "enabled_text"), true, adapter + " true-like bool");
    Equal(WithPath<bool>((object)scalar, r => r.ReadBool(), "rows", 0, "disabled_text"), false, adapter + " false-like bool");
    Equal(WithPath<string>((object)scalar, r => r.ReadString(), "rows", 0, "name"), "Potion", adapter + " string scalar");
    Enter(scalar, "rows", 0); try { Equal((bool)scalar.EnterOptionalField("nullable"), false, adapter + " null optional"); } finally { Exit(scalar, "rows", 0); }

    dynamic escaped = readerFor("escaped_string.json");
    Equal(WithPath<string>((object)escaped, r => r.ReadString(), "rows", 0, "text"), "line\nnext\t\"quoted\"", adapter + " escaped string");
    dynamic collection = readerFor("collection_envelope.json");
    Equal(WithPath<int>((object)collection, r => r.ReadRowCount(), "rows"), 2, adapter + " row count");
    Equal(WithPath<long>((object)collection, r => r.ReadInt(), "rows", 0, "id"), 1001L, adapter + " first row id");
    Equal(WithPath<string>((object)collection, r => r.ReadString(), "rows", 1, "name"), "Shield", adapter + " second row name");
    Equal(WithPath<int>((object)collection, r => r.ReadListLength(), "rows", 0, "tags"), 2, adapter + " tag count");
    Equal(WithPath<string>((object)collection, r => r.ReadString(), "rows", 0, "tags", 0), "weapon", adapter + " first tag");
    dynamic nested = readerFor("nested_struct.json");
    Equal(WithPath<long>((object)nested, r => r.ReadInt(), "rows", 0, "reward", "count"), 3L, adapter + " nested count");
    dynamic fragment = readerFor("fragment.json");
    Equal(WithPath<string>((object)fragment, r => r.ReadString(), "rows", 0, "effect", "kind"), "fixed", adapter + " fragment kind");

    dynamic missing = readerFor("missing_vs_null.json");
    Enter(missing, "rows", 0); try { ExpectFailure(adapter + " missing", () => missing.EnterRequiredField("missing"), "missing required field"); } finally { Exit(missing, "rows", 0); }
    ExpectFailure(adapter + " overflow", () => WithPath<long>((object)scalar, r => r.ReadInt(), "rows", 0, "overflow_int"), "");
    ExpectFailure(adapter + " invalid bool", () => WithPath<bool>((object)scalar, r => r.ReadBool(), "rows", 0, "invalid_bool"), "");
    ExpectFailure(adapter + " non finite", () => WithPath<double>((object)scalar, r => r.ReadFloat(), "rows", 0, "non_finite_float"), "finite");
}

static byte[] EncodeMessagePack(JsonElement value) { var bytes = new List<byte>(); WriteMessagePack(value, bytes); return bytes.ToArray(); }
static void WriteMessagePack(JsonElement value, List<byte> bytes) {
    switch (value.ValueKind) {
        case JsonValueKind.Null: bytes.Add(0xc0); break;
        case JsonValueKind.True: bytes.Add(0xc3); break;
        case JsonValueKind.False: bytes.Add(0xc2); break;
        case JsonValueKind.String: WriteString(value.GetString() ?? "", bytes); break;
        case JsonValueKind.Number: if (value.TryGetInt64(out var integer)) WriteInt(integer, bytes); else { bytes.Add(0xcb); WriteU64Big(bytes, unchecked((ulong)BitConverter.DoubleToInt64Bits(value.GetDouble()))); } break;
        case JsonValueKind.Array: var length = value.GetArrayLength(); bytes.Add((byte)(0x90 | length)); foreach (var element in value.EnumerateArray()) WriteMessagePack(element, bytes); break;
        case JsonValueKind.Object: var props = new List<JsonProperty>(value.EnumerateObject()); bytes.Add((byte)(0x80 | props.Count)); foreach (var prop in props) { WriteString(prop.Name, bytes); WriteMessagePack(prop.Value, bytes); } break;
        default: throw new Exception("unsupported JSON kind");
    }
}
static void WriteString(string value, List<byte> bytes) { var encoded = Encoding.UTF8.GetBytes(value); if (encoded.Length <= 31) bytes.Add((byte)(0xa0 | encoded.Length)); else { bytes.Add(0xd9); bytes.Add((byte)encoded.Length); } bytes.AddRange(encoded); }
static void WriteInt(long value, List<byte> bytes) { if (value >= 0 && value <= 0x7f) bytes.Add((byte)value); else if (value >= -32 && value < 0) bytes.Add((byte)(0x100 + value)); else { bytes.Add(0xd3); WriteU64Big(bytes, unchecked((ulong)value)); } }
static void WriteU64Big(List<byte> bytes, ulong value) { for (var shift = 56; shift >= 0; shift -= 8) bytes.Add((byte)((value >> shift) & 0xff)); }

static void WriteVaruint(List<byte> bytes, ulong value) { do { var b = (byte)(value & 0x7f); value >>= 7; if (value != 0) b |= 0x80; bytes.Add(b); } while (value != 0); }
static void WriteLe64(List<byte> bytes, ulong value) { for (var i = 0; i < 8; i++) bytes.Add((byte)((value >> (i * 8)) & 0xff)); }
static void WriteI64(List<byte> bytes, long value) => WriteVaruint(bytes, unchecked((ulong)((value << 1) ^ (value >> 63))));
static void WriteDouble(List<byte> bytes, double value) => WriteLe64(bytes, unchecked((ulong)BitConverter.DoubleToInt64Bits(value)));
static void WriteBinaryString(List<byte> bytes, string value) { var encoded = Encoding.UTF8.GetBytes(value); WriteVaruint(bytes, (ulong)encoded.Length); bytes.AddRange(encoded); }
static List<byte> Header() { var bytes = new List<byte>{(byte)'C',(byte)'F',(byte)'G',(byte)'B',1,0,16,0}; WriteLe64(bytes, 7); return bytes; }
static void RunBinaryCompliance() {
    var bytes = Header(); WriteVaruint(bytes, 1); WriteI64(bytes, 7); WriteI64(bytes, 12); WriteDouble(bytes, 1.5); bytes.Add(1); bytes.Add(0); bytes.Add(1); WriteBinaryString(bytes, "Potion");
    var reader = new BinaryRuntimeDataReader(bytes.ToArray()); Equal(reader.SchemaRevision(), 7UL, "binary schema revision"); Equal(reader.ReadRowCount(), 1, "binary row count"); reader.EnterRow(0); reader.BeginRecord(0); Equal(reader.ReadInt(), 7L, "binary int"); Equal(reader.ReadInt(), 12L, "binary level"); Check(Math.Abs(reader.ReadFloat() - 1.5) < 0.000001, "binary float"); Equal(reader.ReadBool(), true, "binary true"); Equal(reader.ReadBool(), false, "binary false"); Equal(reader.ReadBool(), true, "binary enabled bool"); Equal(reader.ReadString(), "Potion", "binary string"); reader.EndRecord(); reader.EnsureExhausted();
    ExpectFailure("binary order", () => { var data = Header(); WriteVaruint(data, 1); var r = new BinaryRuntimeDataReader(data.ToArray()); r.ReadRowCount(); r.EnterRow(1); }, "sequentially");
}
"#
}

fn java_runner_source() -> &'static str {
    r#"
import java.io.*;
import java.lang.reflect.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import configforge.runtime.binary.ConfigForgeBinaryRuntimeDataReader;
import configforge.runtime.json.*;
import configforge.runtime.messagepack.*;

public class RuntimeCompliance {
  interface ReaderFactory { Object create(String name) throws Exception; }
  static String fixtureDir;
  public static void main(String[] args) throws Exception {
    fixtureDir = System.getenv("FIXTURE_DIR");
    runNamed("java-json", name -> new ConfigForgeJsonRuntimeDataReader(ConfigForgeJsonParser.parse(readFixture(name))));
    runNamed("java-messagepack", name -> new ConfigForgeMessagePackRuntimeDataReader(ConfigForgeMessagePackParser.parse(encodeMessagePack(ConfigForgeJsonParser.parse(readFixture(name))))));
    runBinary();
  }
  static String readFixture(String name) throws Exception { return new String(Files.readAllBytes(Paths.get(fixtureDir, name)), StandardCharsets.UTF_8); }
  static Object call(Object reader, String name, Class<?>[] types, Object... args) throws Exception { try { return reader.getClass().getMethod(name, types).invoke(reader, args); } catch (InvocationTargetException e) { throw (Exception)e.getCause(); } }
  static void field(Object r, String name) throws Exception { call(r, "enterRequiredField", new Class[]{String.class}, name); }
  static void exitField(Object r) throws Exception { call(r, "exitField", new Class[]{}); }
  static void index(Object r, int i) throws Exception { call(r, "enterListElement", new Class[]{int.class}, i); }
  static void exitIndex(Object r) throws Exception { call(r, "exitListElement", new Class[]{}); }
  static void check(boolean condition, String label) { if (!condition) throw new RuntimeException(label); }
  static void equal(Object actual, Object expected, String label) { if (!Objects.equals(actual, expected)) throw new RuntimeException(label + ": expected " + expected + ", got " + actual); }
  static void expectFailure(String label, ThrowingRunnable op, String keyword) throws Exception { try { op.run(); } catch (Exception e) { if (keyword.isEmpty() || e.getMessage().contains(keyword)) return; throw new RuntimeException(label + ": expected " + keyword + ", got " + e.getMessage()); } throw new RuntimeException(label + ": expected failure"); }
  interface ThrowingRunnable { void run() throws Exception; }
  interface ReadOp { Object run(Object reader) throws Exception; }
  static Object withPath(Object r, ReadOp op, Object... steps) throws Exception { for (Object step: steps) { if (step instanceof String) field(r, (String)step); else index(r, (Integer)step); } try { return op.run(r); } finally { for (int i = steps.length - 1; i >= 0; i--) { if (steps[i] instanceof String) exitField(r); else exitIndex(r); } } }
  static long readInt(Object r) throws Exception { return ((Long)call(r, "readInt", new Class[]{})).longValue(); }
  static double readFloat(Object r) throws Exception { return ((Double)call(r, "readFloat", new Class[]{})).doubleValue(); }
  static boolean readBool(Object r) throws Exception { return ((Boolean)call(r, "readBool", new Class[]{})).booleanValue(); }
  static String readString(Object r) throws Exception { return (String)call(r, "readString", new Class[]{}); }
  static int readRowCount(Object r) throws Exception { return ((Integer)call(r, "readRowCount", new Class[]{})).intValue(); }
  static int readListLength(Object r) throws Exception { return ((Integer)call(r, "readListLength", new Class[]{})).intValue(); }
  static boolean enterOptional(Object r, String name) throws Exception { return ((Boolean)call(r, "enterOptionalField", new Class[]{String.class}, name)).booleanValue(); }

  static void runNamed(String adapter, ReaderFactory readerFor) throws Exception {
    Object scalar = readerFor.create("scalar_compat.json");
    equal(withPath(scalar, r -> readInt(r), "rows", 0, "id"), Long.valueOf(7), adapter + " int");
    equal(withPath(scalar, r -> readInt(r), "rows", 0, "level"), Long.valueOf(12), adapter + " level");
    check(Math.abs((Double)withPath(scalar, r -> readFloat(r), "rows", 0, "ratio") - 1.5) < 0.000001, adapter + " float");
    equal(withPath(scalar, r -> readBool(r), "rows", 0, "enabled_text"), Boolean.TRUE, adapter + " true");
    equal(withPath(scalar, r -> readBool(r), "rows", 0, "disabled_text"), Boolean.FALSE, adapter + " false");
    equal(withPath(scalar, r -> readString(r), "rows", 0, "name"), "Potion", adapter + " string");
    field(scalar, "rows"); index(scalar, 0); try { equal(Boolean.valueOf(enterOptional(scalar, "nullable")), Boolean.FALSE, adapter + " optional null"); } finally { exitIndex(scalar); exitField(scalar); }
    Object collection = readerFor.create("collection_envelope.json");
    equal(withPath(collection, r -> readRowCount(r), "rows"), Integer.valueOf(2), adapter + " row count");
    equal(withPath(collection, r -> readString(r), "rows", 1, "name"), "Shield", adapter + " row name");
    equal(withPath(collection, r -> readListLength(r), "rows", 0, "tags"), Integer.valueOf(2), adapter + " tags");
    Object escaped = readerFor.create("escaped_string.json"); equal(withPath(escaped, r -> readString(r), "rows", 0, "text"), "line\nnext\t\"quoted\"", adapter + " escaped");
    Object nested = readerFor.create("nested_struct.json"); equal(withPath(nested, r -> readInt(r), "rows", 0, "reward", "count"), Long.valueOf(3), adapter + " nested");
    Object fragment = readerFor.create("fragment.json"); equal(withPath(fragment, r -> readString(r), "rows", 0, "effect", "kind"), "fixed", adapter + " fragment");
    Object missing = readerFor.create("missing_vs_null.json"); field(missing, "rows"); index(missing, 0); try { expectFailure(adapter + " missing", () -> field(missing, "missing"), "missing required field"); } finally { exitIndex(missing); exitField(missing); }
    expectFailure(adapter + " overflow", () -> withPath(scalar, r -> readInt(r), "rows", 0, "overflow_int"), "");
    expectFailure(adapter + " invalid bool", () -> withPath(scalar, r -> readBool(r), "rows", 0, "invalid_bool"), "");
    expectFailure(adapter + " non finite", () -> withPath(scalar, r -> readFloat(r), "rows", 0, "non_finite_float"), "finite");
  }

  static byte[] encodeMessagePack(Object value) throws Exception { ByteArrayOutputStream out = new ByteArrayOutputStream(); writeMessagePack(value, out); return out.toByteArray(); }
  static void writeMessagePack(Object value, ByteArrayOutputStream out) throws Exception { if (value == null) out.write(0xc0); else if (value instanceof Boolean) out.write(((Boolean)value) ? 0xc3 : 0xc2); else if (value instanceof String) writeString((String)value, out); else if (value instanceof List) { List<?> list = (List<?>)value; out.write(0x90 | list.size()); for (Object e: list) writeMessagePack(e, out); } else if (value instanceof Map) { Map<?,?> map = (Map<?,?>)value; out.write(0x80 | map.size()); for (Map.Entry<?,?> e: map.entrySet()) { writeString((String)e.getKey(), out); writeMessagePack(e.getValue(), out); } } else writeString(String.valueOf(value), out); }
  static void writeString(String value, ByteArrayOutputStream out) throws Exception { byte[] bytes = value.getBytes(StandardCharsets.UTF_8); out.write(0xa0 | bytes.length); out.write(bytes); }
  static void writeVaruint(ByteArrayOutputStream out, long value) { do { int b = (int)(value & 0x7f); value >>>= 7; if (value != 0) b |= 0x80; out.write(b); } while (value != 0); }
  static void writeLe64(ByteArrayOutputStream out, long value) { for (int i = 0; i < 8; i++) out.write((int)((value >> (i * 8)) & 0xff)); }
  static void writeI64(ByteArrayOutputStream out, long value) { writeVaruint(out, (value << 1) ^ (value >> 63)); }
  static void writeDouble(ByteArrayOutputStream out, double value) { writeLe64(out, Double.doubleToRawLongBits(value)); }
  static void writeBinString(ByteArrayOutputStream out, String value) throws Exception { byte[] bytes = value.getBytes(StandardCharsets.UTF_8); writeVaruint(out, bytes.length); out.write(bytes); }
  static ByteArrayOutputStream header() { ByteArrayOutputStream out = new ByteArrayOutputStream(); out.write('C'); out.write('F'); out.write('G'); out.write('B'); out.write(1); out.write(0); out.write(16); out.write(0); writeLe64(out, 7); return out; }
  static void runBinary() throws Exception { ByteArrayOutputStream out = header(); writeVaruint(out, 1); writeI64(out, 7); writeI64(out, 12); writeDouble(out, 1.5); out.write(1); out.write(0); out.write(1); writeBinString(out, "Potion"); ConfigForgeBinaryRuntimeDataReader r = new ConfigForgeBinaryRuntimeDataReader(out.toByteArray()); equal(r.schemaRevision(), Long.valueOf(7), "binary schema"); equal(Integer.valueOf(r.readRowCount()), Integer.valueOf(1), "binary row count"); r.enterRow(0); r.beginRecord(0); equal(Long.valueOf(r.readInt()), Long.valueOf(7), "binary int"); equal(Long.valueOf(r.readInt()), Long.valueOf(12), "binary level"); check(Math.abs(r.readFloat() - 1.5) < 0.000001, "binary float"); equal(Boolean.valueOf(r.readBool()), Boolean.TRUE, "binary true"); equal(Boolean.valueOf(r.readBool()), Boolean.FALSE, "binary false"); equal(Boolean.valueOf(r.readBool()), Boolean.TRUE, "binary enabled bool"); equal(r.readString(), "Potion", "binary string"); r.endRecord(); r.ensureExhausted(); }
}
"#
}
