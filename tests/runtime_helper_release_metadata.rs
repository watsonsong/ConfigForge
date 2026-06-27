use std::fs;
use std::path::{Path, PathBuf};

const CONTRACT_VERSION: &str = "1.0";
const REQUIRED_CONTRACT: &str = "^1.0";
const PACKAGE_VERSION: &str = "0.1.0";
const DISTRIBUTION_MODE: &str = "copyable-template";

fn repo_path(relative: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read(relative: impl AsRef<Path>) -> String {
    let path = repo_path(relative.as_ref());
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {path:?}: {error}"))
}

fn assert_contains(relative: &str, expected: &str) {
    let content = read(relative);
    assert!(
        content.contains(expected),
        "{relative} should contain {expected:?}"
    );
}

#[test]
fn generated_contract_constants_stay_in_sync() {
    for relative in [
        "src/generator/rust/runtime.rs",
        "src/generator/cpp/runtime.rs",
        "src/generator/typescript/mod.rs",
        "src/generator/csharp/mod.rs",
        "src/generator/java/mod.rs",
    ] {
        assert_contains(relative, CONTRACT_VERSION);
        assert_contains(relative, REQUIRED_CONTRACT);
    }
}

#[test]
fn runtime_helper_packages_declare_supported_contract_range() {
    assert_contains(
        "runtime/rust/reader_core/src/lib.rs",
        &format!("CONFIG_FORGE_READER_CONTRACT_VERSION: &str = \"{CONTRACT_VERSION}\""),
    );
    assert_contains(
        "runtime/rust/reader_core/src/lib.rs",
        &format!("SUPPORTED_CONFIG_FORGE_READER_CONTRACT: &str = \"{REQUIRED_CONTRACT}\""),
    );

    for relative in [
        "runtime/cpp/core/include/config_forge/runtime/reader.hpp",
        "runtime/cpp/binary_reader/include/config_forge/runtime/binary_reader/runtime_loader.hpp",
        "runtime/cpp/rapidjson_reader/include/config_forge/runtime/rapidjson_reader/runtime_loader.hpp",
        "runtime/cpp/msgpackc_reader/include/config_forge/runtime/msgpackc_reader/runtime_loader.hpp",
        "runtime/csharp/json_reader/JsonRuntimeDataReader.cs",
        "runtime/csharp/messagepack_reader/MessagePackRuntimeDataReader.cs",
        "runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs",
        "runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataReader.java",
        "runtime/java/messagepack_reader/src/configforge/runtime/messagepack/ConfigForgeMessagePackRuntimeDataReader.java",
        "runtime/java/binary_reader/src/configforge/runtime/binary/ConfigForgeBinaryRuntimeDataReader.java",
    ] {
        assert_contains(relative, REQUIRED_CONTRACT);
    }

    assert_contains(
        "runtime/typescript/reader_core/src/runtime_reader.ts",
        &format!("SUPPORTED_CONFIG_FORGE_READER_CONTRACT = \"{REQUIRED_CONTRACT}\""),
    );
    for relative in [
        "runtime/typescript/json_reader/src/runtime_loader.ts",
        "runtime/typescript/messagepack_reader/src/runtime_loader.ts",
        "runtime/typescript/binary_reader/src/runtime_loader.ts",
        "runtime/typescript/loaders/node-fs/src/runtime_loader.ts",
    ] {
        assert_contains(relative, "SUPPORTED_CONFIG_FORGE_READER_CONTRACT");
    }
}

#[test]
fn package_and_template_versions_stay_in_sync() {
    for relative in [
        "runtime/rust/reader_core/Cargo.toml",
        "runtime/rust/serde_json_reader/Cargo.toml",
        "runtime/rust/serde_msgpack_reader/Cargo.toml",
        "runtime/rust/binary_reader/Cargo.toml",
    ] {
        assert_contains(relative, &format!("version = \"{PACKAGE_VERSION}\""));
    }

    for relative in [
        "runtime/typescript/reader_core/package.json",
        "runtime/typescript/json_reader/package.json",
        "runtime/typescript/messagepack_reader/package.json",
        "runtime/typescript/binary_reader/package.json",
        "runtime/typescript/loaders/node-fs/package.json",
    ] {
        assert_contains(relative, &format!("\"version\": \"{PACKAGE_VERSION}\""));
        assert_contains(
            relative,
            &format!("\"runtimeReaderContract\": \"{REQUIRED_CONTRACT}\""),
        );
    }

    assert_contains(
        "runtime/cpp/CMakeLists.txt",
        &format!("VERSION {PACKAGE_VERSION}"),
    );

    for relative in [
        "runtime/csharp/json_reader/JsonRuntimeDataReader.cs",
        "runtime/csharp/messagepack_reader/MessagePackRuntimeDataReader.cs",
        "runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs",
        "runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataReader.java",
        "runtime/java/messagepack_reader/src/configforge/runtime/messagepack/ConfigForgeMessagePackRuntimeDataReader.java",
        "runtime/java/binary_reader/src/configforge/runtime/binary/ConfigForgeBinaryRuntimeDataReader.java",
    ] {
        assert_contains(relative, PACKAGE_VERSION);
        assert_contains(relative, DISTRIBUTION_MODE);
        assert_contains(relative, REQUIRED_CONTRACT);
    }
}
