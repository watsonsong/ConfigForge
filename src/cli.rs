use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::exporter::{export_project, validate_project, ExportOptions};

#[derive(Debug, Parser)]
#[command(name = "config-forge")]
#[command(about = "Validate and export designer-authored game data", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate source data against meta definitions.
    Validate {
        /// Path to meta TOML file.
        #[arg(long)]
        meta: PathBuf,
        /// Directory containing sheet data files.
        #[arg(long)]
        data: PathBuf,
    },
    /// Export validated data and generated code into an output directory.
    Export {
        /// Path to meta TOML file.
        #[arg(long)]
        meta: PathBuf,
        /// Directory containing sheet data files.
        #[arg(long)]
        data: PathBuf,
        /// Output directory for generated artifacts.
        #[arg(long)]
        out: PathBuf,
    },
}

pub fn run() -> anyhow::Result<()> {
    run_with_args(Cli::parse())
}

pub fn run_with_args(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Validate { meta, data } => {
            validate_project(&meta, &data)?;
            println!("validation succeeded");
        }
        Commands::Export { meta, data, out } => {
            export_project(ExportOptions {
                meta_path: &meta,
                data_dir: &data,
                out_dir: &out,
            })?;
            println!("export succeeded: {}", out.display());
        }
    }
    Ok(())
}
