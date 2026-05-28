// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use crate::data::RowSourceInfo;

use crate::meta::{
    EnumMeta, EnumValueMeta, ExportCondition, FieldMeta, LinkedField, LinkedFieldMeta,
    LinkedTableMeta, MetaModel, PrimitiveFieldType, TableMeta, TargetTags, ValidationSeverity,
};
use crate::{ConfigForgeError, Result};

use super::*;

fn validate_error(result: Result<()>) -> String {
    match result.unwrap_err() {
        ConfigForgeError::Validation(message) => message,
        ConfigForgeError::ValidationReport(report) => report.error_messages().join("; "),
        error => panic!("expected validation error, got {error}"),
    }
}

fn fragment_row_source(source_row_number: usize, fragments: &[&str]) -> RowSourceInfo {
    RowSourceInfo {
        source_index: 0,
        source_path: PathBuf::from("items.csv"),
        source_row_number,
        fragments: fragments
            .iter()
            .map(|fragment| (*fragment).to_owned())
            .collect(),
    }
}

#[test]
fn rejects_invalid_type() {
    let meta = TableMeta {
        name: "items".to_owned(),
        kind: crate::meta::TableKind::Collection,
        desc: None,
        export: true,
        export_when: ExportCondition::default(),
        row_export: None,
        validators: vec![],
        source: Some("items.csv".to_owned()),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "id".to_owned(),
            desc: None,
            field_type: "int".to_owned(),
            required: true,
            default: None,
            layout: None,
            export: true,
            export_when: ExportCondition::default(),
            validators: vec![],
            tool_only: None,
            filter_field: None,
            validator_only: None,
            field_number: None,
            unique: true,
            foreign_key: None,
        }],
        fragments: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        primary_index: None,
        preserve_source_order: false,
        polymorphic: false,
    };
    let linked_meta = LinkedTableMeta::new(
        &meta,
        vec![LinkedFieldMeta {
            meta: &meta.fields[0],
            linked_field: LinkedField::Primitive(PrimitiveFieldType::Int),
        }],
        vec![],
        vec![],
        vec![],
    );
    let table = DataTable::new(
        "items",
        vec![BTreeMap::from([("id".to_owned(), "abc".to_owned())])],
    );

    assert!(validate_table(&linked_meta, &table).is_err());
}

#[test]
fn applies_defaults_before_required_and_type_validation() {
    let meta = TableMeta {
        name: "items".to_owned(),
        kind: crate::meta::TableKind::Collection,
        desc: None,
        export: true,
        export_when: ExportCondition::default(),
        row_export: None,
        validators: vec![],
        source: Some("items.csv".to_owned()),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "max_stack".to_owned(),
            desc: None,
            field_type: "int".to_owned(),
            required: true,
            default: Some(toml::Value::Integer(1)),
            layout: None,
            export: true,
            export_when: ExportCondition::default(),
            validators: vec![],
            tool_only: None,
            filter_field: None,
            validator_only: None,
            field_number: None,
            unique: false,
            foreign_key: None,
        }],
        fragments: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        primary_index: None,
        preserve_source_order: false,
        polymorphic: false,
    };
    let linked_meta = LinkedTableMeta::new(
        &meta,
        vec![LinkedFieldMeta {
            meta: &meta.fields[0],
            linked_field: LinkedField::Primitive(PrimitiveFieldType::Int),
        }],
        vec![],
        vec![],
        vec![],
    );
    let table = DataTable::new("items", vec![BTreeMap::new()]);

    assert!(validate_table(&linked_meta, &table).is_ok());
    let table = apply_field_defaults(&linked_meta, &table).unwrap();
    assert_eq!(
        table.rows[0].get("max_stack").map(String::as_str),
        Some("1")
    );
}

#[test]
fn allows_missing_optional_field_without_default() {
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
                name = "note"
                type = "string"
                required = false
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn validates_enum_values() {
    let enum_meta = EnumMeta {
        name: "ItemRarity".to_owned(),
        desc: None,
        values: vec![EnumValueMeta {
            name: "common".to_owned(),
            desc: None,
        }],
    };
    let meta = TableMeta {
        name: "items".to_owned(),
        kind: crate::meta::TableKind::Collection,
        desc: None,
        export: true,
        export_when: ExportCondition::default(),
        row_export: None,
        validators: vec![],
        source: Some("items.csv".to_owned()),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "rarity".to_owned(),
            desc: None,
            field_type: "ItemRarity".to_owned(),
            required: true,
            default: None,
            layout: None,
            export: true,
            export_when: ExportCondition::default(),
            validators: vec![],
            tool_only: None,
            filter_field: None,
            validator_only: None,
            field_number: None,
            unique: false,
            foreign_key: None,
        }],
        fragments: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        primary_index: None,
        preserve_source_order: false,
        polymorphic: false,
    };
    let linked_meta = LinkedTableMeta::new(
        &meta,
        vec![LinkedFieldMeta {
            meta: &meta.fields[0],
            linked_field: LinkedField::Enum(&enum_meta),
        }],
        vec![],
        vec![],
        vec![],
    );
    let valid_table = DataTable::new(
        "items",
        vec![BTreeMap::from([("rarity".to_owned(), "common".to_owned())])],
    );
    let invalid_table = DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "rarity".to_owned(),
            "legendary".to_owned(),
        )])],
    );

    assert!(validate_table(&linked_meta, &valid_table).is_ok());
    assert!(validate_table(&linked_meta, &invalid_table).is_err());
}

#[test]
fn validates_fragment_scoped_fields_and_defaults() {
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
            required = true

            [[tables.fragments]]
            name = "weapon"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
            required = true
            default = 1

            [[tables.fragments]]
            name = "armor"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let table_meta = linked_meta.table("items").expect("linked table");
    let table = DataTable::new_with_sources(
        "items",
        vec![
            BTreeMap::from([("id".to_owned(), "1".to_owned())]),
            BTreeMap::from([
                ("id".to_owned(), "2".to_owned()),
                ("armor.power".to_owned(), "10".to_owned()),
            ]),
        ],
        vec![
            RowSourceInfo {
                source_index: 0,
                source_path: PathBuf::from("weapons.csv"),
                source_row_number: 1,
                fragments: vec!["weapon".to_owned()],
            },
            RowSourceInfo {
                source_index: 1,
                source_path: PathBuf::from("armors.csv"),
                source_row_number: 1,
                fragments: vec!["armor".to_owned()],
            },
        ],
    )
    .expect("table should build");

    assert!(validate_table(table_meta, &table).is_ok());
    let normalized = apply_field_defaults(table_meta, &table).expect("defaults should apply");
    assert_eq!(
        normalized.rows[0].get("weapon.power").map(String::as_str),
        Some("1")
    );

    let inactive_fragment_table = DataTable::new_with_sources(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "1".to_owned()),
            ("armor.power".to_owned(), "10".to_owned()),
        ])],
        vec![RowSourceInfo {
            source_index: 0,
            source_path: PathBuf::from("weapons.csv"),
            source_row_number: 1,
            fragments: vec!["weapon".to_owned()],
        }],
    )
    .expect("table should build");
    let error = validate_table(table_meta, &inactive_fragment_table).unwrap_err();
    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("inactive fragment `armor`"))
    );
}

fn foreign_key_meta() -> MetaModel {
    MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            required = true
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load")
}

fn foreign_key_data(drop_item_id: &str) -> DataSet {
    DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([
                ("id".to_owned(), "100".to_owned()),
                ("drop_item_id".to_owned(), drop_item_id.to_owned()),
            ])],
        ),
    ])
}

#[test]
fn validates_foreign_key_reference_integrity() {
    let meta = foreign_key_meta();
    let linked_meta = meta.link().expect("meta should link");

    assert!(validate_data_set(&linked_meta, &foreign_key_data("1")).is_ok());

    let message = validate_error(validate_data_set(&linked_meta, &foreign_key_data("2")));
    assert!(message.contains("foreign key `drop_item` references missing `items.by_id` key"));
}

#[test]
fn uses_raw_string_foreign_key_semantics() {
    let meta = foreign_key_meta();
    let linked_meta = meta.link().expect("meta should link");

    let message = validate_error(validate_data_set(&linked_meta, &foreign_key_data("01")));
    assert!(message.contains("references missing `items.by_id` key `[\"01\"]`"));
}

#[test]
fn handles_foreign_key_empty_and_partial_keys() {
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
            name = "region"
            type = "string"
            required = true

            [[tables.indexes]]
            name = "by_id_region"
            fields = ["id", "region"]
            unique = true

            [[tables]]
            name = "shops"
            source = "shops.csv"

            [[tables.fields]]
            name = "item_id"
            type = "int"
            required = false

            [[tables.fields]]
            name = "region"
            type = "string"
            required = false

            [[tables.foreign_keys]]
            name = "item_region"
            fields = ["item_id", "region"]
            target_table = "items"
            target_index = "by_id_region"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let items = DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "1".to_owned()),
            ("region".to_owned(), "cn".to_owned()),
        ])],
    );

    let empty = DataSet::new(vec![
        items.clone(),
        DataTable::new("shops", vec![BTreeMap::new()]),
    ]);
    assert!(validate_data_set(&linked_meta, &empty).is_ok());

    let partial = DataSet::new(vec![
        items,
        DataTable::new(
            "shops",
            vec![BTreeMap::from([("item_id".to_owned(), "1".to_owned())])],
        ),
    ]);
    let message = validate_error(validate_data_set(&linked_meta, &partial));
    assert!(message.contains("partial empty key"));
}

#[test]
fn target_validation_rejects_non_visible_foreign_key_target_table() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"
            export_when = { any = ["server"] }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            foreign_key = { target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "1".to_owned(),
            )])],
        ),
    ]);

    let message = validate_error(validate_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &data_set,
    ));
    assert!(message.contains("target table `items` which is not exported"));
}

#[test]
fn target_validation_rejects_foreign_key_to_filtered_target_row() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"
            row_export = { excluded_tags_field = "exclude_tags" }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "exclude_tags"
            type = "string"
            export = false

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![
                BTreeMap::from([
                    ("id".to_owned(), "1".to_owned()),
                    ("exclude_tags".to_owned(), String::new()),
                ]),
                BTreeMap::from([
                    ("id".to_owned(), "2".to_owned()),
                    ("exclude_tags".to_owned(), "client".to_owned()),
                ]),
            ],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "2".to_owned(),
            )])],
        ),
    ]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
    let message = validate_error(validate_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &data_set,
    ));
    assert!(message.contains("references missing `items.by_id` key `[\"2\"]`"));
}

#[test]
fn target_validation_skips_foreign_key_when_export_when_does_not_match() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"
            export_when = { any = ["server"] }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            export_when = { any = ["server"] }
            foreign_key = { name = "drop_item", target_table = "items", export_when = { any = ["server"] } }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "1".to_owned(),
            )])],
        ),
    ]);

    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["client"]), &data_set,)
            .is_ok()
    );
}

#[test]
fn target_validation_reports_hidden_foreign_key_source_and_target_fields() {
    let source_hidden = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            export_when = { any = ["server"] }
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked_source_hidden = source_hidden.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "1".to_owned(),
            )])],
        ),
    ]);
    let source_message = validate_error(validate_data_set_for_target(
        &linked_source_hidden,
        &TargetTags::new(["client"]),
        &data_set,
    ));
    assert!(source_message.contains("non-exported source field `drop_item_id`"));

    let target_hidden = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_pk"

            [[tables.fields]]
            name = "pk"
            type = "int"

            [[tables.fields]]
            name = "id"
            type = "int"
            export_when = { any = ["server"] }

            [[tables.indexes]]
            name = "by_pk"
            fields = ["pk"]
            unique = true
            primary = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            foreign_key = { name = "drop_item", target_table = "items", target_index = "by_id" }
        "#,
    )
    .expect("meta should load");
    let linked_target_hidden = target_hidden.link().expect("meta should link");
    let target_data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([
                ("pk".to_owned(), "1".to_owned()),
                ("id".to_owned(), "1".to_owned()),
            ])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "1".to_owned(),
            )])],
        ),
    ]);
    let target_message = validate_error(validate_data_set_for_target(
        &linked_target_hidden,
        &TargetTags::new(["client"]),
        &target_data_set,
    ));
    assert!(target_message.contains("non-exported target index field `items.id`"));
}

#[test]
fn target_fragment_validation_uses_visible_rows() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            polymorphic = true
            row_export = { excluded_tags_field = "exclude_tags" }

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "exclude_tags"
            type = "string"
            export = false

            [[tables.fragments]]
            name = "weapon"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");

    let data_set = DataSet::new(vec![DataTable::new_with_sources(
        "items",
        vec![
            BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                ("exclude_tags".to_owned(), String::new()),
                ("weapon.power".to_owned(), "10".to_owned()),
            ]),
            BTreeMap::from([
                ("id".to_owned(), "2".to_owned()),
                ("exclude_tags".to_owned(), "client".to_owned()),
            ]),
            BTreeMap::from([
                ("id".to_owned(), "3".to_owned()),
                ("exclude_tags".to_owned(), "client".to_owned()),
                ("weapon.power".to_owned(), "bad".to_owned()),
            ]),
        ],
        vec![
            fragment_row_source(1, &["weapon"]),
            fragment_row_source(2, &["weapon"]),
            fragment_row_source(3, &["weapon"]),
        ],
    )
    .expect("table should build")]);

    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["client"]), &data_set).is_ok()
    );
    assert!(validate_data_set(&linked_meta, &data_set).is_err());

    let visible_invalid = DataSet::new(vec![DataTable::new_with_sources(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "1".to_owned()),
            ("exclude_tags".to_owned(), String::new()),
        ])],
        vec![fragment_row_source(1, &["weapon"])],
    )
    .expect("table should build")]);
    let error =
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["client"]), &visible_invalid)
            .unwrap_err();
    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("weapon.power` is required"))
    );
}

#[test]
fn validates_struct_json_cell_values() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "item_id"
            type = "string"
            required = true

            [[structs.fields]]
            name = "count"
            type = "int"
            required = true
            default = 1

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward"
            type = "Reward"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward".to_owned(),
            r#"{"item_id":"coin"}"#.to_owned(),
        )])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn rejects_invalid_struct_json_cell_values() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "item_id"
            type = "string"
            required = true

            [[structs.fields]]
            name = "count"
            type = "int"
            required = true

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward"
            type = "Reward"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let cases = [
        (r#"{"item_id":"coin"}"#, "reward.count` is required"),
        (
            r#"{"item_id":"coin","count":"many"}"#,
            "reward.count` value `many` does not match type",
        ),
        (
            r#"{"item_id":"coin","count":1,"extra":true}"#,
            "unknown struct key `extra`",
        ),
        ("not json", "invalid struct JSON"),
    ];

    for (raw, expected) in cases {
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([("reward".to_owned(), raw.to_owned())])],
        )]);
        let error = validate_data_set(&linked_meta, &data_set).unwrap_err();
        assert!(
            matches!(&error, ConfigForgeError::Validation(message) if message.contains(expected)),
            "expected `{expected}` from `{raw}`, got {error}"
        );
    }
}

#[test]
fn validates_struct_expanded_value_columns() {
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
            required = true
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
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([
            ("id".to_owned(), "hero".to_owned()),
            ("attributes.attack".to_owned(), "10".to_owned()),
        ])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn struct_expanded_value_uses_child_defaults_for_effective_existence() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Attributes"

            [[structs.fields]]
            name = "max_health"
            type = "int"
            default = 100

            [[tables]]
            name = "characters"
            source = "characters.csv"

            [[tables.fields]]
            name = "attributes"
            type = "Attributes"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new("characters", vec![BTreeMap::new()])]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn optional_struct_expanded_empty_cells_do_not_create_occurrence() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "item_id"
            type = "string"
            required = true

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
    let linked_meta = meta.link().expect("meta should link");
    let empty_optional = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward.item_id".to_owned(),
            String::new(),
        )])],
    )]);
    assert!(validate_data_set(&linked_meta, &empty_optional).is_ok());

    let present_but_incomplete = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("reward.item_id".to_owned(), String::new()),
            ("reward.count".to_owned(), "1".to_owned()),
        ])],
    )]);
    let error = validate_error(validate_data_set(&linked_meta, &present_but_incomplete));
    assert!(error.contains("reward.item_id` is required"));
}

#[test]
fn rejects_struct_expanded_value_schema_and_mixing_errors() {
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
            name = "id"
            type = "string"

            [[tables.fields]]
            name = "attributes"
            type = "Attributes"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");

    let mixed = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([
            ("id".to_owned(), "hero".to_owned()),
            ("attributes".to_owned(), r#"{"attack":10}"#.to_owned()),
            ("attributes.attack".to_owned(), "10".to_owned()),
        ])],
    )]);
    let error = validate_error(validate_data_set(&linked_meta, &mixed));
    assert!(error.contains("mixes struct JSON cell with expanded child path"));

    let unknown_child = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([
            ("id".to_owned(), "hero".to_owned()),
            ("attributes.power".to_owned(), "10".to_owned()),
        ])],
    )]);
    let error = validate_error(validate_data_set(&linked_meta, &unknown_child));
    assert!(error.contains("unknown struct child `power`"));
}

#[test]
fn validates_recursive_struct_expanded_value_and_json_boundary() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Offense"

            [[structs.fields]]
            name = "attack"
            type = "int"
            required = true

            [[structs]]
            name = "Attributes"

            [[structs.fields]]
            name = "offense"
            type = "Offense"
            required = true

            [[tables]]
            name = "characters"
            source = "characters.csv"

            [[tables.fields]]
            name = "attributes"
            type = "Attributes"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");

    let expanded = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([(
            "attributes.offense.attack".to_owned(),
            "10".to_owned(),
        )])],
    )]);
    assert!(validate_data_set(&linked_meta, &expanded).is_ok());

    let mixed_nested = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([
            (
                "attributes.offense".to_owned(),
                r#"{"attack":10}"#.to_owned(),
            ),
            ("attributes.offense.attack".to_owned(), "10".to_owned()),
        ])],
    )]);
    let error = validate_error(validate_data_set(&linked_meta, &mixed_nested));
    assert!(error.contains("attributes.offense` mixes struct JSON cell"));
}

#[test]
fn target_validation_skips_non_exported_struct_children() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "item_id"
            type = "string"
            required = true

            [[structs.fields]]
            name = "server_count"
            type = "int"
            required = true
            export_when = { any = ["server"] }

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward"
            type = "Reward"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward".to_owned(),
            r#"{"item_id":"coin"}"#.to_owned(),
        )])],
    )]);

    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["client"]), &data_set).is_ok()
    );
    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["server"]), &data_set)
            .is_err()
    );
}

#[test]
fn target_validation_ignores_hidden_expanded_descendants_for_occurrence_existence() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "client_item"
            type = "string"
            required = true
            export_when = { any = ["client"] }

            [[structs.fields]]
            name = "server_count"
            type = "int"
            export_when = { any = ["server"] }

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward"
            type = "Reward"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward.server_count".to_owned(),
            "10".to_owned(),
        )])],
    )]);

    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["client"]), &data_set).is_ok()
    );
    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["server"]), &data_set).is_ok()
    );
}

#[test]
fn validates_complete_data_set() {
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
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn rejects_data_set_missing_required_table() {
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
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![]);
    let error = validate_data_set(&linked_meta, &data_set).unwrap_err();

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("missing table `items`"))
    );
}

#[test]
fn normalizes_data_set_before_validation() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "max_stack"
                type = "int"
                required = true
                default = 1
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new("items", vec![BTreeMap::new()])]);

    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(validate_normalized_data_set(&linked_meta, &normalized).is_ok());
    assert_eq!(
        normalized.table("items").unwrap().rows[0]
            .get("max_stack")
            .map(String::as_str),
        Some("1")
    );
}

#[test]
fn object_table_validation_uses_single_logical_row_and_field_rules() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"
            required = true
            default = 100

            [[tables.fields]]
            name = "server_note"
            type = "string"
            required = true
            export_when = { any = ["server"] }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new("game_config", vec![BTreeMap::new()])]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &normalized
    )
    .is_ok());
    assert!(validate_normalized_data_set(&linked_meta, &normalized).is_err());
    assert_eq!(
        normalized.table("game_config").unwrap().rows[0]
            .get("max_level")
            .map(String::as_str),
        Some("100")
    );
}

#[test]
fn object_table_validation_rejects_non_singleton_rows() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let table_meta = linked_meta.table("game_config").expect("linked table");
    let table = DataTable::new(
        "game_config",
        vec![
            BTreeMap::from([("max_level".to_owned(), "100".to_owned())]),
            BTreeMap::from([("max_level".to_owned(), "101".to_owned())]),
        ],
    );

    let message = validate_error(validate_table(table_meta, &table));

    assert!(message.contains("must normalize to exactly one logical row"));
}

#[test]
fn object_table_only_header_required_without_default_fails() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new("game_config", vec![BTreeMap::new()])]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("missing field `max_level`"));
}

#[test]
fn object_table_field_type_enum_and_struct_errors_still_apply() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[enums]]
            name = "Difficulty"

            [[enums.values]]
            name = "normal"

            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "count"
            type = "int"

            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"

            [[tables.fields]]
            name = "difficulty"
            type = "Difficulty"

            [[tables.fields]]
            name = "daily_reward"
            type = "Reward"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");

    let cases = [
        (
            BTreeMap::from([
                ("max_level".to_owned(), "bad".to_owned()),
                ("difficulty".to_owned(), "normal".to_owned()),
                ("daily_reward".to_owned(), r#"{"count":1}"#.to_owned()),
            ]),
            "field `max_level` value `bad` does not match type",
        ),
        (
            BTreeMap::from([
                ("max_level".to_owned(), "60".to_owned()),
                ("difficulty".to_owned(), "nightmare".to_owned()),
                ("daily_reward".to_owned(), r#"{"count":1}"#.to_owned()),
            ]),
            "field `difficulty` value `nightmare` does not match type",
        ),
        (
            BTreeMap::from([
                ("max_level".to_owned(), "60".to_owned()),
                ("difficulty".to_owned(), "normal".to_owned()),
                ("daily_reward".to_owned(), "not json".to_owned()),
            ]),
            "field `daily_reward` has invalid struct JSON",
        ),
    ];

    for (row, expected_message) in cases {
        let data_set = DataSet::new(vec![DataTable::new("game_config", vec![row])]);
        let message = validate_error(validate_data_set(&linked_meta, &data_set));
        assert!(
            message.contains(expected_message),
            "expected `{expected_message}` in `{message}`"
        );
    }
}

#[test]
fn object_table_field_validator_still_applies() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 100
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "game_config",
        vec![BTreeMap::from([("max_level".to_owned(), "101".to_owned())])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("exceeds max `100`"));
}

#[test]
fn target_validation_skips_non_exported_tables_fields_and_rows() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                primary_index = "by_id"
                row_export = { excluded_tags_field = "exclude_tags" }

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "server_value"
                type = "int"
                required = true
                export_when = { any = ["server"] }

                [[tables.fields]]
                name = "exclude_tags"
                type = "string"
                export = false

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true
                primary = true

                [[tables]]
                name = "server_only"
                source = "server_only.csv"
                export_when = { any = ["server"] }

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
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
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &normalized
    )
    .is_ok());
    assert!(validate_normalized_data_set(&linked_meta, &normalized).is_err());
}

#[test]
fn range_validator_accepts_values_inside_bounds() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "5".to_owned())])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn range_validator_rejects_values_outside_bounds() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "11".to_owned())])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("exceeds max `10`"));
}

#[test]
fn range_validator_rejects_unknown_parameter() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            minn = 1
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "5".to_owned())])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("unknown parameter `minn`"));
}

#[test]
fn range_validator_respects_target_field_visibility() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"
            export_when = { any = ["server"] }

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "100".to_owned())])],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &normalized
    )
    .is_ok());
}

#[test]
fn range_validator_rejects_empty_value_when_disallowed() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            allow_empty = false
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), String::new())])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("must not be empty"));
}

#[test]
fn range_validator_does_not_panic_on_fractional_int_when_run_directly() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "1.5".to_owned())])],
    )]);
    let registry = ValidatorRegistry::with_builtin_validators();
    let plan =
        CompiledValidationPlan::compile(&linked_meta, &registry).expect("plan should compile");
    let context = ValidationContext::new(
        &linked_meta,
        None,
        &data_set,
        ExportValidationView::full(&linked_meta, &data_set),
    );

    let report = plan.run(&context);

    assert!(report.issues.is_empty());
}

#[test]
fn warning_range_validator_does_not_fail_validation() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            name = "recommended_level"
            severity = "warning"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "11".to_owned())])],
    )]);
    let registry = ValidatorRegistry::with_builtin_validators();
    let plan =
        CompiledValidationPlan::compile(&linked_meta, &registry).expect("plan should compile");
    let context = ValidationContext::new(
        &linked_meta,
        None,
        &data_set,
        ExportValidationView::full(&linked_meta, &data_set),
    );

    let report = plan.run(&context);

    assert!(!report.has_error());
    assert_eq!(report.issues[0].severity, ValidationSeverity::Warning);
    assert_eq!(report.issues[0].validator, "recommended_level");
    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn range_validator_export_when_filters_validator_itself() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
            export_when = { any = ["server"] }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "100".to_owned())])],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &normalized
    )
    .is_ok());

    let message = validate_error(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["server"]),
        &normalized,
    ));
    assert!(message.contains("exceeds max `10`"));
}

#[test]
fn range_validator_supports_exclusive_bounds_and_float_values() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "ratio"
            type = "float"

            [[tables.fields.validators]]
            kind = "range"
            min = 0.5
            max = 1.5
            exclusive_min = true
            exclusive_max = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("ratio".to_owned(), "1.0".to_owned())])],
    )]);
    let min_edge = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("ratio".to_owned(), "0.5".to_owned())])],
    )]);
    let max_edge = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("ratio".to_owned(), "1.5".to_owned())])],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    assert!(validate_error(validate_data_set(&linked_meta, &min_edge)).contains("below min `0.5`"));
    assert!(
        validate_error(validate_data_set(&linked_meta, &max_edge)).contains("exceeds max `1.5`")
    );
}

#[test]
fn rejects_non_finite_float_values() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "ratio"
            type = "float"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");

    for value in ["NaN", "inf", "-inf"] {
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([("ratio".to_owned(), value.to_owned())])],
        )]);

        let message = validate_error(validate_data_set(&linked_meta, &data_set));
        assert!(message.contains("does not match type"));
    }

    let scientific = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("ratio".to_owned(), "1e-3".to_owned())])],
    )]);
    assert!(validate_data_set(&linked_meta, &scientific).is_ok());
}

#[test]
fn range_validator_declared_on_table_reports_scope_error() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.validators]]
            kind = "range"
            min = 1

            [[tables.fields]]
            name = "level"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "5".to_owned())])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("cannot be declared at table `items`"));
}
