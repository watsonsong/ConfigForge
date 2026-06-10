// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, num::NonZeroUsize, path::Path};

use serde::Serialize;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    artifact::{ArtifactWriteMode, ArtifactWriteResult},
    diagnostic::code::CF_BUILD_INVALID_OPTION,
    validation::ValidationReportExportConfig,
};

#[derive(Debug, Clone)]
pub struct BuildOptions<'a> {
    pub project_path: &'a Path,
    pub target_name: &'a str,
    pub extra_tags: &'a [String],
    pub execution: ExecutionOptions,
    pub artifact_write_mode: ArtifactWriteMode,
    pub validation_report: Option<ValidationReportExportConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionOptions {
    pub jobs: usize,
    pub io_concurrency: usize,
    pub network_concurrency: usize,
    pub serial: bool,
}

pub(crate) const MAX_JOBS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutput {
    pub warnings: Vec<String>,
    pub artifacts: ArtifactWriteResult,
    pub trace: BuildTrace,
}

#[derive(Debug)]
pub struct BuildErrorWithTrace {
    pub error: ConfigForgeError,
    pub trace: BuildTrace,
}

impl fmt::Display for BuildErrorWithTrace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl Error for BuildErrorWithTrace {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.error.source()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildTrace {
    pub target: String,
    pub jobs: usize,
    pub io_concurrency: usize,
    pub network_concurrency: usize,
    pub serial: bool,
    pub total_duration_ms: u128,
    pub stages: Vec<BuildTraceStage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildTraceStage {
    pub name: String,
    pub started_at_ms: u128,
    pub duration_ms: u128,
    pub status: BuildTraceStageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildTraceStageStatus {
    Succeeded,
    Failed,
}

impl BuildTrace {
    pub(crate) fn new(target: &str, execution: ExecutionOptions) -> Self {
        Self {
            target: target.to_owned(),
            jobs: execution.jobs,
            io_concurrency: execution.io_concurrency,
            network_concurrency: execution.network_concurrency,
            serial: execution.serial,
            total_duration_ms: 0,
            stages: Vec::new(),
        }
    }
}

impl ExecutionOptions {
    pub fn from_cli(
        jobs: Option<usize>,
        io_concurrency: Option<usize>,
        network_concurrency: Option<usize>,
        serial: bool,
    ) -> Result<Self> {
        let mut options = Self::default();
        if let Some(jobs) = jobs {
            options.jobs = jobs;
        }
        if let Some(io_concurrency) = io_concurrency {
            options.io_concurrency = io_concurrency;
        }
        if let Some(network_concurrency) = network_concurrency {
            options.network_concurrency = network_concurrency;
        }
        options.serial = serial;
        options.normalize()
    }

    fn normalize(mut self) -> Result<Self> {
        if self.serial {
            self.jobs = 1;
            self.io_concurrency = 1;
            self.network_concurrency = 1;
        }
        if self.jobs == 0 {
            return Err(build_option_error(
                "--jobs must be greater than 0".to_owned(),
                "--jobs",
                self.jobs.to_string(),
                "integer in 1..=128",
            ));
        }
        if self.jobs > MAX_JOBS {
            return Err(build_option_error(
                format!("--jobs must be no greater than {MAX_JOBS}"),
                "--jobs",
                self.jobs.to_string(),
                format!("integer in 1..={MAX_JOBS}"),
            ));
        }
        if self.io_concurrency == 0 {
            return Err(build_option_error(
                "--io-concurrency must be greater than 0".to_owned(),
                "--io-concurrency",
                self.io_concurrency.to_string(),
                "positive integer",
            ));
        }
        if self.network_concurrency == 0 {
            return Err(build_option_error(
                "--network-concurrency must be greater than 0".to_owned(),
                "--network-concurrency",
                self.network_concurrency.to_string(),
                "positive integer",
            ));
        }
        Ok(self)
    }
}

fn build_option_error(
    message: String,
    source: &'static str,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_BUILD_INVALID_OPTION,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(source)
        .with_domain("build")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint("Check CLI execution resource flags before starting the build."),
    )
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        let available = std::thread::available_parallelism()
            .unwrap_or(NonZeroUsize::MIN)
            .get();
        let jobs = available.clamp(1, 4);
        let io_concurrency = (jobs * 2).clamp(4, 16);
        let network_concurrency = io_concurrency.min(8);
        Self {
            jobs,
            io_concurrency,
            network_concurrency,
            serial: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_single_diagnostic(error: ConfigForgeError) -> Diagnostic {
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected structured diagnostic report");
        };
        assert_eq!(report.diagnostics.len(), 1);
        report.diagnostics.into_iter().next().unwrap()
    }

    #[test]
    fn execution_options_apply_defaults_overrides_and_serial() {
        let defaults = ExecutionOptions::default();
        assert!(defaults.jobs >= 1);
        assert!(defaults.jobs <= 4);
        assert!(defaults.io_concurrency >= defaults.jobs);
        assert!(defaults.network_concurrency <= defaults.io_concurrency);

        let custom = ExecutionOptions::from_cli(Some(2), Some(5), Some(3), false)
            .expect("custom execution options should be valid");
        assert_eq!(custom.jobs, 2);
        assert_eq!(custom.io_concurrency, 5);
        assert_eq!(custom.network_concurrency, 3);
        assert!(!custom.serial);

        let serial = ExecutionOptions::from_cli(Some(8), Some(8), Some(8), true)
            .expect("serial should override explicit concurrency");
        assert_eq!(serial.jobs, 1);
        assert_eq!(serial.io_concurrency, 1);
        assert_eq!(serial.network_concurrency, 1);
        assert!(serial.serial);
    }

    #[test]
    fn execution_options_reject_zero_limits() {
        let cases = [
            ExecutionOptions::from_cli(Some(0), None, None, false),
            ExecutionOptions::from_cli(None, Some(0), None, false),
            ExecutionOptions::from_cli(None, None, Some(0), false),
        ];

        for (error, source) in cases.into_iter().map(Result::unwrap_err).zip([
            "--jobs",
            "--io-concurrency",
            "--network-concurrency",
        ]) {
            let diagnostic = expect_single_diagnostic(error);
            assert_eq!(diagnostic.code, CF_BUILD_INVALID_OPTION);
            assert!(diagnostic.message.contains("greater than 0"));
            assert_eq!(diagnostic.source.as_deref(), Some(source));
            assert_eq!(diagnostic.domain.as_deref(), Some("build"));
            assert_eq!(diagnostic.current_value.as_deref(), Some("0"));
        }
    }

    #[test]
    fn execution_options_reject_excessive_jobs() {
        let error = ExecutionOptions::from_cli(Some(MAX_JOBS + 1), None, None, false).unwrap_err();

        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, CF_BUILD_INVALID_OPTION);
        assert!(
            diagnostic
                .message
                .contains("--jobs must be no greater than")
        );
        assert_eq!(diagnostic.source.as_deref(), Some("--jobs"));
        assert_eq!(diagnostic.domain.as_deref(), Some("build"));
        assert_eq!(
            diagnostic.current_value.as_deref(),
            Some((MAX_JOBS + 1).to_string().as_str())
        );
        assert_eq!(
            diagnostic.expected.as_deref(),
            Some(format!("integer in 1..={MAX_JOBS}").as_str())
        );
    }
}
