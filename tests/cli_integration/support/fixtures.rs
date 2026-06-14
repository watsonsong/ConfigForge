// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::PathBuf};

pub(crate) struct TempStandardProjectConfig {
    pub(crate) fixture_root: PathBuf,
    pub(crate) output_root: PathBuf,
    pub(crate) project_path: PathBuf,
}

pub(crate) fn absolute_fixture_path(relative_path: &str) -> String {
    std::env::current_dir()
        .expect("current dir")
        .join("tests/fixtures/standard_project")
        .join(relative_path)
        .display()
        .to_string()
}

pub(crate) fn write_temp_standard_project_config(
    temp_dir: &std::path::Path,
) -> TempStandardProjectConfig {
    let fixture_root = std::path::Path::new(&absolute_fixture_path("")).to_path_buf();
    let output_root = temp_dir.join("out");
    let project_path = temp_dir.join("ConfigForge.toml");
    let original_config = fs::read_to_string(fixture_root.join("ConfigForge.toml"))
        .expect("standard fixture project config should be readable");
    let config = original_config
        .replace(
            r#"imports = [
    "meta/common.toml",
    "meta/items.toml",
    "meta/characters.toml",
    "meta/gameplay.toml",
    "meta/misc.toml",
]"#,
            &format!("imports = [{}]", standard_reference_game_meta_imports()),
        )
        .replace(
            r#"input = "data""#,
            &format!(r#"input = "{}""#, fixture_root.join("data").display()),
        )
        .replace(
            r#"output = "out/"#,
            &format!(r#"output = "{}/"#, output_root.display()),
        );
    fs::write(&project_path, config).expect("temporary standard project config should be written");
    TempStandardProjectConfig {
        fixture_root,
        output_root,
        project_path,
    }
}

pub(crate) fn standard_core_meta_imports() -> String {
    [
        "meta/common.toml",
        "meta/items.toml",
        "meta/characters.toml",
        "meta/misc.toml",
    ]
    .into_iter()
    .map(|path| format!("\"{}\"", absolute_fixture_path(path)))
    .collect::<Vec<_>>()
    .join(", ")
}
pub(crate) fn standard_reference_game_meta_imports() -> String {
    [
        "meta/common.toml",
        "meta/items.toml",
        "meta/characters.toml",
        "meta/gameplay.toml",
        "meta/misc.toml",
    ]
    .into_iter()
    .map(|path| format!("\"{}\"", absolute_fixture_path(path)))
    .collect::<Vec<_>>()
    .join(", ")
}
