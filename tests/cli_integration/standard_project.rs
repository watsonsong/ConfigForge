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
    let schema_doc_out_dir = temp_dir.path().join("out/schema_docs");
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

                [[targets]]
                name = "schema_docs"
                tags = ["client", "server"]

                [[targets.exports]]
                name = "meta_schema_doc"
                output = "{}"
                type = "schema_doc"
                format = "markdown"
                title = "Reference Game Meta Schema Documentation"
            "#,
            standard_reference_game_meta_imports(),
            absolute_fixture_path("data"),
            client_out_dir.display(),
            server_out_dir.display(),
            schema_doc_out_dir.display(),
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
    assert_eq!(
        rows[3]["skill"]["skill_asset"],
        "/Game/Skills/GA_Slash.GA_Slash_C"
    );
    assert_eq!(rows[6]["name"], "Leather Armor");
    assert_eq!(rows[6]["attributes"]["defence"], "6");

    let quests_json =
        fs::read_to_string(client_out_dir.join("quests.json")).expect("quests json should exist");
    let quests: serde_json::Value =
        serde_json::from_str(&quests_json).expect("quests json should parse");
    let quest_rows = quests["rows"]
        .as_array()
        .expect("quest rows should be array");
    assert_eq!(quest_rows.len(), 3);
    assert_eq!(quest_rows[0]["title"], "First Steps");
    assert_eq!(quest_rows[0]["reward"]["xp"], "100");
    assert_eq!(
        quest_rows[1]["reward"]["item_ids"],
        serde_json::json!(["2002", "2003"])
    );
    assert_eq!(
        quest_rows[1]["prerequisite_quest_ids"],
        serde_json::json!(["5001"])
    );
    assert!(
        quest_rows
            .iter()
            .all(|row| row.get("required_tags").is_none())
    );

    let loot_pools_json = fs::read_to_string(client_out_dir.join("loot_pools.json"))
        .expect("loot pools json should exist");
    let loot_pools: serde_json::Value =
        serde_json::from_str(&loot_pools_json).expect("loot pools json should parse");
    assert_eq!(loot_pools["rows"].as_array().expect("loot rows").len(), 5);

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

    let mut schema_docs_cmd = Command::cargo_bin("config-forge").expect("binary should build");
    schema_docs_cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "schema_docs",
    ]);
    schema_docs_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("target `schema_docs` built"));
    assert!(schema_doc_out_dir.join("index.md").exists());
    assert!(schema_doc_out_dir.join("game_rules.md").exists());

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
        "id,value,required_tags,excluded_tags\nID,Value,Required Tags,Excluded Tags\n1,Always,,\n2,ClientOnly,client,\n3,ServerOnly,server,\n4,Hidden,qa,\n",
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

    let mut schema_docs_cmd = Command::cargo_bin("config-forge").expect("binary should build");
    schema_docs_cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "schema_docs",
    ]);
    schema_docs_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("target `schema_docs` built"));

    assert!(
        fixture_root
            .join("out/client_data/json/items.json")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_data/json/rarity_levels.json")
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
            .join("out/client_data/json/quests.json")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_data/json/loot_pools.json")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_i18n/reference_game_config.pot")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_assets/client_assets.json")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_assets/client_assets.csv")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_assets/client_assets_simple.csv")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/client_reports/validation.html")
            .exists()
    );
    assert!(fixture_root.join("out/client_docs/index.md").exists());
    assert!(fixture_root.join("out/client_docs/quests.md").exists());
    assert!(fixture_root.join("out/server_docs/index.md").exists());
    assert!(fixture_root.join("out/schema_docs/index.md").exists());
    assert!(
        fixture_root
            .join("out/schema_docs_html/index.html")
            .exists()
    );
    assert!(
        fixture_root
            .join("out/schema_docs_html/rarity_levels.html")
            .exists()
    );
    assert!(fixture_root.join("out/schema_docs/game_rules.md").exists());
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
    assert!(pot.contains("msgctxt \"rarity_levels.name\""));
    assert!(pot.contains("msgid \"Common\""));
    assert!(pot.contains("msgctxt \"classes.description\""));
    assert!(pot.contains("msgctxt \"quests.title\""));
    assert!(pot.contains("msgid \"First Steps\""));

    let validation_report =
        fs::read_to_string(fixture_root.join("out/client_reports/validation.html"))
            .expect("standard fixture validation report should be readable");
    assert!(validation_report.contains("PASS"));
    assert!(validation_report.contains("<h2>0</h2><p>Errors</p>"));

    let schema_index = fs::read_to_string(fixture_root.join("out/client_docs/index.md"))
        .expect("standard fixture schema index should be readable");
    assert!(schema_index.contains("Client Schema Documentation"));
    assert!(schema_index.contains("[rarity_levels](rarity_levels.md)"));
    assert!(schema_index.contains("[quests](quests.md)"));
    assert!(schema_index.contains("[loot_pools](loot_pools.md)"));

    let quests_doc = fs::read_to_string(fixture_root.join("out/client_docs/quests.md"))
        .expect("standard fixture quests doc should be readable");
    assert!(quests_doc.contains("FixedReward"));
    assert!(quests_doc.contains("required_class_id | classes.class_id"));
    assert!(quests_doc.contains("item_ids"));
    assert!(quests_doc.contains("monotonic"));

    let game_rules_doc = fs::read_to_string(fixture_root.join("out/client_docs/game_rules.md"))
        .expect("standard fixture game_rules doc should be readable");
    assert!(game_rules_doc.contains("starter_reward"));
    assert!(game_rules_doc.contains("FixedReward"));

    let rarity_doc = fs::read_to_string(fixture_root.join("out/client_docs/rarity_levels.md"))
        .expect("standard fixture rarity_levels doc should be readable");
    assert!(rarity_doc.contains("道具稀有度品质表"));
    assert!(rarity_doc.contains("standard_color"));
    assert!(rarity_doc.contains("background_icon"));

    let server_schema_index = fs::read_to_string(fixture_root.join("out/server_docs/index.md"))
        .expect("standard fixture server schema index should be readable");
    assert!(server_schema_index.contains("Server Schema Documentation"));

    let meta_schema_index = fs::read_to_string(fixture_root.join("out/schema_docs/index.md"))
        .expect("standard fixture meta schema index should be readable");
    assert!(meta_schema_index.contains("Reference Game Meta Schema Documentation"));
    assert!(meta_schema_index.contains("[rarity_levels](rarity_levels.md)"));
    assert!(meta_schema_index.contains("[game_rules](game_rules.md)"));
    assert!(meta_schema_index.contains("[items](items.md)"));
    assert!(
        meta_schema_index
            .contains("| materials.csv | csv | items | base | stack | collection records | all |")
    );
    assert!(meta_schema_index.contains("| game_rules | starter_item_ids | items | by_id | list-to-many | no | starter_items | all |"));

    let meta_schema_items = fs::read_to_string(fixture_root.join("out/schema_docs/items.md"))
        .expect("standard fixture meta schema items doc should be readable");
    assert!(meta_schema_items.contains("## Fragments"));
    assert!(
        meta_schema_items
            .contains("| stack | inline fields | max_stack | materials.csv (base) | all |")
    );
    assert!(
        meta_schema_items.contains(
            "| rarity_id | rarity_levels.rarity_id | many-to-one | no | rarity | all | - |"
        )
    );

    let meta_schema_game_rules =
        fs::read_to_string(fixture_root.join("out/schema_docs/game_rules.md"))
            .expect("standard fixture meta schema game_rules doc should be readable");
    assert!(!meta_schema_game_rules.contains("## Fragments"));
    assert!(!meta_schema_game_rules.contains("No fragments."));
    assert!(
        meta_schema_game_rules.contains(
            "| starter_item_ids | items.id | list-to-many | no | starter_items | all | - |"
        )
    );

    let meta_schema_html_index =
        fs::read_to_string(fixture_root.join("out/schema_docs_html/index.html"))
            .expect("standard fixture HTML schema index should be readable");
    assert!(meta_schema_html_index.contains("Reference Game Meta Schema Documentation"));
    assert!(meta_schema_html_index.contains("rarity_levels.html"));
    assert!(meta_schema_html_index.contains("<section id=\"relationship-graph\">"));
    assert!(meta_schema_html_index.contains("<a href=\"items.html\"><code>items</code></a>"));
    assert!(meta_schema_html_index.contains("<code>starter_item_ids</code>"));
    assert!(meta_schema_html_index.contains("list-to-many"));
    assert!(meta_schema_html_index.contains("fragments: stack"));

    let meta_schema_html_items =
        fs::read_to_string(fixture_root.join("out/schema_docs_html/items.html"))
            .expect("standard fixture HTML schema items doc should be readable");
    assert!(meta_schema_html_items.contains("<h2>Fragment composition</h2>"));
    assert!(meta_schema_html_items.contains("<code>stack</code>"));
    assert!(meta_schema_html_items.contains("not independent table"));

    let meta_schema_html_game_rules =
        fs::read_to_string(fixture_root.join("out/schema_docs_html/game_rules.html"))
            .expect("standard fixture HTML schema game_rules doc should be readable");
    assert!(!meta_schema_html_game_rules.contains("<section id=\"fragments\">"));
    assert!(!meta_schema_html_game_rules.contains("No fragments."));

    let asset_manifest_json =
        fs::read_to_string(fixture_root.join("out/client_assets/client_assets.json"))
            .expect("standard fixture asset manifest JSON should be readable");
    let asset_manifest: serde_json::Value = serde_json::from_str(&asset_manifest_json)
        .expect("standard fixture asset manifest JSON should parse");
    assert_eq!(asset_manifest["version"], 1);
    assert!(asset_manifest["assets"].as_array().is_some_and(|assets| {
        assets
            .iter()
            .any(|asset| asset["path"] == "/Game/Skills/GA_Slash.GA_Slash_C")
    }));
    assert!(asset_manifest["assets"].as_array().is_some_and(|assets| {
        assets
            .iter()
            .any(|asset| asset["path"] == "assets/icons/rarity/common.png")
    }));

    let asset_manifest_csv =
        fs::read_to_string(fixture_root.join("out/client_assets/client_assets.csv"))
            .expect("standard fixture asset manifest CSV should be readable");
    assert!(
        asset_manifest_csv
            .starts_with("Name,AssetRef,AssetKind,ReferencedBy,Tables,Fields,RecordKeys,Sources\n")
    );
    assert!(asset_manifest_csv.contains("/Game/Skills/GA_Slash.GA_Slash_C"));
    assert!(asset_manifest_csv.contains("assets/icons/rarity/common.png"));

    let simple_asset_manifest_csv =
        fs::read_to_string(fixture_root.join("out/client_assets/client_assets_simple.csv"))
            .expect("standard fixture simple asset manifest CSV should be readable");
    assert!(simple_asset_manifest_csv.starts_with("AssetRef\n"));
    assert!(simple_asset_manifest_csv.contains("/Game/Skills/GA_Slash.GA_Slash_C"));
    assert!(simple_asset_manifest_csv.contains("assets/icons/rarity/common.png"));
    assert!(!simple_asset_manifest_csv.contains("ReferencedBy"));
    assert!(!simple_asset_manifest_csv.contains("items["));

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

    verify_standard_generated_runtime_harnesses(fixture_root);
}
