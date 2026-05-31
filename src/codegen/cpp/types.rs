// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    codegen::{
        CaseStyle, CodeField, CodeFragment, CodeFragmentPayload, CodeIndex, CodeTable,
        CodeTableKind, CodeTypeRef, CodegenOptions, ir::CodeForeignKey, naming,
    },
};

pub(super) struct CppNames<'a> {
    options: &'a CodegenOptions,
}

impl<'a> CppNames<'a> {
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

    pub(super) fn text_function_name(&self, field_name: &str) -> Result<String> {
        self.identifier(&format!("{field_name}_text"), self.options.naming.function)
    }

    pub(super) fn member_name(&self, name: &str) -> Result<String> {
        Ok(format!("{}_", self.field_name(name)?))
    }

    pub(super) fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        Ok(format!("{}_", self.field_name(&index.name)?))
    }

    pub(super) fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.options.naming.function)
    }

    pub(super) fn relation_function_name(&self, foreign_key: &CodeForeignKey) -> Result<String> {
        let name = foreign_key
            .linked_field
            .as_ref()
            .map(|linked_field| linked_field.name.as_str())
            .unwrap_or(foreign_key.name.as_str());
        self.identifier(&format!("get_{name}"), self.options.naming.function)
    }

    pub(super) fn relation_cache_member_name(
        &self,
        foreign_key: &CodeForeignKey,
    ) -> Result<String> {
        Ok(format!(
            "{}{}",
            self.field_name(
                foreign_key
                    .linked_field
                    .as_ref()
                    .map(|linked_field| linked_field.name.as_str())
                    .unwrap_or(foreign_key.name.as_str())
            )?,
            "_link"
        ))
    }

    pub(super) fn reader_function_name(&self, type_name: &str) -> Result<String> {
        self.identifier(&format!("read_{type_name}"), self.options.naming.function)
    }

    pub(super) fn cpp_index_params(&self, index: &CodeIndex) -> Result<String> {
        index
            .fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "{} {}",
                    self.cpp_field_type(field)?,
                    self.field_name(&field.name)?
                ))
            })
            .collect::<Result<Vec<_>>>()
            .map(|params| params.join(", "))
    }

    pub(super) fn cpp_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "std::int64_t".to_owned(),
            CodeTypeRef::Float => "double".to_owned(),
            CodeTypeRef::Bool => "bool".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                "std::string".to_owned()
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
            CodeTypeRef::List(element) => format!("std::vector<{}>", self.cpp_type(element)?),
        })
    }

    pub(super) fn cpp_field_type(&self, field: &CodeField) -> Result<String> {
        let base_type = self.cpp_type(&field.code_type)?;
        if field.required {
            Ok(base_type)
        } else {
            Ok(format!("std::optional<{base_type}>"))
        }
    }

    pub(super) fn cpp_read_field_expr(&self, field: &CodeField) -> Result<String> {
        let source_name = format!("{:?}", field.name);
        if field.required {
            return Ok(match &field.code_type {
                CodeTypeRef::Int => format!("ReadRequiredInt64(reader, {source_name})"),
                CodeTypeRef::Float => format!("ReadRequiredDouble(reader, {source_name})"),
                CodeTypeRef::Bool => format!("ReadRequiredBool(reader, {source_name})"),
                CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                    format!("ReadRequiredString(reader, {source_name})")
                }
                CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                    let type_name = self.type_name(name)?;
                    let reader_function = self.reader_function_name(&type_name)?;
                    format!(
                        concat!(
                            "RequiredField(reader, {source_name}, [](IRuntimeDataReader& value) {{\n",
                            "            return {reader_function}(value);\n",
                            "        }})",
                        ),
                        reader_function = reader_function,
                        source_name = source_name,
                    )
                }
                CodeTypeRef::List(element) => {
                    let read_element = self.cpp_read_current_expr(element)?;
                    format!(
                        concat!(
                            "ReadRequiredList(reader, {source_name}, [](IRuntimeDataReader& value) {{\n",
                            "            return {read_element};\n",
                            "        }})",
                        ),
                        read_element = read_element,
                        source_name = source_name,
                    )
                }
            });
        }

        Ok(match &field.code_type {
            CodeTypeRef::Int => format!("ReadOptionalInt64(reader, {source_name})"),
            CodeTypeRef::Float => format!("ReadOptionalDouble(reader, {source_name})"),
            CodeTypeRef::Bool => format!("ReadOptionalBool(reader, {source_name})"),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                format!("ReadOptionalString(reader, {source_name})")
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                let type_name = self.type_name(name)?;
                let reader_function = self.reader_function_name(&type_name)?;
                format!(
                    concat!(
                        "OptionalField(reader, {source_name}, [](IRuntimeDataReader& value) {{\n",
                        "            return {reader_function}(value);\n",
                        "        }})",
                    ),
                    reader_function = reader_function,
                    source_name = source_name,
                )
            }
            CodeTypeRef::List(element) => {
                let read_element = self.cpp_read_current_expr(element)?;
                format!(
                    concat!(
                        "ReadOptionalList(reader, {source_name}, [](IRuntimeDataReader& value) {{\n",
                        "            return {read_element};\n",
                        "        }})",
                    ),
                    read_element = read_element,
                    source_name = source_name,
                )
            }
        })
    }

    fn cpp_read_current_expr(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "value.AsInt64()".to_owned(),
            CodeTypeRef::Float => "value.AsDouble()".to_owned(),
            CodeTypeRef::Bool => "value.AsBool()".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                "value.AsString()".to_owned()
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                format!(
                    "{}(value)",
                    self.reader_function_name(&self.type_name(name)?)?
                )
            }
            CodeTypeRef::List(element) => {
                let nested = self.cpp_read_current_expr(element)?;
                format!(
                    concat!(
                        "ReadList(value, [](IRuntimeDataReader& value) {{\n",
                        "                return {nested};\n",
                        "            }})",
                    ),
                    nested = nested,
                )
            }
        })
    }

    pub(super) fn cpp_index_storage(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        let key = self.cpp_key_type(table, index)?;
        let map_type = if index.fields.len() == 1 {
            "std::unordered_map"
        } else {
            "std::map"
        };
        if index.unique {
            Ok(format!("{map_type}<{key}, std::size_t>"))
        } else {
            Ok(format!("{map_type}<{key}, std::vector<std::size_t>>"))
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
                self.field_name(&index.fields[0].name)?
            ))
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("{}.{}", row_expr, self.field_name(&field.name)?))),
            )
        }
    }

    pub(super) fn cpp_lookup_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
    ) -> Result<String> {
        if index.fields.len() == 1 {
            self.field_name(&index.fields[0].name)
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index
                    .fields
                    .iter()
                    .map(|field| self.field_name(&field.name)),
            )
        }
    }

    pub(super) fn cpp_compound_key_expr(
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
        self.type_name(&format!("{}_{}_key", table.name, index.name))
    }

    pub(super) fn identifier(&self, value: &str, style: CaseStyle) -> Result<String> {
        naming::make_identifier(
            value,
            style,
            self.options.keyword_conflict,
            naming::is_cpp_keyword,
            |_| None,
        )
        .map_err(|message| ConfigForgeError::Validation(format!("cpp codegen: {message}")))
    }
}
