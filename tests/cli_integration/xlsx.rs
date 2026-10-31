// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::Path};

use assert_cmd::Command;
use predicates::prelude::*;

fn prepare(root: &Path) {
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/xlsx/sources.xlsx"),
        root.join("sources.xlsx"),
    )
    .unwrap();
}

fn write_project(root: &Path, source: &str) {
    fs::write(
        root.join("ConfigForge.toml"),
        format!(
            r#"
[project]
input = "."
[[tables]]
name = "items"
source = {source}
[[tables.fields]]
name = "id"
type = "int"
[[tables.fields]]
name = "name"
type = "string"
[[tables.fields]]
name = "enabled"
type = "bool"
[[tables.fields]]
name = "amount"
type = "float"
[[tables.fields]]
name = "computed"
type = "int"
[[tables.fields]]
name = "empty"
type = "string"
[[targets]]
name = "client"
[[targets.exports]]
type = "json"
output = "out"
"#
        ),
    )
    .unwrap();
}

fn command(root: &Path) -> Command {
    let mut command = Command::cargo_bin("config-forge").unwrap();
    command
        .arg("--project")
        .arg(root.join("ConfigForge.toml"))
        .args(["--target", "client"]);
    command
}

#[test]
fn xlsx_cli_exports_the_same_payload_as_csv_in_all_write_modes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    prepare(root);
    fs::write(root.join("items.csv"), "id,name,enabled,amount,computed,empty\nID,Name,Enabled,Amount,Computed,Empty\n1,0012,true,12.5,3,\n2,药水,false,20,4,\n").unwrap();
    write_project(root, r#""items.csv""#);
    command(root).assert().success();
    let expected = fs::read(root.join("out/items.json")).unwrap();
    write_project(
        root,
        r#"{ provider = "xlsx", path = "sources.xlsx", sheet = "物品", header_row = 3, data_start_row = 5 }"#,
    );
    for mode in ["direct", "temp-commit", "dry-run"] {
        fs::remove_dir_all(root.join("out")).unwrap();
        let mut cmd = command(root);
        if mode == "dry-run" {
            cmd.arg("--dry-run");
        } else {
            cmd.args(["--write-mode", mode]);
        }
        cmd.assert().success();
        if mode == "dry-run" {
            assert!(!root.join("out/items.json").exists());
        } else {
            assert_eq!(expected, fs::read(root.join("out/items.json")).unwrap());
        }
    }
}

#[test]
fn xlsx_object_builtin_validation_uses_the_value_cell_after_mixed_merge() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    prepare(root);
    fs::write(root.join("base.csv"), "key,value\nother,1\n").unwrap();
    fs::write(root.join("ConfigForge.toml"), r#"
[project]
input = "."
[[tables]]
name = "settings"
kind = "object"
sources = [
  { provider = "csv", path = "base.csv" },
  { provider = "xlsx", path = "sources.xlsx", sheet = "settings", header_row = 2, data_start_row = 4 },
]
[[tables.fields]]
name = "other"
type = "int"
[[tables.fields]]
name = "limit"
type = "int"
[[tables.fields.validators]]
kind = "range"
max = 5
[[tables.fields]]
name = "title"
type = "string"
[[targets]]
name = "client"
[[targets.exports]]
type = "json"
output = "out"
"#).unwrap();
    let output = command(root)
        .args(["--diagnostic-format", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    let location = &diagnostics["diagnostics"][0]["location"];
    assert_eq!(location["source_sheet"], "settings");
    assert_eq!(location["source_row_number"], 4);
    assert_eq!(location["source_column_number"], 4);
}

#[test]
fn xlsx_cli_type_errors_include_sheet_and_physical_cell_in_all_reports() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    prepare(root);
    fs::write(root.join("ConfigForge.toml"), r#"
[project]
input = "."
[[tables]]
name = "items"
source = { provider = "xlsx", path = "sources.xlsx", sheet = "invalid", header_row = 3, data_start_row = 5 }
[[tables.fields]]
name = "id"
type = "int"
[[targets]]
name = "client"
[[targets.exports]]
type = "json"
output = "out"
"#).unwrap();
    for format in ["json", "html", "markdown", "csv"] {
        let report = root.join(format!("report.{format}"));
        let output = command(root)
            .args([
                "--diagnostic-format",
                "json",
                "--report",
                format,
                "--report-output",
            ])
            .arg(&report)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        let location = &diagnostics["diagnostics"][0]["location"];
        assert_eq!(location["source_sheet"], "invalid");
        assert_eq!(location["source_row_number"], 5);
        assert_eq!(location["source_column_number"], 2);
        let text = fs::read_to_string(&report).unwrap();
        assert!(text.contains("invalid"), "{text}");
        assert!(text.contains("sources.xlsx"), "{text}");
        if format == "json" {
            let json: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(json["issues"][0]["source_sheet"], "invalid");
            assert_eq!(json["issues"][0]["source_row_number"], 5);
        }
    }
    command(root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("sheet `invalid`").and(predicate::str::contains("row 5")));
}

#[test]
fn xlsx_cli_mixes_variant_sources_and_skips_inactive_overrides() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    prepare(root);
    fs::write(root.join("base.csv"), "id,name\nID,Name\n2,csv\n").unwrap();
    fs::write(root.join("ConfigForge.toml"), r#"
[project]
input = "."
[[tables]]
name = "items"
polymorphic = true
primary_index = "by_id"
sources = [
  { provider = "csv", path = "base.csv", fragments = ["base"] },
  { provider = "xlsx", path = "sources.xlsx", sheet = "override", fragments = ["extra"] },
]
override_sources = [{ provider = "xlsx", path = "absent.xlsx", sheet = "absent", export_when = { all = ["inactive"] } }]
[[tables.fields]]
name = "id"
type = "int"
[[tables.fields]]
name = "name"
type = "string"
[[tables.fragments]]
name = "base"
[[tables.fragments.fields]]
name = "comment"
type = "string"
[[tables.fragments]]
name = "extra"
[[tables.fragments.fields]]
name = "comment"
type = "string"
[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
[[targets]]
name = "client"
[[targets.exports]]
type = "json"
output = "out"
"#).unwrap();
    command(root).assert().success();
    let payload: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("out/items.json")).unwrap()).unwrap();
    assert_eq!(payload["rows"].as_array().unwrap().len(), 2);

    // Both sources use physical row 3; the failing FK must retain the XLSX origin.
    let project = fs::read_to_string(root.join("ConfigForge.toml"))
        .unwrap()
        .replace(
            "type = \"int\"",
            "type = \"int\"\nforeign_key = { target_table = \"allowed\" }",
        )
        .replace(
            "[[targets]]",
            r#"
[[tables]]
name = "allowed"
source = "base.csv"
primary_index = "by_id"
[[tables.fields]]
name = "id"
type = "int"
[[tables.fields]]
name = "name"
type = "string"
[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
[[targets]]"#,
        );
    fs::write(root.join("ConfigForge.toml"), project).unwrap();
    let output = command(root)
        .args(["--diagnostic-format", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(
        diagnostics["diagnostics"][0]["code"],
        "CF_DATA_INVALID_FOREIGN_KEY"
    );
    let location = &diagnostics["diagnostics"][0]["location"];
    assert_eq!(location["source_sheet"], "override");
    assert_eq!(location["source_row_number"], 3);
    assert_eq!(location["source_column_number"], 1);
}
