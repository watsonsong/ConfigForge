// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ConfigForgeError,
    diagnostic::code::{
        CF_GENERATOR_INVALID_OPTION, CF_GENERATOR_UNKNOWN_TYPE, CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    },
    generator::{
        CSharpCodeGenerator, CppCodeGenerator, CppUnrealCodeGenerator, RustCodeGenerator,
        TypeScriptCodeGenerator, TypeScriptCodegenOptions,
    },
    meta::{MetaModel, TargetTags},
    runtime_contract::RuntimeDataFile,
};

use super::ir::{
    CodeBuiltinFieldOptions, CodeForeignKey, CodeRelationCardinality, relation_key_plan,
};
use super::*;

pub(crate) fn expect_single_diagnostic(error: ConfigForgeError) -> crate::Diagnostic {
    let ConfigForgeError::ValidationReport(report) = error else {
        panic!("expected structured diagnostic report");
    };
    assert_eq!(report.diagnostics.len(), 1);
    report.diagnostics.into_iter().next().unwrap()
}

pub(crate) fn linked_fixture() -> MetaModel {
    MetaModel::from_toml_str(
        r#"
            [[enums]]
            name = "ItemRarity"
            desc = "Item rarity."

            [[enums.values]]
            name = "common"

            [[enums.values]]
            name = "rare"

            [[structs]]
            name = "RewardInfo"

            [[structs.fields]]
            name = "count"
            type = "int"
            required = true

            [[tables]]
            name = "items"
            desc = "Item table."
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true
            unique = true

            [[tables.fields]]
            name = "display_name"
            type = "string"
            required = true

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
            required = true

            [[tables.fields]]
            name = "internal_note"
            type = "string"
            export = false

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables.indexes]]
            name = "by_rarity"
            fields = ["rarity"]
            unique = false
        "#,
    )
    .expect("fixture meta should parse")
}

#[test]
fn maps_language_to_default_module_name() {
    assert_eq!(default_module_name("rust"), Some("config_forge_generated"));
    assert_eq!(default_module_name("cpp"), Some("config_forge_generated"));
    assert_eq!(
        default_module_name("typescript"),
        Some("configForgeGenerated")
    );
}

pub(crate) fn source_order_fixture() -> MetaModel {
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
            name = "character_surnames"
            source = "character_surnames.csv"
            preserve_source_order = true

            [[tables.fields]]
            name = "value"
            type = "string"
            required = true
        "#,
    )
    .expect("source-order fixture meta should parse")
}

pub(crate) fn indexed_source_order_fixture() -> MetaModel {
    MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"
            preserve_source_order = true

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "rarity"
            type = "string"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables.indexes]]
            name = "by_rarity"
            fields = ["rarity"]
            unique = false
        "#,
    )
    .expect("indexed source-order fixture meta should parse")
}

pub(crate) fn text_fixture() -> MetaModel {
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
            unique = true

            [[tables.fields]]
            name = "name"
            type = "text"
            required = true

            [[tables.fields]]
            name = "subtitle"
            type = "text"
            required = false

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#,
    )
    .expect("text fixture meta should parse")
}

pub(crate) fn input_for<'a>(meta: &'a MetaModel, language: &str) -> CodeExportInput<'a> {
    let linked = meta.link().expect("fixture meta should link");
    let linked = Box::leak(Box::new(linked));
    CodeExportInput {
        meta: linked,
        target_tags: TargetTags::empty(),
        project: CodeExportProject {
            name: Some("demo".to_owned()),
            desc: Some("Demo config".to_owned()),
            copyright: Some("Copyright Demo".to_owned()),
        },
        project_root: PathBuf::from("."),
        target_name: "test".to_owned(),
        target: CodeExportTarget {
            language: language.to_owned(),
            out_dir: PathBuf::from("out"),
            file_name: None,
            module_name: "generated".to_owned(),
            namespace: None,
            options: CodegenOptions::for_language(language).expect("built-in language options"),
        },
        runtime_data_files: vec![RuntimeDataFile {
            table_name: "items".to_owned(),
            path: "items.json".to_owned(),
            format: "json".to_owned(),
        }],
        generated_at: "2026-05-17T00:00:00Z".to_owned(),
    }
}

#[test]
fn builds_code_model_with_types_tables_and_indexes() {
    let meta = linked_fixture();
    let input = input_for(&meta, RustCodeGenerator.language());

    let model =
        build_code_model(input.meta, &input.target_tags, &input.runtime_data_files).expect("model");

    assert_eq!(model.enums[0].name, "ItemRarity");
    assert_eq!(model.structs[0].name, "RewardInfo");
    assert_eq!(model.tables[0].name, "items");
    assert_eq!(model.tables[0].kind, CodeTableKind::Collection);
    assert!(
        model.tables[0]
            .fields
            .iter()
            .any(|field| field.name == "display_name")
    );
    assert!(
        !model.tables[0]
            .fields
            .iter()
            .any(|field| field.name == "internal_note")
    );
    assert!(
        model.tables[0]
            .indexes
            .iter()
            .any(|index| index.name == "by_id" && index.unique)
    );
    assert!(
        model.tables[0]
            .indexes
            .iter()
            .any(|index| index.name == "by_rarity" && !index.unique)
    );
    assert_eq!(model.tables[0].data_file, "items");
}

#[test]
fn code_model_preserves_builtin_color_and_vector_type_metadata() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "SpawnPoint"

            [[structs.fields]]
            name = "grid"
            type = "vector2_int"

            [[tables]]
            name = "effects"
            source = "effects.csv"

            [[tables.fields]]
            name = "id"
            type = "string"

            [[tables.fields]]
            name = "tint"
            type = "color"
            color_channel = "normalized"

            [[tables.fields]]
            name = "position"
            type = "vector3"
            vector_components = ["right", "up", "forward"]

            [[tables.fields]]
            name = "spawn"
            type = "SpawnPoint"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let model = build_code_model(&linked, &TargetTags::empty(), &[]).expect("model");
    let effects = model
        .tables
        .iter()
        .find(|table| table.name == "effects")
        .expect("effects table should exist");
    let field_type = |name: &str| {
        effects
            .fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| &field.code_type)
            .expect("field should exist")
    };

    assert_eq!(field_type("tint"), &CodeTypeRef::Color);
    let tint = effects
        .fields
        .iter()
        .find(|field| field.name == "tint")
        .expect("tint field should exist");
    assert!(matches!(
        tint.builtin_options,
        Some(CodeBuiltinFieldOptions::Color(_))
    ));
    assert_eq!(
        field_type("position"),
        &CodeTypeRef::Vector {
            dimension: 3,
            element: CodeVectorElementType::Float,
            components: vec!["right".to_owned(), "up".to_owned(), "forward".to_owned()],
        }
    );
    let position = effects
        .fields
        .iter()
        .find(|field| field.name == "position")
        .expect("position field should exist");
    let Some(CodeBuiltinFieldOptions::Vector(vector_options)) = &position.builtin_options else {
        panic!("position should carry vector builtin options");
    };
    assert_eq!(vector_options.components, vec!["right", "up", "forward"]);
    assert_eq!(
        model.structs[0].fields[0].code_type,
        CodeTypeRef::Vector {
            dimension: 2,
            element: CodeVectorElementType::Int,
            components: vec!["x".to_owned(), "y".to_owned()],
        }
    );
}

#[test]
fn code_model_preserves_source_order_opt_in() {
    let meta = source_order_fixture();
    let input = input_for(&meta, RustCodeGenerator.language());

    let model =
        build_code_model(input.meta, &input.target_tags, &input.runtime_data_files).expect("model");

    let items = model
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("indexed table should exist");
    let surnames = model
        .tables
        .iter()
        .find(|table| table.name == "character_surnames")
        .expect("source-order table should exist");
    assert!(!items.preserve_source_order);
    assert!(surnames.preserve_source_order);
    assert!(surnames.indexes.is_empty());
}

#[test]
fn code_model_marks_object_tables_and_skips_indexes() {
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
            name = "client_note"
            type = "string"
            export_when = { any = ["client"] }
        "#,
    )
    .expect("meta should load");
    let input = input_for(&meta, RustCodeGenerator.language());

    let model = build_code_model(input.meta, &TargetTags::new(["client"]), &[]).expect("model");

    assert_eq!(model.tables[0].name, "game_config");
    assert_eq!(model.tables[0].kind, CodeTableKind::Object);
    assert!(model.tables[0].indexes.is_empty());
    assert!(model.tables[0].foreign_keys.is_empty());
    assert_eq!(model.tables[0].data_file, "game_config");
}

#[test]
fn code_model_includes_object_table_foreign_keys() {
    let meta = MetaModel::from_toml_str(
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
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "starter_item_id"
            type = "int"
            required = false
            foreign_key = { name = "starter_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let model = build_code_model(&linked, &TargetTags::empty(), &[]).expect("model");
    let game_config = model
        .tables
        .iter()
        .find(|table| table.name == "game_config")
        .expect("object table should exist");

    assert_eq!(game_config.kind, CodeTableKind::Object);
    assert!(game_config.indexes.is_empty());
    assert_eq!(game_config.foreign_keys.len(), 1);
    assert_eq!(game_config.foreign_keys[0].name, "starter_item");
    assert_eq!(game_config.foreign_keys[0].target_table, "items");
    assert_eq!(game_config.foreign_keys[0].target_index, "by_id");
    assert!(game_config.foreign_keys[0].allow_empty);
}

#[test]
fn code_model_classifies_linked_field_relation_cardinality() {
    let meta = MetaModel::from_toml_str(
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
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "default_item_id"
            type = "int"
            required = true
            foreign_key = { name = "default_item", target_table = "items", linked_field = "default_item" }

            [[tables.fields]]
            name = "bonus_item_id"
            type = "int"
            required = false
            foreign_key = { name = "bonus_item", target_table = "items", linked_field = "bonus_item" }

            [[tables.fields]]
            name = "starter_item_ids"
            type = "int[]"
            required = true
            foreign_key = { name = "starter_item", target_table = "items", linked_field = "starter_items" }
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let model = build_code_model(&linked, &TargetTags::empty(), &[]).expect("model");
    let game_config = model
        .tables
        .iter()
        .find(|table| table.name == "game_config")
        .expect("object table should exist");

    let cardinality = |foreign_key_name: &str| {
        game_config
            .foreign_keys
            .iter()
            .find(|foreign_key| foreign_key.name == foreign_key_name)
            .and_then(|foreign_key| foreign_key.linked_field.as_ref())
            .map(|linked_field| linked_field.cardinality)
            .expect("linked field should exist")
    };
    assert_eq!(cardinality("default_item"), CodeRelationCardinality::One);
    assert_eq!(
        cardinality("bonus_item"),
        CodeRelationCardinality::OptionalOne
    );
    assert_eq!(cardinality("starter_item"), CodeRelationCardinality::Many);
}

#[test]
fn relation_key_plan_centralizes_optional_empty_semantics() {
    let required = CodeField {
        name: "class".to_owned(),
        code_type: CodeTypeRef::String,
        builtin_options: None,
        desc: None,
        required: true,
    };
    let optional_level = CodeField {
        name: "level".to_owned(),
        code_type: CodeTypeRef::Int,
        builtin_options: None,
        desc: None,
        required: false,
    };
    let optional_variant = CodeField {
        name: "variant".to_owned(),
        code_type: CodeTypeRef::String,
        builtin_options: None,
        desc: None,
        required: false,
    };

    let mixed_key = CodeForeignKey {
        name: "mixed_key".to_owned(),
        desc: None,
        fields: vec![required.clone(), optional_level.clone()],
        target_table: "targets".to_owned(),
        target_index: "by_key".to_owned(),
        allow_empty: true,
        linked_field: None,
    };
    let mixed_plan = relation_key_plan(&mixed_key);
    assert_eq!(mixed_plan.key_fields.len(), 2);
    assert_eq!(mixed_plan.optional_key_fields.len(), 1);
    assert!(!mixed_plan.all_key_fields_optional);
    assert!(mixed_plan.should_report_partial_empty);

    let optional_single_key = CodeForeignKey {
        name: "optional_single_key".to_owned(),
        desc: None,
        fields: vec![optional_level.clone()],
        target_table: "targets".to_owned(),
        target_index: "by_key".to_owned(),
        allow_empty: true,
        linked_field: None,
    };
    let optional_single_plan = relation_key_plan(&optional_single_key);
    assert_eq!(optional_single_plan.optional_key_fields.len(), 1);
    assert!(optional_single_plan.all_key_fields_optional);
    assert!(!optional_single_plan.should_report_partial_empty);

    let optional_composite_key = CodeForeignKey {
        name: "optional_composite_key".to_owned(),
        desc: None,
        fields: vec![optional_level, optional_variant],
        target_table: "targets".to_owned(),
        target_index: "by_key".to_owned(),
        allow_empty: true,
        linked_field: None,
    };
    let optional_composite_plan = relation_key_plan(&optional_composite_key);
    assert_eq!(optional_composite_plan.optional_key_fields.len(), 2);
    assert!(optional_composite_plan.all_key_fields_optional);
    assert!(optional_composite_plan.should_report_partial_empty);
}

#[test]
fn rust_codegen_emits_linked_field_relation_api() {
    let meta = MetaModel::from_toml_str(
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
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "default_item_id"
            type = "int"
            required = true
            foreign_key = { name = "default_item", target_table = "items", linked_field = "default_item" }

            [[tables.fields]]
            name = "starter_item_ids"
            type = "int[]"
            required = true
            foreign_key = { name = "starter_item", target_table = "items", linked_field = "starter_items" }
        "#,
    )
    .expect("meta should load");
    let input = input_for(&meta, RustCodeGenerator.language());

    let files = RustCodeGenerator
        .generate(&input)
        .expect("rust should generate");
    let content = &files
        .iter()
        .find(|file| file.path.ends_with("generated.rs"))
        .expect("generated rust file should exist")
        .content;

    assert!(content.contains("enum LinkCache"));
    assert!(content.contains("struct LinkListCache"));
    assert!(content.contains("default_item_link: Cell<LinkCache>"));
    assert!(content.contains("starter_items_link: RefCell<LinkListCache>"));
    assert!(content.contains("pub fn default_item<'a>(&self, data: &'a impl GameDataAccess)"));
    assert!(content.contains("pub fn starter_items<'a>(&self, data: &'a impl GameDataAccess)"));
    assert!(content.contains("fn row_handle_by_id(&self, id: i64) -> Option<usize>"));
    assert!(!content.contains("fn index_of_by_id(&self, id: i64) -> Option<usize>"));
    assert!(
        content.contains("pub fn link(&self, data: &impl GameDataAccess) -> RuntimeDataResult<()>")
    );
    assert!(content.contains("pub fn link_all(&self) -> RuntimeDataResult<()>"));
}

#[test]
fn code_model_includes_fragment_structs_and_optional_row_members() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "AttributeSet"

            [[structs.fields]]
            name = "attack"
            type = "int"

            [[tables]]
            name = "items"
            source = "items.csv"
            polymorphic = true

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "weapon"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"

            [[tables.fragments.fields]]
            name = "internal_note"
            type = "string"
            export = false

            [[tables.fragments]]
            name = "attributes"
            type = "AttributeSet"

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
        "#,
    )
    .expect("meta should load");
    let input = input_for(&meta, RustCodeGenerator.language());

    let model = build_code_model(input.meta, &TargetTags::empty(), &input.runtime_data_files)
        .expect("model should build");
    assert_eq!(model.tables[0].fragments[0].name, "weapon");
    assert!(
        model.tables[0].fragments[0]
            .fields
            .iter()
            .any(|field| field.name == "power")
    );
    assert!(
        !model.tables[0].fragments[0]
            .fields
            .iter()
            .any(|field| field.name == "internal_note")
    );
    let attributes = model.tables[0]
        .fragments
        .iter()
        .find(|fragment| fragment.name == "attributes")
        .expect("typed fragment should be generated");
    assert_eq!(
        attributes.payload,
        CodeFragmentPayload::Struct {
            name: "AttributeSet".to_owned()
        }
    );

    let client_model = build_code_model(input.meta, &TargetTags::new(["client"]), &[])
        .expect("client model should build");
    let client_marker = client_model.tables[0]
        .fragments
        .iter()
        .find(|fragment| fragment.name == "client_marker")
        .expect("client marker should be generated");
    assert!(client_marker.fields.is_empty());
    assert!(
        !client_model.tables[0]
            .fragments
            .iter()
            .any(|fragment| fragment.name == "server_marker")
    );
}

#[test]
fn code_model_uses_logical_table_name_when_no_runtime_file_is_available() {
    let meta = linked_fixture();
    let linked = meta.link().expect("fixture meta should link");

    let model = build_code_model(&linked, &TargetTags::empty(), &[]).expect("model");

    assert_eq!(model.tables[0].data_file, "items");
}

#[test]
fn code_model_includes_visible_foreign_keys() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            polymorphic = true
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
            required = false
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let model = build_code_model(&linked, &TargetTags::empty(), &[]).expect("model");
    let monsters = model
        .tables
        .iter()
        .find(|table| table.name == "monsters")
        .expect("monsters table should exist");
    assert_eq!(monsters.foreign_keys.len(), 1);
    assert_eq!(monsters.foreign_keys[0].name, "drop_item");
    assert_eq!(monsters.foreign_keys[0].target_table, "items");
    assert_eq!(monsters.foreign_keys[0].target_index, "by_id");
    assert!(monsters.foreign_keys[0].allow_empty);
}

#[test]
fn code_model_rejects_visible_foreign_key_to_hidden_target_index() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            polymorphic = true
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
            export_when = { any = ["server"] }

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
    let linked = meta.link().expect("meta should link");

    let error = build_code_model(&linked, &TargetTags::new(["client"]), &[]).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_DATA_INPUT);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.codegen.model")
    );
    assert!(
        diagnostic
            .message
            .contains("target index `items.by_id` which is not exported")
    );
    assert_eq!(diagnostic.location.table.as_deref(), Some("monsters"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("items.by_id"));
    assert_eq!(diagnostic.expected.as_deref(), Some("target-visible index"));
}

#[test]
fn code_model_skips_foreign_key_when_export_when_does_not_match() {
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
    let linked = meta.link().expect("meta should link");

    let model = build_code_model(&linked, &TargetTags::new(["client"]), &[]).expect("model");
    let monsters = model
        .tables
        .iter()
        .find(|table| table.name == "monsters")
        .expect("monsters table should exist");
    assert!(monsters.foreign_keys.is_empty());
}

#[test]
fn code_model_rejects_visible_foreign_key_to_hidden_source_field() {
    let meta = MetaModel::from_toml_str(
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
    let linked = meta.link().expect("meta should link");

    let error = build_code_model(&linked, &TargetTags::new(["client"]), &[]).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_DATA_INPUT);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.codegen.model")
    );
    assert!(
        diagnostic
            .message
            .contains("non-exported source field `drop_item_id`")
    );
    assert_eq!(diagnostic.location.table.as_deref(), Some("monsters"));
    assert_eq!(diagnostic.location.field.as_deref(), Some("drop_item_id"));
    assert_eq!(
        diagnostic.expected.as_deref(),
        Some("target-visible source field")
    );
}

#[test]
fn code_model_rejects_indexes_referencing_non_exported_fields() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "internal_region"
            type = "string"
            export_when = { any = ["server"] }

            [[tables.indexes]]
            name = "by_id_region"
            fields = ["id", "internal_region"]
            unique = true
            primary = true
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let error = build_code_model(&linked, &TargetTags::new(["client"]), &[]).unwrap_err();

    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_DATA_INPUT);
    assert!(
        diagnostic.message.contains(
            "primary index `by_id_region` references non-exported field `internal_region`"
        )
    );
    assert_eq!(diagnostic.location.table.as_deref(), Some("items"));
    assert_eq!(
        diagnostic.location.field.as_deref(),
        Some("internal_region")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("internal_region"));
    assert_eq!(
        diagnostic.expected.as_deref(),
        Some("exported primary index field for current target")
    );
}

#[test]
fn code_model_filters_tables_fields_and_indexes_by_target_tags() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "icon"
            type = "asset"
            asset_kind = "ui_icon"
            export_when = { any = ["client"] }

            [[tables.fields]]
            name = "drop_weight"
            type = "int"
            export_when = { any = ["server"] }

            [[tables.indexes]]
            name = "by_drop_weight"
            fields = ["drop_weight"]
            unique = false

            [[tables]]
            name = "server_only"
            source = "server_only.csv"
            export_when = { any = ["server"] }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[asset_kinds]]
            name = "ui_icon"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let model = build_code_model(&linked, &TargetTags::new(["client"]), &[]).expect("model");

    assert!(model.tables.iter().any(|table| table.name == "items"));
    assert!(!model.tables.iter().any(|table| table.name == "server_only"));
    let items = model
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table should exist");
    assert!(items.fields.iter().any(|field| field.name == "icon"));
    assert!(!items.fields.iter().any(|field| field.name == "drop_weight"));
    assert!(
        !items
            .indexes
            .iter()
            .any(|index| index.name == "by_drop_weight")
    );
}

#[test]
fn code_model_rejects_unsupported_index_key_types() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "ratio"
            type = "float"

            [[tables.indexes]]
            name = "by_ratio"
            fields = ["ratio"]
            unique = true
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let error = build_code_model(&linked, &TargetTags::empty(), &[]).unwrap_err();

    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_DATA_INPUT);
    assert!(diagnostic.message.contains("unsupported index key type"));
    assert_eq!(diagnostic.location.table.as_deref(), Some("items"));
    assert_eq!(diagnostic.location.field.as_deref(), Some("ratio"));
    assert!(
        diagnostic
            .current_value
            .as_deref()
            .unwrap()
            .contains("Float")
    );
    assert!(
        diagnostic
            .expected
            .as_deref()
            .unwrap()
            .contains("index key type")
    );
}

pub(crate) fn assert_duplicate_generated_identifier(
    result: crate::Result<Vec<GeneratedFile>>,
    language: &str,
    identifier: &str,
    scope: &str,
) {
    let error = result.expect_err("generator should reject duplicate generated symbols");
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_DATA_INPUT);
    let expected_language = language.strip_suffix(" codegen").unwrap_or(language);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some(format!("generator.{expected_language}.naming").as_str())
    );
    assert!(diagnostic.message.contains(language));
    assert!(diagnostic.message.contains(identifier));
    assert!(diagnostic.message.contains(scope));
    let expected_current_value = identifier
        .strip_prefix("duplicate generated identifier `")
        .and_then(|value| value.strip_suffix('`'))
        .unwrap_or(identifier);
    assert_eq!(
        diagnostic.current_value.as_deref(),
        Some(expected_current_value)
    );
}

#[test]
fn naming_handles_reserved_words_and_cases() {
    assert_eq!(
        crate::code_artifact::naming::pascal_case("item-table"),
        "ItemTable"
    );
    assert_eq!(crate::code_artifact::naming::snake_case("type"), "type");
    assert_eq!(
        crate::code_artifact::naming::make_identifier(
            "type",
            CaseStyle::SnakeCase,
            KeywordConflictPolicy::SuffixUnderscore,
            crate::code_artifact::naming::is_rust_keyword,
            |identifier| Some(format!("r#{identifier}")),
        )
        .expect("identifier"),
        "type_"
    );
    assert!(crate::code_artifact::naming::is_rust_keyword("gen"));
    assert_eq!(
        crate::code_artifact::naming::make_identifier(
            "gen",
            CaseStyle::SnakeCase,
            KeywordConflictPolicy::SuffixUnderscore,
            crate::code_artifact::naming::is_rust_keyword,
            |identifier| Some(format!("r#{identifier}")),
        )
        .expect("identifier"),
        "gen_"
    );
    assert_eq!(
        crate::code_artifact::naming::camel_case("item-name"),
        "itemName"
    );
}

#[test]
fn parses_codegen_task_config_and_merges_naming_defaults() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "typescript"
            output = "Generated/TS"
            file_name = "game_data"
            file_header = "// {{project.name}} {{datetime}}"
            keyword_conflict = "error"

            [i18n]
            mode = "global_function"
            function = "translateText"
            returns = "optional"
            csharp_using = "Demo.Localization"
            typescript_import = "import { translateText } from './i18n'"

            [naming]
            field = "snake_case"

            [formatting]
            indent_style = "tab"
            indent_width = 8
            trailing_blank_line = true

            [typescript]
            readonly_fields = true
        "#,
    )
    .expect("config should parse");

    let target = CodeExportTarget::from_task(config, &TypeScriptCodeGenerator).expect("target");

    assert_eq!(target.language, TypeScriptCodeGenerator.language());
    assert_eq!(target.out_dir, PathBuf::from("Generated/TS"));
    assert_eq!(target.file_name.as_deref(), Some("game_data"));
    assert_eq!(target.single_file_name("generated", "ts"), "game_data.ts");
    assert_eq!(
        target.options.keyword_conflict,
        KeywordConflictPolicy::Error
    );
    assert_eq!(target.options.naming.type_name, CaseStyle::PascalCase);
    assert_eq!(target.options.i18n.mode, I18nMode::GlobalFunction);
    assert_eq!(
        target.options.i18n.function.as_deref(),
        Some("translateText")
    );
    assert_eq!(target.options.i18n.returns, I18nFunctionReturn::Optional);
    assert_eq!(
        target.options.i18n.csharp_using.as_deref(),
        Some("Demo.Localization")
    );
    assert_eq!(
        target.options.i18n.typescript_import.as_deref(),
        Some("import { translateText } from './i18n'")
    );
    assert_eq!(target.options.naming.field, CaseStyle::SnakeCase);
    assert_eq!(target.options.naming.function, CaseStyle::CamelCase);
    assert_eq!(target.options.formatting.indent_style, IndentStyle::Tab);
    assert_eq!(target.options.formatting.indent_width, 8);
    assert!(target.options.formatting.trailing_newline);
    assert!(target.options.formatting.trailing_blank_line);
    assert_eq!(
        target
            .options
            .language_options::<TypeScriptCodegenOptions>(TypeScriptCodeGenerator.language())
            .expect("typescript options should parse"),
        TypeScriptCodegenOptions {
            profile: Default::default(),
            readonly_fields: true,
            cocos_import: None,
        }
    );
}

#[test]
fn rejects_global_i18n_without_function() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "rust"
            output = "Generated/Rust"

            [i18n]
            mode = "global_function"
        "#,
    )
    .expect("config should parse");

    let error = CodeExportTarget::from_task(config, &RustCodeGenerator).unwrap_err();

    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(diagnostic.source.as_deref(), Some("generator.codegen"));
    assert!(diagnostic.message.contains("requires `function`"));
    assert_eq!(
        diagnostic.current_value.as_deref(),
        Some("missing i18n.function")
    );
    assert_eq!(diagnostic.expected.as_deref(), Some("global function name"));
}

#[test]
fn global_i18n_function_changes_text_helper_signatures() {
    let meta = text_fixture();

    let mut rust_input = input_for(&meta, RustCodeGenerator.language());
    rust_input.target.options.i18n = I18nOptions {
        mode: I18nMode::GlobalFunction,
        function: Some("crate::i18n::translate".to_owned()),
        returns: I18nFunctionReturn::Optional,
        ..Default::default()
    };
    let rust = RustCodeGenerator.generate(&rust_input).expect("rust code")[0]
        .content
        .clone();
    assert!(!rust.contains("pub trait TextTranslator"));
    assert!(rust.contains("pub fn name_text(&self) -> String"));
    assert!(rust.contains("crate::i18n::translate(Some(\"items.name\"), &self.name).unwrap_or_else(|| self.name.clone())"));
    assert!(rust.contains("pub fn subtitle_text(&self) -> Option<String>"));

    let mut cpp_input = input_for(&meta, CppCodeGenerator.language());
    cpp_input.target.options.i18n = I18nOptions {
        mode: I18nMode::GlobalFunction,
        function: Some("TranslateText".to_owned()),
        returns: I18nFunctionReturn::Optional,
        cpp_include: Some("\"i18n.hpp\"".to_owned()),
        ..Default::default()
    };
    let cpp = CppCodeGenerator.generate(&cpp_input).expect("cpp code")[0]
        .content
        .clone();
    assert!(cpp.contains("#include \"i18n.hpp\""));
    assert!(!cpp.contains("using TextTranslator"));
    assert!(cpp.contains("std::string NameText() const"));
    assert!(cpp.contains("auto translated = TranslateText(\"items.name\", name.c_str()); return translated.has_value() ? *translated : name;"));

    let mut ts_input = input_for(&meta, TypeScriptCodeGenerator.language());
    ts_input.target.options.i18n = I18nOptions {
        mode: I18nMode::GlobalFunction,
        function: Some("translateText".to_owned()),
        returns: I18nFunctionReturn::Optional,
        typescript_import: Some("import { translateText } from './i18n';".to_owned()),
        ..Default::default()
    };
    let ts = TypeScriptCodeGenerator
        .generate(&ts_input)
        .expect("typescript code")[0]
        .content
        .clone();
    assert!(ts.contains("import { translateText } from './i18n';"));
    assert!(!ts.contains("export type TextTranslator"));
    assert!(ts.contains("export function itemsNameText(value: { name: string }): string"));
    assert!(ts.contains("return (translateText(\"items.name\", value.name) ?? value.name);"));
}

#[test]
fn rejects_codegen_file_name_with_path_separator() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "rust"
            output = "Generated/Rust"
            file_name = "nested/generated.rs"
        "#,
    )
    .expect("config should parse");

    let error = CodeExportTarget::from_task(config, &RustCodeGenerator).unwrap_err();

    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(diagnostic.source.as_deref(), Some("generator.codegen"));
    assert!(
        diagnostic
            .message
            .contains("invalid generated code file name")
    );
    assert_eq!(
        diagnostic.current_value.as_deref(),
        Some("nested/generated.rs")
    );
    assert_eq!(
        diagnostic.expected.as_deref(),
        Some("single file name without path separators")
    );
}

#[test]
fn codegen_options_report_structured_language_and_option_errors() {
    let diagnostic = expect_single_diagnostic(CodegenOptions::for_language("unknown").unwrap_err());
    assert_eq!(diagnostic.code, CF_GENERATOR_UNKNOWN_TYPE);
    assert_eq!(diagnostic.source.as_deref(), Some("generator.codegen"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("unknown"));
    assert!(
        diagnostic
            .expected
            .as_deref()
            .unwrap()
            .contains("registered code generator")
    );

    let options = CodegenOptions {
        language_options: std::collections::HashMap::from([(
            TypeScriptCodeGenerator.language().to_owned(),
            toml::Value::String("not a table".to_owned()),
        )]),
        ..CodegenOptions::for_language(TypeScriptCodeGenerator.language()).expect("defaults")
    };
    let diagnostic = expect_single_diagnostic(
        options
            .language_options::<TypeScriptCodegenOptions>(TypeScriptCodeGenerator.language())
            .unwrap_err(),
    );
    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(diagnostic.source.as_deref(), Some("generator.typescript"));
    assert!(
        diagnostic
            .current_value
            .as_deref()
            .unwrap()
            .contains("not a table")
    );
}

#[test]
fn builtin_generators_use_language_specific_formatting_defaults() {
    fn accepts_code_artifact_options(_: &crate::code_artifact::CodeArtifactOptions) {}

    accepts_code_artifact_options(
        &CodegenOptions::for_language(RustCodeGenerator.language()).expect("rust options"),
    );
    assert_eq!(
        CodegenOptions::for_language(RustCodeGenerator.language())
            .expect("rust options")
            .formatting
            .indent_width,
        4
    );
    assert_eq!(
        CodegenOptions::for_language(CppCodeGenerator.language())
            .expect("cpp options")
            .formatting
            .indent_width,
        4
    );
    assert_eq!(
        CodegenOptions::for_language(CppUnrealCodeGenerator.language())
            .expect("cpp_unreal options")
            .formatting
            .indent_width,
        4
    );
    assert_eq!(
        CodegenOptions::for_language(CSharpCodeGenerator.language())
            .expect("csharp options")
            .formatting
            .indent_width,
        4
    );
    assert_eq!(
        CodegenOptions::for_language(TypeScriptCodeGenerator.language())
            .expect("typescript options")
            .formatting
            .indent_width,
        2
    );
}

#[test]
fn generated_content_respects_trailing_blank_line_option() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, TypeScriptCodeGenerator.language());
    input.target.options.formatting.trailing_blank_line = true;

    let files = TypeScriptCodeGenerator
        .generate(&input)
        .expect("typescript code should generate");

    assert!(files[0].content.ends_with("\n\n"));
}

#[test]
fn write_file_header_replaces_project_variables_and_datetime() {
    let meta = MetaModel::from_toml_str("").expect("meta should parse");
    let mut input = input_for(&meta, RustCodeGenerator.language());
    input.target.options.file_header = Some(
        "// {{project.name}} | {{project.desc}} | {{project.copyright}} | {{datetime}}".to_owned(),
    );
    let mut writer = CodeWriter::new(input.target.options.formatting);
    RustCodeGenerator.write_file_header(&mut writer, &input);

    assert_eq!(
        writer.finish(),
        "// demo | Demo config | Copyright Demo | 2026-05-17T00:00:00Z\n"
    );
}

#[test]
fn default_file_header_uses_generator_comment_prefix() {
    struct HashCommentGenerator;

    impl CodeGenerator for HashCommentGenerator {
        fn language(&self) -> &'static str {
            "hash"
        }

        fn default_naming(&self) -> NamingConfig {
            NamingConfig {
                type_name: CaseStyle::Preserve,
                enum_value: CaseStyle::Preserve,
                field: CaseStyle::Preserve,
                function: CaseStyle::Preserve,
                constant: CaseStyle::Preserve,
            }
        }

        fn line_comment_prefix(&self) -> &'static str {
            "#"
        }

        fn generate(&self, _input: &CodeExportInput<'_>) -> crate::Result<Vec<GeneratedFile>> {
            Ok(Vec::new())
        }

        fn generate_to_sink(
            &self,
            _input: &CodeExportInput<'_>,
            _sink: &crate::artifact::ArtifactSink,
        ) -> crate::Result<()> {
            Ok(())
        }
    }

    let meta = MetaModel::from_toml_str("").expect("meta should parse");
    let input = input_for(&meta, RustCodeGenerator.language());
    let mut writer = CodeWriter::new(input.target.options.formatting);

    HashCommentGenerator.write_file_header(&mut writer, &input);

    assert_eq!(
        writer.finish(),
        "# Copyright (c) 2026 ConfigForge contributors\n# SPDX-License-Identifier: Apache-2.0\n\n# Generated by config-forge. Do not edit.\n"
    );
}
