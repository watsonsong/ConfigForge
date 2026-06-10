// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    code_artifact::{
        CaseStyle, CodeArtifactOptions, CodeWriterConfig, I18nMode, I18nOptions,
        KeywordConflictPolicy, NamingConfig,
    },
    data_artifact::TargetDataConfig,
    diagnostic::code::CF_GENERATOR_INVALID_OPTION,
};

use super::FORMAT;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(super) struct LuaTableOptions {
    pub(super) file_header: Option<String>,
    pub(super) module_name: String,
    pub(super) root_file: String,
    pub(super) table_file_naming: NamingMode,
    pub(super) row_layout: RowLayout,
    pub(super) object_layout: ObjectLayout,
    pub(super) index_key: IndexKeyMode,
    pub(super) lazy_require: bool,
    pub(super) emit_schema: bool,
    pub(super) readonly: bool,
    pub(super) i18n: I18nOptions,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum NamingMode {
    SnakeCase,
    Preserve,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum RowLayout {
    ArrayWithMetatable,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ObjectLayout {
    KeyedTable,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum IndexKeyMode {
    NestedTable,
}

impl Default for LuaTableOptions {
    fn default() -> Self {
        Self {
            file_header: None,
            module_name: "config".to_owned(),
            root_file: "init.lua".to_owned(),
            table_file_naming: NamingMode::SnakeCase,
            row_layout: RowLayout::ArrayWithMetatable,
            object_layout: ObjectLayout::KeyedTable,
            index_key: IndexKeyMode::NestedTable,
            lazy_require: true,
            emit_schema: false,
            readonly: true,
            i18n: I18nOptions::default(),
        }
    }
}

impl LuaTableOptions {
    pub(super) fn code_artifact_options(&self) -> CodeArtifactOptions {
        CodeArtifactOptions {
            file_header: self.file_header.clone(),
            keyword_conflict: KeywordConflictPolicy::SuffixUnderscore,
            i18n: self.i18n.clone(),
            naming: NamingConfig {
                type_name: CaseStyle::Preserve,
                enum_value: CaseStyle::Preserve,
                field: match self.table_file_naming {
                    NamingMode::SnakeCase => CaseStyle::SnakeCase,
                    NamingMode::Preserve => CaseStyle::Preserve,
                },
                function: CaseStyle::SnakeCase,
                constant: CaseStyle::ScreamingSnakeCase,
            },
            formatting: CodeWriterConfig::default(),
            language_options: std::collections::HashMap::new(),
        }
    }
}

pub(super) fn options_from_task(task: &TargetDataConfig) -> Result<LuaTableOptions> {
    task.parse_options("lua", FORMAT)
}

pub(super) fn validate_options(options: &LuaTableOptions) -> Result<()> {
    if options.module_name.trim().is_empty() {
        return Err(lua_option_error(
            "lua_table module_name must not be empty".to_owned(),
            &options.module_name,
            "non-empty Lua module name",
            "Set lua_table.module_name to a non-empty dotted Lua module path.",
        ));
    }
    if options.root_file.trim().is_empty()
        || options.root_file.contains('/')
        || options.root_file.contains('\\')
    {
        return Err(lua_option_error(
            "lua_table root_file must be a file name".to_owned(),
            &options.root_file,
            "single Lua file name",
            "Use a file name such as `init.lua`, not a nested or absolute path.",
        ));
    }
    if options.module_name.split('.').any(str::is_empty) {
        return Err(lua_option_error(
            format!(
                "lua_table module_name `{}` contains an empty segment",
                options.module_name
            ),
            &options.module_name,
            "dotted Lua module name without empty segments",
            "Remove repeated, leading, or trailing dots from module_name.",
        ));
    }
    if let Some(file_header) = &options.file_header
        && file_header.chars().any(|character| character == '\0')
    {
        return Err(lua_option_error(
            "lua_table file_header must not contain NUL bytes".to_owned(),
            "NUL byte in file_header",
            "text without NUL bytes",
            "Remove NUL bytes from the generated file header.",
        ));
    }
    if matches!(options.i18n.mode, I18nMode::GlobalFunction) {
        let Some(function) = &options.i18n.function else {
            return Err(lua_option_error(
                "lua_table i18n global_function mode requires function".to_owned(),
                "missing i18n.function",
                "global function name",
                "Set lua_table.i18n.function when using global_function mode.",
            ));
        };
        if function.trim().is_empty() || function.chars().any(char::is_control) {
            return Err(lua_option_error(
                format!(
                    "invalid lua_table i18n function `{function}`; value must be non-empty and single-line"
                ),
                function,
                "non-empty single-line Lua function expression",
                "Remove control characters and provide a valid Lua function reference.",
            ));
        }
    }
    Ok(())
}

fn lua_option_error(
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_INVALID_OPTION,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source("generator.lua_table")
        .with_domain("generator")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
}
