// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod binary;
mod config;
mod json;
mod messagepack;
mod model;
mod payload;
mod registry;
mod runtime;

#[cfg(test)]
mod tests;

pub use binary::BinaryDataExporter;
pub use config::TargetDataConfig;
pub use json::JsonDataExporter;
pub use messagepack::MessagePackDataExporter;
pub use model::{ExportedTable, ExportedTablePayload, ExportedValue};
pub use payload::build_exported_table_payload;
pub use registry::{DataFormatExporter, DataFormatRegistry, builtin_exporters};
pub use runtime::{RuntimeArtifactPlan, RuntimeDataFile};

pub(crate) use binary::{FORMAT as BINARY_FORMAT, schema_revision as binary_schema_revision};
pub(crate) use json::FORMAT as JSON_FORMAT;
pub(crate) use messagepack::FORMAT as MESSAGEPACK_FORMAT;
pub(crate) use runtime::export_runtime_data;
