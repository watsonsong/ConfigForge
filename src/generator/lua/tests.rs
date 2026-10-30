// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::PathBuf};

use tempfile::tempdir;

use crate::{
    artifact::{ArtifactContent, ArtifactWriteMode, ArtifactWriter},
    code_artifact::{I18nMode, tests::expect_single_diagnostic},
    data::{DataSet, DataTable},
    data_artifact::TargetDataConfig,
    diagnostic::code::{CF_ARTIFACT_PATH_COLLISION, CF_GENERATOR_UNSUPPORTED_DATA_INPUT},
    meta::{MetaModel, PrimitiveFieldType, TargetTags},
};

use super::{
    FORMAT, LuaTableExporter,
    options::{LuaTableOptions, options_from_task},
    render::{lua_string, render_file_header, render_scalar},
};

fn lua_task() -> TargetDataConfig {
    TargetDataConfig {
        name: Some("lua".to_owned()),
        output: PathBuf::from("out"),
        format: FORMAT.to_owned(),
        options: BTreeMap::from([(
            "module_name".to_owned(),
            toml::Value::String("game.config".to_owned()),
        )]),
    }
}

fn lua_task_with_options(options: BTreeMap<String, toml::Value>) -> TargetDataConfig {
    TargetDataConfig {
        options,
        ..lua_task()
    }
}

#[test]
fn escapes_lua_strings_and_rejects_non_finite_float() {
    assert_eq!(lua_string("a\n\"b\\c"), r#""a\n\"b\\c""#);
    let error = render_scalar("NaN", PrimitiveFieldType::Float).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_DATA_INPUT);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.lua_table.symbols")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("NaN"));
}

#[test]
fn plans_root_and_table_module_artifacts() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let paths = LuaTableExporter
        .plan_paths(
            PathBuf::from("out").as_path(),
            &lua_task(),
            &linked,
            &TargetTags::empty(),
        )
        .expect("paths should plan");
    assert_eq!(
        paths,
        vec![
            PathBuf::from("out/game/config/init.lua"),
            PathBuf::from("out/game/config/items.lua")
        ]
    );
}

#[test]
fn exports_collection_rows_with_metatable_and_indexes() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[structs]]
                name = "Stack"

                [[structs.fields]]
                name = "max_stack"
                type = "int"

                [[tables]]
                name = "items"
                source = "items.csv"
                primary_index = "by_id"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "name"
                type = "text"
                required = true

                [[tables.fields]]
                name = "stack"
                type = "Stack"

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true
                primary = true
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "2001".to_owned()),
            ("name".to_owned(), "Iron Ore".to_owned()),
            ("stack".to_owned(), r#"{"max_stack":999}"#.to_owned()),
        ])],
    )]);
    let artifacts = LuaTableExporter
        .export(
            PathBuf::from("out").as_path(),
            &lua_task(),
            &linked,
            &TargetTags::empty(),
            &data_set,
        )
        .expect("lua should export");
    let items = artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with("items.lua"))
        .expect("items artifact");
    let ArtifactContent::Text(content) = &items.content else {
        panic!("text artifact")
    };
    assert!(content.contains("local FIELD ="));
    assert!(content.contains("name = 2"));
    assert!(content.contains(
        "setmetatable({ [1] = 2001, [2] = \"Iron Ore\", [3] = { max_stack = 999 }, }, ROW_MT)"
    ));
    assert!(content.contains("function T.get_by_id(id)"));
    assert!(content.contains("return by_id[id]"));
}

#[test]
fn export_to_sink_writes_root_and_table_files() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "name"
                type = "text"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "2001".to_owned()),
            ("name".to_owned(), "Iron Ore".to_owned()),
        ])],
    )]);
    let temp_dir = tempdir().expect("temp dir");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();

    LuaTableExporter
        .export_to_sink(
            temp_dir.path(),
            &lua_task(),
            &linked,
            &TargetTags::empty(),
            &data_set,
            &sink,
        )
        .expect("Lua should export to sink");
    sink.finish().expect("sink should finish");

    let root = fs::read_to_string(temp_dir.path().join("game/config/init.lua"))
        .expect("root Lua file should be written");
    let items = fs::read_to_string(temp_dir.path().join("game/config/items.lua"))
        .expect("table Lua file should be written");
    assert!(root.contains("require(\"game.config.items\")"));
    assert!(items.contains("[2] = \"Iron Ore\""));
}

#[test]
fn emits_standard_file_header_and_supports_custom_header() {
    assert!(render_file_header(&LuaTableOptions::default()).starts_with(
        "-- Copyright (c) 2026 ConfigForge contributors\n-- SPDX-License-Identifier: Apache-2.0\n"
    ));

    let options = LuaTableOptions {
        file_header: Some("-- custom header".to_owned()),
        ..LuaTableOptions::default()
    };
    assert_eq!(render_file_header(&options), "-- custom header\n");
    assert_eq!(
        options.code_artifact_options().file_header.as_deref(),
        Some("-- custom header")
    );
}

#[test]
fn exports_text_helpers_with_global_i18n() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "name"
                type = "text"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "2001".to_owned()),
            ("name".to_owned(), "Iron Ore".to_owned()),
        ])],
    )]);
    let task = lua_task_with_options(BTreeMap::from([
        (
            "module_name".to_owned(),
            toml::Value::String("game.config".to_owned()),
        ),
        (
            "i18n".to_owned(),
            toml::Value::Table(toml::Table::from_iter([
                (
                    "mode".to_owned(),
                    toml::Value::String("global_function".to_owned()),
                ),
                (
                    "function".to_owned(),
                    toml::Value::String("globalTranslate".to_owned()),
                ),
                (
                    "returns".to_owned(),
                    toml::Value::String("optional".to_owned()),
                ),
            ])),
        ),
    ]));

    let artifacts = LuaTableExporter
        .export(
            PathBuf::from("out").as_path(),
            &task,
            &linked,
            &TargetTags::empty(),
            &data_set,
        )
        .expect("lua should export");
    let items = artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with("items.lua"))
        .expect("items artifact");
    let ArtifactContent::Text(content) = &items.content else {
        panic!("text artifact")
    };

    assert!(content.contains("function ROW_METHODS.name_text(row)"));
    assert!(content.contains("globalTranslate(\"items.name\", value)"));
    assert!(content.contains("if translated ~= nil then return translated end"));
    let options = options_from_task(&task).expect("lua options should parse");
    assert_eq!(
        options.code_artifact_options().i18n.mode,
        I18nMode::GlobalFunction
    );
}

#[test]
fn exports_foreign_key_link_helpers() {
    let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                primary_index = "by_id"

                [[tables.fields]]
                name = "id"
                type = "int"

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
                name = "default_item_id"
                type = "int"
                foreign_key = { name = "default_item", target_table = "items", target_index = "by_id", linked_field = "default_item" }

                [[tables.fields]]
                name = "starter_item_ids"
                type = "int[]"
                foreign_key = { name = "starter_item", target_table = "items", target_index = "by_id", linked_field = "starter_items" }
            "#,
        )
        .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "2001".to_owned())])],
        ),
        DataTable::new(
            "game_rules",
            vec![BTreeMap::from([
                ("default_item_id".to_owned(), "2001".to_owned()),
                ("starter_item_ids".to_owned(), "[2001]".to_owned()),
            ])],
        ),
    ]);

    let artifacts = LuaTableExporter
        .export(
            PathBuf::from("out").as_path(),
            &lua_task(),
            &linked,
            &TargetTags::empty(),
            &data_set,
        )
        .expect("lua should export");
    let rules = artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with("game_rules.lua"))
        .expect("game rules artifact");
    let ArtifactContent::Text(content) = &rules.content else {
        panic!("text artifact")
    };

    assert!(content.contains("function T.default_item(data)"));
    assert!(content.contains("local target = data.items()"));
    assert!(content.contains("local value = target.get_by_id(T.default_item_id)"));
    assert!(content.contains("function T.starter_items(data)"));
    assert!(content.contains("for _, key in ipairs(keys) do"));
}

#[test]
fn exports_object_table_as_keyed_table() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "game_rules"
                kind = "object"
                source = "game_rules.csv"

                [[tables.fields]]
                name = "max_player_level"
                type = "int"

                [[tables.fields]]
                name = "pvp_enabled"
                type = "bool"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "game_rules",
        vec![BTreeMap::from([
            ("max_player_level".to_owned(), "60".to_owned()),
            ("pvp_enabled".to_owned(), "true".to_owned()),
        ])],
    )]);
    let artifacts = LuaTableExporter
        .export(
            PathBuf::from("out").as_path(),
            &lua_task(),
            &linked,
            &TargetTags::empty(),
            &data_set,
        )
        .expect("lua should export");
    let rules = artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with("game_rules.lua"))
        .expect("game rules artifact");
    let ArtifactContent::Text(content) = &rules.content else {
        panic!("text artifact")
    };
    assert!(content.contains("max_player_level = 60"));
    assert!(content.contains("pvp_enabled = true"));
}

#[test]
fn rejects_duplicate_generated_table_files() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "item-types"
                source = "a.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "item_types"
                source = "b.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let error = LuaTableExporter
        .plan_paths(
            PathBuf::from("out").as_path(),
            &lua_task(),
            &linked,
            &TargetTags::empty(),
        )
        .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_ARTIFACT_PATH_COLLISION);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.lua_table.symbols")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("item_types.lua"));
}

#[test]
fn exports_non_unique_and_composite_indexes() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "class_levels"
                source = "class_levels.csv"

                [[tables.fields]]
                name = "class_id"
                type = "int"

                [[tables.fields]]
                name = "level"
                type = "int"

                [[tables.fields]]
                name = "label"
                type = "string"

                [[tables.indexes]]
                name = "by_class_level"
                fields = ["class_id", "level"]
                unique = true

                [[tables.indexes]]
                name = "by_class"
                fields = ["class_id"]
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "class_levels",
        vec![
            BTreeMap::from([
                ("class_id".to_owned(), "1".to_owned()),
                ("level".to_owned(), "1".to_owned()),
                ("label".to_owned(), "Warrior 1".to_owned()),
            ]),
            BTreeMap::from([
                ("class_id".to_owned(), "1".to_owned()),
                ("level".to_owned(), "2".to_owned()),
                ("label".to_owned(), "Warrior 2".to_owned()),
            ]),
        ],
    )]);

    let artifacts = LuaTableExporter
        .export(
            PathBuf::from("out").as_path(),
            &lua_task(),
            &linked,
            &TargetTags::empty(),
            &data_set,
        )
        .expect("lua should export");
    let class_levels = artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with("class_levels.lua"))
        .expect("class levels artifact");
    let ArtifactContent::Text(content) = &class_levels.content else {
        panic!("text artifact")
    };
    assert!(content.contains("function T.get_by_class_level(class_id, level)"));
    assert!(content.contains("return level_1[level]"));
    assert!(content.contains("function T.get_by_class(class_id)"));
    assert!(content.contains("bucket[#bucket + 1] = row"));
}
