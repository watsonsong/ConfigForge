// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::process::Command as StdCommand;

use assert_cmd::Command;

#[derive(Debug, Clone)]
pub(crate) struct ToolProbe {
    pub(crate) command: String,
    pub(crate) version_args: Vec<String>,
    pub(crate) available: bool,
    pub(crate) exit_status: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) error: Option<String>,
}

impl ToolProbe {
    fn diagnostic(&self) -> String {
        if let Some(error) = &self.error {
            return format!("{} unavailable: {error}", self.command);
        }
        if self.available {
            return format!("{} available", self.command);
        }
        let mut parts = vec![format!(
            "{} unavailable: version command {:?} exited with {:?}",
            self.command, self.version_args, self.exit_status
        )];
        if !self.stderr.trim().is_empty() {
            parts.push(format!("stderr: {}", self.stderr.trim()));
        }
        if !self.stdout.trim().is_empty() {
            parts.push(format!("stdout: {}", self.stdout.trim()));
        }
        parts.join("; ")
    }
}

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
    probe_tool(command).available
}

pub(crate) fn probe_tool(command: &str) -> ToolProbe {
    probe_tool_with_args(command, &["--version"])
}

pub(crate) fn probe_tool_with_args(command: &str, version_args: &[&str]) -> ToolProbe {
    match StdCommand::new(command).args(version_args).output() {
        Ok(output) => ToolProbe {
            command: command.to_owned(),
            version_args: version_args.iter().map(|arg| (*arg).to_owned()).collect(),
            available: output.status.success(),
            exit_status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            error: None,
        },
        Err(error) => ToolProbe {
            command: command.to_owned(),
            version_args: version_args.iter().map(|arg| (*arg).to_owned()).collect(),
            available: false,
            exit_status: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(error.to_string()),
        },
    }
}

#[allow(dead_code)]
pub(crate) fn probe_any_tool(commands: &[&str]) -> Option<ToolProbe> {
    commands
        .iter()
        .map(|command| probe_tool(command))
        .find(|probe| probe.available)
}

pub(crate) fn skip_optional_test(test_name: &str, reason: &str) {
    eprintln!("skipping optional test `{test_name}`: {reason}");
}

pub(crate) fn require_tool_or_skip(test_name: &str, command: &str) -> Option<ToolProbe> {
    let probe = probe_tool(command);
    if probe.available {
        Some(probe)
    } else {
        skip_optional_test(test_name, &probe.diagnostic());
        None
    }
}

pub(crate) fn require_any_tool_or_skip(test_name: &str, commands: &[&str]) -> Option<ToolProbe> {
    let mut diagnostics = Vec::new();
    for command in commands {
        let probe = probe_tool(command);
        if probe.available {
            return Some(probe);
        }
        diagnostics.push(probe.diagnostic());
    }
    skip_optional_test(
        test_name,
        &format!(
            "none of [{}] is available ({})",
            commands.join(", "),
            diagnostics.join(" | ")
        ),
    );
    None
}
pub(crate) fn assert_command_success(output: std::process::Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(test)]
mod tests {
    use super::{probe_tool, require_any_tool_or_skip};

    #[test]
    fn probe_tool_reports_missing_command() {
        let probe = probe_tool("config-forge-definitely-missing-tool");
        assert!(!probe.available);
        assert!(probe.error.is_some() || probe.exit_status != Some(0));
    }

    #[test]
    fn require_any_tool_reports_missing_commands() {
        let probe = require_any_tool_or_skip(
            "missing tool probe test",
            &[
                "config-forge-definitely-missing-tool-a",
                "config-forge-definitely-missing-tool-b",
            ],
        );
        assert!(probe.is_none());
    }
}
