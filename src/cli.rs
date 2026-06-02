// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::PathBuf};

use clap::{Parser, ValueEnum};

use crate::{
    ArtifactMemoryOptions,
    artifact::ArtifactWriteMode,
    build::{BuildOptions, ExecutionOptions, build_target_with_trace},
};

#[derive(Debug, Parser)]
#[command(name = "config-forge")]
#[command(about = "Validate and export designer-authored game data", long_about = None)]
struct Cli {
    /// Path to ConfigForge project config.
    #[arg(long)]
    project: PathBuf,
    /// Target name to build from the project config.
    #[arg(long)]
    target: String,
    /// Additional target tag for this build. Can be provided multiple times.
    #[arg(long = "tag")]
    extra_tags: Vec<String>,
    /// CPU worker count for parsing, validation, codegen and serialization.
    #[arg(long)]
    jobs: Option<usize>,
    /// Reserved for future async providers/writers; currently parsed but not applied.
    #[arg(long)]
    io_concurrency: Option<usize>,
    /// Reserved for future remote source providers; currently parsed but not applied.
    #[arg(long)]
    network_concurrency: Option<usize>,
    /// Run with one CPU worker and one I/O/network slot for reproducibility.
    #[arg(long)]
    serial: bool,
    /// Execute the full build plan without writing artifacts.
    #[arg(long)]
    dry_run: bool,
    /// Artifact write mode to use when not running with --dry-run.
    #[arg(long, value_enum)]
    write_mode: Option<CliWriteMode>,
    /// Maximum generated artifact bytes allowed in the bounded write pipeline.
    #[arg(long)]
    artifact_memory_limit: Option<String>,
    /// Percentage of currently available memory for the artifact write pipeline.
    #[arg(long)]
    artifact_memory_percent: Option<u8>,
    /// Write build tracing / metrics as JSON to this path.
    #[arg(long)]
    trace_tasks: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliWriteMode {
    Direct,
    TempCommit,
}

impl From<CliWriteMode> for ArtifactWriteMode {
    fn from(mode: CliWriteMode) -> Self {
        match mode {
            CliWriteMode::Direct => Self::Direct,
            CliWriteMode::TempCommit => Self::TempCommit,
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    run_with_args(Cli::parse())
}

fn run_with_args(cli: Cli) -> anyhow::Result<()> {
    let execution = ExecutionOptions::from_cli(
        cli.jobs,
        cli.io_concurrency,
        cli.network_concurrency,
        cli.serial,
    )?;
    let artifact_write_mode = if cli.dry_run {
        ArtifactWriteMode::DryRun
    } else {
        cli.write_mode.unwrap_or(CliWriteMode::Direct).into()
    };
    let artifact_memory = ArtifactMemoryOptions::from_cli(
        cli.artifact_memory_limit.as_deref(),
        cli.artifact_memory_percent,
    )?;
    let build_result = build_target_with_trace(BuildOptions {
        project_path: &cli.project,
        target_name: &cli.target,
        extra_tags: &cli.extra_tags,
        execution,
        artifact_write_mode,
        artifact_memory,
    });
    let output = match build_result {
        Ok(output) => output,
        Err(error) => {
            write_trace_if_requested(cli.trace_tasks.as_ref(), &error.trace)?;
            return Err(error.error.into());
        }
    };
    write_trace_if_requested(cli.trace_tasks.as_ref(), &output.trace)?;
    for warning in output.warnings {
        eprintln!("warning: {warning}");
    }
    if cli.dry_run {
        println!(
            "artifact plan ({} entries):",
            output.artifacts.entries.len()
        );
        for entry in &output.artifacts.entries {
            println!(
                "  {} ({} bytes, written: {})",
                entry.path.display(),
                entry.bytes,
                entry.written
            );
        }
        println!("target `{}` dry-run succeeded", cli.target);
    } else {
        println!("target `{}` built", cli.target);
    }
    Ok(())
}

fn write_trace_if_requested(
    trace_path: Option<&PathBuf>,
    trace: &crate::BuildTrace,
) -> anyhow::Result<()> {
    if let Some(trace_path) = trace_path {
        let trace_json = serde_json::to_string_pretty(trace)?;
        fs::write(trace_path, trace_json)?;
    }
    Ok(())
}
