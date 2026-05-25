// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    codegen::{
        CaseStyle, CodeExportInput, CodeGenerator,
        tests::{assert_duplicate_generated_identifier, input_for, linked_fixture},
    },
    exporter::RuntimeDataFile,
    meta::{MetaModel, TargetTags},
};

use super::{LANGUAGE, RustCodeGenerator, RustCodegenOptions};

#[test]
fn emits_loader_table_and_index_helpers() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::Value::try_from(RustCodegenOptions { derive_serde: true }).expect("rust options"),
    );

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
fn emits_fragment_structs_and_optional_row_members() {
    let meta = fragment_fixture();

    let rust = RustCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
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
            ..input_for(&meta, LANGUAGE)
        })
        .expect("client rust code");
    let client_rust_content = &client_rust[0].content;
    assert!(client_rust_content.contains("pub struct ItemsClientMarkerFragment"));
    assert!(client_rust_content.contains("pub client_marker: Option<ItemsClientMarkerFragment>"));
    assert!(client_rust_content.contains("pub fn has_client_marker(&self) -> bool"));
    assert!(!client_rust_content.contains("server_marker"));
}

#[test]
fn sanitizes_gen_keyword_across_api_surface() {
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
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.naming.enum_value = CaseStyle::Preserve;
    input.runtime_data_files = vec![RuntimeDataFile {
        table_name: "gen".to_owned(),
        path: "gen.json".to_owned(),
        format: "json".to_owned(),
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
fn emits_object_table_as_typed_object_not_repository() {
    let meta = object_table_fixture();
    let input = input_for(&meta, LANGUAGE);

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
fn rejects_duplicate_generated_field_names() {
    let meta = duplicate_field_fixture();

    assert_duplicate_generated_identifier(
        RustCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        "rust codegen",
        "duplicate generated identifier `foo_bar`",
        "table `items` row fields",
    );
}

#[test]
fn rejects_duplicate_top_level_generated_types() {
    let meta = duplicate_top_level_fixture();

    assert_duplicate_generated_identifier(
        RustCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        LANGUAGE,
        "duplicate generated identifier `ItemsRow`",
        "top-level types",
    );
}

#[test]
fn rejects_runtime_reader_trait_name_conflicts() {
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
        RustCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        "rust codegen",
        "duplicate generated identifier `RuntimeDataReader`",
        "top-level types",
    );
}

fn fragment_fixture() -> MetaModel {
    MetaModel::from_toml_str(
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

            [[tables.fragments]]
            name = "attributes"
            type = "AttributeSet"

            [[tables.fragments]]
            name = "client_marker"
            export_when = { any = ["client"] }

            [[tables.fragments]]
            name = "server_marker"
            export_when = { any = ["server"] }

            [[tables.fragments.fields]]
            name = "value"
            type = "int"
        "#,
    )
    .expect("meta should load")
}

fn object_table_fixture() -> MetaModel {
    MetaModel::from_toml_str(
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
    .expect("meta should load")
}

fn duplicate_field_fixture() -> MetaModel {
    MetaModel::from_toml_str(
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
    .expect("meta should load")
}

fn duplicate_top_level_fixture() -> MetaModel {
    MetaModel::from_toml_str(
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
    .expect("meta should load")
}
