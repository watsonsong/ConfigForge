// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use assert_cmd::Command;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn diagnostic_project(root: &Path, severity: &str, reports: bool) -> PathBuf {
    fs::write(root.join("items.csv"), "level\n等级\n11\n").unwrap();
    let report_tasks = if reports {
        r#"
        [[targets.exports]]
        name = "broken-report"
        type = "validation_report"
        output = "items.csv"
        format = "json"
        [[targets.exports]]
        name = "good-report"
        type = "validation_report"
        output = "report"
        format = "json"
        "#
    } else {
        ""
    };
    let path = root.join("ConfigForge.toml");
    fs::write(
        &path,
        format!(
            r#"
        [project]
        input = "."
        [[tables]]
        name = "items"
        source = "items.csv"
        [[tables.fields]]
        name = "level"
        type = "int"
        [[tables.fields.validators]]
        kind = "range"
        max = 10
        severity = "{severity}"
        [[targets]]
        name = "client"
        [[targets.exports]]
        type = "json"
        output = "out"
        {report_tasks}
    "#
        ),
    )
    .unwrap();
    path
}

fn diagnostic_run(project: &Path, args: &[&str]) -> serde_json::Value {
    let output = Command::cargo_bin("config-forge")
        .unwrap()
        .args([
            "--project",
            project.to_str().unwrap(),
            "--target",
            "client",
            "--diagnostic-format",
            "json",
        ])
        .args(args)
        .output()
        .unwrap();
    serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)))
}

#[test]
fn diagnostic_report_failure_preserves_validation_and_attempts_other_reports() {
    let temp = tempfile::tempdir().unwrap();
    let project = diagnostic_project(temp.path(), "error", true);
    let result = diagnostic_run(&project, &[]);
    assert_eq!(result["status"], "error");
    let issues = result["diagnostics"].as_array().unwrap();
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "field.range_violation")
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "CF_IO_WRITE_FAILED")
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "CF_ARTIFACT_WRITE_STATE")
    );
}

#[test]
fn diagnostic_trace_failure_preserves_primary_error() {
    let temp = tempfile::tempdir().unwrap();
    let project = diagnostic_project(temp.path(), "error", false);
    let trace = temp.path().join("items.csv/trace.json");
    let result = diagnostic_run(&project, &["--trace-tasks", trace.to_str().unwrap()]);
    let issues = result["diagnostics"].as_array().unwrap();
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0]["code"], "field.range_violation");
    assert_eq!(issues[1]["code"], "CF_IO_WRITE_FAILED");
}

#[test]
fn diagnostic_warning_survives_export_failure_and_is_json_on_success() {
    let temp = tempfile::tempdir().unwrap();
    let project = diagnostic_project(temp.path(), "warning", false);
    let success = diagnostic_run(&project, &[]);
    assert_eq!(success["schema_version"], 1);
    assert_eq!(success["status"], "success");
    assert_eq!(success["diagnostics"][0]["severity"], "warning");
    fs::remove_file(temp.path().join("out/items.json")).unwrap();
    fs::write(temp.path().join("blocked"), "file").unwrap();
    let config = fs::read_to_string(&project)
        .unwrap()
        .replace("output = \"out\"", "output = \"blocked\"");
    fs::write(&project, config).unwrap();
    let failure = diagnostic_run(&project, &[]);
    let issues = failure["diagnostics"].as_array().unwrap();
    assert!(issues.iter().any(|issue| issue["severity"] == "warning"));
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "CF_IO_WRITE_FAILED")
    );
}

#[test]
fn diagnostic_io_parse_and_argument_errors_are_structured() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing.toml");
    let result = diagnostic_run(&missing, &[]);
    assert_eq!(result["diagnostics"][0]["code"], "CF_IO_READ_FAILED");
    assert_eq!(
        result["diagnostics"][0]["location"]["source_path"],
        missing.to_str().unwrap()
    );
    fs::write(&missing, "[project]\nname = @\n").unwrap();
    let result = diagnostic_run(&missing, &[]);
    assert_eq!(result["diagnostics"][0]["code"], "CF_PROJECT_PARSE_FAILED");
    assert_eq!(result["diagnostics"][0]["location"]["source_row_number"], 2);
    let output = Command::cargo_bin("config-forge")
        .unwrap()
        .args(["--diagnostic-format=json", "--unknown"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let result: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(result["diagnostics"][0]["code"], "CF_BUILD_INVALID_OPTION");
}
