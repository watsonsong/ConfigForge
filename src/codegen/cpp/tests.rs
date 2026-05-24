// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    codegen::{
        CodeExportInput, CodeGenerator, CppCodeGenerator, CppCodegenOptions, GeneratedFile,
        languages,
        tests::{assert_duplicate_generated_identifier, input_for, linked_fixture},
    },
    meta::{MetaModel, TargetTags},
};

#[test]
fn emits_loader_and_repository_declarations() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, languages::CPP);
    input.target.options.language_options.insert(
        languages::CPP.to_owned(),
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
fn emits_fragment_structs_and_optional_row_members() {
    let meta = fragment_fixture();

    let cpp = CppCodeGenerator
        .generate(&input_for(&meta, languages::CPP))
        .expect("cpp code");
    let cpp_content = &cpp[0].content;
    assert!(cpp_content.contains("struct ItemsWeaponFragment"));
    assert!(cpp_content.contains("std::optional<ItemsWeaponFragment> weapon"));
    assert!(!cpp_content.contains("struct ItemsAttributesFragment"));
    assert!(cpp_content.contains("std::optional<AttributeSet> attributes"));

    let client_cpp = CppCodeGenerator
        .generate(&CodeExportInput {
            target_tags: TargetTags::new(["client"]),
            ..input_for(&meta, languages::CPP)
        })
        .expect("client cpp code");
    let client_cpp_content = &client_cpp[0].content;
    assert!(client_cpp_content.contains("struct ItemsClientMarkerFragment"));
    assert!(client_cpp_content.contains("std::optional<ItemsClientMarkerFragment> client_marker"));
    assert!(!client_cpp_content.contains("server_marker"));
}

#[test]
fn emits_object_table_as_typed_object_not_repository() {
    let meta = object_table_fixture();
    let input = input_for(&meta, languages::CPP);

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

#[test]
fn rejects_duplicate_generated_field_names() {
    let meta = duplicate_field_fixture();

    assert_duplicate_generated_identifier(
        CppCodeGenerator.generate(&input_for(&meta, languages::CPP)),
        "cpp codegen",
        "duplicate generated identifier `foo_bar`",
        "table `items` row fields",
    );
}

#[test]
fn rejects_duplicate_top_level_generated_types() {
    let meta = duplicate_top_level_fixture();

    assert_duplicate_generated_identifier(
        CppCodeGenerator.generate(&input_for(&meta, languages::CPP)),
        languages::CPP,
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
