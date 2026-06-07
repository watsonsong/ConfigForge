// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::*;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn codegen_task_can_override_generated_file_name() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let rust_out_dir = temp_dir.path().join("out/rust");
    let ts_out_dir = temp_dir.path().join("out/typescript");
    let cpp_out_dir = temp_dir.path().join("out/cpp");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [[targets]]
                name = "custom_file_names"

                [[targets.exports]]
                type = "rust"
                output = "{}"
                file_name = "game_config"

                [[targets.exports]]
                type = "typescript"
                output = "{}"
                file_name = "game_config.ts"

                [[targets.exports]]
                type = "cpp"
                output = "{}"
                file_name = "game_config.hpp"
            "#,
            standard_core_meta_imports(),
            rust_out_dir.display(),
            ts_out_dir.display(),
            cpp_out_dir.display(),
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "custom_file_names",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `custom_file_names` built"));

    assert!(rust_out_dir.join("game_config.rs").exists());
    assert!(!rust_out_dir.join("generated.rs").exists());
    assert!(ts_out_dir.join("game_config.ts").exists());
    assert!(!ts_out_dir.join("generated.ts").exists());
    assert!(cpp_out_dir.join("game_config.hpp").exists());
    assert!(cpp_out_dir.join("game_config.cpp").exists());
    assert!(!cpp_out_dir.join("generated.hpp").exists());
    let cpp_source = fs::read_to_string(cpp_out_dir.join("game_config.cpp")).expect("cpp source");
    assert!(cpp_source.contains("#include \"game_config.hpp\""));
}

#[test]
fn builds_messagepack_target_and_codegen_uses_messagepack_files() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_out_dir = temp_dir.path().join("out/data");
    let rust_out_dir = temp_dir.path().join("out/code/rust");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "standard_project"
                input = "{}"

                [[targets]]
                name = "client_messagepack"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "messagepack"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            rust_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client_messagepack",
    ]);

    cmd.assert().success().stdout(predicate::str::contains(
        "target `client_messagepack` built",
    ));

    let messagepack_path = data_out_dir.join("items.msgpack");
    assert!(messagepack_path.exists());
    let messagepack_bytes = fs::read(messagepack_path).expect("messagepack data");
    assert!(!messagepack_bytes.is_empty());
    let payload: serde_json::Value =
        rmp_serde::from_slice(&messagepack_bytes).expect("messagepack should decode");
    assert_eq!(payload["meta"]["name"], "items");
    assert!(payload["meta"].get("fields").is_none());
    assert!(
        !payload["rows"]
            .as_array()
            .expect("rows should be array")
            .is_empty()
    );

    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"items\""));
    assert!(rust_code.contains("pub trait RuntimeDataReader"));
    assert!(!rust_code.contains("pub use config_forge_runtime_reader_core"));
    assert!(!rust_code.contains("RuntimeDataFormat"));

    verify_generated_rust_crate(&rust_out_dir, false);
}

#[test]
fn codegen_is_format_agnostic_with_multiple_runtime_exports() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let json_data_dir = temp_dir.path().join("out/data/json");
    let messagepack_data_dir = temp_dir.path().join("out/data/messagepack");
    let ts_out_dir = temp_dir.path().join("out/code/typescript");
    let rust_out_dir = temp_dir.path().join("out/code/rust");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                input = "{}"

                [[targets]]
                name = "multi_artifact"
                tags = ["client"]

                [[targets.exports]]
                name = "json"
                output = "{}"
                type = "json"

                [[targets.exports]]
                name = "messagepack"
                output = "{}"
                type = "messagepack"

                [[targets.exports]]
                type = "typescript"
                output = "{}"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            json_data_dir.display(),
            messagepack_data_dir.display(),
            ts_out_dir.display(),
            rust_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "multi_artifact",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `multi_artifact` built"));

    assert!(json_data_dir.join("items.json").exists());
    assert!(messagepack_data_dir.join("items.msgpack").exists());
    let ts_code = fs::read_to_string(ts_out_dir.join("generated.ts")).expect("ts code");
    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    assert!(ts_code.contains("static readonly dataFile = \"items\""));
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"items\""));
}
