// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::*;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn builds_target_from_imported_project_config_fragments() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let config_dir = temp_dir.path().join("Config");
    let meta_dir = config_dir.join("Meta");
    let targets_dir = config_dir.join("Targets");
    fs::create_dir_all(&meta_dir).expect("meta dir should be created");
    fs::create_dir_all(&targets_dir).expect("config dirs should be created");

    let out_dir = config_dir.join("generated/rust");
    fs::write(
        config_dir.join("ConfigForge.toml"),
        r#"
            imports = ["Meta/common.toml", "Meta/items.toml", "Targets/client.toml"]

            [project]
            name = "imported_project"
        "#,
    )
    .expect("root project config should be written");
    fs::write(
        meta_dir.join("common.toml"),
        fs::read_to_string(absolute_fixture_path("meta/common.toml"))
            .expect("fixture common meta should exist"),
    )
    .expect("common meta config fragment should be written");
    fs::write(
        meta_dir.join("items.toml"),
        fs::read_to_string(absolute_fixture_path("meta/items.toml"))
            .expect("fixture item meta should exist"),
    )
    .expect("meta config fragment should be written");
    fs::write(
        targets_dir.join("client.toml"),
        format!(
            r#"
                [[targets]]
                name = "client_imported"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            out_dir.display()
        ),
    )
    .expect("target config fragment should be written");

    let imported_project_path = config_dir.join("ConfigForge.toml");
    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
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

#[test]
fn target_tags_filter_validation_export_and_codegen() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    let rust_out_dir = temp_dir.path().join("out/code/rust");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(
        data_dir.join("items.csv"),
        "id,name,server_value,exclude_tags\nID,Name,Server Value,Exclude Tags\n1,Potion,,\n1,Debug Potion,,client\n",
    )
    .expect("items data should be written");

    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                [project]
                name = "target_tags_project"
                input = "{}"

                [[tables]]
                name = "items"
                source = "items.csv"
                primary_index = "by_id"
                row_export = {{ excluded_tags_field = "exclude_tags" }}

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "name"
                type = "string"
                required = true

                [[tables.fields]]
                name = "server_value"
                type = "int"
                required = true
                export_when = {{ any = ["server"] }}

                [[tables.fields]]
                name = "exclude_tags"
                type = "string"
                export = false

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true
                primary = true

                [[tables]]
                name = "server_only"
                source = "server_only.csv"
                export_when = {{ any = ["server"] }}

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            data_dir.display(),
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
        "client",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client` built"));

    assert!(data_out_dir.join("items.json").exists());
    assert!(!data_out_dir.join("server_only.json").exists());
    let payload: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(data_out_dir.join("items.json")).expect("items json should exist"),
    )
    .expect("items json should parse");
    let rows = payload["rows"].as_array().expect("rows should be array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "1");
    assert!(rows[0].get("server_value").is_none());
    assert!(rows[0].get("exclude_tags").is_none());

    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub struct ItemsRow"));
    assert!(rust_code.contains("pub id: i64"));
    assert!(rust_code.contains("pub name: String"));
    assert!(!rust_code.contains("server_value"));
    assert!(!rust_code.contains("exclude_tags"));
    assert!(!rust_code.contains("ServerOnly"));
}
