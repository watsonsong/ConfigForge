// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::meta::LinkedTableMeta;

use super::options::{LuaTableOptions, NamingMode};

pub(super) fn root_path(options: &LuaTableOptions) -> PathBuf {
    let mut path = module_dir(options);
    path.push(&options.root_file);
    path
}

pub(super) fn table_path(options: &LuaTableOptions, table_meta: &LinkedTableMeta<'_>) -> PathBuf {
    let mut path = module_dir(options);
    path.push(table_file_name(options, table_meta));
    path
}

fn module_dir(options: &LuaTableOptions) -> PathBuf {
    options.module_name.split('.').collect::<PathBuf>()
}

pub(super) fn table_file_name(
    options: &LuaTableOptions,
    table_meta: &LinkedTableMeta<'_>,
) -> String {
    format!("{}.lua", table_module_segment(options, table_meta))
}

pub(super) fn table_require_path(
    options: &LuaTableOptions,
    table_meta: &LinkedTableMeta<'_>,
) -> String {
    format!(
        "{}.{}",
        options.module_name,
        table_module_segment(options, table_meta)
    )
}

fn table_module_segment(options: &LuaTableOptions, table_meta: &LinkedTableMeta<'_>) -> String {
    match options.table_file_naming {
        NamingMode::SnakeCase => snake_case(&table_meta.meta.name),
        NamingMode::Preserve => sanitize_identifier(&table_meta.meta.name),
    }
}

pub(super) fn snake_case(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = true;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if character.is_ascii_uppercase() && !previous_was_separator && !output.ends_with('_') {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            output.push('_');
            previous_was_separator = true;
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        "_".to_owned()
    } else {
        output
    }
}

pub(super) fn sanitize_identifier(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_alphanumeric() || character == '_' {
            if index == 0 && character.is_ascii_digit() {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push('_');
        }
    }
    if output.is_empty() {
        output.push('_');
    }
    if lua_keywords().contains(&output.as_str()) {
        output.push('_');
    }
    output
}

pub(super) fn table_accessor_name(table_meta: &LinkedTableMeta<'_>) -> String {
    sanitize_identifier(&snake_case(&table_meta.meta.name))
}

pub(super) fn text_helper_name(field_name: &str) -> String {
    sanitize_identifier(&format!("{field_name}_text"))
}

fn lua_keywords() -> &'static [&'static str] {
    &[
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if",
        "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
    ]
}
