// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::PathBuf};

use clap::{Parser, ValueEnum};

use crate::{
    ConfigForgeError,
    artifact::ArtifactWriteMode,
    build::{BuildOptions, ExecutionOptions, build_target_with_trace},
    validation::{DesignerReportFormat, ValidationReportExportConfig},
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
    /// Write build tracing / metrics as JSON to this path.
    #[arg(long)]
    trace_tasks: Option<PathBuf>,
    /// Export a human-readable validation report in the selected format.
    #[arg(long, value_enum)]
    report: Option<CliReportFormat>,
    /// Output path for --report.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Optional title for the exported validation report.
    #[arg(long)]
    report_title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliWriteMode {
    Direct,
    TempCommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliReportFormat {
    Html,
    Markdown,
    Json,
    Csv,
}

impl From<CliReportFormat> for DesignerReportFormat {
    fn from(format: CliReportFormat) -> Self {
        match format {
            CliReportFormat::Html => Self::Html,
            CliReportFormat::Markdown => Self::Markdown,
            CliReportFormat::Json => Self::Json,
            CliReportFormat::Csv => Self::Csv,
        }
    }
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
    let validation_report = match cli.report {
        Some(format) => Some(ValidationReportExportConfig {
            format: format.into(),
            output: cli.output.clone().ok_or_else(|| {
                ConfigForgeError::Validation(
                    "--output must be set when --report is used".to_owned(),
                )
            })?,
            title: cli.report_title.clone(),
        }),
        None => {
            if cli.output.is_some() || cli.report_title.is_some() {
                return Err(ConfigForgeError::Validation(
                    "--output and --report-title require --report".to_owned(),
                )
                .into());
            }
            None
        }
    };
    let build_result = build_target_with_trace(BuildOptions {
        project_path: &cli.project,
        target_name: &cli.target,
        extra_tags: &cli.extra_tags,
        execution,
        artifact_write_mode,
        validation_report,
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
