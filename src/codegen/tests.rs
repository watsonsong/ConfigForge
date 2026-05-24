// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ConfigForgeError,
    exporter::RuntimeDataFile,
    meta::{MetaModel, TargetTags},
};

use super::*;

fn linked_fixture() -> MetaModel {
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

            [[tables]]
            name = "items"
            desc = "Item table."
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            unique = true

            [[tables.fields]]
            name = "display_name"
            type = "string"

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"

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

fn input_for<'a>(meta: &'a MetaModel, language: CodeExportLanguage) -> CodeExportInput<'a> {
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
        target: CodeExportTarget {
            language,
            out_dir: PathBuf::from("out"),
            module_name: "generated".to_owned(),
            namespace: None,
            options: CodegenOptions::for_language(language),
        },
        runtime_data_files: vec![RuntimeDataFile {
            table_name: "items".to_owned(),
            path: "items.json".to_owned(),
        }],
        generated_at: "2026-05-17T00:00:00Z".to_owned(),
    }
}

#[test]
fn builds_code_model_with_types_tables_and_indexes() {
    let meta = linked_fixture();
    let input = input_for(&meta, CodeExportLanguage::Rust);

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
    assert_eq!(model.tables[0].data_file, "items.json");
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
    let input = input_for(&meta, CodeExportLanguage::Rust);

    let model = build_code_model(input.meta, &TargetTags::new(["client"]), &[]).expect("model");

    assert_eq!(model.tables[0].name, "game_config");
    assert_eq!(model.tables[0].kind, CodeTableKind::Object);
    assert!(model.tables[0].indexes.is_empty());
    assert!(model.tables[0].foreign_keys.is_empty());
    assert_eq!(model.tables[0].data_file, "game_config.json");
}

#[test]
fn codegen_includes_fragment_structs_and_optional_row_members() {
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
    let input = input_for(&meta, CodeExportLanguage::Rust);

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

    let rust = RustCodeGenerator
        .generate(&input_for(&meta, CodeExportLanguage::Rust))
        .expect("rust code");
    let rust_content = &rust[0].content;
    assert!(rust_content.contains("pub struct ItemsWeaponFragment"));
    assert!(rust_content.contains("pub weapon: Option<ItemsWeaponFragment>"));
    assert!(!rust_content.contains("pub struct ItemsAttributesFragment"));
    assert!(rust_content.contains("pub attributes: Option<AttributeSet>"));
    assert!(rust_content.contains("pub fn has_weapon(&self) -> bool"));

    let client_rust = RustCodeGenerator
        .generate(&CodeExportInput {
            target_tags: TargetTags::new(["client"]),
            ..input_for(&meta, CodeExportLanguage::Rust)
        })
        .expect("client rust code");
    let client_rust_content = &client_rust[0].content;
    assert!(client_rust_content.contains("pub struct ItemsClientMarkerFragment"));
    assert!(client_rust_content.contains("pub client_marker: Option<ItemsClientMarkerFragment>"));
    assert!(client_rust_content.contains("pub fn has_client_marker(&self) -> bool"));
    assert!(!client_rust_content.contains("server_marker"));

    let typescript = TypeScriptCodeGenerator
        .generate(&input_for(&meta, CodeExportLanguage::TypeScript))
        .expect("typescript code");
    let ts_content = &typescript[0].content;
    assert!(ts_content.contains("export interface ItemsWeaponFragment"));
    assert!(ts_content.contains("weapon?: ItemsWeaponFragment;"));
    assert!(!ts_content.contains("export interface ItemsAttributesFragment"));
    assert!(ts_content.contains("attributes?: AttributeSet;"));

    let client_typescript = TypeScriptCodeGenerator
        .generate(&CodeExportInput {
            target_tags: TargetTags::new(["client"]),
            ..input_for(&meta, CodeExportLanguage::TypeScript)
        })
        .expect("client typescript code");
    let client_ts_content = &client_typescript[0].content;
    assert!(client_ts_content.contains("export interface ItemsClientMarkerFragment"));
    assert!(client_ts_content.contains("clientMarker?: ItemsClientMarkerFragment;"));
    assert!(!client_ts_content.contains("serverMarker"));

    let cpp = CppCodeGenerator
        .generate(&input_for(&meta, CodeExportLanguage::Cpp))
        .expect("cpp code");
    let cpp_content = &cpp[0].content;
    assert!(cpp_content.contains("struct ItemsWeaponFragment"));
    assert!(cpp_content.contains("std::optional<ItemsWeaponFragment> weapon"));
    assert!(!cpp_content.contains("struct ItemsAttributesFragment"));
    assert!(cpp_content.contains("std::optional<AttributeSet> attributes"));

    let client_cpp = CppCodeGenerator
        .generate(&CodeExportInput {
            target_tags: TargetTags::new(["client"]),
            ..input_for(&meta, CodeExportLanguage::Cpp)
        })
        .expect("client cpp code");
    let client_cpp_content = &client_cpp[0].content;
    assert!(client_cpp_content.contains("struct ItemsClientMarkerFragment"));
    assert!(client_cpp_content.contains("std::optional<ItemsClientMarkerFragment> client_marker"));
    assert!(!client_cpp_content.contains("server_marker"));
}

#[test]
fn code_model_defaults_data_file_to_json_when_no_runtime_file_is_available() {
    let meta = linked_fixture();
    let linked = meta.link().expect("fixture meta should link");

    let model = build_code_model(&linked, &TargetTags::empty(), &[]).expect("model");

    assert_eq!(model.tables[0].data_file, "items.json");
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
    assert!(
        error
            .to_string()
            .contains("target index `items.by_id` which is not exported")
    );
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
    assert!(
        error
            .to_string()
            .contains("non-exported source field `drop_item_id`")
    );
}

#[test]
fn rust_generator_emits_loader_table_and_index_helpers() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, CodeExportLanguage::Rust);
    input.target.options.language =
        LanguageCodegenOptions::Rust(RustCodegenOptions { derive_serde: true });

    let files = RustCodeGenerator.generate(&input).expect("rust code");
    let content = &files[0].content;

    assert!(content.contains("pub trait RuntimeDataLoader"));
    assert!(content.contains("pub trait RuntimeDataReader"));
    assert!(content.contains("pub const CONFIG_FORGE_READER_CONTRACT_VERSION: &str = \"1.0\";"));
    assert!(content.contains("fn open(&self, path: &str) -> Result<Self::Reader, String>;"));
    assert!(content.contains("pub const DATA_FILE: &'static str = \"items.json\";"));
    assert!(!content.contains("RuntimeDataFormat"));
    assert!(!content.contains("RuntimeDataDecoder"));
    assert!(!content.contains("load_bytes"));
    assert!(content.contains("serde::Serialize, serde::Deserialize"));
    assert!(content.contains("#[serde(rename = \"common\")]"));
    assert!(content.contains("#[serde(rename = \"display_name\")]"));
    assert!(content.contains("pub enum ItemRarity"));
    assert!(content.contains("pub struct RewardInfo"));
    assert!(content.contains("pub struct ItemsRow"));
    assert!(content.contains("fn read_from_reader<R: RuntimeDataReader>(reader: &R)"));
    assert!(content.contains("pub fn get_by_id"));
    assert!(content.contains("pub fn find_by_rarity"));
    assert!(content.contains("pub struct GameData"));
}

#[test]
fn rust_generator_sanitizes_gen_keyword_across_api_surface() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[enums]]
            name = "gen_enum"

            [[enums.values]]
            name = "gen"

            [[structs]]
            name = "gen"

            [[structs.fields]]
            name = "gen"
            type = "int"

            [[tables]]
            name = "gen"
            source = "gen.csv"
            primary_index = "gen"

            [[tables.fields]]
            name = "gen"
            type = "int"

            [[tables.fields]]
            name = "kind"
            type = "gen_enum"

            [[tables.fields]]
            name = "payload"
            type = "gen"

            [[tables.indexes]]
            name = "gen"
            fields = ["gen"]
            unique = true
            primary = true
        "#,
    )
    .expect("meta should load");
    let mut input = input_for(&meta, CodeExportLanguage::Rust);
    input.target.options.naming.enum_value = CaseStyle::Preserve;
    input.runtime_data_files = vec![RuntimeDataFile {
        table_name: "gen".to_owned(),
        path: "gen.json".to_owned(),
    }];

    let files = RustCodeGenerator.generate(&input).expect("rust code");
    let content = &files[0].content;

    assert!(content.contains("pub enum GenEnum"));
    assert!(content.contains("    gen_,"));
    assert!(content.contains("\"gen\" => Ok(Self::gen_)"));
    assert!(content.contains("pub struct Gen"));
    assert!(content.contains("pub gen_: i64,"));
    assert!(content.contains("pub struct GenRow"));
    assert!(content.contains("pub struct GenTable"));
    assert!(content.contains("gen_: HashMap<i64, usize>"));
    assert!(content.contains("pub fn get_by_gen(&self, gen_: i64) -> Option<&GenRow>"));
    assert!(content.contains("pub gen_: GenTable,"));
    assert!(content.contains("gen_: GenTable::load(loader)?"));
}

#[test]
fn rust_generator_emits_object_table_as_typed_object_not_repository() {
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
    let input = input_for(&meta, CodeExportLanguage::Rust);

    let files = RustCodeGenerator.generate(&input).expect("rust code");
    let content = &files[0].content;

    assert!(content.contains("fn open(&self, path: &str) -> Result<Self::Reader, String>;"));
    assert!(content.contains("pub game_config: GameConfigRow,"));
    assert!(content.contains("impl GameConfigRow"));
    assert!(content.contains("let root = loader.open(Self::DATA_FILE)?;"));
    assert!(content.contains("Self::read_from_reader(&root)"));
    assert!(!content.contains("pub struct GameConfigTable"));
    assert!(!content.contains("from_rows"));
    assert!(!content.contains("pub fn rows(&self)"));
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

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("primary index `by_id_region` references non-exported field `internal_region`"))
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

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("unsupported index key type"))
    );
}

#[test]
fn generators_reject_duplicate_generated_field_names() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "foo_bar"
            type = "int"

            [[tables.fields]]
            name = "foo-bar"
            type = "int"
        "#,
    )
    .expect("meta should load");

    assert_duplicate_generated_identifier(
        RustCodeGenerator.generate(&input_for(&meta, CodeExportLanguage::Rust)),
        "rust codegen",
        "duplicate generated identifier `foo_bar`",
        "table `items` row fields",
    );
    assert_duplicate_generated_identifier(
        CppCodeGenerator.generate(&input_for(&meta, CodeExportLanguage::Cpp)),
        "cpp codegen",
        "duplicate generated identifier `foo_bar`",
        "table `items` row fields",
    );
    assert_duplicate_generated_identifier(
        TypeScriptCodeGenerator.generate(&input_for(&meta, CodeExportLanguage::TypeScript)),
        "typescript codegen",
        "duplicate generated identifier `fooBar`",
        "table `items` row fields",
    );
}

#[test]
fn generators_reject_duplicate_top_level_generated_types() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "items_row"

            [[structs.fields]]
            name = "value"
            type = "int"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("meta should load");

    for language in [
        CodeExportLanguage::Rust,
        CodeExportLanguage::Cpp,
        CodeExportLanguage::TypeScript,
    ] {
        let result = match language {
            CodeExportLanguage::Rust => RustCodeGenerator.generate(&input_for(&meta, language)),
            CodeExportLanguage::Cpp => CppCodeGenerator.generate(&input_for(&meta, language)),
            CodeExportLanguage::TypeScript => {
                TypeScriptCodeGenerator.generate(&input_for(&meta, language))
            }
        };
        assert_duplicate_generated_identifier(
            result,
            language.as_str(),
            "duplicate generated identifier `ItemsRow`",
            if language == CodeExportLanguage::TypeScript {
                "top-level exports"
            } else {
                "top-level types"
            },
        );
    }
}

#[test]
fn rust_generator_rejects_runtime_reader_trait_name_conflicts() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "runtime_data_reader"

            [[structs.fields]]
            name = "value"
            type = "int"
        "#,
    )
    .expect("meta should load");

    assert_duplicate_generated_identifier(
        RustCodeGenerator.generate(&input_for(&meta, CodeExportLanguage::Rust)),
        "rust codegen",
        "duplicate generated identifier `RuntimeDataReader`",
        "top-level types",
    );
}

fn assert_duplicate_generated_identifier(
    result: crate::Result<Vec<GeneratedFile>>,
    language: &str,
    identifier: &str,
    scope: &str,
) {
    let error = result.expect_err("generator should reject duplicate generated symbols");
    assert!(
        matches!(&error, ConfigForgeError::Validation(message) if message.contains(language) && message.contains(identifier) && message.contains(scope)),
        "unexpected error: {error}"
    );
}

#[test]
fn typescript_generator_emits_async_loader_and_maps() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, CodeExportLanguage::TypeScript);
    input.target.options.language = LanguageCodegenOptions::TypeScript(TypeScriptCodegenOptions {
        readonly_fields: true,
    });

    let files = TypeScriptCodeGenerator
        .generate(&input)
        .expect("typescript code");
    let content = &files[0].content;

    assert!(content.contains("export interface RuntimeDataLoader"));
    assert!(content.contains("export interface RuntimeDataReader"));
    assert!(
        content.contains("export const CONFIG_FORGE_READER_CONTRACT_VERSION = \"1.0\" as const;")
    );
    assert!(
        content.contains("open(path: string): Promise<RuntimeDataReader> | RuntimeDataReader;")
    );
    assert!(content.contains("static readonly dataFile = \"items.json\";"));
    assert!(!content.contains("RuntimeDataFormat"));
    assert!(!content.contains("RuntimeDataDecoder"));
    assert!(!content.contains("loadBytes"));
    assert!(content.contains("export type ItemRarity"));
    assert!(content.contains("export interface ItemsRow"));
    assert!(content.contains("function readItemsRow(reader: RuntimeDataReader): ItemsRow"));
    assert!(content.contains("readonly displayName: string;"));
    assert!(content.contains("static async load"));
    assert!(content.contains("getById(id: number): ItemsRow | undefined"));
    assert!(content.contains("findByRarity(rarity: ItemRarity): readonly ItemsRow[]"));
    assert!(content.contains("Duplicate unique index key for ItemsTable.by_id"));
}

#[test]
fn typescript_generator_reads_optional_fragment_once() {
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

            [[tables.fragments]]
            name = "stack"

            [[tables.fragments.fields]]
            name = "max_stack"
            type = "int"
        "#,
    )
    .expect("meta should load");

    let files = TypeScriptCodeGenerator
        .generate(&input_for(&meta, CodeExportLanguage::TypeScript))
        .expect("typescript code");
    let content = &files[0].content;

    assert!(content.contains("const stackReader = optionalField(reader, \"stack\");"));
    assert!(
        content.contains("stack: stackReader ? readItemsStackFragment(stackReader) : undefined,")
    );
    assert!(!content.contains("optionalField(reader, \"stack\") ?"));
}

#[test]
fn typescript_generator_emits_object_table_as_typed_object_not_repository() {
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
    let input = input_for(&meta, CodeExportLanguage::TypeScript);

    let files = TypeScriptCodeGenerator
        .generate(&input)
        .expect("typescript code");
    let content = &files[0].content;

    assert!(
        content.contains("open(path: string): Promise<RuntimeDataReader> | RuntimeDataReader;")
    );
    assert!(content.contains("readonly gameConfig: GameConfigRow;"));
    assert!(
        content.contains("gameConfig: readGameConfigRow(await loader.open(\"game_config.json\"))")
    );
    assert!(!content.contains("export class GameConfigTable"));
    assert!(!content.contains("rows(): readonly GameConfigRow[]"));
}

#[test]
fn cpp_generator_emits_loader_and_repository_declarations() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, CodeExportLanguage::Cpp);
    input.target.options.language = LanguageCodegenOptions::Cpp(CppCodegenOptions {
        namespace: Some("game::config".to_owned()),
    });

    let files = CppCodeGenerator.generate(&input).expect("cpp code");
    assert_eq!(files.len(), 2);
    let content = generated_content(&files, "generated.hpp");
    let source = generated_content(&files, "generated.cpp");

    assert!(content.contains("class IRuntimeDataLoader"));
    assert!(content.contains("class IRuntimeDataReader"));
    assert!(content.contains(
        "virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;"
    ));
    assert!(
        content.contains(
            "inline constexpr const char* CONFIG_FORGE_READER_CONTRACT_VERSION = \"1.0\";"
        )
    );
    assert!(content.contains("static constexpr const char* DataFile = \"items.json\";"));
    assert!(!content.contains("RuntimeDataFormat"));
    assert!(!content.contains("IRuntimeDataDecoder"));
    assert!(!content.contains("IDataFileLoader"));
    assert!(content.contains("namespace game::config"));
    assert!(content.contains("} // namespace game::config"));
    assert!(content.contains("enum class ItemRarity"));
    assert!(content.contains("struct ItemsRow"));
    assert!(content.contains("class ItemsTable"));
    assert!(content.contains("const ItemsRow* GetById"));
    assert!(content.contains("std::vector<const ItemsRow*> FindByRarity"));
    assert!(source.contains("#include \"generated.hpp\""));
    assert!(source.contains("ItemsTable ItemsTable::Load(IRuntimeDataLoader& loader)"));
    assert!(source.contains("auto root = loader.Open(DataFile);"));
    assert!(source.contains("rows.push_back(ReadItemsRow(*rows_reader->Index(row_index)));"));
    assert!(source.contains("ItemsTable ItemsTable::FromRows(std::vector<ItemsRow> rows)"));
    assert!(source.contains("table.by_id_.emplace(row.id, row_index);"));
    assert!(source.contains("table.by_rarity_[row.rarity].push_back(row_index);"));
    assert!(source.contains("const ItemsRow* ItemsTable::GetById(std::int64_t id) const"));
    assert!(source.contains(
        "std::vector<const ItemsRow*> ItemsTable::FindByRarity(ItemRarity rarity) const"
    ));
    assert!(source.contains("GameData GameData::Load(IRuntimeDataLoader& loader)"));
    assert!(source.contains("data.items_ = ItemsTable::Load(loader);"));
}

#[test]
fn cpp_generator_emits_object_table_as_typed_object_not_repository() {
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
    let input = input_for(&meta, CodeExportLanguage::Cpp);

    let files = CppCodeGenerator.generate(&input).expect("cpp code");
    let content = generated_content(&files, "generated.hpp");
    let source = generated_content(&files, "generated.cpp");

    assert!(content.contains(
        "virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;"
    ));
    assert!(content.contains("static constexpr const char* DataFile = \"game_config.json\";"));
    assert!(content.contains("const GameConfigRow& GameConfig() const"));
    assert!(content.contains("GameConfigRow game_config_;"));
    assert!(!content.contains("class GameConfigTable"));
    assert!(!content.contains("Rows() const"));
    assert!(
        source.contains(
            "data.game_config_ = ReadGameConfigRow(*loader.Open(GameConfigRow::DataFile));"
        )
    );
}

fn generated_content<'a>(files: &'a [GeneratedFile], file_name: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.path.file_name().and_then(|name| name.to_str()) == Some(file_name))
        .unwrap_or_else(|| panic!("generated file `{file_name}` should exist"))
        .content
        .as_str()
}

#[test]
fn naming_handles_reserved_words_and_cases() {
    assert_eq!(
        crate::codegen::naming::pascal_case("item-table"),
        "ItemTable"
    );
    assert_eq!(crate::codegen::naming::snake_case("type"), "type");
    assert_eq!(
        crate::codegen::naming::make_identifier(
            "type",
            CaseStyle::SnakeCase,
            KeywordConflictPolicy::SuffixUnderscore,
            crate::codegen::naming::is_rust_keyword,
            |identifier| Some(format!("r#{identifier}")),
        )
        .expect("identifier"),
        "type_"
    );
    assert!(crate::codegen::naming::is_rust_keyword("gen"));
    assert_eq!(
        crate::codegen::naming::make_identifier(
            "gen",
            CaseStyle::SnakeCase,
            KeywordConflictPolicy::SuffixUnderscore,
            crate::codegen::naming::is_rust_keyword,
            |identifier| Some(format!("r#{identifier}")),
        )
        .expect("identifier"),
        "gen_"
    );
    assert_eq!(crate::codegen::naming::camel_case("item-name"), "itemName");
}

#[test]
fn parses_codegen_task_config_and_merges_naming_defaults() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "typescript"
            output = "Generated/TS"
            file_header = "// {{project.name}} {{datetime}}"
            keyword_conflict = "error"

            [naming]
            field = "snake_case"

            [typescript]
            readonly_fields = true
        "#,
    )
    .expect("config should parse");

    let target = CodeExportTarget::from_task(config);

    assert_eq!(target.language, CodeExportLanguage::TypeScript);
    assert_eq!(target.out_dir, PathBuf::from("Generated/TS"));
    assert_eq!(
        target.options.keyword_conflict,
        KeywordConflictPolicy::Error
    );
    assert_eq!(target.options.naming.type_name, CaseStyle::PascalCase);
    assert_eq!(target.options.naming.field, CaseStyle::SnakeCase);
    assert_eq!(target.options.naming.function, CaseStyle::CamelCase);
    assert_eq!(
        target.options.language,
        LanguageCodegenOptions::TypeScript(TypeScriptCodegenOptions {
            readonly_fields: true,
        })
    );
}

#[test]
fn render_file_header_replaces_project_variables_and_datetime() {
    let meta = MetaModel::from_toml_str("").expect("meta should parse");
    let mut input = input_for(&meta, CodeExportLanguage::Rust);
    input.target.options.file_header = Some(
        "// {{project.name}} | {{project.desc}} | {{project.copyright}} | {{datetime}}".to_owned(),
    );

    assert_eq!(
        render_file_header(&input),
        "// demo | Demo config | Copyright Demo | 2026-05-17T00:00:00Z\n"
    );
}
