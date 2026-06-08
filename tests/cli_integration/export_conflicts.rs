// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::*;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

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
fn rejects_artifact_output_that_escapes_project_root() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let outside_out_dir = temp_dir.path().join("../outside-config-forge-artifacts");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                input = "{}"

                [[targets]]
                name = "escape"

                [[targets.exports]]
                name = "json"
                output = "{}"
                type = "json"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            outside_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "escape",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("escapes output root"));
    assert!(!outside_out_dir.join("items.json").exists());
    assert!(!outside_out_dir.join("game_rules.json").exists());
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
