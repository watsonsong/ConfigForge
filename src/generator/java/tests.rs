// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use tempfile::tempdir;

use crate::{
    artifact::{ArtifactWriteMode, ArtifactWriter},
    code_artifact::{
        CodeExportTarget, CodeGenerator, CodegenOptions, CodegenTaskConfig, RuntimeInterfaceMode,
        tests::{
            assert_duplicate_generated_identifier, expect_single_diagnostic, input_for,
            linked_fixture,
        },
    },
    meta::MetaModel,
};

use super::{JavaCodeGenerator, JavaCodegenOptions, LANGUAGE};

#[test]
fn codegen_options_for_language_succeeds() {
    let options = CodegenOptions::for_language(LANGUAGE).expect("java options");
    assert_eq!(options.formatting.indent_width, 4);
}

#[test]
fn export_target_from_task_reads_java_options() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "java"
            output = "out/java"
            namespace = "demo.shared"

            [java]
            package = "demo.generated"
            root_class_name = "ReferenceGameConfig"
            game_data_class_name = "ReferenceGameData"
            lazy_class_name = "ReferenceLazyGameData"
            access_modifier = "package"
            emit_final = false
            runtime_interface = "external"
            runtime_package = "configforge.runtime"
            emit_runtime_adapters = true

            [java.type_mappings]
            color = "com.badlogic.gdx.graphics.Color"
            vector3 = "com.badlogic.gdx.math.Vector3"
        "#,
    )
    .expect("config should parse");
    let target = CodeExportTarget::from_task(config, &JavaCodeGenerator).expect("target");
    let options = target
        .options
        .language_options::<JavaCodegenOptions>(LANGUAGE)
        .expect("java options");

    assert_eq!(target.namespace.as_deref(), Some("demo.shared"));
    assert_eq!(options.package.as_deref(), Some("demo.generated"));
    assert_eq!(
        options.root_class_name.as_deref(),
        Some("ReferenceGameConfig")
    );
    assert_eq!(
        options.game_data_class_name.as_deref(),
        Some("ReferenceGameData")
    );
    assert_eq!(
        options.lazy_class_name.as_deref(),
        Some("ReferenceLazyGameData")
    );
    assert_eq!(options.access_modifier, "package");
    assert!(!options.emit_final);
    assert_eq!(options.runtime_interface, RuntimeInterfaceMode::External);
    assert_eq!(
        options.runtime_package.as_deref(),
        Some("configforge.runtime")
    );
    assert!(options.emit_runtime_adapters);
    assert_eq!(
        options.type_mappings.color.as_deref(),
        Some("com.badlogic.gdx.graphics.Color")
    );
    assert_eq!(
        options.type_mappings.vector3.as_deref(),
        Some("com.badlogic.gdx.math.Vector3")
    );
}

#[test]
fn rejects_invalid_java_options() {
    for (key, value, expected) in [
        ("package", "1.bad", "invalid package"),
        ("access_modifier", "private", "invalid access_modifier"),
        ("root_class_name", "class", "invalid root_class_name"),
        (
            "game_data_class_name",
            "1GameData",
            "invalid game_data_class_name",
        ),
        ("lazy_class_name", "1Lazy", "invalid lazy_class_name"),
        ("runtime_package", "1.bad", "invalid package"),
    ] {
        let meta = linked_fixture();
        let mut input = input_for(&meta, LANGUAGE);
        let toml = if key == "runtime_package" {
            format!("runtime_interface = \"external\"\n{key} = {value:?}")
        } else {
            format!("{key} = {value:?}")
        };
        input.target.options.language_options.insert(
            LANGUAGE.to_owned(),
            toml::from_str(&toml).expect("toml value"),
        );

        let error = JavaCodeGenerator.generate(&input).unwrap_err();
        let diagnostic = expect_single_diagnostic(error);
        assert!(
            diagnostic.message.contains(expected),
            "unexpected error for {key}: {}",
            diagnostic.message
        );
    }
}

#[test]
fn java_type_mappings_override_builtin_value_types() {
    let meta = MetaModel::from_toml_str(
        r#"
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
            required = true

            [[tables.fields]]
            name = "grid"
            type = "vector2_int"
            required = true
        "#,
    )
    .expect("meta should parse");
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str(
            r#"
                [type_mappings]
                color = "com.badlogic.gdx.graphics.Color"
                vector3 = "com.badlogic.gdx.math.Vector3"
                vector2_int = "android.graphics.Point"
            "#,
        )
        .expect("toml value"),
    );

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("private final com.badlogic.gdx.graphics.Color tint;"));
    assert!(content.contains("private final com.badlogic.gdx.math.Vector3 position;"));
    assert!(content.contains("private final android.graphics.Point grid;"));
    assert!(content.contains("static com.badlogic.gdx.graphics.Color readColorValue"));
    assert!(content.contains("return new com.badlogic.gdx.graphics.Color("));
    assert!(content.contains("static com.badlogic.gdx.math.Vector3 readConfigForgeVector3Value"));
    assert!(content.contains("return new com.badlogic.gdx.math.Vector3("));
    assert!(content.contains("static android.graphics.Point readConfigForgeVector2IntValue"));
}

#[test]
fn rejects_java_file_name_root_class_mismatch() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.file_name = Some("OtherName.java".to_owned());
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str("root_class_name = \"ReferenceGameConfig\"").expect("toml value"),
    );

    let error = JavaCodeGenerator.generate(&input).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert!(
        diagnostic.message.contains("must match root_class_name"),
        "unexpected error: {}",
        diagnostic.message
    );
}

#[test]
fn emits_java_standard_runtime_adapters_when_requested() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.language_options.insert(
        LANGUAGE.to_owned(),
        toml::from_str("emit_runtime_adapters = true").expect("toml value"),
    );

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("public static final class ConfigForgeJsonRuntimeDataLoaderAdapter implements ConfigForgeRuntimeDataLoader"));
    assert!(content.contains("public static final class ConfigForgeJsonRuntimeDataReaderAdapter implements ConfigForgeRuntimeDataReader"));
    assert!(content.contains("public static final class ConfigForgeMessagePackRuntimeDataLoaderAdapter implements ConfigForgeRuntimeDataLoader"));
    assert!(content.contains("configforge.runtime.json.ConfigForgeJsonRuntimeDataLoader"));
    assert!(
        content.contains("configforge.runtime.messagepack.ConfigForgeMessagePackRuntimeDataReader")
    );
}

#[test]
fn generate_returns_single_java_file() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, LANGUAGE);
    input.target.file_name = Some("ReferenceGameConfig.java".to_owned());
    input.target.namespace = Some("referencegame.config".to_owned());

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].path,
        std::path::Path::new("out/ReferenceGameConfig.java")
    );
    assert!(files[0].content.contains("package referencegame.config;"));
    assert!(files[0].content.contains("import java.util.*;"));
    assert!(
        files[0]
            .content
            .contains("public final class ReferenceGameConfig")
    );
    assert!(
        files[0]
            .content
            .contains("private ReferenceGameConfig() {}")
    );
    assert!(files[0].content.contains(
        "public static final class ConfigForgeRuntimeException extends RuntimeException"
    ));
    assert!(files[0].content.contains("public enum ItemRarity"));
    assert!(files[0].content.contains("COMMON,"));
    assert!(files[0].content.contains("RARE;"));
    assert!(
        files[0]
            .content
            .contains("public static final class RewardInfo")
    );
    assert!(files[0].content.contains("private final long count;"));
    assert!(
        files[0]
            .content
            .contains("public long getCount() { return count; }")
    );
    assert!(
        files[0]
            .content
            .contains("public static final class ItemsRow")
    );
    assert!(files[0].content.contains("private final long id;"));
    assert!(
        files[0]
            .content
            .contains("private final String displayName;")
    );
    assert!(
        files[0]
            .content
            .contains("private final ItemRarity rarity;")
    );
    assert!(
        files[0]
            .content
            .contains("public String getDisplayName() { return displayName; }")
    );
    assert!(
        files[0]
            .content
            .contains("public static final class ItemsTable")
    );
    assert!(
        files[0]
            .content
            .contains("public static final String DATA_FILE = \"items\";")
    );
}

#[test]
fn emits_java_types_and_optional_field_mapping() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[enums]]
            name = "item_rarity"

            [[enums.values]]
            name = "common"

            [[structs]]
            name = "reward_info"

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
            name = "optional_level"
            type = "int"
            required = false

            [[tables.fields]]
            name = "ratio"
            type = "float"
            required = false

            [[tables.fields]]
            name = "enabled"
            type = "bool"
            required = false

            [[tables.fields]]
            name = "name"
            type = "string"
            required = false

            [[tables.fields]]
            name = "rarity"
            type = "item_rarity"
            required = true

            [[tables.fields]]
            name = "rarities"
            type = "list<item_rarity>"
            required = true

            [[tables.fields]]
            name = "flags"
            type = "list<bool>"
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

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("public enum ItemRarity"));
    assert!(content.contains("private final Long optionalLevel;"));
    assert!(content.contains("private final Double ratio;"));
    assert!(content.contains("private final Boolean enabled;"));
    assert!(content.contains("private final String name;"));
    assert!(content.contains("private final ItemRarity rarity;"));
    assert!(content.contains("private final ItemRarity[] rarities;"));
    assert!(content.contains("private final boolean[] flags;"));
}

#[test]
fn emits_runtime_reader_contract_and_game_data_load() {
    let meta = linked_fixture();
    let input = input_for(&meta, LANGUAGE);

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("public interface ConfigForgeRuntimeDataReader"));
    assert!(content.contains("public interface ConfigForgeRuntimeDataLoader"));
    assert!(content.contains("void enterRequiredField(String name);"));
    assert!(content.contains("boolean enterOptionalField(String name);"));
    assert!(content.contains("int readRowCount();"));
    assert!(content.contains("int readListLength();"));
    assert!(content.contains("static <T> T readRequiredField"));
    assert!(content.contains("static <T> T readOptionalField"));
    assert!(content.contains("static long[] readIntList"));
    assert!(content.contains("readObjectList(ConfigForgeRuntimeDataReader reader"));
    assert!(content.contains("private static ItemsRow readItemsRow"));
    assert!(content.contains("ConfigForgeRuntimeReader.readRequiredField(reader, \"id\", ConfigForgeRuntimeReader::readIntValue)"));
    assert!(content.contains("public static ItemsTable load(ConfigForgeRuntimeDataLoader loader)"));
    assert!(content.contains("ConfigForgeRuntimeDataReader reader = loader.open(DATA_FILE);"));
    assert!(content.contains("int rowCount = reader.readRowCount();"));
    assert!(content.contains("rows[rowIndex] = readItemsRow(reader);"));
    assert!(content.contains("public static GameData load(ConfigForgeRuntimeDataLoader loader)"));
    assert!(content.contains("ItemsTable.load(loader)"));
}

#[test]
fn emits_object_table_loader_without_repository() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.toml"

            [[tables.fields]]
            name = "season"
            type = "string"
            required = true

            [[tables.fields]]
            name = "optional_level"
            type = "int"
            required = false
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("public static final class GameConfigRow"));
    assert!(!content.contains("class GameConfigTable"));
    assert!(content.contains(
        "private static GameConfigRow loadGameConfig(ConfigForgeRuntimeDataLoader loader)"
    ));
    assert!(
        content.contains("ConfigForgeRuntimeDataReader reader = loader.open(\"game_config\");")
    );
    assert!(content.contains("public GameConfigRow getGameConfig() { return gameConfig; }"));
    assert!(content.contains("loadGameConfig(loader)"));
    assert!(content.contains("ConfigForgeRuntimeReader.readOptionalField(reader, \"optional_level\", ConfigForgeRuntimeReader::readIntValue)"));
}

#[test]
fn emits_hash_indexes_and_compound_key() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[enums]]
            name = "item_rarity"

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
            type = "item_rarity"
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

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("private final Map<Long, Integer> byIdIndex;"));
    assert!(content.contains("private final Map<ItemRarity, int[]> byRarityIndex;"));
    assert!(content.contains("private static final class ItemsByRarityLevelKey"));
    assert!(content.contains("public boolean equals(Object other)"));
    assert!(content.contains("return Objects.hash(rarity, level);"));
    assert!(content.contains("Map<Long, Integer> byIdIndex = new HashMap<>();"));
    assert!(
        content.contains("Map<ItemRarity, List<Integer>> byRarityIndexBuilder = new HashMap<>();")
    );
    assert!(content.contains(
        "throw new ConfigForgeRuntimeException(\"Duplicate unique index key for ItemsTable.by_id\")"
    ));
    assert!(content.contains("Map<ItemRarity, int[]> byRarityIndex = new HashMap<>();"));
    assert!(content.contains("public ItemsRow getById(long id)"));
    assert!(content.contains("Integer rowIndex = byIdIndex.get(id);"));
    assert!(content.contains("public List<ItemsRow> findByRarity(ItemRarity rarity)"));
    assert!(content.contains("return Collections.unmodifiableList(result);"));
    assert!(content.contains("public ItemsRow getByRarityLevel(ItemRarity rarity, long level)"));
    assert!(content.contains("new ItemsByRarityLevelKey(rarity, level)"));
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

            [[tables.fields]]
            name = "category"
            type = "string"
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

            [[tables.indexes]]
            name = "by_category_level"
            fields = ["category", "level"]
            unique = true
            ordered = true
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("private static final class ItemsByLevelEntry"));
    assert!(content.contains("private final ItemsByLevelEntry[] byLevelIndex;"));
    assert!(content.contains("Arrays.sort(byLevelIndex, (left, right) -> ConfigForgeRuntimeReader.compareValues(left.key, right.key));"));
    assert!(content.contains(
        "private static int lowerBoundItemsByLevelEntry(ItemsByLevelEntry[] entries, Long key)"
    ));
    assert!(content.contains(
        "private static int upperBoundItemsByLevelEntry(ItemsByLevelEntry[] entries, Long key)"
    ));
    assert!(content.contains("public List<ItemsRow> findByLevel(long level)"));
    assert!(content.contains("public int lowerBoundByLevel(long level)"));
    assert!(content.contains("public int upperBoundByLevel(long level)"));
    assert!(content.contains("public List<ItemsRow> rangeByLevel(long levelMin, long levelMax)"));
    assert!(content.contains("private static final class ItemsByCategoryLevelKey implements Comparable<ItemsByCategoryLevelKey>"));
    assert!(content.contains("public int compareTo(ItemsByCategoryLevelKey other)"));
    assert!(content.contains("throw new ConfigForgeRuntimeException(\"Duplicate unique index key for ItemsTable.by_category_level\")"));
    assert!(content.contains("public ItemsRow getByCategoryLevel(String category, long level)"));
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

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("public interface GameDataAccess"));
    assert!(content.contains("public static final class GameData implements GameDataAccess"));
    assert!(content.contains("private int defaultItemLink = -1;"));
    assert!(content.contains("private int bonusItemLink = -1;"));
    assert!(content.contains("private int[] starterItemLink;"));
    assert!(content.contains("public ItemsRow defaultItem(GameDataAccess data)"));
    assert!(content.contains("public ItemsRow bonusItem(GameDataAccess data)"));
    assert!(content.contains("public List<ItemsRow> starterItems(GameDataAccess data)"));
    assert!(content.contains("int targetIndex = data.getItems().indexOfById(getDefaultItemId());"));
    assert!(content.contains("return data.getItems().rowAt(defaultItemLink);"));
    assert!(content.contains("int targetIndex = data.getItems().indexOfById(getBonusItemId());"));
    assert!(content.contains("for (long key : getStarterItemIds())"));
    assert!(content.contains("starterItemLink = new int[indexes.size()];"));
    assert!(content.contains("public void linkAll()"));
    assert!(content.contains("gameConfig.defaultItem(this);"));
}

#[test]
fn emits_lazy_game_data_cache_retry_and_preload() {
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
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("public static final class LazyGameData implements GameDataAccess"));
    assert!(
        content
            .contains("public static LazyGameData loadLazy(ConfigForgeRuntimeDataLoader loader)")
    );
    assert!(content.contains("return new LazyGameData(loader);"));
    assert!(content.contains("private final ConfigForgeRuntimeDataLoader loader;"));
    assert!(content.contains("private ItemsTable items;"));
    assert!(content.contains("private GameConfigRow gameConfig;"));
    assert!(content.contains(
        "throw new ConfigForgeRuntimeException(\"LazyGameData requires a runtime data loader\")"
    ));
    assert!(content.contains("public ItemsTable getItems()"));
    assert!(content.contains("if (items != null)"));
    assert!(content.contains("ItemsTable loaded = ItemsTable.load(loader);"));
    assert!(content.contains("items = loaded;"));
    assert!(content.contains("public GameConfigRow getGameConfig()"));
    assert!(content.contains("GameConfigRow loaded = loadGameConfig(loader);"));
    assert!(content.contains("public GameData preloadAll()"));
    assert!(content.contains("return GameData.fromLoaded("));
    assert!(content.contains("getItems()"));
    assert!(content.contains("getGameConfig()"));
    assert!(content.contains("public ItemsRow defaultItem(GameDataAccess data)"));
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

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("public interface TextTranslator"));
    assert!(content.contains("String translate(String context, String text);"));
    assert!(content.contains("public String nameText(TextTranslator translator)"));
    assert!(content.contains("String translated = translator.translate(\"items.name\", name);"));
    assert!(content.contains("return translated != null ? translated : name;"));
    assert!(content.contains("public String descriptionText(TextTranslator translator)"));
    assert!(content.contains("if (description == null)"));
    assert!(content.contains("translator.translate(\"items.description\", description)"));
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

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "subtitle"
            type = "text"
            required = false
        "#,
    )
    .expect("meta should parse");
    let mut input = input_for(&meta, LANGUAGE);
    input.target.options.i18n.mode = crate::code_artifact::I18nMode::GlobalFunction;
    input.target.options.i18n.function = Some("GameI18n.translate".to_owned());
    input.target.options.i18n.returns = crate::code_artifact::I18nFunctionReturn::Optional;
    input.target.options.i18n.java_import = Some("game.runtime.i18n.GameI18n".to_owned());

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("import game.runtime.i18n.GameI18n;"));
    assert!(!content.contains("public interface TextTranslator"));
    assert!(content.contains("public String displayText()"));
    assert!(
        content.contains(
            "String translated = GameI18n.translate(\"localized_name.display\", display);"
        )
    );
    assert!(content.contains("return translated != null ? translated : display;"));
    assert!(content.contains("public String subtitleText()"));
    assert!(content.contains("if (subtitle == null)"));
    assert!(content.contains("GameI18n.translate(\"items.subtitle\", subtitle)"));
}

#[test]
fn rejects_text_helper_name_collisions() {
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
            name = "name_text"
            type = "string"
            required = true
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    assert_duplicate_generated_identifier(
        JavaCodeGenerator.generate(&input),
        LANGUAGE,
        "duplicate generated identifier `nameText`",
        "table `items` row members",
    );
}

#[test]
fn rejects_lazy_game_data_member_collisions() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "loader"
            source = "loader.csv"
        "#,
    )
    .expect("meta should parse");
    let input = input_for(&meta, LANGUAGE);

    assert_duplicate_generated_identifier(
        JavaCodeGenerator.generate(&input),
        LANGUAGE,
        "duplicate generated identifier `loader`",
        "LazyGameData members",
    );
}

#[test]
fn suffixes_java_keywords_by_default() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "class"
            source = "class.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "default"
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

    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let content = &files[0].content;

    assert!(content.contains("public static final class ClassRow"));
    assert!(content.contains("private final String default_;"));
    assert!(content.contains("public String getDefault_() { return default_; }"));
}

#[test]
fn rejects_duplicate_generated_java_member_names() {
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
            name = "display-name"
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

    assert_duplicate_generated_identifier(
        JavaCodeGenerator.generate(&input),
        LANGUAGE,
        "duplicate generated identifier `displayName`",
        "table `items` row members",
    );
}

#[test]
fn generate_to_sink_matches_legacy_generate_output() {
    let meta = linked_fixture();
    let temp_dir = tempdir().expect("temp dir");
    let mut input = input_for(&meta, LANGUAGE);
    input.target.out_dir = temp_dir.path().join("java_out");
    let files = JavaCodeGenerator.generate(&input).expect("java code");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();

    JavaCodeGenerator
        .generate_to_sink(&input, &sink)
        .expect("sink generation");
    let result = sink.finish().expect("finish sink");

    assert_eq!(files.len(), 1);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].path, files[0].path);
    assert_eq!(result.entries[0].bytes, files[0].content.len());
    let written = fs::read_to_string(&files[0].path).expect("written java");
    assert_eq!(written, files[0].content);
}
