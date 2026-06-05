// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ConfigForgeError, Result};

use super::{
    CSharpCodegenOptions, validate_csharp_identifier, validate_csharp_namespace, validation_error,
};
use crate::codegen::{
    CodeExportInput, CodeField, CodeFragment, CodeFragmentPayload, CodeIndex, CodeTable,
    CodeTypeRef, KeywordConflictPolicy, NamingConfig, naming,
};

pub(super) struct CSharpNames {
    namespace: String,
    access_modifier: String,
    game_data_class_name: String,
    lazy_game_data_class_name: String,
    naming: NamingConfig,
    keyword_conflict: KeywordConflictPolicy,
    nullable: bool,
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
            naming: input.target.options.naming.clone(),
            keyword_conflict: input.target.options.keyword_conflict,
            nullable: options.nullable,
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

    pub(super) fn property_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.naming.field)
    }

    pub(super) fn local_name(&self, name: &str) -> Result<String> {
        self.identifier(name, crate::codegen::CaseStyle::CamelCase)
    }

    pub(super) fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        Ok(format!("_{}Index", self.local_name(&index.name)?))
    }

    pub(super) fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.naming.function)
    }

    pub(super) fn ordered_lower_bound_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("lower_bound_by_{suffix}"), self.naming.function)
    }

    pub(super) fn ordered_upper_bound_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("upper_bound_by_{suffix}"), self.naming.function)
    }

    pub(super) fn ordered_range_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("range_by_{suffix}"), self.naming.function)
    }

    pub(super) fn compound_key_type_name(
        &self,
        table_name: &str,
        index: &CodeIndex,
    ) -> Result<String> {
        self.type_name(&format!("{table_name}_{}_key", index.name))
    }

    pub(super) fn index_key_type(&self, table_name: &str, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.csharp_type(&index.fields[0].code_type)
        } else {
            self.compound_key_type_name(table_name, index)
        }
    }

    pub(super) fn index_key_from_row(
        &self,
        table_name: &str,
        index: &CodeIndex,
        row_var: &str,
    ) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(format!(
                "{row_var}.{}",
                self.property_name(&index.fields[0].name)?
            ))
        } else {
            Ok(format!(
                "new {}({})",
                self.compound_key_type_name(table_name, index)?,
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("{row_var}.{}", self.property_name(&field.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    pub(super) fn index_parameters(&self, index: &CodeIndex) -> Result<String> {
        self.index_parameters_with_suffix(index, "")
    }

    pub(super) fn index_parameters_with_suffix(
        &self,
        index: &CodeIndex,
        suffix: &str,
    ) -> Result<String> {
        index
            .fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "{} {}",
                    self.csharp_type(&field.code_type)?,
                    suffixed_name(self.local_name(&field.name)?, suffix)
                ))
            })
            .collect::<Result<Vec<_>>>()
            .map(|parts| parts.join(", "))
    }

    pub(super) fn index_key_from_parameters(
        &self,
        table_name: &str,
        index: &CodeIndex,
    ) -> Result<String> {
        self.index_key_from_parameter_suffix(table_name, index, "")
    }

    pub(super) fn index_key_from_parameter_suffix(
        &self,
        table_name: &str,
        index: &CodeIndex,
        suffix: &str,
    ) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(suffixed_name(
                self.local_name(&index.fields[0].name)?,
                suffix,
            ))
        } else {
            Ok(format!(
                "new {}({})",
                self.compound_key_type_name(table_name, index)?,
                index
                    .fields
                    .iter()
                    .map(|field| Ok(suffixed_name(self.local_name(&field.name)?, suffix)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    pub(super) fn csharp_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "long".to_owned(),
            CodeTypeRef::Float => "double".to_owned(),
            CodeTypeRef::Bool => "bool".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => "string".to_owned(),
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
            CodeTypeRef::List(element) => format!("{}[]", self.csharp_type(element)?),
        })
    }

    pub(super) fn csharp_field_type(&self, field: &CodeField) -> Result<String> {
        let base_type = self.csharp_type(&field.code_type)?;
        if field.required {
            Ok(base_type)
        } else if self.is_value_type(&field.code_type) || self.nullable {
            Ok(format!("{base_type}?"))
        } else {
            Ok(base_type)
        }
    }

    pub(super) fn is_value_type(&self, code_type: &CodeTypeRef) -> bool {
        matches!(
            code_type,
            CodeTypeRef::Int | CodeTypeRef::Float | CodeTypeRef::Bool | CodeTypeRef::Enum { .. }
        )
    }

    pub(super) fn read_value_function(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "ConfigForgeRuntimeReader.ReadIntValue".to_owned(),
            CodeTypeRef::Float => "ConfigForgeRuntimeReader.ReadFloatValue".to_owned(),
            CodeTypeRef::Bool => "ConfigForgeRuntimeReader.ReadBoolValue".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                "ConfigForgeRuntimeReader.ReadStringValue".to_owned()
            }
            CodeTypeRef::Enum { name } => format!(
                "ConfigForgeRuntimeReader.Read{}Value",
                self.type_name(name)?
            ),
            CodeTypeRef::Struct { name } => format!("{}Reader.Read", self.type_name(name)?),
            CodeTypeRef::List(element) => format!(
                "reader => ConfigForgeRuntimeReader.ReadListValue(reader, {})",
                self.read_value_function(element)?
            ),
        })
    }

    pub(super) fn read_field_expression(&self, field: &CodeField) -> Result<String> {
        let helper = if field.required {
            "ReadRequiredField"
        } else if self.is_value_type(&field.code_type) {
            "ReadOptionalStructField"
        } else {
            "ReadOptionalClassField"
        };
        Ok(format!(
            "ConfigForgeRuntimeReader.{helper}(reader, {:?}, {})",
            field.name,
            self.read_value_function(&field.code_type)?
        ))
    }

    pub(super) fn field_initializer(&self, field: &CodeField) -> Result<String> {
        if !field.required {
            return Ok(String::new());
        }
        match &field.code_type {
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                Ok(" = string.Empty;".to_owned())
            }
            CodeTypeRef::Struct { .. } => Ok(" = new();".to_owned()),
            CodeTypeRef::List(element) => {
                Ok(format!(" = Array.Empty<{}>();", self.csharp_type(element)?))
            }
            _ => Ok(String::new()),
        }
    }

    fn identifier(&self, name: &str, style: crate::codegen::CaseStyle) -> Result<String> {
        naming::make_identifier(
            name,
            style,
            self.keyword_conflict,
            super::symbols::is_csharp_keyword,
            |identifier| Some(format!("@{identifier}")),
        )
        .map_err(|message| ConfigForgeError::Validation(format!("csharp codegen: {message}")))
    }
}

fn suffixed_name(name: String, suffix: &str) -> String {
    if suffix.is_empty() {
        name
    } else {
        format!("{name}{suffix}")
    }
}
