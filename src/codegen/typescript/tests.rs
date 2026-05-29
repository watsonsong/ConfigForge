// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    codegen::{
        CodeExportInput, CodeGenerator,
        tests::{assert_duplicate_generated_identifier, input_for, linked_fixture},
    },
    exporter::RuntimeDataFile,
    meta::{MetaModel, TargetTags},
};

use super::{LANGUAGE, TypeScriptCodeGenerator, TypeScriptCodegenOptions};

#[test]
fn emits_async_loader_and_maps() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::Value::try_from(TypeScriptCodegenOptions {
            readonly_fields: true,
        })
        .expect("typescript options"),
    );

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
fn emits_fragment_structs_and_optional_row_members() {
    let meta = fragment_fixture();

    let typescript = TypeScriptCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .expect("typescript code");
    let ts_content = &typescript[0].content;
    assert!(ts_content.contains("export interface ItemsWeaponFragment"));
    assert!(ts_content.contains("weapon?: ItemsWeaponFragment;"));
    assert!(!ts_content.contains("export interface ItemsAttributesFragment"));
    assert!(ts_content.contains("attributes?: AttributeSet;"));

    let client_typescript = TypeScriptCodeGenerator
        .generate(&CodeExportInput {
            target_tags: TargetTags::new(["client"]),
            ..input_for(&meta, LANGUAGE)
        })
        .expect("client typescript code");
    let client_ts_content = &client_typescript[0].content;
    assert!(client_ts_content.contains("export interface ItemsClientMarkerFragment"));
    assert!(client_ts_content.contains("clientMarker?: ItemsClientMarkerFragment;"));
    assert!(!client_ts_content.contains("serverMarker"));
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

    let typescript = TypeScriptCodeGenerator
        .generate(&input)
        .expect("typescript code");
    let content = &typescript[0].content;

    assert!(content.contains("bonus?: number;"));
    assert!(content.contains("note?: string;"));
    assert!(content.contains("payload?: RewardInfo;"));
    assert!(content.contains("bonus: readOptionalInt64(reader, \"bonus\"),"));
    assert!(
        content.contains("optionalField(reader, \"payload\", (value) => readRewardInfo(value))")
    );
    assert!(!content.contains("readRewardInfoBinary"));
}

#[test]
fn reads_optional_fragment_once() {
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
        .generate(&input_for(&meta, LANGUAGE))
        .expect("typescript code");
    let content = &files[0].content;

    assert!(content.contains(
        "stack: optionalField(reader, \"stack\", (value) => readItemsStackFragment(value)),"
    ));
    assert!(!content.contains("stackReader"));
}

#[test]
fn emits_object_table_as_typed_object_not_repository() {
    let meta = object_table_fixture();
    let input = input_for(&meta, LANGUAGE);

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
fn rejects_duplicate_generated_field_names() {
    let meta = duplicate_field_fixture();

    assert_duplicate_generated_identifier(
        TypeScriptCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        "typescript codegen",
        "duplicate generated identifier `fooBar`",
        "table `items` row fields",
    );
}

#[test]
fn rejects_duplicate_top_level_generated_types() {
    let meta = duplicate_top_level_fixture();

    assert_duplicate_generated_identifier(
        TypeScriptCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        LANGUAGE,
        "duplicate generated identifier `ItemsRow`",
        "top-level exports",
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
