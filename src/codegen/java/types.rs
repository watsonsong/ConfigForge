// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Result;

use super::{
    JavaCodegenOptions,
    symbols::{is_java_keyword, validate_java_identifier, validate_java_package},
};
use crate::codegen::{
    CaseStyle, CodeExportInput, CodeField, CodeFragment, CodeFragmentPayload, CodeTable,
    CodeTypeRef, KeywordConflictPolicy, NamingConfig, naming,
};

pub(super) struct JavaNames {
    package: String,
    root_access_modifier: String,
    root_class_name: String,
    game_data_class_name: String,
    lazy_game_data_class_name: String,
    naming: NamingConfig,
    keyword_conflict: KeywordConflictPolicy,
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
            naming: input.target.options.naming.clone(),
            keyword_conflict: input.target.options.keyword_conflict,
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

    pub(super) fn type_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.naming.type_name)
    }

    pub(super) fn row_type_name(&self, table_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_row"))
    }

    pub(super) fn repository_type_name(&self, table_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_table"))
    }

    pub(super) fn fragment_type_name(
        &self,
        table_name: &str,
        fragment_name: &str,
    ) -> Result<String> {
        self.type_name(&format!("{table_name}_{fragment_name}_fragment"))
    }

    pub(super) fn fragment_payload_type_name(
        &self,
        table: &CodeTable,
        fragment: &CodeFragment,
    ) -> Result<String> {
        match &fragment.payload {
            CodeFragmentPayload::InlineFields => {
                self.fragment_type_name(&table.name, &fragment.name)
            }
            CodeFragmentPayload::Struct { name } => self.type_name(name),
        }
    }

    pub(super) fn enum_value_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.naming.enum_value)
    }

    pub(super) fn field_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.naming.field)
    }

    pub(super) fn getter_name(&self, name: &str) -> Result<String> {
        let field = self.field_name(name)?;
        let mut chars = field.chars();
        let suffix = match chars.next() {
            Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
            None => "Value".to_owned(),
        };
        let getter = format!("get{suffix}");
        validate_java_identifier("getter", &getter)?;
        Ok(getter)
    }

    pub(super) fn java_type(&self, field: &CodeField) -> Result<String> {
        self.java_type_ref(&field.code_type, field.required)
    }

    pub(super) fn java_type_ref(&self, code_type: &CodeTypeRef, required: bool) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int if required => "long".to_owned(),
            CodeTypeRef::Int => "Long".to_owned(),
            CodeTypeRef::Float if required => "double".to_owned(),
            CodeTypeRef::Float => "Double".to_owned(),
            CodeTypeRef::Bool if required => "boolean".to_owned(),
            CodeTypeRef::Bool => "Boolean".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => "String".to_owned(),
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
            CodeTypeRef::List(element) => format!("{}[]", self.java_array_element_type(element)?),
        })
    }

    fn java_array_element_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "long".to_owned(),
            CodeTypeRef::Float => "double".to_owned(),
            CodeTypeRef::Bool => "boolean".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => "String".to_owned(),
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
            CodeTypeRef::List(element) => format!("{}[]", self.java_array_element_type(element)?),
        })
    }

    fn identifier(&self, name: &str, style: CaseStyle) -> Result<String> {
        naming::make_identifier(name, style, self.keyword_conflict, is_java_keyword, |_| {
            None
        })
        .map_err(|message| super::validation_error(format!("java codegen: {message}")))
    }
}
