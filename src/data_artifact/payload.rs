// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    data::{DataResolver, DataTable, DataValue, ResolvedValue},
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    meta::{LinkedTableMeta, TableKind, TargetTags},
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
                    let value = exported_value_from_resolved(value)?;
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
                            exported_value_from_resolved(value)?,
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
            let value = exported_value_from_resolved(value)?;
            exported_object.insert(field.meta.name.clone(), value);
        }
    }

    Ok(exported_object)
}

fn exported_value_from_resolved(value: &ResolvedValue) -> Result<ExportedValue> {
    match &value.value {
        DataValue::Scalar(_) => Ok(ExportedValue::Scalar(
            value.scalar_export_string().unwrap_or_default(),
        )),
        DataValue::Color(color) => Ok(ExportedValue::NumberArray(vec![
            color.r, color.g, color.b, color.a,
        ])),
        DataValue::Vector(vector) => Ok(ExportedValue::NumberArray(vector_export_numbers(vector))),
        DataValue::Struct(children) => children
            .iter()
            .map(|(field_name, child)| {
                Ok((field_name.clone(), exported_value_from_resolved(child)?))
            })
            .collect::<Result<BTreeMap<_, _>>>()
            .map(ExportedValue::Struct),
        DataValue::Array(elements) => elements
            .iter()
            .map(exported_value_from_array_element)
            .collect::<Result<Vec<_>>>()
            .map(ExportedValue::Array),
    }
}

fn exported_value_from_array_element(
    value: &crate::data::ArrayElementValue,
) -> Result<ExportedValue> {
    match &value.value {
        DataValue::Scalar(_) => Ok(ExportedValue::Scalar(
            value.scalar_export_string().unwrap_or_default(),
        )),
        DataValue::Color(color) => Ok(ExportedValue::NumberArray(vec![
            color.r, color.g, color.b, color.a,
        ])),
        DataValue::Vector(vector) => Ok(ExportedValue::NumberArray(vector_export_numbers(vector))),
        DataValue::Struct(children) => children
            .iter()
            .map(|(field_name, child)| {
                Ok((field_name.clone(), exported_value_from_resolved(child)?))
            })
            .collect::<Result<BTreeMap<_, _>>>()
            .map(ExportedValue::Struct),
        DataValue::Array(elements) => elements
            .iter()
            .map(exported_value_from_array_element)
            .collect::<Result<Vec<_>>>()
            .map(ExportedValue::Array),
    }
}

fn vector_export_numbers(vector: &crate::data::VectorValue) -> Vec<f64> {
    match vector {
        crate::data::VectorValue::Float(values) => values.clone(),
        crate::data::VectorValue::Int(values) => values.iter().map(|value| *value as f64).collect(),
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
