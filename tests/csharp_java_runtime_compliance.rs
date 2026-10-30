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
            "runtime/csharp/system_text_json_reader/SystemTextJsonRuntimeDataReader.cs",
            "SystemTextJsonRuntimeDataReader.cs",
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
        "ConfigForgeJsonRuntimeDataLoader.java",
        "ConfigForgeJsonRuntimeDataReader.java",
        "ConfigForgeRuntimeErrorKind.java",
        "ConfigForgeRuntimeException.java",
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
        "ConfigForgeMessagePackRuntimeDataLoader.java",
        "ConfigForgeMessagePackRuntimeDataReader.java",
        "ConfigForgeRuntimeErrorKind.java",
        "ConfigForgeRuntimeException.java",
    ] {
        fs::copy(
            root.join("runtime/java/messagepack_reader/src/configforge/runtime/messagepack")
                .join(file),
            msgpack_dir.join(file),
        )
        .expect("Java MessagePack source should be copied");
    }
    for file in [
        "ConfigForgeBinaryRuntimeDataLoader.java",
        "ConfigForgeBinaryRuntimeDataReader.java",
        "ConfigForgeRuntimeErrorKind.java",
        "ConfigForgeRuntimeException.java",
    ] {
        fs::copy(
            root.join("runtime/java/binary_reader/src/configforge/runtime/binary")
                .join(file),
            binary_dir.join(file),
        )
        .expect("Java binary source should be copied");
    }
    fs::write(
        source_root.join("RuntimeCompliance.java"),
        java_runner_source(),
    )
    .expect("Java compliance source should be written");

    let output = Command::new("javac")
        .args(["-encoding", "UTF-8"])
        .arg("-d")
        .arg(&class_dir)
        .arg(source_root.join("RuntimeCompliance.java"))
        .arg(json_dir.join("ConfigForgeJsonParser.java"))
        .arg(json_dir.join("ConfigForgeJsonRuntimeDataLoader.java"))
        .arg(json_dir.join("ConfigForgeJsonRuntimeDataReader.java"))
        .arg(json_dir.join("ConfigForgeRuntimeErrorKind.java"))
        .arg(json_dir.join("ConfigForgeRuntimeException.java"))
        .arg(msgpack_dir.join("ConfigForgeMessagePackParser.java"))
        .arg(msgpack_dir.join("ConfigForgeMessagePackRuntimeDataLoader.java"))
        .arg(msgpack_dir.join("ConfigForgeMessagePackRuntimeDataReader.java"))
        .arg(msgpack_dir.join("ConfigForgeRuntimeErrorKind.java"))
        .arg(msgpack_dir.join("ConfigForgeRuntimeException.java"))
        .arg(binary_dir.join("ConfigForgeBinaryRuntimeDataLoader.java"))
        .arg(binary_dir.join("ConfigForgeBinaryRuntimeDataReader.java"))
        .arg(binary_dir.join("ConfigForgeRuntimeErrorKind.java"))
        .arg(binary_dir.join("ConfigForgeRuntimeException.java"))
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
using ConfigForge.Runtime.SystemTextJsonReader;

var fixtureDir = Environment.GetEnvironmentVariable("FIXTURE_DIR") ?? throw new Exception("FIXTURE_DIR missing");
var adapters = new (string Name, Func<string, dynamic> ReaderFor)[] {
    ("csharp-json", name => new JsonRuntimeDataReader(JsonDocument.Parse(File.ReadAllText(Path.Combine(fixtureDir, name))))),
    ("csharp-system-text-json", name => new SystemTextJsonRuntimeDataReader(JsonDocument.Parse(File.ReadAllText(Path.Combine(fixtureDir, name))))),
    ("csharp-messagepack", name => new MessagePackRuntimeDataReader(EncodeMessagePack(JsonDocument.Parse(File.ReadAllText(Path.Combine(fixtureDir, name))).RootElement))),
};
foreach (var adapter in adapters) {
    RunNamedCompliance(adapter.Name, adapter.ReaderFor);
}
RunBinaryCompliance();
RunLoaderIoCompliance(fixtureDir);

static void Check(bool condition, string label) { if (!condition) throw new Exception(label); }
static void Equal<T>(T actual, T expected, string label) { if (!EqualityComparer<T>.Default.Equals(actual, expected)) throw new Exception($"{label}: expected {expected}, got {actual}"); }
static void ExpectFailure(string label, Action operation, string keyword) { try { operation(); } catch (Exception error) { if (keyword.Length == 0 || error.Message.Contains(keyword)) return; throw new Exception($"{label}: expected {keyword}, got {error.Message}"); } throw new Exception(label + ": expected failure"); }
static void ExpectStructuredFailure(string label, Action operation, string keyword, string kind, string code, string? pathContains = null) { try { operation(); } catch (Exception error) { if (keyword.Length != 0 && !error.Message.Contains(keyword)) throw new Exception($"{label}: expected {keyword}, got {error.Message}"); var kindProp = error.GetType().GetProperty("Kind") ?? throw new Exception(label + ": missing Kind"); var codeProp = error.GetType().GetProperty("Code") ?? throw new Exception(label + ": missing Code"); Equal(kindProp.GetValue(error)?.ToString(), kind, label + " kind"); Equal(codeProp.GetValue(error)?.ToString(), code, label + " code"); if (pathContains != null) { var pathProp = error.GetType().GetProperty("Path") ?? throw new Exception(label + ": missing Path"); var path = pathProp.GetValue(error)?.ToString() ?? ""; if (!path.Contains(pathContains)) throw new Exception($"{label}: expected path containing {pathContains}, got {path}"); } return; } throw new Exception(label + ": expected failure"); }
static void Enter(dynamic reader, params object[] steps) { foreach (var step in steps) { if (step is string field) reader.EnterRequiredField(field); else reader.EnterListElement((int)step); } }
static void Exit(dynamic reader, params object[] steps) { for (var i = steps.Length - 1; i >= 0; i--) { if (steps[i] is string) reader.ExitField(); else reader.ExitListElement(); } }
static T WithPath<T>(dynamic reader, Func<dynamic, T> read, params object[] steps) { Enter(reader, steps); try { return read(reader); } finally { Exit(reader, steps); } }

static void RunNamedCompliance(string adapter, Func<string, dynamic> readerFor) {
    AssertCSharpTemplateMetadata(adapter);
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
    Enter(missing, "rows", 0); try { ExpectStructuredFailure(adapter + " missing", () => missing.EnterRequiredField("missing"), "missing required field", "NotFound", "CF_RT_NOT_FOUND"); } finally { Exit(missing, "rows", 0); }
    ExpectFailure(adapter + " overflow", () => WithPath<long>((object)scalar, r => r.ReadInt(), "rows", 0, "overflow_int"), "");
    ExpectFailure(adapter + " invalid bool", () => WithPath<bool>((object)scalar, r => r.ReadBool(), "rows", 0, "invalid_bool"), "");
    ExpectFailure(adapter + " non finite", () => WithPath<double>((object)scalar, r => r.ReadFloat(), "rows", 0, "non_finite_float"), "finite");
    ExpectFailure(adapter + " list length on scalar", () => WithPath<int>((object)scalar, r => r.ReadListLength(), "rows", 0, "id"), "array");
    ExpectStructuredFailure(adapter + " string type mismatch", () => WithPath<string>((object)scalar, r => r.ReadString(), "rows", 0, "enabled_bool"), "string", "TypeMismatch", "CF_RT_TYPE_MISMATCH");
    ExpectFailure(adapter + " unknown enum", () => WithPath<int>((object)scalar, r => r.ReadEnumIndex(new[] { "Sword", "Shield" }), "rows", 0, "name"), "unknown enum");
}

static void RunLoaderIoCompliance(string fixtureDir) {
    ExpectStructuredFailure("csharp json loader missing file", () => new JsonRuntimeDataFileLoader(fixtureDir).OpenReader("missing_runtime_file"), "failed to read runtime data", "Io", "CF_RT_IO", "missing_runtime_file.json");
    ExpectStructuredFailure("csharp messagepack loader missing file", () => new MessagePackRuntimeDataFileLoader(fixtureDir).OpenReader("missing_runtime_file"), "failed to read runtime data", "Io", "CF_RT_IO", "missing_runtime_file.msgpack");
    ExpectStructuredFailure("csharp binary loader missing file", () => new BinaryRuntimeDataFileLoader(fixtureDir).OpenReader("missing_runtime_file"), "failed to read runtime data", "Io", "CF_RT_IO", "missing_runtime_file.bin");
}

static void AssertCSharpTemplateMetadata(string adapter) {
    Equal(JsonRuntimeDataReader.TemplateVersion, "0.1.0", adapter + " JSON template version");
    Equal(JsonRuntimeDataReader.SupportedConfigForgeReaderContract, "^1.0", adapter + " JSON contract range");
    Equal(JsonRuntimeDataReader.DistributionMode, "copyable-template", adapter + " JSON distribution");
    Equal(SystemTextJsonRuntimeDataReader.TemplateVersion, "0.1.0", adapter + " System.Text.Json template version");
    Equal(SystemTextJsonRuntimeDataReader.SupportedConfigForgeReaderContract, "^1.0", adapter + " System.Text.Json contract range");
    Equal(SystemTextJsonRuntimeDataReader.DistributionMode, "copyable-template", adapter + " System.Text.Json distribution");
    Equal(SystemTextJsonRuntimeDataReader.ParserBackend, "System.Text.Json", adapter + " System.Text.Json parser backend");
    Equal(SystemTextJsonRuntimeDataReader.AdapterSupportLevel, "stable", adapter + " System.Text.Json support level");
    Equal(MessagePackRuntimeDataReader.TemplateVersion, "0.1.0", adapter + " MessagePack template version");
    Equal(MessagePackRuntimeDataReader.SupportedConfigForgeReaderContract, "^1.0", adapter + " MessagePack contract range");
    Equal(MessagePackRuntimeDataReader.DistributionMode, "copyable-template", adapter + " MessagePack distribution");
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
static List<byte> HeaderWithRevision(ulong revision) { var bytes = new List<byte>{(byte)'C',(byte)'F',(byte)'G',(byte)'B',1,0,16,0}; WriteLe64(bytes, revision); return bytes; }
static List<byte> ScalarBinaryPayload() { var bytes = Header(); WriteVaruint(bytes, 1); bytes.Add(0); WriteI64(bytes, 7); WriteI64(bytes, 12); WriteDouble(bytes, 1.5); bytes.Add(1); bytes.Add(0); bytes.Add(1); WriteBinaryString(bytes, "Potion"); return bytes; }
static List<byte> CollectionBinaryPayload() { var bytes = Header(); WriteVaruint(bytes, 2); WriteI64(bytes, 1001); WriteBinaryString(bytes, "Sword"); WriteVaruint(bytes, 2); WriteBinaryString(bytes, "weapon"); WriteBinaryString(bytes, "starter"); WriteI64(bytes, 1002); WriteBinaryString(bytes, "Shield"); WriteVaruint(bytes, 1); WriteBinaryString(bytes, "armor"); return bytes; }
static List<byte> ObjectBinaryPayload() { var bytes = Header(); WriteI64(bytes, 10); bytes.Add(1); WriteBinaryString(bytes, "Demo Game"); return bytes; }
static List<byte> NestedBinaryPayload() { var bytes = Header(); WriteVaruint(bytes, 1); WriteI64(bytes, 2001); WriteI64(bytes, 3001); WriteI64(bytes, 3); WriteVaruint(bytes, 2); WriteBinaryString(bytes, "fire"); WriteBinaryString(bytes, "rare"); return bytes; }
static List<byte> FragmentBinaryPayload() { var bytes = Header(); WriteVaruint(bytes, 1); WriteI64(bytes, 4001); WriteBinaryString(bytes, "fixed"); WriteI64(bytes, 50); return bytes; }
static void RequireBinarySchema(BinaryRuntimeDataReader reader, ulong expected) { var revision = reader.SchemaRevision(); if (!revision.HasValue || revision.Value != expected) throw new Exception($"schema revision mismatch: expected {expected}, got {revision}"); }
static void RunBinaryCompliance() {
    Equal(BinaryRuntimeDataReader.TemplateVersion, "0.1.0", "binary template version"); Equal(BinaryRuntimeDataReader.SupportedConfigForgeReaderContract, "^1.0", "binary contract range"); Equal(BinaryRuntimeDataReader.DistributionMode, "copyable-template", "binary distribution");
    var reader = new BinaryRuntimeDataReader(ScalarBinaryPayload().ToArray()); Equal(reader.SchemaRevision(), 7UL, "binary schema revision"); Equal(reader.ReadRowCount(), 1, "binary scalar row count"); reader.EnterRow(0); reader.BeginRecord(1); Equal(reader.ReadInt(), 7L, "binary int"); Equal(reader.ReadInt(), 12L, "binary level"); Check(Math.Abs(reader.ReadFloat() - 1.5) < 0.000001, "binary float"); Equal(reader.ReadBool(), true, "binary true"); Equal(reader.ReadBool(), false, "binary false"); Equal(reader.ReadBool(), true, "binary enabled bool"); Equal(reader.ReadString(), "Potion", "binary string"); Equal(reader.EnterOptionalField("nullable"), false, "binary optional null"); reader.EndRecord(); reader.EnsureExhausted();
    reader = new BinaryRuntimeDataReader(CollectionBinaryPayload().ToArray()); Equal(reader.ReadRowCount(), 2, "binary collection row count"); reader.EnterRow(0); reader.BeginRecord(0); Equal(reader.ReadInt(), 1001L, "binary first id"); Equal(reader.ReadString(), "Sword", "binary first name"); Equal(reader.ReadListLength(), 2, "binary tag count"); Equal(reader.ReadString(), "weapon", "binary first tag"); Equal(reader.ReadString(), "starter", "binary second tag"); reader.EndRecord(); reader.EnterRow(1); reader.BeginRecord(0); Equal(reader.ReadInt(), 1002L, "binary second id"); Equal(reader.ReadString(), "Shield", "binary second name"); Equal(reader.ReadListLength(), 1, "binary second tag count"); Equal(reader.ReadString(), "armor", "binary third tag"); reader.EndRecord(); reader.EnsureExhausted();
    reader = new BinaryRuntimeDataReader(ObjectBinaryPayload().ToArray()); reader.BeginRecord(0); Equal(reader.ReadInt(), 10L, "binary object int"); Equal(reader.ReadBool(), true, "binary object bool"); Equal(reader.ReadString(), "Demo Game", "binary object title"); reader.EndRecord(); reader.EnsureExhausted();
    reader = new BinaryRuntimeDataReader(NestedBinaryPayload().ToArray()); Equal(reader.ReadRowCount(), 1, "binary nested row count"); reader.EnterRow(0); reader.BeginRecord(0); Equal(reader.ReadInt(), 2001L, "binary nested id"); reader.BeginRecord(0); Equal(reader.ReadInt(), 3001L, "binary reward item"); Equal(reader.ReadInt(), 3L, "binary reward count"); reader.EndRecord(); Equal(reader.ReadListLength(), 2, "binary attributes"); Equal(reader.ReadString(), "fire", "binary first attribute"); Equal(reader.ReadString(), "rare", "binary second attribute"); reader.EndRecord(); reader.EnsureExhausted();
    reader = new BinaryRuntimeDataReader(FragmentBinaryPayload().ToArray()); Equal(reader.ReadRowCount(), 1, "binary fragment row count"); reader.EnterRow(0); reader.BeginRecord(0); Equal(reader.ReadInt(), 4001L, "binary fragment id"); reader.BeginRecord(0); Equal(reader.ReadString(), "fixed", "binary fragment kind"); Equal(reader.ReadInt(), 50L, "binary fragment amount"); reader.EndRecord(); reader.EndRecord(); reader.EnsureExhausted();
    RunMalformedBinaryCorpus();
}
static void RunMalformedBinaryCorpus() {
    ExpectFailure("binary bad magic", () => { var data = Header(); data[0] = (byte)'X'; _ = new BinaryRuntimeDataReader(data.ToArray()); }, "magic");
    ExpectFailure("binary unsupported version", () => { var data = Header(); data[4] = 2; _ = new BinaryRuntimeDataReader(data.ToArray()); }, "version");
    ExpectFailure("binary truncated header", () => { _ = new BinaryRuntimeDataReader(new byte[] { (byte)'C', (byte)'F', (byte)'G', (byte)'B' }); }, "header");
    ExpectFailure("binary truncated body", () => { var data = Header(); WriteVaruint(data, 1); WriteI64(data, 7); var r = new BinaryRuntimeDataReader(data.ToArray()); r.ReadRowCount(); r.EnterRow(0); r.BeginRecord(0); r.ReadInt(); r.ReadInt(); }, "truncated");
    ExpectFailure("binary wrong schema revision", () => { var r = new BinaryRuntimeDataReader(HeaderWithRevision(8).ToArray()); RequireBinarySchema(r, 7); }, "schema revision");
    ExpectFailure("binary invalid optional bitmap", () => { var data = Header(); WriteVaruint(data, 1); data.Add(0x80); var r = new BinaryRuntimeDataReader(data.ToArray()); r.ReadRowCount(); r.EnterRow(0); r.BeginRecord(1); }, "bitmap");
    ExpectFailure("binary invalid enum ordinal", () => { var data = Header(); WriteVaruint(data, 2); var r = new BinaryRuntimeDataReader(data.ToArray()); r.ReadEnumIndex(new[] { "a", "b" }); }, "ordinal");
    ExpectFailure("binary huge length prefix", () => { var data = Header(); WriteVaruint(data, (ulong)int.MaxValue + 1); var r = new BinaryRuntimeDataReader(data.ToArray()); r.ReadString(); }, "string length");
    ExpectFailure("binary reader not exhausted", () => { var data = Header(); WriteVaruint(data, 0); data.Add(0); var r = new BinaryRuntimeDataReader(data.ToArray()); r.ReadRowCount(); r.EnsureExhausted(); }, "trailing bytes");
    ExpectFailure("binary wrong record boundary", () => { var r = new BinaryRuntimeDataReader(ScalarBinaryPayload().ToArray()); r.ReadRowCount(); r.EnterRow(0); r.EndRecord(); }, "underflow");
    ExpectFailure("binary wrong row boundary", () => { var data = Header(); WriteVaruint(data, 1); var r = new BinaryRuntimeDataReader(data.ToArray()); r.ReadRowCount(); r.EnterRow(1); }, "sequentially");
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
import configforge.runtime.binary.ConfigForgeBinaryRuntimeDataLoader;
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
    runLoaderIoCompliance();
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
  static void expectStructuredFailure(String label, ThrowingRunnable op, String keyword, String kind, String code) throws Exception { expectStructuredFailure(label, op, keyword, kind, code, null); }
  static void expectStructuredFailure(String label, ThrowingRunnable op, String keyword, String kind, String code, String pathContains) throws Exception { try { op.run(); } catch (Exception e) { if (!keyword.isEmpty() && !e.getMessage().contains(keyword)) throw new RuntimeException(label + ": expected " + keyword + ", got " + e.getMessage()); Object actualKind = e.getClass().getMethod("kind").invoke(e); Object actualCode = e.getClass().getMethod("code").invoke(e); equal(String.valueOf(actualKind), kind, label + " kind"); equal(String.valueOf(actualCode), code, label + " code"); if (pathContains != null) { Object actualPath = e.getClass().getMethod("path").invoke(e); String path = String.valueOf(actualPath); if (!path.contains(pathContains)) throw new RuntimeException(label + ": expected path containing " + pathContains + ", got " + path); } return; } throw new RuntimeException(label + ": expected failure"); }
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
    assertJavaTemplateMetadata(adapter);
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
    Object missing = readerFor.create("missing_vs_null.json"); field(missing, "rows"); index(missing, 0); try { expectStructuredFailure(adapter + " missing", () -> field(missing, "missing"), "missing required field", "NotFound", "CF_RT_NOT_FOUND"); } finally { exitIndex(missing); exitField(missing); }
    expectFailure(adapter + " overflow", () -> withPath(scalar, r -> readInt(r), "rows", 0, "overflow_int"), "");
    expectFailure(adapter + " invalid bool", () -> withPath(scalar, r -> readBool(r), "rows", 0, "invalid_bool"), "");
    expectFailure(adapter + " non finite", () -> withPath(scalar, r -> readFloat(r), "rows", 0, "non_finite_float"), "finite");
    expectFailure(adapter + " list length on scalar", () -> withPath(scalar, r -> readListLength(r), "rows", 0, "id"), "array");
    expectStructuredFailure(adapter + " string type mismatch", () -> withPath(scalar, r -> readString(r), "rows", 0, "enabled_bool"), "string", "TypeMismatch", "CF_RT_TYPE_MISMATCH");
    expectFailure(adapter + " unknown enum", () -> withPath(scalar, r -> call(r, "readEnumIndex", new Class[]{String[].class}, (Object)new String[]{"Sword", "Shield"}), "rows", 0, "name"), "unknown enum");
  }

  static void runLoaderIoCompliance() throws Exception {
    Path root = Paths.get(fixtureDir);
    expectStructuredFailure("java json loader missing file", () -> new ConfigForgeJsonRuntimeDataLoader(root).openReader("missing_runtime_file"), "failed to read runtime data", "Io", "CF_RT_IO", "missing_runtime_file.json");
    expectStructuredFailure("java messagepack loader missing file", () -> new ConfigForgeMessagePackRuntimeDataLoader(root).openReader("missing_runtime_file"), "failed to read runtime data", "Io", "CF_RT_IO", "missing_runtime_file.msgpack");
    expectStructuredFailure("java binary loader missing file", () -> new ConfigForgeBinaryRuntimeDataLoader(root).openReader("missing_runtime_file"), "failed to read runtime data", "Io", "CF_RT_IO", "missing_runtime_file.bin");
  }

  static void assertJavaTemplateMetadata(String adapter) {
    equal(ConfigForgeJsonRuntimeDataReader.TEMPLATE_VERSION, "0.1.0", adapter + " JSON template version");
    equal(ConfigForgeJsonRuntimeDataReader.SUPPORTED_CONFIG_FORGE_READER_CONTRACT, "^1.0", adapter + " JSON contract range");
    equal(ConfigForgeJsonRuntimeDataReader.DISTRIBUTION_MODE, "copyable-template", adapter + " JSON distribution");
    equal(ConfigForgeMessagePackRuntimeDataReader.TEMPLATE_VERSION, "0.1.0", adapter + " MessagePack template version");
    equal(ConfigForgeMessagePackRuntimeDataReader.SUPPORTED_CONFIG_FORGE_READER_CONTRACT, "^1.0", adapter + " MessagePack contract range");
    equal(ConfigForgeMessagePackRuntimeDataReader.DISTRIBUTION_MODE, "copyable-template", adapter + " MessagePack distribution");
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
  static ByteArrayOutputStream headerWithRevision(long revision) { ByteArrayOutputStream out = new ByteArrayOutputStream(); out.write('C'); out.write('F'); out.write('G'); out.write('B'); out.write(1); out.write(0); out.write(16); out.write(0); writeLe64(out, revision); return out; }
  static ByteArrayOutputStream scalarBinaryPayload() throws Exception { ByteArrayOutputStream out = header(); writeVaruint(out, 1); out.write(0); writeI64(out, 7); writeI64(out, 12); writeDouble(out, 1.5); out.write(1); out.write(0); out.write(1); writeBinString(out, "Potion"); return out; }
  static ByteArrayOutputStream collectionBinaryPayload() throws Exception { ByteArrayOutputStream out = header(); writeVaruint(out, 2); writeI64(out, 1001); writeBinString(out, "Sword"); writeVaruint(out, 2); writeBinString(out, "weapon"); writeBinString(out, "starter"); writeI64(out, 1002); writeBinString(out, "Shield"); writeVaruint(out, 1); writeBinString(out, "armor"); return out; }
  static ByteArrayOutputStream objectBinaryPayload() throws Exception { ByteArrayOutputStream out = header(); writeI64(out, 10); out.write(1); writeBinString(out, "Demo Game"); return out; }
  static ByteArrayOutputStream nestedBinaryPayload() throws Exception { ByteArrayOutputStream out = header(); writeVaruint(out, 1); writeI64(out, 2001); writeI64(out, 3001); writeI64(out, 3); writeVaruint(out, 2); writeBinString(out, "fire"); writeBinString(out, "rare"); return out; }
  static ByteArrayOutputStream fragmentBinaryPayload() throws Exception { ByteArrayOutputStream out = header(); writeVaruint(out, 1); writeI64(out, 4001); writeBinString(out, "fixed"); writeI64(out, 50); return out; }
  static void requireBinarySchema(ConfigForgeBinaryRuntimeDataReader reader, long expected) { Long revision = reader.schemaRevision(); if (!Objects.equals(revision, Long.valueOf(expected))) throw new RuntimeException("schema revision mismatch: expected " + expected + ", got " + revision); }
  static void runBinary() throws Exception { equal(ConfigForgeBinaryRuntimeDataReader.TEMPLATE_VERSION, "0.1.0", "binary template version"); equal(ConfigForgeBinaryRuntimeDataReader.SUPPORTED_CONFIG_FORGE_READER_CONTRACT, "^1.0", "binary contract range"); equal(ConfigForgeBinaryRuntimeDataReader.DISTRIBUTION_MODE, "copyable-template", "binary distribution"); ConfigForgeBinaryRuntimeDataReader r = new ConfigForgeBinaryRuntimeDataReader(scalarBinaryPayload().toByteArray()); equal(r.schemaRevision(), Long.valueOf(7), "binary schema"); equal(Integer.valueOf(r.readRowCount()), Integer.valueOf(1), "binary scalar row count"); r.enterRow(0); r.beginRecord(1); equal(Long.valueOf(r.readInt()), Long.valueOf(7), "binary int"); equal(Long.valueOf(r.readInt()), Long.valueOf(12), "binary level"); check(Math.abs(r.readFloat() - 1.5) < 0.000001, "binary float"); equal(Boolean.valueOf(r.readBool()), Boolean.TRUE, "binary true"); equal(Boolean.valueOf(r.readBool()), Boolean.FALSE, "binary false"); equal(Boolean.valueOf(r.readBool()), Boolean.TRUE, "binary enabled bool"); equal(r.readString(), "Potion", "binary string"); equal(Boolean.valueOf(r.enterOptionalField("nullable")), Boolean.FALSE, "binary optional null"); r.endRecord(); r.ensureExhausted();
    r = new ConfigForgeBinaryRuntimeDataReader(collectionBinaryPayload().toByteArray()); equal(Integer.valueOf(r.readRowCount()), Integer.valueOf(2), "binary collection row count"); r.enterRow(0); r.beginRecord(0); equal(Long.valueOf(r.readInt()), Long.valueOf(1001), "binary first id"); equal(r.readString(), "Sword", "binary first name"); equal(Integer.valueOf(r.readListLength()), Integer.valueOf(2), "binary tag count"); equal(r.readString(), "weapon", "binary first tag"); equal(r.readString(), "starter", "binary second tag"); r.endRecord(); r.enterRow(1); r.beginRecord(0); equal(Long.valueOf(r.readInt()), Long.valueOf(1002), "binary second id"); equal(r.readString(), "Shield", "binary second name"); equal(Integer.valueOf(r.readListLength()), Integer.valueOf(1), "binary second tag count"); equal(r.readString(), "armor", "binary third tag"); r.endRecord(); r.ensureExhausted();
    r = new ConfigForgeBinaryRuntimeDataReader(objectBinaryPayload().toByteArray()); r.beginRecord(0); equal(Long.valueOf(r.readInt()), Long.valueOf(10), "binary object int"); equal(Boolean.valueOf(r.readBool()), Boolean.TRUE, "binary object bool"); equal(r.readString(), "Demo Game", "binary object title"); r.endRecord(); r.ensureExhausted();
    r = new ConfigForgeBinaryRuntimeDataReader(nestedBinaryPayload().toByteArray()); equal(Integer.valueOf(r.readRowCount()), Integer.valueOf(1), "binary nested row count"); r.enterRow(0); r.beginRecord(0); equal(Long.valueOf(r.readInt()), Long.valueOf(2001), "binary nested id"); r.beginRecord(0); equal(Long.valueOf(r.readInt()), Long.valueOf(3001), "binary reward item"); equal(Long.valueOf(r.readInt()), Long.valueOf(3), "binary reward count"); r.endRecord(); equal(Integer.valueOf(r.readListLength()), Integer.valueOf(2), "binary attributes"); equal(r.readString(), "fire", "binary first attribute"); equal(r.readString(), "rare", "binary second attribute"); r.endRecord(); r.ensureExhausted();
    r = new ConfigForgeBinaryRuntimeDataReader(fragmentBinaryPayload().toByteArray()); equal(Integer.valueOf(r.readRowCount()), Integer.valueOf(1), "binary fragment row count"); r.enterRow(0); r.beginRecord(0); equal(Long.valueOf(r.readInt()), Long.valueOf(4001), "binary fragment id"); r.beginRecord(0); equal(r.readString(), "fixed", "binary fragment kind"); equal(Long.valueOf(r.readInt()), Long.valueOf(50), "binary fragment amount"); r.endRecord(); r.endRecord(); r.ensureExhausted(); runMalformedBinaryCorpus(); }
  static void runMalformedBinaryCorpus() throws Exception {
    expectFailure("binary bad magic", () -> { ByteArrayOutputStream data = header(); byte[] bytes = data.toByteArray(); bytes[0] = 'X'; new ConfigForgeBinaryRuntimeDataReader(bytes); }, "magic");
    expectFailure("binary unsupported version", () -> { ByteArrayOutputStream data = header(); byte[] bytes = data.toByteArray(); bytes[4] = 2; new ConfigForgeBinaryRuntimeDataReader(bytes); }, "version");
    expectFailure("binary truncated header", () -> new ConfigForgeBinaryRuntimeDataReader(new byte[] { 'C', 'F', 'G', 'B' }), "header");
    expectFailure("binary truncated body", () -> { ByteArrayOutputStream data = header(); writeVaruint(data, 1); writeI64(data, 7); ConfigForgeBinaryRuntimeDataReader reader = new ConfigForgeBinaryRuntimeDataReader(data.toByteArray()); reader.readRowCount(); reader.enterRow(0); reader.beginRecord(0); reader.readInt(); reader.readInt(); }, "truncated");
    expectFailure("binary wrong schema revision", () -> requireBinarySchema(new ConfigForgeBinaryRuntimeDataReader(headerWithRevision(8).toByteArray()), 7), "schema revision");
    expectFailure("binary invalid optional bitmap", () -> { ByteArrayOutputStream data = header(); writeVaruint(data, 1); data.write(0x80); ConfigForgeBinaryRuntimeDataReader reader = new ConfigForgeBinaryRuntimeDataReader(data.toByteArray()); reader.readRowCount(); reader.enterRow(0); reader.beginRecord(1); }, "bitmap");
    expectFailure("binary invalid enum ordinal", () -> { ByteArrayOutputStream data = header(); writeVaruint(data, 2); ConfigForgeBinaryRuntimeDataReader reader = new ConfigForgeBinaryRuntimeDataReader(data.toByteArray()); reader.readEnumIndex(new String[] { "a", "b" }); }, "ordinal");
    expectFailure("binary huge length prefix", () -> { ByteArrayOutputStream data = header(); writeVaruint(data, (long)Integer.MAX_VALUE + 1L); ConfigForgeBinaryRuntimeDataReader reader = new ConfigForgeBinaryRuntimeDataReader(data.toByteArray()); reader.readString(); }, "string length");
    expectFailure("binary reader not exhausted", () -> { ByteArrayOutputStream data = header(); writeVaruint(data, 0); data.write(0); ConfigForgeBinaryRuntimeDataReader reader = new ConfigForgeBinaryRuntimeDataReader(data.toByteArray()); reader.readRowCount(); reader.ensureExhausted(); }, "trailing bytes");
    expectFailure("binary wrong record boundary", () -> { ConfigForgeBinaryRuntimeDataReader reader = new ConfigForgeBinaryRuntimeDataReader(scalarBinaryPayload().toByteArray()); reader.readRowCount(); reader.enterRow(0); reader.endRecord(); }, "underflow");
    expectFailure("binary wrong row boundary", () -> { ByteArrayOutputStream data = header(); writeVaruint(data, 1); ConfigForgeBinaryRuntimeDataReader reader = new ConfigForgeBinaryRuntimeDataReader(data.toByteArray()); reader.readRowCount(); reader.enterRow(1); }, "sequentially");
  }
}
"#
}
