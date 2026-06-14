// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use tempfile::tempdir;

use crate::{
    artifact::{ArtifactWriteMode, ArtifactWriter},
    code_artifact::{
        CodeExportTarget, CodeGenerator, CodegenOptions, CodegenTaskConfig, GeneratedFile,
        tests::{expect_single_diagnostic, input_for, linked_fixture},
    },
    diagnostic::code::CF_GENERATOR_INVALID_OPTION,
    meta::MetaModel,
};

use super::{CppUnrealCodeGenerator, CppUnrealCodegenOptions, CppUnrealSupportLevel, LANGUAGE};

#[test]
fn parses_default_options_for_registered_language() {
    let options = CodegenOptions::for_language(LANGUAGE).expect("cpp_unreal options");

    assert_eq!(
        options.naming.type_name,
        crate::code_artifact::CaseStyle::PascalCase
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
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.cpp_unreal.options")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("GameConfigApi"));
}

#[test]
fn rejects_blueprint_support_without_api_macro() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"

            [cpp_unreal]
            support = "blueprint"
        "#,
    )
    .expect("config should parse");

    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let error = CppUnrealCodeGenerator.generate(&input).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.cpp_unreal.options")
    );
    assert_eq!(
        diagnostic.current_value.as_deref(),
        Some("missing cpp_unreal.api_macro")
    );
}

#[test]
fn rejects_namespace_in_blueprint_support() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"
            namespace = "ReferenceGameConfig"

            [cpp_unreal]
            support = "blueprint"
            api_macro = "REFERENCE_GAME_CONFIG_API"
        "#,
    )
    .expect("config should parse");

    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let error = CppUnrealCodeGenerator.generate(&input).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.cpp_unreal.options")
    );
    assert_eq!(
        diagnostic.current_value.as_deref(),
        Some("namespace with blueprint support")
    );
}

#[test]
fn rejects_invalid_type_name_prefix() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"

            [cpp_unreal]
            type_name_prefix = "3Game"
        "#,
    )
    .expect("config should parse");

    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let error = CppUnrealCodeGenerator.generate(&input).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.cpp_unreal.options")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("3Game"));
}

#[test]
fn emits_blueprint_reflection_type_markers() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"
            file_name = "reference_game_config"

            [cpp_unreal]
            support = "blueprint"
            api_macro = "REFERENCE_GAME_CONFIG_API"
            blueprint_category = "Reference Config"
        "#,
    )
    .expect("config should parse");

    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let files = CppUnrealCodeGenerator
        .generate(&input)
        .expect("cpp_unreal blueprint code");
    let header = generated_content(&files, "reference_game_config.h");

    let core_include = header
        .find("#include \"CoreMinimal.h\"")
        .expect("CoreMinimal include");
    let generated_include = header
        .find("#include \"reference_game_config.generated.h\"")
        .expect("generated include");
    assert!(core_include < generated_include);
    assert_eq!(
        header[generated_include..]
            .lines()
            .take_while(|line| line.starts_with("#include"))
            .count(),
        1,
        "generated.h should be the final include"
    );
    assert!(header.contains("UENUM(BlueprintType)\nenum class EItemRarity : uint8"));
    assert!(
        header.contains("USTRUCT(BlueprintType)\nstruct REFERENCE_GAME_CONFIG_API FRewardInfo")
    );
    assert!(header.contains("USTRUCT(BlueprintType)\nstruct REFERENCE_GAME_CONFIG_API FItemsRow"));
    assert!(header.contains("    GENERATED_BODY()"));
    assert!(header.contains(
        "    UPROPERTY(BlueprintReadOnly, Category=\"Reference Config\")\n    int64 Id;"
    ));
    assert!(header.contains("#include \"Kismet/BlueprintFunctionLibrary.h\""));
    assert!(
        header.contains("UCLASS()\nclass REFERENCE_GAME_CONFIG_API UConfigForgeBlueprintLibrary")
    );
    assert!(header.contains("UFUNCTION(BlueprintPure, Category=\"Reference Config\")"));
    assert!(header.contains(
        "static bool GetItemsById(const FItemsTable& Table, int64 Id, FItemsRow& OutRow);"
    ));
    assert!(header.contains("static void FindItemsByRarity(const FItemsTable& Table, EItemRarity Rarity, TArray<FItemsRow>& OutRows);"));
    assert!(!header.contains("std::"));
    assert!(!header.contains("std::optional"));

    let source = generated_content(&files, "reference_game_config.cpp");
    assert!(source.contains(
        "bool UConfigForgeBlueprintLibrary::GetItemsById(const FItemsTable& Table, int64 Id, FItemsRow& OutRow)"
    ));
    assert!(source.contains("const FItemsRow* Row = Table.GetById(Id);"));
    assert!(source.contains("OutRow = *Row;"));
    assert!(source.contains(
        "void UConfigForgeBlueprintLibrary::FindItemsByRarity(const FItemsTable& Table, EItemRarity Rarity, TArray<FItemsRow>& OutRows)"
    ));
    assert!(source.contains("TArray<const FItemsRow*> Rows = Table.FindByRarity(Rarity);"));
}

#[test]
fn applies_type_name_prefix_to_generated_unreal_types() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"
            file_name = "reference_game_config"

            [cpp_unreal]
            support = "blueprint"
            api_macro = "REFERENCE_GAME_CONFIG_API"
            type_name_prefix = "Reference"
        "#,
    )
    .expect("config should parse");

    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let files = CppUnrealCodeGenerator
        .generate(&input)
        .expect("cpp_unreal blueprint code");
    let header = generated_content(&files, "reference_game_config.h");
    let source = generated_content(&files, "reference_game_config.cpp");

    assert!(header.contains("enum class EReferenceItemRarity : uint8"));
    assert!(header.contains("struct REFERENCE_GAME_CONFIG_API FReferenceRewardInfo"));
    assert!(!header.contains("struct REFERENCEItemsRow"));
    assert!(header.contains("struct REFERENCE_GAME_CONFIG_API FReferenceItemsRow"));
    assert!(header.contains("class IReferenceConfigForgeGameDataAccess"));
    assert!(header.contains("struct REFERENCE_GAME_CONFIG_API FReferenceGameData : public IReferenceConfigForgeGameDataAccess"));
    assert!(
        header
            .contains("struct FReferenceLazyGameData : public IReferenceConfigForgeGameDataAccess")
    );
    assert!(header.contains(
        "UCLASS()\nclass REFERENCE_GAME_CONFIG_API UReferenceConfigForgeBlueprintLibrary"
    ));
    assert!(header.contains("const FReferenceItemsTable& Table"));
    assert!(source.contains(
        "bool UReferenceConfigForgeBlueprintLibrary::GetItemsById(const FReferenceItemsTable& Table"
    ));
    assert!(source.contains("TConfigForgeResult<FReferenceGameData> FReferenceGameData::Load"));
    assert!(source.contains(
        "TConfigForgeResult<FReferenceGameData> FReferenceLazyGameData::PreloadAll() const"
    ));
    assert!(!header.contains("namespace ReferenceGameConfig"));
}

#[test]
fn skips_blueprint_property_for_optional_fields() {
    let meta = crate::meta::MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "reward_info"

            [[structs.fields]]
            name = "count"
            type = "int"
            required = true

            [[structs.fields]]
            name = "note"
            type = "string"
            required = false

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "subtitle"
            type = "text"
            required = false
        "#,
    )
    .expect("meta should parse");
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"

            [cpp_unreal]
            support = "blueprint"
            api_macro = "REFERENCE_GAME_CONFIG_API"
        "#,
    )
    .expect("config should parse");

    let mut input = input_for(&meta, LANGUAGE);
    input.target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let files = CppUnrealCodeGenerator
        .generate(&input)
        .expect("cpp_unreal blueprint code");
    let header = generated_content(&files, "generated.h");

    assert!(
        header.contains("UPROPERTY(BlueprintReadOnly, Category=\"ConfigForge\")\n    int64 Count;")
    );
    assert!(header.contains("    TOptional<FString> Note;"));
    assert!(header.contains("    TOptional<FString> Subtitle;"));
    assert!(!header.contains(
        "UPROPERTY(BlueprintReadOnly, Category=\"ConfigForge\")\n    TOptional<FString> Note;"
    ));
    assert!(!header.contains(
        "UPROPERTY(BlueprintReadOnly, Category=\"ConfigForge\")\n    TOptional<FString> Subtitle;"
    ));
}

#[test]
fn emits_blueprint_facade_for_ordered_range_queries() {
    let meta = crate::meta::MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "level_curves"
            source = "level_curves.csv"

            [[tables.fields]]
            name = "level"
            type = "int"
            required = true

            [[tables.fields]]
            name = "experience"
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
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "cpp_unreal"
            output = "Generated/Unreal"
            file_name = "reference_game_config"

            [cpp_unreal]
            support = "blueprint"
            api_macro = "REFERENCE_GAME_CONFIG_API"
            blueprint_category = "Reference Config"
        "#,
    )
    .expect("config should parse");

    let mut input = input_for(&meta, LANGUAGE);
    input.target = CodeExportTarget::from_task(config, &CppUnrealCodeGenerator).expect("target");
    let files = CppUnrealCodeGenerator
        .generate(&input)
        .expect("cpp_unreal blueprint code");
    let header = generated_content(&files, "reference_game_config.h");
    let source = generated_content(&files, "reference_game_config.cpp");

    assert!(header.contains("static void RangeLevelCurvesByLevel(const FLevelCurvesTable& Table, int64 Lower, int64 Upper, TArray<FLevelCurvesRow>& OutRows);"));
    assert!(source.contains(
        "void UConfigForgeBlueprintLibrary::RangeLevelCurvesByLevel(const FLevelCurvesTable& Table, int64 Lower, int64 Upper, TArray<FLevelCurvesRow>& OutRows)"
    ));
    assert!(
        source.contains("TArray<const FLevelCurvesRow*> Rows = Table.RangeByLevel(Lower, Upper);")
    );
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
fn emits_unreal_builtin_color_and_vector_mappings() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "SpawnPoint"

            [[structs.fields]]
            name = "grid"
            type = "vector2_int"
            required = true

            [[tables]]
            name = "effects"
            source = "effects.csv"

            [[tables.fields]]
            name = "id"
            type = "string"
            required = true

            [[tables.fields]]
            name = "tint"
            type = "color"
            color_channel = "normalized"
            required = true

            [[tables.fields]]
            name = "position"
            type = "vector3"
            vector_components = ["right", "up", "forward"]
            required = true

            [[tables.fields]]
            name = "spawn"
            type = "SpawnPoint"
            required = true
        "#,
    )
    .expect("meta should parse");

    let files = CppUnrealCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .expect("cpp_unreal code");
    let header = generated_content(&files, "generated.h");
    let source = generated_content(&files, "generated.cpp");

    assert!(header.contains("FLinearColor Tint;"));
    assert!(header.contains("FVector Position;"));
    assert!(header.contains("FIntPoint Grid;"));
    assert!(source.contains("Value.Tint = FLinearColor(static_cast<float>(R), static_cast<float>(G), static_cast<float>(B), static_cast<float>(A));"));
    assert!(source.contains("Reader.EnterRequiredField(TEXT(\"right\"));"));
    assert!(source.contains("Value.Position = FVector(static_cast<float>(V1), static_cast<float>(V2), static_cast<float>(V3));"));
    assert!(
        source.contains("Value.Grid = FIntPoint(static_cast<int32>(V1), static_cast<int32>(V2));")
    );
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
fn emits_relation_text_and_lazy_helpers() {
    let meta = crate::meta::MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "display_name"
            type = "text"
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
    .expect("meta should parse");

    let files = CppUnrealCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .expect("cpp_unreal code");
    let header = generated_content(&files, "generated.h");
    let source = generated_content(&files, "generated.cpp");

    assert!(header.contains("FString DisplayNameText("));
    assert!(header.contains(
        "const FItemsRow* GetDefaultItem(const IConfigForgeGameDataAccess& Data) const;"
    ));
    assert!(header.contains(
        "TArray<const FItemsRow*> GetStarterItems(const IConfigForgeGameDataAccess& Data) const;"
    ));
    assert!(header.contains("class IConfigForgeGameDataAccess"));
    assert!(header.contains("struct FGameData : public IConfigForgeGameDataAccess"));
    assert!(header.contains("struct FLazyGameData : public IConfigForgeGameDataAccess"));
    assert!(header.contains("void Link(const IConfigForgeGameDataAccess& Data) const;"));
    assert!(header.contains("void LinkAll() const;"));
    assert!(header.contains("TConfigForgeResult<const FItemsTable*> GetItems() const override;"));
    assert!(
        header
            .contains("TConfigForgeResult<const FGameConfigRow*> GetGameConfig() const override;")
    );
    assert!(
        source.contains(
            "const FItemsRow* FGameConfigRow::GetDefaultItem(const IConfigForgeGameDataAccess& Data) const"
        )
    );
    assert!(source.contains("auto TargetTableResult = Data.GetItems();"));
    assert!(source.contains(
        "if (!TargetTableResult.IsOk() || TargetTableResult.Value == nullptr) { return nullptr; }"
    ));
    assert!(source.contains("return TargetTableResult.Value->GetById(DefaultItemId);"));
    assert!(source.contains(
        "TArray<const FItemsRow*> FGameConfigRow::GetStarterItems(const IConfigForgeGameDataAccess& Data) const"
    ));
    assert!(source.contains(
        "if (!TargetTableResult.IsOk() || TargetTableResult.Value == nullptr) { return Result; }"
    ));
    assert!(source.contains("const auto* Row = TargetTableResult.Value->GetById(Key);"));
    assert!(source.contains("if (Row != nullptr) { Result.Add(Row); }"));
    assert!(
        source.contains("void FGameConfigRow::Link(const IConfigForgeGameDataAccess& Data) const")
    );
    assert!(source.contains("void FGameData::LinkAll() const"));
    assert!(source.contains("GameConfig.Link(*this);"));
    assert!(source.contains("TConfigForgeResult<const FItemsTable*> FGameData::GetItems() const"));
    assert!(source.contains("return TConfigForgeResult<const FItemsTable*>::Success(&Items);"));
    assert!(
        source.contains("TConfigForgeResult<const FItemsTable*> FLazyGameData::GetItems() const")
    );
    assert!(source.contains("TConfigForgeResult<FGameData> FLazyGameData::PreloadAll() const"));
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
