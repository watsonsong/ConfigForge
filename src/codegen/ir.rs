// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, path::PathBuf};

use crate::{
    ConfigForgeError, Result,
    exporter::{JSON_FORMAT, RuntimeDataFile, binary_schema_revision},
    meta::{LinkedField, LinkedFragmentPayload, LinkedMetaModel, PrimitiveFieldType, TableKind},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeModel {
    pub enums: Vec<CodeEnum>,
    pub structs: Vec<CodeStruct>,
    pub tables: Vec<CodeTable>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeEnum {
    pub name: String,
    pub desc: Option<String>,
    pub values: Vec<CodeEnumValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeEnumValue {
    pub name: String,
    pub desc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeStruct {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeTable {
    pub name: String,
    pub kind: CodeTableKind,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub fragments: Vec<CodeFragment>,
    pub indexes: Vec<CodeIndex>,
    pub foreign_keys: Vec<CodeForeignKey>,
    pub primary_index: Option<String>,
    pub data_file: String,
    pub data_format: String,
    pub schema_revision: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeTableKind {
    Object,
    Collection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeFragment {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub payload: CodeFragmentPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodeFragmentPayload {
    InlineFields,
    Struct { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeField {
    pub name: String,
    pub code_type: CodeTypeRef,
    pub desc: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeIndex {
    pub name: String,
    pub unique: bool,
    pub primary: bool,
    pub fields: Vec<CodeField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeForeignKey {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub target_table: String,
    pub target_index: String,
    pub allow_empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodeTypeRef {
    Int,
    Float,
    Bool,
    String,
    Text,
    Asset,
    Enum { name: String },
    Struct { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub content: String,
}

pub(crate) fn build_code_model(
    meta: &LinkedMetaModel<'_>,
    target_tags: &crate::meta::TargetTags,
    _runtime_data_files: &[RuntimeDataFile],
) -> Result<CodeModel> {
    let enums = meta
        .model
        .enums
        .iter()
        .map(|enum_meta| CodeEnum {
            name: enum_meta.name.clone(),
            desc: enum_meta.desc.clone(),
            values: enum_meta
                .values
                .iter()
                .map(|value| CodeEnumValue {
                    name: value.name.clone(),
                    desc: value.desc.clone(),
                })
                .collect(),
        })
        .collect::<Vec<_>>();

    ensure_model_unique_names(&enums, |item| item.name.as_str(), "enum")?;
    for code_enum in &enums {
        ensure_model_unique_names(
            &code_enum.values,
            |value| value.name.as_str(),
            &format!("enum `{}` values", code_enum.name),
        )?;
    }

    let structs = meta
        .structs
        .iter()
        .map(|struct_meta| {
            let fields = struct_meta
                .fields
                .iter()
                .filter(|field| is_runtime_codegen_field(field, target_tags))
                .map(code_field_from_linked)
                .collect::<Result<Vec<_>>>()?;
            ensure_model_unique_names(
                &fields,
                |field| field.name.as_str(),
                &format!("struct `{}` fields", struct_meta.meta.name),
            )?;
            Ok(CodeStruct {
                name: struct_meta.meta.name.clone(),
                desc: struct_meta.meta.desc.clone(),
                fields,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let tables = meta
        .tables
        .iter()
        .filter(|table_meta| table_meta.meta.is_exported_for(target_tags))
        .map(|table_meta| {
            let fields = table_meta
                .fields
                .iter()
                .filter(|field| is_runtime_codegen_field(field, target_tags))
                .map(code_field_from_linked)
                .collect::<Result<Vec<_>>>()?;
            let kind = CodeTableKind::from_table_kind(table_meta.meta.kind);
            let fragments = table_meta
                .fragments
                .iter()
                .filter(|fragment| fragment.meta.is_exported_for(target_tags))
                .map(|fragment| {
                    let fields = fragment
                        .fields
                        .iter()
                        .filter(|field| is_runtime_codegen_field(field, target_tags))
                        .map(code_field_from_linked)
                        .collect::<Result<Vec<_>>>()?;
                    ensure_model_unique_names(
                        &fields,
                        |field| field.name.as_str(),
                        &format!(
                            "table `{}` fragment `{}` fields",
                            table_meta.meta.name, fragment.meta.name
                        ),
                    )?;
                    Ok(CodeFragment {
                        name: fragment.meta.name.clone(),
                        desc: fragment.meta.desc.clone(),
                        fields,
                        payload: match &fragment.payload {
                            LinkedFragmentPayload::InlineFields => CodeFragmentPayload::InlineFields,
                            LinkedFragmentPayload::Struct(struct_type) => CodeFragmentPayload::Struct {
                                name: struct_type.meta.name.clone(),
                            },
                        },
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            ensure_model_unique_names(
                &fields,
                |field| field.name.as_str(),
                &format!("table `{}` fields", table_meta.meta.name),
            )?;
            ensure_model_unique_names(
                &fragments,
                |fragment| fragment.name.as_str(),
                &format!("table `{}` fragments", table_meta.meta.name),
            )?;
            let field_index = fields
                .iter()
                .map(|field| (field.name.as_str(), field.clone()))
                .collect::<HashMap<_, _>>();
            let mut indexes = Vec::new();
            if kind == CodeTableKind::Collection {
                let primary_index_name = table_meta.primary_index().map(|index| index.meta.name.as_str());
                for index_meta in &table_meta.indexes {
                    if !index_meta.meta.is_exported_for(target_tags) {
                        continue;
                    }
                    let is_primary_index =
                        index_meta.meta.primary || primary_index_name == Some(index_meta.meta.name.as_str());
                    let mut index_fields = Vec::new();
                    let mut missing_exported_field = None;
                    for field in &index_meta.fields {
                        let Some(code_field) = field_index.get(field.meta.name.as_str()).cloned() else {
                            missing_exported_field = Some(field.meta.name.as_str());
                            break;
                        };
                        validate_index_key_type(
                            &table_meta.meta.name,
                            &index_meta.meta.name,
                            &code_field,
                        )?;
                        index_fields.push(code_field);
                    }
                    if let Some(field_name) = missing_exported_field {
                        if is_primary_index {
                            // Primary indexes are a generated loader contract, so hiding one is an error.
                            return Err(ConfigForgeError::Validation(format!(
                                "table `{}` primary index `{}` references non-exported field `{}` for current target",
                                table_meta.meta.name, index_meta.meta.name, field_name
                            )));
                        }
                        // Secondary indexes can disappear with target-hidden fields without breaking table access.
                        continue;
                    }
                    indexes.push(CodeIndex {
                        name: index_meta.meta.name.clone(),
                        unique: index_meta.meta.unique,
                        primary: is_primary_index,
                        fields: index_fields,
                    });
                }
                for field in &fields {
                    let synthetic_index_name = format!("by_{}", field.name);
                    if table_meta.fields.iter().any(|linked_field| {
                        linked_field.meta.name == field.name && linked_field.meta.unique
                    }) && !indexes
                        .iter()
                        .any(|index| index.name == synthetic_index_name)
                    {
                        validate_index_key_type(&table_meta.meta.name, &synthetic_index_name, field)?;
                        indexes.push(CodeIndex {
                            name: synthetic_index_name,
                            unique: true,
                            primary: false,
                            fields: vec![field.clone()],
                        });
                    }
                }
            }
            ensure_model_unique_names(
                &indexes,
                |index| index.name.as_str(),
                &format!("table `{}` indexes", table_meta.meta.name),
            )?;

            let foreign_keys = if kind == CodeTableKind::Collection {
                build_code_foreign_keys(meta, table_meta, &field_index, target_tags)?
            } else {
                Vec::new()
            };

            let data_file = table_meta.meta.name.clone();
            let data_format = JSON_FORMAT.to_owned();
            let schema_revision = Some(binary_schema_revision(table_meta, target_tags));
            let primary_index = indexes
                .iter()
                .find(|index| index.primary)
                .map(|index| index.name.clone());
            Ok(CodeTable {
                name: table_meta.meta.name.clone(),
                kind,
                desc: table_meta.meta.desc.clone(),
                fields,
                fragments,
                indexes,
                foreign_keys,
                primary_index,
                data_file,
                data_format,
                schema_revision,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    ensure_model_unique_names(&structs, |item| item.name.as_str(), "struct")?;
    ensure_model_unique_names(&tables, |item| item.name.as_str(), "table")?;

    Ok(CodeModel {
        enums,
        structs,
        tables,
    })
}

impl CodeTableKind {
    fn from_table_kind(kind: TableKind) -> Self {
        match kind {
            TableKind::Object => Self::Object,
            TableKind::Collection => Self::Collection,
        }
    }
}

fn build_code_foreign_keys(
    meta: &LinkedMetaModel<'_>,
    table_meta: &crate::meta::LinkedTableMeta<'_>,
    field_index: &HashMap<&str, CodeField>,
    target_tags: &crate::meta::TargetTags,
) -> Result<Vec<CodeForeignKey>> {
    let mut foreign_keys = Vec::new();
    for foreign_key in &table_meta.foreign_keys {
        if !foreign_key.is_exported_for(target_tags) {
            continue;
        }
        // Linking proves the full-model FK shape; IR must still prove the target-visible shape.
        let target_table = meta.table(foreign_key.target_table_name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "table `{}` foreign key `{}` references unknown target table `{}`",
                table_meta.meta.name, foreign_key.name, foreign_key.target_table_name
            ))
        })?;
        if !target_table.meta.is_exported_for(target_tags) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` foreign key `{}` references target table `{}` which is not exported for current target",
                table_meta.meta.name, foreign_key.name, foreign_key.target_table_name
            )));
        }
        let target_index = target_table
            .index(foreign_key.target_index_name)
            .ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "table `{}` foreign key `{}` references unknown target index `{}.{}`",
                    table_meta.meta.name,
                    foreign_key.name,
                    target_table.meta.name,
                    foreign_key.target_index_name
                ))
            })?;
        if !target_index.meta.is_exported_for(target_tags) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` foreign key `{}` references target index `{}.{}` which is not exported for current target",
                table_meta.meta.name,
                foreign_key.name,
                target_table.meta.name,
                target_index.meta.name
            )));
        }
        for target_field in &target_index.fields {
            if !target_field.meta.is_exported_for(target_tags) {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{}` foreign key `{}` references non-exported target index field `{}.{}` for current target",
                    table_meta.meta.name,
                    foreign_key.name,
                    target_table.meta.name,
                    target_field.meta.name
                )));
            }
        }
        let mut fields = Vec::new();
        for field in &foreign_key.fields {
            let Some(code_field) = field_index.get(field.meta.name.as_str()).cloned() else {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{}` foreign key `{}` references non-exported source field `{}` for current target",
                    table_meta.meta.name, foreign_key.name, field.meta.name
                )));
            };
            fields.push(code_field);
        }
        foreign_keys.push(CodeForeignKey {
            name: foreign_key.name.to_owned(),
            desc: foreign_key.desc.map(str::to_owned),
            fields,
            target_table: foreign_key.target_table_name.to_owned(),
            target_index: foreign_key.target_index_name.to_owned(),
            allow_empty: foreign_key.allow_empty,
        });
    }
    ensure_model_unique_names(
        &foreign_keys,
        |foreign_key| foreign_key.name.as_str(),
        &format!("table `{}` foreign keys", table_meta.meta.name),
    )?;
    Ok(foreign_keys)
}

fn is_runtime_codegen_field(
    field: &crate::meta::LinkedFieldMeta<'_>,
    target_tags: &crate::meta::TargetTags,
) -> bool {
    field.meta.is_exported_for(target_tags)
}

fn validate_index_key_type(table_name: &str, index_name: &str, field: &CodeField) -> Result<()> {
    match field.code_type {
        CodeTypeRef::Float | CodeTypeRef::Struct { .. } => {
            Err(ConfigForgeError::Validation(format!(
                "table `{table_name}` index `{index_name}` field `{}` uses unsupported index key type `{:?}`",
                field.name, field.code_type
            )))
        }
        CodeTypeRef::Int
        | CodeTypeRef::Bool
        | CodeTypeRef::String
        | CodeTypeRef::Text
        | CodeTypeRef::Asset
        | CodeTypeRef::Enum { .. } => Ok(()),
    }
}

fn code_field_from_linked(field: &crate::meta::LinkedFieldMeta<'_>) -> Result<CodeField> {
    Ok(CodeField {
        name: field.meta.name.clone(),
        code_type: code_type_from_linked(field)?,
        desc: field.meta.desc.clone(),
        required: field.meta.required,
    })
}

fn code_type_from_linked(field: &crate::meta::LinkedFieldMeta<'_>) -> Result<CodeTypeRef> {
    Ok(match &field.linked_field {
        LinkedField::Primitive(PrimitiveFieldType::Int) => CodeTypeRef::Int,
        LinkedField::Primitive(PrimitiveFieldType::Float) => CodeTypeRef::Float,
        LinkedField::Primitive(PrimitiveFieldType::Bool) => CodeTypeRef::Bool,
        LinkedField::Primitive(PrimitiveFieldType::String) => CodeTypeRef::String,
        LinkedField::Primitive(PrimitiveFieldType::Text) => CodeTypeRef::Text,
        LinkedField::Primitive(PrimitiveFieldType::Asset) => CodeTypeRef::Asset,
        LinkedField::Enum(enum_meta) => CodeTypeRef::Enum {
            name: enum_meta.name.clone(),
        },
        LinkedField::Struct(struct_type) => CodeTypeRef::Struct {
            name: struct_type.meta.name.clone(),
        },
    })
}

pub fn require_non_empty_index_fields(model: &CodeModel) -> Result<()> {
    for table in &model.tables {
        for index in &table.indexes {
            if index.fields.is_empty() {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{}` index `{}` has no codegen fields",
                    table.name, index.name
                )));
            }
        }
    }
    Ok(())
}

fn ensure_model_unique_names<T>(
    items: &[T],
    name_of: impl Fn(&T) -> &str,
    scope: &str,
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let name = name_of(item);
        if !seen.insert(name) {
            return Err(ConfigForgeError::Validation(format!(
                "code model contains duplicate {scope} name `{name}`"
            )));
        }
    }
    Ok(())
}
