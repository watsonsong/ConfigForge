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
