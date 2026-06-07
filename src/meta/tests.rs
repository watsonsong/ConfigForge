// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use super::*;
use super::{link::link_meta_model, validate::validate_meta_model};
use crate::ConfigForgeError;
use tempfile::tempdir;

fn parse(input: &str) -> MetaModel {
    MetaModel::from_toml_str(input).expect("meta should parse")
}

fn diagnostics(input: &str) -> Vec<MetaDiagnostic> {
    match MetaModel::from_toml_str(input) {
        Ok(model) => validate_meta_model(&model),
        Err(ConfigForgeError::MetaValidation(diagnostics)) => diagnostics,
        Err(error) => panic!("unexpected error: {error}"),
    }
}

#[test]
fn parses_basic_collection_table() {
    let meta = parse(
        r#"
            [[tables]]
            name = "items"
            kind = "collection"
            desc = "所有物品配置。"
            primary_index = "by_id"
            sources = [{ provider = "csv", path = "items.csv" }]

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true
            desc = "物品 ID。"

            [[tables.fields]]
            name = "display_name"
            type = "text"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
            desc = "按物品 ID 查询。"
        "#,
    );

    let table = meta.table("items").expect("table should exist");
    assert_eq!(table.kind, TableKind::Collection);
    assert_eq!(table.desc.as_deref(), Some("所有物品配置。"));
    assert_eq!(table.source_path(), Some("items.csv"));
    let linked_meta = meta.link().expect("meta should link");
    let linked_table = linked_meta
        .table("items")
        .expect("linked table should exist");
    assert_eq!(
        linked_table.field("id").unwrap().linked_field,
        LinkedField::Primitive(PrimitiveFieldType::Int)
    );
    assert_eq!(
        linked_table.field("display_name").unwrap().linked_field,
        LinkedField::Primitive(PrimitiveFieldType::Text)
    );
    assert_eq!(table.primary_index_meta().unwrap().name, "by_id");
    let linked_primary_index = linked_table
        .primary_index()
        .expect("linked primary index should exist");
    assert_eq!(linked_primary_index.meta.name, "by_id");
    assert_eq!(linked_primary_index.fields[0].meta.name, "id");
    assert_eq!(
        linked_primary_index.fields[0].linked_field,
        LinkedField::Primitive(PrimitiveFieldType::Int)
    );
}

#[test]
fn links_primary_index_marked_on_index_when_table_field_is_empty() {
    let meta = parse(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#,
    );

    let linked_meta = meta.link().expect("meta should link");
    let linked_table = linked_meta.table("items").unwrap();
    let primary_index = linked_table
        .primary_index()
        .expect("primary index should be linked from primary flag");
    assert_eq!(primary_index.meta.name, "by_id");
    assert_eq!(primary_index.fields[0].meta.name, "id");
}

#[test]
fn parses_basic_object_table_without_index() {
    let meta = parse(
        r#"
            [[tables]]
            name = "game_rules"
            kind = "object"
            source = "game_rules.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"
            required = true
        "#,
    );

    let table = meta.table("game_rules").expect("table should exist");
    assert_eq!(table.kind, TableKind::Object);
    assert_eq!(table.source_path(), Some("game_rules.csv"));
    assert!(table.indexes.is_empty());
}

#[test]
fn rejects_object_table_step1_unsupported_features_except_foreign_keys() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"
            sources = [{ provider = "csv", path = "extra.csv" }]
            override_sources = [{ provider = "csv", path = "override.csv" }]
            row_export = { required_tags_field = "tags" }
            primary_index = "by_id"
            preserve_source_order = true
            polymorphic = true

            [[tables.fields]]
            name = "id"
            type = "int"
            unique = true
            foreign_key = { target_table = "items" }

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true

            [[tables.fragments]]
            name = "debug"

            [[tables.foreign_keys]]
            name = "bad_fk"
            fields = ["id"]
            target_table = "items"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#,
    );
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    for expected in [
        "only supports legacy `source`",
        "does not support `override_sources`",
        "does not support `row_export`",
        "does not support fragments",
        "does not support indexes",
        "does not support `primary_index`",
        "does not support `preserve_source_order`",
        "does not support `polymorphic`",
        "does not support unique field indexes",
    ] {
        assert!(
            messages.contains(expected),
            "missing diagnostic `{expected}` in\n{messages}"
        );
    }
}

#[test]
fn accepts_object_table_foreign_keys_as_source() {
    let meta = parse(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "region"
            type = "string"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables.indexes]]
            name = "by_id_region"
            fields = ["id", "region"]
            unique = true

            [[tables]]
            name = "game_rules"
            kind = "object"
            source = "game_rules.csv"

            [[tables.fields]]
            name = "starter_item_id"
            type = "int"
            required = true
            foreign_key = { name = "starter_item", target_table = "items" }

            [[tables.fields]]
            name = "starter_region_item_id"
            type = "int"
            required = false

            [[tables.fields]]
            name = "starter_region"
            type = "string"
            required = false

            [[tables.foreign_keys]]
            name = "starter_region_item"
            fields = ["starter_region_item_id", "starter_region"]
            target_table = "items"
            target_index = "by_id_region"
        "#,
    );

    let rules = meta.table("game_rules").expect("object table should exist");
    let resolved = rules.resolved_foreign_keys();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].name, "starter_region_item");
    assert!(resolved[0].allow_empty);
    assert_eq!(resolved[1].name, "starter_item");
    assert!(!resolved[1].allow_empty);

    let linked = meta.link().expect("object foreign keys should link");
    let linked_rules = linked
        .table("game_rules")
        .expect("linked object table should exist");
    assert_eq!(linked_rules.foreign_keys.len(), 2);
    assert!(linked_rules.foreign_key("starter_item").is_some());
    assert!(linked_rules.foreign_key("starter_region_item").is_some());
}

#[test]
fn rejects_foreign_key_targeting_object_table() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "config_id"
            type = "int"
            foreign_key = { target_table = "game_config" }
        "#,
    );
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(messages.contains("uses ObjectTable `game_config` as target"));
}

#[test]
fn links_custom_enum_and_struct_types_after_loading_all_meta() {
    let meta = parse(
        r#"
            [[enums]]
            name = "ItemRarity"
            [[enums.values]]
            name = "common"

            [[structs]]
            name = "Reward"
            [[structs.fields]]
            name = "count"
            type = "int"

            [[tables]]
            name = "items"
            source = "items.csv"
            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
            [[tables.fields]]
            name = "reward"
            type = "Reward"
        "#,
    );

    let linked_meta = meta.link().expect("meta should link");
    let table = linked_meta.table("items").unwrap();
    match &table.field("rarity").unwrap().linked_field {
        LinkedField::Enum(enum_meta) => assert_eq!(enum_meta.name, "ItemRarity"),
        other => panic!("expected linked enum, got {other:?}"),
    }
    match &table.field("reward").unwrap().linked_field {
        LinkedField::Struct(struct_type) => {
            assert_eq!(struct_type.meta.name, "Reward");
            assert_eq!(struct_type.fields[0].meta.name, "count");
            assert_eq!(
                struct_type.fields[0].linked_field,
                LinkedField::Primitive(PrimitiveFieldType::Int)
            );
        }
        other => panic!("expected linked struct, got {other:?}"),
    }
}

#[test]
fn linked_struct_fields_recursively_include_nested_type_info() {
    let meta = parse(
        r#"
            [[enums]]
            name = "RewardKind"
            [[enums.values]]
            name = "coin"

            [[structs]]
            name = "RewardDetail"
            [[structs.fields]]
            name = "kind"
            type = "RewardKind"

            [[structs]]
            name = "Reward"
            [[structs.fields]]
            name = "count"
            type = "int"
            [[structs.fields]]
            name = "detail"
            type = "RewardDetail"

            [[tables]]
            name = "items"
            source = "items.csv"
            [[tables.fields]]
            name = "reward"
            type = "Reward"
        "#,
    );

    let linked_meta = meta.link().expect("meta should link");
    let reward_field = linked_meta.table("items").unwrap().field("reward").unwrap();
    let LinkedField::Struct(reward_type) = &reward_field.linked_field else {
        panic!("reward should be linked as struct");
    };
    let detail_field = reward_type
        .fields
        .iter()
        .find(|field| field.meta.name == "detail")
        .expect("nested struct field should exist");
    let LinkedField::Struct(detail_type) = &detail_field.linked_field else {
        panic!("detail should be linked as struct");
    };
    let kind_field = detail_type
        .fields
        .iter()
        .find(|field| field.meta.name == "kind")
        .expect("nested enum field should exist");
    match &kind_field.linked_field {
        LinkedField::Enum(enum_meta) => assert_eq!(enum_meta.name, "RewardKind"),
        other => panic!("expected nested enum, got {other:?}"),
    }
}

#[test]
fn supports_imported_meta_files_before_linking_field_types() {
    let dir = tempdir().expect("temp dir should exist");
    let types_path = dir.path().join("types.toml");
    let root_path = dir.path().join("root.toml");
    fs::write(
        &types_path,
        r#"
            [[enums]]
            name = "ItemRarity"
            [[enums.values]]
            name = "common"
        "#,
    )
    .expect("types meta should be written");
    fs::write(
        &root_path,
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
        "#,
    )
    .expect("root meta should be written");

    let meta = MetaModel::from_paths([&root_path, &types_path]).expect("meta files should load");
    assert!(meta.enum_meta("ItemRarity").is_some());
    let linked_meta = meta.link().expect("imported meta should link");
    let field = linked_meta.table("items").unwrap().field("rarity").unwrap();
    match &field.linked_field {
        LinkedField::Enum(enum_meta) => assert_eq!(enum_meta.name, "ItemRarity"),
        other => panic!("expected linked enum, got {other:?}"),
    }
}

#[test]
fn parses_and_links_field_level_foreign_key() {
    let meta = parse(
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
            name = "drop_item_id"
            type = "int"
            required = false
            foreign_key = { target_table = "items", target_index = "by_id" }
        "#,
    );

    let monsters = meta.table("monsters").expect("table should exist");
    let resolved = monsters.resolved_foreign_keys();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].name, "drop_item_id");
    assert_eq!(resolved[0].fields, vec!["drop_item_id"]);
    assert!(resolved[0].allow_empty);

    let linked = meta.link().expect("meta should link");
    let linked_monsters = linked.table("monsters").expect("linked table should exist");
    let foreign_key = linked_monsters
        .foreign_key("drop_item_id")
        .expect("foreign key should link");
    assert_eq!(foreign_key.target_table_name, "items");
    assert_eq!(foreign_key.target_index_name, "by_id");
    assert_eq!(foreign_key.fields[0].meta.name, "drop_item_id");
}

#[test]
fn derives_table_level_foreign_key_allow_empty_from_source_fields() {
    let meta = parse(
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
            name = "item_in_region"
            fields = ["item_id", "region"]
            target_table = "items"
            target_index = "by_id_region"
        "#,
    );

    let shops = meta.table("shops").expect("table should exist");
    let resolved = shops.resolved_foreign_keys();
    assert_eq!(resolved.len(), 1);
    assert!(resolved[0].allow_empty);
}

#[test]
fn rejects_invalid_foreign_key_declarations() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "rules"
            kind = "object"
            source = "rules.csv"

            [[tables.fields]]
            name = "item_id"
            type = "int"
            foreign_key = { target_table = "items" }

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "region"
            type = "string"

            [[tables.indexes]]
            name = "by_id_region"
            fields = ["id", "region"]
            unique = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "item_id"
            type = "string"
            foreign_key = { name = "bad", target_table = "items", target_index = "by_id_region" }

            [[tables.foreign_keys]]
            name = "bad"
            fields = ["missing"]
            target_table = "items"
            target_index = "by_id_region"
        "#,
    );
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(messages.contains("field-level foreign key only supports single-field index"));
    assert!(messages.contains("defined more than once"));
    assert!(messages.contains("unknown source field `missing`"));
}

#[test]
fn rejects_invalid_linked_field_declarations() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "rules"
            kind = "object"
            source = "rules.csv"

            [[tables.fields]]
            name = "item_id"
            type = "int"
            foreign_key = { name = "item", target_table = "items", target_index = "by_id", linked_field = "" }

            [[tables.fields]]
            name = "reward_item_id"
            type = "int"
            foreign_key = { name = "reward_item", target_table = "items", target_index = "by_id", linked_field = "item" }

            [[tables.fields]]
            name = "other_item_id"
            type = "int"
            foreign_key = { name = "other_item", target_table = "items", target_index = "by_id", linked_field = "item" }

            [[tables.fields]]
            name = "conflicting_field"
            type = "int"

            [[tables.fields]]
            name = "field_conflict_id"
            type = "int"
            foreign_key = { name = "field_conflict", target_table = "items", target_index = "by_id", linked_field = "conflicting_field" }

            [[tables.fragments]]
            name = "conflicting_fragment"
            when_field = "item_id"
            cases = []

            [[tables.fields]]
            name = "fragment_conflict_id"
            type = "int"
            foreign_key = { name = "fragment_conflict", target_table = "items", target_index = "by_id", linked_field = "conflicting_fragment" }
        "#,
    );
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(messages.contains("linked_field must be non-empty"));
    assert!(messages.contains("linked_field `item` is defined more than once"));
    assert!(messages.contains("linked_field `conflicting_field` conflicts with source field"));
    assert!(messages.contains("linked_field `conflicting_fragment` conflicts with fragment"));
}

#[test]
fn link_rejects_semantically_invalid_foreign_keys() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "region"
            type = "string"

            [[tables.indexes]]
            name = "by_id_region"
            fields = ["id", "region"]
            unique = true

            [[tables.indexes]]
            name = "by_region"
            fields = ["region"]
            unique = false

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "item_id"
            type = "int"
            foreign_key = { name = "composite_from_field", target_table = "items", target_index = "by_id_region" }

            [[tables.fields]]
            name = "region"
            type = "string"

            [[tables.foreign_keys]]
            name = "non_unique"
            fields = ["region"]
            target_table = "items"
            target_index = "by_region"
        "#,
    )
    .expect_err("invalid FK meta should fail loading");
    let ConfigForgeError::MetaValidation(diagnostics) = meta else {
        panic!("expected meta validation diagnostics");
    };
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(messages.contains("field-level foreign key only supports single-field index"));
    assert!(messages.contains("which is not unique"));
}

#[test]
fn accepts_canonical_and_legacy_primitive_field_types() {
    let meta = parse(
        r#"
            [[tables]]
            name = "types"
            source = "types.csv"

            [[tables.fields]]
            name = "int_value"
            type = "int"

            [[tables.fields]]
            name = "legacy_int_value"
            type = "i64"

            [[tables.fields]]
            name = "float_value"
            type = "float"

            [[tables.fields]]
            name = "legacy_float_value"
            type = "f64"

            [[tables.fields]]
            name = "bool_value"
            type = "bool"

            [[tables.fields]]
            name = "string_value"
            type = "string"

            [[tables.fields]]
            name = "text_value"
            type = "text"

            [[tables.fields]]
            name = "asset_value"
            type = "asset"
            asset_kind = "generic_asset"

            [[tables.fields]]
            name = "int_list"
            type = "int[]"

            [[asset_kinds]]
            name = "generic_asset"
        "#,
    );

    let table = meta.table("types").unwrap();
    assert_eq!(
        table.field("int_value").unwrap().primitive_type(),
        Some(PrimitiveFieldType::Int)
    );
    assert_eq!(
        table.field("legacy_int_value").unwrap().primitive_type(),
        Some(PrimitiveFieldType::Int)
    );
    assert_eq!(
        table.field("float_value").unwrap().primitive_type(),
        Some(PrimitiveFieldType::Float)
    );
    assert_eq!(
        table.field("legacy_float_value").unwrap().primitive_type(),
        Some(PrimitiveFieldType::Float)
    );
    assert_eq!(
        table.field("bool_value").unwrap().primitive_type(),
        Some(PrimitiveFieldType::Bool)
    );
    assert_eq!(
        table.field("string_value").unwrap().primitive_type(),
        Some(PrimitiveFieldType::String)
    );
    assert_eq!(
        table.field("text_value").unwrap().primitive_type(),
        Some(PrimitiveFieldType::Text)
    );
    assert_eq!(
        table.field("asset_value").unwrap().primitive_type(),
        Some(PrimitiveFieldType::Asset)
    );
    assert_eq!(table.field("int_list").unwrap().primitive_type(), None);
    assert_eq!(
        table
            .field("int_list")
            .unwrap()
            .foreign_key_scalar_type_name(),
        "int"
    );
}

#[test]
fn parses_asset_kinds_and_field_asset_kind_links() {
    let meta = parse(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "icon"
            type = "asset"
            asset_kind = "unreal_icon"

            [[asset_kinds]]
            name = "unreal_icon"
            extends = "unreal_asset"
            search_paths = ["unreal_game"]
            base_dirs = ["UI"]
            extensions = [".uasset"]
            input = "path"
            path_style = "mounted_object"
            extension_policy = "required"
            strip_wrappers = ["Texture2D'{value}'"]
            object_separator = "."
            object_name = "{stem}"
            case_sensitive = true
            require_existing = true
            require_sidecars = ["{path}.meta"]
            allow_path_prefixes = ["/Game/UI/"]
            deny_path_prefixes = ["/Game/UI/Dev/"]
            deny_when_tags = ["shipping"]
            name_prefixes = ["T_"]
            name_suffixes = ["_Icon"]
            allow_patterns = ["^/Game/UI/.+$"]
            deny_patterns = ["Dev"]
            allow_auto_complete = true

            [[asset_kinds.regex_rules]]
            name = "canonical_unreal_icon"
            target = "logical_ref"
            pattern = "^/Game/UI/.+\\..+$"
            severity = "error"
            message = "must be a mounted object path"
            when_tags = ["client"]

            [[asset_kinds]]
            name = "unreal_asset"
            search_paths = ["unreal_game"]
            path_style = "mounted_object"
        "#,
    );

    let field = meta.table("items").unwrap().field("icon").unwrap();
    assert_eq!(field.asset_kind.as_deref(), Some("unreal_icon"));
    let asset_kind = meta.asset_kind("unreal_icon").unwrap();
    assert_eq!(asset_kind.extends.as_deref(), Some("unreal_asset"));
    assert_eq!(asset_kind.search_paths, ["unreal_game"]);
    assert_eq!(asset_kind.base_dirs, ["UI"]);
    assert_eq!(asset_kind.extensions, [".uasset"]);
    assert!(asset_kind.allow_auto_complete);
    assert_eq!(asset_kind.regex_rules[0].name, "canonical_unreal_icon");

    let linked = meta.link().expect("meta should link");
    assert!(linked.asset_kind("unreal_icon").is_some());
}

#[test]
fn rejects_invalid_asset_kind_extends() {
    let diagnostics = diagnostics(
        r#"
            [[asset_kinds]]
            name = "self_ref"
            extends = "self_ref"

            [[asset_kinds]]
            name = "unknown_ref"
            extends = "missing"

            [[asset_kinds]]
            name = "cycle_a"
            extends = "cycle_b"

            [[asset_kinds]]
            name = "cycle_b"
            extends = "cycle_a"
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidAssetKindDeclaration
            && diagnostic
                .message
                .contains("asset kind `self_ref` cannot extend itself")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidAssetKindDeclaration
            && diagnostic
                .message
                .contains("asset kind `unknown_ref` extends unknown asset kind `missing`")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidAssetKindDeclaration
            && diagnostic
                .message
                .contains("asset kind extends cycle detected")
    }));
}

#[test]
fn rejects_asset_fields_without_asset_kind() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "icon"
            type = "asset"
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidAssetKindDeclaration
            && diagnostic.message.contains("does not declare `asset_kind`")
    }));
}

#[test]
fn rejects_asset_kind_on_non_asset_fields() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "name"
            type = "string"
            asset_kind = "ui_text"

            [[asset_kinds]]
            name = "ui_text"
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidAssetKindDeclaration
            && diagnostic
                .message
                .contains("only type `asset` fields may use it")
    }));
}

#[test]
fn rejects_duplicate_asset_kind_names() {
    let diagnostics = diagnostics(
        r#"
            [[asset_kinds]]
            name = "ui_icon"

            [[asset_kinds]]
            name = "ui_icon"
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::DuplicateAssetKindName
            && diagnostic
                .message
                .contains("asset kind `ui_icon` is defined more than once")
    }));
}

#[test]
fn rejects_invalid_asset_regex_patterns() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "icon"
            type = "asset"
            asset_kind = "ui_icon"

            [[asset_kinds]]
            name = "ui_icon"
            allow_patterns = ["("]

            [[asset_kinds.regex_rules]]
            name = "broken"
            pattern = "["
        "#,
    );

    let invalid_regex_count = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == MetaDiagnosticCode::InvalidAssetKindDeclaration
                && diagnostic.message.contains("regex pattern")
                && diagnostic.message.contains("is invalid")
        })
        .count();
    assert_eq!(invalid_regex_count, 2);
}

#[test]
fn primitive_field_type_names_are_canonical() {
    assert_eq!(PrimitiveFieldType::Int.as_str(), "int");
    assert_eq!(PrimitiveFieldType::Float.as_str(), "float");
    assert_eq!(PrimitiveFieldType::Bool.as_str(), "bool");
    assert_eq!(PrimitiveFieldType::String.as_str(), "string");
    assert_eq!(PrimitiveFieldType::Text.as_str(), "text");
    assert_eq!(PrimitiveFieldType::Asset.as_str(), "asset");
    assert_eq!(PrimitiveFieldType::parse("unknown"), None);
}

#[test]
fn export_condition_matches_target_tags() {
    let condition = ExportCondition {
        any: vec!["client".to_owned(), "tool".to_owned()],
        all: vec!["shipping".to_owned()],
        none: vec!["debug".to_owned()],
    };

    assert!(condition.matches(&TargetTags::new(["client", "shipping"])));
    assert!(!condition.matches(&TargetTags::new(["server", "shipping"])));
    assert!(!condition.matches(&TargetTags::new(["client"])));
    assert!(!condition.matches(&TargetTags::new(["client", "shipping", "debug"])));
}

#[test]
fn target_tags_are_trimmed_and_empty_tags_are_ignored() {
    let tags = TargetTags::new([" client ", "", " shipping"]);

    assert!(tags.contains("client"));
    assert!(tags.contains("shipping"));
    assert!(!tags.contains(" client "));
    assert!(!tags.contains(""));
}

#[test]
fn source_ref_deserializes_export_condition() {
    let meta = parse(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            override_sources = [
                { provider = "csv", path = "items_s1.csv", export_when = { any = ["s1"], none = ["s2"] } },
            ]
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#,
    );

    let source = &meta.table("items").unwrap().override_sources[0];
    assert_eq!(source.export_when.any, vec!["s1".to_owned()]);
    assert_eq!(source.export_when.none, vec!["s2".to_owned()]);
}

#[test]
fn rejects_invalid_export_condition_tags() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            export_when = { any = [" client "] }

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic
                .message
                .contains("export_when tag ` client ` is invalid")
    }));
}

#[test]
fn row_export_policy_matches_required_and_excluded_tags() {
    let policy = RowExportPolicy {
        required_tags_field: Some("required_tags".to_owned()),
        excluded_tags_field: Some("excluded_tags".to_owned()),
        match_mode: TagMatchMode::All,
    };

    assert!(policy.matches(
        &TargetTags::new(["client", "shipping"]),
        |field| match field {
            "required_tags" => Some("client, shipping"),
            "excluded_tags" => Some("server"),
            _ => None,
        }
    ));
    assert!(
        !policy.matches(&TargetTags::new(["client"]), |field| match field {
            "required_tags" => Some("client, shipping"),
            "excluded_tags" => Some("server"),
            _ => None,
        })
    );
    assert!(
        !policy.matches(&TargetTags::new(["client", "debug"]), |field| match field {
            "required_tags" => Some("client"),
            "excluded_tags" => Some("debug"),
            _ => None,
        })
    );
}

#[test]
fn rejects_deprecated_field_export_flags() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "debug_note"
            type = "string"
            tool_only = true
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic.message.contains("deprecated `tool_only`")
    }));
}

#[test]
fn validates_model_directly_with_ok_and_error_results() {
    let valid = parse(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    );
    assert!(valid.validate().is_ok());

    let invalid: MetaModel = toml::from_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables]]
            name = "items"
            source = "other.csv"
        "#,
    )
    .expect("raw invalid meta should deserialize");
    let diagnostics = invalid.validate().expect_err("validation should fail");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateTableName)
    );

    let invalid_link: MetaModel = toml::from_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "rarity"
            type = "MissingType"
        "#,
    )
    .expect("raw link-invalid meta should deserialize");
    let diagnostics = invalid_link
        .validate()
        .expect_err("link diagnostics should fail validation");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::UnknownFieldType)
    );
}

#[test]
fn rejects_unknown_field_type_after_linking() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "rarity"
            type = "MissingType"
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::UnknownFieldType)
    );
}

#[test]
fn rejects_unsupported_list_element_types() {
    let diagnostics = diagnostics(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "item_id"
            type = "int"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "struct_rewards"
            type = "list<Reward>"

            [[tables.fields]]
            name = "nested_ids"
            type = "list<int[]>"
        "#,
    );

    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(messages.contains("unsupported list element type `Reward`"));
    assert!(messages.contains("unsupported list element type `int[]`"));
}

#[test]
fn rejects_duplicate_table_names() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables]]
            name = "items"
            source = "other.csv"
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateTableName)
    );
}

#[test]
fn rejects_duplicate_field_names() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "id"
            type = "string"
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateFieldName)
    );
}

#[test]
fn rejects_index_referencing_missing_field() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_missing"
            fields = ["missing"]
            unique = true
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::MissingFieldReference)
    );
}

#[test]
fn link_rejects_indexes_with_missing_fields() {
    let meta: MetaModel = toml::from_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_missing"
            fields = ["missing"]
            unique = true
        "#,
    )
    .expect("raw meta should deserialize");

    let (linked, diagnostics) = link_meta_model(&meta);

    assert!(linked.is_none());
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::MissingFieldReference
            && diagnostic
                .message
                .contains("index `by_missing` references unknown field `missing`")
    }));
}

#[test]
fn rejects_primary_index_mismatch() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "name"
            type = "string"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true

            [[tables.indexes]]
            name = "by_name"
            fields = ["name"]
            unique = true
            primary = true
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::PrimaryIndexMismatch)
    );
}

#[test]
fn rejects_non_unique_primary_index() {
    let diagnostics = diagnostics(
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
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::InvalidPrimaryIndex)
    );
}

#[test]
fn exposes_raw_and_linked_query_helpers() {
    let meta = parse(
        r#"
            [[enums]]
            name = "ItemRarity"
            [[enums.values]]
            name = "common"

            [[structs]]
            name = "Reward"
            [[structs.fields]]
            name = "count"
            type = "int"

            [[tables]]
            name = "items"
            kind = "collection"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"

            [[tables.fields]]
            name = "reward"
            type = "Reward"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true

            [[tables]]
            name = "rules"
            kind = "object"
            source = "rules.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"
        "#,
    );

    assert_eq!(meta.collection_tables().count(), 1);
    assert!(meta.table("missing").is_none());
    assert!(meta.enum_meta("missing").is_none());
    assert_eq!(meta.struct_meta("Reward").unwrap().name, "Reward");
    assert!(meta.struct_meta("missing").is_none());

    let table = meta.table("items").unwrap();
    assert!(table.field("missing").is_none());
    assert_eq!(table.index("by_id").unwrap().name, "by_id");
    assert!(table.index("missing").is_none());
    assert_eq!(table.primary_index().unwrap().name, "by_id");
    assert!(meta.table("rules").unwrap().primary_index().is_none());
    assert_eq!(
        table.field("rarity").unwrap().primitive_type(),
        None,
        "custom enum fields are not primitive types"
    );

    let linked_meta = meta.link().expect("meta should link");
    assert_eq!(linked_meta.model.tables.len(), 2);
    assert_eq!(
        linked_meta.enum_meta("ItemRarity").unwrap().name,
        "ItemRarity"
    );
    assert!(linked_meta.enum_meta("missing").is_none());
    assert_eq!(
        linked_meta.raw_struct_meta("Reward").unwrap().name,
        "Reward"
    );
    assert!(linked_meta.raw_struct_meta("missing").is_none());
    assert!(linked_meta.table("missing").is_none());
    assert_eq!(
        linked_meta.struct_meta("Reward").unwrap().meta.name,
        "Reward"
    );
    assert!(linked_meta.struct_meta("missing").is_none());

    let linked_table = linked_meta.table("items").unwrap();
    assert!(linked_table.field("missing").is_none());
    assert_eq!(linked_table.index("by_id").unwrap().meta.name, "by_id");
    assert!(linked_table.index("missing").is_none());
    assert_eq!(
        linked_table.field("id").unwrap().primitive_type(),
        Some(PrimitiveFieldType::Int)
    );
    assert_eq!(linked_table.field("rarity").unwrap().primitive_type(), None);

    let enum_field = &linked_table.field("rarity").unwrap().linked_field;
    let struct_field = &linked_table.field("reward").unwrap().linked_field;
    let primitive_field = &linked_table.field("id").unwrap().linked_field;
    assert_eq!(meta.linked_enum(enum_field).unwrap().name, "ItemRarity");
    assert!(meta.linked_enum(struct_field).is_none());
    assert_eq!(
        meta.linked_struct(struct_field).unwrap().meta.name,
        "Reward"
    );
    assert!(meta.linked_struct(primitive_field).is_none());

    let linked_struct = linked_meta.struct_meta("Reward").unwrap();
    assert_eq!(linked_struct.field("count").unwrap().meta.name, "count");
    assert!(linked_struct.field("missing").is_none());
}

#[test]
fn source_path_reads_single_source_form_and_reports_missing_source() {
    let meta = parse(
        r#"
            [[tables]]
            name = "items"
            sources = [{ provider = "csv", path = "primary.csv" }]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "generated"

            [[tables.fields]]
            name = "value"
            type = "string"
        "#,
    );

    assert_eq!(
        meta.table("items").unwrap().source_path(),
        Some("primary.csv")
    );
    assert_eq!(
        meta.table("items")
            .unwrap()
            .source_paths()
            .collect::<Vec<_>>(),
        vec!["primary.csv"]
    );
    assert_eq!(meta.table("generated").unwrap().source_path(), None);
    assert!(
        meta.table("generated")
            .unwrap()
            .source_paths()
            .next()
            .is_none()
    );
}

#[test]
fn exposes_all_declared_source_paths_for_variant_sources() {
    let model: MetaModel = toml::from_str(
        r#"
            [[structs]]
            name = "Stats"

            [[structs.fields]]
            name = "attack"
            type = "int"

            [[tables]]
            name = "items"
            polymorphic = true
            sources = [
                { provider = "csv", path = "weapons.csv", fragments = ["stats"] },
                { provider = "csv", path = "armors.csv", fragments = ["stats"] },
            ]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "stats"
            type = "Stats"
        "#,
    )
    .expect("raw meta should parse");

    let table = model.table("items").unwrap();
    assert_eq!(table.source_path(), Some("weapons.csv"));
    assert_eq!(
        table.source_paths().collect::<Vec<_>>(),
        vec!["weapons.csv", "armors.csv"]
    );
}

#[test]
fn rejects_target_state_fields_without_runtime_semantics() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
            field_number = 1
        "#,
    );
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        messages.contains("`field_number`") && messages.contains("not implemented yet"),
        "missing field_number unsupported diagnostic in\n{messages}"
    );
}

#[test]
fn rejects_unsupported_source_declarations() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "legacy.csv"
            sources = [
                { provider = "csv", path = "items.csv" },
                { provider = "lark", path = "items-sheet", fragments = ["stats"] },
            ]
            override_sources = [{ provider = "csv", path = "override.csv" }]

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidSourceDeclaration
            && diagnostic
                .message
                .contains("both legacy `source` and `sources`")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidSourceDeclaration
            && diagnostic.message.contains("unsupported provider `lark`")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic.message.contains("multiple sources")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidSourceDeclaration
            && diagnostic.message.contains("override_sources")
            && diagnostic.message.contains("primary index")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic.message.contains("fragments")
    }));
}

#[test]
fn rejects_base_source_export_when_until_target_specific_base_sources_are_supported() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            sources = [{ provider = "csv", path = "items.csv", export_when = { any = ["s1"] } }]

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic.message.contains("base source")
            && diagnostic.message.contains("export_when")
            && diagnostic.message.contains("override_sources")
    }));
}

#[test]
fn rejects_multi_source_outside_variant_source_mode() {
    let cases = [
        r#"
            [[tables]]
            name = "items"
            kind = "object"
            polymorphic = true
            sources = [
                { provider = "csv", path = "items.csv" },
                { provider = "csv", path = "items-extra.csv" },
            ]

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
        r#"
            [[tables]]
            name = "items"
            polymorphic = true
            sources = [
                { provider = "csv", path = "items.csv" },
                { provider = "csv", path = "items-extra.csv" },
            ]

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
        r#"
            [[tables]]
            name = "items"
            polymorphic = true
            sources = [
                { provider = "csv", path = "items.csv" },
                { provider = "csv", path = "items-extra.csv" },
            ]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "weapon"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
        "#,
    ];

    for input in cases {
        let diagnostics = diagnostics(input);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
                && diagnostic.message.contains("variant source mode")
        }));
    }
}

#[test]
fn accepts_multi_source_variant_source_mode() {
    let meta = parse(
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

            [[tables.fragments]]
            name = "armor"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
        "#,
    );

    assert!(validate_meta_model(&meta).is_empty());
}

#[test]
fn validates_typed_fragment_payload_shape() {
    let valid = parse(
        r#"
            [[structs]]
            name = "AttributeSet"

            [[structs.fields]]
            name = "attack"
            type = "int"

            [[tables]]
            name = "items"
            polymorphic = true
            sources = [{ provider = "csv", path = "weapons.csv", fragments = ["attributes"] }]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "attributes"
            type = "AttributeSet"
        "#,
    );
    assert!(validate_meta_model(&valid).is_empty());
    let linked = valid.link().expect("typed fragment should link");
    assert_eq!(
        linked.tables[0].fragments[0]
            .struct_type()
            .expect("payload should be struct")
            .meta
            .name,
        "AttributeSet"
    );

    let mixed = diagnostics(
        r#"
            [[structs]]
            name = "AttributeSet"

            [[tables]]
            name = "items"
            polymorphic = true

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "attributes"
            type = "AttributeSet"

            [[tables.fragments.fields]]
            name = "attack"
            type = "int"
        "#,
    );
    assert!(mixed.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic.message.contains("both `type` and inline fields")
    }));

    let unknown = diagnostics(
        r#"
            [[tables]]
            name = "items"
            polymorphic = true

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "attributes"
            type = "MissingStruct"
        "#,
    );
    assert!(unknown.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnknownFieldType
            && diagnostic.message.contains("MissingStruct")
    }));
}

#[test]
fn validates_primitive_and_enum_default_values() {
    let meta = parse(
        r#"
            [[enums]]
            name = "ItemRarity"
            [[enums.values]]
            name = "common"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "max_stack"
            type = "int"
            default = 1

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
            default = "common"
        "#,
    );
    assert!(meta.validate().is_ok());

    let diagnostics = diagnostics(
        r#"
            [[enums]]
            name = "ItemRarity"
            [[enums.values]]
            name = "common"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "max_stack"
            type = "int"
            default = "one"

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
            default = "legendary"
        "#,
    );

    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == MetaDiagnosticCode::InvalidDefaultValue)
            .count(),
        2
    );
}

#[test]
fn validates_struct_field_default_values() {
    let diagnostics = diagnostics(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "count"
            type = "int"
            default = "one"
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidDefaultValue
            && diagnostic.message.contains("struct `Reward` field `count`")
    }));
}

#[test]
fn validates_phase_one_struct_layout_rules() {
    let explicit_json_cell_diagnostics = diagnostics(
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
            layout = "json_cell"
        "#,
    );

    assert!(explicit_json_cell_diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic.message.contains("remove `layout`")
    }));

    let explicit_expanded_value_diagnostics = diagnostics(
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
            layout = "expanded_value"

            [[tables.fields]]
            name = "name"
            type = "string"
            layout = "json_cell"
        "#,
    );

    assert!(
        explicit_expanded_value_diagnostics
            .iter()
            .any(|diagnostic| {
                diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
                    && diagnostic.message.contains("remove `layout`")
            })
    );
    assert!(
        explicit_expanded_value_diagnostics
            .iter()
            .any(|diagnostic| {
                diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
                    && diagnostic
                        .message
                        .contains("non-struct fields only support scalar_cell")
            })
    );
}

#[test]
fn rejects_whole_struct_default_in_phase_one() {
    let diagnostics = diagnostics(
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
            default = { count = 1 }
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic
                .message
                .contains("whole-struct defaults are not implemented yet")
    }));
}

#[test]
fn rejects_struct_child_validators_until_nested_validation_is_supported() {
    let diagnostics = diagnostics(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "count"
            type = "int"

            [[structs.fields.validators]]
            kind = "range"
            min = 1
            max = 99
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic
                .message
                .contains("struct child validators are not implemented yet")
    }));
}

#[test]
fn rejects_duplicate_enum_struct_enum_value_and_index_names() {
    let diagnostics = diagnostics(
        r#"
            [[enums]]
            name = "ItemRarity"
            [[enums.values]]
            name = "common"
            [[enums.values]]
            name = "common"

            [[enums]]
            name = "ItemRarity"

            [[structs]]
            name = "Reward"
            [[structs.fields]]
            name = "count"
            type = "int"

            [[structs]]
            name = "Reward"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateEnumName)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateEnumValueName)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateStructName)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateIndexName)
    );
}

#[test]
fn rejects_duplicate_struct_field_names() {
    let diagnostics = diagnostics(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "count"
            type = "int"

            [[structs.fields]]
            name = "count"
            type = "float"
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateFieldName)
    );
}

#[test]
fn rejects_empty_index_fields_missing_primary_index_and_multiple_primary_flags() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "missing"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "name"
            type = "string"

            [[tables.indexes]]
            name = "empty"
            fields = []

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables.indexes]]
            name = "by_name"
            fields = ["name"]
            unique = true
            primary = true
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::EmptyIndexFields)
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidPrimaryIndex
            && diagnostic.message.contains("does not exist")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::PrimaryIndexMismatch
            && diagnostic.message.contains("multiple primary indexes")
    }));
}

#[test]
fn rejects_primary_flag_when_index_is_not_unique() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            primary = true
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidPrimaryIndex
            && diagnostic.message.contains("is primary but is not unique")
    }));
}

#[test]
fn rejects_field_path_reserved_name_characters() {
    let diagnostics = diagnostics(
        r#"
            [[structs]]
            name = "Stats"

            [[structs.fields]]
            name = "attack.power"
            type = "int"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward[0]"
            type = "string"

            [[tables.fragments]]
            name = "debug.note"
        "#,
    );

    let reserved_diagnostics = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
                && diagnostic
                    .message
                    .contains("reserved field path characters")
        })
        .count();
    assert_eq!(reserved_diagnostics, 3);
}

#[test]
fn rejects_ambiguous_or_reserved_custom_type_names() {
    let diagnostics = diagnostics(
        r#"
            [[enums]]
            name = "Reward"

            [[structs]]
            name = "Reward"

            [[structs]]
            name = "int"

            [[structs]]
            name = "MetaModel"
        "#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateTypeName)
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == MetaDiagnosticCode::ReservedTypeName)
            .count(),
        2
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("primitive field type"))
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("internal meta model type"))
    );
}

#[test]
fn link_reports_structural_validation_errors() {
    let invalid: MetaModel = toml::from_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables]]
            name = "items"
            source = "items-override.csv"
        "#,
    )
    .expect("raw invalid meta should deserialize");

    let diagnostics = match invalid.link() {
        Ok(_) => panic!("link should include structural validation"),
        Err(diagnostics) => diagnostics,
    };

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateTableName)
    );
}

#[test]
fn reports_parse_errors_for_string_and_file_inputs() {
    let string_error = MetaModel::from_toml_str("[[tables]\nname =").unwrap_err();
    assert!(matches!(string_error, ConfigForgeError::ParseMetaString(_)));

    let dir = tempdir().expect("temp dir should exist");
    let invalid_path = dir.path().join("invalid.toml");
    fs::write(&invalid_path, "[[tables]\nname =").expect("invalid meta should be written");

    let file_error = MetaModel::from_path(&invalid_path).unwrap_err();
    match file_error {
        ConfigForgeError::ParseMeta { path, .. } => {
            assert_eq!(path, invalid_path.canonicalize().unwrap());
        }
        other => panic!("expected ParseMeta error, got {other}"),
    }
}

#[test]
fn reports_read_errors_for_missing_meta_or_import_files() {
    let dir = tempdir().expect("temp dir should exist");
    let missing_path = dir.path().join("missing.toml");
    let missing_error = MetaModel::from_path(&missing_path).unwrap_err();
    assert!(matches!(missing_error, ConfigForgeError::ReadFile { .. }));
}

#[test]
fn loads_multiple_meta_files_as_one_model() {
    let dir = tempdir().expect("temp dir should exist");
    let root_path = dir.path().join("root.toml");
    let types_path = dir.path().join("types.toml");

    fs::write(
        &root_path,
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
        "#,
    )
    .expect("root meta should be written");
    fs::write(
        &types_path,
        r#"
            [[enums]]
            name = "ItemRarity"
            [[enums.values]]
            name = "common"
        "#,
    )
    .expect("types meta should be written");

    let meta = MetaModel::from_paths([&root_path, &types_path]).expect("meta graph should load");

    assert_eq!(meta.enums.len(), 1);
    assert_eq!(meta.tables.len(), 1);
    assert_eq!(meta.enum_meta("ItemRarity").unwrap().name, "ItemRarity");
    assert_eq!(meta.table("items").unwrap().name, "items");
}

#[test]
fn parses_field_and_table_validators() {
    let meta = parse(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.validators]]
            kind = "unique"
            fields = ["id"]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
            severity = "warning"
        "#,
    );

    let table = meta.table("items").unwrap();
    assert_eq!(table.validators[0].kind, "unique");
    assert_eq!(
        table.validators[0]
            .params
            .get("fields")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(1)
    );
    let field_validator = &table.field("id").unwrap().validators[0];
    assert_eq!(field_validator.kind, "range");
    assert_eq!(field_validator.severity, ValidationSeverity::Warning);
    assert_eq!(
        field_validator
            .params
            .get("min")
            .and_then(toml::Value::as_integer),
        Some(1)
    );
}

#[test]
fn rejects_empty_validator_kind() {
    let diagnostics = diagnostics(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields.validators]]
            kind = "  "
        "#,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::InvalidValidatorConfig
            && diagnostic.message.contains("kind must be non-empty")
    }));
}
