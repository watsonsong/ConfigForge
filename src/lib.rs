// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

pub mod artifact;
pub mod build;
pub mod cli;
pub mod codegen;
pub mod data;
pub mod diagnostic;
pub mod exporter;
pub mod meta;
pub mod project;
pub mod validation;

pub mod error {
    pub use crate::diagnostic::error::{ConfigForgeError, Result};
}

pub use diagnostic::{ConfigForgeError, Result};
