// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::Path};

use assert_cmd::Command;
use serde_json::{Value, json};

fn asset_project(root: &Path, object: bool, manifest: bool) {
    fs::create_dir(root.join("UI")).unwrap();
    fs::write(root.join("UI/Icon.png"), "image").unwrap();
    let table_options = if object {
        "kind = 'object'"
    } else {
        "primary_index = 'by_id'\nrow_export = { excluded_tags_field = 'exclude' }"
    };
    let index = if object {
        ""
    } else {
        "[[tables.indexes]]\nname = 'by_id'\nfields = ['id']\nunique = true"
    };
    let manifest = if manifest {
        "[[targets.exports]]\ntype = 'asset_manifest'\noutput = 'manifest'\ninclude_source_location = true"
    } else {
        ""
    };
    fs::write(
        root.join("ConfigForge.toml"),
        format!(
            r#"
            [project]
            input = "."
            [assets]
            require_existing = true
            [[assets.search_paths]]
            name = "game"
            root = "."
            [[asset_kinds]]
            name = "icon"
            search_paths = ["game"]
            base_dirs = ["UI"]
            extensions = [".png"]
            path_style = "relative"
            extension_policy = "required"
            [[structs]]
            name = "Visual"
            [[structs.fields]]
            name = "inner"
            type = "Icon"
            [[structs.fields]]
            name = "server_icon"
            type = "asset"
            asset_kind = "icon"
            export_when = {{ any = ["server"] }}
            [[structs.fields]]
            name = "private_icon"
            type = "asset"
            asset_kind = "icon"
            export = false
            [[structs]]
            name = "Icon"
            [[structs.fields]]
            name = "icon"
            type = "asset"
            asset_kind = "icon"
            default = "Icon.png"
            [[tables]]
            name = "items"
            source = "items.csv"
            {table_options}
            [[tables.fields]]
            name = "id"
            type = "int"
            [[tables.fields]]
            name = "exclude"
            type = "string"
            export = false
            [[tables.fields]]
            name = "visual"
            type = "Visual"
            [[tables.fields]]
            name = "server_visual"
            type = "Visual"
            export_when = {{ any = ["server"] }}
            {index}
            [[targets]]
            name = "client"
            tags = ["client"]
            [[targets.exports]]
            type = "json"
            output = "out"
            {manifest}
            "#
        ),
    )
    .unwrap();
}

fn write_cells(root: &Path, object: bool, fields: &[(&str, &str)]) {
    let mut writer = csv::Writer::from_path(root.join("items.csv")).unwrap();
    if object {
        writer.write_record(["key", "value"]).unwrap();
        for &(key, value) in fields {
            writer.write_record([key, value]).unwrap();
        }
    } else {
        writer
            .write_record(fields.iter().map(|(key, _)| key))
            .unwrap();
        writer.write_record(fields.iter().map(|_| "label")).unwrap();
        writer
            .write_record(fields.iter().map(|(_, value)| value))
            .unwrap();
    }
    writer.flush().unwrap();
}

fn run(root: &Path, success: bool) -> Value {
    let output = Command::cargo_bin("config-forge")
        .unwrap()
        .args(["--project"])
        .arg(root.join("ConfigForge.toml"))
        .args(["--target", "client", "--diagnostic-format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stderr).unwrap()
}

#[test]
fn nested_asset_refs_share_validation_completion_and_manifest_across_layouts() {
    for object in [false, true] {
        for (key, value) in [
            ("visual.inner.icon", "Icon.png"),
            ("visual.inner", r#"{"icon":"Icon.png"}"#),
            ("visual", r#"{"inner":{"icon":"Icon.png"}}"#),
            ("visual.inner", "{}"),
            ("visual", r#"{"inner":{}}"#),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            asset_project(root, object, true);
            write_cells(root, object, &[("id", "001"), (key, value)]);
            run(root, true);
            let data: Value =
                serde_json::from_str(&fs::read_to_string(root.join("out/items.json")).unwrap())
                    .unwrap();
            let row = if object { &data } else { &data["rows"][0] };
            assert_eq!(row["visual"]["inner"]["icon"], "UI/Icon.png");
            let manifest: Value = serde_json::from_str(
                &fs::read_to_string(root.join("manifest/asset_manifest.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(manifest["assets"].as_array().unwrap().len(), 1);
            let asset = &manifest["assets"][0];
            assert_eq!(asset["path"], "UI/Icon.png");
            assert_eq!(asset["referenced_by"][0]["field"], "visual.inner.icon");
            assert_eq!(
                asset["referenced_by"][0]["row"],
                if object { "<object>" } else { "id=001" }
            );
        }
    }
}

#[test]
fn nested_missing_assets_report_source_location_without_a_manifest_task() {
    for object in [false, true] {
        for (key, value) in [
            ("visual.inner.icon", "Missing.png"),
            ("visual", r#"{"inner":{"icon":"Missing.png"}}"#),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            asset_project(root, object, false);
            write_cells(root, object, &[("id", "001"), (key, value)]);
            let result = run(root, false);
            let issue = result["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .find(|issue| issue["code"] == "CF_DATA_INVALID_ASSET_REF")
                .unwrap();
            assert_eq!(issue["location"]["field"], "visual.inner.icon");
            assert_eq!(issue["location"]["source_row_number"], 3);
            assert_eq!(
                issue["location"]["source_column_name"],
                if object { "value" } else { key }
            );
            assert_eq!(issue["current_value"], "Missing.png");
            assert!(!root.join("out/items.json").exists());
        }
    }
}

#[test]
fn nested_asset_refs_respect_child_parent_and_row_visibility() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    asset_project(root, false, true);
    write_cells(
        root,
        false,
        &[
            ("id", "001"),
            ("visual.inner.icon", "Icon.png"),
            ("visual.server_icon", "Missing.png"),
            ("visual.private_icon", "Missing.png"),
            ("server_visual.inner.icon", "Missing.png"),
            ("exclude", ""),
        ],
    );
    let mut csv = fs::OpenOptions::new()
        .append(true)
        .open(root.join("items.csv"))
        .unwrap();
    use std::io::Write;
    writeln!(
        csv,
        "002,Missing.png,Missing.png,Missing.png,Missing.png,client"
    )
    .unwrap();
    let result = run(root, true);
    assert_eq!(result["diagnostics"], json!([]));
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("manifest/asset_manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["assets"].as_array().unwrap().len(), 1);
    assert_eq!(
        manifest["assets"][0]["referenced_by"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn nested_asset_refs_in_typed_and_inline_fragments_use_the_same_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    asset_project(root, false, true);
    let path = root.join("ConfigForge.toml");
    let config = fs::read_to_string(&path)
        .unwrap()
        .replace(
            "source = \"items.csv\"",
            "sources = [{ path = 'items.csv', fragments = ['typed', 'inline'] }]\npolymorphic = true",
        )
        .replace(
            "[[targets]]",
            r#"
            [[tables.fragments]]
            name = "typed"
            type = "Visual"
            [[tables.fragments]]
            name = "inline"
            [[tables.fragments.fields]]
            name = "visual"
            type = "Visual"
            [[targets]]
            "#,
        );
    fs::write(&path, config).unwrap();
    write_cells(
        root,
        false,
        &[
            ("id", "001"),
            ("visual.inner.icon", "Icon.png"),
            ("typed.inner.icon", "Icon.png"),
            ("inline.visual", r#"{"inner":{}}"#),
        ],
    );
    run(root, true);
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("manifest/asset_manifest.json")).unwrap(),
    )
    .unwrap();
    let references = manifest["assets"][0]["referenced_by"].as_array().unwrap();
    let paths = references
        .iter()
        .map(|reference| reference["field"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "inline.visual.inner.icon",
            "typed.inner.icon",
            "visual.inner.icon"
        ]
    );
    let data: Value =
        serde_json::from_str(&fs::read_to_string(root.join("out/items.json")).unwrap()).unwrap();
    assert_eq!(data["rows"][0]["typed"]["inner"]["icon"], "UI/Icon.png");
    assert_eq!(
        data["rows"][0]["inline"]["visual"]["inner"]["icon"],
        "UI/Icon.png"
    );
}

#[test]
fn nested_asset_override_is_validated_and_keeps_override_source_location() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    asset_project(root, true, true);
    let path = root.join("ConfigForge.toml");
    let config = fs::read_to_string(&path).unwrap().replace(
        "source = \"items.csv\"",
        "source = 'items.csv'\noverride_sources = [{ path = 'override.csv' }]",
    );
    fs::write(&path, config).unwrap();
    write_cells(
        root,
        true,
        &[("id", "001"), ("visual.inner.icon", "Missing.png")],
    );
    fs::write(
        root.join("override.csv"),
        "key,value\nvisual.inner.icon,Icon.png\n",
    )
    .unwrap();
    run(root, true);
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("manifest/asset_manifest.json")).unwrap(),
    )
    .unwrap();
    assert!(
        manifest["assets"][0]["referenced_by"][0]["source"]
            .as_str()
            .unwrap()
            .contains("override.csv:2")
    );
    fs::write(
        root.join("override.csv"),
        "key,value\nvisual.inner.icon,Missing.png\n",
    )
    .unwrap();
    let result = run(root, false);
    let issue = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| issue["code"] == "CF_DATA_INVALID_ASSET_REF")
        .unwrap();
    assert!(
        issue["location"]["source_path"]
            .as_str()
            .unwrap()
            .ends_with("override.csv")
    );
    assert_eq!(issue["location"]["source_row_number"], 2);
}
