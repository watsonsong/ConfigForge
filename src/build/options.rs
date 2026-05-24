// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{num::NonZeroUsize, path::Path};

use crate::{
    ConfigForgeError, Result,
    artifact::{ArtifactWriteMode, ArtifactWriteResult},
};

#[derive(Debug, Clone, Copy)]
pub struct BuildOptions<'a> {
    pub project_path: &'a Path,
    pub target_name: &'a str,
    pub execution: ExecutionOptions,
    pub artifact_write_mode: ArtifactWriteMode,
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
            .unwrap_or_else(|_| NonZeroUsize::new(1).expect("1 is non-zero"))
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
}
