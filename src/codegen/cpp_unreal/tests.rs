// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use tempfile::tempdir;

use crate::{
    artifact::{ArtifactWriteMode, ArtifactWriter},
    codegen::{
        CodeExportTarget, CodeGenerator, CodegenOptions, CodegenTaskConfig, GeneratedFile,
        tests::{input_for, linked_fixture},
    },
};

use super::{CppUnrealCodeGenerator, CppUnrealCodegenOptions, CppUnrealSupportLevel, LANGUAGE};

#[test]
fn parses_default_options_for_registered_language() {
    let options = CodegenOptions::for_language(LANGUAGE).expect("cpp_unreal options");

    assert_eq!(
        options.naming.type_name,
        crate::codegen::CaseStyle::PascalCase
    );
    let unreal = options
        .language_options::<CppUnrealCodegenOptions>(LANGUAGE)
        .expect("default cpp_unreal options");
    assert_eq!(unreal.support, CppUnrealSupportLevel::Containers);
}

#[test]
fn parses_containers_export_task_options() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"
            file_name = "reference_game_config"
            namespace = "ReferenceGameConfig"

            [cpp_unreal]
            support = "containers"
            api_macro = "REFERENCE_GAME_CONFIG_API"
            blueprint_category = "ConfigForge"
        "#,
    )
    .expect("config should parse");

    let target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let options = target
        .options
        .language_options::<CppUnrealCodegenOptions>(LANGUAGE)
        .expect("cpp_unreal options");

    assert_eq!(target.language, LANGUAGE);
    assert_eq!(
        target.single_file_name("generated", "h"),
        "reference_game_config.h"
    );
    assert_eq!(options.support, CppUnrealSupportLevel::Containers);
    assert_eq!(
        options.api_macro.as_deref(),
        Some("REFERENCE_GAME_CONFIG_API")
    );
    assert_eq!(options.blueprint_category.as_deref(), Some("ConfigForge"));
}

#[test]
fn rejects_invalid_support_option() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"

            [cpp_unreal]
            support = "native"
        "#,
    )
    .expect("config should parse");

    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let error = CppUnrealCodeGenerator.generate(&input).unwrap_err();

    assert!(format!("{error}").contains("failed to parse cpp_unreal codegen options"));
}

#[test]
fn rejects_invalid_api_macro() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"

            [cpp_unreal]
            api_macro = "GameConfigApi"
        "#,
    )
    .expect("config should parse");

    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let error = CppUnrealCodeGenerator.generate(&input).unwrap_err();

    assert!(format!("{error}").contains("api_macro `GameConfigApi`"));
}

#[test]
fn emits_header_and_source_skeleton() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.namespace = Some("ReferenceGameConfig".to_owned());

    let files = CppUnrealCodeGenerator
        .generate(&input)
        .expect("cpp_unreal code");

    assert_eq!(files.len(), 2);
    let header = generated_content(&files, "generated.h");
    let source = generated_content(&files, "generated.cpp");
    assert!(header.contains("#pragma once"));
    assert!(header.contains("#include \"CoreMinimal.h\""));
    assert!(header.contains("namespace ReferenceGameConfig"));
    assert!(header.contains("enum class EItemRarity"));
    assert!(header.contains("struct FRewardInfo"));
    assert!(header.contains("int64 Count;"));
    assert!(header.contains("struct FItemsRow"));
    assert!(header.contains("int64 Id;"));
    assert!(header.contains("FString DisplayName;"));
    assert!(header.contains("EItemRarity Rarity;"));
    assert!(header.contains("struct FItemsTable"));
    assert!(header.contains("TArray<FItemsRow> Rows;"));
    assert!(header.contains("struct FGameData"));
    assert!(header.contains("FItemsTable Items;"));
    assert!(header.contains("class IConfigForgeRuntimeDataReader"));
    assert!(header.contains("class IConfigForgeRuntimeDataLoader"));
    assert!(header.contains("TConfigForgeResult<TUniquePtr<IConfigForgeRuntimeDataReader>> Open"));
    assert!(header.contains("static constexpr const TCHAR* DataFile = TEXT(\"items\");"));
    assert!(header.contains("static TConfigForgeResult<FItemsTable> Load"));
    assert!(header.contains("static TConfigForgeResult<FItemsRow> ReadItemsRow"));
    assert!(header.contains("TMap<int64, int32> ById;"));
    assert!(header.contains("TMap<EItemRarity, TArray<int32>> ByRarity;"));
    assert!(header.contains("const FItemsRow* GetById(int64 Id) const;"));
    assert!(header.contains("TArray<const FItemsRow*> FindByRarity(EItemRarity Rarity) const;"));
    assert!(header.contains("static TConfigForgeResult<FGameData> Load"));
    assert!(header.contains("TEXT(\"ConfigForge\")"));
    assert!(!header.contains("std::vector"));
    assert!(!header.contains("std::unordered_map"));
    assert!(!header.contains("std::string"));
    assert!(!source.contains("throw std::runtime_error"));
    assert!(source.contains("#include \"generated.h\""));
    assert!(source.contains("TConfigForgeResult<FItemsTable> FItemsTable::Load"));
    assert!(source.contains("auto ReaderResult = Loader.Open(DataFile);"));
    assert!(source.contains("const int64 RowCount = Reader.ReadRowCount();"));
    assert!(source.contains("Reader.EnterIndex(RowIndex);"));
    assert!(source.contains("auto RowResult = FItemsRow::ReadItemsRow(Reader);"));
    assert!(source.contains("return FromRows(Rows);"));
    assert!(source.contains("Table.ById.Add(Row.Id, RowIndex);"));
    assert!(source.contains("Table.ByRarity.FindOrAdd(Row.Rarity).Add(RowIndex);"));
    assert!(source.contains("const FItemsRow* FItemsTable::GetById(int64 Id) const"));
    assert!(
        source.contains(
            "TArray<const FItemsRow*> FItemsTable::FindByRarity(EItemRarity Rarity) const"
        )
    );
    assert!(source.contains("TConfigForgeResult<FGameData> FGameData::Load"));
}

#[test]
fn emits_compound_unordered_index_key_and_lookup() {
    let meta = crate::meta::MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "localized_items"
            source = "localized_items.csv"

            [[tables.fields]]
            name = "item_id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "locale"
            type = "string"
            required = true

            [[tables.fields]]
            name = "display_name"
            type = "text"
            required = true

            [[tables.indexes]]
            name = "by_item_locale"
            fields = ["item_id", "locale"]
            unique = true
        "#,
    )
    .expect("meta should parse");

    let files = CppUnrealCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .expect("cpp_unreal code");
    let header = generated_content(&files, "generated.h");
    let source = generated_content(&files, "generated.cpp");

    assert!(header.contains("struct FLocalizedItemsByItemLocaleKey"));
    assert!(header.contains("int64 ItemId;"));
    assert!(header.contains("FString Locale;"));
    assert!(header.contains("bool operator==(const FLocalizedItemsByItemLocaleKey& Other) const"));
    assert!(
        header.contains(
            "FORCEINLINE uint32 GetTypeHash(const FLocalizedItemsByItemLocaleKey& Value)"
        )
    );
    assert!(header.contains("Hash = HashCombine(Hash, ::GetTypeHash(Value.ItemId));"));
    assert!(header.contains("TMap<FLocalizedItemsByItemLocaleKey, int32> ByItemLocale;"));
    assert!(header.contains(
        "const FLocalizedItemsRow* GetByItemLocale(int64 ItemId, FString Locale) const;"
    ));
    assert!(source.contains("FLocalizedItemsByItemLocaleKey{Row.ItemId, Row.Locale}"));
    assert!(source.contains("FLocalizedItemsByItemLocaleKey{ItemId, Locale}"));
}

#[test]
fn emits_non_unique_unordered_index_lookup() {
    let meta = crate::meta::MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "spawns"
            source = "spawns.csv"

            [[tables.fields]]
            name = "zone"
            type = "string"
            required = true

            [[tables.fields]]
            name = "npc_id"
            type = "int"
            required = true

            [[tables.indexes]]
            name = "by_zone"
            fields = ["zone"]
            unique = false
        "#,
    )
    .expect("meta should parse");

    let files = CppUnrealCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .expect("cpp_unreal code");
    let header = generated_content(&files, "generated.h");
    let source = generated_content(&files, "generated.cpp");

    assert!(header.contains("TMap<FString, TArray<int32>> ByZone;"));
    assert!(header.contains("TArray<const FSpawnsRow*> FindByZone(FString Zone) const;"));
    assert!(source.contains("Table.ByZone.FindOrAdd(Row.Zone).Add(RowIndex);"));
    assert!(source.contains("const TArray<int32>* RowIndexes = ByZone.Find(Zone);"));
    assert!(source.contains("Result.Reserve(RowIndexes->Num());"));
}

#[test]
fn emits_ordered_index_storage_and_range_lookup() {
    let meta = crate::meta::MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "level"
            type = "int"
            required = true

            [[tables.indexes]]
            name = "by_level"
            fields = ["level"]
            unique = false
            ordered = true
        "#,
    )
    .expect("meta should parse");

    let files = CppUnrealCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .expect("cpp_unreal code");
    let header = generated_content(&files, "generated.h");
    let source = generated_content(&files, "generated.cpp");

    assert!(header.contains("TArray<TPair<int64, int32>> ByLevel;"));
    assert!(header.contains("TArray<const FItemsRow*> FindByLevel(int64 Level) const;"));
    assert!(header.contains("const FItemsRow* LowerBoundByLevel(int64 Level) const;"));
    assert!(header.contains("const FItemsRow* UpperBoundByLevel(int64 Level) const;"));
    assert!(
        header.contains("TArray<const FItemsRow*> RangeByLevel(int64 Lower, int64 Upper) const;")
    );
    assert!(source.contains("Table.ByLevel.Add(TPair<int64, int32>(Row.Level, RowIndex));"));
    assert!(source.contains("Table.ByLevel.StableSort([](const auto& Left, const auto& Right) { return Left.Key < Right.Key; });"));
    assert!(source.contains("return RangeByLevel(Level, Level);"));
    assert!(source.contains("Algo::LowerBoundBy(ByLevel, Level"));
    assert!(source.contains("Algo::UpperBoundBy(ByLevel, Level"));
    assert!(source.contains(
        "TArray<const FItemsRow*> FItemsTable::RangeByLevel(int64 Lower, int64 Upper) const"
    ));
}

#[test]
fn maps_unreal_container_and_optional_types() {
    let meta = crate::meta::MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "RewardInfo"

            [[structs.fields]]
            name = "count"
            type = "int"
            required = true

            [[enums]]
            name = "ItemRarity"

            [[enums.values]]
            name = "common"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "tradable"
            type = "bool"
            required = true

            [[tables.fields]]
            name = "subtitle"
            type = "text"
            required = false

            [[tables.fields]]
            name = "rewards"
            type = "list<int>"
            required = true
        "#,
    )
    .expect("meta should parse");

    let files = CppUnrealCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .expect("cpp_unreal code");
    let header = generated_content(&files, "generated.h");

    assert!(header.contains("bool bTradable;"));
    assert!(header.contains("TOptional<FString> Subtitle;"));
    assert!(header.contains("TArray<int64> Rewards;"));
}

#[test]
fn rejects_unreal_generated_field_collisions() {
    let meta = crate::meta::MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "flags"
            source = "flags.csv"

            [[tables.fields]]
            name = "enabled"
            type = "bool"
            required = true

            [[tables.fields]]
            name = "Enabled"
            type = "bool"
            required = true
        "#,
    )
    .expect("meta should parse");

    let error = CppUnrealCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .unwrap_err();

    assert!(format!("{error}").contains("duplicate generated identifier `bEnabled`"));
}

#[test]
fn generate_to_sink_matches_legacy_generate_output() {
    let meta = linked_fixture();
    let temp_dir = tempdir().expect("temp dir");
    let mut input = input_for(&meta, LANGUAGE);
    input.target.out_dir = temp_dir.path().join("cpp_unreal_out");

    let legacy = CppUnrealCodeGenerator
        .generate(&input)
        .expect("legacy cpp_unreal code");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    CppUnrealCodeGenerator
        .generate_to_sink(&input, &sink)
        .expect("streamed cpp_unreal code");
    let result = sink.finish().expect("sink should finish");

    assert_eq!(legacy.len(), 2);
    assert_eq!(result.entries.len(), 2);
    for file in &legacy {
        let entry = result
            .entries
            .iter()
            .find(|entry| entry.path == file.path)
            .expect("streamed entry should match legacy path");
        assert_eq!(entry.bytes, file.content.len());
        assert_eq!(
            fs::read_to_string(&file.path).expect("streamed cpp_unreal file"),
            file.content
        );
    }
}

fn generated_content(files: &[GeneratedFile], file_name: &str) -> String {
    files
        .iter()
        .find(|file| file.path.file_name().and_then(|name| name.to_str()) == Some(file_name))
        .unwrap_or_else(|| panic!("missing generated file {file_name}"))
        .content
        .clone()
}
