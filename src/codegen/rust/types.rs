// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    codegen::{
        CaseStyle, CodeField, CodeFragment, CodeFragmentPayload, CodeIndex, CodeTable,
        CodeTableKind, CodeTypeRef, CodegenOptions, naming,
    },
};

pub(super) struct RustNames<'a> {
    options: &'a CodegenOptions,
}

impl<'a> RustNames<'a> {
    pub(super) fn new(options: &'a CodegenOptions) -> Self {
        Self { options }
    }

    pub(super) fn type_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.type_name)
    }

    pub(super) fn row_type_name(&self, table_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_row"))
    }

    pub(super) fn repository_type_name(&self, table_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_table"))
    }

    pub(super) fn table_data_type_name(&self, table: &CodeTable) -> Result<String> {
        match table.kind {
            CodeTableKind::Object => self.row_type_name(&table.name),
            CodeTableKind::Collection => self.repository_type_name(&table.name),
        }
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
        self.fragment_payload_type_name_for_type_name(&table.name, fragment)
    }

    pub(super) fn fragment_payload_type_name_for_type_name(
        &self,
        table_name: &str,
        fragment: &CodeFragment,
    ) -> Result<String> {
        match &fragment.payload {
            CodeFragmentPayload::InlineFields => {
                self.fragment_type_name(table_name, &fragment.name)
            }
            CodeFragmentPayload::Struct { name } => self.type_name(name),
        }
    }

    pub(super) fn enum_variant_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.enum_value)
    }

    pub(super) fn field_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.field)
    }

    pub(super) fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        self.field_name(&index.name)
    }

    pub(super) fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.options.naming.function)
    }

    pub(super) fn rust_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "i64".to_owned(),
            CodeTypeRef::Float => "f64".to_owned(),
            CodeTypeRef::Bool => "bool".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => "String".to_owned(),
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
            CodeTypeRef::List(element) => format!("Vec<{}>", self.rust_type(element)?),
        })
    }

    pub(super) fn rust_field_type(&self, field: &CodeField) -> Result<String> {
        let base_type = self.rust_type(&field.code_type)?;
        if field.required {
            Ok(base_type)
        } else {
            Ok(format!("Option<{base_type}>"))
        }
    }

    pub(super) fn rust_read_field_expr(&self, field: &CodeField) -> Result<String> {
        let source_name = format!("{:?}", field.name);
        if field.required {
            return Ok(match &field.code_type {
                CodeTypeRef::Int => format!("read_required_i64(reader, {source_name})?"),
                CodeTypeRef::Float => format!("read_required_f64(reader, {source_name})?"),
                CodeTypeRef::Bool => format!("read_required_bool(reader, {source_name})?"),
                CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                    format!("read_required_string(reader, {source_name})?")
                }
                CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                    let type_name = self.type_name(name)?;
                    format!(
                        "with_required_field(reader, {source_name}, {type_name}::read_from_reader)?"
                    )
                }
                CodeTypeRef::List(element) => {
                    let read_element = self.rust_read_current_expr(element)?;
                    format!("read_required_list(reader, {source_name}, |value| {read_element})?")
                }
            });
        }

        Ok(match &field.code_type {
            CodeTypeRef::Int => format!("read_optional_i64(reader, {source_name})?"),
            CodeTypeRef::Float => format!("read_optional_f64(reader, {source_name})?"),
            CodeTypeRef::Bool => format!("read_optional_bool(reader, {source_name})?"),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                format!("read_optional_string(reader, {source_name})?")
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                let type_name = self.type_name(name)?;
                format!(
                    "with_optional_field(reader, {source_name}, {type_name}::read_from_reader)?"
                )
            }
            CodeTypeRef::List(element) => {
                let read_element = self.rust_read_current_expr(element)?;
                format!("read_optional_list(reader, {source_name}, |value| {read_element})?")
            }
        })
    }

    fn rust_read_current_expr(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "value.as_i64()".to_owned(),
            CodeTypeRef::Float => "value.as_f64()".to_owned(),
            CodeTypeRef::Bool => "value.as_bool()".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                "value.as_string()".to_owned()
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                format!("{}::read_from_reader(value)", self.type_name(name)?)
            }
            CodeTypeRef::List(element) => {
                let nested = self.rust_read_current_expr(element)?;
                format!("read_list(value, |value| {nested})")
            }
        })
    }

    pub(super) fn rust_index_storage(&self, index: &CodeIndex) -> Result<String> {
        let key_type = self.rust_key_type(index)?;
        if index.unique {
            Ok(format!("HashMap<{key_type}, usize>"))
        } else {
            Ok(format!("HashMap<{key_type}, Vec<usize>>"))
        }
    }

    pub(super) fn rust_key_type(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.rust_field_type(&index.fields[0])
        } else {
            Ok(format!(
                "({})",
                index
                    .fields
                    .iter()
                    .map(|field| self.rust_field_type(field))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    pub(super) fn rust_key_expr(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(format!(
                "row.{}.clone()",
                self.field_name(&index.fields[0].name)?
            ))
        } else {
            Ok(format!(
                "({})",
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("row.{}.clone()", self.field_name(&field.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    pub(super) fn rust_lookup_key(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.field_name(&index.fields[0].name)
        } else {
            Ok(format!(
                "({})",
                index
                    .fields
                    .iter()
                    .map(|field| self.field_name(&field.name))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    pub(super) fn identifier(&self, value: &str, style: CaseStyle) -> Result<String> {
        naming::make_identifier(
            value,
            style,
            self.options.keyword_conflict,
            naming::is_rust_keyword,
            |identifier| Some(format!("r#{identifier}")),
        )
        .map_err(|message| ConfigForgeError::Validation(format!("rust codegen: {message}")))
    }
}
