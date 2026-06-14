// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    code_artifact::{
        CaseStyle, CodeField, CodeFragment, CodeFragmentPayload, CodeIndex, CodeTable,
        CodeTableKind, CodeTypeRef, CodeVectorElementType, CodegenOptions, ir::CodeForeignKey,
        naming,
    },
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
};

pub(super) struct TypeScriptNames<'a> {
    options: &'a CodegenOptions,
}

impl<'a> TypeScriptNames<'a> {
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

    pub(super) fn enum_value_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.enum_value)
    }

    pub(super) fn field_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.field)
    }

    pub(super) fn text_function_name(&self, owner_name: &str, field_name: &str) -> Result<String> {
        self.identifier(
            &format!("{owner_name}_{field_name}_text"),
            self.options.naming.function,
        )
    }

    pub(super) fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        self.identifier(
            &format!("{}_index", self.index_function_name(index)?),
            self.options.naming.field,
        )
    }

    pub(super) fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.options.naming.function)
    }

    pub(super) fn index_of_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(
            &format!("index_of_by_{suffix}"),
            self.options.naming.function,
        )
    }

    pub(super) fn relation_function_name(&self, foreign_key: &CodeForeignKey) -> Result<String> {
        let name = foreign_key
            .linked_field
            .as_ref()
            .map(|linked_field| linked_field.name.as_str())
            .unwrap_or(foreign_key.name.as_str());
        self.identifier(name, self.options.naming.function)
    }

    pub(super) fn relation_cache_field_name(&self, foreign_key: &CodeForeignKey) -> Result<String> {
        let name = foreign_key
            .linked_field
            .as_ref()
            .map(|linked_field| linked_field.name.as_str())
            .unwrap_or(foreign_key.name.as_str());
        self.field_name(&format!("{name}_link"))
    }

    pub(super) fn ts_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int | CodeTypeRef::Float => "number".to_owned(),
            CodeTypeRef::Bool => "boolean".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => "string".to_owned(),
            CodeTypeRef::Color => "ConfigForgeColor".to_owned(),
            CodeTypeRef::Vector {
                dimension, element, ..
            } => ts_vector_type(*dimension, *element),
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
            CodeTypeRef::List(element) => format!("{}[]", self.ts_type(element)?),
        })
    }

    pub(super) fn ts_field_type(&self, field: &CodeField) -> Result<String> {
        let base_type = self.ts_type(&field.code_type)?;
        if field.required {
            Ok(base_type)
        } else {
            Ok(format!("{base_type} | undefined"))
        }
    }

    pub(super) fn ts_optional_property_suffix(&self, field: &CodeField) -> &'static str {
        if field.required { "" } else { "?" }
    }

    pub(super) fn reader_function_name(&self, type_name: &str) -> Result<String> {
        if self.options.naming.function == CaseStyle::CamelCase {
            return naming::resolve_keyword_conflict(
                format!("read{type_name}"),
                self.options.keyword_conflict,
                naming::is_ts_keyword,
                |_| None,
            )
            .map_err(|message| {
                crate::code_artifact::generator_option_error(
                    CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
                    "generator.typescript.naming",
                    format!("typescript codegen: {message}"),
                    type_name,
                    "valid TypeScript reader function identifier",
                    "Adjust source names or naming options so TypeScript reader identifiers can be generated.",
                )
            });
        }
        self.identifier(&format!("read_{type_name}"), self.options.naming.function)
    }

    pub(super) fn ts_read_field_expr(&self, field: &CodeField) -> Result<String> {
        let source_name = format!("{:?}", field.name);
        if field.required {
            return Ok(match &field.code_type {
                CodeTypeRef::Int => format!("readRequiredInt64(reader, {source_name})"),
                CodeTypeRef::Float => format!("readRequiredFloat64(reader, {source_name})"),
                CodeTypeRef::Bool => format!("readRequiredBool(reader, {source_name})"),
                CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                    format!("readRequiredString(reader, {source_name})")
                }
                CodeTypeRef::Color => format!("readRequiredColor(reader, {source_name})"),
                CodeTypeRef::Vector {
                    dimension, element, ..
                } => format!(
                    "readRequired{}(reader, {source_name})",
                    ts_vector_type(*dimension, *element)
                ),
                CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                    let type_name = self.type_name(name)?;
                    let reader_function = self.reader_function_name(&type_name)?;
                    format!(
                        "requiredField(reader, {source_name}, (value) => {reader_function}(value))"
                    )
                }
                CodeTypeRef::List(element) => {
                    let read_element = self.ts_read_current_expr(element)?;
                    format!("readRequiredList(reader, {source_name}, (value) => {read_element})")
                }
            });
        }

        Ok(match &field.code_type {
            CodeTypeRef::Int => format!("readOptionalInt64(reader, {source_name})"),
            CodeTypeRef::Float => format!("readOptionalFloat64(reader, {source_name})"),
            CodeTypeRef::Bool => format!("readOptionalBool(reader, {source_name})"),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                format!("readOptionalString(reader, {source_name})")
            }
            CodeTypeRef::Color => format!("readOptionalColor(reader, {source_name})"),
            CodeTypeRef::Vector {
                dimension, element, ..
            } => format!(
                "readOptional{}(reader, {source_name})",
                ts_vector_type(*dimension, *element)
            ),
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                let type_name = self.type_name(name)?;
                let reader_function = self.reader_function_name(&type_name)?;
                format!("optionalField(reader, {source_name}, (value) => {reader_function}(value))")
            }
            CodeTypeRef::List(element) => {
                let read_element = self.ts_read_current_expr(element)?;
                format!("readOptionalList(reader, {source_name}, (value) => {read_element})")
            }
        })
    }

    fn ts_read_current_expr(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "value.asInt64()".to_owned(),
            CodeTypeRef::Float => "value.asFloat64()".to_owned(),
            CodeTypeRef::Bool => "value.asBool()".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                "value.asString()".to_owned()
            }
            CodeTypeRef::Color => "readColor(value)".to_owned(),
            CodeTypeRef::Vector {
                dimension, element, ..
            } => format!("read{}(value)", ts_vector_type(*dimension, *element)),
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                let type_name = self.type_name(name)?;
                let reader_function = self.reader_function_name(&type_name)?;
                format!("{reader_function}(value)")
            }
            CodeTypeRef::List(element) => {
                let nested = self.ts_read_current_expr(element)?;
                format!("readList(value, (value) => {nested})")
            }
        })
    }

    pub(super) fn ts_index_storage(&self, index: &CodeIndex, _table: &CodeTable) -> Result<String> {
        let key_type = if index.fields.len() == 1 {
            self.ts_field_type(&index.fields[0])?
        } else {
            "string".to_owned()
        };
        if index.unique {
            Ok(format!("Map<{key_type}, number>"))
        } else {
            Ok(format!("Map<{key_type}, number[]>"))
        }
    }

    pub(super) fn ts_key_expr(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(format!("row.{}", self.field_name(&index.fields[0].name)?))
        } else {
            Ok(format!(
                "JSON.stringify([{}])",
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("row.{}", self.field_name(&field.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    pub(super) fn ts_lookup_key(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.field_name(&index.fields[0].name)
        } else {
            Ok(format!(
                "JSON.stringify([{}])",
                index
                    .fields
                    .iter()
                    .map(|field| self.field_name(&field.name))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    pub(super) fn ts_relation_lookup_key(
        &self,
        target_index: &CodeIndex,
        values: &[String],
    ) -> Result<String> {
        if target_index.fields.len() == 1 {
            Ok(values.first().cloned().unwrap_or_default())
        } else {
            Ok(format!("JSON.stringify([{}])", values.join(", ")))
        }
    }

    pub(super) fn identifier(&self, value: &str, style: CaseStyle) -> Result<String> {
        naming::make_identifier(
            value,
            style,
            self.options.keyword_conflict,
            naming::is_ts_keyword,
            |_| None,
        )
        .map_err(|message| {
            crate::code_artifact::generator_option_error(
                CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
                "generator.typescript.naming",
                format!("typescript codegen: {message}"),
                value,
                "valid TypeScript generated identifier",
                "Adjust source names or naming options so TypeScript identifiers can be generated.",
            )
        })
    }
}

fn ts_vector_type(dimension: usize, element: CodeVectorElementType) -> String {
    let suffix = match element {
        CodeVectorElementType::Float => "",
        CodeVectorElementType::Int => "Int",
    };
    format!("ConfigForgeVector{dimension}{suffix}")
}
