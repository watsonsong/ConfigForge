// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use tempfile::tempdir;

use crate::{
    artifact::{ArtifactWriteMode, ArtifactWriter},
    code_artifact::{
        CaseStyle, CodeExportInput, CodeGenerator, RuntimeInterfaceMode,
        tests::{
            assert_duplicate_generated_identifier, indexed_source_order_fixture, input_for,
            linked_fixture, source_order_fixture,
        },
    },
    meta::{MetaModel, TargetTags},
    runtime_contract::RuntimeDataFile,
};

use super::{LANGUAGE, RustCodeGenerator, RustCodegenOptions};

#[test]
fn emits_loader_table_and_index_helpers() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::Value::try_from(RustCodegenOptions {
            derive_serde: true,
            ..RustCodegenOptions::default()
        })
        .expect("rust options"),
    );

    let files = RustCodeGenerator.generate(&input).expect("rust code");
    let content = &files[0].content;

    assert!(!content.contains("pub use config_forge_runtime_reader_core"));
    assert!(content.contains("pub trait RuntimeDataLoader"));
    assert!(content.contains("pub trait RuntimeDataReader"));
    assert!(content.contains("pub struct RuntimeDataError"));
    assert!(content.contains("pub type RuntimeDataResult<T> = Result<T, RuntimeDataError>;"));
    assert!(content.contains("pub const CONFIG_FORGE_READER_CONTRACT_VERSION: &str = \"1.0\";"));
    assert!(content.contains("pub const DATA_FILE: &'static str = \"items\";"));
    assert!(!content.contains("RuntimeDataFormat"));
    assert!(!content.contains("RuntimeDataDecoder"));
    assert!(!content.contains("load_bytes"));
    assert!(content.contains("serde::Serialize, serde::Deserialize"));
    assert!(content.contains("#[serde(rename = \"common\")]"));
    assert!(content.contains("#[serde(rename = \"display_name\")]"));
    assert!(content.contains("pub enum ItemRarity"));
    assert!(content.contains("pub struct RewardInfo"));
    assert!(content.contains("pub struct ItemsRow"));
    assert!(content.contains(
        "fn read_from_reader<R: RuntimeDataReader>(reader: &R) -> RuntimeDataResult<Self>"
    ));
    assert!(content.contains("reader.read_list_len()?"));
    assert!(!content.contains("reader.read_array_len()?"));
    assert!(content.contains("pub fn get_by_id"));
    assert!(content.contains("pub fn find_by_rarity"));
    assert!(content.contains("pub struct GameData"));
    assert!(content.contains("pub trait GameDataAccess"));
    assert!(content.contains("impl GameDataAccess for GameData"));
    assert!(content.contains("pub struct LazyGameData<L: RuntimeDataLoader>"));
    assert!(content.contains("items: OnceCell<ItemsTable>"));
    assert!(content.contains("pub fn new(loader: L) -> Self"));
    assert!(content.contains("pub fn preload_all(&self) -> RuntimeDataResult<()>"));
    assert!(content.contains("impl<L: RuntimeDataLoader> GameDataAccess for LazyGameData<L>"));
    assert!(content.contains("let value = ItemsTable::load(&self.loader)?;"));
}

#[test]
fn parses_and_validates_rust_runtime_interface_options() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str(
            r#"
                runtime_interface = "external"
                runtime_core_path = "crate::runtime"
            "#,
        )
        .expect("toml value"),
    );

    let options = input
        .target
        .options
        .language_options::<RustCodegenOptions>(LANGUAGE)
        .expect("rust options");
    assert_eq!(options.runtime_interface, RuntimeInterfaceMode::External);
    assert_eq!(options.runtime_core_path.as_deref(), Some("crate::runtime"));

    RustCodeGenerator
        .generate(&input)
        .expect("external runtime options should generate");
    let external_content = &RustCodeGenerator
        .generate(&input)
        .expect("external runtime options should generate")[0]
        .content;
    assert!(external_content.contains("pub use crate::runtime::{"));
    assert!(!external_content.contains("pub trait RuntimeDataReader"));

    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str(
            r#"
                runtime_interface = "external"
                runtime_core_path = ""
            "#,
        )
        .expect("toml value"),
    );
    let error = RustCodeGenerator.generate(&input).unwrap_err();
    assert!(
        matches!(error, crate::ConfigForgeError::Validation(ref message) if message.contains("runtime_core_path")),
        "unexpected error: {error}"
    );
}

#[test]
fn generate_to_sink_matches_legacy_generate_output() {
    let meta = linked_fixture();
    let temp_dir = tempdir().expect("temp dir");
    let mut input = input_for(&meta, LANGUAGE);
    input.target.out_dir = temp_dir.path().join("rust_out");

    let legacy = RustCodeGenerator
        .generate(&input)
        .expect("legacy rust code");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    RustCodeGenerator
        .generate_to_sink(&input, &sink)
        .expect("streamed rust code");
    let result = sink.finish().expect("sink should finish");

    assert_eq!(legacy.len(), 1);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].path, legacy[0].path);
    assert_eq!(result.entries[0].bytes, legacy[0].content.len());
    assert_eq!(
        fs::read_to_string(&legacy[0].path).expect("streamed rust file"),
        legacy[0].content
    );
}

#[test]
fn source_order_apis_are_opt_in() {
    let meta = source_order_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = RustCodeGenerator.generate(&input).expect("rust code");
    let content = &files[0].content;

    assert!(content.contains("pub struct ItemsTable"));
    assert!(!content.contains("pub fn row_at(&self, row_index: usize) -> Option<&ItemsRow>"));
    assert!(!content.contains("pub fn rows(&self) -> &[ItemsRow]"));
    assert!(content.contains("pub struct CharacterSurnamesTable"));
    assert!(content.contains("pub fn len(&self) -> usize { self.rows.len() }"));
    assert!(content.contains("pub fn rows(&self) -> &[CharacterSurnamesRow]"));
    assert!(
        content.contains("pub fn row_at(&self, row_index: usize) -> Option<&CharacterSurnamesRow>")
    );
}

#[test]
fn indexed_source_order_table_emits_indexes_and_ordinal_apis() {
    let meta = indexed_source_order_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = RustCodeGenerator.generate(&input).expect("rust code");
    let content = &files[0].content;

    assert!(content.contains("pub fn len(&self) -> usize { self.rows.len() }"));
    assert!(content.contains("pub fn is_empty(&self) -> bool { self.rows.is_empty() }"));
    assert!(content.contains("pub fn rows(&self) -> &[ItemsRow]"));
    assert!(content.contains("pub fn row_at(&self, row_index: usize) -> Option<&ItemsRow>"));
    assert!(content.contains("pub fn get_by_id(&self, id: i64) -> Option<&ItemsRow>"));
    assert!(content.contains("pub fn find_by_rarity(&self, rarity: String) -> Vec<&ItemsRow>"));
}

#[test]
fn rejects_source_order_index_collisions() {
    let meta = source_order_index_conflict_fixture("rows");

    assert_duplicate_generated_identifier(
        RustCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        LANGUAGE,
        "duplicate generated identifier `rows`",
        "table `items` repository fields",
    );
}

#[test]
fn rejects_source_order_relation_and_helper_collisions() {
    let meta = source_order_relation_conflict_fixture("rows_link", "rows");

    assert_duplicate_generated_identifier(
        RustCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        LANGUAGE,
        "duplicate generated identifier `rows_link`",
        "table `items` row fields",
    );
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
fn emits_optional_scalar_and_struct_field_types() {
    let meta = optional_field_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.runtime_data_files = vec![RuntimeDataFile {
        table_name: "items".to_owned(),
        path: "items.bin".to_owned(),
        format: "bin".to_owned(),
    }];

    let rust = RustCodeGenerator.generate(&input).expect("rust code");
    let content = &rust[0].content;

    assert!(content.contains("pub bonus: Option<i64>,"));
    assert!(content.contains("pub note: Option<String>,"));
    assert!(content.contains("pub payload: Option<RewardInfo>,"));
    assert!(content.contains("bonus: read_optional_i64(reader, \"bonus\")?"));
    assert!(content.contains(
        "payload: with_optional_field(reader, \"payload\", RewardInfo::read_from_reader)?"
    ));
    assert!(!content.contains("read_from_binary_reader"));
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
            required = true

            [[tables]]
            name = "gen"
            source = "gen.csv"
            primary_index = "gen"

            [[tables.fields]]
            name = "gen"
            type = "int"
            required = true

            [[tables.fields]]
            name = "kind"
            type = "gen_enum"
            required = true

            [[tables.fields]]
            name = "payload"
            type = "gen"
            required = true

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
    assert!(content.contains("match reader.read_enum_index(&["));
    assert!(content.contains("            \"gen\","));
    assert!(content.contains("            0 => Ok(Self::gen_),"));
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

    assert!(content.contains("pub trait RuntimeDataReader"));
    assert!(content.contains("pub game_config: GameConfigRow,"));
    assert!(content.contains("game_config: OnceCell<GameConfigRow>"));
    assert!(content.contains("let value = GameConfigRow::load(&self.loader)?;"));
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

#[test]
fn rejects_lazy_game_data_top_level_name_conflicts() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "LazyGameData"

            [[structs.fields]]
            name = "value"
            type = "int"
        "#,
    )
    .expect("meta should load");

    assert_duplicate_generated_identifier(
        RustCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        "rust codegen",
        "duplicate generated identifier `LazyGameData`",
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

fn source_order_index_conflict_fixture(index_name: &str) -> MetaModel {
    MetaModel::from_toml_str(&format!(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            preserve_source_order = true

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "{index_name}"
            fields = ["id"]
            unique = true
        "#
    ))
    .expect("meta should load")
}

fn source_order_relation_conflict_fixture(field_name: &str, linked_field_name: &str) -> MetaModel {
    MetaModel::from_toml_str(&format!(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            preserve_source_order = true
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "{field_name}"
            type = "int"
            foreign_key = {{ name = "item", target_table = "items", linked_field = "{linked_field_name}" }}

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#
    ))
    .expect("meta should load")
}

fn optional_field_fixture() -> MetaModel {
    MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "RewardInfo"

            [[structs.fields]]
            name = "amount"
            type = "int"

            [[structs.fields]]
            name = "note"
            type = "string"
            required = false

            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "bonus"
            type = "int"
            required = false

            [[tables.fields]]
            name = "note"
            type = "string"
            required = false

            [[tables.fields]]
            name = "payload"
            type = "RewardInfo"
            required = false

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
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
