// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::*;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn stderr_json(output: std::process::Output) -> serde_json::Value {
    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    serde_json::from_str(&stderr).expect("stderr should be diagnostic JSON")
}

#[test]
fn builds_target_with_execution_resource_flags() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            toml_path(&out_dir)
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
        "--jobs",
        "2",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));
    assert!(out_dir.join("generated.rs").exists());
}

#[test]
fn builds_target_with_serial_overriding_jobs() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    let trace_path = temp_dir.path().join("trace.json");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            toml_path(&out_dir)
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
        "--serial",
        "--jobs",
        "0",
        "--trace-tasks",
        trace_path.to_str().expect("path should be utf8"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));
    assert!(out_dir.join("generated.rs").exists());
    let trace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(trace_path).expect("trace should be readable"))
            .expect("trace should be valid JSON");
    assert_eq!(trace["jobs"], 1);
    assert_eq!(trace["serial"], true);
    assert_eq!(trace["io_concurrency"], 1);
    assert_eq!(trace["network_concurrency"], 1);
}

#[test]
fn rejects_zero_jobs_flag() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        r#"
            [[targets]]
            name = "empty"
        "#,
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "empty",
        "--jobs",
        "0",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--jobs must be greater than 0"));
}

#[test]
fn zero_jobs_json_diagnostic_has_stable_code_and_option_context() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        r#"
            [[targets]]
            name = "empty"
        "#,
    )
    .expect("project config should be written");

    let output = Command::cargo_bin("config-forge")
        .expect("binary should build")
        .args([
            "--project",
            project_path.to_str().expect("path should be utf8"),
            "--target",
            "empty",
            "--jobs",
            "0",
            "--diagnostic-format",
            "json",
        ])
        .output()
        .expect("config-forge should run");

    let json = stderr_json(output);
    assert_eq!(json["diagnostics"][0]["code"], "CF_BUILD_INVALID_OPTION");
    assert_eq!(json["diagnostics"][0]["severity"], "error");
    assert_eq!(json["diagnostics"][0]["source"], "--jobs");
    assert_eq!(json["diagnostics"][0]["domain"], "build");
    assert_eq!(json["diagnostics"][0]["current_value"], "0");
    assert_eq!(json["diagnostics"][0]["expected"], "integer in 1..=128");
}

#[test]
fn removed_cli_options_are_rejected() {
    for flag in ["--output", "--io-concurrency", "--network-concurrency"] {
        Command::cargo_bin("config-forge")
            .expect("binary should build")
            .args(["--project", "unused.toml", "--target", "client", flag, "1"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(format!(
                "unexpected argument '{flag}'"
            )));
    }
}

#[test]
fn help_describes_report_output_and_effective_resource_controls() {
    Command::cargo_bin("config-forge")
        .expect("binary should build")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--report-output <FILE>"))
        .stdout(predicate::str::contains(
            "does not override project artifact directories",
        ))
        .stdout(predicate::str::contains("--jobs"))
        .stdout(predicate::str::contains("--serial"))
        .stdout(predicate::str::contains("--output").not())
        .stdout(predicate::str::contains("--io-concurrency").not())
        .stdout(predicate::str::contains("--network-concurrency").not());
}
