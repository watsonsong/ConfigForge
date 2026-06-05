// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::process::Command as StdCommand;

use assert_cmd::Command;

pub(crate) fn run_config_forge_stdout(args: &[&str]) -> String {
    let output = Command::cargo_bin("config-forge")
        .expect("binary should build")
        .args(args)
        .output()
        .expect("config-forge should run");
    assert!(
        output.status.success(),
        "config-forge failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout should be utf8")
}
pub(crate) fn run_config_forge_success(args: &[&str]) {
    let output = Command::cargo_bin("config-forge")
        .expect("binary should build")
        .args(args)
        .output()
        .expect("config-forge should run");
    assert!(
        output.status.success(),
        "config-forge failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
pub(crate) fn command_exists(command: &str) -> bool {
    StdCommand::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
pub(crate) fn assert_command_success(output: std::process::Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
