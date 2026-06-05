// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::*;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

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
                imports = [{}]

                [project]
                name = "standard_project"
                input = "{}"
                desc = "Standard fixture"
                copyright = "Copyright Fixture"

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"

                [[targets.exports]]
                type = "rust"
                output = "{}"

                [[targets.exports]]
                type = "cpp"
                output = "{}"

                [[targets.exports]]
                type = "typescript"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            rust_out_dir.display(),
            cpp_out_dir.display(),
            ts_out_dir.display()
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
    assert!(data_out_dir.join("game_rules.json").exists());
    let game_rules_json = fs::read_to_string(data_out_dir.join("game_rules.json"))
        .expect("game rules json should exist");
    let game_rules: serde_json::Value =
        serde_json::from_str(&game_rules_json).expect("game rules json should parse");
    assert_eq!(game_rules["max_player_level"], "60");
    assert_eq!(game_rules["starter_gold"], "500");
    assert_eq!(game_rules["default_class_id"], "1");
    assert_eq!(
        game_rules["starter_item_ids"],
        serde_json::json!(["2001", "2002"])
    );
    assert_eq!(game_rules["stamina_cap"], "120");
    assert_eq!(game_rules["daily_reset_hour"], "5");
    assert_eq!(game_rules["pvp_enabled"], "true");
    assert!(game_rules.get("rows").is_none());
    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    let cpp_code = fs::read_to_string(cpp_out_dir.join("generated.hpp")).expect("cpp code");
    let cpp_source = fs::read_to_string(cpp_out_dir.join("generated.cpp")).expect("cpp source");
    let ts_code = fs::read_to_string(ts_out_dir.join("generated.ts")).expect("ts code");
    assert!(rust_code.contains("pub struct ItemsRow"));
    assert!(rust_code.contains("pub struct GameRulesRow"));
    assert!(rust_code.contains("pub use config_forge_runtime_reader_core"));
    assert!(!rust_code.contains("pub struct GameRulesTable"));
    assert!(rust_code.contains("RuntimeDataLoader"));
    assert!(rust_code.contains("pub trait TextTranslator"));
    assert!(rust_code.contains("pub fn name_text"));
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"items\""));
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"game_rules\""));
    assert!(!rust_code.contains("RuntimeDataFormat"));
    assert!(rust_code.contains("pub fn get_by_id"));
    assert!(rust_code.contains("Self::read_from_reader(&root)"));
    assert!(cpp_code.contains("class IRuntimeDataLoader"));
    assert!(cpp_code.contains("class IRuntimeDataReader"));
    assert!(cpp_code.contains("using TextTranslator"));
    assert!(cpp_code.contains("std::string NameText"));
    assert!(cpp_code.contains("static constexpr const char* DataFile = \"items\""));
    assert!(
        cpp_code
            .contains("virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path)")
    );
    assert!(cpp_code.contains("static constexpr const char* DataFile = \"game_rules\""));
    assert!(!cpp_code.contains("class GameRulesTable"));
    assert!(!cpp_code.contains("RuntimeDataFormat"));
    assert!(cpp_code.contains("const ItemsRow* GetById"));
    assert!(cpp_source.contains("GameData GameData::Load"));
    assert!(cpp_source.contains("ItemsTable ItemsTable::FromRows"));
    assert!(cpp_source.contains("loader.Open(GameRulesRow::DataFile)"));
    assert!(ts_code.contains("export interface RuntimeDataLoader"));
    assert!(ts_code.contains("export interface RuntimeDataReader"));
    assert!(ts_code.contains("export type TextTranslator"));
    assert!(ts_code.contains("export function itemsNameText"));
    assert!(ts_code.contains("static readonly dataFile = \"items\""));
    assert!(ts_code.contains("open(path: string): Promise<RuntimeDataReader> | RuntimeDataReader"));
    assert!(!ts_code.contains("export class GameRulesTable"));
    assert!(!ts_code.contains("RuntimeDataFormat"));
    assert!(ts_code.contains("getById(id: number): ItemsRow | undefined"));

    verify_generated_rust_crate(&rust_out_dir, false);
    verify_generated_cpp_runtime(&cpp_out_dir);
    verify_generated_typescript(&ts_out_dir);
}

#[test]
fn gettext_pot_export_and_generated_text_translation_helpers_work() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let pot_out_dir = temp_dir.path().join("out/i18n");
    let rust_out_dir = temp_dir.path().join("out/code/rust");
    let cpp_out_dir = temp_dir.path().join("out/code/cpp");
    let ts_out_dir = temp_dir.path().join("out/code/typescript");
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
                type = "gettext_pot"
                domain = "fixture"
                file_name = "fixture.pot"
                context = "table_field"
                source_reference = "both"

                [[targets.exports]]
                type = "rust"
                output = "{}"

                [[targets.exports]]
                type = "cpp"
                output = "{}"

                [[targets.exports]]
                type = "typescript"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            pot_out_dir.display(),
            rust_out_dir.display(),
            cpp_out_dir.display(),
            ts_out_dir.display()
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

    let pot = fs::read_to_string(pot_out_dir.join("fixture.pot")).expect("POT should be written");
    assert!(pot.contains("Project-Id-Version: fixture\\n"));
    assert!(pot.contains("msgctxt \"items.name\""));
    assert!(pot.contains("msgid \"Iron Ore\""));
    assert!(pot.contains("msgid \"Iron Sword\""));
    assert!(pot.contains("#. table=items field=name record=id=2001"));
    assert!(pot.contains("#: "));

    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    let cpp_code = fs::read_to_string(cpp_out_dir.join("generated.hpp")).expect("cpp code");
    let ts_code = fs::read_to_string(ts_out_dir.join("generated.ts")).expect("ts code");
    assert!(rust_code.contains("pub trait TextTranslator"));
    assert!(rust_code.contains("pub fn name_text"));
    assert!(cpp_code.contains("using TextTranslator"));
    assert!(cpp_code.contains("std::string NameText"));
    assert!(ts_code.contains("export type TextTranslator"));
    assert!(ts_code.contains("export function itemsNameText"));

    verify_generated_rust_crate(&rust_out_dir, false);
    verify_generated_cpp_runtime(&cpp_out_dir);
    verify_generated_typescript(&ts_out_dir);
}

#[test]
fn generated_text_helpers_can_call_global_i18n_functions() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let rust_out_dir = temp_dir.path().join("out/code/rust");
    let cpp_out_dir = temp_dir.path().join("out/code/cpp");
    let ts_out_dir = temp_dir.path().join("out/code/typescript");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                type = "rust"
                output = "{}"

                [targets.exports.i18n]
                mode = "global_function"
                function = "crate::global_translate"

                [[targets.exports]]
                type = "cpp"
                output = "{}"

                [targets.exports.i18n]
                mode = "global_function"
                function = "GlobalTranslate"
                cpp_include = "\"i18n.hpp\""

                [[targets.exports]]
                type = "typescript"
                output = "{}"

                [targets.exports.i18n]
                mode = "global_function"
                function = "globalTranslate"
                typescript_import = "import {{ globalTranslate }} from './i18n'"
            "#,
            standard_core_meta_imports(),
            rust_out_dir.display(),
            cpp_out_dir.display(),
            ts_out_dir.display()
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

    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    let cpp_code = fs::read_to_string(cpp_out_dir.join("generated.hpp")).expect("cpp code");
    let ts_code = fs::read_to_string(ts_out_dir.join("generated.ts")).expect("ts code");
    assert!(!rust_code.contains("pub trait TextTranslator"));
    assert!(rust_code.contains("pub fn name_text(&self) -> String"));
    assert!(rust_code.contains("crate::global_translate(Some(\"items.name\"), &self.name).into()"));
    assert!(!cpp_code.contains("using TextTranslator"));
    assert!(cpp_code.contains("#include \"i18n.hpp\""));
    assert!(cpp_code.contains("std::string NameText() const"));
    assert!(cpp_code.contains("return GlobalTranslate(\"items.name\", name.c_str());"));
    assert!(!ts_code.contains("export type TextTranslator"));
    assert!(ts_code.contains("import { globalTranslate } from './i18n';"));
    assert!(ts_code.contains("export function itemsNameText(value: { name: string }): string"));
    assert!(ts_code.contains("return globalTranslate(\"items.name\", value.name);"));

    verify_generated_rust_global_i18n(&rust_out_dir);
    verify_generated_cpp_global_i18n(&cpp_out_dir);
    verify_generated_typescript_global_i18n(&ts_out_dir);
}
