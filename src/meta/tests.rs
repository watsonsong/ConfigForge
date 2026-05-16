use std::fs;

use super::*;
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
            [project]
            name = "demo"
            copyright = "Copyright 2026 Demo Studio."

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

    assert_eq!(
        meta.project
            .as_ref()
            .and_then(|project| project.copyright.as_deref()),
        Some("Copyright 2026 Demo Studio.")
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
    assert_eq!(linked_primary_index.fields[0].name, "id");
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
    assert_eq!(primary_index.fields[0].name, "id");
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
    match table.field("rarity").unwrap().linked_field {
        LinkedField::Enum(enum_meta) => assert_eq!(enum_meta.name, "ItemRarity"),
        other => panic!("expected linked enum, got {other:?}"),
    }
    match table.field("reward").unwrap().linked_field {
        LinkedField::Struct(struct_meta) => assert_eq!(struct_meta.name, "Reward"),
        other => panic!("expected linked struct, got {other:?}"),
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
            imports = ["types.toml"]

            [[tables]]
            name = "items"
            source = "items.csv"
            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
        "#,
    )
    .expect("root meta should be written");

    let meta = MetaModel::from_path(&root_path).expect("imported meta should load");
    assert!(meta.enum_meta("ItemRarity").is_some());
    let linked_meta = meta.link().expect("imported meta should link");
    let field = linked_meta.table("items").unwrap().field("rarity").unwrap();
    match field.linked_field {
        LinkedField::Enum(enum_meta) => assert_eq!(enum_meta.name, "ItemRarity"),
        other => panic!("expected linked enum, got {other:?}"),
    }
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

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::UnknownFieldType));
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

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateTableName));
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

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateFieldName));
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

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::MissingFieldReference));
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

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::PrimaryIndexMismatch));
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

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::InvalidPrimaryIndex));
}
