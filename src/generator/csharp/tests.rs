// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use tempfile::tempdir;

use crate::{
    artifact::{ArtifactWriteMode, ArtifactWriter},
    code_artifact::{
        CodeExportTarget, CodeGenerator, CodegenOptions, CodegenTaskConfig, I18nFunctionReturn,
        I18nMode, RuntimeInterfaceMode,
        tests::{
            assert_duplicate_generated_identifier, expect_single_diagnostic, input_for,
            linked_fixture,
        },
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
            profile = "unity"
            namespace = "Demo.Generated"
            nullable = false
            access_modifier = "internal"
            class_name = "ReferenceGameData"
            lazy_class_name = "ReferenceLazyGameData"
            emit_partial = false
            runtime_interface = "external"
            runtime_namespace = "ConfigForge.Runtime"
            emit_runtime_adapters = true
        "#,
    )
    .expect("config should parse");
    let target = CodeExportTarget::from_task(config, &CSharpCodeGenerator).expect("target");
    let options = target
        .options
        .language_options::<CSharpCodegenOptions>(LANGUAGE)
        .expect("csharp options");

    assert_eq!(target.namespace.as_deref(), Some("Demo.Shared"));
    assert_eq!(options.profile, super::CSharpProfile::Unity);
    assert_eq!(options.namespace.as_deref(), Some("Demo.Generated"));
    assert!(!options.nullable);
    assert_eq!(options.access_modifier, "internal");
    assert_eq!(options.class_name.as_deref(), Some("ReferenceGameData"));
    assert_eq!(
        options.lazy_class_name.as_deref(),
        Some("ReferenceLazyGameData")
    );
    assert!(!options.emit_partial);
    assert_eq!(options.runtime_interface, RuntimeInterfaceMode::External);
    assert_eq!(
        options.runtime_namespace.as_deref(),
        Some("ConfigForge.Runtime")
    );
    assert!(options.emit_runtime_adapters);
}

#[test]
fn emits_builtin_value_types_and_dotnet_profile_mappings() {
    let meta = builtin_value_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("public readonly struct ConfigForgeColor"));
    assert!(content.contains("public readonly struct ConfigForgeVector2Int"));
    assert!(content.contains("public ConfigForgeColor Tint { get; init; }"));
    assert!(content.contains("public System.Numerics.Vector3 Position { get; init; }"));
    assert!(content.contains("public ConfigForgeVector2Int Grid { get; init; }"));
    assert!(content.contains("ReadColorValue(reader"));
    assert!(content.contains("ReadConfigForgeVector3Value(reader, new[] { \"right\", \"up\", \"forward\" }, (v1, v2, v3) => new System.Numerics.Vector3((float)v1, (float)v2, (float)v3))"));
}

#[test]
fn emits_unity_and_godot_profile_builtin_mappings() {
    let meta = builtin_value_fixture();
    let mut unity_input = input_for(&meta, LANGUAGE);
    unity_input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str("profile = \"unity\"").expect("toml value"),
    );

    let unity = CSharpCodeGenerator
        .generate(&unity_input)
        .expect("unity csharp code");
    let unity_content = &unity[0].content;
    assert!(unity_content.contains("public UnityEngine.Color Tint { get; init; }"));
    assert!(unity_content.contains("public UnityEngine.Vector3 Position { get; init; }"));
    assert!(unity_content.contains("public UnityEngine.Vector2Int Grid { get; init; }"));
    assert!(
        unity_content.contains("new UnityEngine.Color((float)r, (float)g, (float)b, (float)a)")
    );

    let mut godot_input = input_for(&meta, LANGUAGE);
    godot_input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str("profile = \"godot\"").expect("toml value"),
    );

    let godot = CSharpCodeGenerator
        .generate(&godot_input)
        .expect("godot csharp code");
    let godot_content = &godot[0].content;
    assert!(godot_content.contains("public Godot.Color Tint { get; init; }"));
    assert!(godot_content.contains("public Godot.Vector3 Position { get; init; }"));
    assert!(godot_content.contains("public Godot.Vector2I Grid { get; init; }"));
    assert!(godot_content.contains("new Godot.Color((float)r, (float)g, (float)b, (float)a)"));
}

#[test]
fn csharp_type_mappings_and_usings_override_builtin_profiles() {
    let meta = builtin_value_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str(
            r#"
                using = ["Game.Math"]

                [type_mappings]
                color = "Game.Math.ColorRgba"
                vector3 = "Game.Math.Float3"
                vector2_int = "Game.Math.Int2"
            "#,
        )
        .expect("toml value"),
    );

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("using Game.Math;"));
    assert!(content.contains("public Game.Math.ColorRgba Tint { get; init; }"));
    assert!(content.contains("public Game.Math.Float3 Position { get; init; }"));
    assert!(content.contains("public Game.Math.Int2 Grid { get; init; }"));
}

#[test]
fn csharp_unity_profile_normalizes_byte_color_channels() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "themes"
            source = "themes.csv"

            [[tables.fields]]
            name = "id"
            type = "string"
            required = true

            [[tables.fields]]
            name = "tint"
            type = "color"
            color_channel = "byte"
            required = true
        "#,
    )
    .expect("meta should parse");
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str("profile = \"unity\"").expect("toml value"),
    );

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains(
        "new UnityEngine.Color((float)(r / 255.0), (float)(g / 255.0), (float)(b / 255.0), (float)(a / 255.0))"
    ));
}

#[test]
fn rejects_invalid_csharp_options() {
    for (key, value, expected) in [
        ("namespace", "1.Bad", "invalid namespace"),
        ("access_modifier", "private", "invalid access_modifier"),
        ("class_name", "class", "invalid class_name"),
        ("lazy_class_name", "1Lazy", "invalid lazy_class_name"),
        ("runtime_namespace", "1.Bad", "invalid namespace"),
        ("using", "[\"Bad;Namespace\"]", "invalid using"),
    ] {
        let meta = linked_fixture();
        let mut input = input_for(&meta, LANGUAGE);
        let toml = if key == "runtime_namespace" {
            format!("runtime_interface = \"external\"\n{key} = {value:?}")
        } else if key == "using" {
            format!("{key} = {value}")
        } else {
            format!("{key} = {value:?}")
        };
        input.target.options.language_options.insert(
            LANGUAGE.to_owned(),
            toml::from_str(&toml).expect("toml value"),
        );

        let error = CSharpCodeGenerator.generate(&input).unwrap_err();
        let diagnostic = expect_single_diagnostic(error);
        assert!(
            diagnostic.message.contains(expected),
            "unexpected error for {key}: {}",
            diagnostic.message
        );
    }
}

#[test]
fn emits_csharp_standard_runtime_adapters_when_requested() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str("emit_runtime_adapters = true").expect("toml value"),
    );

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("public sealed class ConfigForgeJsonRuntimeDataLoaderAdapter : IConfigForgeRuntimeDataLoader"));
    assert!(content.contains("public sealed class ConfigForgeJsonRuntimeDataReaderAdapter : IConfigForgeRuntimeDataReader"));
    assert!(content.contains("public sealed class ConfigForgeMessagePackRuntimeDataLoaderAdapter : IConfigForgeRuntimeDataLoader"));
    assert!(content.contains("ConfigForge.Runtime.JsonReader.JsonRuntimeDataFileLoader"));
    assert!(content.contains("ConfigForge.Runtime.MessagePackReader.MessagePackRuntimeDataReader"));
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
fn emits_relation_helpers_and_game_data_access_interface() {
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
            name = "bonus_item_id"
            type = "int"
            required = false
            foreign_key = { name = "bonus_item", target_table = "items", linked_field = "bonus_item" }

            [[tables.fields]]
            name = "starter_item_ids"
            type = "int[]"
            required = true
            foreign_key = { name = "starter_item", target_table = "items", linked_field = "starter_items" }
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("public interface IGameDataAccess"));
    assert!(content.contains("public partial class GameData : IGameDataAccess"));
    assert!(content.contains("private int _defaultItemLink = -1;"));
    assert!(content.contains("private int _bonusItemLink = -1;"));
    assert!(content.contains("private int[]? _starterItemLink;"));
    assert!(content.contains("public ItemsRow DefaultItem(IGameDataAccess data)"));
    assert!(content.contains("public ItemsRow? BonusItem(IGameDataAccess data)"));
    assert!(content.contains("public IReadOnlyList<ItemsRow> StarterItems(IGameDataAccess data)"));
    assert!(content.contains("return data.Items.GetById(DefaultItemId)"));
    assert!(content.contains("return data.Items.GetById(BonusItemId.Value);"));
    assert!(content.contains("foreach (var key in StarterItemIds)"));
    assert!(content.contains("public void LinkAll()"));
}

#[test]
fn emits_generic_translator_text_helpers() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "name"
            type = "text"
            required = true

            [[tables.fields]]
            name = "description"
            type = "text"
            required = false
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("public interface ITextTranslator"));
    assert!(content.contains("string? Translate(string context, string text);"));
    assert!(content.contains("public string NameText(ITextTranslator translator)"));
    assert!(content.contains("var translated = translator.Translate(\"items.name\", Name);"));
    assert!(content.contains("return translated ?? Name;"));
    assert!(content.contains("public string? DescriptionText(ITextTranslator translator)"));
    assert!(content.contains("if (Description is null)"));
    assert!(content.contains("translator.Translate(\"items.description\", Description)"));
}

#[test]
fn emits_global_function_text_helpers() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "localized_name"

            [[structs.fields]]
            name = "display"
            type = "text"
            required = true
        "#,
    )
    .expect("meta should parse");
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.i18n.mode = I18nMode::GlobalFunction;
    input.target.options.i18n.function = Some("Translator.Translate".to_owned());
    input.target.options.i18n.returns = I18nFunctionReturn::Optional;
    input.target.options.i18n.csharp_using = Some("Demo.Localization".to_owned());

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("using Demo.Localization;"));
    assert!(!content.contains("public interface ITextTranslator"));
    assert!(content.contains("public string DisplayText()"));
    assert!(
        content.contains(
            "var translated = Translator.Translate(\"localized_name.display\", Display);"
        )
    );
    assert!(content.contains("return translated ?? Display;"));
}

#[test]
fn rejects_text_helper_name_collisions() {
    for toml in [
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "name"
            type = "text"
            required = true

            [[tables.fields]]
            name = "name_text"
            type = "string"
            required = true
        "#,
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "get_by_id"
            type = "text"
            required = true

            [[tables.indexes]]
            name = "by_id_text"
            fields = ["id"]
            unique = true
        "#,
        r#"
            [[tables]]
            name = "classes"
            source = "classes.csv"
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
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "name"
            type = "text"
            required = true

            [[tables.fields]]
            name = "class_id"
            type = "int"
            required = true
            foreign_key = { name = "class", target_table = "classes", linked_field = "name_text" }
        "#,
    ] {
        let meta = MetaModel::from_toml_str(toml).expect("meta should parse");
        let input = input_for(&meta, LANGUAGE);

        let error = CSharpCodeGenerator.generate(&input).unwrap_err();
        let diagnostic = expect_single_diagnostic(error);
        assert!(
            diagnostic
                .message
                .contains("duplicate generated identifier `NameText`")
                || diagnostic
                    .message
                    .contains("duplicate generated identifier `GetByIdText`"),
            "unexpected error: {}",
            diagnostic.message
        );
    }
}

#[test]
fn emits_lazy_game_data_with_retry_and_preload() {
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

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "game_rules"
            kind = "object"
            source = "game_rules.csv"

            [[tables.fields]]
            name = "starter_item_id"
            type = "int"
            required = true
            foreign_key = { name = "starter_item", target_table = "items", linked_field = "starter_item" }
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = CSharpCodeGenerator.generate(&input).expect("csharp code");
    let content = &files[0].content;

    assert!(content.contains("public partial class LazyGameData : IGameDataAccess"));
    assert!(content.contains("private readonly IConfigForgeRuntimeDataLoader _loader;"));
    assert!(content.contains("private ItemsTable? _items;"));
    assert!(content.contains("private GameRulesRow? _gameRules;"));
    assert!(content.contains("public ItemsTable Items"));
    assert!(content.contains("if (_items is not null)"));
    assert!(content.contains("var loaded = ItemsTable.Load(_loader);"));
    assert!(content.contains("_items = loaded;"));
    assert!(content.contains("public GameData PreloadAll()"));
    assert!(content.contains("return GameData.FromLoaded("));
    assert!(content.contains("Items"));
    assert!(content.contains("GameRules"));
    assert!(content.contains("public ItemsRow StarterItem(IGameDataAccess data)"));
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

    assert_duplicate_generated_identifier(
        CSharpCodeGenerator.generate(&input_for(&meta, LANGUAGE)),
        LANGUAGE,
        "duplicate generated identifier `DisplayName`",
        "table `items` row properties",
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

fn builtin_value_fixture() -> MetaModel {
    MetaModel::from_toml_str(
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
    .expect("meta should parse")
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
            asset_kind = "ui_icon"
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

            [[asset_kinds]]
            name = "ui_icon"
        "#,
    )
    .expect("meta should parse")
}
