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

#[test]
fn diagnostic_asset_regex_warning_reaches_cli_and_report() {
    let temp = tempfile::tempdir().unwrap();
    let project = diagnostic_project(temp.path(), "warning", false);
    let config = fs::read_to_string(&project).unwrap();
    let config = config
        .replace(
            "name = \"level\"\n        type = \"int\"",
            "name = \"level\"\n        type = \"asset\"\n        asset_kind = \"icon\"",
        )
        .replace(
            "kind = \"range\"\n        max = 10",
            "kind = \"regex\"\n        pattern = \"Icon_.*\"",
        );
    fs::write(
        &project,
        format!(
            r#"
        [assets]
        require_existing = false
        [[assets.search_paths]]
        name = "local"
        root = "."
        [[asset_kinds]]
        name = "icon"
        search_paths = ["local"]
        [[asset_kinds.regex_rules]]
        name = "preferred_name"
        pattern = "Icon_.*"
        severity = "warning"
        {config}
    "#
        ),
    )
    .unwrap();
    let report_path = temp.path().join("report.json");
    let result = diagnostic_run(
        &project,
        &[
            "--report",
            "json",
            "--report-output",
            report_path.to_str().unwrap(),
        ],
    );
    assert_eq!(result["status"], "success", "{result}");
    let issues = result["diagnostics"].as_array().unwrap();
    let asset = issues
        .iter()
        .find(|issue| issue["source"] == "asset.resolver")
        .unwrap();
    assert_eq!(asset["severity"], "warning");
    assert_eq!(asset["location"]["source_row_number"], 3);
    assert_eq!(asset["context"]["asset_kind"], "icon");
    assert_ne!(asset["target"], "11");
    let regex = issues
        .iter()
        .find(|issue| issue["code"] == "validator.regex_mismatch")
        .unwrap();
    assert_eq!(regex["current_value"], "11");
    assert!(regex["expected"].as_str().unwrap().contains("Icon_"));
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(report_path).unwrap()).unwrap();
    assert_eq!(report["summary"]["warning_count"], 2);
}

#[test]
fn diagnostic_imported_duplicate_declarations_have_both_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let source = "[[tables]]\nname = 'items'\nsource = 'items.csv'\n";
    fs::write(root.join("a.toml"), source).unwrap();
    fs::write(root.join("b.toml"), source).unwrap();
    let project = root.join("ConfigForge.toml");
    fs::write(&project, "imports = ['a.toml', 'b.toml']\n[[targets]]\nname = 'client'\n[[targets.exports]]\ntype = 'rust'\noutput = 'out'\n").unwrap();
    let result = diagnostic_run(&project, &[]);
    let issue = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| issue["code"] == "CF_META_DUPLICATE_TABLE_NAME")
        .unwrap();
    assert!(
        issue["location"]["source_path"]
            .as_str()
            .unwrap()
            .ends_with("b.toml")
    );
    assert_eq!(issue["location"]["source_row_number"], 2);
    assert!(
        issue["related_locations"][0]["source_path"]
            .as_str()
            .unwrap()
            .ends_with("a.toml")
    );
}

#[test]
fn diagnostic_duplicate_rows_have_related_cell_and_record_key() {
    let temp = tempfile::tempdir().unwrap();
    let project = diagnostic_project(temp.path(), "warning", false);
    let config = fs::read_to_string(&project)
        .unwrap()
        .replace("type = \"int\"", "type = \"int\"\n        unique = true");
    let config = config.replace("source = \"items.csv\"", "source = \"items.csv\"\n        primary_index = \"by_level\"\n        [[tables.indexes]]\n        name = \"by_level\"\n        fields = [\"level\"]\n        unique = true");
    fs::write(&project, config).unwrap();
    fs::write(temp.path().join("items.csv"), "level\n等级\n11\n11\n").unwrap();
    let result = diagnostic_run(&project, &[]);
    let issue = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| issue["code"] == "CF_DATA_DUPLICATE_KEY")
        .unwrap();
    assert_eq!(issue["location"]["source_row_number"], 4);
    assert_eq!(issue["related_locations"][0]["source_row_number"], 3);
    assert_eq!(issue["related_locations"][0]["source_column_name"], "level");
    assert_eq!(issue["record_key"], "level=11");
}

#[test]
fn diagnostic_invalid_imported_field_points_to_its_declaration() {
    let temp = tempfile::tempdir().unwrap();
    let project = diagnostic_project(temp.path(), "warning", false);
    let config = fs::read_to_string(&project)
        .unwrap()
        .replace("type = \"int\"", "type = \"missing_type\"");
    fs::write(&project, config).unwrap();
    let result = diagnostic_run(&project, &[]);
    let issue = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| issue["code"] == "CF_META_UNKNOWN_FIELD_TYPE")
        .unwrap();
    assert_eq!(issue["location"]["table"], "items");
    assert_eq!(issue["location"]["field"], "level");
    assert!(issue["location"]["source_row_number"].as_u64().unwrap() > 5);
    assert!(
        issue["location"]["source_path"]
            .as_str()
            .unwrap()
            .ends_with("ConfigForge.toml")
    );
}
