// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    codegen::{
        CodeExportInput, CodeGenerator,
        tests::{
            assert_duplicate_generated_identifier, input_for, linked_fixture, source_order_fixture,
        },
    },
    meta::{MetaModel, TargetTags},
    runtime_contract::RuntimeDataFile,
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
    assert!(content.contains("static readonly dataFile = \"items\";"));
    assert!(!content.contains("RuntimeDataFormat"));
    assert!(!content.contains("RuntimeDataDecoder"));
    assert!(!content.contains("loadBytes"));
    assert!(content.contains("export type ItemRarity"));
    assert!(content.contains("export interface ItemsRow"));
    assert!(content.contains("function readItemsRow(reader: RuntimeDataReader): ItemsRow"));
    assert!(content.contains("readListLength?(): number;"));
    assert!(content.contains("runtime reader does not support row count"));
    assert!(content.contains("runtime reader does not support list length"));
    assert!(content.contains(
        "function openRuntimeDataSync(loader: RuntimeDataLoader, path: string): RuntimeDataReader"
    ));
    assert!(content.contains("runtime loader returned a Promise while synchronously opening"));
    assert!(content.contains("return reader.readRowCount();"));
    assert!(content.contains("return reader.readListLength();"));
    assert!(!content.contains("reader.length();"));
    assert!(content.contains("const length = readListLength(reader);"));
    assert!(content.contains("readonly displayName: string;"));
    assert!(content.contains("static async load"));
    assert!(content.contains("static loadSync(loader: RuntimeDataLoader): GameData"));
    assert!(content.contains("static loadLazy(loader: RuntimeDataLoader): LazyGameData"));
    assert!(content.contains("export class LazyGameData"));
    assert!(content.contains("private itemsPromise?: Promise<ItemsTable>;"));
    assert!(content.contains("private itemsValue?: ItemsTable;"));
    assert!(content.contains("constructor(loader: RuntimeDataLoader)"));
    assert!(content.contains("static loadLazy(loader: RuntimeDataLoader): LazyGameData { return new LazyGameData(loader); }"));
    assert!(content.contains("items(): Promise<ItemsTable>"));
    assert!(content.contains("itemsSync(): ItemsTable"));
    assert!(content.contains("this.itemsPromise = ItemsTable.load(this.loader).then((value) =>"));
    assert!(content.contains("const value = ItemsTable.loadSync(this.loader);"));
    assert!(content.contains("async preloadAll(): Promise<void>"));
    assert!(content.contains("preloadAllSync(): void"));
    assert!(content.contains("getById(id: number): ItemsRow | undefined"));
    assert!(content.contains("findByRarity(rarity: ItemRarity): readonly ItemsRow[]"));
    assert!(content.contains("Duplicate unique index key for ItemsTable.by_id"));
}

#[test]
fn source_order_apis_are_opt_in() {
    let meta = source_order_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = TypeScriptCodeGenerator
        .generate(&input)
        .expect("typescript code");
    let content = &files[0].content;

    assert!(content.contains("export class ItemsTable"));
    assert!(!content.contains("rowAt(rowIndex: number): ItemsRow | undefined"));
    assert!(!content.contains("rows(): readonly ItemsRow[]"));
    assert!(content.contains("export class CharacterSurnamesTable"));
    assert!(content.contains("get length(): number { return this.rows_.length; }"));
    assert!(content.contains("rows(): readonly CharacterSurnamesRow[]"));
    assert!(content.contains("rowAt(rowIndex: number): CharacterSurnamesRow | undefined"));
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
    assert!(content.contains("gameConfig: (() => {"));
    assert!(content.contains("const root = openRuntimeDataSync(loader, \"game_config\");"));
    assert!(content.contains("private gameConfigPromise?: Promise<GameConfigRow>;"));
    assert!(content.contains("private gameConfigValue?: GameConfigRow;"));
    assert!(content.contains("gameConfig(): Promise<GameConfigRow>"));
    assert!(content.contains("gameConfigSync(): GameConfigRow"));
    assert!(content.contains("Promise.resolve(this.loader.open(\"game_config\")).then((root) =>"));
    assert!(content.contains("const root = openRuntimeDataSync(this.loader, \"game_config\");"));
    assert!(
        content.contains(
            "gameConfig: await Promise.resolve(loader.open(\"game_config\")).then(root =>"
        )
    );
    assert!(content.contains("const value = readGameConfigRow(root);"));
    assert!(!content.contains("export class GameConfigTable"));
    assert!(!content.contains("rows(): readonly GameConfigRow[]"));
}

#[test]
fn emits_linked_field_relation_api() {
    let meta = linked_field_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = TypeScriptCodeGenerator
        .generate(&input)
        .expect("typescript code");
    let content = &files[0].content;

    assert!(content.contains("type LinkCache = number;"));
    assert!(content.contains("type LinkListCache = number[] | undefined;"));
    assert!(content.contains("defaultItemLink: LinkCache;"));
    assert!(content.contains("starterItemsLink: LinkListCache;"));
    assert!(content.contains("defaultItem(data: GameData): ItemsRow;"));
    assert!(content.contains("starterItems(data: GameData): readonly ItemsRow[];"));
    assert!(!content.contains("  indexOfById(id: number): number | undefined"));
    assert!(content.contains(
        "function __ItemsTableindexOfById(table: ItemsTable, id: number): number | undefined"
    ));
    assert!(content.contains("link(data: GameData): void;"));
    assert!(content.contains("this.gameConfig.link(this);"));
    assert!(content.contains("async linkAll(): Promise<void>"));
    assert!(content.contains("linkAllSync(): void"));
    assert!(content.contains("await this.preloadAll();"));
    assert!(content.contains("gameConfig: await this.gameConfig(),"));
    assert!(content.contains("gameConfig: this.gameConfigSync(),"));
    assert!(content.contains("data.gameConfig.link(data);"));
    assert!(content.contains(
        "function defaultItemGameConfigRow(this: GameConfigRow, data: GameData): ItemsRow"
    ));
    assert!(content.contains("function starterItemsGameConfigRow(this: GameConfigRow, data: GameData): readonly ItemsRow[]"));
    assert!(
        content.contains("function linkGameConfigRow(this: GameConfigRow, data: GameData): void")
    );
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
        TypeScriptCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        "typescript codegen",
        "duplicate generated identifier `LazyGameData`",
        "top-level exports",
    );
}

fn linked_field_fixture() -> MetaModel {
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
    .expect("meta should load")
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
