use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn builds_code_only_target_without_loading_data() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = ["{}"]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            absolute_fixture_path("meta.toml"),
            out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    let rust_code = fs::read_to_string(out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub struct ItemsRow"));
}

#[test]
fn builds_target_from_project_config() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_out_dir = temp_dir.path().join("out/data");
    let rust_out_dir = temp_dir.path().join("out/code/rust");
    let cpp_out_dir = temp_dir.path().join("out/code/cpp");
    let ts_out_dir = temp_dir.path().join("out/code/typescript");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = ["{}"]

                [project]
                name = "standard_project"
                desc = "Standard fixture"
                copyright = "Copyright Fixture"

                [[targets]]
                name = "client"

                [targets.data]
                input = "{}"
                output = "{}"
                format = "json"

                [[targets.code]]
                language = "rust"
                output = "{}"

                [[targets.code]]
                language = "cpp"
                output = "{}"

                [[targets.code]]
                language = "typescript"
                output = "{}"
            "#,
            absolute_fixture_path("meta.toml"),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            rust_out_dir.display(),
            cpp_out_dir.display(),
            ts_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client` built"));

    assert!(data_out_dir.join("items.json").exists());
    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    let cpp_code = fs::read_to_string(cpp_out_dir.join("generated.hpp")).expect("cpp code");
    let ts_code = fs::read_to_string(ts_out_dir.join("generated.ts")).expect("ts code");
    assert!(rust_code.contains("pub struct ItemsRow"));
    assert!(rust_code.contains("pub trait RuntimeDataLoader"));
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"items.json\""));
    assert!(!rust_code.contains("RuntimeDataFormat"));
    assert!(rust_code.contains("pub fn get_by_id"));
    assert!(cpp_code.contains("class IRuntimeDataLoader"));
    assert!(cpp_code.contains("static constexpr const char* DataFile = \"items.json\""));
    assert!(!cpp_code.contains("RuntimeDataFormat"));
    assert!(cpp_code.contains("const ItemsRow* GetById"));
    assert!(ts_code.contains("export interface RuntimeDataLoader"));
    assert!(ts_code.contains("static readonly dataFile = \"items.json\""));
    assert!(!ts_code.contains("RuntimeDataFormat"));
    assert!(ts_code.contains("getById(id: number): ItemsRow | undefined"));
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
                imports = ["{}"]

                [project]
                name = "standard_project"

                [[targets]]
                name = "client_messagepack"

                [targets.data]
                input = "{}"
                output = "{}"
                format = "messagepack"

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            absolute_fixture_path("meta.toml"),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            rust_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
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
    assert!(!payload["rows"]
        .as_array()
        .expect("rows should be array")
        .is_empty());

    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"items.msgpack\""));
    assert!(rust_code.contains("pub trait RuntimeDataLoader"));
    assert!(!rust_code.contains("RuntimeDataFormat"));
}

#[test]
fn builds_target_from_imported_project_config_fragments() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let config_dir = temp_dir.path().join("Config");
    let meta_dir = config_dir.join("Meta");
    let targets_dir = config_dir.join("Targets");
    fs::create_dir_all(&meta_dir).expect("meta dir should be created");
    fs::create_dir_all(&targets_dir).expect("config dirs should be created");

    let out_dir = temp_dir.path().join("generated/rust");
    fs::write(
        config_dir.join("ConfigForge.toml"),
        r#"
            imports = ["Meta/items.toml", "Targets/client.toml"]

            [project]
            name = "imported_project"
        "#,
    )
    .expect("root project config should be written");
    fs::write(
        meta_dir.join("items.toml"),
        fs::read_to_string(absolute_fixture_path("meta.toml")).expect("fixture meta should exist"),
    )
    .expect("meta config fragment should be written");
    fs::write(
        targets_dir.join("client.toml"),
        format!(
            r#"
                [[targets]]
                name = "client_imported"

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            out_dir.display()
        ),
    )
    .expect("target config fragment should be written");

    let imported_project_path = config_dir.join("ConfigForge.toml");
    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        imported_project_path.to_str().expect("path should be utf8"),
        "--target",
        "client_imported",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client_imported` built"));

    let rust_code = fs::read_to_string(out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub struct ItemsRow"));
}

fn absolute_fixture_path(relative_path: &str) -> String {
    std::env::current_dir()
        .expect("current dir")
        .join("tests/fixtures/standard_project")
        .join(relative_path)
        .display()
        .to_string()
}
