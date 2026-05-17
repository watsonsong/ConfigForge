use std::path::PathBuf;

use clap::Parser;

use crate::exporter::{build_target, BuildOptions, ExecutionOptions};

#[derive(Debug, Parser)]
#[command(name = "config-forge")]
#[command(about = "Validate and export designer-authored game data", long_about = None)]
pub struct Cli {
    /// Path to ConfigForge project config.
    #[arg(long)]
    project: PathBuf,
    /// Target name to build from the project config.
    #[arg(long)]
    target: String,
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
}

pub fn run() -> anyhow::Result<()> {
    run_with_args(Cli::parse())
}

pub fn run_with_args(cli: Cli) -> anyhow::Result<()> {
    let execution = ExecutionOptions::from_cli(
        cli.jobs,
        cli.io_concurrency,
        cli.network_concurrency,
        cli.serial,
    )?;
    let output = build_target(BuildOptions {
        project_path: &cli.project,
        target_name: &cli.target,
        execution,
    })?;
    for warning in output.warnings {
        eprintln!("warning: {warning}");
    }
    println!("target `{}` built", cli.target);
    Ok(())
}
