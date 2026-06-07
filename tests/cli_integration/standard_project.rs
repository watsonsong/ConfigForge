// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::*;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn builds_reference_game_fixture_with_fragments_structs_and_composite_indexes() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let client_out_dir = temp_dir.path().join("out/client_data");
    let server_out_dir = temp_dir.path().join("out/server_data");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "reference_game_fixture"
                input = "{}"

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"

                [[targets]]
                name = "server"
                tags = ["server"]

                [[targets.exports]]
                output = "{}"
                type = "messagepack"
            "#,
            standard_reference_game_meta_imports(),
            absolute_fixture_path("data"),
            client_out_dir.display(),
            server_out_dir.display(),
        ),
    )
    .expect("project config should be written");

    let mut client_cmd = Command::cargo_bin("config-forge").expect("binary should build");
    client_cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client",
    ]);
    client_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("target `client` built"));

    let class_levels_json = fs::read_to_string(client_out_dir.join("class_levels.json"))
        .expect("class levels json should exist");
    let class_levels: serde_json::Value =
        serde_json::from_str(&class_levels_json).expect("class levels json should parse");
    assert_eq!(class_levels["rows"][0]["class_id"], "1");
    assert_eq!(class_levels["rows"][0]["level"], "1");
    assert_eq!(class_levels["rows"][0]["attributes"]["max_health"], "120");
    assert_eq!(class_levels["rows"][0]["attributes"]["attack"], "12");

    let items_json =
        fs::read_to_string(client_out_dir.join("items.json")).expect("items json should exist");
    let items: serde_json::Value =
        serde_json::from_str(&items_json).expect("items json should parse");
    let rows = items["rows"]
        .as_array()
        .expect("items rows should be array");
    assert_eq!(rows.len(), 9);
    assert_eq!(rows[0]["name"], "Iron Ore");
    assert_eq!(rows[0]["stack"]["max_stack"], "999");
    assert_eq!(rows[3]["name"], "Iron Sword");
    assert_eq!(rows[3]["attributes"]["attack"], "12");
    assert_eq!(rows[3]["skill"]["skill_asset"], "assets/skills/slash.asset");
    assert_eq!(rows[6]["name"], "Leather Armor");
    assert_eq!(rows[6]["attributes"]["defence"], "6");

    let mut server_cmd = Command::cargo_bin("config-forge").expect("binary should build");
    server_cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "server",
    ]);
    server_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("target `server` built"));

    assert!(!client_out_dir.join("materials.json").exists());
    assert!(!client_out_dir.join("weapons.json").exists());
    assert!(!client_out_dir.join("armors.json").exists());

    let server_items_bytes = fs::read(server_out_dir.join("items.msgpack"))
        .expect("server items messagepack should exist");
    let server_items: serde_json::Value =
        rmp_serde::from_slice(&server_items_bytes).expect("server items messagepack should parse");
    assert!(server_items["rows"][3].get("name").is_none());
    assert!(server_items["rows"][3].get("skill").is_none());
    assert_eq!(server_items["rows"][3]["attributes"]["attack"], "12");
}

#[test]
fn cli_extra_tag_enables_s1_material_override_source() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_out_dir = temp_dir.path().join("out/data");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                input = "{}"

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            data_out_dir.display(),
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client",
        "--tag",
        "s1",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client` built"));

    let items_json =
        fs::read_to_string(data_out_dir.join("items.json")).expect("items json should exist");
    let items: serde_json::Value =
        serde_json::from_str(&items_json).expect("items json should parse");
    let oak = items["rows"]
        .as_array()
        .expect("rows should be array")
        .iter()
        .find(|row| row["id"] == "2002")
        .expect("Oak Wood row should exist");
    assert_eq!(oak["name"], "Oak Wood");
    assert_eq!(oak["price"], "5");
    assert_eq!(oak["stack"]["max_stack"], "999");
}

#[test]
fn source_order_ordinals_follow_target_visible_rows() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let client_out_dir = temp_dir.path().join("out/client_data");
    let server_out_dir = temp_dir.path().join("out/server_data");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(
        data_dir.join("names.csv"),
        "id,value,required_tags,excluded_tags\n1,Always,,\n2,ClientOnly,client,\n3,ServerOnly,server,\n4,Hidden,qa,\n",
    )
    .expect("source-order data should be written");
    fs::write(
        &project_path,
        format!(
            r#"
                [project]
                input = "{}"

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"

                [[targets]]
                name = "server"
                tags = ["server"]

                [[targets.exports]]
                output = "{}"
                type = "json"

                [[tables]]
                name = "names"
                source = "names.csv"
                preserve_source_order = true
                row_export = {{ required_tags_field = "required_tags", excluded_tags_field = "excluded_tags" }}

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "value"
                type = "string"

                [[tables.fields]]
                name = "required_tags"
                type = "string"
                export = false

                [[tables.fields]]
                name = "excluded_tags"
                type = "string"
                export = false
            "#,
            data_dir.display(),
            client_out_dir.display(),
            server_out_dir.display(),
        ),
    )
    .expect("project config should be written");

    let mut client_cmd = Command::cargo_bin("config-forge").expect("binary should build");
    client_cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client",
    ]);
    client_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("target `client` built"));

    let mut server_cmd = Command::cargo_bin("config-forge").expect("binary should build");
    server_cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "server",
    ]);
    server_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("target `server` built"));

    let client_json =
        fs::read_to_string(client_out_dir.join("names.json")).expect("client names json");
    let client_names: serde_json::Value =
        serde_json::from_str(&client_json).expect("client names json should parse");
    assert_eq!(
        client_names["rows"].as_array().expect("client rows").len(),
        2
    );
    assert_eq!(client_names["rows"][0]["value"], "Always");
    assert_eq!(client_names["rows"][1]["value"], "ClientOnly");

    let server_json =
        fs::read_to_string(server_out_dir.join("names.json")).expect("server names json");
    let server_names: serde_json::Value =
        serde_json::from_str(&server_json).expect("server names json should parse");
    assert_eq!(
        server_names["rows"].as_array().expect("server rows").len(),
        2
    );
    assert_eq!(server_names["rows"][0]["value"], "Always");
    assert_eq!(server_names["rows"][1]["value"], "ServerOnly");
}

#[test]
fn standard_project_generated_code_loads_exported_runtime_data() {
    let project_path = std::env::current_dir()
        .expect("current dir")
        .join("tests/fixtures/standard_project/ConfigForge.toml");
    let fixture_root = project_path.parent().expect("standard project root");
    let fixture_out_dir = fixture_root.join("out");
    if fixture_out_dir.exists() {
        fs::remove_dir_all(&fixture_out_dir)
            .expect("stale standard project out dir should be removed");
    }

    let mut client_cmd = Command::cargo_bin("config-forge").expect("binary should build");
    client_cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client",
    ]);
    client_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("target `client` built"));

    let mut server_cmd = Command::cargo_bin("config-forge").expect("binary should build");
    server_cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "server",
    ]);
    server_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("target `server` built"));

    assert!(
        fixture_root
            .join("out/client_data/json/items.json")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_data/binary/items.bin")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_data/msgpack/items.msgpack")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_data/json/character_surnames.json")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_data/json/character_male_given_names.json")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_data/json/character_female_given_names.json")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_i18n/reference_game_config.pot")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_lua/reference_game_config/init.lua")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_lua/reference_game_config/items.lua")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/server_data/msgpack/items.msgpack")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/server_data/binary/items.bin")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/rust/reference_game_config.rs")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/rust_external/reference_game_config.rs")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/typescript/reference_game_config.ts")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/cpp/reference_game_config.hpp")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/cpp/reference_game_config.cpp")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/cpp_external/reference_game_config.hpp")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/cpp_unreal/reference_game_config.h")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/cpp_unreal/reference_game_config.cpp")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/csharp/reference_game_config.cs")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_code/java/ReferenceGameConfig.java")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/server_code/rust/reference_game_config.rs")
            .exists()
    );
    let pot = fs::read_to_string(fixture_root.join("out/client_i18n/reference_game_config.pot"))
        .expect("standard fixture POT should be readable");
    assert!(pot.contains("msgctxt \"items.name\""));
    assert!(pot.contains("msgid \"Iron Ore\""));
    assert!(pot.contains("msgctxt \"classes.description\""));

    let rust_code =
        fs::read_to_string(fixture_root.join("out/client_code/rust/reference_game_config.rs"))
            .expect("standard fixture Rust code should be readable");
    assert!(rust_code.contains("pub fn rows(&self) -> &[CharacterSurnamesRow]"));
    assert!(rust_code.contains("pub fn rows(&self) -> &[CharacterMaleGivenNamesRow]"));
    assert!(rust_code.contains("pub fn rows(&self) -> &[CharacterFemaleGivenNamesRow]"));
    assert!(!rust_code.contains("pub fn rows(&self) -> &[ItemsRow]"));
    assert!(!rust_code.contains("pub use config_forge_runtime_reader_core"));

    let external_rust_code = fs::read_to_string(
        fixture_root.join("out/client_code/rust_external/reference_game_config.rs"),
    )
    .expect("standard fixture external Rust code should be readable");
    assert!(external_rust_code.contains("pub use config_forge_runtime_reader_core::{"));
    assert!(!external_rust_code.contains("pub trait RuntimeDataReader"));

    let cpp_external_code = fs::read_to_string(
        fixture_root.join("out/client_code/cpp_external/reference_game_config.hpp"),
    )
    .expect("standard fixture external C++ header should be readable");
    assert!(cpp_external_code.contains("#include \"config_forge/runtime/reader.hpp\""));
    assert!(cpp_external_code.contains("using config_forge_runtime::IRuntimeDataReader;"));
    assert!(!cpp_external_code.contains("class IRuntimeDataReader"));

    let java_code =
        fs::read_to_string(fixture_root.join("out/client_code/java/ReferenceGameConfig.java"))
            .expect("standard fixture Java code should be readable");
    assert!(java_code.contains("package referencegame.config;"));
    assert!(java_code.contains("public final class ReferenceGameConfig"));
    assert!(java_code.contains("public static final class LazyGameData implements GameDataAccess"));
    assert!(java_code.contains(
        "ConfigForgeJsonRuntimeDataLoaderAdapter implements ConfigForgeRuntimeDataLoader"
    ));

    let csharp_code =
        fs::read_to_string(fixture_root.join("out/client_code/csharp/reference_game_config.cs"))
            .expect("standard fixture C# code should be readable");
    assert!(
        csharp_code
            .contains("ConfigForgeJsonRuntimeDataLoaderAdapter : IConfigForgeRuntimeDataLoader")
    );
    assert!(csharp_code.contains(
        "ConfigForgeMessagePackRuntimeDataReaderAdapter : IConfigForgeRuntimeDataReader"
    ));

    verify_standard_generated_rust_runtime(fixture_root);
    verify_standard_generated_cpp_runtime(fixture_root);
    verify_standard_generated_cpp_unreal_runtime(fixture_root);
    verify_standard_generated_csharp_runtime(fixture_root);
    verify_standard_generated_java_runtime(fixture_root);
    verify_standard_generated_typescript_runtime(fixture_root);
    verify_standard_generated_lua_runtime(fixture_root);
}
