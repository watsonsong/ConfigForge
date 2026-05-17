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
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateTableName));
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
    assert_eq!(meta.linked_struct(struct_field).unwrap().name, "Reward");
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
    assert_eq!(meta.table("generated").unwrap().source_path(), None);
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
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic.message.contains("override_sources")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == MetaDiagnosticCode::UnsupportedFeature
            && diagnostic.message.contains("fragments")
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

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateEnumName));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateEnumValueName));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateStructName));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateIndexName));
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

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::DuplicateFieldName));
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

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == MetaDiagnosticCode::EmptyIndexFields));
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
