// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    artifact::{ArtifactSink, ArtifactWriteStream},
    data::DataTable,
    data_artifact::{RuntimeTableFormat, export_runtime_data_to_sink, runtime_table_artifacts},
    meta::{LinkedTableMeta, TargetTags},
};

use super::{ArtifactGenerator, GeneratorCommonInput, GeneratorDataInput, GeneratorRequirements};

mod encoder;
mod wire;

#[cfg(test)]
mod tests;

use encoder::BinaryEncoder;

pub const FORMAT: &str = "bin";

pub struct BinaryDataExporter;

impl RuntimeTableFormat for BinaryDataExporter {
    fn name(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["binary"]
    }

    fn extension(&self) -> &'static str {
        "bin"
    }

    fn serialize_table(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
    ) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut encoder = BinaryEncoder::new(table_meta, target_tags, &mut bytes);
        encoder.write_table(table)?;
        Ok(bytes)
    }

    fn serialize_table_to_stream(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
        stream: &mut ArtifactWriteStream,
    ) -> Result<()> {
        BinaryEncoder::new(table_meta, target_tags, stream).write_table(table)
    }
}

impl ArtifactGenerator for BinaryDataExporter {
    fn type_id(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["binary"]
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_and_data()
    }

    fn generate_to_sink(
        &self,
        _input: &GeneratorCommonInput<'_>,
        _sink: &ArtifactSink,
    ) -> Result<()> {
        Err(ConfigForgeError::Validation(format!(
            "generator `{}` requires Data-backed artifact input",
            FORMAT
        )))
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<std::path::PathBuf>> {
        runtime_table_artifacts(&input.output, input.meta, &input.target_tags, self)
    }

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        export_runtime_data_to_sink(
            &input.common.output,
            input.common.meta,
            &input.common.target_tags,
            input.data_set,
            self,
            input.pool,
            sink,
        )
        .map(|_| ())
    }
}
