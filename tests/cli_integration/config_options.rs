// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::Path};

use assert_cmd::Command;
use serde_json::Value;

const SCHEMA: &str = r#"[[tables]]
name = "items"
source = "items.csv"
[[tables.fields]]
name = "id"
type = "int"
"#;

fn project(root: &Path, schema: &str, generator: &str, options: &str) {
    fs::write(root.join("items.csv"), "id\nID\n1\n").unwrap();
    fs::write(
        root.join("ConfigForge.toml"),
        format!(
            r#"
            [project]
            input = "."
            {schema}
            [[targets]]
            name = "client"
            [[targets.exports]]
            type = "{generator}"
            output = "out"
            {options}
            "#
        ),
    )
    .unwrap();
}

fn run(root: &Path, success: bool) -> Value {
    let output = Command::cargo_bin("config-forge")
        .unwrap()
        .args(["--project"])
        .arg(root.join("ConfigForge.toml"))
        .args([
            "--target",
            "client",
            "--dry-run",
            "--diagnostic-format",
            "json",
        ])
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
fn unknown_project_meta_and_import_keys_produce_located_parse_diagnostics() {
    for (from, to, unknown) in [
        ("[project]", "root_typo = true\n[project]", "root_typo"),
        ("input = \".\"", "input = '.'\ninpt = '.'", "inpt"),
        (
            "source = \"items.csv\"",
            "source = 'items.csv'\npolymorphc = true",
            "polymorphc",
        ),
        ("type = \"int\"", "type = 'int'\nrequird = true", "requird"),
        (
            "type = \"int\"",
            "type = 'int'\nexport_when = { anny = ['client'] }",
            "anny",
        ),
        (
            "type = \"int\"",
            "type = 'int'\nforeign_key = { target_table = 'items', allow_emtpy = true }",
            "allow_emtpy",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        project(temp.path(), SCHEMA, "rust", "");
        let path = temp.path().join("ConfigForge.toml");
        let config = fs::read_to_string(&path).unwrap().replace(from, to);
        fs::write(&path, config).unwrap();
        let result = run(temp.path(), false);
        let diagnostic = &result["diagnostics"][0];
        assert_eq!(diagnostic["code"], "CF_PROJECT_PARSE_FAILED");
        assert!(diagnostic["message"].as_str().unwrap().contains(unknown));
        assert!(
            diagnostic["location"]["source_row_number"]
                .as_u64()
                .is_some()
        );
    }
    let temp = tempfile::tempdir().unwrap();
    project(temp.path(), "imports = []", "rust", "");
    let path = temp.path().join("ConfigForge.toml");
    fs::write(
        &path,
        "imports = ['meta.toml']\n[[targets]]\nname = 'client'\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("meta.toml"),
        format!("{SCHEMA}\nrequird = true\n"),
    )
    .unwrap();
    let result = run(temp.path(), false);
    assert!(
        result["diagnostics"][0]["location"]["source_path"]
            .as_str()
            .unwrap()
            .ends_with("meta.toml")
    );
}

#[test]
fn unknown_generator_options_are_rejected_at_every_configuration_level() {
    for (generator, options, unknown) in [
        ("rust", "ruts = { derive_serde = true }", "ruts"),
        ("rust", "rust = { derive_sered = true }", "derive_sered"),
        ("rust", "cpp = {}", "cpp"),
        ("rust", "naming = { filed = 'snake_case' }", "filed"),
        ("rust", "formatting = { indent_wdith = 2 }", "indent_wdith"),
        ("rust", "i18n = { mod = 'generic' }", "mod"),
        ("cpp", "cpp = { namespce = 'game' }", "namespce"),
        (
            "cpp",
            "cpp = { type_mappings = { vector_2 = 'Vec2' } }",
            "vector_2",
        ),
        (
            "cpp_unreal",
            "cpp_unreal = { suport = 'containers' }",
            "suport",
        ),
        (
            "typescript",
            "typescript = { readonly_fileds = true }",
            "readonly_fileds",
        ),
        ("csharp", "csharp = { nullabble = true }", "nullabble"),
        ("java", "java = { packge = 'game' }", "packge"),
        ("json", "prettyy = true", "prettyy"),
        ("msgpack", "prettyy = true", "prettyy"),
        ("binary", "prettyy = true", "prettyy"),
        ("lua", "lazy_requre = true", "lazy_requre"),
        ("lua", "lua = { lazy_requre = true }", "lazy_requre"),
        ("lua", "lua = {}\nlazy_requre = true", "lazy_requre"),
        ("pot", "deduplcate = true", "deduplcate"),
        (
            "assets",
            "include_referened_by = true",
            "include_referened_by",
        ),
        ("schema_doc", "include_hiden = true", "include_hiden"),
        ("validation_report", "titel = 'Issues'", "titel"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        project(temp.path(), SCHEMA, generator, options);
        let result = run(temp.path(), false);
        let diagnostic = &result["diagnostics"][0];
        assert!(
            diagnostic["code"]
                .as_str()
                .unwrap()
                .starts_with("CF_GENERATOR_"),
            "{result}"
        );
        assert!(
            diagnostic["message"].as_str().unwrap().contains(unknown),
            "{result}"
        );
        assert!(!temp.path().join("out").exists());
    }
}

#[test]
fn strict_options_preserve_generator_aliases_nested_options_and_validator_params() {
    for (generator, options) in [
        ("rust", "rust = { derive_serde = true }"),
        ("c++", "cpp = { namespace = 'game' }"),
        ("ts", "typescript = { readonly_fields = true }"),
        ("cs", "csharp = { namespace = 'Game' }"),
        ("java", "java = { package = 'game' }"),
        ("cpp_unreal", "cpp_unreal = { support = 'containers' }"),
        ("msgpack", ""),
        ("binary", ""),
        ("lua", "lua = { lazy_require = true }"),
        ("lua_table", "lazy_require = true"),
        ("pot", "gettext_pot = { domain = 'game' }"),
        (
            "assets",
            "asset_manifest = { include_referenced_by = true }",
        ),
        ("json", ""),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let schema =
            format!("{SCHEMA}\n[[tables.fields.validators]]\nkind = 'range'\nmin = 0\nmax = 10\n");
        project(temp.path(), &schema, generator, options);
        run(temp.path(), true);
    }
}

#[test]
fn dynamic_validator_params_remain_owned_by_the_validator_parser() {
    let temp = tempfile::tempdir().unwrap();
    let schema = format!("{SCHEMA}\n[[tables.fields.validators]]\nkind = 'range'\nminn = 0\n");
    project(temp.path(), &schema, "json", "");
    let result = run(temp.path(), false);
    let text = result.to_string();
    assert!(text.contains("minn"));
    assert!(!text.contains("CF_PROJECT_PARSE_FAILED"));
}
