use std::{fs, process::Command as StdCommand};

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

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
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
                imports = [{}]

                [project]
                name = "standard_project"
                desc = "Standard fixture"
                copyright = "Copyright Fixture"

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.data]]
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
            standard_core_meta_imports(),
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
    assert!(data_out_dir.join("game_rules.json").exists());
    let game_rules_json = fs::read_to_string(data_out_dir.join("game_rules.json"))
        .expect("game rules json should exist");
    let game_rules: serde_json::Value =
        serde_json::from_str(&game_rules_json).expect("game rules json should parse");
    assert_eq!(game_rules["max_player_level"], "60");
    assert_eq!(game_rules["starter_gold"], "500");
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
    assert!(rust_code.contains("pub trait RuntimeDataReader"));
    assert!(rust_code.contains("fn open(&self, path: &str) -> Result<Self::Reader, String>;"));
    assert!(!rust_code.contains("pub struct GameRulesTable"));
    assert!(rust_code.contains("pub trait RuntimeDataLoader"));
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"items.json\""));
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"game_rules.json\""));
    assert!(!rust_code.contains("RuntimeDataFormat"));
    assert!(rust_code.contains("pub fn get_by_id"));
    assert!(rust_code.contains("Self::read_from_reader(&root)"));
    assert!(cpp_code.contains("class IRuntimeDataLoader"));
    assert!(cpp_code.contains("class IRuntimeDataReader"));
    assert!(cpp_code.contains("static constexpr const char* DataFile = \"items.json\""));
    assert!(cpp_code
        .contains("virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path)"));
    assert!(cpp_code.contains("static constexpr const char* DataFile = \"game_rules.json\""));
    assert!(!cpp_code.contains("class GameRulesTable"));
    assert!(!cpp_code.contains("RuntimeDataFormat"));
    assert!(cpp_code.contains("const ItemsRow* GetById"));
    assert!(cpp_source.contains("GameData GameData::Load"));
    assert!(cpp_source.contains("ItemsTable ItemsTable::FromRows"));
    assert!(cpp_source.contains("loader.Open(GameRulesRow::DataFile)"));
    assert!(ts_code.contains("export interface RuntimeDataLoader"));
    assert!(ts_code.contains("export interface RuntimeDataReader"));
    assert!(ts_code.contains("static readonly dataFile = \"items.json\""));
    assert!(ts_code.contains("open(path: string): Promise<RuntimeDataReader> | RuntimeDataReader"));
    assert!(!ts_code.contains("export class GameRulesTable"));
    assert!(!ts_code.contains("RuntimeDataFormat"));
    assert!(ts_code.contains("getById(id: number): ItemsRow | undefined"));

    verify_generated_rust_crate(&rust_out_dir, false);
    verify_generated_cpp_runtime(&cpp_out_dir);
    verify_generated_typescript(&ts_out_dir);
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

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.data]]
                input = "{}"
                output = "{}"
                format = "json"

                [[targets]]
                name = "server"
                tags = ["server"]

                [[targets.data]]
                input = "{}"
                output = "{}"
                format = "messagepack"
            "#,
            standard_reference_game_meta_imports(),
            absolute_fixture_path("data"),
            client_out_dir.display(),
            absolute_fixture_path("data"),
            server_out_dir.display(),
        ),
    )
    .expect("project config should be written");

    let mut client_cmd = Command::cargo_bin("config_forge").expect("binary should build");
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

    let mut server_cmd = Command::cargo_bin("config_forge").expect("binary should build");
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

    let mut client_cmd = Command::cargo_bin("config_forge").expect("binary should build");
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

    let mut server_cmd = Command::cargo_bin("config_forge").expect("binary should build");
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

    verify_standard_generated_rust_runtime(fixture_root);
    verify_standard_generated_cpp_runtime(fixture_root);
    verify_standard_generated_typescript_runtime(fixture_root);
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

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
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

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
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

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
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

        let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
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

                [[targets]]
                name = "client_messagepack"
                tags = ["client"]

                [[targets.data]]
                input = "{}"
                output = "{}"
                format = "messagepack"

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
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

    verify_generated_rust_crate(&rust_out_dir, false);
}

#[test]
fn binds_code_tasks_to_selected_data_tasks() {
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

                [[targets]]
                name = "multi_artifact"
                tags = ["client"]

                [[targets.data]]
                name = "json"
                input = "{}"
                output = "{}"
                format = "json"

                [[targets.data]]
                name = "messagepack"
                input = "{}"
                output = "{}"
                format = "messagepack"

                [[targets.code]]
                language = "typescript"
                output = "{}"
                data = "json"

                [[targets.code]]
                language = "rust"
                output = "{}"
                data = "messagepack"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            json_data_dir.display(),
            absolute_fixture_path("data"),
            messagepack_data_dir.display(),
            ts_out_dir.display(),
            rust_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
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
    assert!(ts_code.contains("static readonly dataFile = \"items.json\""));
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"items.msgpack\""));
}

#[test]
fn rejects_code_task_without_data_binding_when_multiple_data_tasks_exist() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [[targets]]
                name = "ambiguous"

                [[targets.data]]
                name = "json"
                input = "{}"
                output = "{}"
                format = "json"

                [[targets.data]]
                name = "messagepack"
                input = "{}"
                output = "{}"
                format = "messagepack"

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            temp_dir.path().join("out/json").display(),
            absolute_fixture_path("data"),
            temp_dir.path().join("out/messagepack").display(),
            temp_dir.path().join("out/rust").display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "ambiguous",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("must set `data`"));
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

                [[targets]]
                name = "conflict"

                [[targets.data]]
                name = "json_a"
                input = "{}"
                output = "{}"
                format = "json"

                [[targets.data]]
                name = "json_b"
                input = "{}"
                output = "{}"
                format = "json"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            absolute_fixture_path("data"),
            data_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "conflict",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "multiple runtime data artifacts target",
    ));
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

                [[targets]]
                name = "conflict"

                [[targets.data]]
                name = "json_a"
                input = "{}"
                output = "{}"
                format = "json"

                [[targets.data]]
                name = "json_b"
                input = "{}"
                output = "{}"
                format = "json"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            absolute_fixture_path("data"),
            equivalent_data_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "conflict",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "multiple runtime data artifacts target",
    ));
    assert!(!data_out_dir.join("items.json").exists());
    assert!(!data_out_dir.join("game_rules.json").exists());
}

#[test]
fn rejects_unknown_second_data_format_before_writing_first_data_task() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let json_out_dir = temp_dir.path().join("out/json");
    let bad_out_dir = temp_dir.path().join("out/bad");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [[targets]]
                name = "bad_format"

                [[targets.data]]
                name = "json"
                input = "{}"
                output = "{}"
                format = "json"

                [[targets.data]]
                name = "bad"
                input = "{}"
                output = "{}"
                format = "unknown_binary"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            json_out_dir.display(),
            absolute_fixture_path("data"),
            bad_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "bad_format",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("no data exporter registered"));
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

                [[targets.data]]
                input = "{}"
                output = "{}"
                format = "json"

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            data_dir.display(),
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

                [[targets.data]]
                input = "{}"
                output = "{}"
                format = "json"
            "#,
            data_dir.display(),
            data_out_dir.display()
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

                [[targets.data]]
                input = "{}"
                output = "{}"
                format = "json"
            "#,
            data_dir.display(),
            data_out_dir.display()
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

fn standard_core_meta_imports() -> String {
    ["meta/items.toml", "meta/misc.toml"]
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
    fs::write(
        generated_dir.join("Cargo.toml"),
        format!(
            r#"
                [package]
                name = "generated_config_check"
                version = "0.1.0"
                edition = "2021"

                [lib]
                path = "lib.rs"

                {dependencies}
            "#,
            dependencies = if derive_serde {
                "[dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\nserde_json = \"1.0\""
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

                #[derive(Clone)]
                struct MockReader {{
                    path: &'static str,
                    index: Option<usize>,
                    field: Option<&'static str>,
                }}

                impl MockReader {{
                    fn root(path: &'static str) -> Self {{
                        Self {{ path, index: None, field: None }}
                    }}
                }}

                impl RuntimeDataReader for MockReader {{
                    fn field(&self, name: &str) -> Result<Option<Self>, String> {{
                        let field = match name {{
                            "rows" => "rows",
                            "id" => "id",
                            "name" => "name",
                            "price" => "price",
                            "max_player_level" => "max_player_level",
                            "starter_gold" => "starter_gold",
                            "stamina_cap" => "stamina_cap",
                            "daily_reset_hour" => "daily_reset_hour",
                            "pvp_enabled" => "pvp_enabled",
                            "stack" | "attributes" | "skill" => return Ok(None),
                            other => return Err(format!("unexpected field: {{other}}")),
                        }};
                        Ok(Some(Self {{ path: self.path, index: self.index, field: Some(field) }}))
                    }}

                    fn index(&self, index: usize) -> Result<Self, String> {{
                        if self.path == "items" && self.field == Some("rows") && index == 0 {{
                            Ok(Self {{ path: self.path, index: Some(index), field: None }})
                        }} else {{
                            Err(format!("unexpected index: {{index}}"))
                        }}
                    }}

                    fn len(&self) -> Result<usize, String> {{
                        if self.path == "items" && self.field == Some("rows") {{ Ok(1) }} else {{ Err("unexpected len".to_owned()) }}
                    }}

                    fn as_i64(&self) -> Result<i64, String> {{
                        match (self.path, self.field) {{
                            ("items", Some("id")) => Ok(1),
                            ("items", Some("price")) => Ok(10),
                            ("game_rules", Some("max_player_level")) => Ok(60),
                            ("game_rules", Some("starter_gold")) => Ok(500),
                            ("game_rules", Some("stamina_cap")) => Ok(120),
                            ("game_rules", Some("daily_reset_hour")) => Ok(5),
                            _ => Err("unexpected i64".to_owned()),
                        }}
                    }}

                    fn as_f64(&self) -> Result<f64, String> {{ Ok(self.as_i64()? as f64) }}

                    fn as_bool(&self) -> Result<bool, String> {{
                        if self.path == "game_rules" && self.field == Some("pvp_enabled") {{ Ok(true) }} else {{ Err("unexpected bool".to_owned()) }}
                    }}

                    fn as_string(&self) -> Result<String, String> {{
                        if self.path == "items" && self.field == Some("name") {{ Ok("Potion".to_owned()) }} else {{ Err("unexpected string".to_owned()) }}
                    }}

                    fn is_null(&self) -> bool {{ false }}
                }}

                struct MockLoader;

                impl RuntimeDataLoader for MockLoader {{
                    type Reader = MockReader;

                    fn open(&self, path: &str) -> Result<Self::Reader, String> {{
                        match path {{
                            "items.json" | "items.msgpack" => Ok(MockReader::root("items")),
                            "game_rules.json" | "game_rules.msgpack" => Ok(MockReader::root("game_rules")),
                            other => Err(format!("unexpected path: {{other}}")),
                        }}
                    }}
                }}

                #[test]
                fn generated_code_loads_and_queries_rows() {{
                    let data = GameData::load(&MockLoader).expect("generated data should load");
                    let item = data.items.get_by_id(1).expect("item should be indexed");
                    assert_eq!(item.name, "Potion");
                    assert_eq!(data.items.rows().len(), 1);
                    assert_eq!(data.game_rules.max_player_level, 60);
                    assert!(data.game_rules.pvp_enabled);
                }}

                {serde_assertion}
            "#,
            serde_assertion = if derive_serde {
                "#[test]\nfn generated_rows_support_serde() {\n    let _ = serde_json::to_string(&ItemsRow { id: 1, name: \"Potion\".to_owned(), price: 10, stack: None, attributes: None, skill: None }).unwrap();\n    let _ = serde_json::to_string(&GameRulesRow { max_player_level: 60, starter_gold: 500, stamina_cap: 120, daily_reset_hour: 5, pvp_enabled: true }).unwrap();\n}"
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
    assert_command_success(output, "generated Rust cargo test");
}

fn verify_generated_cpp_runtime(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("compile_check.cpp"),
        r#"
            #include "generated.hpp"

            #include <memory>
            #include <stdexcept>

            class MockReader final : public IRuntimeDataReader {
            public:
                MockReader(std::string path, std::string field = "", std::size_t index = 0)
                    : path_(std::move(path)), field_(std::move(field)), index_(index) {}

                std::unique_ptr<IRuntimeDataReader> Field(const std::string& name) const override {
                    if (name == "stack" || name == "attributes" || name == "skill") {
                        return nullptr;
                    }
                    return std::make_unique<MockReader>(path_, name, index_);
                }

                std::unique_ptr<IRuntimeDataReader> Index(std::size_t index) const override {
                    if (path_ == "items.json" && field_ == "rows" && index == 0) {
                        return std::make_unique<MockReader>(path_, "", index);
                    }
                    throw std::runtime_error("unexpected index");
                }

                std::size_t Size() const override {
                    if (path_ == "items.json" && field_ == "rows") {
                        return 1;
                    }
                    throw std::runtime_error("unexpected size");
                }

                std::int64_t AsInt64() const override {
                    if (path_ == "items.json" && field_ == "id") { return 1; }
                    if (path_ == "items.json" && field_ == "price") { return 10; }
                    if (path_ == "game_rules.json" && field_ == "max_player_level") { return 60; }
                    if (path_ == "game_rules.json" && field_ == "starter_gold") { return 500; }
                    if (path_ == "game_rules.json" && field_ == "stamina_cap") { return 120; }
                    if (path_ == "game_rules.json" && field_ == "daily_reset_hour") { return 5; }
                    throw std::runtime_error("unexpected int64");
                }

                double AsDouble() const override { return static_cast<double>(AsInt64()); }

                bool AsBool() const override {
                    if (path_ == "game_rules.json" && field_ == "pvp_enabled") { return true; }
                    throw std::runtime_error("unexpected bool");
                }

                std::string AsString() const override {
                    if (path_ == "items.json" && field_ == "name") { return "Potion"; }
                    throw std::runtime_error("unexpected string");
                }

                bool IsNull() const override { return false; }
            private:
                std::string path_;
                std::string field_;
                std::size_t index_;
            };

            class MockLoader final : public IRuntimeDataLoader {
            public:
                std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) override {
                    if (path == "items.json" || path == "game_rules.json") {
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
                (void)row;
                GameRulesRow rules{};
                rules.max_player_level = 60;
                rules.starter_gold = 500;
                rules.stamina_cap = 120;
                rules.daily_reset_hour = 5;
                rules.pvp_enabled = true;
                if (std::string(GameRulesRow::DataFile) != "game_rules.json") {
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

fn verify_generated_typescript(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("compile_check.ts"),
        r#"
            import { GameData, RuntimeDataLoader, RuntimeDataReader } from "./generated";

            class MockReader implements RuntimeDataReader {
                constructor(private readonly path: string, private readonly fieldName = "") {}

                field(name: string): RuntimeDataReader | undefined {
                    if (name === "stack" || name === "attributes" || name === "skill") {
                        return undefined;
                    }
                    return new MockReader(this.path, name);
                }

                index(index: number): RuntimeDataReader {
                    if (this.path === "items.json" && this.fieldName === "rows" && index === 0) {
                        return new MockReader(this.path);
                    }
                    throw new Error(`unexpected index: ${index}`);
                }

                length(): number {
                    if (this.path === "items.json" && this.fieldName === "rows") {
                        return 1;
                    }
                    throw new Error("unexpected length");
                }

                asNumber(): number {
                    if (this.path === "items.json" && this.fieldName === "id") { return 1; }
                    if (this.path === "items.json" && this.fieldName === "price") { return 10; }
                    if (this.path === "game_rules.json" && this.fieldName === "max_player_level") { return 60; }
                    if (this.path === "game_rules.json" && this.fieldName === "starter_gold") { return 500; }
                    if (this.path === "game_rules.json" && this.fieldName === "stamina_cap") { return 120; }
                    if (this.path === "game_rules.json" && this.fieldName === "daily_reset_hour") { return 5; }
                    throw new Error("unexpected number");
                }

                asBool(): boolean {
                    if (this.path === "game_rules.json" && this.fieldName === "pvp_enabled") { return true; }
                    throw new Error("unexpected bool");
                }

                asString(): string {
                    if (this.path === "items.json" && this.fieldName === "name") { return "Potion"; }
                    throw new Error("unexpected string");
                }

                isNull(): boolean { return false; }
            }

            const loader: RuntimeDataLoader = {
                open(path: string): RuntimeDataReader {
                    if (path === "items.json" || path === "game_rules.json") {
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
                if (data.gameRules.maxPlayerLevel !== 60 || !data.gameRules.pvpEnabled) {
                    throw new Error("generated TypeScript object table failed");
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

fn verify_standard_generated_rust_runtime(fixture_root: &std::path::Path) {
    let temp_dir = tempfile::tempdir().expect("rust harness temp dir should be created");
    let harness_dir = fixture_root.join("harness/rust");
    fs::copy(
        harness_dir.join("Cargo.toml"),
        temp_dir.path().join("Cargo.toml"),
    )
    .expect("rust harness Cargo.toml should be copied");
    fs::copy(harness_dir.join("lib.rs"), temp_dir.path().join("lib.rs"))
        .expect("rust harness lib.rs should be copied");
    fs::copy(
        fixture_root.join("out/server_code/rust/generated.rs"),
        temp_dir.path().join("generated.rs"),
    )
    .expect("generated Rust code should be copied into harness");

    let output = StdCommand::new("cargo")
        .args(["test", "--quiet", "--manifest-path"])
        .arg(temp_dir.path().join("Cargo.toml"))
        .env("DATA_DIR", fixture_root.join("out/server_data"))
        .output()
        .expect("standard generated Rust runtime test should run");
    assert_command_success(output, "standard generated Rust runtime test");
}

fn verify_standard_generated_cpp_runtime(fixture_root: &std::path::Path) {
    if !command_exists("c++") {
        eprintln!("skipping standard generated C++ runtime test: c++ not found");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("cpp harness temp dir should be created");
    let binary_path = temp_dir.path().join("runtime_check");
    let output = StdCommand::new("c++")
        .args(["-std=c++17"])
        .arg(fixture_root.join("harness/cpp/runtime_check.cpp"))
        .arg(fixture_root.join("out/client_code/cpp/generated.cpp"))
        .arg("-I")
        .arg(fixture_root.join("out/client_code/cpp"))
        .arg("-o")
        .arg(&binary_path)
        .output()
        .expect("standard generated C++ runtime test should compile");
    assert_command_success(output, "standard generated C++ runtime compile");

    let output = StdCommand::new(&binary_path)
        .env("DATA_DIR", fixture_root.join("out/client_data"))
        .output()
        .expect("standard generated C++ runtime test should run");
    assert_command_success(output, "standard generated C++ runtime test");
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
    fs::copy(
        harness_dir.join("tsconfig.json"),
        temp_dir.path().join("tsconfig.json"),
    )
    .expect("TypeScript tsconfig should be copied");
    fs::copy(
        fixture_root.join("out/client_code/typescript/generated.ts"),
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

    let output = StdCommand::new("node")
        .arg(temp_dir.path().join("dist/runtime_check.js"))
        .env("DATA_DIR", fixture_root.join("out/client_data"))
        .output()
        .expect("standard generated TypeScript runtime test should run");
    assert_command_success(output, "standard generated TypeScript runtime test");
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
