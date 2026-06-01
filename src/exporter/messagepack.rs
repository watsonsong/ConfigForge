// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    artifact::ArtifactWriteStream,
    data::DataTable,
    meta::{LinkedTableMeta, TargetTags},
};

use super::{DataFormatExporter, build_exported_table_payload};

pub const FORMAT: &str = "messagepack";

pub struct MessagePackDataExporter;

impl DataFormatExporter for MessagePackDataExporter {
    fn name(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["msgpack"]
    }

    fn extension(&self) -> &'static str {
        "msgpack"
    }

    fn serialize_table(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
    ) -> Result<Vec<u8>> {
        let payload = build_exported_table_payload(table_meta, table, target_tags)?;
        rmp_serde::to_vec_named(&payload)
            .map_err(|source| ConfigForgeError::SerializeMessagePack { source })
    }

    fn serialize_table_to_stream(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
        stream: &mut ArtifactWriteStream,
    ) -> Result<()> {
        let payload = build_exported_table_payload(table_meta, table, target_tags)?;
        rmp_serde::encode::write_named(stream, &payload)
            .map_err(|source| ConfigForgeError::SerializeMessagePack { source })
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

    use super::MessagePackDataExporter;

    #[test]
    fn preserves_expanded_struct_nested_map() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Attributes"

                [[structs.fields]]
                name = "attack"
                type = "int"

                [[tables]]
                name = "characters"
                source = "characters.csv"

                [[tables.fields]]
                name = "attributes"
                type = "Attributes"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("characters").expect("linked table");
        let table = DataTable::new(
            "characters",
            vec![BTreeMap::from([(
                "attributes.attack".to_owned(),
                "10".to_owned(),
            )])],
        );

        let value = serialize_and_decode(table_meta, &table);

        assert_eq!(value["rows"][0]["attributes"]["attack"], "10");
    }

    #[test]
    fn preserves_struct_nested_map() {
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

                [[tables]]
                name = "items"
                source = "items.csv"

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
            vec![BTreeMap::from([(
                "reward".to_owned(),
                r#"{"item_id":"coin","count":100}"#.to_owned(),
            )])],
        );

        let value = serialize_and_decode(table_meta, &table);

        assert_eq!(value["rows"][0]["reward"]["item_id"], "coin");
        assert_eq!(value["rows"][0]["reward"]["count"], "100");
    }

    #[test]
    fn preserves_object_table_shape_without_collection_envelope() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "game_config"
                kind = "object"
                source = "game_config.csv"

                [[tables.fields]]
                name = "max_level"
                type = "int"

                [[tables.fields]]
                name = "pvp_enabled"
                type = "bool"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("game_config").expect("linked table");
        let table = DataTable::new(
            "game_config",
            vec![BTreeMap::from([
                ("max_level".to_owned(), "60".to_owned()),
                ("pvp_enabled".to_owned(), "true".to_owned()),
            ])],
        );

        let value = serialize_and_decode(table_meta, &table);

        assert_eq!(value["max_level"], "60");
        assert_eq!(value["pvp_enabled"], "true");
        assert!(value.get("rows").is_none());
        assert!(value.get("meta").is_none());
    }

    fn serialize_and_decode(
        table_meta: &crate::meta::LinkedTableMeta<'_>,
        table: &DataTable,
    ) -> serde_json::Value {
        let bytes = MessagePackDataExporter
            .serialize_table(table_meta, table, &TargetTags::empty())
            .expect("messagepack should serialize");
        rmp_serde::from_slice(&bytes).expect("messagepack should decode")
    }
}
