// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::*;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn all_builtin_generators_share_a_complete_target_plan_in_all_modes() {
    let temp = tempfile::tempdir().expect("temp dir");
    fs::create_dir(temp.path().join("data")).unwrap();
    fs::write(temp.path().join("data/items.csv"), "id\n1\n").unwrap();
    let mut config = String::from(
        r#"
        [project]
        input = "data"
        [[tables]]
        name = "items"
        source = "items.csv"
        [[tables.fields]]
        name = "id"
        type = "int"
        [[targets]]
        name = "all"
    "#,
    );
    for type_id in [
        "rust",
        "cpp",
        "cpp_unreal",
        "typescript",
        "csharp",
        "java",
        "json",
        "messagepack",
        "bin",
        "gettext_pot",
        "lua_table",
        "asset_manifest",
        "schema_doc",
        "validation_report",
    ] {
        config.push_str(&format!(
            r#"
            [[targets.exports]]
            type = "{type_id}"
            output = "out/{type_id}"
        "#
        ));
        if matches!(type_id, "rust" | "cpp") {
            config.push_str(&format!(r#"
                {type_id} = {{ runtime_interface = "external", runtime_interface_emit = "reference" }}
            "#));
        }
    }
    let project = temp.path().join("ConfigForge.toml");
    fs::write(&project, config).unwrap();
    let project = project.to_str().unwrap();
    let out = temp
        .path()
        .canonicalize()
        .expect("canonical project root")
        .join("out");
    let dry_run = run_config_forge_stdout(&[
        "--project",
        project,
        "--target",
        "all",
        "--dry-run",
        "--jobs",
        "4",
    ]);
    assert!(!out.exists());
    let planned_paths = dry_run_artifact_entries(&dry_run, &out)
        .into_iter()
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    for mode in ["direct", "temp-commit"] {
        run_config_forge_success(&[
            "--project",
            project,
            "--target",
            "all",
            "--write-mode",
            mode,
            "--jobs",
            "4",
        ]);
        let actual = collect_file_tree(&out);
        assert_eq!(actual.keys().cloned().collect::<Vec<_>>(), planned_paths);
        assert!(actual.contains_key("rust/config_forge_runtime_interface.json"));
        assert!(actual.contains_key("cpp/generated.cpp"));
        assert!(actual.contains_key("cpp_unreal/generated.cpp"));
        assert!(actual.contains_key("json/items.json"));
    }
}

#[test]
fn target_planning_rejects_cross_branch_and_auxiliary_collisions_before_writing() {
    let cases = [
        (
            "items.json",
            r#"
            [[targets.exports]]
            type = "json"
            output = "out/collision"
            [[targets.exports]]
            type = "rust"
            output = "out/collision"
            file_name = "items.json"
        "#,
        ),
        (
            "generated.rs",
            r#"
            [[targets.exports]]
            name = "first-rust"
            type = "rust"
            output = "out/collision"
            [[targets.exports]]
            name = "second-rust"
            type = "rust"
            output = "out/collision"
        "#,
        ),
        (
            "generated.cpp",
            r#"
            [[targets.exports]]
            type = "cpp"
            output = "out/collision"
            [[targets.exports]]
            type = "rust"
            output = "out/collision"
            file_name = "generated.cpp"
        "#,
        ),
        (
            "config_forge_runtime_interface.json",
            r#"
            [[targets.exports]]
            type = "rust"
            output = "out/collision"
            rust = { runtime_interface = "external", runtime_interface_emit = "reference" }
            [[targets.exports]]
            type = "cpp"
            output = "out/collision"
            cpp = { runtime_interface = "external", runtime_interface_emit = "reference" }
        "#,
        ),
    ];
    for (file_name, exports) in cases {
        for mode in ["direct", "temp-commit", "dry-run"] {
            for jobs in ["1", "4"] {
                let temp = tempfile::tempdir().expect("temp dir");
                let out = temp.path().join("out");
                fs::create_dir(temp.path().join("data")).unwrap();
                fs::write(temp.path().join("data/items.csv"), "id\n1\n").unwrap();
                fs::create_dir_all(out.join("collision")).unwrap();
                let existing = out.join("collision").join(file_name);
                fs::write(&existing, "original artifact").unwrap();
                let before = collect_file_tree(&out);
                let project = temp.path().join("ConfigForge.toml");
                fs::write(
                    &project,
                    format!(
                        r#"
                    [project]
                    input = "data"
                    [[tables]]
                    name = "items"
                    source = "items.csv"
                    [[tables.fields]]
                    name = "id"
                    type = "int"
                    [[targets]]
                    name = "client"
                    tags = ["client"]
                    [[targets.exports]]
                    type = "typescript"
                    output = "out/unrelated"
                    {exports}
                "#
                    ),
                )
                .unwrap();
                let mut command = Command::cargo_bin("config-forge").unwrap();
                command.args([
                    "--project",
                    project.to_str().unwrap(),
                    "--target",
                    "client",
                    "--jobs",
                    jobs,
                ]);
                if mode == "dry-run" {
                    command.arg("--dry-run");
                } else {
                    command.args(["--write-mode", mode]);
                }
                command
                    .assert()
                    .failure()
                    .stderr(predicate::str::contains("multiple artifacts"));
                assert_eq!(
                    collect_file_tree(&out),
                    before,
                    "{file_name}, {mode}, jobs={jobs}"
                );
                assert!(!out.join("unrelated").exists());
            }
        }
    }
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
            toml_path(&data_out_dir),
            toml_path(&rust_out_dir)
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
    ]);

    assert_eq!(serial_stdout, parallel_stdout);
    assert!(serial_stdout.contains("artifact plan (9 entries):"));
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
    ]);
    let dry_run_stdout = run_config_forge_stdout(&[
        "--project",
        dry_run_project.to_str().expect("path should be utf8"),
        "--target",
        "client",
        "--dry-run",
        "--jobs",
        "4",
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
            toml_path(&data_out_dir),
            toml_path(&rust_out_dir)
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
        .stdout(predicate::str::contains("artifact plan (9 entries):"))
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
            toml_path(&out_dir)
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
            toml_path(&out_dir)
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
            toml_path(&data_out_dir),
            toml_path(&rust_out_dir)
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
