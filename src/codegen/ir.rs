use std::{collections::HashMap, path::PathBuf};

use crate::{
    meta::{LinkedField, LinkedMetaModel, PrimitiveFieldType},
    ConfigForgeError, Result,
};

use super::{ArtifactKind, RuntimeDataFile};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeModel {
    pub enums: Vec<CodeEnum>,
    pub structs: Vec<CodeStruct>,
    pub tables: Vec<CodeTable>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeEnum {
    pub name: String,
    pub desc: Option<String>,
    pub values: Vec<CodeEnumValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeEnumValue {
    pub name: String,
    pub desc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeStruct {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeTable {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub indexes: Vec<CodeIndex>,
    pub primary_index: Option<String>,
    pub data_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeField {
    pub name: String,
    pub code_type: CodeTypeRef,
    pub desc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeIndex {
    pub name: String,
    pub unique: bool,
    pub primary: bool,
    pub fields: Vec<CodeField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeTypeRef {
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
    pub kind: ArtifactKind,
    pub content: String,
}

pub fn build_code_model(
    meta: &LinkedMetaModel<'_>,
    runtime_data_files: &[RuntimeDataFile],
) -> Result<CodeModel> {
    let data_file_index = runtime_data_files
        .iter()
        .map(|file| (file.table_name.as_str(), file.path.as_str()))
        .collect::<HashMap<_, _>>();
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
                .filter(|field| is_runtime_codegen_field(field))
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
        .map(|table_meta| {
            let fields = table_meta
                .fields
                .iter()
                .filter(|field| is_runtime_codegen_field(field))
                .map(code_field_from_linked)
                .collect::<Result<Vec<_>>>()?;
            ensure_model_unique_names(
                &fields,
                |field| field.name.as_str(),
                &format!("table `{}` fields", table_meta.meta.name),
            )?;
            let field_index = fields
                .iter()
                .map(|field| (field.name.as_str(), field.clone()))
                .collect::<HashMap<_, _>>();
            let mut indexes = Vec::new();
            for index_meta in &table_meta.indexes {
                let mut index_fields = Vec::new();
                for field in &index_meta.fields {
                    let Some(code_field) = field_index.get(field.meta.name.as_str()).cloned()
                    else {
                        return Err(ConfigForgeError::Validation(format!(
                            "table `{}` index `{}` references non-exported field `{}`",
                            table_meta.meta.name, index_meta.meta.name, field.meta.name
                        )));
                    };
                    validate_index_key_type(
                        &table_meta.meta.name,
                        &index_meta.meta.name,
                        &code_field,
                    )?;
                    index_fields.push(code_field);
                }
                indexes.push(CodeIndex {
                    name: index_meta.meta.name.clone(),
                    unique: index_meta.meta.unique,
                    primary: index_meta.meta.primary,
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
            ensure_model_unique_names(
                &indexes,
                |index| index.name.as_str(),
                &format!("table `{}` indexes", table_meta.meta.name),
            )?;

            let data_file = data_file_index
                .get(table_meta.meta.name.as_str())
                .copied()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{}.json", table_meta.meta.name));
            Ok(CodeTable {
                name: table_meta.meta.name.clone(),
                desc: table_meta.meta.desc.clone(),
                fields,
                indexes,
                primary_index: table_meta
                    .primary_index()
                    .map(|index| index.meta.name.clone()),
                data_file,
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

fn is_runtime_codegen_field(field: &crate::meta::LinkedFieldMeta<'_>) -> bool {
    field.meta.is_runtime_exported()
}

fn validate_index_key_type(table_name: &str, index_name: &str, field: &CodeField) -> Result<()> {
    match field.code_type {
        CodeTypeRef::Float | CodeTypeRef::Struct { .. } => Err(ConfigForgeError::Validation(
            format!(
                "table `{table_name}` index `{index_name}` field `{}` uses unsupported index key type `{:?}`",
                field.name, field.code_type
            ),
        )),
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
