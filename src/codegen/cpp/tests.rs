// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    codegen::{
        CodeExportInput, CodeGenerator, GeneratedFile,
        tests::{assert_duplicate_generated_identifier, input_for, linked_fixture},
    },
    meta::{MetaModel, TargetTags},
    runtime_contract::RuntimeDataFile,
};

use super::{CppCodeGenerator, CppCodegenOptions, LANGUAGE};

#[test]
fn emits_loader_and_repository_declarations() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::Value::try_from(CppCodegenOptions {
            namespace: Some("game::config".to_owned()),
        })
        .expect("cpp options"),
    );

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
    assert!(content.contains("#define CONFIG_FORGE_RUNTIME_CPP_READER_HAS_READ_LIST_LENGTH 1"));
    assert!(content.contains("static constexpr const char* DataFile = \"items\";"));
    assert!(!content.contains("RuntimeDataFormat"));
    assert!(!content.contains("IRuntimeDataDecoder"));
    assert!(!content.contains("IDataFileLoader"));
    assert!(content.contains("namespace game::config"));
    assert!(content.contains("} // namespace game::config"));
    assert!(content.contains("enum class ItemRarity"));
    assert!(content.contains("struct ItemsRow"));
    assert!(content.contains("class ItemsTable"));
    assert!(content.contains("class GameDataAccess"));
    assert!(content.contains("class GameData : public GameDataAccess"));
    assert!(content.contains("class LazyGameData : public GameDataAccess"));
    assert!(content.contains("explicit LazyGameData(std::shared_ptr<IRuntimeDataLoader> loader);"));
    assert!(content.contains("void PreloadAll() const;"));
    assert!(content.contains("void LinkAll() const;"));
    assert!(content.contains("mutable std::optional<ItemsTable> items_;"));
    assert!(content.contains("const ItemsTable& Items() const override;"));
    assert!(content.contains("const ItemsRow* GetById"));
    assert!(content.contains("std::vector<const ItemsRow*> FindByRarity"));
    assert!(source.contains("#include \"generated.hpp\""));
    assert!(source.contains("ItemsTable ItemsTable::Load(IRuntimeDataLoader& loader)"));
    assert!(source.contains("auto root = loader.Open(DataFile);"));
    assert!(source.contains("const std::size_t row_count = root->ReadRowCount();"));
    assert!(content.contains("virtual std::size_t ReadListLength()"));
    assert!(source.contains("const std::size_t length = reader.ReadListLength();"));
    assert!(!source.contains("reader.ReadArrayLength()"));
    assert!(source.contains("root->EnterIndex(row_index);"));
    assert!(source.contains("rows.push_back(ReadItemsRow(*root));"));
    assert!(source.contains("ItemsTable ItemsTable::FromRows(std::vector<ItemsRow> rows)"));
    assert!(source.contains("table.by_id_.emplace(row.id, row_index);"));
    assert!(source.contains("table.by_rarity_[row.rarity].push_back(row_index);"));
    assert!(source.contains("const ItemsRow* ItemsTable::GetById(std::int64_t id) const"));
    assert!(source.contains(
        "std::vector<const ItemsRow*> ItemsTable::FindByRarity(ItemRarity rarity) const"
    ));
    assert!(source.contains("GameData GameData::Load(IRuntimeDataLoader& loader)"));
    assert!(source.contains("data.items_ = ItemsTable::Load(loader);"));
    assert!(
        source.contains("LazyGameData::LazyGameData(std::shared_ptr<IRuntimeDataLoader> loader)")
    );
    assert!(source.contains("items_.emplace(ItemsTable::Load(*loader_));"));
    assert!(source.contains("void LazyGameData::PreloadAll() const"));
}

#[test]
fn emits_fragment_structs_and_optional_row_members() {
    let meta = fragment_fixture();

    let cpp = CppCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .expect("cpp code");
    let cpp_content = &cpp[0].content;
    assert!(cpp_content.contains("struct ItemsWeaponFragment"));
    assert!(cpp_content.contains("std::optional<ItemsWeaponFragment> weapon"));
    assert!(!cpp_content.contains("struct ItemsAttributesFragment"));
    assert!(cpp_content.contains("std::optional<AttributeSet> attributes"));

    let client_cpp = CppCodeGenerator
        .generate(&CodeExportInput {
            target_tags: TargetTags::new(["client"]),
            ..input_for(&meta, LANGUAGE)
        })
        .expect("client cpp code");
    let client_cpp_content = &client_cpp[0].content;
    assert!(client_cpp_content.contains("struct ItemsClientMarkerFragment"));
    assert!(client_cpp_content.contains("std::optional<ItemsClientMarkerFragment> client_marker"));
    assert!(!client_cpp_content.contains("server_marker"));
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

    let cpp = CppCodeGenerator.generate(&input).expect("cpp code");
    let header = generated_content(&cpp, "generated.hpp");
    let source = generated_content(&cpp, "generated.cpp");

    assert!(header.contains("std::optional<std::int64_t> bonus"));
    assert!(header.contains("std::optional<std::string> note"));
    assert!(header.contains("std::optional<RewardInfo> payload"));
    assert!(source.contains("value.bonus = ReadOptionalInt64(reader, \"bonus\");"));
    assert!(source.contains("OptionalField(reader, \"payload\", [](IRuntimeDataReader& value)"));
    assert!(source.contains("return ReadRewardInfo(value);"));
    assert!(!source.contains("ReadRewardInfoBinary"));
}

#[test]
fn emits_object_table_as_typed_object_not_repository() {
    let meta = object_table_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = CppCodeGenerator.generate(&input).expect("cpp code");
    let content = generated_content(&files, "generated.hpp");
    let source = generated_content(&files, "generated.cpp");

    assert!(content.contains(
        "virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;"
    ));
    assert!(content.contains("static constexpr const char* DataFile = \"game_config\";"));
    assert!(content.contains("const GameConfigRow& GameConfig() const"));
    assert!(content.contains("mutable std::optional<GameConfigRow> game_config_;"));
    assert!(content.contains("GameConfigRow game_config_;"));
    assert!(!content.contains("class GameConfigTable"));
    assert!(!content.contains("Rows() const"));
    assert!(source.contains("auto root_game_config_ = loader.Open(GameConfigRow::DataFile);"));
    assert!(source.contains("data.game_config_ = ReadGameConfigRow(*root_game_config_);"));
    assert!(source.contains("auto root = loader_->Open(GameConfigRow::DataFile);"));
    assert!(source.contains("game_config_.emplace(ReadGameConfigRow(*root));"));
}

#[test]
fn emits_linked_field_relation_api() {
    let meta = linked_field_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = CppCodeGenerator.generate(&input).expect("cpp code");
    let header = generated_content(&files, "generated.hpp");
    let source = generated_content(&files, "generated.cpp");

    assert!(header.contains("struct LinkCache"));
    assert!(header.contains("struct LinkListCache"));
    assert!(header.contains("mutable LinkCache default_item_link;"));
    assert!(header.contains("mutable LinkListCache starter_items_link;"));
    assert!(header.contains("const ItemsRow& GetDefaultItem(const GameDataAccess& data) const;"));
    assert!(header.contains(
        "std::vector<const ItemsRow*> GetStarterItems(const GameDataAccess& data) const;"
    ));
    assert!(header.contains("std::optional<std::size_t> IndexOfById(std::int64_t id) const;"));
    assert!(header.contains("void Link(const GameDataAccess& data) const;"));
    assert!(header.contains("void LinkAll() const;"));
    assert!(source.contains(
        "const ItemsRow& GameConfigRow::GetDefaultItem(const GameDataAccess& data) const"
    ));
    assert!(source.contains(
        "std::vector<const ItemsRow*> GameConfigRow::GetStarterItems(const GameDataAccess& data) const"
    ));
    assert!(
        source
            .contains("std::optional<std::size_t> ItemsTable::IndexOfById(std::int64_t id) const")
    );
    assert!(source.contains("void GameConfigRow::Link(const GameDataAccess& data) const"));
    assert!(source.contains("void GameData::LinkAll() const"));
    assert!(source.contains("void LazyGameData::LinkAll() const"));
    assert!(source.contains("PreloadAll();"));
    assert!(source.contains("GameConfig().Link(*this);"));
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
        CppCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        LANGUAGE,
        "duplicate generated identifier `LazyGameData`",
        "top-level types",
    );
}

#[test]
fn rejects_duplicate_generated_field_names() {
    let meta = duplicate_field_fixture();

    assert_duplicate_generated_identifier(
        CppCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        "cpp codegen",
        "duplicate generated identifier `foo_bar`",
        "table `items` row fields",
    );
}

#[test]
fn rejects_duplicate_top_level_generated_types() {
    let meta = duplicate_top_level_fixture();

    assert_duplicate_generated_identifier(
        CppCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        LANGUAGE,
        "duplicate generated identifier `ItemsRow`",
        "top-level types",
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
