// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod config;
mod model;
mod payload;
mod registry;
mod runtime;

#[cfg(test)]
mod tests;

pub(crate) use config::TargetDataConfig;
pub use model::{ExportedTable, ExportedTablePayload, ExportedValue};
pub(crate) use payload::build_exported_table_payload;
pub(crate) use registry::{AggregateDataExporter, RuntimeTableFormat};
pub use runtime::RuntimeArtifactPlan;

pub(crate) use runtime::{export_runtime_data_to_sink, runtime_table_artifacts};
