// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    artifact::ArtifactWriteStream,
    data::DataTable,
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    meta::{LinkedTableMeta, TargetTags},
};

use super::RuntimeArtifactPlan;

/// Runtime table serialization format.
///
/// Implementations may be shared across worker threads. `plan_table_artifact` and
/// `serialize_table_to_stream` must therefore be thread-safe and reentrant;
/// plugins with internal mutable state must synchronize it themselves or use
/// per-task factory instances in a future registry model.
pub trait RuntimeTableFormat: Send + Sync {
    fn name(&self) -> &'static str;

    #[allow(dead_code)]
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn extension(&self) -> &'static str;

    fn plan_table_artifact(&self, table_meta: &LinkedTableMeta<'_>) -> Result<RuntimeArtifactPlan> {
        Ok(RuntimeArtifactPlan {
            table_name: table_meta.meta.name.clone(),
            path: PathBuf::from(format!("{}.{}", table_meta.meta.name, self.extension())),
        })
    }

    /// Legacy compatibility API for callers that still need a materialized
    /// payload. New exporters should implement `serialize_table_to_stream` and
    /// avoid building a full `Vec<u8>` when writing final artifacts.
    fn serialize_table(
        &self,
        _table_meta: &LinkedTableMeta<'_>,
        _table: &DataTable,
        _target_tags: &TargetTags,
    ) -> Result<Vec<u8>> {
        Err(data_exporter_contract_error(
            self.name(),
            format!(
                "exporter `{}` does not implement legacy serialize_table fallback",
                self.name()
            ),
            "legacy serialize_table fallback missing",
            "serialize_table or serialize_table_to_stream implementation",
        ))
    }

    /// Primary artifact serialization API. Runtime table export uses this path
    /// directly after reserving the output path in `ArtifactSink`.
    fn serialize_table_to_stream(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
        stream: &mut ArtifactWriteStream,
    ) -> Result<()> {
        let bytes = self.serialize_table(table_meta, table, target_tags)?;
        stream.write_all_artifact(&bytes)
    }

    #[allow(dead_code)]
    fn matches(&self, format: &str) -> bool {
        self.name() == format || self.aliases().contains(&format)
    }
}

fn data_exporter_contract_error(
    exporter: &str,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(format!("generator.{exporter}.registry"))
        .with_domain("generator")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint("Implement the streaming export API for runtime data generators."),
    )
}
