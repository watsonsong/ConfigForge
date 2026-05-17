use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn validates_standard_project_fixture() {
    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "validate",
        "--meta",
        "tests/fixtures/standard_project/meta.toml",
        "--data",
        "tests/fixtures/standard_project/data",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("validation succeeded"));
}

#[test]
fn exports_standard_project_fixture() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("out");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "export",
        "--meta",
        "tests/fixtures/standard_project/meta.toml",
        "--data",
        "tests/fixtures/standard_project/data",
        "--out",
        out_dir.to_str().expect("path should be utf8"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("export succeeded"));

    assert!(out_dir.join("items.json").exists());
    let rust_code = fs::read_to_string(out_dir.join("rust/generated.rs")).expect("rust code");
    let cpp_code = fs::read_to_string(out_dir.join("cpp/generated.hpp")).expect("cpp code");
    let ts_code = fs::read_to_string(out_dir.join("typescript/generated.ts")).expect("ts code");
    assert!(rust_code.contains("pub struct ItemsRow"));
    assert!(rust_code.contains("pub fn get_by_id"));
    assert!(cpp_code.contains("class IDataFileLoader"));
    assert!(cpp_code.contains("const ItemsRow* GetById"));
    assert!(ts_code.contains("export interface DataFileLoader"));
    assert!(ts_code.contains("getById(id: number): ItemsRow | undefined"));
}
