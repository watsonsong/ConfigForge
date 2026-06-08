// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ConfigForgeError, Result,
    artifact::{Artifact, ArtifactSink, ArtifactWriteStream},
    data::{DataSet, DataTable},
    data_artifact::TargetDataConfig,
    meta::{LinkedTableMeta, TargetTags},
};

use super::RuntimeArtifactPlan;

/// Runtime data format exporter.
///
/// Implementations may be shared across worker threads. `plan_table_artifact` and
/// `serialize_table_to_stream` must therefore be thread-safe and reentrant;
/// plugins with internal mutable state must synchronize it themselves or use
/// per-task factory instances in a future registry model.
pub trait DataFormatExporter: Send + Sync {
    fn name(&self) -> &'static str;

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
        Err(ConfigForgeError::Validation(format!(
            "exporter `{}` does not implement legacy serialize_table fallback",
            self.name()
        )))
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

    fn matches(&self, format: &str) -> bool {
        self.name() == format || self.aliases().contains(&format)
    }
}

pub trait AggregateDataExporter: Send + Sync {
    fn name(&self) -> &'static str;

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn plan_artifacts(
        &self,
        out_dir: &std::path::Path,
        task: &TargetDataConfig,
        linked_meta: &crate::meta::LinkedMetaModel<'_>,
        target_tags: &TargetTags,
    ) -> Result<Vec<PathBuf>>;

    #[allow(dead_code)]
    fn export(
        &self,
        out_dir: &std::path::Path,
        task: &TargetDataConfig,
        linked_meta: &crate::meta::LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
    ) -> Result<Vec<Artifact>> {
        let _ = (out_dir, task, linked_meta, target_tags, data_set);
        Err(ConfigForgeError::Validation(format!(
            "exporter `{}` does not implement legacy aggregate export fallback",
            self.name()
        )))
    }

    fn export_to_sink(
        &self,
        out_dir: &std::path::Path,
        task: &TargetDataConfig,
        linked_meta: &crate::meta::LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
        sink: &ArtifactSink,
    ) -> Result<()>;
}
