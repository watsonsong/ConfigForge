// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};

use tempfile::tempdir;

use crate::{
    artifact::{ArtifactWriteMode, ArtifactWriter},
    data::DataTable,
    meta::{LinkedTableMeta, MetaModel, TargetTags},
    ConfigForgeError, Result,
};

use super::*;

struct CustomDataExporter {
    seen_tables: Mutex<Vec<String>>,
}

fn export_runtime_data_to_disk(
    out_dir: &std::path::Path,
    linked_meta: &crate::meta::LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &crate::data::DataSet,
    exporter: &dyn DataFormatExporter,
    pool: &rayon::ThreadPool,
) -> Result<Vec<RuntimeDataFile>> {
    let artifacts =
        export_runtime_data(out_dir, linked_meta, target_tags, data_set, exporter, pool)?;
    let files = artifacts
        .iter()
        .map(|artifact| artifact.file.clone())
        .collect::<Vec<_>>();
    ArtifactWriter::new(ArtifactWriteMode::Direct).write(
        artifacts
            .into_iter()
            .map(|artifact| artifact.artifact)
            .collect(),
    )?;
    Ok(files)
}

impl DataFormatExporter for CustomDataExporter {
    fn name(&self) -> &'static str {
        "custom"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["custom_alias"]
    }

    fn extension(&self) -> &'static str {
        "custom"
    }

    fn plan_table_artifact(&self, table_meta: &LinkedTableMeta<'_>) -> Result<RuntimeArtifactPlan> {
        Ok(RuntimeArtifactPlan {
            table_name: table_meta.meta.name.clone(),
            path: PathBuf::from("custom").join(format!("{}.custom", table_meta.meta.name)),
        })
    }

    fn serialize_table(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        _table: &DataTable,
        _target_tags: &TargetTags,
    ) -> Result<Vec<u8>> {
        self.seen_tables
            .lock()
            .expect("seen tables lock should not be poisoned")
            .push(table_meta.meta.name.clone());
        Ok(table_meta.meta.name.as_bytes().to_vec())
    }
}

#[test]
fn registry_resolves_builtin_exporters_and_aliases() {
    let registry = DataFormatRegistry::with_builtin_exporters();

    assert_eq!(registry.require("json").unwrap().extension(), "json");
    assert_eq!(
        registry.require("messagepack").unwrap().extension(),
        "msgpack"
    );
    assert_eq!(registry.require("msgpack").unwrap().extension(), "msgpack");
    assert_eq!(registry.require("bin").unwrap().extension(), "bin");
    assert_eq!(registry.require("binary").unwrap().extension(), "bin");
    assert!(registry.require("unknown").is_err());
}

#[test]
fn registry_accepts_custom_exporter_plugins() {
    let mut registry = DataFormatRegistry::new();
    registry.register(CustomDataExporter {
        seen_tables: Mutex::new(Vec::new()),
    });

    assert_eq!(registry.require("custom").unwrap().extension(), "custom");
    assert_eq!(
        registry.require("custom_alias").unwrap().extension(),
        "custom"
    );
}

#[test]
fn export_runtime_data_uses_registered_exporter() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = crate::data::DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
    )]);
    let out_dir = tempdir().expect("temp dir should exist");
    let exporter = CustomDataExporter {
        seen_tables: Mutex::new(Vec::new()),
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("test thread pool should build");

    let files = export_runtime_data_to_disk(
        out_dir.path(),
        &linked,
        &TargetTags::empty(),
        &data_set,
        &exporter,
        &pool,
    )
    .expect("runtime data should export");

    assert_eq!(files[0].path, "custom/items.custom");
    assert_eq!(
        exporter
            .seen_tables
            .into_inner()
            .expect("seen tables lock should not be poisoned"),
        vec!["items"]
    );
    assert!(out_dir.path().join("custom/items.custom").exists());
}

#[test]
fn binary_exporter_writes_header_row_count_bitmap_and_values() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[enums]]
                name = "Rarity"

                [[enums.values]]
                name = "common"

                [[enums.values]]
                name = "rare"

                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "enabled"
                type = "bool"
                required = true

                [[tables.fields]]
                name = "name"
                type = "string"
                required = false

                [[tables.fields]]
                name = "rarity"
                type = "Rarity"
                required = true
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("items").expect("items table should exist");
    let data_table = DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "-7".to_owned()),
            ("enabled".to_owned(), "true".to_owned()),
            ("name".to_owned(), "Potion".to_owned()),
            ("rarity".to_owned(), "rare".to_owned()),
        ])],
    );

    let bytes = BinaryDataExporter
        .serialize_table(table_meta, &data_table, &TargetTags::empty())
        .expect("binary data should serialize");

    assert_eq!(&bytes[0..4], b"CFGB");
    assert_eq!(u16::from_le_bytes(bytes[4..6].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(bytes[6..8].try_into().unwrap()), 16);
    assert_eq!(
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        binary_schema_revision(table_meta, &TargetTags::empty())
    );

    let mut offset = 16;
    assert_eq!(read_varuint_for_test(&bytes, &mut offset), 1);
    assert_eq!(bytes[offset], 0b0000_0001);
    offset += 1;
    assert_eq!(read_i64_for_test(&bytes, &mut offset), -7);
    assert_eq!(bytes[offset], 1);
    offset += 1;
    let name_len = read_varuint_for_test(&bytes, &mut offset) as usize;
    assert_eq!(&bytes[offset..offset + name_len], b"Potion");
    offset += name_len;
    assert_eq!(read_varuint_for_test(&bytes, &mut offset), 1);
    assert_eq!(offset, bytes.len());
}

#[test]
fn binary_exporter_rejects_int_outside_javascript_safe_range() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("items").expect("items table should exist");
    let data_table = DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "id".to_owned(),
            "9007199254740992".to_owned(),
        )])],
    );

    let error = BinaryDataExporter
        .serialize_table(table_meta, &data_table, &TargetTags::empty())
        .expect_err("unsafe JavaScript int should be rejected for bin export");

    assert!(matches!(
        error,
        ConfigForgeError::Validation(message)
            if message.contains("JavaScript safe integer range") && message.contains("id")
    ));
}

#[test]
fn binary_exporter_omits_absent_optional_fields() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "subtitle"
                type = "string"
                required = false
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("items").expect("items table should exist");
    let data_table = DataTable::new(
        "items",
        vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
    );

    let bytes = BinaryDataExporter
        .serialize_table(table_meta, &data_table, &TargetTags::empty())
        .expect("binary data should serialize with absent optional fields");

    assert_eq!(bytes[16], 1); // row_count
    assert_eq!(bytes[17], 0); // subtitle optional absent
    assert_eq!(bytes[18], 2); // zigzag(1)
    assert_eq!(bytes.len(), 19);
}

struct ConflictDataExporter;

impl DataFormatExporter for ConflictDataExporter {
    fn name(&self) -> &'static str {
        "conflict"
    }

    fn extension(&self) -> &'static str {
        "conflict"
    }

    fn plan_table_artifact(&self, table_meta: &LinkedTableMeta<'_>) -> Result<RuntimeArtifactPlan> {
        Ok(RuntimeArtifactPlan {
            table_name: table_meta.meta.name.clone(),
            path: PathBuf::from("same.conflict"),
        })
    }

    fn serialize_table(
        &self,
        _table_meta: &LinkedTableMeta<'_>,
        _table: &DataTable,
        _target_tags: &TargetTags,
    ) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

#[test]
fn export_runtime_data_preserves_plan_order_for_multiple_artifacts() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "levels"
                source = "levels.csv"
                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "quests"
                source = "quests.csv"
                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = crate::data::DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "levels",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "quests",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
    ]);
    let out_dir = tempdir().expect("temp dir should exist");
    let exporter = CustomDataExporter {
        seen_tables: Mutex::new(Vec::new()),
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("test thread pool should build");

    let files = export_runtime_data_to_disk(
        out_dir.path(),
        &linked,
        &TargetTags::empty(),
        &data_set,
        &exporter,
        &pool,
    )
    .expect("runtime data should export");

    assert_eq!(files[0].table_name, "items");
    assert_eq!(files[1].table_name, "levels");
    assert_eq!(files[2].table_name, "quests");
    assert_eq!(files[0].path, "custom/items.custom");
    assert_eq!(files[1].path, "custom/levels.custom");
    assert_eq!(files[2].path, "custom/quests.custom");
}

#[test]
fn export_runtime_data_rejects_duplicate_artifact_paths_before_writing() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "levels"
                source = "levels.csv"
                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = crate::data::DataSet::new(vec![
        DataTable::new("items", Vec::new()),
        DataTable::new("levels", Vec::new()),
    ]);
    let out_dir = tempdir().expect("temp dir should exist");
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("test thread pool should build");

    let error = export_runtime_data(
        out_dir.path(),
        &linked,
        &TargetTags::empty(),
        &data_set,
        &ConflictDataExporter,
        &pool,
    )
    .unwrap_err();

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("multiple runtime data artifacts target `same.conflict`"))
    );
    assert!(!out_dir.path().join("same.conflict").exists());
}

#[test]
fn row_export_filter_fields_control_rows_without_runtime_output() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                row_export = { excluded_tags_field = "exclude_tags" }

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "exclude_tags"
                type = "string"
                export = false
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("items").expect("linked table");
    let table = DataTable::new(
        "items",
        vec![
            BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                ("exclude_tags".to_owned(), String::new()),
            ]),
            BTreeMap::from([
                ("id".to_owned(), "bad".to_owned()),
                ("exclude_tags".to_owned(), "client".to_owned()),
            ]),
        ],
    );

    let payload = build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
        .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");

    let rows = value["rows"].as_array().expect("rows should be array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "1");
    assert!(rows[0].get("exclude_tags").is_none());
}

#[test]
fn struct_expanded_value_exports_as_nested_object_for_collection_and_object() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[structs]]
                name = "Attributes"

                [[structs.fields]]
                name = "attack"
                type = "int"

                [[structs.fields]]
                name = "max_health"
                type = "int"
                default = 100

                [[tables]]
                name = "characters"
                source = "characters.csv"

                [[tables.fields]]
                name = "id"
                type = "string"

                [[tables.fields]]
                name = "attributes"
                type = "Attributes"

                [[tables]]
                name = "game_rules"
                kind = "object"
                source = "game_rules.csv"

                [[tables.fields]]
                name = "default_attributes"
                type = "Attributes"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let characters = DataTable::new(
        "characters",
        vec![BTreeMap::from([
            ("id".to_owned(), "hero".to_owned()),
            ("attributes.attack".to_owned(), "10".to_owned()),
        ])],
    );
    let payload = build_exported_table_payload(
        linked.table("characters").expect("linked table"),
        &characters,
        &TargetTags::empty(),
    )
    .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");
    assert_eq!(value["rows"][0]["attributes"]["attack"], "10");
    assert_eq!(value["rows"][0]["attributes"]["max_health"], "100");

    let rules = DataTable::new(
        "game_rules",
        vec![BTreeMap::from([
            ("default_attributes.attack".to_owned(), "5".to_owned()),
            ("default_attributes.max_health".to_owned(), "80".to_owned()),
        ])],
    );
    let payload = build_exported_table_payload(
        linked.table("game_rules").expect("linked table"),
        &rules,
        &TargetTags::empty(),
    )
    .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");
    assert_eq!(value["default_attributes"]["attack"], "5");
    assert_eq!(value["default_attributes"]["max_health"], "80");
}

#[test]
fn struct_expanded_value_exports_defaults_and_omits_empty_optional_occurrence() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[structs]]
                name = "Attributes"

                [[structs.fields]]
                name = "attack"
                type = "int"

                [[structs.fields]]
                name = "max_health"
                type = "int"
                default = 100

                [[structs]]
                name = "PlainReward"

                [[structs.fields]]
                name = "item_id"
                type = "string"

                [[tables]]
                name = "characters"
                source = "characters.csv"

                [[tables.fields]]
                name = "required_attributes"
                type = "Attributes"
                required = true

                [[tables.fields]]
                name = "optional_attributes"
                type = "Attributes"

                [[tables.fields]]
                name = "optional_reward"
                type = "PlainReward"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("characters").expect("linked table");
    let table = DataTable::new(
        "characters",
        vec![BTreeMap::from([
            ("optional_attributes.attack".to_owned(), String::new()),
            ("optional_attributes.max_health".to_owned(), String::new()),
            ("optional_reward.item_id".to_owned(), String::new()),
        ])],
    );

    let payload = build_exported_table_payload(table_meta, &table, &TargetTags::empty())
        .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");

    assert_eq!(value["rows"][0]["required_attributes"]["max_health"], "100");
    assert_eq!(value["rows"][0]["optional_attributes"]["max_health"], "100");
    assert!(value["rows"][0]["optional_attributes"]
        .get("attack")
        .is_none());
    assert!(value["rows"][0].get("optional_reward").is_none());
}

#[test]
fn object_table_exports_single_object_and_filters_fields() {
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
                export_when = { any = ["server"] }

                [[tables]]
                name = "game_config"
                kind = "object"
                source = "game_config.csv"

                [[tables.fields]]
                name = "max_level"
                type = "int"

                [[tables.fields]]
                name = "start_gold"
                type = "int"
                export_when = { any = ["server"] }

                [[tables.fields]]
                name = "reward"
                type = "Reward"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("game_config").expect("linked table");
    let table = DataTable::new(
        "game_config",
        vec![BTreeMap::from([
            ("max_level".to_owned(), "100".to_owned()),
            ("start_gold".to_owned(), "500".to_owned()),
            (
                "reward".to_owned(),
                r#"{"item_id":"coin","count":10}"#.to_owned(),
            ),
        ])],
    );

    let payload = build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
        .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");

    assert_eq!(value["max_level"], "100");
    assert!(value.get("start_gold").is_none());
    assert_eq!(value["reward"]["item_id"], "coin");
    assert!(value["reward"].get("count").is_none());
    assert!(value.get("rows").is_none());
    assert!(value.get("meta").is_none());
}

#[test]
fn object_table_only_header_exports_empty_or_default_object() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "game_config"
                kind = "object"
                source = "game_config.csv"

                [[tables.fields]]
                name = "max_level"
                type = "int"
                default = 100
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("game_config").expect("linked table");
    let table = crate::validation::normalize_table(
        table_meta,
        &DataTable::new("game_config", vec![BTreeMap::new()]),
    )
    .expect("defaults should apply");

    let payload = build_exported_table_payload(table_meta, &table, &TargetTags::empty())
        .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");

    assert_eq!(value["max_level"], "100");

    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "empty_config"
                kind = "object"
                source = "empty_config.csv"

                [[tables.fields]]
                name = "optional_note"
                type = "string"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("empty_config").expect("linked table");
    let payload = build_exported_table_payload(
        table_meta,
        &DataTable::new("empty_config", vec![BTreeMap::new()]),
        &TargetTags::empty(),
    )
    .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");
    assert!(value
        .as_object()
        .expect("payload should be object")
        .is_empty());
}

#[test]
fn object_table_exports_empty_object_when_all_fields_hidden_for_target() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "game_config"
                kind = "object"
                source = "game_config.csv"

                [[tables.fields]]
                name = "server_only"
                type = "string"
                export_when = { any = ["server"] }
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("game_config").expect("linked table");
    let table = DataTable::new(
        "game_config",
        vec![BTreeMap::from([(
            "server_only".to_owned(),
            "hidden".to_owned(),
        )])],
    );

    let payload = build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
        .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");

    assert!(value
        .as_object()
        .expect("payload should be object")
        .is_empty());
}

#[test]
fn fragment_export_uses_nested_objects_and_presence() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                polymorphic = true
                sources = [
                    { provider = "csv", path = "weapons.csv", fragments = ["weapon"] },
                    { provider = "csv", path = "armors.csv", fragments = ["armor"] },
                ]

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fragments]]
                name = "weapon"

                [[tables.fragments.fields]]
                name = "power"
                type = "int"

                [[tables.fragments.fields]]
                name = "server_note"
                type = "string"
                export_when = { any = ["server"] }

                [[tables.fragments]]
                name = "armor"

                [[tables.fragments.fields]]
                name = "power"
                type = "int"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("items").expect("linked table");
    let table = DataTable::new_with_sources(
        "items",
        vec![
            BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                ("weapon.power".to_owned(), "10".to_owned()),
                ("weapon.server_note".to_owned(), "hidden".to_owned()),
            ]),
            BTreeMap::from([
                ("id".to_owned(), "2".to_owned()),
                ("armor.power".to_owned(), "20".to_owned()),
            ]),
        ],
        vec![
            crate::data::RowSourceInfo {
                source_index: 0,
                source_path: PathBuf::from("weapons.csv"),
                source_row_number: 1,
                fragments: vec!["weapon".to_owned()],
            },
            crate::data::RowSourceInfo {
                source_index: 1,
                source_path: PathBuf::from("armors.csv"),
                source_row_number: 1,
                fragments: vec!["armor".to_owned()],
            },
        ],
    )
    .expect("table should build");

    let payload = build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
        .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");

    assert_eq!(value["rows"][0]["id"], "1");
    assert_eq!(value["rows"][0]["weapon"]["power"], "10");
    assert!(value["rows"][0]["weapon"].get("server_note").is_none());
    assert!(value["rows"][0].get("armor").is_none());
    assert_eq!(value["rows"][1]["armor"]["power"], "20");
    assert!(value["rows"][1].get("weapon").is_none());
}

#[test]
fn fragment_export_locks_empty_and_hidden_fragment_presence() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[structs]]
                name = "AttributeSet"

                [[structs.fields]]
                name = "server_power"
                type = "int"
                export_when = { any = ["server"] }

                [[tables]]
                name = "items"
                source = "items.csv"
                polymorphic = true

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fragments]]
                name = "client_marker"
                export_when = { any = ["client"] }

                [[tables.fragments.fields]]
                name = "server_note"
                type = "string"
                export_when = { any = ["server"] }

                [[tables.fragments]]
                name = "server_marker"
                export_when = { any = ["server"] }

                [[tables.fragments.fields]]
                name = "value"
                type = "int"

                [[tables.fragments]]
                name = "typed_marker"
                type = "AttributeSet"
                export_when = { any = ["client"] }
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("items").expect("linked table");
    let table = DataTable::new_with_sources(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "1".to_owned()),
            ("client_marker.server_note".to_owned(), "hidden".to_owned()),
            ("server_marker.value".to_owned(), "10".to_owned()),
        ])],
        vec![crate::data::RowSourceInfo {
            source_index: 0,
            source_path: PathBuf::from("items.csv"),
            source_row_number: 1,
            fragments: vec![
                "client_marker".to_owned(),
                "server_marker".to_owned(),
                "typed_marker".to_owned(),
            ],
        }],
    )
    .expect("table should build");

    let payload = build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
        .expect("payload should build");
    let value = serde_json::to_value(payload).expect("payload should serialize");

    assert!(value["rows"][0]["client_marker"]
        .as_object()
        .expect("client marker should be object")
        .is_empty());
    assert!(value["rows"][0]["typed_marker"]
        .as_object()
        .expect("typed marker should be object")
        .is_empty());
    assert!(value["rows"][0].get("server_marker").is_none());
}

#[test]
fn struct_json_cell_export_errors_include_row_and_field_context() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[structs]]
                name = "Reward"

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
    let invalid_json_table = DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward".to_owned(),
            "not json".to_owned(),
        )])],
    );

    let error = build_exported_table_payload(table_meta, &invalid_json_table, &TargetTags::empty())
        .unwrap_err();
    assert!(
        matches!(&error, ConfigForgeError::Validation(message) if message.contains("table `items` row 1 field `reward` has invalid struct JSON"))
    );

    let unknown_key_table = DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward".to_owned(),
            r#"{"count":1,"extra":true}"#.to_owned(),
        )])],
    );
    let error = build_exported_table_payload(table_meta, &unknown_key_table, &TargetTags::empty())
        .unwrap_err();
    assert!(
        matches!(&error, ConfigForgeError::Validation(message) if message.contains("field `reward` contains unknown struct key `extra`"))
    );
}

fn read_i64_for_test(bytes: &[u8], offset: &mut usize) -> i64 {
    let encoded = read_varuint_for_test(bytes, offset);
    ((encoded >> 1) as i64) ^ (-((encoded & 1) as i64))
}

fn read_varuint_for_test(bytes: &[u8], offset: &mut usize) -> u64 {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = bytes[*offset];
        *offset += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if (byte & 0x80) == 0 {
            return value;
        }
        shift += 7;
    }
}
