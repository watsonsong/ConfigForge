// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

pub(crate) fn absolute_fixture_path(relative_path: &str) -> String {
    std::env::current_dir()
        .expect("current dir")
        .join("tests/fixtures/standard_project")
        .join(relative_path)
        .display()
        .to_string()
}
pub(crate) fn standard_core_meta_imports() -> String {
    ["meta/items.toml", "meta/characters.toml", "meta/misc.toml"]
        .into_iter()
        .map(|path| format!("\"{}\"", absolute_fixture_path(path)))
        .collect::<Vec<_>>()
        .join(", ")
}
pub(crate) fn standard_reference_game_meta_imports() -> String {
    ["meta/items.toml", "meta/characters.toml", "meta/misc.toml"]
        .into_iter()
        .map(|path| format!("\"{}\"", absolute_fixture_path(path)))
        .collect::<Vec<_>>()
        .join(", ")
}
