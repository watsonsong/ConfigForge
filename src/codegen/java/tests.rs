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
    ] {
        let meta = linked_fixture();
        let mut input = input_for(&meta, LANGUAGE);
        input.target.options.language_options.insert(
            LANGUAGE.to_owned(),
            toml::from_str(&format!("{key} = {value:?}")).expect("toml value"),
        );

        let error = JavaCodeGenerator.generate(&input).unwrap_err();
        assert!(
            matches!(error, ConfigForgeError::Validation(ref message) if message.contains(expected)),
            "unexpected error for {key}: {error}"
        );
    }
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
    assert!(
        matches!(error, ConfigForgeError::Validation(ref message) if message.contains("must match root_class_name")),
        "unexpected error: {error}"
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

    let error = JavaCodeGenerator.generate(&input).unwrap_err();
    assert!(
        matches!(error, ConfigForgeError::Validation(ref message) if message.contains("duplicate generated identifier")),
        "unexpected error: {error}"
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
