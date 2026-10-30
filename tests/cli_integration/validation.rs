// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::toml_path;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn stderr_json(output: std::process::Output) -> serde_json::Value {
    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    serde_json::from_str(&stderr).expect("stderr should be diagnostic JSON")
}

#[test]
fn report_option_dependencies_preserve_human_and_json_diagnostics() {
    let cases: &[(&[&str], &str)] = &[
        (&["--report", "json"], "--report-output must be set"),
        (&["--report-output", "report.json"], "require --report"),
        (&["--report-title", "QA report"], "require --report"),
    ];
    for (args, expected) in cases {
        for format in ["human", "json"] {
            let output = Command::cargo_bin("config-forge")
                .expect("binary should build")
                .args(["--project", "unused.toml", "--target", "client"])
                .args(*args)
                .args(["--diagnostic-format", format])
                .output()
                .expect("config-forge should run");
            assert!(!output.status.success());
            if format == "json" {
                let json = stderr_json(output);
                assert_eq!(json["diagnostics"][0]["code"], "CF_BUILD_INVALID_OPTION");
                assert_eq!(json["diagnostics"][0]["source"], "cli.options");
                assert!(json["message"].as_str().unwrap().contains(expected));
            } else {
                let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
                assert!(stderr.contains(expected), "{stderr}");
            }
        }
    }
}

#[test]
fn cli_report_formats_respect_write_modes_and_project_output() {
    for format in ["html", "markdown", "json", "csv"] {
        for mode in ["direct", "temp-commit", "dry-run"] {
            let temp_dir = tempfile::tempdir().expect("temp dir should be created");
            let data_dir = temp_dir.path().join("data");
            let project_path = temp_dir.path().join("ConfigForge.toml");
            let report_path = temp_dir.path().join("reports/validation.result");
            let trace_path = temp_dir.path().join("trace.json");
            let data_out_dir = temp_dir.path().join("artifacts");
            fs::create_dir(&data_dir).expect("data dir should be created");
            fs::write(data_dir.join("items.csv"), "level\n等级\n10\n")
                .expect("data should be written");
            fs::write(
                &project_path,
                r#"
                    [project]
                    input = "data"

                    [[tables]]
                    name = "items"
                    source = "items.csv"

                    [[tables.fields]]
                    name = "level"
                    type = "int"

                    [[targets]]
                    name = "client"

                    [[targets.exports]]
                    type = "json"
                    output = "artifacts"
                "#,
            )
            .expect("project config should be written");
            let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
            cmd.arg("--project")
                .arg(&project_path)
                .args(["--target", "client", "--report", format, "--report-output"])
                .arg(&report_path)
                .args(["--report-title", "QA report", "--trace-tasks"])
                .arg(&trace_path);
            if mode == "dry-run" {
                cmd.arg("--dry-run");
            } else {
                cmd.args(["--write-mode", mode]);
            }
            cmd.assert().success();
            assert!(
                trace_path.is_file(),
                "explicit trace must also be written in dry-run"
            );
            if mode == "dry-run" {
                assert!(!report_path.parent().unwrap().exists());
                assert!(!data_out_dir.exists());
            } else {
                assert!(data_out_dir.join("items.json").is_file());
                assert!(!report_path.parent().unwrap().join("items.json").exists());
                let report = fs::read_to_string(&report_path).expect("report should be readable");
                assert!(!report.is_empty());
                if format != "csv" {
                    assert!(report.contains("QA report"));
                }
            }
        }
    }
}

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
            toml_path(&data_dir),
            toml_path(&data_out_dir)
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
fn cli_diagnostic_format_json_emits_machine_readable_diagnostics() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(data_dir.join("items.csv"), "level\n等级\nabc\n")
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

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"
            "#,
            toml_path(&data_dir),
            toml_path(&data_out_dir)
        ),
    )
    .expect("project config should be written");

    let output = Command::cargo_bin("config-forge")
        .expect("binary should build")
        .args([
            "--project",
            project_path.to_str().expect("path should be utf8"),
            "--target",
            "client",
            "--diagnostic-format",
            "json",
        ])
        .output()
        .expect("config-forge should run");
    assert!(!output.status.success(), "command should fail");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let diagnostic: serde_json::Value =
        serde_json::from_str(&stderr).expect("stderr should be diagnostic JSON");
    assert_eq!(diagnostic["status"], "error");
    assert_eq!(diagnostic["diagnostics"][0]["code"], "CF_DATA_INVALID_TYPE");
    assert_eq!(diagnostic["diagnostics"][0]["severity"], "error");
    assert!(diagnostic["diagnostics"][0].get("target").is_some());
    assert_eq!(diagnostic["diagnostics"][0]["current_value"], "abc");
    assert_eq!(diagnostic["diagnostics"][0]["expected"], "int");
    assert_eq!(diagnostic["diagnostics"][0]["location"]["table"], "items");
    assert_eq!(diagnostic["diagnostics"][0]["location"]["field"], "level");
    assert_eq!(
        diagnostic["diagnostics"][0]["location"]["source_column_name"],
        "level"
    );
}

#[test]
fn duplicate_csv_header_json_diagnostic_has_stable_code_and_source_context() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(data_dir.join("items.csv"), "id,id\nID,ID\n1,2\n")
        .expect("items data should be written");

    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                [project]
                input = "{}"

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
                output = "{}"
                type = "json"
            "#,
            toml_path(&data_dir),
            toml_path(&data_out_dir)
        ),
    )
    .expect("project config should be written");

    let output = Command::cargo_bin("config-forge")
        .expect("binary should build")
        .args([
            "--project",
            project_path.to_str().expect("path should be utf8"),
            "--target",
            "client",
            "--diagnostic-format",
            "json",
        ])
        .output()
        .expect("config-forge should run");

    let json = stderr_json(output);
    let issue = &json["diagnostics"][0];
    assert_eq!(issue["code"], "CF_DATA_DUPLICATE_HEADER");
    assert_eq!(issue["severity"], "error");
    assert_eq!(issue["source"], "data.csv");
    assert_eq!(issue["domain"], "data");
    assert_eq!(issue["current_value"], "id");
    assert_eq!(issue["expected"], "unique CSV header name");
    assert_eq!(issue["location"]["source_row_number"], 1);
    assert_eq!(issue["location"]["source_column_name"], "id");
}

#[test]
fn source_path_escape_json_diagnostic_has_stable_code_and_table_context() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(temp_dir.path().join("outside.csv"), "id\nID\n1\n")
        .expect("outside data should be written");

    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                [project]
                input = "{}"

                [[tables]]
                name = "items"
                source = "../outside.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"
            "#,
            toml_path(&data_dir),
            toml_path(&data_out_dir)
        ),
    )
    .expect("project config should be written");

    let output = Command::cargo_bin("config-forge")
        .expect("binary should build")
        .args([
            "--project",
            project_path.to_str().expect("path should be utf8"),
            "--target",
            "client",
            "--diagnostic-format",
            "json",
        ])
        .output()
        .expect("config-forge should run");

    let json = stderr_json(output);
    let issue = &json["diagnostics"][0];
    assert_eq!(issue["code"], "CF_DATA_SOURCE_PATH_ESCAPE");
    assert_eq!(issue["severity"], "error");
    assert_eq!(issue["source"], "data.loader");
    assert_eq!(issue["domain"], "data");
    assert_eq!(issue["current_value"], "../outside.csv");
    assert_eq!(issue["location"]["table"], "items");
}

#[test]
fn asset_missing_json_diagnostic_has_stable_code_and_value_context() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let asset_dir = temp_dir.path().join("assets");
    let data_out_dir = temp_dir.path().join("out/data");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::create_dir_all(&asset_dir).expect("asset dir should be created");
    fs::write(
        data_dir.join("items.csv"),
        "id,icon\n编号,图标\n1,Missing.png\n",
    )
    .expect("items data should be written");

    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                [project]
                name = "asset_project"
                input = "{}"

                [assets]
                require_existing = true

                [[assets.search_paths]]
                name = "game"
                root = "{}"

                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"

                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"
            "#,
            toml_path(&data_dir),
            toml_path(&asset_dir),
            toml_path(&data_out_dir)
        ),
    )
    .expect("project config should be written");

    let output = Command::cargo_bin("config-forge")
        .expect("binary should build")
        .args([
            "--project",
            project_path.to_str().expect("path should be utf8"),
            "--target",
            "client",
            "--diagnostic-format",
            "json",
        ])
        .output()
        .expect("config-forge should run");
    assert!(!output.status.success(), "command should fail");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let diagnostic: serde_json::Value =
        serde_json::from_str(&stderr).expect("stderr should be diagnostic JSON");
    let issue = &diagnostic["diagnostics"][0];
    assert_eq!(issue["code"], "CF_DATA_INVALID_ASSET_REF");
    assert_eq!(issue["severity"], "error");
    assert_eq!(issue["source"], "asset.resolver");
    assert_eq!(issue["target"], "client");
    assert_eq!(issue["current_value"], "Missing.png");
    assert_eq!(issue["expected"], "existing asset file");
    assert_eq!(issue["domain"], "data");
    assert_eq!(issue["location"]["table"], "items");
    assert_eq!(issue["location"]["field"], "icon");
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
            toml_path(&data_dir),
            toml_path(&data_out_dir)
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
            toml_path(&data_dir),
            toml_path(&data_out_dir)
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
        "--report-output",
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
    assert_eq!(report["metadata"]["project_name"], "validator_project");
    assert_eq!(report["metadata"]["target"], "client");
}

#[test]
fn cli_report_json_is_written_when_validation_succeeds() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_dir = temp_dir.path().join("data");
    let data_out_dir = temp_dir.path().join("out/data");
    let report_path = temp_dir.path().join("out/report/validation.json");
    fs::create_dir_all(&data_dir).expect("data dir should be created");
    fs::write(data_dir.join("items.csv"), "level\n等级\n10\n")
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
            toml_path(&data_dir),
            toml_path(&data_out_dir)
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
        "--report-output",
        report_path.to_str().expect("path should be utf8"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client` built"));
    assert!(data_out_dir.join("items.json").exists());
    assert!(report_path.exists());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("report should be readable"))
            .expect("report should be valid JSON");
    assert_eq!(report["summary"]["error_count"], 0);
    assert_eq!(report["metadata"]["project_name"], "validator_project");
    assert_eq!(report["metadata"]["target"], "client");
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
            toml_path(&data_dir),
            toml_path(&data_out_dir),
            toml_path(&report_dir)
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

#[test]
fn build_validation_report_export_writes_json_artifact_when_validation_fails() {
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
                format = "json"
                file_name = "client-validation.json"
            "#,
            toml_path(&data_dir),
            toml_path(&data_out_dir),
            toml_path(&report_dir)
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
    let report_path = report_dir.join("client-validation.json");
    assert!(report_path.exists());
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(report_path).expect("report should be readable"))
            .expect("report should be valid JSON");
    assert_eq!(report["summary"]["error_count"], 1);
    assert_eq!(report["metadata"]["project_name"], "validator_project");
    assert_eq!(report["metadata"]["target"], "client");
}
