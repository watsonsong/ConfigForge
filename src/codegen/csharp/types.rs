// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Result;

use super::{
    CSharpCodegenOptions, validate_csharp_identifier, validate_csharp_namespace, validation_error,
};
use crate::codegen::CodeExportInput;

pub(super) struct CSharpNames {
    namespace: String,
    access_modifier: String,
    game_data_class_name: String,
    lazy_game_data_class_name: String,
}

impl CSharpNames {
    pub(super) fn new(input: &CodeExportInput<'_>, options: &CSharpCodegenOptions) -> Result<Self> {
        let namespace = options
            .namespace
            .as_deref()
            .or(input.target.namespace.as_deref())
            .unwrap_or(input.target.module_name.as_str())
            .to_owned();
        validate_csharp_namespace(&namespace)?;

        let access_modifier = options.access_modifier.trim().to_owned();
        if !matches!(access_modifier.as_str(), "public" | "internal") {
            return Err(validation_error(format!(
                "invalid access_modifier `{access_modifier}`; expected `public` or `internal`"
            )));
        }

        let game_data_class_name = options
            .class_name
            .as_deref()
            .unwrap_or("GameData")
            .to_owned();
        validate_csharp_identifier("class_name", &game_data_class_name)?;
        let lazy_game_data_class_name = options
            .lazy_class_name
            .as_deref()
            .unwrap_or("LazyGameData")
            .to_owned();
        validate_csharp_identifier("lazy_class_name", &lazy_game_data_class_name)?;

        Ok(Self {
            namespace,
            access_modifier,
            game_data_class_name,
            lazy_game_data_class_name,
        })
    }

    pub(super) fn namespace(&self) -> &str {
        &self.namespace
    }

    pub(super) fn access_modifier(&self) -> &str {
        &self.access_modifier
    }

    pub(super) fn game_data_class_name(&self) -> &str {
        &self.game_data_class_name
    }

    pub(super) fn lazy_game_data_class_name(&self) -> &str {
        &self.lazy_game_data_class_name
    }
}
