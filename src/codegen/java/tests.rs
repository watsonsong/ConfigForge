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
            .contains("private final ReferenceGameConfig() {}")
    );
    assert!(files[0].content.contains(
        "public static final class ConfigForgeRuntimeException extends RuntimeException"
    ));
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
