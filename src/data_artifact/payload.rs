// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    data::{ArrayElementValue, DataResolver, DataTable, DataValue, ResolvedValue, VectorValue},
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    meta::{LinkedField, LinkedFieldMeta, LinkedTableMeta, TableKind, TargetTags},
};

use super::{ExportedTable, ExportedTablePayload, ExportedValue, model::ExportedTableMeta};

pub fn build_exported_table_payload(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
) -> Result<ExportedTablePayload> {
    match table_meta.meta.kind {
        TableKind::Collection => build_exported_collection_payload(table_meta, table, target_tags)
            .map(ExportedTablePayload::Collection),
        TableKind::Object => build_exported_object_payload(table_meta, table, target_tags)
            .map(ExportedTablePayload::Object),
    }
}

fn build_exported_collection_payload(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
) -> Result<ExportedTable> {
    let visible_row_indices = table
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            table_meta
                .meta
                .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
        })
        .map(|(row_index, _)| row_index)
        .collect::<Vec<_>>();
    let resolved_table = DataResolver::resolve_table_rows(
        table_meta,
        table,
        Some(target_tags),
        visible_row_indices.iter().copied(),
    )?;
    let exported_fields = table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .collect::<Vec<_>>();
    let rows = visible_row_indices
        .iter()
        .enumerate()
        .map(|(resolved_row_index, row_index)| {
            let resolved_row = resolved_table.rows.get(resolved_row_index).ok_or_else(|| {
                data_payload_error(
                    &table_meta.meta.name,
                    None::<&str>,
                    format!(
                        "resolved table `{}` is missing row {}",
                        table_meta.meta.name,
                        resolved_row_index + 1
                    ),
                    format!("resolved row {}", resolved_row_index + 1),
                    "resolved row for every exported source row",
                    "Check data normalization and target row filtering before payload export.",
                )
            })?;
            let mut exported_row = BTreeMap::new();
            for field in &exported_fields {
                if let Some(value) = resolved_row.get(&field.meta.name) {
                    let value = exported_value_from_resolved(value, &field.linked_field, field)?;
                    exported_row.insert(field.meta.name.clone(), value);
                }
            }
            if let Some(row_source) = table.row_source(*row_index) {
                // Variant-source rows export only fragments activated by their originating source.
                for fragment_name in &row_source.fragments {
                    let Some(fragment) = table_meta.fragment(fragment_name) else {
                        continue;
                    };
                    if !fragment.meta.is_exported_for(target_tags) {
                        continue;
                    }
                    if let Some(value) = resolved_row.get(fragment.meta.name.as_str()) {
                        exported_row.insert(
                            fragment.meta.name.clone(),
                            exported_value_from_fragment(value, fragment)?,
                        );
                    }
                }
            }
            Ok(exported_row)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ExportedTable {
        meta: ExportedTableMeta {
            name: table_meta.meta.name.clone(),
        },
        rows,
    })
}

fn exported_value_from_fragment(
    value: &ResolvedValue,
    fragment: &crate::meta::LinkedFragmentMeta<'_>,
) -> Result<ExportedValue> {
    let DataValue::Struct(children) = &value.value else {
        return Err(data_payload_error(
            "<unknown>",
            Some(fragment.meta.name.as_str()),
            format!(
                "resolved fragment `{}` is not a struct value",
                fragment.meta.name
            ),
            "non-struct resolved fragment value",
            "struct resolved fragment value",
            "Check resolver and fragment payload shape alignment.",
        ));
    };
    fragment
        .fields
        .iter()
        .filter_map(|field| children.get(&field.meta.name).map(|child| (field, child)))
        .map(|(field, child)| {
            Ok((
                field.meta.name.clone(),
                exported_value_from_resolved(child, &field.linked_field, field)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()
        .map(ExportedValue::Struct)
}

fn build_exported_object_payload(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
) -> Result<BTreeMap<String, ExportedValue>> {
    if table.rows.len() != 1 {
        return Err(data_payload_error(
            &table_meta.meta.name,
            None::<&str>,
            format!(
                "object table `{}` must normalize to exactly one logical row, got {}",
                table_meta.meta.name,
                table.rows.len()
            ),
            table.rows.len().to_string(),
            "exactly one normalized object-table row",
            "Normalize and validate object table input before runtime payload export.",
        ));
    }

    let Some(row) = table.rows.first() else {
        return Ok(BTreeMap::new());
    };
    if !table_meta
        .meta
        .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
    {
        return Ok(BTreeMap::new());
    }
    let resolved_table = DataResolver::resolve_table(table_meta, table, Some(target_tags))?;
    let resolved_row = resolved_table.rows.first().ok_or_else(|| {
        data_payload_error(
            &table_meta.meta.name,
            None::<&str>,
            format!(
                "resolved object table `{}` is missing singleton row",
                table_meta.meta.name
            ),
            "no resolved singleton row",
            "resolved singleton row",
            "Check object table normalization and resolver output before payload export.",
        )
    })?;

    let mut exported_object = BTreeMap::new();
    for field in table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
    {
        if let Some(value) = resolved_row.get(&field.meta.name) {
            let value = exported_value_from_resolved(value, &field.linked_field, field)?;
            exported_object.insert(field.meta.name.clone(), value);
        }
    }

    Ok(exported_object)
}

fn exported_value_from_resolved(
    value: &ResolvedValue,
    field_type: &LinkedField<'_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<ExportedValue> {
    match &value.value {
        DataValue::Scalar(_) => Ok(ExportedValue::Scalar(
            value.scalar_export_string().unwrap_or_default(),
        )),
        DataValue::Color(color) => Ok(ExportedValue::Struct(BTreeMap::from([
            ("r".to_owned(), ExportedValue::Number(color.r)),
            ("g".to_owned(), ExportedValue::Number(color.g)),
            ("b".to_owned(), ExportedValue::Number(color.b)),
            ("a".to_owned(), ExportedValue::Number(color.a)),
        ]))),
        DataValue::Vector(vector) => Ok(ExportedValue::Struct(vector_export_object(vector, field))),
        DataValue::Struct(children) => {
            let LinkedField::Struct(struct_type) = field_type else {
                return Err(data_payload_error(
                    "<unknown>",
                    Some(field.meta.name.as_str()),
                    format!(
                        "resolved field `{}` is a struct but linked field type is not struct",
                        field.meta.name
                    ),
                    "struct resolved value",
                    "struct linked field type",
                    "Check resolver and payload field shape alignment.",
                ));
            };
            struct_type
                .fields
                .iter()
                .filter_map(|child_field| {
                    children
                        .get(&child_field.meta.name)
                        .map(|child| (child_field, child))
                })
                .map(|(child_field, child)| {
                    Ok((
                        child_field.meta.name.clone(),
                        exported_value_from_resolved(
                            child,
                            &child_field.linked_field,
                            child_field,
                        )?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()
                .map(ExportedValue::Struct)
        }
        DataValue::Array(elements) => elements
            .iter()
            .map(|element| exported_value_from_array_element(element, field_type))
            .collect::<Result<Vec<_>>>()
            .map(ExportedValue::Array),
    }
}

fn exported_value_from_array_element(
    value: &ArrayElementValue,
    field_type: &LinkedField<'_>,
) -> Result<ExportedValue> {
    match &value.value {
        DataValue::Scalar(_) => Ok(ExportedValue::Scalar(
            value.scalar_export_string().unwrap_or_default(),
        )),
        DataValue::Color(color) => Ok(ExportedValue::Struct(BTreeMap::from([
            ("r".to_owned(), ExportedValue::Number(color.r)),
            ("g".to_owned(), ExportedValue::Number(color.g)),
            ("b".to_owned(), ExportedValue::Number(color.b)),
            ("a".to_owned(), ExportedValue::Number(color.a)),
        ]))),
        DataValue::Vector(_) => Err(data_payload_error(
            "<unknown>",
            None::<&str>,
            "runtime payload cannot export vector list element without field component metadata"
                .to_owned(),
            "vector list element",
            "scalar list element",
            "Use top-level or struct vector fields until builtin list element metadata is modeled.",
        )),
        DataValue::Struct(children) => {
            let LinkedField::Struct(struct_type) = field_type else {
                return Err(data_payload_error(
                    "<unknown>",
                    None::<&str>,
                    "resolved list element is a struct but linked element type is not struct"
                        .to_owned(),
                    "struct resolved value",
                    "struct linked element type",
                    "Check resolver and payload list element shape alignment.",
                ));
            };
            struct_type
                .fields
                .iter()
                .filter_map(|child_field| {
                    children
                        .get(&child_field.meta.name)
                        .map(|child| (child_field, child))
                })
                .map(|(child_field, child)| {
                    Ok((
                        child_field.meta.name.clone(),
                        exported_value_from_resolved(
                            child,
                            &child_field.linked_field,
                            child_field,
                        )?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()
                .map(ExportedValue::Struct)
        }
        DataValue::Array(elements) => elements
            .iter()
            .map(|element| exported_value_from_array_element(element, field_type))
            .collect::<Result<Vec<_>>>()
            .map(ExportedValue::Array),
    }
}

fn vector_export_object(
    vector: &VectorValue,
    field: &LinkedFieldMeta<'_>,
) -> BTreeMap<String, ExportedValue> {
    let components = field.meta.vector_component_names();
    match vector {
        VectorValue::Float(values) => components
            .into_iter()
            .zip(values.iter())
            .map(|(component, value)| (component, ExportedValue::Number(*value)))
            .collect(),
        VectorValue::Int(values) => components
            .into_iter()
            .zip(values.iter())
            .map(|(component, value)| (component, ExportedValue::Number(*value as f64)))
            .collect(),
    }
}

fn data_payload_error(
    table: impl Into<String>,
    field: Option<impl Into<String>>,
    message: String,
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
        .with_source("generator.data.payload")
        .with_domain("generator")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
}
