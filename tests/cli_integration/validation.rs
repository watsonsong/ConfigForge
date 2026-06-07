// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn range_validator_blocks_invalid_data_before_export() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(data_dir.join("items.csv"), "level\n等级\n11\n")
        .expect("items data should be written");

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
    fs::write(data_dir.join("items.csv"), "level\n等级\n11\n")
        .expect("items data should be written");

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

#[test]
fn cli_report_json_is_written_when_validation_fails() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    let report_path = temp_dir.path().join("out/report/validation.json");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(data_dir.join("items.csv"), "level\n等级\n11\n")
        .expect("items data should be written");

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
        "--report",
        "json",
        "--output",
        report_path.to_str().expect("path should be utf8"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("exceeds max `10`"));
    assert!(report_path.exists());
    assert!(!data_out_dir.join("items.json").exists());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("report should be readable"))
            .expect("report should be valid JSON");
    assert_eq!(report["summary"]["error_count"], 1);
    assert_eq!(report["issues"][0]["id"], "E0001");
    assert_eq!(report["issues"][0]["friendly_title"], "数值超出允许范围");
}

#[test]
fn build_validation_report_export_writes_html_artifact() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    let report_dir = temp_dir.path().join("out/report");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(data_dir.join("items.csv"), "level\n等级\n11\n")
        .expect("items data should be written");

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

                [[targets.exports]]
                name = "designer-report"
                output = "{}"
                type = "validation_report"
                format = "html"
                file_name = "client-validation.html"
                title = "Client Validation"
            "#,
            data_dir.display(),
            data_out_dir.display(),
            report_dir.display()
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
    let report_path = report_dir.join("client-validation.html");
    let html = fs::read_to_string(report_path).expect("HTML report should be readable");
    assert!(html.contains("Client Validation"));
    assert!(html.contains("PASS WITH WARNINGS"));
    assert!(html.contains("field.range_violation"));
}
