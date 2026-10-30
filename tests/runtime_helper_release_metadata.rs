use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const CONTRACT_VERSION: &str = "1.0";
const REQUIRED_CONTRACT: &str = "^1.0";
const PACKAGE_VERSION: &str = "0.1.0";
const DISTRIBUTION_MODE: &str = "copyable-template";
const REPOSITORY: &str = "https://github.com/ConfigForge/ConfigForge";

fn repo_path(relative: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read(relative: impl AsRef<Path>) -> String {
    let path = repo_path(relative.as_ref());
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {path:?}: {error}"))
        .replace("\r\n", "\n")
}

fn read_json(relative: &str) -> Value {
    serde_json::from_str(&read(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"))
}

fn read_toml(relative: &str) -> toml::Value {
    toml::from_str(&read(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"))
}

fn package<'a>(relative: &str, manifest: &'a toml::Value) -> &'a toml::value::Table {
    manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("{relative}: missing [package]"))
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
        assert_uses_shared_contract_metadata(relative);
    }
}

#[test]
fn runtime_helper_packages_declare_supported_contract_range() {
    assert_exact_declaration(
        "runtime/rust/reader_core/src/lib.rs",
        &format!("CONFIG_FORGE_READER_CONTRACT_VERSION: &str = \"{CONTRACT_VERSION}\""),
    );
    assert_exact_declaration(
        "runtime/rust/reader_core/src/lib.rs",
        &format!("SUPPORTED_CONFIG_FORGE_READER_CONTRACT: &str = \"{REQUIRED_CONTRACT}\""),
    );

    for relative in [
        "runtime/cpp/core/include/config_forge/runtime/reader.hpp",
        "runtime/cpp/binary_reader/include/config_forge/runtime/binary_reader/runtime_loader.hpp",
        "runtime/cpp/rapidjson_reader/include/config_forge/runtime/rapidjson_reader/runtime_loader.hpp",
        "runtime/cpp/msgpackc_reader/include/config_forge/runtime/msgpackc_reader/runtime_loader.hpp",
    ] {
        assert_exact_declaration(
            relative,
            &format!("SUPPORTED_CONFIG_FORGE_READER_CONTRACT = \"{REQUIRED_CONTRACT}\""),
        );
    }

    for relative in [
        "runtime/csharp/json_reader/JsonRuntimeDataReader.cs",
        "runtime/csharp/system_text_json_reader/SystemTextJsonRuntimeDataReader.cs",
        "runtime/csharp/messagepack_reader/MessagePackRuntimeDataReader.cs",
        "runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs",
    ] {
        assert_exact_declaration(
            relative,
            &format!("SupportedConfigForgeReaderContract = \"{REQUIRED_CONTRACT}\""),
        );
    }

    for relative in [
        "runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataReader.java",
        "runtime/java/messagepack_reader/src/configforge/runtime/messagepack/ConfigForgeMessagePackRuntimeDataReader.java",
        "runtime/java/binary_reader/src/configforge/runtime/binary/ConfigForgeBinaryRuntimeDataReader.java",
    ] {
        assert_exact_declaration(
            relative,
            &format!("SUPPORTED_CONFIG_FORGE_READER_CONTRACT = \"{REQUIRED_CONTRACT}\""),
        );
    }

    assert_exact_declaration(
        "runtime/typescript/reader_core/src/runtime_reader.ts",
        &format!("SUPPORTED_CONFIG_FORGE_READER_CONTRACT = \"{REQUIRED_CONTRACT}\""),
    );
    for relative in [
        "runtime/typescript/json_reader/src/runtime_loader.ts",
        "runtime/typescript/messagepack_reader/src/runtime_loader.ts",
        "runtime/typescript/binary_reader/src/runtime_loader.ts",
        "runtime/typescript/loaders/node-fs/src/runtime_loader.ts",
    ] {
        assert_exact_declaration(relative, "SUPPORTED_CONFIG_FORGE_READER_CONTRACT");
    }
}

#[test]
fn package_and_template_versions_stay_in_sync() {
    assert_rust_package(
        "runtime/rust/reader_core/Cargo.toml",
        "config-forge-runtime-reader-core",
    );
    assert_rust_package(
        "runtime/rust/serde_json_reader/Cargo.toml",
        "config-forge-runtime-serde-json-reader",
    );
    assert_rust_package(
        "runtime/rust/serde_msgpack_reader/Cargo.toml",
        "config-forge-runtime-serde-msgpack-reader",
    );
    assert_rust_package(
        "runtime/rust/binary_reader/Cargo.toml",
        "config-forge-runtime-binary-reader",
    );

    assert_typescript_package(
        "runtime/typescript/reader_core/package.json",
        "@config-forge/runtime-reader-core",
        "dist/runtime_reader.d.ts",
        "dist/runtime_reader.js",
    );
    assert_typescript_package(
        "runtime/typescript/json_reader/package.json",
        "@config-forge/runtime-reader-json",
        "dist/runtime_loader.d.ts",
        "dist/runtime_loader.js",
    );
    assert_typescript_package(
        "runtime/typescript/messagepack_reader/package.json",
        "@config-forge/runtime-reader-messagepack",
        "dist/runtime_loader.d.ts",
        "dist/runtime_loader.js",
    );
    assert_typescript_package(
        "runtime/typescript/binary_reader/package.json",
        "@config-forge/runtime-reader-binary",
        "dist/runtime_loader.d.ts",
        "dist/runtime_loader.js",
    );
    assert_typescript_package(
        "runtime/typescript/loaders/node-fs/package.json",
        "@config-forge/runtime-loader-node-fs",
        "dist/runtime_loader.d.ts",
        "dist/runtime_loader.js",
    );

    assert_cmake_package_version("runtime/cpp/CMakeLists.txt");

    for relative in [
        "runtime/csharp/json_reader/JsonRuntimeDataReader.cs",
        "runtime/csharp/messagepack_reader/MessagePackRuntimeDataReader.cs",
        "runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs",
    ] {
        assert_exact_declaration(
            relative,
            &format!("TemplateVersion = \"{PACKAGE_VERSION}\""),
        );
        assert_exact_declaration(
            relative,
            &format!("DistributionMode = \"{DISTRIBUTION_MODE}\""),
        );
        assert_exact_declaration(
            relative,
            &format!("SupportedConfigForgeReaderContract = \"{REQUIRED_CONTRACT}\""),
        );
    }

    for relative in [
        "runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataReader.java",
        "runtime/java/messagepack_reader/src/configforge/runtime/messagepack/ConfigForgeMessagePackRuntimeDataReader.java",
        "runtime/java/binary_reader/src/configforge/runtime/binary/ConfigForgeBinaryRuntimeDataReader.java",
    ] {
        assert_exact_declaration(
            relative,
            &format!("TEMPLATE_VERSION = \"{PACKAGE_VERSION}\""),
        );
        assert_exact_declaration(
            relative,
            &format!("DISTRIBUTION_MODE = \"{DISTRIBUTION_MODE}\""),
        );
        assert_exact_declaration(
            relative,
            &format!("SUPPORTED_CONFIG_FORGE_READER_CONTRACT = \"{REQUIRED_CONTRACT}\""),
        );
    }
}

#[test]
fn runtime_reader_parser_backend_metadata_stays_in_sync() {
    assert_rust_parser_backend(
        "runtime/rust/serde_json_reader/Cargo.toml",
        "serde_json",
        "1.0",
        "stable",
    );
    assert_rust_parser_backend(
        "runtime/rust/serde_msgpack_reader/Cargo.toml",
        "rmp-serde",
        "1.3",
        "stable",
    );
    assert_rust_parser_backend(
        "runtime/rust/binary_reader/Cargo.toml",
        "configforge-binary",
        "0.1",
        "stable",
    );

    assert_typescript_parser_backend(
        "runtime/typescript/json_reader/package.json",
        "JSON.parse",
        "ECMAScript",
        "stable",
    );
    assert_typescript_parser_backend(
        "runtime/typescript/messagepack_reader/package.json",
        "@msgpack/msgpack",
        "^3.0.0",
        "stable",
    );
    assert_typescript_parser_backend(
        "runtime/typescript/binary_reader/package.json",
        "configforge-binary",
        "0.1",
        "stable",
    );

    assert_csharp_parser_backend(
        "runtime/csharp/json_reader/JsonRuntimeDataReader.cs",
        "System.Text.Json",
        "net6.0",
        "stable",
    );
    assert_csharp_parser_backend(
        "runtime/csharp/system_text_json_reader/SystemTextJsonRuntimeDataReader.cs",
        "System.Text.Json",
        "net6.0",
        "stable",
    );
    assert_csharp_parser_backend(
        "runtime/csharp/messagepack_reader/MessagePackRuntimeDataReader.cs",
        "configforge-dependency-free-messagepack-subset",
        "0.1",
        "legacy-bootstrap",
    );
    assert_csharp_parser_backend(
        "runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs",
        "configforge-binary",
        "0.1",
        "stable",
    );

    assert_java_parser_backend(
        "runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataReader.java",
        "configforge-jdk-json-bootstrap",
        "0.1",
        "legacy-bootstrap",
    );
    assert_java_parser_backend(
        "runtime/java/messagepack_reader/src/configforge/runtime/messagepack/ConfigForgeMessagePackRuntimeDataReader.java",
        "configforge-jdk-messagepack-subset",
        "0.1",
        "legacy-bootstrap",
    );
    assert_java_parser_backend(
        "runtime/java/binary_reader/src/configforge/runtime/binary/ConfigForgeBinaryRuntimeDataReader.java",
        "configforge-binary",
        "0.1",
        "stable",
    );

    assert_exact_declaration("src/code_artifact/runtime_interface.rs", "parser_backend");
    assert_exact_declaration("src/code_artifact/runtime_interface.rs", "runtime_readers");
}

fn assert_rust_package(relative: &str, expected_name: &str) {
    let manifest = read_toml(relative);
    let package = package(relative, &manifest);
    assert_eq!(
        package["name"].as_str(),
        Some(expected_name),
        "{relative} name"
    );
    assert_eq!(
        package["version"].as_str(),
        Some(PACKAGE_VERSION),
        "{relative} version"
    );
    assert_eq!(
        package["license"].as_str(),
        Some("Apache-2.0"),
        "{relative} license"
    );
    assert_eq!(
        package["repository"].as_str(),
        Some(REPOSITORY),
        "{relative} repository"
    );
}

fn assert_typescript_package(
    relative: &str,
    expected_name: &str,
    expected_types: &str,
    expected_require: &str,
) {
    let package = read_json(relative);
    assert_eq!(package["name"], expected_name, "{relative} name");
    assert_eq!(package["version"], PACKAGE_VERSION, "{relative} version");
    assert_eq!(package["types"], expected_types, "{relative} types");
    assert_eq!(package["sideEffects"], false, "{relative} sideEffects");
    assert_eq!(package["license"], "Apache-2.0", "{relative} license");
    assert_eq!(
        package["repository"]["type"], "git",
        "{relative} repository type"
    );
    assert_eq!(
        package["repository"]["url"],
        format!("{REPOSITORY}.git"),
        "{relative} repository url"
    );
    assert_eq!(
        package["exports"]["."]["types"],
        format!("./{expected_types}"),
        "{relative} exports types"
    );
    assert_eq!(
        package["exports"]["."]["require"],
        format!("./{expected_require}"),
        "{relative} exports require"
    );
    assert_eq!(
        package["configForge"]["runtimeReaderContract"], REQUIRED_CONTRACT,
        "{relative} contract"
    );
}

fn assert_rust_parser_backend(
    relative: &str,
    parser_backend: &str,
    parser_backend_version: &str,
    support_level: &str,
) {
    let manifest = read_toml(relative);
    let metadata = manifest
        .get("package")
        .and_then(|value| value.get("metadata"))
        .and_then(|value| value.get("config-forge"))
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("{relative}: missing [package.metadata.config-forge]"));
    assert_eq!(metadata["parser-backend"].as_str(), Some(parser_backend));
    assert_eq!(
        metadata["parser-backend-version"].as_str(),
        Some(parser_backend_version)
    );
    assert_eq!(
        metadata["adapter-support-level"].as_str(),
        Some(support_level)
    );
}

fn assert_typescript_parser_backend(
    relative: &str,
    parser_backend: &str,
    parser_backend_version: &str,
    support_level: &str,
) {
    let package = read_json(relative);
    assert_eq!(package["configForge"]["parserBackend"], parser_backend);
    assert_eq!(
        package["configForge"]["parserBackendVersion"],
        parser_backend_version
    );
    assert_eq!(package["configForge"]["adapterSupportLevel"], support_level);
}

fn assert_csharp_parser_backend(
    relative: &str,
    parser_backend: &str,
    parser_backend_version: &str,
    support_level: &str,
) {
    assert_exact_declaration(relative, &format!("ParserBackend = \"{parser_backend}\""));
    assert_exact_declaration(
        relative,
        &format!("ParserBackendVersion = \"{parser_backend_version}\""),
    );
    assert_exact_declaration(
        relative,
        &format!("AdapterSupportLevel = \"{support_level}\""),
    );
}

fn assert_java_parser_backend(
    relative: &str,
    parser_backend: &str,
    parser_backend_version: &str,
    support_level: &str,
) {
    assert_exact_declaration(relative, &format!("PARSER_BACKEND = \"{parser_backend}\""));
    assert_exact_declaration(
        relative,
        &format!("PARSER_BACKEND_VERSION = \"{parser_backend_version}\""),
    );
    assert_exact_declaration(
        relative,
        &format!("ADAPTER_SUPPORT_LEVEL = \"{support_level}\""),
    );
}

fn assert_cmake_package_version(relative: &str) {
    let content = read(relative);
    let expected = format!(
        "write_basic_package_version_file(\n    ${{CMAKE_CURRENT_BINARY_DIR}}/ConfigForgeRuntimeConfigVersion.cmake\n    VERSION {PACKAGE_VERSION}\n    COMPATIBILITY SameMajorVersion)"
    );
    assert!(
        content.contains(&expected),
        "{relative} package version block drifted"
    );
}

fn assert_exact_declaration(relative: &str, expected: &str) {
    let content = read(relative);
    assert!(
        content.contains(expected),
        "{relative} should contain exact declaration {expected:?}"
    );
}

fn assert_uses_shared_contract_metadata(relative: &str) {
    let content = read(relative);
    assert!(
        content.contains("RUNTIME_READER_CONTRACT_VERSION"),
        "{relative} should use shared contract version metadata"
    );
    assert!(
        content.contains("REQUIRED_RUNTIME_READER_CONTRACT"),
        "{relative} should use shared required contract metadata"
    );
}
