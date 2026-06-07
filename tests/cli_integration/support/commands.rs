// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    process::Command as StdCommand,
    sync::atomic::{AtomicUsize, Ordering},
};

use assert_cmd::Command;

static SKIPPED_OPTIONAL_TESTS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestProfile {
    Local,
    Quick,
    Full,
    Nightly,
}

impl TestProfile {
    pub(crate) fn current() -> Self {
        match std::env::var("CONFIG_FORGE_TEST_PROFILE") {
            Ok(value) if value.eq_ignore_ascii_case("quick") => Self::Quick,
            Ok(value) if value.eq_ignore_ascii_case("full") => Self::Full,
            Ok(value) if value.eq_ignore_ascii_case("nightly") => Self::Nightly,
            _ => Self::Local,
        }
    }

    fn requires_optional_tools(self) -> bool {
        matches!(self, Self::Full | Self::Nightly)
    }
}

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
    SKIPPED_OPTIONAL_TESTS.fetch_add(1, Ordering::Relaxed);
    eprintln!("skipping optional test `{test_name}`: {reason}");
}

pub(crate) fn network_allowed() -> bool {
    std::env::var("CONFIG_FORGE_ALLOW_NETWORK")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub(crate) fn skipped_optional_test_count() -> usize {
    SKIPPED_OPTIONAL_TESTS.load(Ordering::Relaxed)
}

#[allow(dead_code)]
pub(crate) fn language_is_skipped(language: &str) -> bool {
    parse_tool_list(std::env::var("CONFIG_FORGE_SKIP_LANGUAGES").ok().as_deref())
        .any(|candidate| candidate.eq_ignore_ascii_case(language))
}

#[allow(dead_code)]
pub(crate) fn skip_language_or_continue(test_name: &str, language: &str) -> bool {
    if language_is_skipped(language) {
        skip_optional_test(
            test_name,
            &format!("language `{language}` disabled by CONFIG_FORGE_SKIP_LANGUAGES"),
        );
        true
    } else {
        false
    }
}

pub(crate) fn require_tool_or_skip(test_name: &str, command: &str) -> Option<ToolProbe> {
    let probe = probe_tool(command);
    if probe.available {
        Some(probe)
    } else {
        handle_missing_required_tool(test_name, &[command], &probe.diagnostic());
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
    handle_missing_required_tool(
        test_name,
        commands,
        &format!(
            "none of [{}] is available ({})",
            commands.join(", "),
            diagnostics.join(" | ")
        ),
    );
    None
}

fn handle_missing_required_tool(test_name: &str, commands: &[&str], reason: &str) {
    if commands.iter().any(|command| tool_is_required(command)) {
        panic!("required tool missing for `{test_name}`: {reason}");
    }
    skip_optional_test(test_name, reason);
}

fn tool_is_required(command: &str) -> bool {
    tool_is_required_with(
        TestProfile::current(),
        std::env::var("CONFIG_FORGE_REQUIRE_TOOLS").ok().as_deref(),
        command,
    )
}

fn tool_is_required_with(profile: TestProfile, require_tools: Option<&str>, command: &str) -> bool {
    profile.requires_optional_tools() || parse_tool_list(require_tools).any(|tool| tool == command)
}

fn parse_tool_list(raw: Option<&str>) -> impl Iterator<Item = &str> {
    raw.into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[allow(dead_code)]
pub(crate) fn current_test_profile() -> TestProfile {
    TestProfile::current()
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
    use super::{TestProfile, probe_tool, require_any_tool_or_skip, tool_is_required_with};

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

    #[test]
    fn tool_requirement_respects_profile_and_env_list() {
        assert!(!tool_is_required_with(TestProfile::Local, None, "node"));
        assert!(tool_is_required_with(TestProfile::Full, None, "node"));
        assert!(tool_is_required_with(
            TestProfile::Local,
            Some("cmake,node"),
            "node"
        ));
        assert!(!tool_is_required_with(
            TestProfile::Quick,
            Some("cmake,node"),
            "dotnet"
        ));
    }

    #[test]
    fn parse_tool_list_trims_empty_entries() {
        let values: Vec<_> = super::parse_tool_list(Some(" cmake, ,node,, ")).collect();
        assert_eq!(values, vec!["cmake", "node"]);
    }

    #[test]
    fn network_allowed_defaults_to_false_without_env() {
        // This test relies on the standard test environment not setting the opt-in variable.
        if std::env::var("CONFIG_FORGE_ALLOW_NETWORK").is_err() {
            assert!(!super::network_allowed());
        }
    }
}
