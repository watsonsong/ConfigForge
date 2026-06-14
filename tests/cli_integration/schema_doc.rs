// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn schema_doc_export_writes_index_and_table_markdown_without_data_input() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let docs_dir = temp_dir.path().join("out/docs");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                [project]
                name = "schema_project"

                [[asset_kinds]]
                name = "ui_icon"
                extensions = [".png"]
                name_prefixes = ["UI_"]

                [[tables]]
                name = "classes"
                source = "classes.csv"
                desc = "职业表"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables]]
                name = "items"
                source = "items.csv"
                desc = "道具表"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
                desc = "道具 ID"

                [[tables.fields]]
                name = "name"
                type = "text"
                required = true
                export_when = {{ any = ["client"] }}
                desc = "道具名"

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"
                export_when = {{ any = ["client"] }}
                desc = "图标资源"

                [[tables.fields]]
                name = "server_note"
                type = "string"
                export_when = {{ any = ["server"] }}

                [[tables.fields]]
                name = "class_id"
                type = "int"
                foreign_key = {{ target_table = "classes", target_index = "primary", linked_field = "class" }}

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables.fields.validators]]
                kind = "range"
                min = 1
                max = 999

                [[targets]]
                name = "client_docs"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "schema_doc"
                format = "markdown"
                title = "Client Schema"
            "#,
            docs_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client_docs",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client_docs` built"));

    let index = fs::read_to_string(docs_dir.join("index.md")).expect("index should be readable");
    let items =
        fs::read_to_string(docs_dir.join("items.md")).expect("items doc should be readable");
    assert!(index.contains("# Client Schema"));
    assert!(index.contains("[items](items.md)"));
    assert!(items.contains("| name | text | yes | - | client | 道具名 |"));
    assert!(!items.contains("server_note"));
    assert!(items.contains("| class_id | classes.id | many-to-one | yes | class | all | - |"));
    assert!(items.contains("## Assets"));
    assert!(items.contains("## Text / POT"));
    assert!(items.contains("range(max=999, min=1)"));
}

#[test]
fn schema_doc_export_writes_html_docs_and_dry_run_plan_without_data_input() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let docs_dir = temp_dir.path().join("out/schema-html");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                [project]
                name = "schema_project"

                [[asset_kinds]]
                name = "ui_icon"
                extensions = [".png"]

                [[tables]]
                name = "classes"
                source = "classes.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables]]
                name = "items"
                source = "items.csv"
                desc = "道具表"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "name"
                type = "text"
                required = true
                export_when = {{ any = ["client"] }}

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"
                export_when = {{ any = ["client"] }}

                [[tables.fields]]
                name = "class_id"
                type = "int"
                foreign_key = {{ target_table = "classes", target_index = "primary", linked_field = "class" }}

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[targets]]
                name = "client_docs"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "schema_doc"
                format = "html"
                title = "Client Schema"
            "#,
            docs_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut dry_run = Command::cargo_bin("config-forge").expect("binary should build");
    dry_run.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client_docs",
        "--dry-run",
    ]);
    dry_run
        .assert()
        .success()
        .stdout(predicate::str::contains("artifact plan"))
        .stdout(predicate::str::contains("index.html"))
        .stdout(predicate::str::contains("items.html"))
        .stdout(predicate::str::contains("written: false"))
        .stdout(predicate::str::contains(
            "target `client_docs` dry-run succeeded",
        ));
    assert!(!docs_dir.join("index.html").exists());
    assert!(!docs_dir.join("items.html").exists());

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client_docs",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client_docs` built"));

    let index = fs::read_to_string(docs_dir.join("index.html")).expect("index should be readable");
    let items = fs::read_to_string(docs_dir.join("items.html")).expect("items should be readable");
    assert!(index.contains("<!doctype html>"));
    assert!(index.contains("<title>Client Schema</title>"));
    assert!(index.contains("<a href=\"items.html\"><code>items</code></a>"));
    assert!(index.contains("<h2>Asset fields</h2>"));
    assert!(index.contains("<h2>Text / POT fields</h2>"));
    assert!(index.contains("<h2>Relation overview</h2>"));
    assert!(items.contains("<h2>Fields</h2>"));
    assert!(items.contains("<h2>Indexes</h2>"));
    assert!(items.contains("<h2>Relations</h2>"));
    assert!(items.contains("<code>classes.id</code>"));
}

#[test]
fn schema_doc_rejects_unknown_format() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[targets]]
                name = "docs"

                [[targets.exports]]
                output = "{}"
                type = "schema_doc"
                format = "pdf"
            "#,
            temp_dir.path().join("out/docs").display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "docs",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unsupported schema_doc format"));
}
