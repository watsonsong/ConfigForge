// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, process::Command as StdCommand};

use super::{
    assert_command_success, assert_optional_skips_within_limit, command_exists,
    current_test_profile, network_allowed, require_any_tool_or_skip, require_tool_or_skip,
    skip_language_or_continue, skip_optional_test, skipped_optional_test_count,
    verify_standard_generated_cpp_unreal_runtime,
};

pub(crate) struct GeneratedRuntimeHarness {
    pub(crate) language: &'static str,
    pub(crate) test_name: &'static str,
    pub(crate) required_tools: &'static [&'static str],
    pub(crate) network_policy: &'static str,
    pub(crate) runner: fn(&std::path::Path, &std::path::Path),
}

pub(crate) fn standard_generated_runtime_harnesses() -> &'static [GeneratedRuntimeHarness] {
    &[
        GeneratedRuntimeHarness {
            language: "rust",
            test_name: "standard generated Rust runtime test",
            required_tools: &["cargo"],
            network_policy: "offline",
            runner: verify_standard_generated_rust_runtime,
        },
        GeneratedRuntimeHarness {
            language: "cpp",
            test_name: "standard generated C++ runtime test",
            required_tools: &["cmake", "c++"],
            network_policy: "offline unless FetchContent is explicitly allowed",
            runner: verify_standard_generated_cpp_runtime,
        },
        GeneratedRuntimeHarness {
            language: "cpp_unreal",
            test_name: "standard generated cpp_unreal syntax test",
            required_tools: &["c++"],
            network_policy: "offline",
            runner: verify_standard_generated_cpp_unreal_runtime,
        },
        GeneratedRuntimeHarness {
            language: "csharp",
            test_name: "standard generated C# runtime test",
            required_tools: &["dotnet"],
            network_policy: "offline",
            runner: verify_standard_generated_csharp_runtime,
        },
        GeneratedRuntimeHarness {
            language: "java",
            test_name: "standard generated Java runtime test",
            required_tools: &["javac", "java"],
            network_policy: "offline",
            runner: verify_standard_generated_java_runtime,
        },
        GeneratedRuntimeHarness {
            language: "typescript",
            test_name: "standard generated TypeScript runtime test",
            required_tools: &["node", "tsc"],
            network_policy: "npx fallback requires explicit network opt-in",
            runner: verify_standard_generated_typescript_runtime,
        },
        GeneratedRuntimeHarness {
            language: "lua",
            test_name: "standard generated Lua runtime test",
            required_tools: &["lua5.4|lua5.3|lua|luajit"],
            network_policy: "offline",
            runner: verify_standard_generated_lua_runtime,
        },
    ]
}

pub(crate) fn verify_standard_generated_runtime_harnesses(
    fixture_root: &std::path::Path,
    output_root: &std::path::Path,
) {
    let mut executed = 0usize;
    let mut skipped = 0usize;
    let mut failed = Vec::new();

    for harness in standard_generated_runtime_harnesses() {
        eprintln!(
            "running generated runtime harness `{}` for {} (tools: {}; network: {})",
            harness.test_name,
            harness.language,
            harness.required_tools.join(", "),
            harness.network_policy
        );
        let skips_before = skipped_optional_test_count();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (harness.runner)(fixture_root, output_root);
        }));
        if result.is_err() {
            failed.push(harness.test_name);
            continue;
        }
        if skipped_optional_test_count() > skips_before {
            skipped += 1;
        } else {
            executed += 1;
        }
    }

    eprintln!(
        "generated runtime harness summary: executed={executed}, skipped={skipped}, failed={}",
        failed.len()
    );
    assert_optional_skips_within_limit("generated runtime harnesses", skipped);
    assert!(
        failed.is_empty(),
        "generated runtime harnesses failed: {failed:?}"
    );
}

pub(crate) fn verify_standard_generated_rust_runtime(
    fixture_root: &std::path::Path,
    output_root: &std::path::Path,
) {
    let temp_dir = tempfile::tempdir().expect("rust harness temp dir should be created");
    let harness_dir = fixture_root.join("harness/rust");
    let harness_cargo_toml = fs::read_to_string(harness_dir.join("Cargo.toml"))
        .expect("rust harness Cargo.toml should be read");
    let harness_cargo_toml = harness_cargo_toml.replace(
        "../../../../../runtime/rust/reader_core",
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("runtime/rust/reader_core")
            .to_string_lossy()
            .replace('\\', "/"),
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
            output_root.join("client_code/rust/reference_game_config.rs"),
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
            .env("JSON_DATA_DIR", output_root.join("client_data/json"))
            .env(
                "I18N_MO",
                fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
            )
            .env("MSGPACK_DATA_DIR", output_root.join("client_data/msgpack"))
            .output()
            .expect("standard generated Rust JSON runtime test should run");
        assert_command_success(
            output,
            &format!("standard generated Rust JSON runtime test ({edition})"),
        );
        fs::copy(
            output_root.join("client_code/rust/reference_game_config.rs"),
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
            .env("JSON_DATA_DIR", output_root.join("client_data/json"))
            .env(
                "I18N_MO",
                fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
            )
            .env("MSGPACK_DATA_DIR", output_root.join("client_data/msgpack"))
            .output()
            .expect("standard generated Rust MessagePack runtime test should run");
        assert_command_success(
            output,
            &format!("standard generated Rust MessagePack runtime test ({edition})"),
        );

        fs::copy(
            output_root.join("client_code/rust/reference_game_config.rs"),
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
            .env("BIN_DATA_DIR", output_root.join("client_data/binary"))
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

        fs::write(temp_dir.path().join("lib.rs"), RUST_EXTERNAL_HARNESS)
            .expect("external Rust harness lib.rs should be written");
        fs::copy(
            output_root.join("client_code/rust_external/reference_game_config.rs"),
            temp_dir.path().join("generated.rs"),
        )
        .expect("generated external Rust code should be copied into harness");
        let output = StdCommand::new("cargo")
            .args(["test", "--quiet", "--manifest-path"])
            .arg(temp_dir.path().join("Cargo.toml"))
            .arg("generated_external_rust_loads_exported_json")
            .env(
                "CARGO_TARGET_DIR",
                temp_dir.path().join(format!("target-{edition}-external")),
            )
            .env("JSON_DATA_DIR", output_root.join("client_data/json"))
            .output()
            .expect("standard generated external Rust runtime test should run");
        assert_command_success(
            output,
            &format!("standard generated external Rust runtime test ({edition})"),
        );

        fs::copy(harness_dir.join("lib.rs"), temp_dir.path().join("lib.rs"))
            .expect("rust harness lib.rs should be restored");
    }
}

const RUST_EXTERNAL_HARNESS: &str = r#"
fn global_translate(_context: Option<&str>, _msgid: &str) -> Option<String> {
    None
}

include!("generated.rs");

mod json_runtime_reader;

#[test]
fn generated_external_rust_loads_exported_json() {
    let data_dir = std::env::var("JSON_DATA_DIR").expect("JSON_DATA_DIR should be set");
    let loader = json_runtime_reader::FsJsonRuntimeDataLoader::new(data_dir);
    let data = GameData::load(&loader).expect("external generated data should load from JSON");
    let item = data.items.get_by_id(2001).expect("item id should be indexed");
    assert_eq!(item.price, 5);
    assert_eq!(data.game_rules.max_player_level, 60);
}
"#;

pub(crate) fn verify_standard_generated_cpp_runtime(
    fixture_root: &std::path::Path,
    output_root: &std::path::Path,
) {
    if skip_language_or_continue("standard generated C++ runtime test", "cpp") {
        return;
    }
    if require_tool_or_skip("standard generated C++ runtime test", "cmake").is_none() {
        return;
    }

    let temp_dir = tempfile::tempdir().expect("cpp harness temp dir should be created");
    let runtime_cpp_dir = fixture_root.join("../../../runtime/cpp");
    let generated_cpp_dir = output_root.join("client_code/cpp");
    let generated_external_cpp_dir = output_root.join("client_code/cpp_external");
    let harness_cpp_dir = fixture_root.join("harness/cpp");
    let harness_cpp = fixture_root.join("harness/cpp/runtime_check.cpp");
    let generated_cpp = generated_cpp_dir.join("reference_game_config.cpp");
    let generated_external_cpp = generated_external_cpp_dir.join("reference_game_config.cpp");
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
                set(CONFIG_FORGE_RUNTIME_FETCH_DEPS {fetch_deps} CACHE BOOL "" FORCE)
                add_subdirectory({runtime_cpp_dir:?} config_forge_runtime)

                find_package(Intl QUIET)

                add_executable(standard_runtime_check
                    {harness_cpp:?}
                    {generated_cpp:?})
                target_include_directories(standard_runtime_check PRIVATE {generated_cpp_dir:?} {harness_cpp_dir:?})
                target_link_libraries(standard_runtime_check PRIVATE config_forge_runtime::rapidjson_reader)

                add_executable(standard_external_runtime_check
                    {harness_cpp:?}
                    {generated_external_cpp:?})
                target_include_directories(standard_external_runtime_check PRIVATE {generated_external_cpp_dir:?} {harness_cpp_dir:?})
                target_link_libraries(standard_external_runtime_check PRIVATE config_forge_runtime::rapidjson_reader)

                add_executable(standard_binary_runtime_check
                    {harness_cpp:?}
                    {binary_generated_cpp:?})
                target_include_directories(standard_binary_runtime_check PRIVATE {binary_generated_cpp_dir:?} {harness_cpp_dir:?})
                target_compile_definitions(standard_binary_runtime_check PRIVATE CONFIG_FORGE_USE_BINARY_RUNTIME_READER)
                target_link_libraries(standard_binary_runtime_check PRIVATE config_forge_runtime::binary_reader)

                if(Intl_FOUND)
                    target_compile_definitions(standard_runtime_check PRIVATE CONFIG_FORGE_USE_LIBINTL)
                    target_compile_definitions(standard_external_runtime_check PRIVATE CONFIG_FORGE_USE_LIBINTL)
                    target_compile_definitions(standard_binary_runtime_check PRIVATE CONFIG_FORGE_USE_LIBINTL)
                    target_link_libraries(standard_runtime_check PRIVATE Intl::Intl)
                    target_link_libraries(standard_external_runtime_check PRIVATE Intl::Intl)
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
            generated_external_cpp = generated_external_cpp.display().to_string(),
            binary_generated_cpp = generated_cpp.display().to_string(),
            generated_cpp_dir = generated_cpp_dir.display().to_string(),
            generated_external_cpp_dir = generated_external_cpp_dir.display().to_string(),
            binary_generated_cpp_dir = generated_cpp_dir.display().to_string(),
            harness_cpp_dir = harness_cpp_dir.display().to_string(),
            temp_dir = temp_dir.path().display().to_string(),
            fetch_deps = if network_allowed() { "ON" } else { "OFF" }
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
    if !output.status.success() && !network_allowed() {
        skip_optional_test(
            "standard generated C++ runtime test",
            "C++ runtime dependencies are unavailable and CONFIG_FORGE_ALLOW_NETWORK=1 is not set",
        );
        return;
    }
    assert_command_success(output, "standard generated C++ CMake configure");

    let output = StdCommand::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .output()
        .expect("standard generated C++ CMake build should run");
    assert_command_success(output, "standard generated C++ CMake build");

    let output = StdCommand::new(build_dir.join("standard_runtime_check"))
        .env("DATA_DIR", output_root.join("client_data/json"))
        .env("I18N_LOCALE_DIR", &gettext_locale_dir)
        .env("LANGUAGE", "zh_CN")
        .env(
            "I18N_MO",
            fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
        )
        .output()
        .expect("standard generated C++ runtime test should run");
    assert_command_success(output, "standard generated C++ runtime test");

    let output = StdCommand::new(build_dir.join("standard_external_runtime_check"))
        .env("DATA_DIR", output_root.join("client_data/json"))
        .env("I18N_LOCALE_DIR", &gettext_locale_dir)
        .env("LANGUAGE", "zh_CN")
        .env(
            "I18N_MO",
            fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
        )
        .output()
        .expect("standard generated external C++ runtime test should run");
    assert_command_success(output, "standard generated external C++ runtime test");

    let output = StdCommand::new(build_dir.join("standard_binary_runtime_check"))
        .env("DATA_DIR", output_root.join("client_data/binary"))
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
pub(crate) fn verify_standard_generated_typescript_runtime(
    fixture_root: &std::path::Path,
    output_root: &std::path::Path,
) {
    if skip_language_or_continue("standard generated TypeScript runtime test", "typescript") {
        return;
    }
    if require_tool_or_skip("standard generated TypeScript runtime test", "node").is_none() {
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
        output_root.join("client_code/typescript/reference_game_config.ts"),
        temp_dir.path().join("generated.ts"),
    )
    .expect("generated TypeScript code should be copied into harness");

    let msgpack_package_dir = temp_dir.path().join("node_modules/@msgpack/msgpack");
    fs::create_dir_all(&msgpack_package_dir)
        .expect("TypeScript MessagePack backend package dir should be created");
    fs::write(
        msgpack_package_dir.join("package.json"),
        r#"{"type":"commonjs","main":"index.js","types":"index.d.ts"}"#,
    )
    .expect("TypeScript MessagePack backend package metadata should be written");
    fs::write(msgpack_package_dir.join("index.js"), msgpack_backend_shim())
        .expect("TypeScript MessagePack backend shim should be written");
    fs::write(
        msgpack_package_dir.join("index.d.ts"),
        "export function decode(bytes: Uint8Array): unknown;\n",
    )
    .expect("TypeScript MessagePack backend types should be written");

    if command_exists("tsc") {
        assert_command_success(
            StdCommand::new("tsc")
                .args(["--project"])
                .arg(temp_dir.path().join("tsconfig.json"))
                .output()
                .expect("standard generated TypeScript tsc should run"),
            "standard generated TypeScript tsc",
        );
    } else if current_test_profile().requires_optional_tools() {
        let _ = require_tool_or_skip("standard generated TypeScript runtime test", "tsc");
        return;
    } else if command_exists("npx") && network_allowed() {
        assert_command_success(
            StdCommand::new("npx")
                .args(["--yes", "-p", "typescript", "tsc", "--project"])
                .arg(temp_dir.path().join("tsconfig.json"))
                .output()
                .expect("standard generated TypeScript npx tsc should run"),
            "standard generated TypeScript npx tsc",
        );
    } else if command_exists("npx") {
        skip_optional_test(
            "standard generated TypeScript runtime test",
            "npx fallback requires CONFIG_FORGE_ALLOW_NETWORK=1",
        );
        return;
    } else {
        let _ = require_any_tool_or_skip(
            "standard generated TypeScript runtime test",
            &["tsc", "npx"],
        );
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
        .env("DATA_DIR", output_root.join("client_data/json"))
        .env("MSGPACK_DATA_DIR", output_root.join("client_data/msgpack"))
        .env("BIN_DATA_DIR", output_root.join("client_data/binary"))
        .env(
            "I18N_MO",
            fixture_root.join("i18n/zh_CN/reference_game_config.mo"),
        )
        .output()
        .expect("standard generated TypeScript runtime test should run");
    assert_command_success(output, "standard generated TypeScript runtime test");
}

fn msgpack_backend_shim() -> &'static str {
    r#"
exports.decode = function decode(bytes) {
  const decoder = new MessagePackDecoder(bytes);
  const value = decoder.readValue();
  decoder.ensureExhausted();
  return value;
};

class MessagePackDecoder {
  constructor(bytes) {
    this.bytes = bytes;
    this.offset = 0;
    this.textDecoder = new TextDecoder("utf-8", { fatal: true });
  }

  readValue() {
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
      case 0xca: return this.readFloat32();
      case 0xcb: return this.readFloat64();
      case 0xcc: return this.readU8();
      case 0xcd: return this.readU16();
      case 0xce: return this.readU32();
      case 0xd0: return this.readI8();
      case 0xd1: return this.readI16();
      case 0xd2: return this.readI32();
      case 0xd9: return this.readString(this.readU8());
      case 0xda: return this.readString(this.readU16());
      case 0xdc: return this.readArray(this.readU16());
      case 0xde: return this.readMap(this.readU16());
      default: throw new Error(`unsupported MessagePack tag 0x${tag.toString(16)}`);
    }
  }

  ensureExhausted() {
    if (this.offset !== this.bytes.length) {
      throw new Error("trailing MessagePack bytes");
    }
  }

  readArray(length) {
    return Array.from({ length }, () => this.readValue());
  }

  readMap(length) {
    const value = {};
    for (let index = 0; index < length; index += 1) {
      const key = this.readValue();
      if (typeof key !== "string") {
        throw new Error("unsupported non-string map key");
      }
      value[key] = this.readValue();
    }
    return value;
  }

  readString(length) {
    return this.textDecoder.decode(this.readExact(length));
  }

  readFloat32() {
    const bytes = this.readExact(4);
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getFloat32(0, false);
  }

  readFloat64() {
    const bytes = this.readExact(8);
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getFloat64(0, false);
  }

  readU8() { return this.readExact(1)[0]; }
  readU16() { const bytes = this.readExact(2); return (bytes[0] << 8) | bytes[1]; }
  readU32() { const bytes = this.readExact(4); return bytes[0] * 0x1000000 + ((bytes[1] << 16) | (bytes[2] << 8) | bytes[3]); }
  readI8() { const value = this.readU8(); return value & 0x80 ? value - 0x100 : value; }
  readI16() { const value = this.readU16(); return value & 0x8000 ? value - 0x10000 : value; }
  readI32() { const value = this.readU32(); return value > 0x7fffffff ? value - 0x100000000 : value; }

  readExact(length) {
    if (this.offset + length > this.bytes.length) {
      throw new Error("truncated MessagePack payload");
    }
    const value = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return value;
  }
}
"#
}
pub(crate) fn verify_standard_generated_lua_runtime(
    fixture_root: &std::path::Path,
    output_root: &std::path::Path,
) {
    if skip_language_or_continue("standard generated Lua runtime test", "lua") {
        return;
    }
    let Some(lua_probe) = require_any_tool_or_skip(
        "standard generated Lua runtime test",
        &["lua5.4", "lua5.3", "lua", "luajit"],
    ) else {
        return;
    };

    let output = StdCommand::new(&lua_probe.command)
        .arg(fixture_root.join("harness/lua/runtime_check.lua"))
        .env("LUA_CONFIG_ROOT", output_root.join("client_lua"))
        .output()
        .expect("standard generated Lua runtime test should run");
    assert_command_success(output, "standard generated Lua runtime test");
}

pub(crate) fn verify_standard_generated_csharp_runtime(
    fixture_root: &std::path::Path,
    output_root: &std::path::Path,
) {
    if skip_language_or_continue("standard generated C# runtime test", "csharp") {
        return;
    }
    if require_tool_or_skip("standard generated C# runtime test", "dotnet").is_none() {
        return;
    }

    let temp_dir = tempfile::tempdir().expect("csharp harness temp dir should be created");
    let project_dir = temp_dir.path();
    fs::copy(
        output_root.join("client_code/csharp/reference_game_config.cs"),
        project_dir.join("ReferenceGameConfig.cs"),
    )
    .expect("generated C# code should be copied into harness");
    fs::copy(
        fixture_root.join("../../../runtime/csharp/json_reader/JsonRuntimeDataReader.cs"),
        project_dir.join("JsonRuntimeDataReader.cs"),
    )
    .expect("C# JSON runtime reader should be copied into harness");
    fs::copy(
        fixture_root.join(
            "../../../runtime/csharp/system_text_json_reader/SystemTextJsonRuntimeDataReader.cs",
        ),
        project_dir.join("SystemTextJsonRuntimeDataReader.cs"),
    )
    .expect("C# System.Text.Json runtime reader should be copied into harness");
    fs::copy(
        fixture_root
            .join("../../../runtime/csharp/messagepack_reader/MessagePackRuntimeDataReader.cs"),
        project_dir.join("MessagePackRuntimeDataReader.cs"),
    )
    .expect("C# MessagePack runtime reader should be copied into harness");
    fs::copy(
        fixture_root.join("../../../runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs"),
        project_dir.join("BinaryRuntimeDataReader.cs"),
    )
    .expect("C# binary runtime reader should be copied into harness");
    fs::write(
        project_dir.join("ConfigForgeCSharpHarness.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
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
using ConfigForge.Runtime.BinaryReader;
using ConfigForge.Runtime.JsonReader;
using ConfigForge.Runtime.MessagePackReader;
using ReferenceGame.Config;

static void Check(bool condition, string message)
{
    if (!condition)
    {
        throw new Exception(message);
    }
}

var dataDir = Environment.GetEnvironmentVariable("DATA_DIR") ?? throw new Exception("DATA_DIR missing");
var msgpackDataDir = Environment.GetEnvironmentVariable("MSGPACK_DATA_DIR") ?? throw new Exception("MSGPACK_DATA_DIR missing");
var binaryDataDir = Environment.GetEnvironmentVariable("BIN_DATA_DIR") ?? throw new Exception("BIN_DATA_DIR missing");

CheckRuntime(new JsonRuntimeDataLoader(dataDir), "json eager");
CheckLazyRuntime(new JsonRuntimeDataLoader(dataDir), "json lazy");
CheckRuntime(new MessagePackRuntimeDataLoader(msgpackDataDir), "messagepack eager");
CheckLazyRuntime(new MessagePackRuntimeDataLoader(msgpackDataDir), "messagepack lazy");
CheckRuntime(new BinaryRuntimeDataLoader(binaryDataDir), "binary eager");
CheckLazyRuntime(new BinaryRuntimeDataLoader(binaryDataDir), "binary lazy");
CheckRuntime(new ConfigForgeJsonRuntimeDataLoaderAdapter(dataDir), "json generated adapter eager");
CheckLazyRuntime(new CountingAdapter(new ConfigForgeJsonRuntimeDataLoaderAdapter(dataDir)), "json generated adapter lazy");
CheckRuntime(new ConfigForgeMessagePackRuntimeDataLoaderAdapter(msgpackDataDir), "messagepack generated adapter eager");
CheckLazyRuntime(new CountingAdapter(new ConfigForgeMessagePackRuntimeDataLoaderAdapter(msgpackDataDir)), "messagepack generated adapter lazy");
CheckRuntime(new ConfigForgeBinaryRuntimeDataLoaderAdapter(binaryDataDir), "binary generated adapter eager");
CheckLazyRuntime(new CountingAdapter(new ConfigForgeBinaryRuntimeDataLoaderAdapter(binaryDataDir)), "binary generated adapter lazy");

static void CheckRuntime(IConfigForgeRuntimeDataLoader loader, string label)
{
    var data = GameData.Load(loader);
    Check(data.Items.GetById(2001)?.Name == "Iron Ore", label + ": eager item lookup failed");
    Check(data.Items.GetById(2001)?.Rarity(data).RarityId == 2, label + ": item rarity relation failed");
    Check(data.RarityLevels.GetById(5)?.RarityId == 5, label + ": rarity id index failed");
    Check(data.RarityLevels.GetById(2)?.StandardColor.R == 255.0, label + ": rarity color field load failed");
    Check(data.GameRules.DefaultClass(data).Name == "Warrior", label + ": relation lookup failed");
    Check(data.GameRules.StarterItems(data).Count == 2, label + ": list relation lookup failed");
    Check(data.GameRules.SpawnPosition.X == 1.5 && data.GameRules.SpawnPosition.Z == 3.5, label + ": vector field load failed");
    Check(data.GameRules.GridSize.X == 16 && data.GameRules.GridSize.Y == 9, label + ": int vector field load failed");
    Check(data.Items.GetById(2001)?.NameText(new TestTranslator()) == "铁矿石", label + ": text helper translation failed");
}

static void CheckLazyRuntime(ICountingRuntimeDataLoader loader, string label)
{
    var lazy = new LazyGameData(loader);
    Check(loader.OpenCount == 0, label + ": lazy loader opened data before first access");
    Check(lazy.Items.GetById(2001)?.Name == "Iron Ore", label + ": lazy item lookup failed");
    Check(loader.OpenCount == 1, label + ": lazy loader should open items once");
    Check(lazy.Items.GetById(2002)?.Name == "Oak Wood", label + ": lazy cache lookup failed");
    Check(loader.OpenCount == 1, label + ": lazy cache should avoid duplicate load");
    var eagerFromLazy = lazy.PreloadAll();
    Check(eagerFromLazy.GameRules.DefaultClass(eagerFromLazy).Name == "Warrior", label + ": preload relation failed");
    Check(eagerFromLazy.RarityLevels.GetById(2)?.StandardColor.B == 255.0, label + ": preload rarity color field failed");
    Check(eagerFromLazy.GameRules.SpawnPosition.Y == 2.5, label + ": preload vector field failed");
}

sealed class TestTranslator : ITextTranslator
{
    public string? Translate(string context, string text)
    {
        return context == "items.name" && text == "Iron Ore" ? "铁矿石" : null;
    }
}

interface ICountingRuntimeDataLoader : IConfigForgeRuntimeDataLoader
{
    int OpenCount { get; }
}

sealed class CountingAdapter : ICountingRuntimeDataLoader
{
    private readonly IConfigForgeRuntimeDataLoader _loader;

    public CountingAdapter(IConfigForgeRuntimeDataLoader loader)
    {
        _loader = loader;
    }

    public int OpenCount { get; private set; }

    public IConfigForgeRuntimeDataReader Open(string path)
    {
        OpenCount++;
        return _loader.Open(path);
    }
}

sealed class JsonRuntimeDataLoader : ICountingRuntimeDataLoader
{
    private readonly JsonRuntimeDataFileLoader _loader;
    public int OpenCount => _loader.OpenCount;

    public JsonRuntimeDataLoader(string root)
    {
        _loader = new JsonRuntimeDataFileLoader(root);
    }

    public IConfigForgeRuntimeDataReader Open(string path)
    {
        return new JsonRuntimeDataReaderAdapter(_loader.OpenReader(path));
    }
}

sealed class MessagePackRuntimeDataLoader : ICountingRuntimeDataLoader
{
    private readonly MessagePackRuntimeDataFileLoader _loader;
    public int OpenCount => _loader.OpenCount;

    public MessagePackRuntimeDataLoader(string root)
    {
        _loader = new MessagePackRuntimeDataFileLoader(root);
    }

    public IConfigForgeRuntimeDataReader Open(string path)
    {
        return new MessagePackRuntimeDataReaderAdapter(_loader.OpenReader(path));
    }
}

sealed class BinaryRuntimeDataLoader : ICountingRuntimeDataLoader
{
    private readonly BinaryRuntimeDataFileLoader _loader;
    public int OpenCount => _loader.OpenCount;

    public BinaryRuntimeDataLoader(string root)
    {
        _loader = new BinaryRuntimeDataFileLoader(root);
    }

    public IConfigForgeRuntimeDataReader Open(string path)
    {
        return new BinaryRuntimeDataReaderAdapter(_loader.OpenReader(path));
    }
}

sealed class JsonRuntimeDataReaderAdapter : IConfigForgeRuntimeDataReader
{
    private readonly JsonRuntimeDataReader _reader;

    public JsonRuntimeDataReaderAdapter(JsonRuntimeDataReader reader)
    {
        _reader = reader;
    }

    public void EnterRequiredField(string name) => _reader.EnterRequiredField(name);
    public bool EnterOptionalField(string name) => _reader.EnterOptionalField(name);
    public void ExitField() => _reader.ExitField();
    public ulong? SchemaRevision() => _reader.SchemaRevision();
    public int ReadRowCount() => _reader.ReadRowCount();
    public void EnterRow(int index) => _reader.EnterRow(index);
    public void ExitRow() => _reader.ExitRow();
    public int ReadListLength() => _reader.ReadListLength();
    public void EnterListElement(int index) => _reader.EnterListElement(index);
    public void ExitListElement() => _reader.ExitListElement();
    public void BeginRecord(int optionalSlotCount) => _reader.BeginRecord(optionalSlotCount);
    public void EndRecord() => _reader.EndRecord();
    public int ReadEnumIndex(IReadOnlyList<string> variants) => _reader.ReadEnumIndex(variants);
    public void EnsureExhausted() => _reader.EnsureExhausted();
    public long ReadInt() => _reader.ReadInt();
    public double ReadFloat() => _reader.ReadFloat();
    public bool ReadBool() => _reader.ReadBool();
    public string ReadString() => _reader.ReadString();
}

sealed class MessagePackRuntimeDataReaderAdapter : IConfigForgeRuntimeDataReader
{
    private readonly MessagePackRuntimeDataReader _reader;

    public MessagePackRuntimeDataReaderAdapter(MessagePackRuntimeDataReader reader)
    {
        _reader = reader;
    }

    public void EnterRequiredField(string name) => _reader.EnterRequiredField(name);
    public bool EnterOptionalField(string name) => _reader.EnterOptionalField(name);
    public void ExitField() => _reader.ExitField();
    public ulong? SchemaRevision() => _reader.SchemaRevision();
    public int ReadRowCount() => _reader.ReadRowCount();
    public void EnterRow(int index) => _reader.EnterRow(index);
    public void ExitRow() => _reader.ExitRow();
    public int ReadListLength() => _reader.ReadListLength();
    public void EnterListElement(int index) => _reader.EnterListElement(index);
    public void ExitListElement() => _reader.ExitListElement();
    public void BeginRecord(int optionalSlotCount) => _reader.BeginRecord(optionalSlotCount);
    public void EndRecord() => _reader.EndRecord();
    public int ReadEnumIndex(IReadOnlyList<string> variants) => _reader.ReadEnumIndex(variants);
    public void EnsureExhausted() => _reader.EnsureExhausted();
    public long ReadInt() => _reader.ReadInt();
    public double ReadFloat() => _reader.ReadFloat();
    public bool ReadBool() => _reader.ReadBool();
    public string ReadString() => _reader.ReadString();
}

sealed class BinaryRuntimeDataReaderAdapter : IConfigForgeRuntimeDataReader
{
    private readonly BinaryRuntimeDataReader _reader;

    public BinaryRuntimeDataReaderAdapter(BinaryRuntimeDataReader reader)
    {
        _reader = reader;
    }

    public void EnterRequiredField(string name) => _reader.EnterRequiredField(name);
    public bool EnterOptionalField(string name) => _reader.EnterOptionalField(name);
    public void ExitField() => _reader.ExitField();
    public ulong? SchemaRevision() => _reader.SchemaRevision();
    public int ReadRowCount() => _reader.ReadRowCount();
    public void EnterRow(int index) => _reader.EnterRow(index);
    public void ExitRow() => _reader.ExitRow();
    public int ReadListLength() => _reader.ReadListLength();
    public void EnterListElement(int index) => _reader.EnterListElement(index);
    public void ExitListElement() => _reader.ExitListElement();
    public void BeginRecord(int optionalSlotCount) => _reader.BeginRecord(optionalSlotCount);
    public void EndRecord() => _reader.EndRecord();
    public int ReadEnumIndex(IReadOnlyList<string> variants) => _reader.ReadEnumIndex(variants);
    public void EnsureExhausted() => _reader.EnsureExhausted();
    public long ReadInt() => _reader.ReadInt();
    public double ReadFloat() => _reader.ReadFloat();
    public bool ReadBool() => _reader.ReadBool();
    public string ReadString() => _reader.ReadString();
}
"#,
    )
    .expect("C# harness Program.cs should be written");

    let output = StdCommand::new("dotnet")
        .args(["run", "--project"])
        .arg(project_dir.join("ConfigForgeCSharpHarness.csproj"))
        .env("DATA_DIR", output_root.join("client_data/json"))
        .env("MSGPACK_DATA_DIR", output_root.join("client_data/msgpack"))
        .env("BIN_DATA_DIR", output_root.join("client_data/binary"))
        .output()
        .expect("standard generated C# runtime test should run");
    assert_command_success(output, "standard generated C# runtime test");
}

pub(crate) fn verify_standard_generated_java_runtime(
    fixture_root: &std::path::Path,
    output_root: &std::path::Path,
) {
    if skip_language_or_continue("standard generated Java runtime test", "java") {
        return;
    }
    if require_tool_or_skip("standard generated Java runtime test", "javac").is_none()
        || require_tool_or_skip("standard generated Java runtime test", "java").is_none()
    {
        return;
    }

    let temp_dir = tempfile::tempdir().expect("java harness temp dir should be created");
    let source_root = temp_dir.path().join("src");
    let package_dir = source_root.join("referencegame/config");
    let runtime_package_dir = source_root.join("configforge/runtime/json");
    let msgpack_runtime_package_dir = source_root.join("configforge/runtime/messagepack");
    let binary_runtime_package_dir = source_root.join("configforge/runtime/binary");
    let class_dir = temp_dir.path().join("classes");
    fs::create_dir_all(&package_dir).expect("Java package dir should be created");
    fs::create_dir_all(&runtime_package_dir).expect("Java runtime package dir should be created");
    fs::create_dir_all(&msgpack_runtime_package_dir)
        .expect("Java MessagePack runtime package dir should be created");
    fs::create_dir_all(&binary_runtime_package_dir)
        .expect("Java binary runtime package dir should be created");
    fs::create_dir_all(&class_dir).expect("Java class dir should be created");
    fs::copy(
        output_root.join("client_code/java/ReferenceGameConfig.java"),
        package_dir.join("ReferenceGameConfig.java"),
    )
    .expect("generated Java code should be copied into harness");
    let runtime_java_dir =
        fixture_root.join("../../../runtime/java/json_reader/src/configforge/runtime/json");
    for file_name in [
        "ConfigForgeJsonRuntimeDataLoader.java",
        "ConfigForgeJsonRuntimeDataReader.java",
        "ConfigForgeJsonParser.java",
        "ConfigForgeRuntimeErrorKind.java",
        "ConfigForgeRuntimeException.java",
    ] {
        fs::copy(
            runtime_java_dir.join(file_name),
            runtime_package_dir.join(file_name),
        )
        .expect("Java JSON runtime reader should be copied into harness");
    }
    let msgpack_runtime_java_dir = fixture_root
        .join("../../../runtime/java/messagepack_reader/src/configforge/runtime/messagepack");
    for file_name in [
        "ConfigForgeMessagePackRuntimeDataLoader.java",
        "ConfigForgeMessagePackRuntimeDataReader.java",
        "ConfigForgeMessagePackParser.java",
        "ConfigForgeRuntimeErrorKind.java",
        "ConfigForgeRuntimeException.java",
    ] {
        fs::copy(
            msgpack_runtime_java_dir.join(file_name),
            msgpack_runtime_package_dir.join(file_name),
        )
        .expect("Java MessagePack runtime reader should be copied into harness");
    }
    let binary_runtime_java_dir =
        fixture_root.join("../../../runtime/java/binary_reader/src/configforge/runtime/binary");
    for file_name in [
        "ConfigForgeBinaryRuntimeDataLoader.java",
        "ConfigForgeBinaryRuntimeDataReader.java",
        "ConfigForgeRuntimeErrorKind.java",
        "ConfigForgeRuntimeException.java",
    ] {
        fs::copy(
            binary_runtime_java_dir.join(file_name),
            binary_runtime_package_dir.join(file_name),
        )
        .expect("Java binary runtime reader should be copied into harness");
    }
    fs::write(
        source_root.join("RuntimeCheck.java"),
        r#"
import java.nio.file.Path;
import java.nio.file.Paths;
import configforge.runtime.binary.ConfigForgeBinaryRuntimeDataLoader;
import configforge.runtime.binary.ConfigForgeBinaryRuntimeDataReader;
import configforge.runtime.json.ConfigForgeJsonRuntimeDataLoader;
import configforge.runtime.json.ConfigForgeJsonRuntimeDataReader;
import configforge.runtime.messagepack.ConfigForgeMessagePackRuntimeDataLoader;
import configforge.runtime.messagepack.ConfigForgeMessagePackRuntimeDataReader;
import referencegame.config.ReferenceGameConfig;

public final class RuntimeCheck {
    public static void main(String[] args) throws Exception {
        Path dataDir = Paths.get(System.getenv("DATA_DIR"));
        Path msgpackDataDir = Paths.get(System.getenv("MSGPACK_DATA_DIR"));
        Path binaryDataDir = Paths.get(System.getenv("BIN_DATA_DIR"));
        checkRuntime(new JsonRuntimeDataLoader(dataDir), "json eager");
        checkLazyRuntime(new JsonRuntimeDataLoader(dataDir), "json lazy");
        checkRuntime(new MessagePackRuntimeDataLoader(msgpackDataDir), "messagepack eager");
        checkLazyRuntime(new MessagePackRuntimeDataLoader(msgpackDataDir), "messagepack lazy");
        checkRuntime(new BinaryRuntimeDataLoader(binaryDataDir), "binary eager");
        checkLazyRuntime(new BinaryRuntimeDataLoader(binaryDataDir), "binary lazy");
        checkRuntime(new ReferenceGameConfig.ConfigForgeJsonRuntimeDataLoaderAdapter(dataDir), "json generated adapter eager");
        checkLazyRuntime(new CountingAdapter(new ReferenceGameConfig.ConfigForgeJsonRuntimeDataLoaderAdapter(dataDir)), "json generated adapter lazy");
        checkRuntime(new ReferenceGameConfig.ConfigForgeMessagePackRuntimeDataLoaderAdapter(msgpackDataDir), "messagepack generated adapter eager");
        checkLazyRuntime(new CountingAdapter(new ReferenceGameConfig.ConfigForgeMessagePackRuntimeDataLoaderAdapter(msgpackDataDir)), "messagepack generated adapter lazy");
        checkRuntime(new ReferenceGameConfig.ConfigForgeBinaryRuntimeDataLoaderAdapter(binaryDataDir), "binary generated adapter eager");
        checkLazyRuntime(new CountingAdapter(new ReferenceGameConfig.ConfigForgeBinaryRuntimeDataLoaderAdapter(binaryDataDir)), "binary generated adapter lazy");
    }

    static void checkRuntime(ReferenceGameConfig.ConfigForgeRuntimeDataLoader loader, String label) {
        ReferenceGameConfig.GameData data = ReferenceGameConfig.GameData.load(loader);
        check(data.getItems().getById(2001).getName().equals("Iron Ore"), label + ": eager item lookup failed");
        check(data.getItems().getById(2001).rarity(data).getRarityId() == 2, label + ": item rarity relation failed");
        check(data.getRarityLevels().getById(5).getRarityId() == 5, label + ": rarity id index failed");
        check(data.getRarityLevels().getById(2).getStandardColor().r() == 255.0, label + ": rarity color field load failed");
        check(data.getGameRules().defaultClass(data).getName().equals("Warrior"), label + ": relation lookup failed");
        check(data.getGameRules().starterItems(data).size() == 2, label + ": list relation lookup failed");
        check(data.getGameRules().getSpawnPosition().x() == 1.5 && data.getGameRules().getSpawnPosition().z() == 3.5, label + ": vector field load failed");
        check(data.getGameRules().getGridSize().x() == 16 && data.getGameRules().getGridSize().y() == 9, label + ": int vector field load failed");
        check(data.getItems().getById(2001).nameText(new TestTranslator()).equals("铁矿石"), label + ": text helper translation failed");
    }

    static void checkLazyRuntime(CountingRuntimeDataLoader lazyLoader, String label) {
        ReferenceGameConfig.LazyGameData lazy = new ReferenceGameConfig.LazyGameData(lazyLoader);
        check(lazyLoader.openCount() == 0, label + ": lazy loader opened data before first access");
        check(lazy.getItems().getById(2001).getName().equals("Iron Ore"), label + ": lazy item lookup failed");
        check(lazyLoader.openCount() == 1, label + ": lazy loader should open items once");
        check(lazy.getItems().getById(2002).getName().equals("Oak Wood"), label + ": lazy cache lookup failed");
        check(lazyLoader.openCount() == 1, label + ": lazy cache should avoid duplicate load");
        ReferenceGameConfig.GameData eagerFromLazy = lazy.preloadAll();
        check(eagerFromLazy.getGameRules().defaultClass(eagerFromLazy).getName().equals("Warrior"), label + ": preload relation failed");
        check(eagerFromLazy.getRarityLevels().getById(2).getStandardColor().b() == 255.0, label + ": preload rarity color field failed");
        check(eagerFromLazy.getGameRules().getSpawnPosition().y() == 2.5, label + ": preload vector field failed");
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

    interface CountingRuntimeDataLoader extends ReferenceGameConfig.ConfigForgeRuntimeDataLoader {
        int openCount();
    }

    static final class CountingAdapter implements CountingRuntimeDataLoader {
        private final ReferenceGameConfig.ConfigForgeRuntimeDataLoader loader;
        int openCount;

        CountingAdapter(ReferenceGameConfig.ConfigForgeRuntimeDataLoader loader) {
            this.loader = loader;
        }

        public int openCount() {
            return openCount;
        }

        public ReferenceGameConfig.ConfigForgeRuntimeDataReader open(String path) {
            openCount++;
            return loader.open(path);
        }
    }

    static final class JsonRuntimeDataLoader implements CountingRuntimeDataLoader {
        private final ConfigForgeJsonRuntimeDataLoader loader;
        int openCount;

        JsonRuntimeDataLoader(Path root) {
            this.loader = new ConfigForgeJsonRuntimeDataLoader(root);
        }

        public int openCount() {
            return openCount;
        }

        public ReferenceGameConfig.ConfigForgeRuntimeDataReader open(String path) {
            openCount++;
            return new JsonRuntimeDataReaderAdapter(loader.openReader(path));
        }
    }

    static final class MessagePackRuntimeDataLoader implements CountingRuntimeDataLoader {
        private final ConfigForgeMessagePackRuntimeDataLoader loader;
        int openCount;

        MessagePackRuntimeDataLoader(Path root) {
            this.loader = new ConfigForgeMessagePackRuntimeDataLoader(root);
        }

        public int openCount() {
            return openCount;
        }

        public ReferenceGameConfig.ConfigForgeRuntimeDataReader open(String path) {
            openCount++;
            return new MessagePackRuntimeDataReaderAdapter(loader.openReader(path));
        }
    }

    static final class BinaryRuntimeDataLoader implements CountingRuntimeDataLoader {
        private final ConfigForgeBinaryRuntimeDataLoader loader;
        int openCount;

        BinaryRuntimeDataLoader(Path root) {
            this.loader = new ConfigForgeBinaryRuntimeDataLoader(root);
        }

        public int openCount() {
            return openCount;
        }

        public ReferenceGameConfig.ConfigForgeRuntimeDataReader open(String path) {
            openCount++;
            return new BinaryRuntimeDataReaderAdapter(loader.openReader(path));
        }
    }

    static final class JsonRuntimeDataReaderAdapter implements ReferenceGameConfig.ConfigForgeRuntimeDataReader {
        private final ConfigForgeJsonRuntimeDataReader reader;

        JsonRuntimeDataReaderAdapter(ConfigForgeJsonRuntimeDataReader reader) {
            this.reader = reader;
        }

        public void enterRequiredField(String name) { reader.enterRequiredField(name); }
        public boolean enterOptionalField(String name) { return reader.enterOptionalField(name); }
        public void exitField() { reader.exitField(); }
        public Long schemaRevision() { return reader.schemaRevision(); }
        public int readRowCount() { return reader.readRowCount(); }
        public void enterRow(int index) { reader.enterRow(index); }
        public void exitRow() { reader.exitRow(); }
        public int readListLength() { return reader.readListLength(); }
        public void enterListElement(int index) { reader.enterListElement(index); }
        public void exitListElement() { reader.exitListElement(); }
        public void beginRecord(int optionalSlotCount) { reader.beginRecord(optionalSlotCount); }
        public void endRecord() { reader.endRecord(); }
        public int readEnumIndex(String[] variants) { return reader.readEnumIndex(variants); }
        public void ensureExhausted() { reader.ensureExhausted(); }
        public long readInt() { return reader.readInt(); }
        public double readFloat() { return reader.readFloat(); }
        public boolean readBool() { return reader.readBool(); }
        public String readString() { return reader.readString(); }
    }

    static final class MessagePackRuntimeDataReaderAdapter implements ReferenceGameConfig.ConfigForgeRuntimeDataReader {
        private final ConfigForgeMessagePackRuntimeDataReader reader;

        MessagePackRuntimeDataReaderAdapter(ConfigForgeMessagePackRuntimeDataReader reader) {
            this.reader = reader;
        }

        public void enterRequiredField(String name) { reader.enterRequiredField(name); }
        public boolean enterOptionalField(String name) { return reader.enterOptionalField(name); }
        public void exitField() { reader.exitField(); }
        public Long schemaRevision() { return reader.schemaRevision(); }
        public int readRowCount() { return reader.readRowCount(); }
        public void enterRow(int index) { reader.enterRow(index); }
        public void exitRow() { reader.exitRow(); }
        public int readListLength() { return reader.readListLength(); }
        public void enterListElement(int index) { reader.enterListElement(index); }
        public void exitListElement() { reader.exitListElement(); }
        public void beginRecord(int optionalSlotCount) { reader.beginRecord(optionalSlotCount); }
        public void endRecord() { reader.endRecord(); }
        public int readEnumIndex(String[] variants) { return reader.readEnumIndex(variants); }
        public void ensureExhausted() { reader.ensureExhausted(); }
        public long readInt() { return reader.readInt(); }
        public double readFloat() { return reader.readFloat(); }
        public boolean readBool() { return reader.readBool(); }
        public String readString() { return reader.readString(); }
    }

    static final class BinaryRuntimeDataReaderAdapter implements ReferenceGameConfig.ConfigForgeRuntimeDataReader {
        private final ConfigForgeBinaryRuntimeDataReader reader;

        BinaryRuntimeDataReaderAdapter(ConfigForgeBinaryRuntimeDataReader reader) {
            this.reader = reader;
        }

        public void enterRequiredField(String name) { reader.enterRequiredField(name); }
        public boolean enterOptionalField(String name) { return reader.enterOptionalField(name); }
        public void exitField() { reader.exitField(); }
        public Long schemaRevision() { return reader.schemaRevision(); }
        public int readRowCount() { return reader.readRowCount(); }
        public void enterRow(int index) { reader.enterRow(index); }
        public void exitRow() { reader.exitRow(); }
        public int readListLength() { return reader.readListLength(); }
        public void enterListElement(int index) { reader.enterListElement(index); }
        public void exitListElement() { reader.exitListElement(); }
        public void beginRecord(int optionalSlotCount) { reader.beginRecord(optionalSlotCount); }
        public void endRecord() { reader.endRecord(); }
        public int readEnumIndex(String[] variants) { return reader.readEnumIndex(variants); }
        public void ensureExhausted() { reader.ensureExhausted(); }
        public long readInt() { return reader.readInt(); }
        public double readFloat() { return reader.readFloat(); }
        public boolean readBool() { return reader.readBool(); }
        public String readString() { return reader.readString(); }
    }
}
"#,
    )
    .expect("Java harness RuntimeCheck.java should be written");

    let generated = package_dir.join("ReferenceGameConfig.java");
    let runtime_check = source_root.join("RuntimeCheck.java");
    let output = StdCommand::new("javac")
        .args(["-encoding", "UTF-8"])
        .arg("-d")
        .arg(&class_dir)
        .arg(&generated)
        .arg(&runtime_check)
        .arg(runtime_package_dir.join("ConfigForgeJsonRuntimeDataLoader.java"))
        .arg(runtime_package_dir.join("ConfigForgeJsonRuntimeDataReader.java"))
        .arg(runtime_package_dir.join("ConfigForgeJsonParser.java"))
        .arg(runtime_package_dir.join("ConfigForgeRuntimeErrorKind.java"))
        .arg(runtime_package_dir.join("ConfigForgeRuntimeException.java"))
        .arg(msgpack_runtime_package_dir.join("ConfigForgeMessagePackRuntimeDataLoader.java"))
        .arg(msgpack_runtime_package_dir.join("ConfigForgeMessagePackRuntimeDataReader.java"))
        .arg(msgpack_runtime_package_dir.join("ConfigForgeMessagePackParser.java"))
        .arg(msgpack_runtime_package_dir.join("ConfigForgeRuntimeErrorKind.java"))
        .arg(msgpack_runtime_package_dir.join("ConfigForgeRuntimeException.java"))
        .arg(binary_runtime_package_dir.join("ConfigForgeBinaryRuntimeDataLoader.java"))
        .arg(binary_runtime_package_dir.join("ConfigForgeBinaryRuntimeDataReader.java"))
        .arg(binary_runtime_package_dir.join("ConfigForgeRuntimeErrorKind.java"))
        .arg(binary_runtime_package_dir.join("ConfigForgeRuntimeException.java"))
        .output()
        .expect("standard generated Java javac should run");
    assert_command_success(output, "standard generated Java javac");

    let output = StdCommand::new("java")
        .arg("-cp")
        .arg(&class_dir)
        .arg("RuntimeCheck")
        .env("DATA_DIR", output_root.join("client_data/json"))
        .env("MSGPACK_DATA_DIR", output_root.join("client_data/msgpack"))
        .env("BIN_DATA_DIR", output_root.join("client_data/binary"))
        .output()
        .expect("standard generated Java runtime test should run");
    assert_command_success(output, "standard generated Java runtime test");
}
