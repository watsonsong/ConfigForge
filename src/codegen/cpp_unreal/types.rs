// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    codegen::{CodeField, CodeTable, CodeTableKind, CodeTypeRef, CodegenOptions, naming},
};

pub(super) struct CppUnrealNames<'a> {
    options: &'a CodegenOptions,
}

impl<'a> CppUnrealNames<'a> {
    pub(super) fn new(options: &'a CodegenOptions) -> Self {
        Self { options }
    }

    pub(super) fn type_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.type_name)
    }

    pub(super) fn enum_type_name(&self, name: &str) -> Result<String> {
        Ok(format!("E{}", self.type_name(name)?))
    }

    pub(super) fn struct_type_name(&self, name: &str) -> Result<String> {
        Ok(format!("F{}", self.type_name(name)?))
    }

    pub(super) fn row_type_name(&self, table_name: &str) -> Result<String> {
        Ok(format!("F{}Row", self.type_name(table_name)?))
    }

    pub(super) fn repository_type_name(&self, table_name: &str) -> Result<String> {
        Ok(format!("F{}Table", self.type_name(table_name)?))
    }

    pub(super) fn table_data_type_name(&self, table: &CodeTable) -> Result<String> {
        match table.kind {
            CodeTableKind::Object => self.row_type_name(&table.name),
            CodeTableKind::Collection => self.repository_type_name(&table.name),
        }
    }

    pub(super) fn enum_variant_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.enum_value)
    }

    pub(super) fn field_name(&self, field: &CodeField) -> Result<String> {
        let name = self.identifier(&field.name, self.options.naming.field)?;
        if matches!(field.code_type, CodeTypeRef::Bool) && !name.starts_with('b') {
            Ok(format!("b{name}"))
        } else {
            Ok(name)
        }
    }

    pub(super) fn member_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.field)
    }

    pub(super) fn cpp_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "int64".to_owned(),
            CodeTypeRef::Float => "double".to_owned(),
            CodeTypeRef::Bool => "bool".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => "FString".to_owned(),
            CodeTypeRef::Enum { name } => self.enum_type_name(name)?,
            CodeTypeRef::Struct { name } => self.struct_type_name(name)?,
            CodeTypeRef::List(inner) => format!("TArray<{}>", self.cpp_type(inner)?),
        })
    }

    pub(super) fn cpp_field_type(&self, field: &CodeField) -> Result<String> {
        let inner = self.cpp_type(&field.code_type)?;
        if field.required {
            Ok(inner)
        } else {
            Ok(format!("TOptional<{inner}>"))
        }
    }

    fn identifier(&self, value: &str, style: crate::codegen::CaseStyle) -> Result<String> {
        naming::make_identifier(
            value,
            style,
            self.options.keyword_conflict,
            naming::is_cpp_keyword,
            |_| None,
        )
        .map_err(|message| ConfigForgeError::Validation(format!("cpp_unreal codegen: {message}")))
    }
}
