// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    artifact::{ArtifactSink, ArtifactWriteStream},
    data::DataTable,
    exporter::{
        DataFormatExporter, build_exported_table_payload, export_runtime_data_to_sink,
        runtime_table_artifacts,
    },
    meta::{LinkedTableMeta, TargetTags},
};

use super::{ArtifactGenerator, GeneratorCommonInput, GeneratorDataInput, GeneratorRequirements};

pub const FORMAT: &str = "json";

pub struct JsonDataExporter;

impl DataFormatExporter for JsonDataExporter {
    fn name(&self) -> &'static str {
        FORMAT
    }

    fn extension(&self) -> &'static str {
        "json"
    }

    fn serialize_table(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
    ) -> Result<Vec<u8>> {
        let payload = build_exported_table_payload(table_meta, table, target_tags)?;
        Ok(serde_json::to_vec_pretty(&payload)?)
    }

    fn serialize_table_to_stream(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
        stream: &mut ArtifactWriteStream,
    ) -> Result<()> {
        let payload = build_exported_table_payload(table_meta, table, target_tags)?;
        serde_json::to_writer_pretty(stream, &payload)?;
        Ok(())
    }
}

impl ArtifactGenerator for JsonDataExporter {
    fn type_id(&self) -> &'static str {
        FORMAT
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        data::DataTable,
        exporter::DataFormatExporter,
        meta::{MetaModel, TargetTags},
    };

    use super::JsonDataExporter;

    #[test]
    fn exports_struct_json_cell_as_nested_object_and_filters_children() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Reward"

                [[structs.fields]]
                name = "item_id"
                type = "string"

                [[structs.fields]]
                name = "count"
                type = "int"

                [[structs.fields]]
                name = "internal_note"
                type = "string"
                export = false

                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "reward"
                type = "Reward"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("items").expect("linked table");
        let table = DataTable::new(
            "items",
            vec![BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                (
                    "reward".to_owned(),
                    r#"{"item_id":"coin","count":100,"internal_note":"hidden"}"#.to_owned(),
                ),
            ])],
        );

        let bytes = JsonDataExporter
            .serialize_table(table_meta, &table, &TargetTags::empty())
            .expect("json should serialize");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json should decode");

        assert_eq!(value["rows"][0]["id"], "1");
        assert_eq!(value["rows"][0]["reward"]["item_id"], "coin");
        assert_eq!(value["rows"][0]["reward"]["count"], "100");
        assert!(value["rows"][0]["reward"].get("internal_note").is_none());
    }
}
