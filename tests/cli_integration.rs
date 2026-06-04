// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path, process::Command as StdCommand};

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
                imports = [{}]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
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
fn builds_cpp_unreal_code_only_target() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_unreal_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "cpp_unreal"
                output = "{}"
                file_name = "reference_game_config"
                namespace = "ReferenceGameConfig"

                [targets.exports.cpp_unreal]
                support = "containers"
                api_macro = "REFERENCE_GAME_CONFIG_API"
            "#,
            standard_core_meta_imports(),
            out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    let header =
        fs::read_to_string(out_dir.join("reference_game_config.h")).expect("cpp_unreal header");
    let source =
        fs::read_to_string(out_dir.join("reference_game_config.cpp")).expect("cpp_unreal source");
    assert!(header.contains("#include \"CoreMinimal.h\""));
    assert!(header.contains("namespace ReferenceGameConfig"));
    assert!(source.contains("#include \"reference_game_config.h\""));

    if command_exists("c++") {
        let shim_dir = temp_dir.path().join("unreal_shim");
        fs::create_dir_all(&shim_dir).expect("shim dir should be created");
        fs::write(
            shim_dir.join("CoreMinimal.h"),
            r#"
                #pragma once
                #include <cstdint>
                #include <memory>
                #include <optional>
                #include <string>
                #include <vector>

                using int64 = std::int64_t;
                using TCHAR = char;
                #define TEXT(value) value
                using FString = std::string;
                template <typename T>
                using TUniquePtr = std::unique_ptr<T>;
                template <typename T>
                struct TArray : public std::vector<T> {
                    using std::vector<T>::vector;
                    void Add(const T& Value) { this->push_back(Value); }
                    void Reserve(std::size_t Count) { this->reserve(Count); }
                };
                template <typename T>
                using TOptional = std::optional<T>;
            "#,
        )
        .expect("CoreMinimal shim should be written");
        let output = StdCommand::new("c++")
            .arg("-std=c++17")
            .arg("-fsyntax-only")
            .arg(out_dir.join("reference_game_config.cpp"))
            .arg("-I")
            .arg(&out_dir)
            .arg("-I")
            .arg(&shim_dir)
            .output()
            .expect("cpp_unreal syntax smoke should run");
        assert_command_success(output, "cpp_unreal syntax smoke");
    }
}

#[test]
fn code_only_target_ignores_missing_project_input_and_sources() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let missing_input_dir = temp_dir.path().join("missing_data_root");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                input = "{}"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            missing_input_dir.display(),
            out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    assert!(!missing_input_dir.exists());
    let rust_code = fs::read_to_string(out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub struct ItemsRow"));
}

#[test]
fn dry_run_serial_and_parallel_outputs_match() {
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
                input = "{}"

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
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            rust_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let serial_stdout = run_config_forge_stdout(&[
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client",
        "--dry-run",
        "--serial",
    ]);
    let parallel_stdout = run_config_forge_stdout(&[
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client",
        "--dry-run",
        "--jobs",
        "2",
        "--io-concurrency",
        "4",
        "--network-concurrency",
        "2",
    ]);

    assert_eq!(serial_stdout, parallel_stdout);
    assert!(serial_stdout.contains("artifact plan (8 entries):"));
    assert!(!data_out_dir.join("items.json").exists());
    assert!(!rust_out_dir.join("generated.rs").exists());
}

#[test]
fn serial_and_parallel_writes_match_for_mixed_target() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let serial_root = temp_dir.path().join("serial");
    let parallel_root = temp_dir.path().join("parallel");
    let serial_project = write_stability_project(temp_dir.path(), "serial", &serial_root);
    let parallel_project = write_stability_project(temp_dir.path(), "parallel", &parallel_root);

    run_config_forge_success(&[
        "--project",
        serial_project.to_str().expect("path should be utf8"),
        "--target",
        "client",
        "--serial",
    ]);
    run_config_forge_success(&[
        "--project",
        parallel_project.to_str().expect("path should be utf8"),
        "--target",
        "client",
        "--jobs",
        "4",
        "--io-concurrency",
        "4",
        "--network-concurrency",
        "2",
    ]);

    let serial_files = collect_file_tree(&serial_root);
    let parallel_files = collect_file_tree(&parallel_root);
    assert_eq!(serial_files, parallel_files);
    assert!(serial_files.contains_key("data/items.json"));
    assert!(serial_files.contains_key("code/rust/generated.rs"));
}

#[test]
fn write_modes_share_artifact_plan_and_written_content() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let direct_root = temp_dir.path().join("direct");
    let temp_commit_root = temp_dir.path().join("temp_commit");
    let dry_run_root = temp_dir.path().join("dry_run");
    let direct_project = write_stability_project(temp_dir.path(), "direct", &direct_root);
    let temp_commit_project =
        write_stability_project(temp_dir.path(), "temp_commit", &temp_commit_root);
    let dry_run_project = write_stability_project(temp_dir.path(), "dry_run", &dry_run_root);

    run_config_forge_success(&[
        "--project",
        direct_project.to_str().expect("path should be utf8"),
        "--target",
        "client",
        "--write-mode",
        "direct",
        "--serial",
    ]);
    run_config_forge_success(&[
        "--project",
        temp_commit_project.to_str().expect("path should be utf8"),
        "--target",
        "client",
        "--write-mode",
        "temp-commit",
        "--jobs",
        "4",
        "--io-concurrency",
        "4",
        "--network-concurrency",
        "2",
    ]);
    let dry_run_stdout = run_config_forge_stdout(&[
        "--project",
        dry_run_project.to_str().expect("path should be utf8"),
        "--target",
        "client",
        "--dry-run",
        "--jobs",
        "4",
        "--io-concurrency",
        "4",
        "--network-concurrency",
        "2",
    ]);

    let direct_files = collect_file_tree(&direct_root);
    let temp_commit_files = collect_file_tree(&temp_commit_root);
    assert_eq!(direct_files, temp_commit_files);
    assert!(!dry_run_root.exists());

    let dry_run_entries = dry_run_artifact_entries(&dry_run_stdout, &dry_run_root);
    let direct_entries = file_tree_entries(&direct_files);
    assert_eq!(dry_run_entries, direct_entries);
}

#[test]
fn dry_run_builds_plan_without_writing_artifacts() {
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
                input = "{}"

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
        "client",
        "--dry-run",
        "--write-mode",
        "temp-commit",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("artifact plan (8 entries):"))
        .stdout(predicate::str::contains("items.json"))
        .stdout(predicate::str::contains("game_rules.json"))
        .stdout(predicate::str::contains("generated.rs"))
        .stdout(predicate::str::contains("written: false"))
        .stdout(predicate::str::contains(
            "target `client` dry-run succeeded",
        ));

    assert!(!data_out_dir.join("items.json").exists());
    assert!(!rust_out_dir.join("generated.rs").exists());
}

#[test]
fn trace_tasks_flag_writes_build_metrics_json() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let trace_path = temp_dir.path().join("trace.json");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
        "--trace-tasks",
        trace_path.to_str().expect("path should be utf8"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    let trace: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&trace_path).expect("trace file should be written"),
    )
    .expect("trace file should contain json");
    assert_eq!(trace["target"], "code_only");
    assert!(trace["jobs"].as_u64().expect("jobs should be numeric") >= 1);
    let stage_names = trace["stages"]
        .as_array()
        .expect("stages should be an array")
        .iter()
        .map(|stage| stage["name"].as_str().expect("stage name"))
        .collect::<Vec<_>>();
    assert!(stage_names.contains(&"load_project"));
    assert!(stage_names.contains(&"prepare_build_inputs"));
    assert!(stage_names.contains(&"run_exporters"));
    assert!(stage_names.contains(&"finish_artifacts"));
    assert!(
        trace["stages"]
            .as_array()
            .expect("stages should be an array")
            .iter()
            .all(|stage| stage["status"] == "succeeded")
    );
}

#[test]
fn trace_tasks_flag_writes_build_metrics_json_on_build_failure() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let trace_path = temp_dir.path().join("trace.json");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "missing_target",
        "--trace-tasks",
        trace_path.to_str().expect("path should be utf8"),
    ]);

    cmd.assert().failure();

    let trace: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&trace_path).expect("trace file should be written on failure"),
    )
    .expect("trace file should contain json");
    assert_eq!(trace["target"], "missing_target");
    let stages = trace["stages"]
        .as_array()
        .expect("stages should be an array");
    assert!(
        stages
            .iter()
            .any(|stage| { stage["name"] == "load_project" && stage["status"] == "succeeded" })
    );
    assert!(
        stages
            .iter()
            .any(|stage| { stage["name"] == "normalize_target" && stage["status"] == "failed" })
    );
    assert!(
        stages
            .iter()
            .all(|stage| stage["name"] != "finish_artifacts")
    );
}

#[test]
fn temp_commit_write_mode_builds_artifacts() {
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
                input = "{}"

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
        "client",
        "--write-mode",
        "temp-commit",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client` built"));

    assert!(data_out_dir.join("items.json").exists());
    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
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

    verify_standard_generated_rust_runtime(fixture_root);
    verify_standard_generated_cpp_runtime(fixture_root);
    verify_standard_generated_typescript_runtime(fixture_root);
    verify_standard_generated_lua_runtime(fixture_root);
}

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
fn builds_target_with_execution_resource_flags() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
        "--jobs",
        "2",
        "--io-concurrency",
        "3",
        "--network-concurrency",
        "2",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));
    assert!(out_dir.join("generated.rs").exists());
}

#[test]
fn builds_target_with_serial_overriding_resource_flags() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
        "--serial",
        "--jobs",
        "8",
        "--io-concurrency",
        "8",
        "--network-concurrency",
        "8",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));
    assert!(out_dir.join("generated.rs").exists());
}

#[test]
fn rejects_zero_jobs_flag() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        r#"
            [[targets]]
            name = "empty"
        "#,
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "empty",
        "--jobs",
        "0",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--jobs must be greater than 0"));
}

#[test]
fn rejects_zero_io_and_network_concurrency_flags() {
    let cases = [
        (
            "--io-concurrency",
            "--io-concurrency must be greater than 0",
        ),
        (
            "--network-concurrency",
            "--network-concurrency must be greater than 0",
        ),
    ];

    for (flag, expected) in cases {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let project_path = temp_dir.path().join("ConfigForge.toml");
        fs::write(
            &project_path,
            r#"
                [[targets]]
                name = "empty"
            "#,
        )
        .expect("project config should be written");

        let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
        cmd.args([
            "--project",
            project_path.to_str().expect("path should be utf8"),
            "--target",
            "empty",
            flag,
            "0",
        ]);

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains(expected));
    }
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
    assert!(rust_code.contains("pub use config_forge_runtime_reader_core"));
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

#[test]
fn rejects_meta_only_export_data_binding() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                input = "{}"

                [[targets]]
                name = "ambiguous"

                [[targets.exports]]
                name = "json"
                output = "{}"
                type = "json"

                [[targets.exports]]
                name = "messagepack"
                output = "{}"
                type = "messagepack"

                [[targets.exports]]
                type = "rust"
                output = "{}"
                data = "json"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            temp_dir.path().join("out/json").display(),
            temp_dir.path().join("out/messagepack").display(),
            temp_dir.path().join("out/rust").display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "ambiguous",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("must not set `data`"));
}

#[test]
fn rejects_runtime_data_path_conflict_before_writing_files() {
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
                name = "conflict"

                [[targets.exports]]
                name = "json_a"
                output = "{}"
                type = "json"

                [[targets.exports]]
                name = "json_b"
                output = "{}"
                type = "json"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            data_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "conflict",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("multiple data artifacts target"));
    assert!(!data_out_dir.join("items.json").exists());
    assert!(!data_out_dir.join("game_rules.json").exists());
}

#[test]
fn rejects_lexically_equivalent_runtime_data_path_conflict_before_writing_files() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_out_dir = temp_dir.path().join("out/data");
    let equivalent_data_out_dir = temp_dir.path().join("out/./data");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                input = "{}"

                [[targets]]
                name = "conflict"

                [[targets.exports]]
                name = "json_a"
                output = "{}"
                type = "json"

                [[targets.exports]]
                name = "json_b"
                output = "{}"
                type = "json"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            equivalent_data_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "conflict",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("multiple data artifacts target"));
    assert!(!data_out_dir.join("items.json").exists());
    assert!(!data_out_dir.join("game_rules.json").exists());
}

#[test]
fn rejects_unknown_second_export_type_before_writing_first_export() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let json_out_dir = temp_dir.path().join("out/json");
    let bad_out_dir = temp_dir.path().join("out/bad");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                input = "{}"

                [[targets]]
                name = "bad_format"

                [[targets.exports]]
                name = "json"
                output = "{}"
                type = "json"

                [[targets.exports]]
                name = "bad"
                output = "{}"
                type = "unknown_binary"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            json_out_dir.display(),
            bad_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "bad_format",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unknown export type `unknown_binary`",
    ));
    assert!(!json_out_dir.join("items.json").exists());
    assert!(!bad_out_dir.exists());
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
        "id,name,server_value,exclude_tags\n1,Potion,,\n1,Debug Potion,,client\n",
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

#[test]
fn range_validator_blocks_invalid_data_before_export() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(data_dir.join("items.csv"), "level\n11\n").expect("items data should be written");

    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                [project]
                name = "validator_project"
                input = "{}"

                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "level"
                type = "int"

                [[tables.fields.validators]]
                kind = "range"
                min = 1
                max = 10

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"
            "#,
            data_dir.display(),
            data_out_dir.display()
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
        .failure()
        .stderr(predicate::str::contains("exceeds max `10`"));
    assert!(!data_out_dir.join("items.json").exists());
}

#[test]
fn warning_validator_is_visible_but_does_not_block_export() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(data_dir.join("items.csv"), "level\n11\n").expect("items data should be written");

    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                [project]
                name = "validator_project"
                input = "{}"

                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "level"
                type = "int"

                [[tables.fields.validators]]
                kind = "range"
                name = "recommended_level"
                severity = "warning"
                min = 1
                max = 10

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"
            "#,
            data_dir.display(),
            data_out_dir.display()
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
        .stderr(predicate::str::contains("warning: recommended_level"))
        .stdout(predicate::str::contains("target `client` built"));
    assert!(data_out_dir.join("items.json").exists());
}

fn absolute_fixture_path(relative_path: &str) -> String {
    std::env::current_dir()
        .expect("current dir")
        .join("tests/fixtures/standard_project")
        .join(relative_path)
        .display()
        .to_string()
}

fn run_config_forge_stdout(args: &[&str]) -> String {
    let output = Command::cargo_bin("config-forge")
        .expect("binary should build")
        .args(args)
        .output()
        .expect("config-forge should run");
    assert!(
        output.status.success(),
        "config-forge failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout should be utf8")
}

fn run_config_forge_success(args: &[&str]) {
    let output = Command::cargo_bin("config-forge")
        .expect("binary should build")
        .args(args)
        .output()
        .expect("config-forge should run");
    assert!(
        output.status.success(),
        "config-forge failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_stability_project(temp_root: &Path, name: &str, output_root: &Path) -> std::path::PathBuf {
    let project_path = temp_root.join(format!("ConfigForge_{name}.toml"));
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

                [[targets.exports]]
                type = "rust"
                output = "{}"
                file_header = "// Stable generated fixture\n"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            output_root.join("data").display(),
            output_root.join("code/rust").display()
        ),
    )
    .expect("project config should be written");
    project_path
}

fn collect_file_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect_file_tree_into(root, root, &mut files);
    files
}

fn collect_file_tree_into(root: &Path, current: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    if !current.exists() {
        return;
    }
    let mut entries = fs::read_dir(current)
        .expect("directory should be readable")
        .map(|entry| entry.expect("directory entry should be readable"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_file_tree_into(root, &path, files);
        } else {
            let relative_path = path
                .strip_prefix(root)
                .expect("file should be under root")
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            files.insert(
                relative_path,
                fs::read(path).expect("file should be readable"),
            );
        }
    }
}

fn file_tree_entries(files: &BTreeMap<String, Vec<u8>>) -> Vec<(String, usize)> {
    files
        .iter()
        .map(|(path, content)| (path.clone(), content.len()))
        .collect()
}

fn dry_run_artifact_entries(stdout: &str, output_root: &Path) -> Vec<(String, usize)> {
    let mut entries = Vec::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        let Some((path, metadata)) = rest.rsplit_once(" (") else {
            continue;
        };
        let Some((bytes, written)) = metadata.strip_suffix(')').and_then(|metadata| {
            let (bytes, written) = metadata.split_once(" bytes, written: ")?;
            Some((bytes.parse::<usize>().ok()?, written))
        }) else {
            continue;
        };
        assert_eq!(written, "false");
        let relative_path = Path::new(path)
            .strip_prefix(output_root)
            .expect("artifact path should be under output root")
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        entries.push((relative_path, bytes));
    }
    entries.sort();
    entries
}

fn standard_core_meta_imports() -> String {
    ["meta/items.toml", "meta/characters.toml", "meta/misc.toml"]
        .into_iter()
        .map(|path| format!("\"{}\"", absolute_fixture_path(path)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn standard_reference_game_meta_imports() -> String {
    ["meta/items.toml", "meta/characters.toml", "meta/misc.toml"]
        .into_iter()
        .map(|path| format!("\"{}\"", absolute_fixture_path(path)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn verify_generated_rust_crate(generated_dir: &std::path::Path, derive_serde: bool) {
    let reader_core_dependency = format!(
        "config-forge-runtime-reader-core = {{ path = {:?} }}",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/rust/reader_core")
    );
    for edition in ["2021", "2024"] {
        fs::write(
            generated_dir.join("Cargo.toml"),
            format!(
                r#"
                [package]
                name = "generated_config_check"
                version = "0.1.0"
                edition = "{edition}"

                [lib]
                path = "lib.rs"

                [dependencies]
                {reader_core_dependency}
                {dependencies}
            "#,
                edition = edition,
                reader_core_dependency = reader_core_dependency,
                dependencies = if derive_serde {
                    "serde = { version = \"1.0\", features = [\"derive\"] }\nserde_json = \"1.0\""
                } else {
                    ""
                }
            ),
        )
        .expect("generated rust Cargo.toml should be written");
        fs::write(
        generated_dir.join("lib.rs"),
        format!(
            r#"
                include!("generated.rs");

                struct MockReader {{
                    path: &'static str,
                    fields: std::cell::RefCell<Vec<&'static str>>,
                }}

                impl MockReader {{
                    fn root(path: &'static str) -> Self {{
                        Self {{ path, fields: std::cell::RefCell::new(Vec::new()) }}
                    }}

                    fn current_field(&self) -> Option<&'static str> {{
                        self.fields.borrow().last().copied()
                    }}
                }}

                impl RuntimeDataReader for MockReader {{
                    fn enter_field(&self, name: &'static str) -> RuntimeDataResult<bool> {{
                        let field = match name {{
                            "rows" => "rows",
                            "id" => "id",
                            "name" => "name",
                            "price" => "price",
                            "max_player_level" => "max_player_level",
                            "starter_gold" => "starter_gold",
                            "default_class_id" => "default_class_id",
                            "starter_item_ids" => "starter_item_ids",
                            "stamina_cap" => "stamina_cap",
                            "daily_reset_hour" => "daily_reset_hour",
                            "pvp_enabled" => "pvp_enabled",
                            "stack" | "attributes" | "skill" => return Ok(false),
                            other => return Err(RuntimeDataError::not_found(format!("unexpected field: {{other}}"))),
                        }};
                        self.fields.borrow_mut().push(field);
                        Ok(true)
                    }}

                    fn enter_index(&self, index: usize) -> RuntimeDataResult<()> {{
                        if self.path == "items" && self.current_field() == Some("rows") && index == 0 {{
                            self.fields.borrow_mut().push("");
                            Ok(())
                        }} else if self.path == "game_rules" && self.current_field() == Some("starter_item_ids") && index < 2 {{
                            self.fields.borrow_mut().push(if index == 0 {{ "starter_item_ids[0]" }} else {{ "starter_item_ids[1]" }});
                            Ok(())
                        }} else {{
                            Err(RuntimeDataError::not_found(format!("unexpected index: {{index}}")))
                        }}
                    }}

                    fn exit(&self) -> RuntimeDataResult<()> {{
                        self.fields
                            .borrow_mut()
                            .pop()
                            .map(|_| ())
                            .ok_or_else(|| RuntimeDataError::decode("unexpected exit"))
                    }}

                    fn len(&self) -> RuntimeDataResult<usize> {{
                        if self.path == "items" && self.current_field() == Some("rows") {{ Ok(1) }} else if matches!(self.path, "classes" | "class_levels" | "character_surnames" | "character_male_given_names" | "character_female_given_names") && self.current_field() == Some("rows") {{ Ok(0) }} else if self.path == "game_rules" && self.current_field() == Some("starter_item_ids") {{ Ok(2) }} else {{ Err(RuntimeDataError::type_mismatch("unexpected len")) }}
                    }}

                    fn as_i64(&self) -> RuntimeDataResult<i64> {{
                        match (self.path, self.current_field()) {{
                            ("items", Some("id")) => Ok(1),
                            ("items", Some("price")) => Ok(10),
                            ("game_rules", Some("max_player_level")) => Ok(60),
                            ("game_rules", Some("starter_gold")) => Ok(500),
                            ("game_rules", Some("default_class_id")) => Ok(1),
                            ("game_rules", Some("starter_item_ids[0]")) => Ok(2001),
                            ("game_rules", Some("starter_item_ids[1]")) => Ok(2002),
                            ("game_rules", Some("stamina_cap")) => Ok(120),
                            ("game_rules", Some("daily_reset_hour")) => Ok(5),
                            _ => Err(RuntimeDataError::type_mismatch("unexpected i64")),
                        }}
                    }}

                    fn as_f64(&self) -> RuntimeDataResult<f64> {{ Ok(self.as_i64()? as f64) }}

                    fn as_bool(&self) -> RuntimeDataResult<bool> {{
                        if self.path == "game_rules" && self.current_field() == Some("pvp_enabled") {{ Ok(true) }} else {{ Err(RuntimeDataError::type_mismatch("unexpected bool")) }}
                    }}

                    fn as_string(&self) -> RuntimeDataResult<String> {{
                        if self.path == "items" && self.current_field() == Some("name") {{ Ok("Potion".to_owned()) }} else {{ Err(RuntimeDataError::type_mismatch("unexpected string")) }}
                    }}

                }}

                struct MockLoader;

                impl RuntimeDataLoader for MockLoader {{
                    type Reader = MockReader;

                    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader> {{
                        match path {{
                            "items" => Ok(MockReader::root("items")),
                            "classes" => Ok(MockReader::root("classes")),
                            "class_levels" => Ok(MockReader::root("class_levels")),
                            "character_surnames" => Ok(MockReader::root("character_surnames")),
                            "character_male_given_names" => Ok(MockReader::root("character_male_given_names")),
                            "character_female_given_names" => Ok(MockReader::root("character_female_given_names")),
                            "game_rules" => Ok(MockReader::root("game_rules")),
                            other => Err(RuntimeDataError::not_found(format!("unexpected path: {{other}}"))),
                        }}
                    }}
                }}

                struct MockTranslator;

                impl TextTranslator for MockTranslator {{
                    fn translate(&self, msgctxt: Option<&str>, msgid: &str) -> Option<String> {{
                        match (msgctxt, msgid) {{
                            (Some("items.name"), "Potion") => Some("药水".to_owned()),
                            _ => None,
                        }}
                    }}
                }}

                #[test]
                fn generated_code_loads_and_queries_rows() {{
                    let data = GameData::load(&MockLoader).expect("generated data should load");
                    let item = data.items.get_by_id(1).expect("item should be indexed");
                    assert_eq!(item.name, "Potion");
                    assert_eq!(item.name_text(&MockTranslator), "药水");
                    assert_eq!(data.game_rules.max_player_level, 60);
                    assert!(data.items.get_by_id(404).is_none());
                    assert_eq!(data.game_rules.default_class_id, 1);
                    assert_eq!(data.game_rules.starter_item_ids, vec![2001, 2002]);
                    assert!(data.game_rules.pvp_enabled);
                }}

                {serde_assertion}
            "#,
            serde_assertion = if derive_serde {
                "#[test]\nfn generated_rows_support_serde() {\n    let _ = serde_json::to_string(&ItemsRow { id: 1, name: \"Potion\".to_owned(), price: 10, stack: None, attributes: None, skill: None }).unwrap();\n    let _ = serde_json::to_string(&GameRulesRow { max_player_level: 60, starter_gold: 500, default_class_id: 1, starter_item_ids: vec![2001], stamina_cap: 120, daily_reset_hour: 5, pvp_enabled: true, default_class_link: RefCell::new(LinkCache::default()), starter_items_link: RefCell::new(LinkListCache::default()) }).unwrap();\n}"
            } else {
                ""
            }
        ),
    )
    .expect("generated rust lib.rs should be written");

        let output = StdCommand::new("cargo")
            .args(["test", "--quiet", "--manifest-path"])
            .arg(generated_dir.join("Cargo.toml"))
            .output()
            .expect("generated Rust cargo test should run");
        assert_command_success(output, &format!("generated Rust cargo test ({edition})"));
    }
}

fn verify_generated_rust_global_i18n(generated_dir: &std::path::Path) {
    let reader_core_dependency = format!(
        "config-forge-runtime-reader-core = {{ path = {:?} }}",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/rust/reader_core")
    );
    fs::write(
        generated_dir.join("Cargo.toml"),
        format!(
            r#"
                [package]
                name = "generated_global_i18n_check"
                version = "0.1.0"
                edition = "2024"

                [lib]
                path = "lib.rs"

                [dependencies]
                {reader_core_dependency}
            "#
        ),
    )
    .expect("generated global i18n Rust Cargo.toml should be written");
    fs::write(
        generated_dir.join("lib.rs"),
        r#"
            fn global_translate(context: Option<&str>, msgid: &str) -> String {
                if context == Some("items.name") && msgid == "Potion" {
                    "Potion CN".to_owned()
                } else {
                    msgid.to_owned()
                }
            }

            include!("generated.rs");

            #[test]
            fn global_i18n_text_helper_does_not_require_translator_arg() {
                let row = ItemsRow {
                    id: 1,
                    name: "Potion".to_owned(),
                    price: 10,
                    stack: None,
                    attributes: None,
                    skill: None,
                };
                assert_eq!(row.name_text(), "Potion CN");
            }
        "#,
    )
    .expect("generated global i18n Rust lib.rs should be written");

    let output = StdCommand::new("cargo")
        .args(["test", "--quiet", "--manifest-path"])
        .arg(generated_dir.join("Cargo.toml"))
        .output()
        .expect("generated global i18n Rust cargo test should run");
    assert_command_success(output, "generated Rust global i18n cargo test");
}

fn verify_generated_cpp_runtime(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("compile_check.cpp"),
        r#"
            #include "generated.hpp"

            #include <memory>
            #include <stdexcept>
            #include <vector>

            class MockReader final : public IRuntimeDataReader {
            public:
                explicit MockReader(std::string path) : path_(std::move(path)) {}

                void EnterIndex(std::size_t index) override {
                    if (path_ == "items" && CurrentField() == "rows" && index == 0) {
                        fields_.push_back("");
                        return;
                    }
                    if (path_ == "game_rules" && CurrentField() == "starter_item_ids" && index < 2) {
                        fields_.push_back("");
                        return;
                    }
                    throw std::runtime_error("unexpected index");
                }

                void ExitField() override {
                    if (fields_.empty()) { throw std::runtime_error("unexpected exit"); }
                    fields_.pop_back();
                }

                std::size_t ReadRowCount() override {
                    if (path_ == "items" && CurrentField() == "rows") {
                        return 1;
                    }
                    if ((path_ == "classes" || path_ == "class_levels" || path_ == "character_surnames" || path_ == "character_male_given_names" || path_ == "character_female_given_names") && CurrentField() == "rows") {
                        return 0;
                    }
                    if (path_ == "game_rules" && CurrentField() == "starter_item_ids") {
                        return 2;
                    }
                    throw std::runtime_error("unexpected row count");
                }

                std::size_t ReadListLength() override {
                    if (path_ == "game_rules" && CurrentField() == "starter_item_ids") {
                        return 2;
                    }
                    throw std::runtime_error("unexpected list length");
                }

                std::int64_t AsInt64() const override {
                    if (path_ == "items" && CurrentField() == "id") { return 1; }
                    if (path_ == "items" && CurrentField() == "price") { return 10; }
                    if (path_ == "game_rules" && CurrentField() == "max_player_level") { return 60; }
                    if (path_ == "game_rules" && CurrentField() == "starter_gold") { return 500; }
                    if (path_ == "game_rules" && CurrentField() == "default_class_id") { return 1; }
                    if (path_ == "game_rules" && CurrentField() == "") { return 2001; }
                    if (path_ == "game_rules" && CurrentField() == "stamina_cap") { return 120; }
                    if (path_ == "game_rules" && CurrentField() == "daily_reset_hour") { return 5; }
                    throw std::runtime_error("unexpected int64");
                }

                double AsDouble() const override { return static_cast<double>(AsInt64()); }

                bool AsBool() const override {
                    if (path_ == "game_rules" && CurrentField() == "pvp_enabled") { return true; }
                    throw std::runtime_error("unexpected bool");
                }

                std::string AsString() const override {
                    if (path_ == "items" && CurrentField() == "name") { return "Potion"; }
                    throw std::runtime_error("unexpected string");
                }

                void EnterRequiredField(const std::string& name) override {
                    if (!EnterNamedField(name)) { throw std::runtime_error("missing required field"); }
                }

                bool EnterOptionalField(const std::string& name) override {
                    return EnterNamedField(name);
                }
            private:
                bool EnterNamedField(const std::string& name) {
                    if (name == "stack" || name == "attributes" || name == "skill") {
                        return false;
                    }
                    fields_.push_back(name);
                    return true;
                }

                const std::string& CurrentField() const {
                    static const std::string root;
                    return fields_.empty() ? root : fields_.back();
                }

                std::string path_;
                std::vector<std::string> fields_;
            };

            class MockLoader final : public IRuntimeDataLoader {
            public:
                std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) override {
                    if (path == "items" || path == "classes" || path == "class_levels" || path == "character_surnames" || path == "character_male_given_names" || path == "character_female_given_names" || path == "game_rules") {
                        return std::make_unique<MockReader>(path);
                    }
                    throw std::runtime_error("unexpected path");
                }
            };

            int main() {
                ItemsRow row{};
                row.id = 1;
                row.name = "Potion";
                row.price = 10;
                TextTranslator translator = [](const char* context, const std::string& msgid) -> std::optional<std::string> {
                    if (std::string(context) == "items.name" && msgid == "Potion") {
                        return std::string("Potion CN");
                    }
                    return std::nullopt;
                };
                if (row.NameText(translator) != "Potion CN") {
                    return 6;
                }
                (void)row;
                GameRulesRow rules{};
                rules.max_player_level = 60;
                rules.starter_gold = 500;
                rules.default_class_id = 1;
                rules.starter_item_ids = {2001, 2002};
                rules.stamina_cap = 120;
                rules.daily_reset_hour = 5;
                rules.pvp_enabled = true;
                if (std::string(GameRulesRow::DataFile) != "game_rules") {
                    return 1;
                }
                (void)rules;
                MockLoader loader;
                GameData data = GameData::Load(loader);
                const ItemsRow* item = data.Items().GetById(1);
                if (item == nullptr || item->name != "Potion") {
                    return 2;
                }
                if (data.Items().GetById(404) != nullptr) {
                    return 3;
                }
                if (data.GameRules().max_player_level != 60) {
                    return 4;
                }
                if (data.GameRules().default_class_id != 1 || data.GameRules().starter_item_ids.size() != 2) {
                    return 5;
                }
                return 0;
            }
        "#,
    )
    .expect("generated C++ compile check should be written");

    if !command_exists("c++") {
        eprintln!("skipping generated C++ runtime check: c++ not found");
        return;
    }

    let output = StdCommand::new("c++")
        .args([
            "-std=c++17",
            generated_dir
                .join("compile_check.cpp")
                .to_str()
                .expect("utf8 path"),
            generated_dir
                .join("generated.cpp")
                .to_str()
                .expect("utf8 path"),
            "-o",
            generated_dir
                .join("compile_check")
                .to_str()
                .expect("utf8 path"),
        ])
        .output()
        .expect("generated C++ compile check should compile");
    assert_command_success(output, "generated C++ header compile");

    let output = StdCommand::new(generated_dir.join("compile_check"))
        .output()
        .expect("generated C++ compile check should run");
    assert_command_success(output, "generated C++ runtime check");
}

fn verify_generated_cpp_global_i18n(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("i18n.hpp"),
        r#"
            #pragma once
            #include <string>

            inline std::string GlobalTranslate(const char* context, const char* msgid) {
                if (std::string(context) == "items.name" && std::string(msgid) == "Potion") {
                    return "Potion CN";
                }
                return msgid;
            }
        "#,
    )
    .expect("generated C++ i18n header should be written");
    fs::write(
        generated_dir.join("global_i18n_check.cpp"),
        r#"
            #include "generated.hpp"

            int main() {
                ItemsRow row{};
                row.id = 1;
                row.name = "Potion";
                row.price = 10;
                return row.NameText() == "Potion CN" ? 0 : 1;
            }
        "#,
    )
    .expect("generated C++ global i18n check should be written");

    if !command_exists("c++") {
        eprintln!("skipping generated C++ global i18n check: c++ not found");
        return;
    }

    let output = StdCommand::new("c++")
        .args(["-std=c++17"])
        .arg(generated_dir.join("global_i18n_check.cpp"))
        .arg(generated_dir.join("generated.cpp"))
        .arg("-o")
        .arg(generated_dir.join("global_i18n_check"))
        .output()
        .expect("generated C++ global i18n check should compile");
    assert_command_success(output, "generated C++ global i18n compile");

    let output = StdCommand::new(generated_dir.join("global_i18n_check"))
        .output()
        .expect("generated C++ global i18n check should run");
    assert_command_success(output, "generated C++ global i18n runtime check");
}

fn verify_generated_typescript(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("compile_check.ts"),
        r#"
            import { GameData, RuntimeDataLoader, RuntimeDataReader, itemsNameText } from "./generated";

            class MockReader implements RuntimeDataReader {
                private readonly fields: string[] = [];

                constructor(private readonly path: string) {}

                enterField(name: string): boolean {
                    if (name === "stack" || name === "attributes" || name === "skill") {
                        return false;
                    }
                    this.fields.push(name);
                    return true;
                }

                enterIndex(index: number): void {
                    if (this.path === "items" && this.currentField() === "rows" && index === 0) {
                        this.fields.push("");
                        return;
                    }
                    if (this.path === "game_rules" && this.currentField() === "starter_item_ids" && index < 2) {
                        this.fields.push("");
                        return;
                    }
                    throw new Error(`unexpected index: ${index}`);
                }

                exit(): void {
                    if (this.fields.length === 0) {
                        throw new Error("unexpected exit");
                    }
                    this.fields.pop();
                }

                length(): number {
                    if (this.path === "items" && this.currentField() === "rows") {
                        return 1;
                    }
                    if ((this.path === "classes" || this.path === "class_levels") && this.currentField() === "rows") {
                        return 0;
                    }
                    if (this.path === "game_rules" && this.currentField() === "starter_item_ids") {
                        return 2;
                    }
                    throw new Error("unexpected length");
                }

                asNumber(): number {
                    if (this.path === "items" && this.currentField() === "id") { return 1; }
                    if (this.path === "items" && this.currentField() === "price") { return 10; }
                    if (this.path === "game_rules" && this.currentField() === "max_player_level") { return 60; }
                    if (this.path === "game_rules" && this.currentField() === "starter_gold") { return 500; }
                    if (this.path === "game_rules" && this.currentField() === "default_class_id") { return 1; }
                    if (this.path === "game_rules" && this.currentField() === "") { return 2001; }
                    if (this.path === "game_rules" && this.currentField() === "stamina_cap") { return 120; }
                    if (this.path === "game_rules" && this.currentField() === "daily_reset_hour") { return 5; }
                    throw new Error("unexpected number");
                }

                asBool(): boolean {
                    if (this.path === "game_rules" && this.currentField() === "pvp_enabled") { return true; }
                    throw new Error("unexpected bool");
                }

                asString(): string {
                    if (this.path === "items" && this.currentField() === "name") { return "Potion"; }
                    throw new Error("unexpected string");
                }

                private currentField(): string {
                    return this.fields[this.fields.length - 1] ?? "";
                }
            }

            const loader: RuntimeDataLoader = {
                open(path: string): RuntimeDataReader {
                    if (path === "items" || path === "classes" || path === "class_levels" || path === "game_rules") {
                        return new MockReader(path);
                    }
                    throw new Error(`unexpected path: ${path}`);
                },
            };

            async function main(): Promise<void> {
                const data = await GameData.load(loader);
                const item = data.items.getById(1);
                if (!item || item.name !== "Potion") {
                    throw new Error("generated TypeScript query failed");
                }
                const translated = itemsNameText(item, (context, msgid) => context === "items.name" && msgid === "Potion" ? "Potion CN" : undefined);
                if (translated !== "Potion CN") {
                    throw new Error("generated TypeScript text translation failed");
                }
                if (data.gameRules.maxPlayerLevel !== 60 || !data.gameRules.pvpEnabled) {
                    throw new Error("generated TypeScript object table failed");
                }
                if (data.gameRules.defaultClassId !== 1 || data.gameRules.starterItemIds.length !== 2) {
                    throw new Error("generated TypeScript list field failed");
                }
            }

            main().catch((error) => {
                console.error(error);
                throw error;
            });
        "#,
    )
    .expect("generated TypeScript compile check should be written");
    fs::write(
        generated_dir.join("tsconfig.json"),
        r#"
            {
                "compilerOptions": {
                    "strict": true,
                    "target": "ES2020",
                    "module": "ESNext",
                    "moduleResolution": "Bundler",
                    "noEmit": true,
                    "skipLibCheck": true
                },
                "include": ["generated.ts", "compile_check.ts"]
            }
        "#,
    )
    .expect("generated TypeScript tsconfig should be written");

    if command_exists("tsc") {
        assert_command_success(
            StdCommand::new("tsc")
                .args(["--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript type check should run"),
            "generated TypeScript tsc",
        );
    } else if command_exists("npx") {
        assert_command_success(
            StdCommand::new("npx")
                .args(["--yes", "-p", "typescript", "tsc", "--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript npx type check should run"),
            "generated TypeScript npx tsc",
        );
    } else {
        eprintln!("skipping generated TypeScript type check: tsc/npx not found");
    }
}

fn verify_generated_typescript_global_i18n(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("i18n.ts"),
        r#"
            export function globalTranslate(context: string | undefined, msgid: string): string {
              return context === "items.name" && msgid === "Potion" ? "Potion CN" : msgid;
            }
        "#,
    )
    .expect("generated TypeScript i18n module should be written");
    fs::write(
        generated_dir.join("global_i18n_check.ts"),
        r#"
            import { itemsNameText } from "./generated";

            const translated = itemsNameText({ name: "Potion" });
            if (translated !== "Potion CN") {
              throw new Error("generated TypeScript global i18n failed");
            }
        "#,
    )
    .expect("generated TypeScript global i18n check should be written");
    fs::write(
        generated_dir.join("tsconfig.json"),
        r#"
            {
                "compilerOptions": {
                    "strict": true,
                    "target": "ES2020",
                    "module": "ESNext",
                    "moduleResolution": "Bundler",
                    "noEmit": true,
                    "skipLibCheck": true
                },
                "include": ["generated.ts", "i18n.ts", "global_i18n_check.ts"]
            }
        "#,
    )
    .expect("generated TypeScript global i18n tsconfig should be written");

    if command_exists("tsc") {
        assert_command_success(
            StdCommand::new("tsc")
                .args(["--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript global i18n type check should run"),
            "generated TypeScript global i18n tsc",
        );
    } else if command_exists("npx") {
        assert_command_success(
            StdCommand::new("npx")
                .args(["--yes", "-p", "typescript", "tsc", "--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript global i18n npx type check should run"),
            "generated TypeScript global i18n npx tsc",
        );
    } else {
        eprintln!("skipping generated TypeScript global i18n type check: tsc/npx not found");
    }
}

fn verify_standard_generated_rust_runtime(fixture_root: &std::path::Path) {
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

fn verify_standard_generated_cpp_runtime(fixture_root: &std::path::Path) {
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

fn verify_standard_generated_typescript_runtime(fixture_root: &std::path::Path) {
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

fn verify_standard_generated_lua_runtime(fixture_root: &std::path::Path) {
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

fn command_exists(command: &str) -> bool {
    StdCommand::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn assert_command_success(output: std::process::Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
