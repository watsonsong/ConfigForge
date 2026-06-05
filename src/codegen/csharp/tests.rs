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
