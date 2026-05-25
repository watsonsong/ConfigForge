// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

pub mod artifact;
pub mod build;
mod cli;
pub mod codegen;
pub mod data;
pub mod diagnostic;
pub mod exporter;
pub mod meta;
pub mod project;
pub mod validation;

pub mod error {
    pub use crate::diagnostic::{ConfigForgeError, Result};
}

pub use diagnostic::{ConfigForgeError, Result};

pub fn run_cli() -> anyhow::Result<()> {
    cli::run()
}
