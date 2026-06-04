// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    codegen::{
        CodeField, CodeIndex, CodeTable, CodeTableKind, CodeTypeRef, CodegenOptions, naming,
    },
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

    pub(super) fn read_row_function_name(&self, table_name: &str) -> Result<String> {
        self.identifier(
            &format!("read_{table_name}_row"),
            self.options.naming.function,
        )
    }

    pub(super) fn read_struct_function_name(&self, struct_name: &str) -> Result<String> {
        self.identifier(&format!("read_{struct_name}"), self.options.naming.function)
    }

    pub(super) fn load_function_name(&self) -> Result<String> {
        self.identifier("load", self.options.naming.function)
    }

    pub(super) fn data_file_constant_name(&self) -> Result<String> {
        self.identifier("data_file", self.options.naming.constant)
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

    pub(super) fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        self.identifier(&index.name, self.options.naming.field)
    }

    pub(super) fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.options.naming.function)
    }

    pub(super) fn lower_bound_index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(
            &format!("lower_bound_by_{suffix}"),
            self.options.naming.function,
        )
    }

    pub(super) fn upper_bound_index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(
            &format!("upper_bound_by_{suffix}"),
            self.options.naming.function,
        )
    }

    pub(super) fn range_index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("range_by_{suffix}"), self.options.naming.function)
    }

    pub(super) fn cpp_index_params(&self, index: &CodeIndex) -> Result<String> {
        index
            .fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "{} {}",
                    self.cpp_field_type(field)?,
                    self.field_name(field)?
                ))
            })
            .collect::<Result<Vec<_>>>()
            .map(|params| params.join(", "))
    }

    pub(super) fn cpp_range_index_params(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
    ) -> Result<String> {
        let key_type = self.cpp_key_type(table, index)?;
        Ok(format!("{key_type} Lower, {key_type} Upper"))
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

    pub(super) fn cpp_index_storage(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        let key_type = self.cpp_key_type(table, index)?;
        if index.ordered {
            return Ok(format!("TArray<TPair<{key_type}, int32>>"));
        }
        if index.unique {
            Ok(format!("TMap<{key_type}, int32>"))
        } else {
            Ok(format!("TMap<{key_type}, TArray<int32>>"))
        }
    }

    pub(super) fn cpp_key_type(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.cpp_field_type(&index.fields[0])
        } else {
            self.compound_index_key_type_name(table, index)
        }
    }

    pub(super) fn cpp_row_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
        row_expr: &str,
    ) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(format!(
                "{}.{}",
                row_expr,
                self.field_name(&index.fields[0])?
            ))
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("{}.{}", row_expr, self.field_name(field)?))),
            )
        }
    }

    pub(super) fn cpp_lookup_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
    ) -> Result<String> {
        if index.fields.len() == 1 {
            self.field_name(&index.fields[0])
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index.fields.iter().map(|field| self.field_name(field)),
            )
        }
    }

    fn cpp_compound_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
        values: impl Iterator<Item = Result<String>>,
    ) -> Result<String> {
        Ok(format!(
            "{}{{{}}}",
            self.compound_index_key_type_name(table, index)?,
            values.collect::<Result<Vec<_>>>()?.join(", ")
        ))
    }

    pub(super) fn compound_index_key_type_name(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
    ) -> Result<String> {
        Ok(format!(
            "F{}{}Key",
            self.type_name(&table.name)?,
            self.type_name(&index.name)?
        ))
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
