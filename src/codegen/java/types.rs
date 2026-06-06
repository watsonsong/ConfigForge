// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Result;

use super::{
    JavaCodegenOptions, symbols::validate_java_identifier, symbols::validate_java_package,
};
use crate::codegen::CodeExportInput;

pub(super) struct JavaNames {
    package: String,
    root_access_modifier: String,
    root_class_name: String,
    game_data_class_name: String,
    lazy_game_data_class_name: String,
}

impl JavaNames {
    pub(super) fn new(input: &CodeExportInput<'_>, options: &JavaCodegenOptions) -> Result<Self> {
        let package = options
            .package
            .as_deref()
            .or(input.target.namespace.as_deref())
            .unwrap_or(input.target.module_name.as_str())
            .to_owned();
        validate_java_package(&package)?;

        let root_access_modifier = options.access_modifier.trim().to_owned();
        if !matches!(root_access_modifier.as_str(), "public" | "package" | "") {
            return Err(super::validation_error(format!(
                "invalid access_modifier `{root_access_modifier}`; expected `public` or `package`"
            )));
        }
        let root_access_modifier = if root_access_modifier == "package" {
            String::new()
        } else {
            root_access_modifier
        };

        let root_class_name = Self::resolve_root_class_name(input, options)?;
        validate_java_identifier("root_class_name", &root_class_name)?;
        let game_data_class_name = options
            .game_data_class_name
            .as_deref()
            .unwrap_or("GameData")
            .to_owned();
        validate_java_identifier("game_data_class_name", &game_data_class_name)?;
        let lazy_game_data_class_name = options
            .lazy_class_name
            .as_deref()
            .unwrap_or("LazyGameData")
            .to_owned();
        validate_java_identifier("lazy_class_name", &lazy_game_data_class_name)?;

        Ok(Self {
            package,
            root_access_modifier,
            root_class_name,
            game_data_class_name,
            lazy_game_data_class_name,
        })
    }

    pub(super) fn resolve_root_class_name(
        input: &CodeExportInput<'_>,
        options: &JavaCodegenOptions,
    ) -> Result<String> {
        let root_class_name = options
            .root_class_name
            .clone()
            .unwrap_or_else(|| input.target.file_stem("GeneratedGameConfig"));
        validate_java_identifier("root_class_name", &root_class_name)?;
        Ok(root_class_name)
    }

    pub(super) fn package(&self) -> &str {
        &self.package
    }

    pub(super) fn root_access_modifier(&self) -> &str {
        &self.root_access_modifier
    }

    pub(super) fn root_class_name(&self) -> &str {
        &self.root_class_name
    }

    #[allow(dead_code)]
    pub(super) fn game_data_class_name(&self) -> &str {
        &self.game_data_class_name
    }

    #[allow(dead_code)]
    pub(super) fn lazy_game_data_class_name(&self) -> &str {
        &self.lazy_game_data_class_name
    }
}
