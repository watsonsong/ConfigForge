// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
#[cfg(test)]
use std::path::PathBuf;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    meta::{
        ColorAlphaPolicy, ColorChannel, ColorFormat, ColorSpace, LinkedField,
        LinkedFragmentPayload, LinkedMetaModel, PrimitiveFieldType, TableKind, VectorFormat,
    },
    runtime_contract::{DEFAULT_RUNTIME_DATA_FORMAT, RuntimeDataFile, binary_schema_revision},
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
    pub preserve_source_order: bool,
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
    pub builtin_options: Option<CodeBuiltinFieldOptions>,
    pub desc: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodeBuiltinFieldOptions {
    Color(CodeColorOptions),
    Vector(CodeVectorOptions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeColorOptions {
    pub format: ColorFormat,
    pub channel: ColorChannel,
    pub space: ColorSpace,
    pub alpha: ColorAlphaPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeVectorOptions {
    pub format: VectorFormat,
    pub components: Vec<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeIndex {
    pub name: String,
    pub unique: bool,
    pub primary: bool,
    pub ordered: bool,
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
    pub linked_field: Option<CodeLinkedField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeLinkedField {
    pub name: String,
    pub cardinality: CodeRelationCardinality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RelationKeyPlan<'a> {
    pub key_fields: &'a [CodeField],
    pub optional_key_fields: Vec<&'a CodeField>,
    pub all_key_fields_optional: bool,
    pub should_report_partial_empty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeRelationCardinality {
    One,
    OptionalOne,
    Many,
}

pub(crate) fn relation_key_plan(foreign_key: &CodeForeignKey) -> RelationKeyPlan<'_> {
    let optional_key_fields = foreign_key
        .fields
        .iter()
        .filter(|field| !field.required)
        .collect::<Vec<_>>();
    let has_optional_key_fields = !optional_key_fields.is_empty();
    let all_key_fields_optional =
        has_optional_key_fields && optional_key_fields.len() == foreign_key.fields.len();
    let should_report_partial_empty =
        has_optional_key_fields && (!all_key_fields_optional || foreign_key.fields.len() > 1);

    RelationKeyPlan {
        key_fields: &foreign_key.fields,
        optional_key_fields,
        all_key_fields_optional,
        should_report_partial_empty,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodeTypeRef {
    Int,
    Float,
    Bool,
    String,
    Text,
    Asset,
    Color,
    Vector {
        dimension: usize,
        element: CodeVectorElementType,
        components: Vec<String>,
    },
    Enum {
        name: String,
    },
    Struct {
        name: String,
    },
    List(Box<CodeTypeRef>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeVectorElementType {
    Float,
    Int,
}

#[cfg(test)]
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
                            return Err(code_model_error(
                                format!(
                                    "table `{}` primary index `{}` references non-exported field `{}` for current target",
                                    table_meta.meta.name, index_meta.meta.name, field_name
                                ),
                                &table_meta.meta.name,
                                Some(field_name),
                                field_name,
                                "exported primary index field for current target",
                                "Make the primary index field visible for this target or choose a visible primary index.",
                            ));
                        }
                        // Secondary indexes can disappear with target-hidden fields without breaking table access.
                        continue;
                    }
                    indexes.push(CodeIndex {
                        name: index_meta.meta.name.clone(),
                        unique: index_meta.meta.unique,
                        primary: is_primary_index,
                        ordered: index_meta.meta.ordered,
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
                            ordered: false,
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

            let foreign_keys = build_code_foreign_keys(meta, table_meta, &field_index, target_tags)?;

            let data_file = table_meta.meta.name.clone();
            let data_format = DEFAULT_RUNTIME_DATA_FORMAT.to_owned();
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
                preserve_source_order: table_meta.meta.preserve_source_order,
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
            code_model_error(
                format!(
                    "table `{}` foreign key `{}` references unknown target table `{}`",
                    table_meta.meta.name, foreign_key.name, foreign_key.target_table_name
                ),
                &table_meta.meta.name,
                None::<&str>,
                foreign_key.target_table_name,
                "linked target table in code model",
                "Check the foreign key target table and target filtering rules.",
            )
        })?;
        if !target_table.meta.is_exported_for(target_tags) {
            return Err(code_model_error(
                format!(
                    "table `{}` foreign key `{}` references target table `{}` which is not exported for current target",
                    table_meta.meta.name, foreign_key.name, foreign_key.target_table_name
                ),
                &table_meta.meta.name,
                None::<&str>,
                foreign_key.target_table_name,
                "target-visible table",
                "Align the foreign key export_when with the target table visibility.",
            ));
        }
        let target_index = target_table
            .index(foreign_key.target_index_name)
            .ok_or_else(|| {
                code_model_error(
                    format!(
                        "table `{}` foreign key `{}` references unknown target index `{}.{}`",
                        table_meta.meta.name,
                        foreign_key.name,
                        target_table.meta.name,
                        foreign_key.target_index_name
                    ),
                    &table_meta.meta.name,
                    None::<&str>,
                    format!(
                        "{}.{}",
                        target_table.meta.name, foreign_key.target_index_name
                    ),
                    "linked target index in code model",
                    "Check the foreign key target index and target filtering rules.",
                )
            })?;
        if !target_index.meta.is_exported_for(target_tags) {
            return Err(code_model_error(
                format!(
                    "table `{}` foreign key `{}` references target index `{}.{}` which is not exported for current target",
                    table_meta.meta.name,
                    foreign_key.name,
                    target_table.meta.name,
                    target_index.meta.name
                ),
                &table_meta.meta.name,
                None::<&str>,
                format!("{}.{}", target_table.meta.name, target_index.meta.name),
                "target-visible index",
                "Align the foreign key export_when with the target index visibility.",
            ));
        }
        for target_field in &target_index.fields {
            if !target_field.meta.is_exported_for(target_tags) {
                return Err(code_model_error(
                    format!(
                        "table `{}` foreign key `{}` references non-exported target index field `{}.{}` for current target",
                        table_meta.meta.name,
                        foreign_key.name,
                        target_table.meta.name,
                        target_field.meta.name
                    ),
                    &table_meta.meta.name,
                    None::<&str>,
                    format!("{}.{}", target_table.meta.name, target_field.meta.name),
                    "target-visible index field",
                    "Ensure all target index fields are exported for the same target.",
                ));
            }
        }
        let mut fields = Vec::new();
        for field in &foreign_key.fields {
            let Some(code_field) = field_index.get(field.meta.name.as_str()).cloned() else {
                return Err(code_model_error(
                    format!(
                        "table `{}` foreign key `{}` references non-exported source field `{}` for current target",
                        table_meta.meta.name, foreign_key.name, field.meta.name
                    ),
                    &table_meta.meta.name,
                    Some(field.meta.name.as_str()),
                    field.meta.name.as_str(),
                    "target-visible source field",
                    "Align the foreign key export_when with its source field visibility.",
                ));
            };
            fields.push(code_field);
        }
        let linked_field = foreign_key.linked_field.map(|name| {
            let cardinality =
                if fields.len() == 1 && matches!(fields[0].code_type, CodeTypeRef::List(_)) {
                    CodeRelationCardinality::Many
                } else if foreign_key.allow_empty {
                    CodeRelationCardinality::OptionalOne
                } else {
                    CodeRelationCardinality::One
                };
            CodeLinkedField {
                name: name.to_owned(),
                cardinality,
            }
        });
        foreign_keys.push(CodeForeignKey {
            name: foreign_key.name.to_owned(),
            desc: foreign_key.desc.map(str::to_owned),
            fields,
            target_table: foreign_key.target_table_name.to_owned(),
            target_index: foreign_key.target_index_name.to_owned(),
            allow_empty: foreign_key.allow_empty,
            linked_field,
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
        CodeTypeRef::Float
        | CodeTypeRef::Color
        | CodeTypeRef::Vector { .. }
        | CodeTypeRef::Struct { .. }
        | CodeTypeRef::List(_) => Err(code_model_error(
            format!(
                "table `{table_name}` index `{index_name}` field `{}` uses unsupported index key type `{:?}`",
                field.name, field.code_type
            ),
            table_name,
            Some(field.name.as_str()),
            format!("{:?}", field.code_type),
            "int, bool, string, text, asset, or enum index key type",
            "Use a stable scalar key type for generated index helpers.",
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
        code_type: code_type_from_field(field)?,
        builtin_options: builtin_options_from_field(field),
        desc: field.meta.desc.clone(),
        required: field.meta.required,
    })
}

fn builtin_options_from_field(
    field: &crate::meta::LinkedFieldMeta<'_>,
) -> Option<CodeBuiltinFieldOptions> {
    let primitive = field.primitive_type()?;
    if primitive == PrimitiveFieldType::Color {
        return Some(CodeBuiltinFieldOptions::Color(CodeColorOptions {
            format: field.meta.color_format.unwrap_or_default(),
            channel: field.meta.color_channel?,
            space: field.meta.color_space.unwrap_or_default(),
            alpha: field.meta.color_alpha.unwrap_or_default(),
        }));
    }
    if primitive.vector_dimension().is_some() {
        return Some(CodeBuiltinFieldOptions::Vector(CodeVectorOptions {
            format: field.meta.vector_format.unwrap_or_default(),
            components: field.meta.vector_component_names(),
            unit: field.meta.unit.clone(),
        }));
    }
    None
}

fn code_type_from_field(field: &crate::meta::LinkedFieldMeta<'_>) -> Result<CodeTypeRef> {
    match &field.linked_field {
        LinkedField::Primitive(primitive) if primitive.vector_dimension().is_some() => {
            Ok(CodeTypeRef::Vector {
                dimension: primitive.vector_dimension().unwrap_or(4),
                element: if primitive.is_int_vector() {
                    CodeVectorElementType::Int
                } else {
                    CodeVectorElementType::Float
                },
                components: field.meta.vector_component_names(),
            })
        }
        _ => code_type_from_linked(&field.linked_field),
    }
}

fn code_type_from_linked(field: &LinkedField<'_>) -> Result<CodeTypeRef> {
    Ok(match field {
        LinkedField::Primitive(PrimitiveFieldType::Int) => CodeTypeRef::Int,
        LinkedField::Primitive(PrimitiveFieldType::Float) => CodeTypeRef::Float,
        LinkedField::Primitive(PrimitiveFieldType::Bool) => CodeTypeRef::Bool,
        LinkedField::Primitive(PrimitiveFieldType::String) => CodeTypeRef::String,
        LinkedField::Primitive(PrimitiveFieldType::Text) => CodeTypeRef::Text,
        LinkedField::Primitive(PrimitiveFieldType::Asset) => CodeTypeRef::Asset,
        LinkedField::Primitive(PrimitiveFieldType::Color) => CodeTypeRef::Color,
        LinkedField::Primitive(
            primitive @ (PrimitiveFieldType::Vector2
            | PrimitiveFieldType::Vector3
            | PrimitiveFieldType::Vector4
            | PrimitiveFieldType::Vector2Int
            | PrimitiveFieldType::Vector3Int
            | PrimitiveFieldType::Vector4Int),
        ) => CodeTypeRef::Vector {
            dimension: primitive.vector_dimension().unwrap_or(4),
            element: if primitive.is_int_vector() {
                CodeVectorElementType::Int
            } else {
                CodeVectorElementType::Float
            },
            components: default_vector_components(primitive.vector_dimension().unwrap_or(4)),
        },
        LinkedField::Enum(enum_meta) => CodeTypeRef::Enum {
            name: enum_meta.name.clone(),
        },
        LinkedField::Struct(struct_type) => CodeTypeRef::Struct {
            name: struct_type.meta.name.clone(),
        },
        LinkedField::List(element_type) => {
            CodeTypeRef::List(Box::new(code_type_from_linked(element_type)?))
        }
    })
}

fn default_vector_components(dimension: usize) -> Vec<String> {
    ["x", "y", "z", "w"]
        .into_iter()
        .take(dimension)
        .map(str::to_owned)
        .collect()
}

pub fn require_non_empty_index_fields(model: &CodeModel) -> Result<()> {
    for table in &model.tables {
        for index in &table.indexes {
            if index.fields.is_empty() {
                return Err(code_model_error(
                    format!(
                        "table `{}` index `{}` has no codegen fields",
                        table.name, index.name
                    ),
                    &table.name,
                    None::<&str>,
                    &index.name,
                    "at least one target-visible index field",
                    "Ensure the index has fields exported for this target or hide the index.",
                ));
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
            return Err(code_model_error(
                format!("code model contains duplicate {scope} name `{name}`"),
                "<code model>",
                None::<&str>,
                name,
                format!("unique {scope} name"),
                "Rename the model element so generated code symbols remain unique.",
            ));
        }
    }
    Ok(())
}

fn code_model_error(
    message: String,
    table: impl Into<String>,
    field: Option<impl Into<String>>,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                table: Some(table.into()),
                field: field.map(Into::into),
                ..Default::default()
            },
        )
        .with_source("generator.codegen.model")
        .with_domain("generator")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
}
