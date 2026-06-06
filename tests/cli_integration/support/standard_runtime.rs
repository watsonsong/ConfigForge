// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, process::Command as StdCommand};

use super::{assert_command_success, command_exists};

pub(crate) fn verify_standard_generated_rust_runtime(fixture_root: &std::path::Path) {
    let temp_dir = tempfile::tempdir().expect("rust harness temp dir should be created");
    let harness_dir = fixture_root.join("harness/rust");
    let harness_cargo_toml = fs::read_to_string(harness_dir.join("Cargo.toml"))
        .expect("rust harness Cargo.toml should be read");
    let harness_cargo_toml = harness_cargo_toml.replace(
        "../../../../../runtime/rust/reader_core",
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("runtime/rust/reader_core")
            .display()
            .to_string(),
    );
    let harness_cargo_toml = format!(
        "{harness_cargo_toml}config-forge-runtime-binary-reader = {{ path = {:?} }}\n",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("runtime/rust/binary_reader")
            .display()
            .to_string()
    );
    fs::copy(harness_dir.join("lib.rs"), temp_dir.path().join("lib.rs"))
        .expect("rust harness lib.rs should be copied");
    fs::copy(
        fixture_root.join("../../../runtime/rust/serde_json_reader/src/runtime_reader.rs"),
        temp_dir.path().join("json_runtime_reader.rs"),
    )
    .expect("rust JSON runtime reader should be copied");
    fs::copy(
        fixture_root.join("../../../runtime/rust/serde_msgpack_reader/src/runtime_reader.rs"),
        temp_dir.path().join("msgpack_runtime_reader.rs"),
    )
    .expect("rust MessagePack runtime reader should be copied");
    for edition in ["2021", "2024"] {
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            harness_cargo_toml.replace("edition = \"2024\"", &format!("edition = \"{edition}\"")),
        )
        .expect("rust harness Cargo.toml should be written");

        fs::copy(
            fixture_root.join("out/client_code/rust/reference_game_config.rs"),
            temp_dir.path().join("generated.rs"),
        )
        .expect("generated Rust JSON code should be copied into harness");
        let output = StdCommand::new("cargo")
            .args(["test", "--quiet", "--manifest-path"])
            .arg(temp_dir.path().join("Cargo.toml"))
            .arg("generated_rust_loads_exported_json_and_queries_indexes")
            .env(
                "CARGO_TARGET_DIR",
                temp_dir.path().join(format!("target-{edition}-json")),
            )
            .env("JSON_DATA_DIR", fixture_root.join("out/client_data/json"))
            .env(
                "I18N_MO",
                fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
            )
            .env(
                "MSGPACK_DATA_DIR",
                fixture_root.join("out/client_data/msgpack"),
            )
            .output()
            .expect("standard generated Rust JSON runtime test should run");
        assert_command_success(
            output,
            &format!("standard generated Rust JSON runtime test ({edition})"),
        );
        fs::copy(
            fixture_root.join("out/client_code/rust/reference_game_config.rs"),
            temp_dir.path().join("generated.rs"),
        )
        .expect("generated Rust MessagePack code should be copied into harness");
        let output = StdCommand::new("cargo")
            .args(["test", "--quiet", "--manifest-path"])
            .arg(temp_dir.path().join("Cargo.toml"))
            .arg("generated_rust_loads_exported_messagepack_and_queries_indexes")
            .env(
                "CARGO_TARGET_DIR",
                temp_dir
                    .path()
                    .join(format!("target-{edition}-messagepack")),
            )
            .env("JSON_DATA_DIR", fixture_root.join("out/client_data/json"))
            .env(
                "I18N_MO",
                fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
            )
            .env(
                "MSGPACK_DATA_DIR",
                fixture_root.join("out/client_data/msgpack"),
            )
            .output()
            .expect("standard generated Rust MessagePack runtime test should run");
        assert_command_success(
            output,
            &format!("standard generated Rust MessagePack runtime test ({edition})"),
        );

        fs::copy(
            fixture_root.join("out/client_code/rust/reference_game_config.rs"),
            temp_dir.path().join("generated.rs"),
        )
        .expect("generated Rust code should be copied into binary harness");
        let output = StdCommand::new("cargo")
            .args(["test", "--quiet", "--manifest-path"])
            .arg(temp_dir.path().join("Cargo.toml"))
            .arg("generated_rust_loads_exported_binary_and_queries_indexes")
            .env(
                "CARGO_TARGET_DIR",
                temp_dir.path().join(format!("target-{edition}-binary")),
            )
            .env("BIN_DATA_DIR", fixture_root.join("out/client_data/binary"))
            .env(
                "I18N_MO",
                fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
            )
            .output()
            .expect("standard generated Rust binary runtime test should run");
        assert_command_success(
            output,
            &format!("standard generated Rust binary runtime test ({edition})"),
        );
    }
}
pub(crate) fn verify_standard_generated_cpp_runtime(fixture_root: &std::path::Path) {
    if !command_exists("cmake") {
        eprintln!("skipping standard generated C++ runtime test: cmake not found");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("cpp harness temp dir should be created");
    let runtime_cpp_dir = fixture_root.join("../../../runtime/cpp");
    let generated_cpp_dir = fixture_root.join("out/client_code/cpp");
    let harness_cpp_dir = fixture_root.join("harness/cpp");
    let harness_cpp = fixture_root.join("harness/cpp/runtime_check.cpp");
    let generated_cpp = generated_cpp_dir.join("reference_game_config.cpp");
    let gettext_locale_dir = temp_dir.path().join("locale");
    let gettext_messages_dir = gettext_locale_dir.join("zh_CN/LC_MESSAGES");
    fs::create_dir_all(&gettext_messages_dir).expect("gettext LC_MESSAGES dir should be created");
    fs::copy(
        fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
        gettext_messages_dir.join("reference_game_config.mo"),
    )
    .expect("gettext MO catalog should be copied into locale dir");

    fs::write(
        temp_dir.path().join("generated.hpp"),
        r#"
            #pragma once
            #include <cstdint>
            #include <cstddef>
            #include <memory>
            #include <string>

            class IRuntimeDataReader {
            public:
                virtual ~IRuntimeDataReader() = default;
                virtual void EnterIndex(std::size_t index) = 0;
                virtual std::int64_t AsInt64() const = 0;
                virtual double AsDouble() const = 0;
                virtual bool AsBool() const = 0;
                virtual std::string AsString() const = 0;
                virtual void EnterRequiredField(const std::string& name) = 0;
                virtual bool EnterOptionalField(const std::string& name) = 0;
                virtual void ExitField() = 0;
                virtual std::size_t ReadRowCount() = 0;
            };

            class IRuntimeDataLoader {
            public:
                virtual ~IRuntimeDataLoader() = default;
                virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;
            };
        "#,
    )
    .expect("minimal generated C++ header should be written");
    fs::write(
        temp_dir.path().join("adapter_namespace_check.cpp"),
        r#"
            #include "generated.hpp"
            #define CONFIG_FORGE_RUNTIME_CPP_READER IRuntimeDataReader
            #define CONFIG_FORGE_RUNTIME_CPP_LOADER IRuntimeDataLoader
            #include "config_forge/runtime/rapidjson_reader/runtime_loader.hpp"

            int main() {
                return config_forge_runtime::JsonValueReader("\"1\"").AsBool() ? 0 : 1;
            }
        "#,
    )
    .expect("C++ adapter namespace check should be written");
    fs::write(
        temp_dir.path().join("messagepack_adapter_namespace_check.cpp"),
        r#"
            #include "generated.hpp"
            #include <msgpack.hpp>
            #define CONFIG_FORGE_RUNTIME_CPP_READER IRuntimeDataReader
            #define CONFIG_FORGE_RUNTIME_CPP_LOADER IRuntimeDataLoader
            #include "config_forge/runtime/msgpackc_reader/runtime_loader.hpp"

            int main() {
                msgpack::sbuffer buffer;
                msgpack::packer<msgpack::sbuffer> packer(buffer);
                packer.pack(std::string("1"));
                return config_forge_runtime::MessagePackValueReader(buffer.data(), buffer.size()).AsBool() ? 0 : 1;
            }
        "#,
    )
    .expect("C++ MessagePack adapter namespace check should be written");

    fs::write(
        temp_dir.path().join("CMakeLists.txt"),
        format!(
            r#"
                cmake_minimum_required(VERSION 3.16)
                project(ConfigForgeStandardCppRuntimeHarness LANGUAGES CXX)

                set(CMAKE_CXX_STANDARD 17)
                set(CMAKE_CXX_STANDARD_REQUIRED ON)
                set(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER ON CACHE BOOL "" FORCE)
                set(CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER ON CACHE BOOL "" FORCE)
                set(CONFIG_FORGE_RUNTIME_FETCH_DEPS ON CACHE BOOL "" FORCE)
                add_subdirectory({runtime_cpp_dir:?} config_forge_runtime)

                find_package(Intl QUIET)

                add_executable(standard_runtime_check
                    {harness_cpp:?}
                    {generated_cpp:?})
                target_include_directories(standard_runtime_check PRIVATE {generated_cpp_dir:?} {harness_cpp_dir:?})
                target_link_libraries(standard_runtime_check PRIVATE config_forge_runtime::rapidjson_reader)

                add_executable(standard_binary_runtime_check
                    {harness_cpp:?}
                    {binary_generated_cpp:?})
                target_include_directories(standard_binary_runtime_check PRIVATE {binary_generated_cpp_dir:?} {harness_cpp_dir:?})
                target_compile_definitions(standard_binary_runtime_check PRIVATE CONFIG_FORGE_USE_BINARY_RUNTIME_READER)
                target_link_libraries(standard_binary_runtime_check PRIVATE config_forge_runtime::binary_reader)

                if(Intl_FOUND)
                    target_compile_definitions(standard_runtime_check PRIVATE CONFIG_FORGE_USE_LIBINTL)
                    target_compile_definitions(standard_binary_runtime_check PRIVATE CONFIG_FORGE_USE_LIBINTL)
                    target_link_libraries(standard_runtime_check PRIVATE Intl::Intl)
                    target_link_libraries(standard_binary_runtime_check PRIVATE Intl::Intl)
                endif()

                add_executable(adapter_namespace_check adapter_namespace_check.cpp)
                target_include_directories(adapter_namespace_check PRIVATE {temp_dir:?})
                target_link_libraries(adapter_namespace_check PRIVATE config_forge_runtime::rapidjson_reader)

                add_executable(messagepack_adapter_namespace_check messagepack_adapter_namespace_check.cpp)
                target_include_directories(messagepack_adapter_namespace_check PRIVATE {temp_dir:?})
                target_link_libraries(messagepack_adapter_namespace_check PRIVATE config_forge_runtime::msgpackc_reader)
            "#,
            runtime_cpp_dir = runtime_cpp_dir.display().to_string(),
            harness_cpp = harness_cpp.display().to_string(),
            generated_cpp = generated_cpp.display().to_string(),
            binary_generated_cpp = generated_cpp.display().to_string(),
            generated_cpp_dir = generated_cpp_dir.display().to_string(),
            binary_generated_cpp_dir = generated_cpp_dir.display().to_string(),
            harness_cpp_dir = harness_cpp_dir.display().to_string(),
            temp_dir = temp_dir.path().display().to_string()
        ),
    )
    .expect("C++ CMake harness should be written");

    let build_dir = temp_dir.path().join("build");
    let output = StdCommand::new("cmake")
        .arg("-S")
        .arg(temp_dir.path())
        .arg("-B")
        .arg(&build_dir)
        .output()
        .expect("standard generated C++ CMake configure should run");
    assert_command_success(output, "standard generated C++ CMake configure");

    let output = StdCommand::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .output()
        .expect("standard generated C++ CMake build should run");
    assert_command_success(output, "standard generated C++ CMake build");

    let output = StdCommand::new(build_dir.join("standard_runtime_check"))
        .env("DATA_DIR", fixture_root.join("out/client_data/json"))
        .env("I18N_LOCALE_DIR", &gettext_locale_dir)
        .env("LANGUAGE", "zh_CN")
        .env(
            "I18N_MO",
            fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
        )
        .output()
        .expect("standard generated C++ runtime test should run");
    assert_command_success(output, "standard generated C++ runtime test");

    let output = StdCommand::new(build_dir.join("standard_binary_runtime_check"))
        .env("DATA_DIR", fixture_root.join("out/client_data/binary"))
        .env("I18N_LOCALE_DIR", &gettext_locale_dir)
        .env("LANGUAGE", "zh_CN")
        .env(
            "I18N_MO",
            fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
        )
        .output()
        .expect("standard generated C++ binary runtime test should run");
    assert_command_success(output, "standard generated C++ binary runtime test");

    let output = StdCommand::new(build_dir.join("adapter_namespace_check"))
        .output()
        .expect("C++ adapter namespace check should run");
    assert_command_success(output, "C++ adapter namespace check run");

    let output = StdCommand::new(build_dir.join("messagepack_adapter_namespace_check"))
        .output()
        .expect("C++ MessagePack adapter namespace check should run");
    assert_command_success(output, "C++ MessagePack adapter namespace check run");
}
pub(crate) fn verify_standard_generated_typescript_runtime(fixture_root: &std::path::Path) {
    if !command_exists("node") {
        eprintln!("skipping standard generated TypeScript runtime test: node not found");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("typescript harness temp dir should be created");
    let harness_dir = fixture_root.join("harness/typescript");
    fs::copy(
        harness_dir.join("runtime_check.ts"),
        temp_dir.path().join("runtime_check.ts"),
    )
    .expect("TypeScript runtime harness should be copied");
    fs::copy(harness_dir.join("i18n.ts"), temp_dir.path().join("i18n.ts"))
        .expect("TypeScript i18n harness should be copied");
    fs::copy(
        fixture_root.join("../../../runtime/typescript/reader_core/src/runtime_reader.ts"),
        temp_dir.path().join("runtime_reader_core.ts"),
    )
    .expect("TypeScript runtime reader core should be copied");
    fs::copy(
        fixture_root.join("../../../runtime/typescript/binary_reader/src/runtime_loader.ts"),
        temp_dir.path().join("runtime_loader_binary.ts"),
    )
    .expect("TypeScript binary runtime loader should be copied");
    fs::copy(
        fixture_root.join("../../../runtime/typescript/json_reader/src/runtime_loader.ts"),
        temp_dir.path().join("runtime_loader_json.ts"),
    )
    .expect("TypeScript JSON runtime loader should be copied");
    fs::copy(
        fixture_root.join("../../../runtime/typescript/loaders/node-fs/src/runtime_loader.ts"),
        temp_dir.path().join("runtime_loader_node_fs.ts"),
    )
    .expect("TypeScript Node fs runtime loader should be copied");
    fs::copy(
        fixture_root.join("../../../runtime/typescript/messagepack_reader/src/runtime_loader.ts"),
        temp_dir.path().join("runtime_loader_messagepack.ts"),
    )
    .expect("TypeScript MessagePack runtime loader should be copied");
    fs::copy(
        harness_dir.join("tsconfig.json"),
        temp_dir.path().join("tsconfig.json"),
    )
    .expect("TypeScript tsconfig should be copied");
    fs::copy(
        fixture_root.join("out/client_code/typescript/reference_game_config.ts"),
        temp_dir.path().join("generated.ts"),
    )
    .expect("generated TypeScript code should be copied into harness");

    if command_exists("tsc") {
        assert_command_success(
            StdCommand::new("tsc")
                .args(["--project"])
                .arg(temp_dir.path().join("tsconfig.json"))
                .output()
                .expect("standard generated TypeScript tsc should run"),
            "standard generated TypeScript tsc",
        );
    } else if command_exists("npx") {
        assert_command_success(
            StdCommand::new("npx")
                .args(["--yes", "-p", "typescript", "tsc", "--project"])
                .arg(temp_dir.path().join("tsconfig.json"))
                .output()
                .expect("standard generated TypeScript npx tsc should run"),
            "standard generated TypeScript npx tsc",
        );
    } else {
        eprintln!("skipping standard generated TypeScript runtime test: tsc/npx not found");
        return;
    }

    let package_dir = temp_dir
        .path()
        .join("node_modules/@config-forge/runtime-reader-core");
    fs::create_dir_all(&package_dir)
        .expect("TypeScript core runtime package dir should be created");
    fs::write(
        package_dir.join("package.json"),
        r#"{"type":"commonjs","main":"index.js"}"#,
    )
    .expect("TypeScript core runtime package metadata should be written");
    fs::write(
        package_dir.join("index.js"),
        r#"module.exports = require("../../../dist/runtime_reader_core.js");"#,
    )
    .expect("TypeScript core runtime package entry should be written");

    let output = StdCommand::new("node")
        .arg(temp_dir.path().join("dist/runtime_check.js"))
        .env("DATA_DIR", fixture_root.join("out/client_data/json"))
        .env("BIN_DATA_DIR", fixture_root.join("out/client_data/binary"))
        .env(
            "I18N_MO",
            fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
        )
        .output()
        .expect("standard generated TypeScript runtime test should run");
    assert_command_success(output, "standard generated TypeScript runtime test");
}
pub(crate) fn verify_standard_generated_lua_runtime(fixture_root: &std::path::Path) {
    let Some(lua_command) = ["lua5.4", "lua5.3", "lua", "luajit"]
        .iter()
        .find(|command| command_exists(command))
    else {
        eprintln!("skipping standard generated Lua runtime test: lua interpreter not found");
        return;
    };

    let output = StdCommand::new(lua_command)
        .arg(fixture_root.join("harness/lua/runtime_check.lua"))
        .env("LUA_CONFIG_ROOT", fixture_root.join("out/client_lua"))
        .output()
        .expect("standard generated Lua runtime test should run");
    assert_command_success(output, "standard generated Lua runtime test");
}

pub(crate) fn verify_standard_generated_csharp_runtime(fixture_root: &std::path::Path) {
    if !command_exists("dotnet") {
        eprintln!("skipping standard generated C# runtime test: dotnet not found");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("csharp harness temp dir should be created");
    let project_dir = temp_dir.path();
    fs::copy(
        fixture_root.join("out/client_code/csharp/reference_game_config.cs"),
        project_dir.join("ReferenceGameConfig.cs"),
    )
    .expect("generated C# code should be copied into harness");
    fs::write(
        project_dir.join("ConfigForgeCSharpHarness.csproj"),
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
    .expect("C# harness csproj should be written");
    fs::write(
        project_dir.join("Program.cs"),
        r#"
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text.Json;
using ReferenceGame.Config;

static void Check(bool condition, string message)
{
    if (!condition)
    {
        throw new Exception(message);
    }
}

var dataDir = Environment.GetEnvironmentVariable("DATA_DIR") ?? throw new Exception("DATA_DIR missing");
var loader = new JsonRuntimeDataLoader(dataDir);
var data = GameData.Load(loader);
Check(data.Items.GetById(2001)?.Name == "Iron Ore", "eager item lookup failed");
Check(data.GameRules.DefaultClass(data).Name == "Warrior", "relation lookup failed");
Check(data.GameRules.StarterItems(data).Count == 2, "list relation lookup failed");
Check(data.Items.GetById(2001)?.NameText(new TestTranslator()) == "铁矿石", "text helper translation failed");

var lazyLoader = new JsonRuntimeDataLoader(dataDir);
var lazy = new LazyGameData(lazyLoader);
Check(lazyLoader.OpenCount == 0, "lazy loader opened data before first access");
Check(lazy.Items.GetById(2001)?.Name == "Iron Ore", "lazy item lookup failed");
Check(lazyLoader.OpenCount == 1, "lazy loader should open items once");
Check(lazy.Items.GetById(2002)?.Name == "Oak Wood", "lazy cache lookup failed");
Check(lazyLoader.OpenCount == 1, "lazy cache should avoid duplicate load");
var eagerFromLazy = lazy.PreloadAll();
Check(eagerFromLazy.GameRules.DefaultClass(eagerFromLazy).Name == "Warrior", "preload relation failed");

sealed class TestTranslator : ITextTranslator
{
    public string? Translate(string context, string text)
    {
        return context == "items.name" && text == "Iron Ore" ? "铁矿石" : null;
    }
}

sealed class JsonRuntimeDataLoader : IConfigForgeRuntimeDataLoader
{
    private readonly string _root;
    public int OpenCount { get; private set; }

    public JsonRuntimeDataLoader(string root)
    {
        _root = root;
    }

    public IConfigForgeRuntimeDataReader Open(string path)
    {
        OpenCount++;
        return new JsonRuntimeDataReader(Path.Combine(_root, path + ".json"));
    }
}

sealed class JsonRuntimeDataReader : IConfigForgeRuntimeDataReader
{
    private readonly JsonDocument _document;
    private readonly JsonDocument _nullDocument = JsonDocument.Parse("null");
    private readonly Stack<JsonElement> _stack = new();

    public JsonRuntimeDataReader(string path)
    {
        _document = JsonDocument.Parse(File.ReadAllText(path));
        _stack.Push(_document.RootElement);
    }

    public void EnterRequiredField(string name)
    {
        var current = _stack.Peek();
        if (current.ValueKind == JsonValueKind.Object && current.TryGetProperty(name, out var value))
        {
            _stack.Push(value);
            return;
        }
        _stack.Push(_nullDocument.RootElement);
    }

    public bool EnterOptionalField(string name)
    {
        var current = _stack.Peek();
        if (current.ValueKind == JsonValueKind.Object && current.TryGetProperty(name, out var value) && value.ValueKind != JsonValueKind.Null)
        {
            _stack.Push(value);
            return true;
        }
        return false;
    }

    public void ExitField() => _stack.Pop();

    public int ReadRowCount()
    {
        var current = _stack.Peek();
        return current.ValueKind == JsonValueKind.Object && current.TryGetProperty("rows", out var rows) && rows.ValueKind == JsonValueKind.Array
            ? rows.GetArrayLength()
            : 1;
    }

    public void EnterRow(int index)
    {
        var rows = _stack.Peek().GetProperty("rows");
        _stack.Push(rows[index]);
    }

    public void ExitRow() => _stack.Pop();

    public int ReadListLength()
    {
        var current = _stack.Peek();
        return current.ValueKind == JsonValueKind.Array ? current.GetArrayLength() : 0;
    }

    public void EnterListElement(int index) => _stack.Push(_stack.Peek()[index]);
    public void ExitListElement() => _stack.Pop();

    public long ReadInt()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => long.Parse(current.GetString() ?? "0", CultureInfo.InvariantCulture),
            JsonValueKind.Number => current.GetInt64(),
            _ => 0,
        };
    }

    public double ReadFloat()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => double.Parse(current.GetString() ?? "0", CultureInfo.InvariantCulture),
            JsonValueKind.Number => current.GetDouble(),
            _ => 0.0,
        };
    }

    public bool ReadBool()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => bool.Parse(current.GetString() ?? "false"),
            JsonValueKind.True => true,
            JsonValueKind.False => false,
            _ => false,
        };
    }

    public string ReadString()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => current.GetString() ?? string.Empty,
            JsonValueKind.Number => current.GetRawText(),
            JsonValueKind.True => "true",
            JsonValueKind.False => "false",
            _ => string.Empty,
        };
    }
}
"#,
    )
    .expect("C# harness Program.cs should be written");

    let output = StdCommand::new("dotnet")
        .args(["run", "--project"])
        .arg(project_dir.join("ConfigForgeCSharpHarness.csproj"))
        .env("DATA_DIR", fixture_root.join("out/client_data/json"))
        .output()
        .expect("standard generated C# runtime test should run");
    assert_command_success(output, "standard generated C# runtime test");
}

pub(crate) fn verify_standard_generated_java_runtime(fixture_root: &std::path::Path) {
    if !command_exists("javac") || !command_exists("java") {
        eprintln!("skipping standard generated Java runtime test: javac/java not found");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("java harness temp dir should be created");
    let source_root = temp_dir.path().join("src");
    let package_dir = source_root.join("referencegame/config");
    let class_dir = temp_dir.path().join("classes");
    fs::create_dir_all(&package_dir).expect("Java package dir should be created");
    fs::create_dir_all(&class_dir).expect("Java class dir should be created");
    fs::copy(
        fixture_root.join("out/client_code/java/ReferenceGameConfig.java"),
        package_dir.join("ReferenceGameConfig.java"),
    )
    .expect("generated Java code should be copied into harness");
    fs::write(
        source_root.join("RuntimeCheck.java"),
        r#"
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Stack;
import referencegame.config.ReferenceGameConfig;

public final class RuntimeCheck {
    public static void main(String[] args) throws Exception {
        Path dataDir = Paths.get(System.getenv("DATA_DIR"));
        JsonRuntimeDataLoader loader = new JsonRuntimeDataLoader(dataDir);
        ReferenceGameConfig.GameData data = ReferenceGameConfig.GameData.load(loader);
        check(data.getItems().getById(2001).getName().equals("Iron Ore"), "eager item lookup failed");
        check(data.getGameRules().defaultClass(data).getName().equals("Warrior"), "relation lookup failed");
        check(data.getGameRules().starterItems(data).size() == 2, "list relation lookup failed");
        check(data.getItems().getById(2001).nameText(new TestTranslator()).equals("铁矿石"), "text helper translation failed");

        JsonRuntimeDataLoader lazyLoader = new JsonRuntimeDataLoader(dataDir);
        ReferenceGameConfig.LazyGameData lazy = new ReferenceGameConfig.LazyGameData(lazyLoader);
        check(lazyLoader.openCount == 0, "lazy loader opened data before first access");
        check(lazy.getItems().getById(2001).getName().equals("Iron Ore"), "lazy item lookup failed");
        check(lazyLoader.openCount == 1, "lazy loader should open items once");
        check(lazy.getItems().getById(2002).getName().equals("Oak Wood"), "lazy cache lookup failed");
        check(lazyLoader.openCount == 1, "lazy cache should avoid duplicate load");
        ReferenceGameConfig.GameData eagerFromLazy = lazy.preloadAll();
        check(eagerFromLazy.getGameRules().defaultClass(eagerFromLazy).getName().equals("Warrior"), "preload relation failed");
    }

    static void check(boolean condition, String message) {
        if (!condition) {
            throw new RuntimeException(message);
        }
    }

    static final class TestTranslator implements ReferenceGameConfig.TextTranslator {
        public String translate(String context, String text) {
            return context.equals("items.name") && text.equals("Iron Ore") ? "铁矿石" : null;
        }
    }

    static final class JsonRuntimeDataLoader implements ReferenceGameConfig.ConfigForgeRuntimeDataLoader {
        private final Path root;
        int openCount;

        JsonRuntimeDataLoader(Path root) {
            this.root = root;
        }

        public ReferenceGameConfig.ConfigForgeRuntimeDataReader open(String path) {
            openCount++;
            try {
                return new JsonRuntimeDataReader(Json.parse(new String(Files.readAllBytes(root.resolve(path + ".json")), StandardCharsets.UTF_8)));
            } catch (IOException error) {
                throw new RuntimeException(error);
            }
        }
    }

    static final class JsonRuntimeDataReader implements ReferenceGameConfig.ConfigForgeRuntimeDataReader {
        private final Stack<Object> stack = new Stack<>();

        JsonRuntimeDataReader(Object root) {
            stack.push(root);
        }

        public void enterRequiredField(String name) {
            Object current = stack.peek();
            if (current instanceof Map && ((Map<?, ?>) current).containsKey(name)) {
                Map<?, ?> map = (Map<?, ?>) current;
                stack.push(map.get(name));
            } else {
                stack.push(null);
            }
        }

        public boolean enterOptionalField(String name) {
            Object current = stack.peek();
            if (current instanceof Map && ((Map<?, ?>) current).containsKey(name) && ((Map<?, ?>) current).get(name) != null) {
                Map<?, ?> map = (Map<?, ?>) current;
                stack.push(map.get(name));
                return true;
            }
            return false;
        }

        public void exitField() { stack.pop(); }

        public int readRowCount() {
            Object current = stack.peek();
            if (current instanceof Map && ((Map<?, ?>) current).get("rows") instanceof List) {
                List<?> rows = (List<?>) ((Map<?, ?>) current).get("rows");
                return rows.size();
            }
            return 1;
        }

        public void enterRow(int index) {
            Map<?, ?> current = (Map<?, ?>) stack.peek();
            stack.push(((List<?>) current.get("rows")).get(index));
        }

        public void exitRow() { stack.pop(); }

        public int readListLength() { return ((List<?>) stack.peek()).size(); }
        public void enterListElement(int index) { stack.push(((List<?>) stack.peek()).get(index)); }
        public void exitListElement() { stack.pop(); }
        public long readInt() { return Long.parseLong(String.valueOf(stack.peek())); }
        public double readFloat() { return Double.parseDouble(String.valueOf(stack.peek())); }
        public boolean readBool() { return Boolean.parseBoolean(String.valueOf(stack.peek())); }
        public String readString() { Object value = stack.peek(); return value == null ? "" : String.valueOf(value); }
    }

    static final class Json {
        private final String text;
        private int index;

        private Json(String text) { this.text = text; }
        static Object parse(String text) { return new Json(text).readValue(); }

        private Object readValue() {
            skipWhitespace();
            char ch = text.charAt(index);
            if (ch == '{') { return readObject(); }
            if (ch == '[') { return readArray(); }
            if (ch == '"') { return readStringValue(); }
            if (text.startsWith("true", index)) { index += 4; return Boolean.TRUE; }
            if (text.startsWith("false", index)) { index += 5; return Boolean.FALSE; }
            if (text.startsWith("null", index)) { index += 4; return null; }
            return readNumber();
        }

        private Map<String, Object> readObject() {
            index++;
            skipWhitespace();
            Map<String, Object> result = new HashMap<>();
            if (text.charAt(index) == '}') { index++; return result; }
            while (true) {
                String key = readStringValue();
                skipWhitespace();
                expect(':');
                result.put(key, readValue());
                skipWhitespace();
                if (text.charAt(index) == '}') { index++; return result; }
                expect(',');
            }
        }

        private List<Object> readArray() {
            index++;
            skipWhitespace();
            List<Object> result = new ArrayList<>();
            if (text.charAt(index) == ']') { index++; return result; }
            while (true) {
                result.add(readValue());
                skipWhitespace();
                if (text.charAt(index) == ']') { index++; return result; }
                expect(',');
            }
        }

        private String readStringValue() {
            expect('"');
            StringBuilder builder = new StringBuilder();
            while (true) {
                char ch = text.charAt(index++);
                if (ch == '"') { return builder.toString(); }
                if (ch == '\\') {
                    char escaped = text.charAt(index++);
                    switch (escaped) {
                        case '"': builder.append('"'); break;
                        case '\\': builder.append('\\'); break;
                        case '/': builder.append('/'); break;
                        case 'b': builder.append('\b'); break;
                        case 'f': builder.append('\f'); break;
                        case 'n': builder.append('\n'); break;
                        case 'r': builder.append('\r'); break;
                        case 't': builder.append('\t'); break;
                        case 'u':
                            builder.append((char) Integer.parseInt(text.substring(index, index + 4), 16));
                            index += 4;
                            break;
                        default: throw new RuntimeException("invalid escape: " + escaped);
                    }
                } else {
                    builder.append(ch);
                }
            }
        }

        private String readNumber() {
            int start = index;
            while (index < text.length() && "-+0123456789.eE".indexOf(text.charAt(index)) >= 0) { index++; }
            return text.substring(start, index);
        }

        private void expect(char expected) {
            skipWhitespace();
            if (text.charAt(index) != expected) {
                throw new RuntimeException("expected " + expected + " at " + index);
            }
            index++;
            skipWhitespace();
        }

        private void skipWhitespace() {
            while (index < text.length() && Character.isWhitespace(text.charAt(index))) { index++; }
        }
    }
}
"#,
    )
    .expect("Java harness RuntimeCheck.java should be written");

    let generated = package_dir.join("ReferenceGameConfig.java");
    let runtime_check = source_root.join("RuntimeCheck.java");
    let output = StdCommand::new("javac")
        .arg("-d")
        .arg(&class_dir)
        .arg(&generated)
        .arg(&runtime_check)
        .output()
        .expect("standard generated Java javac should run");
    assert_command_success(output, "standard generated Java javac");

    let output = StdCommand::new("java")
        .arg("-cp")
        .arg(&class_dir)
        .arg("RuntimeCheck")
        .env("DATA_DIR", fixture_root.join("out/client_data/json"))
        .output()
        .expect("standard generated Java runtime test should run");
    assert_command_success(output, "standard generated Java runtime test");
}
