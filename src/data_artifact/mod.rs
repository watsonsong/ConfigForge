// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod config;
mod model;
mod payload;
mod registry;
mod runtime;

#[cfg(test)]
mod tests;

pub use config::TargetDataConfig;
pub use model::{ExportedTable, ExportedTablePayload, ExportedValue};
pub use payload::build_exported_table_payload;
pub use registry::{AggregateDataExporter, DataFormatExporter};
pub use runtime::RuntimeArtifactPlan;

pub(crate) use runtime::{export_runtime_data_to_sink, runtime_table_artifacts};
