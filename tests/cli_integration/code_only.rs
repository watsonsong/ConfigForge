// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::*;
use assert_cmd::Command;
use predicates::prelude::*;
use std::{fs, process::Command as StdCommand};

#[test]
fn builds_code_only_target_without_loading_data() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            toml_path(&out_dir)
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    let rust_code = fs::read_to_string(out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub struct ItemsRow"));
}

#[test]
fn builds_csharp_code_only_target() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_csharp_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "csharp"
                output = "{}"
                file_name = "reference_game_config"
                namespace = "ReferenceGame.Config"
            "#,
            standard_core_meta_imports(),
            toml_path(&out_dir)
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    let csharp_code =
        fs::read_to_string(out_dir.join("reference_game_config.cs")).expect("csharp code");
    assert!(csharp_code.contains("namespace ReferenceGame.Config"));
    assert!(csharp_code.contains("public sealed class ConfigForgeRuntimeException"));
    assert!(csharp_code.contains("public partial class GameData"));
}

#[test]
fn builds_java_code_only_target() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_java_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "java"
                output = "{}"
                file_name = "ReferenceGameConfig"
                namespace = "referencegame.config"
            "#,
            standard_core_meta_imports(),
            toml_path(&out_dir)
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    let java_code = fs::read_to_string(out_dir.join("ReferenceGameConfig.java"))
        .expect("java code should be generated");
    assert!(java_code.contains("package referencegame.config;"));
    assert!(java_code.contains("public final class ReferenceGameConfig"));
    assert!(java_code.contains("ConfigForgeRuntimeException extends RuntimeException"));
}

#[test]
fn builds_cpp_unreal_code_only_target() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_unreal_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "cpp_unreal"
                output = "{}"
                file_name = "reference_game_config"
                namespace = "ReferenceGameConfig"

                [targets.exports.cpp_unreal]
                support = "containers"
                api_macro = "REFERENCE_GAME_CONFIG_API"
            "#,
            standard_core_meta_imports(),
            toml_path(&out_dir)
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    let header =
        fs::read_to_string(out_dir.join("reference_game_config.h")).expect("cpp_unreal header");
    let source =
        fs::read_to_string(out_dir.join("reference_game_config.cpp")).expect("cpp_unreal source");
    assert!(header.contains("#include \"CoreMinimal.h\""));
    assert!(header.contains("namespace ReferenceGameConfig"));
    assert!(source.contains("#include \"reference_game_config.h\""));

    if require_tool_or_skip("cpp_unreal syntax smoke", "c++").is_some() {
        let shim_dir = temp_dir.path().join("unreal_shim");
        write_cpp_unreal_core_minimal_shim(&shim_dir);
        let output = StdCommand::new("c++")
            .arg("-std=c++17")
            .arg("-fsyntax-only")
            .arg(out_dir.join("reference_game_config.cpp"))
            .arg("-I")
            .arg(&out_dir)
            .arg("-I")
            .arg(&shim_dir)
            .output()
            .expect("cpp_unreal syntax smoke should run");
        assert_command_success(output, "cpp_unreal syntax smoke");
    }
}

#[test]
fn builds_cpp_unreal_blueprint_code_only_target() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_unreal_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "cpp_unreal"
                output = "{}"
                file_name = "reference_game_config"

                [targets.exports.cpp_unreal]
                support = "blueprint"
                api_macro = "REFERENCE_GAME_CONFIG_API"
                blueprint_category = "Reference Config"
                type_name_prefix = "Reference"
            "#,
            standard_core_meta_imports(),
            toml_path(&out_dir)
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    let header =
        fs::read_to_string(out_dir.join("reference_game_config.h")).expect("cpp_unreal header");
    assert!(header.contains("#include \"reference_game_config.generated.h\""));
    assert!(header.contains("USTRUCT(BlueprintType)"));
    assert!(header.contains("FReferenceItemsRow"));
    assert!(header.contains("UReferenceConfigForgeBlueprintLibrary"));
    assert!(header.contains("UPROPERTY(BlueprintReadOnly, Category=\"Reference Config\")"));

    if require_tool_or_skip("cpp_unreal blueprint syntax smoke", "c++").is_some() {
        let shim_dir = temp_dir.path().join("unreal_shim");
        write_cpp_unreal_core_minimal_shim(&shim_dir);
        fs::write(out_dir.join("reference_game_config.generated.h"), "")
            .expect("generated include shim should be written");
        let output = StdCommand::new("c++")
            .arg("-std=c++17")
            .arg("-fsyntax-only")
            .arg(out_dir.join("reference_game_config.cpp"))
            .arg("-I")
            .arg(&out_dir)
            .arg("-I")
            .arg(&shim_dir)
            .output()
            .expect("cpp_unreal blueprint syntax smoke should run");
        assert_command_success(output, "cpp_unreal blueprint syntax smoke");
    }
}

#[test]
fn code_only_target_ignores_missing_project_input_and_sources() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let missing_input_dir = temp_dir.path().join("missing_data_root");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                input = "{}"

                [[targets]]
                name = "code_only"

                [[targets.exports]]
                type = "rust"
                output = "{}"
            "#,
            standard_core_meta_imports(),
            toml_path(&missing_input_dir),
            toml_path(&out_dir)
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config-forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    assert!(!missing_input_dir.exists());
    let rust_code = fs::read_to_string(out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub struct ItemsRow"));
}
