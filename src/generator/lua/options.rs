// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

use crate::{
    ConfigForgeError, Result,
    code_artifact::{
        CaseStyle, CodeArtifactOptions, CodeWriterConfig, I18nMode, I18nOptions,
        KeywordConflictPolicy, NamingConfig,
    },
    data_artifact::TargetDataConfig,
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
        return Err(ConfigForgeError::Validation(
            "lua_table module_name must not be empty".to_owned(),
        ));
    }
    if options.root_file.trim().is_empty()
        || options.root_file.contains('/')
        || options.root_file.contains('\\')
    {
        return Err(ConfigForgeError::Validation(
            "lua_table root_file must be a file name".to_owned(),
        ));
    }
    if options.module_name.split('.').any(str::is_empty) {
        return Err(ConfigForgeError::Validation(format!(
            "lua_table module_name `{}` contains an empty segment",
            options.module_name
        )));
    }
    if let Some(file_header) = &options.file_header
        && file_header.chars().any(|character| character == '\0')
    {
        return Err(ConfigForgeError::Validation(
            "lua_table file_header must not contain NUL bytes".to_owned(),
        ));
    }
    if matches!(options.i18n.mode, I18nMode::GlobalFunction) {
        let Some(function) = &options.i18n.function else {
            return Err(ConfigForgeError::Validation(
                "lua_table i18n global_function mode requires function".to_owned(),
            ));
        };
        if function.trim().is_empty() || function.chars().any(char::is_control) {
            return Err(ConfigForgeError::Validation(format!(
                "invalid lua_table i18n function `{function}`; value must be non-empty and single-line"
            )));
        }
    }
    Ok(())
}
