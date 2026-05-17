use std::path::PathBuf;

use clap::Parser;

use crate::exporter::{build_target, BuildOptions};

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
}

pub fn run() -> anyhow::Result<()> {
    run_with_args(Cli::parse())
}

pub fn run_with_args(cli: Cli) -> anyhow::Result<()> {
    let output = build_target(BuildOptions {
        project_path: &cli.project,
        target_name: &cli.target,
    })?;
    for warning in output.warnings {
        eprintln!("warning: {warning}");
    }
    println!("target `{}` built", cli.target);
    Ok(())
}
