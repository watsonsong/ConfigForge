// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ConfigForgeError, Result,
    artifact::Artifact,
    data::{DataSet, DataTable},
    exporter::TargetDataConfig,
    meta::{LinkedTableMeta, TargetTags},
};

use super::{RuntimeArtifactPlan, binary, json, messagepack};
use crate::exporter::gettext_pot::GettextPotExporter;
use crate::exporter::lua_table::LuaTableExporter;

/// Runtime data format exporter.
///
/// Implementations may be shared across worker threads. `plan_table_artifact` and
/// `serialize_table` must therefore be thread-safe and reentrant; plugins with
/// internal mutable state must synchronize it themselves or use per-task factory
/// instances in a future registry model.
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

    fn serialize_table(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
    ) -> Result<Vec<u8>>;

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

    fn export(
        &self,
        out_dir: &std::path::Path,
        task: &TargetDataConfig,
        linked_meta: &crate::meta::LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
    ) -> Result<Vec<Artifact>>;

    fn matches(&self, format: &str) -> bool {
        self.name() == format || self.aliases().contains(&format)
    }
}

#[derive(Default)]
pub struct DataFormatRegistry {
    exporters: Vec<Box<dyn DataFormatExporter>>,
    aggregate_exporters: Vec<Box<dyn AggregateDataExporter>>,
}

impl DataFormatRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtin_exporters() -> Self {
        let mut registry = Self::new();
        for exporter in builtin_exporters() {
            registry.register_boxed(exporter);
        }
        for exporter in builtin_aggregate_exporters() {
            registry.register_aggregate_boxed(exporter);
        }
        registry
    }

    pub fn register(&mut self, exporter: impl DataFormatExporter + 'static) {
        self.exporters.push(Box::new(exporter));
    }

    pub fn register_boxed(&mut self, exporter: Box<dyn DataFormatExporter>) {
        self.exporters.push(exporter);
    }

    pub fn register_aggregate(&mut self, exporter: impl AggregateDataExporter + 'static) {
        self.aggregate_exporters.push(Box::new(exporter));
    }

    pub fn register_aggregate_boxed(&mut self, exporter: Box<dyn AggregateDataExporter>) {
        self.aggregate_exporters.push(exporter);
    }

    pub fn get(&self, format: &str) -> Option<&dyn DataFormatExporter> {
        self.exporters
            .iter()
            .find(|exporter| exporter.matches(format))
            .map(Box::as_ref)
    }

    pub fn require(&self, format: &str) -> Result<&dyn DataFormatExporter> {
        self.get(format).ok_or_else(|| {
            ConfigForgeError::Validation(format!("no data exporter registered for `{format}`"))
        })
    }

    pub fn get_aggregate(&self, format: &str) -> Option<&dyn AggregateDataExporter> {
        self.aggregate_exporters
            .iter()
            .find(|exporter| exporter.matches(format))
            .map(Box::as_ref)
    }

    pub fn require_any(&self, format: &str) -> Result<DataExporterRef<'_>> {
        if let Some(exporter) = self.get(format) {
            return Ok(DataExporterRef::Table(exporter));
        }
        if let Some(exporter) = self.get_aggregate(format) {
            return Ok(DataExporterRef::Aggregate(exporter));
        }
        Err(ConfigForgeError::Validation(format!(
            "no data exporter registered for `{format}`"
        )))
    }

    pub fn formats(&self) -> impl Iterator<Item = &dyn DataFormatExporter> {
        self.exporters.iter().map(Box::as_ref)
    }
}

pub enum DataExporterRef<'a> {
    Table(&'a dyn DataFormatExporter),
    Aggregate(&'a dyn AggregateDataExporter),
}

pub fn builtin_exporters() -> Vec<Box<dyn DataFormatExporter>> {
    vec![
        Box::new(json::JsonDataExporter),
        Box::new(messagepack::MessagePackDataExporter),
        Box::new(binary::BinaryDataExporter),
    ]
}

pub fn builtin_aggregate_exporters() -> Vec<Box<dyn AggregateDataExporter>> {
    vec![Box::new(GettextPotExporter), Box::new(LuaTableExporter)]
}
