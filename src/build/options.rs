// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{num::NonZeroUsize, path::Path};

use serde::Serialize;

use crate::{
    ConfigForgeError, Result,
    artifact::{ArtifactWriteLimits, ArtifactWriteMode, ArtifactWriteResult},
};

#[derive(Debug, Clone, Copy)]
pub struct BuildOptions<'a> {
    pub project_path: &'a Path,
    pub target_name: &'a str,
    pub extra_tags: &'a [String],
    pub execution: ExecutionOptions,
    pub artifact_write_mode: ArtifactWriteMode,
    pub artifact_write_limits: ArtifactWriteLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionOptions {
    pub jobs: usize,
    pub io_concurrency: usize,
    pub network_concurrency: usize,
    pub serial: bool,
}

pub(crate) const MAX_JOBS: usize = 128;
const MIN_ARTIFACT_MEMORY_LIMIT_BYTES: usize = 1024 * 1024;
const MIN_DEFAULT_ARTIFACT_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const MAX_DEFAULT_ARTIFACT_MEMORY_BYTES: usize = 2 * 1024 * 1024 * 1024;
const FALLBACK_ARTIFACT_MEMORY_BYTES: usize = 256 * 1024 * 1024;
const DEFAULT_ARTIFACT_MEMORY_PERCENT: u8 = 25;
const MAX_ARTIFACT_MEMORY_PERCENT: u8 = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutput {
    pub warnings: Vec<String>,
    pub artifacts: ArtifactWriteResult,
    pub trace: BuildTrace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildTrace {
    pub target: String,
    pub jobs: usize,
    pub io_concurrency: usize,
    pub network_concurrency: usize,
    pub serial: bool,
    pub artifact_memory_bytes: usize,
    pub artifact_queue_capacity: usize,
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
    pub(crate) fn new(
        target: &str,
        execution: ExecutionOptions,
        artifact_write_limits: ArtifactWriteLimits,
    ) -> Self {
        Self {
            target: target.to_owned(),
            jobs: execution.jobs,
            io_concurrency: execution.io_concurrency,
            network_concurrency: execution.network_concurrency,
            serial: execution.serial,
            artifact_memory_bytes: artifact_write_limits.max_in_flight_bytes,
            artifact_queue_capacity: artifact_write_limits.max_in_flight_artifacts,
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
            return Err(ConfigForgeError::Validation(
                "--jobs must be greater than 0".to_owned(),
            ));
        }
        if self.jobs > MAX_JOBS {
            return Err(ConfigForgeError::Validation(format!(
                "--jobs must be no greater than {MAX_JOBS}"
            )));
        }
        if self.io_concurrency == 0 {
            return Err(ConfigForgeError::Validation(
                "--io-concurrency must be greater than 0".to_owned(),
            ));
        }
        if self.network_concurrency == 0 {
            return Err(ConfigForgeError::Validation(
                "--network-concurrency must be greater than 0".to_owned(),
            ));
        }
        Ok(self)
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactMemoryOptions {
    pub limit_bytes: Option<usize>,
    pub percent: Option<u8>,
}

impl ArtifactMemoryOptions {
    pub fn from_cli(limit: Option<&str>, percent: Option<u8>) -> Result<Self> {
        let limit_bytes = limit.map(parse_byte_size).transpose()?;
        let options = Self {
            limit_bytes,
            percent,
        };
        options.validate()?;
        Ok(options)
    }

    pub fn resolve_limits(self, jobs: usize) -> ArtifactWriteLimits {
        let max_in_flight_bytes = match self.limit_bytes {
            Some(bytes) => bytes,
            None => {
                let percent = self.percent.unwrap_or(DEFAULT_ARTIFACT_MEMORY_PERCENT);
                available_memory_bytes()
                    .map(|available| available.saturating_mul(percent as usize) / 100)
                    .unwrap_or(FALLBACK_ARTIFACT_MEMORY_BYTES)
                    .clamp(
                        MIN_DEFAULT_ARTIFACT_MEMORY_BYTES,
                        MAX_DEFAULT_ARTIFACT_MEMORY_BYTES,
                    )
            }
        };
        ArtifactWriteLimits::new(max_in_flight_bytes, (jobs * 4).max(16))
            .expect("resolved artifact write limits should be valid")
    }

    fn validate(self) -> Result<()> {
        if self.limit_bytes.is_some() && self.percent.is_some() {
            return Err(ConfigForgeError::Validation(
                "--artifact-memory-limit and --artifact-memory-percent cannot be used together"
                    .to_owned(),
            ));
        }
        if let Some(limit_bytes) = self.limit_bytes {
            if limit_bytes < MIN_ARTIFACT_MEMORY_LIMIT_BYTES {
                return Err(ConfigForgeError::Validation(format!(
                    "--artifact-memory-limit must be at least {MIN_ARTIFACT_MEMORY_LIMIT_BYTES} bytes"
                )));
            }
        }
        if let Some(percent) = self.percent {
            if percent == 0 || percent > MAX_ARTIFACT_MEMORY_PERCENT {
                return Err(ConfigForgeError::Validation(format!(
                    "--artifact-memory-percent must be between 1 and {MAX_ARTIFACT_MEMORY_PERCENT}"
                )));
            }
        }
        Ok(())
    }
}

impl Default for ArtifactMemoryOptions {
    fn default() -> Self {
        Self {
            limit_bytes: None,
            percent: Some(DEFAULT_ARTIFACT_MEMORY_PERCENT),
        }
    }
}

fn parse_byte_size(input: &str) -> Result<usize> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ConfigForgeError::Validation(
            "--artifact-memory-limit must not be empty".to_owned(),
        ));
    }
    let digits_len = trimmed
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(trimmed.len());
    if digits_len == 0 {
        return Err(ConfigForgeError::Validation(format!(
            "invalid --artifact-memory-limit `{input}`"
        )));
    }
    let value = trimmed[..digits_len].parse::<usize>().map_err(|_| {
        ConfigForgeError::Validation(format!("invalid --artifact-memory-limit `{input}`"))
    })?;
    let unit = trimmed[digits_len..].trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        _ => {
            return Err(ConfigForgeError::Validation(format!(
                "unsupported --artifact-memory-limit unit `{}`",
                &trimmed[digits_len..]
            )));
        }
    };
    value.checked_mul(multiplier).ok_or_else(|| {
        ConfigForgeError::Validation(format!("--artifact-memory-limit `{input}` is too large"))
    })
}

fn available_memory_bytes() -> Option<usize> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    usize::try_from(system.available_memory()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

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

        for error in cases.into_iter().map(Result::unwrap_err) {
            assert!(
                matches!(error, ConfigForgeError::Validation(message) if message.contains("greater than 0"))
            );
        }
    }

    #[test]
    fn execution_options_reject_excessive_jobs() {
        let error = ExecutionOptions::from_cli(Some(MAX_JOBS + 1), None, None, false).unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("--jobs must be no greater than"))
        );
    }

    #[test]
    fn artifact_memory_options_parse_fixed_sizes() {
        let options = ArtifactMemoryOptions::from_cli(Some("128MiB"), None)
            .expect("fixed artifact memory should parse");

        assert_eq!(options.limit_bytes, Some(128 * 1024 * 1024));
        assert_eq!(options.percent, None);
        assert_eq!(
            options.resolve_limits(2).max_in_flight_bytes,
            128 * 1024 * 1024
        );
        assert_eq!(options.resolve_limits(2).max_in_flight_artifacts, 16);
        assert_eq!(options.resolve_limits(8).max_in_flight_artifacts, 32);
    }

    #[test]
    fn artifact_memory_options_validate_conflicts_and_bounds() {
        assert!(matches!(
            ArtifactMemoryOptions::from_cli(Some("64MiB"), Some(25)),
            Err(ConfigForgeError::Validation(message))
                if message.contains("cannot be used together")
        ));
        assert!(matches!(
            ArtifactMemoryOptions::from_cli(Some("512KiB"), None),
            Err(ConfigForgeError::Validation(message))
                if message.contains("must be at least")
        ));
        assert!(matches!(
            ArtifactMemoryOptions::from_cli(None, Some(0)),
            Err(ConfigForgeError::Validation(message))
                if message.contains("between 1")
        ));
        assert!(matches!(
            ArtifactMemoryOptions::from_cli(None, Some(81)),
            Err(ConfigForgeError::Validation(message))
                if message.contains("between 1")
        ));
        assert!(matches!(
            ArtifactMemoryOptions::from_cli(Some("10XB"), None),
            Err(ConfigForgeError::Validation(message))
                if message.contains("unsupported")
        ));
    }
}
