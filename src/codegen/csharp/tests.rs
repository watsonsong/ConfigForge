// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use tempfile::tempdir;

use crate::{
    ConfigForgeError,
    artifact::{ArtifactWriteMode, ArtifactWriter},
    codegen::{
        CodeExportTarget, CodeGenerator, CodegenOptions, CodegenTaskConfig,
        tests::{input_for, linked_fixture},
    },
    meta::MetaModel,
};

use super::{CSharpCodeGenerator, CSharpCodegenOptions, LANGUAGE};

#[test]
fn codegen_options_for_language_succeeds() {
    let options = CodegenOptions::for_language(LANGUAGE).expect("csharp options");
    assert_eq!(options.formatting.indent_width, 4);
}

#[test]
fn export_target_from_task_reads_csharp_options() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "csharp"
            output = "out/csharp"
            namespace = "Demo.Shared"

            [csharp]
            namespace = "Demo.Generated"
            nullable = false
            access_modifier = "internal"
            class_name = "ReferenceGameData"
            lazy_class_name = "ReferenceLazyGameData"
            emit_partial = false
        "#,
    )
    .expect("config should parse");
    let target = CodeExportTarget::from_task(config, &CSharpCodeGenerator).expect("target");
    let options = target
        .options
        .language_options::<CSharpCodegenOptions>(LANGUAGE)
        .expect("csharp options");

    assert_eq!(target.namespace.as_deref(), Some("Demo.Shared"));
    assert_eq!(options.namespace.as_deref(), Some("Demo.Generated"));
    assert!(!options.nullable);
    assert_eq!(options.access_modifier, "internal");
    assert_eq!(options.class_name.as_deref(), Some("ReferenceGameData"));
    assert_eq!(
        options.lazy_class_name.as_deref(),
        Some("ReferenceLazyGameData")
    );
    assert!(!options.emit_partial);
}

#[test]
fn rejects_invalid_csharp_options() {
    for (key, value, expected) in [
        ("namespace", "1.Bad", "invalid namespace"),
        ("access_modifier", "private", "invalid access_modifier"),
        ("class_name", "class", "invalid class_name"),
        ("lazy_class_name", "1Lazy", "invalid lazy_class_name"),
    ] {
        let meta = linked_fixture();
        let mut input = input_for(&meta, LANGUAGE);
        input.target.options.language_options.insert(
            LANGUAGE.to_owned(),
            toml::from_str(&format!("{key} = {value:?}")).expect("toml value"),
        );

        let error = CSharpCodeGenerator.generate(&input).unwrap_err();
        assert!(
            matches!(error, ConfigForgeError::Validation(ref message) if message.contains(expected)),
            "unexpected error for {key}: {error}"
        );
    }
}

#[test]
fn generate_returns_single_csharp_file() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.file_name = Some("ReferenceGameConfig.cs".to_owned());
    input.target.namespace = Some("ReferenceGame.Config".to_owned());

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].path,
        std::path::Path::new("out/ReferenceGameConfig.cs")
    );
    assert!(files[0].content.contains("#nullable enable"));
    assert!(files[0].content.contains("namespace ReferenceGame.Config"));
    assert!(
        files[0]
            .content
            .contains("public sealed class ConfigForgeRuntimeException : Exception")
    );
    assert!(files[0].content.contains("public partial class GameData"));
    assert!(
        files[0]
            .content
            .contains("public partial class LazyGameData")
    );
}

#[test]
fn emits_runtime_reader_contract_and_game_data_load() {
    let meta = linked_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("public interface IConfigForgeRuntimeDataReader"));
    assert!(content.contains("public interface IConfigForgeRuntimeDataLoader"));
    assert!(content.contains("void EnterRequiredField(string name);"));
    assert!(content.contains("bool EnterOptionalField(string name);"));
    assert!(content.contains("int ReadRowCount();"));
    assert!(content.contains("int ReadListLength();"));
    assert!(content.contains("public static T ReadRequiredField<T>"));
    assert!(content.contains("public static T? ReadOptionalClassField<T>"));
    assert!(content.contains("public static T[] ReadListValue<T>"));
    assert!(content.contains("internal static partial class ItemsRowReader"));
    assert!(content.contains("Id = ConfigForgeRuntimeReader.ReadRequiredField(reader, \"id\", ConfigForgeRuntimeReader.ReadIntValue)"));
    assert!(content.contains("public const string DataFile = \"items\";"));
    assert!(
        content.contains("internal static ItemsTable Load(IConfigForgeRuntimeDataLoader loader)")
    );
    assert!(content.contains("var rowCount = reader.ReadRowCount();"));
    assert!(content.contains("rows[rowIndex] = ItemsRowReader.Read(reader);"));
    assert!(content.contains("public static GameData Load(IConfigForgeRuntimeDataLoader loader)"));
    assert!(content.contains("ItemsTable.Load(loader)"));
}

#[test]
fn emits_hash_indexes_and_compound_key() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[enums]]
            name = "ItemRarity"

            [[enums.values]]
            name = "common"

            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
            required = true

            [[tables.fields]]
            name = "level"
            type = "int"
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

            [[tables.indexes]]
            name = "by_rarity_level"
            fields = ["rarity", "level"]
            unique = true
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("private readonly Dictionary<long, int> _byIdIndex;"));
    assert!(content.contains("private readonly Dictionary<ItemRarity, int[]> _byRarityIndex;"));
    assert!(content.contains(
        "public readonly struct ItemsByRarityLevelKey : IEquatable<ItemsByRarityLevelKey>"
    ));
    assert!(
        content.contains(
            "private readonly Dictionary<ItemsByRarityLevelKey, int> _byRarityLevelIndex;"
        )
    );
    assert!(content.contains("Duplicate unique index key for ItemsTable.by_id"));
    assert!(
        content.contains("var _byRarityIndexBuilder = new Dictionary<ItemRarity, List<int>>();")
    );
    assert!(content.contains("var _byRarityIndex = new Dictionary<ItemRarity, int[]>();"));
    assert!(content.contains("public ItemsRow? GetById(long id)"));
    assert!(content.contains("public IReadOnlyList<ItemsRow> FindByRarity(ItemRarity rarity)"));
    assert!(content.contains("public ItemsRow? GetByRarityLevel(ItemRarity rarity, long level)"));
}

#[test]
fn emits_ordered_index_storage_and_range_api() {
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

            [[tables.fields]]
            name = "level"
            type = "int"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables.indexes]]
            name = "by_level"
            fields = ["level"]
            unique = false
            ordered = true
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("public static int LowerBound<TKey>"));
    assert!(content.contains("public static int UpperBound<TKey>"));
    assert!(content.contains("private readonly (long Key, int RowIndex)[] _byLevelIndex;"));
    assert!(content.contains("Array.Sort(_byLevelIndex, (left, right) => Comparer<long>.Default.Compare(left.Key, right.Key));"));
    assert!(content.contains("public IReadOnlyList<ItemsRow> FindByLevel(long level)"));
    assert!(content.contains("public int LowerBoundByLevel(long level)"));
    assert!(content.contains("public int UpperBoundByLevel(long level)"));
    assert!(
        content
            .contains("public IReadOnlyList<ItemsRow> RangeByLevel(long levelMin, long levelMax)")
    );
    assert!(content.contains("RowsFromOrderedRange(_byLevelIndex, start, end)"));
}

#[test]
fn emits_object_table_as_loaded_property_without_repository() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"
            required = true
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("public partial class GameConfigRow"));
    assert!(!content.contains("class GameConfigTable"));
    assert!(content.contains("public GameConfigRow GameConfig { get; }"));
    assert!(content.contains("GameConfigRow.Load(loader)"));
}

#[test]
fn emits_csharp_types_and_nullable_field_mapping() {
    let meta = csharp_type_mapping_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("public enum ItemRarity"));
    assert!(content.contains("        Common,"));
    assert!(content.contains("public partial class RewardInfo"));
    assert!(content.contains("public long Count { get; init; }"));
    assert!(content.contains("public partial class ItemsRow"));
    assert!(content.contains("public long Id { get; init; }"));
    assert!(content.contains("public double Weight { get; init; }"));
    assert!(content.contains("public bool Enabled { get; init; }"));
    assert!(content.contains("public string DisplayName { get; init; } = string.Empty;"));
    assert!(content.contains("public string? OptionalName { get; init; }"));
    assert!(content.contains("public string Description { get; init; } = string.Empty;"));
    assert!(content.contains("public string Icon { get; init; } = string.Empty;"));
    assert!(content.contains("public ItemRarity Rarity { get; init; }"));
    assert!(content.contains("public RewardInfo Reward { get; init; } = new();"));
    assert!(content.contains("public long[] Values { get; init; } = Array.Empty<long>();"));
}

#[test]
fn rejects_duplicate_generated_csharp_field_names() {
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

            [[tables.fields]]
            name = "display_name"
            type = "string"
            required = true

            [[tables.fields]]
            name = "displayName"
            type = "string"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#,
    )
    .expect("meta should parse");

    let error = CSharpCodeGenerator
        .generate(&input_for(&meta, LANGUAGE))
        .unwrap_err();
    assert!(
        matches!(error, ConfigForgeError::Validation(ref message) if message.contains("duplicate generated identifier `DisplayName`")),
        "unexpected error: {error}"
    );
}

#[test]
fn suffixes_csharp_keywords_by_default() {
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

            [[tables.fields]]
            name = "class"
            type = "string"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    assert!(
        files[0]
            .content
            .contains("public string Class { get; init; } = string.Empty;")
    );
}

#[test]
fn generate_to_sink_matches_legacy_generate_output() {
    let meta = linked_fixture();
    let temp_dir = tempdir().expect("temp dir");
    let mut input = input_for(&meta, LANGUAGE);
    input.target.out_dir = temp_dir.path().join("csharp_out");

    let legacy = CSharpCodeGenerator
        .generate(&input)
        .expect("legacy csharp code");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    CSharpCodeGenerator
        .generate_to_sink(&input, &sink)
        .expect("streamed csharp code");
    let result = sink.finish().expect("sink should finish");

    assert_eq!(legacy.len(), 1);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].path, legacy[0].path);
    assert_eq!(result.entries[0].bytes, legacy[0].content.len());
    assert_eq!(
        fs::read_to_string(&legacy[0].path).expect("streamed csharp file"),
        legacy[0].content
    );
}

fn csharp_type_mapping_fixture() -> MetaModel {
    MetaModel::from_toml_str(
        r#"
            [[enums]]
            name = "ItemRarity"

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
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "weight"
            type = "float"
            required = true

            [[tables.fields]]
            name = "enabled"
            type = "bool"
            required = true

            [[tables.fields]]
            name = "display_name"
            type = "string"
            required = true

            [[tables.fields]]
            name = "optional_name"
            type = "string"
            required = false

            [[tables.fields]]
            name = "description"
            type = "text"
            required = true

            [[tables.fields]]
            name = "icon"
            type = "asset"
            required = true

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
            required = true

            [[tables.fields]]
            name = "reward"
            type = "RewardInfo"
            required = true

            [[tables.fields]]
            name = "values"
            type = "list<int>"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#,
    )
    .expect("meta should parse")
}
