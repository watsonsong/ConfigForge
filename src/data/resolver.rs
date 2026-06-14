// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    data::typed_value::{
        parse_color_field_value, parse_list_cell_value, parse_scalar_field_value,
        parse_scalar_for_type, parse_vector_field_value,
    },
    data::value_conversion::{
        json_value_is_empty, json_value_to_cell, json_value_to_scalar_cell_for_field,
        toml_default_to_json_value,
    },
    diagnostic::code::{CF_DATA_INVALID_TYPE, CF_DATA_SOURCE_SCHEMA_INVALID},
    meta::{
        LinkedField, LinkedFieldMeta, LinkedMetaModel, LinkedStructType, LinkedTableMeta,
        PrimitiveFieldType, TableKind, TargetTags, VectorFormat,
    },
};

use super::{
    ArrayElementValue, DataRow, DataTable, DataValue, ResolvedRow, ResolvedTable, ResolvedValue,
    RowSourceInfo, ValueSourceKind,
};

#[cfg(test)]
use super::{DataSet, ResolvedDataSet};

#[cfg_attr(not(test), allow(dead_code))]
pub struct DataResolver<'meta> {
    meta: &'meta LinkedMetaModel<'meta>,
    target_tags: Option<TargetTags>,
}

impl<'meta> DataResolver<'meta> {
    #[cfg(test)]
    pub fn new(meta: &'meta LinkedMetaModel<'meta>) -> Self {
        Self {
            meta,
            target_tags: None,
        }
    }

    #[cfg(test)]
    pub fn for_target(meta: &'meta LinkedMetaModel<'meta>, target_tags: &TargetTags) -> Self {
        Self {
            meta,
            target_tags: Some(target_tags.clone()),
        }
    }

    #[cfg(test)]
    pub fn resolve_required_tables(&self, data_set: &DataSet) -> Result<ResolvedDataSet> {
        let mut tables = Vec::with_capacity(self.meta.tables.len());
        for table_meta in &self.meta.tables {
            let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
                resolver_table_error(
                    table_meta.meta.name.as_str(),
                    format!("data set is missing table `{}`", table_meta.meta.name),
                    "missing table",
                    "data set table declared by Meta",
                    "Add source data for the table or remove the table from Meta.",
                )
            })?;
            tables.push(Self::resolve_table(
                table_meta,
                table,
                self.target_tags.as_ref(),
            )?);
        }
        ResolvedDataSet::try_new(tables)
    }

    pub fn resolve_table(
        table_meta: &LinkedTableMeta<'meta>,
        table: &DataTable,
        target_tags: Option<&TargetTags>,
    ) -> Result<ResolvedTable> {
        let row_indices = (0..table.rows.len()).collect::<Vec<_>>();
        Self::resolve_table_rows(table_meta, table, target_tags, row_indices)
    }

    pub fn resolve_table_rows(
        table_meta: &LinkedTableMeta<'meta>,
        table: &DataTable,
        target_tags: Option<&TargetTags>,
        row_indices: impl IntoIterator<Item = usize>,
    ) -> Result<ResolvedTable> {
        let row_indices = row_indices.into_iter().collect::<Vec<_>>();
        let mut rows = Vec::with_capacity(row_indices.len());
        let mut row_sources = Vec::with_capacity(row_indices.len());
        for row_index in row_indices {
            rows.push(resolve_row(table_meta, table, target_tags, row_index)?);
            let row_source = table.row_source(row_index).cloned().ok_or_else(|| {
                resolver_row_error(
                    table.name.as_str(),
                    row_index + 1,
                    format!(
                        "table `{}` is missing row source for row {}",
                        table.name,
                        row_index + 1
                    ),
                    "missing row source",
                    "row source metadata",
                    "Load the table through a DataLoader so each row retains source trace.",
                )
            })?;
            row_sources.push(row_source);
        }

        ResolvedTable::new(table.name.clone(), rows, row_sources)
    }
}

fn resolve_row(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: Option<&TargetTags>,
    row_index: usize,
) -> Result<ResolvedRow> {
    let row = table.rows.get(row_index).ok_or_else(|| {
        resolver_row_error(
            table.name.as_str(),
            row_index + 1,
            format!("table `{}` is missing row {}", table.name, row_index + 1),
            "missing row",
            "data row",
            "Ensure requested row indices are within the table row range.",
        )
    })?;
    let row_source = table.row_source(row_index);
    let row_number = row_source
        .map(|source| source.source_row_number)
        .unwrap_or(row_index + 1);
    let active_fragments = row_source
        .map(|source| source.fragments.as_slice())
        .unwrap_or(&[]);
    let context = ResolveContext {
        table_name: table_meta.meta.name.as_str(),
        row,
        row_source,
        row_number,
        target_tags,
    };
    validate_source_schema(table_meta, &context, active_fragments)?;

    let mut resolved_row = ResolvedRow::new();
    for field in table_meta
        .fields
        .iter()
        .filter(|field| context.field_is_visible(field))
    {
        if let Some(value) = resolve_field_value(context.value(field.meta.name.as_str()), field)? {
            resolved_row.insert(field.meta.name.clone(), value);
        }
    }

    if table_meta.meta.kind == TableKind::Collection {
        resolve_active_fragments(table_meta, &context, active_fragments, &mut resolved_row)?;
    }

    Ok(resolved_row)
}

#[derive(Clone, Copy)]
struct ResolveContext<'a> {
    table_name: &'a str,
    row: &'a DataRow,
    row_source: Option<&'a RowSourceInfo>,
    row_number: usize,
    target_tags: Option<&'a TargetTags>,
}

impl<'a> ResolveContext<'a> {
    fn value<'path>(&self, field_path: &'path str) -> ValueContext<'a, 'path> {
        ValueContext {
            table_name: self.table_name,
            row: self.row,
            row_source: self.row_source,
            row_number: self.row_number,
            target_tags: self.target_tags,
            field_path,
        }
    }

    fn field_is_visible(&self, field: &LinkedFieldMeta<'_>) -> bool {
        field_is_visible(field, self.target_tags)
    }
}

#[derive(Clone, Copy)]
struct ValueContext<'a, 'path> {
    table_name: &'a str,
    row: &'a DataRow,
    row_source: Option<&'a RowSourceInfo>,
    row_number: usize,
    target_tags: Option<&'a TargetTags>,
    field_path: &'path str,
}

impl<'a, 'path> ValueContext<'a, 'path> {
    fn at<'child>(&self, field_path: &'child str) -> ValueContext<'a, 'child> {
        ValueContext {
            table_name: self.table_name,
            row: self.row,
            row_source: self.row_source,
            row_number: self.row_number,
            target_tags: self.target_tags,
            field_path,
        }
    }

    fn parse_scalar_field(
        &self,
        field: &LinkedFieldMeta<'_>,
        raw: &str,
    ) -> Result<super::ScalarValue> {
        parse_scalar_field_value(
            self.table_name,
            self.row_number,
            self.field_path,
            field,
            raw,
        )
    }

    fn parse_color_field(
        &self,
        field: &LinkedFieldMeta<'_>,
        raw: &str,
    ) -> Result<crate::data::ColorValue> {
        parse_color_field_value(
            self.table_name,
            self.row_number,
            self.field_path,
            field,
            raw,
        )
    }

    fn parse_vector_field(
        &self,
        field: &LinkedFieldMeta<'_>,
        raw: &str,
    ) -> Result<crate::data::VectorValue> {
        parse_vector_field_value(
            self.table_name,
            self.row_number,
            self.field_path,
            field,
            raw,
        )
    }

    fn parse_list_cell(
        &self,
        element_type: &LinkedField<'_>,
        raw: &str,
        source_kind: ValueSourceKind,
    ) -> Result<Vec<super::ArrayElementValue>> {
        parse_list_cell_value(
            self.table_name,
            self.row_number,
            self.field_path,
            element_type,
            raw,
            self.row_source,
            source_kind,
        )
    }

    fn json_value_to_scalar_cell(&self, value: &serde_json::Value) -> Result<String> {
        json_value_to_scalar_cell_for_field(
            value,
            self.table_name,
            self.row_number,
            self.field_path,
        )
    }

    fn explicit(&self, value: DataValue, raw: impl Into<String>) -> ResolvedValue {
        ResolvedValue::explicit(self.field_path, value, raw, self.row_source)
    }

    fn defaulted(&self, value: DataValue, raw: impl Into<String>) -> ResolvedValue {
        ResolvedValue::defaulted(self.field_path, value, raw, self.row_source)
    }

    fn computed_empty_object(&self, value: DataValue) -> ResolvedValue {
        ResolvedValue::computed_empty_object(self.field_path, value, self.row_source)
    }
}

fn resolve_active_fragments(
    table_meta: &LinkedTableMeta<'_>,
    context: &ResolveContext<'_>,
    active_fragments: &[String],
    resolved_row: &mut ResolvedRow,
) -> Result<()> {
    for fragment_name in active_fragments {
        let Some(fragment) = table_meta.fragment(fragment_name) else {
            continue;
        };
        if context
            .target_tags
            .is_some_and(|target_tags| !fragment.meta.is_exported_for(target_tags))
        {
            continue;
        }
        if let Some(struct_type) = fragment.struct_type() {
            let fragment_context = context.value(fragment.meta.name.as_str());
            let value = resolve_struct_occurrence_value(fragment_context, struct_type)?
                .unwrap_or_else(|| {
                    fragment_context.computed_empty_object(DataValue::Struct(BTreeMap::new()))
                });
            resolved_row.insert(fragment.meta.name.clone(), value);
            continue;
        }

        let mut fragment_object = BTreeMap::new();
        for field in fragment
            .fields
            .iter()
            .filter(|field| context.field_is_visible(field))
        {
            let field_path =
                fragment_field_key(fragment.meta.name.as_str(), field.meta.name.as_str());
            if let Some(value) = resolve_field_value(context.value(&field_path), field)? {
                fragment_object.insert(field.meta.name.clone(), value);
            }
        }
        resolved_row.insert(
            fragment.meta.name.clone(),
            context
                .value(fragment.meta.name.as_str())
                .computed_empty_object(DataValue::Struct(fragment_object)),
        );
    }
    Ok(())
}

fn resolve_field_value(
    context: ValueContext<'_, '_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<Option<ResolvedValue>> {
    match &field.linked_field {
        LinkedField::Struct(struct_type) => resolve_struct_occurrence_value(context, struct_type),
        LinkedField::List(element_type) => resolve_list_cell_value(context, element_type, field),
        LinkedField::Primitive(PrimitiveFieldType::Color) => {
            resolve_color_cell_value(context, field)
        }
        LinkedField::Primitive(
            PrimitiveFieldType::Vector2
            | PrimitiveFieldType::Vector3
            | PrimitiveFieldType::Vector4
            | PrimitiveFieldType::Vector2Int
            | PrimitiveFieldType::Vector3Int
            | PrimitiveFieldType::Vector4Int,
        ) => resolve_vector_cell_value(context, field),
        _ => resolve_scalar_cell_value(context, field),
    }
}

fn resolve_color_cell_value(
    context: ValueContext<'_, '_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<Option<ResolvedValue>> {
    if let Some(raw) = non_empty_row_value(context.row, context.field_path) {
        let color = context.parse_color_field(field, raw)?;
        return Ok(Some(context.explicit(DataValue::Color(color), raw)));
    }

    let Some(default) = field.meta.default.as_ref() else {
        return Ok(None);
    };
    let default_value = toml_default_to_json_value(default);
    if json_value_is_empty(&default_value) {
        return Ok(None);
    }
    let raw = json_value_to_cell(&default_value).map_err(|_| {
        resolver_value_error(
            context,
            CF_DATA_INVALID_TYPE,
            format!(
                "table `{}` row {} field `{}` must be a color cell value",
                context.table_name, context.row_number, context.field_path
            ),
            default_value.to_string(),
            "string, numeric array, or object color value",
            "Use a default value that matches the field color_format.",
        )
    })?;
    let color = context.parse_color_field(field, &raw)?;
    Ok(Some(context.defaulted(DataValue::Color(color), raw)))
}

fn resolve_vector_cell_value(
    context: ValueContext<'_, '_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<Option<ResolvedValue>> {
    if let Some(raw) = non_empty_row_value(context.row, context.field_path) {
        if let Some(expanded_path) = first_vector_component_cell(context, field) {
            return Err(resolver_value_error(
                context,
                CF_DATA_SOURCE_SCHEMA_INVALID,
                format!(
                    "table `{}` row {} field `{}` mixes vector cell with expanded vector component path `{}`",
                    context.table_name, context.row_number, context.field_path, expanded_path
                ),
                format!("{} and {}", context.field_path, expanded_path),
                "either one vector cell or expanded vector component cells",
                "Use only one vector source layout for the field in the same row.",
            ));
        }
        let vector = context.parse_vector_field(field, raw)?;
        return Ok(Some(context.explicit(DataValue::Vector(vector), raw)));
    }

    if field.meta.vector_format == Some(VectorFormat::Expanded) {
        if let Some((raw, paths)) = expanded_vector_raw_value(context, field)? {
            let vector = context.parse_vector_field(field, &raw)?;
            return Ok(Some(
                context.explicit(DataValue::Vector(vector), paths.join(",")),
            ));
        }
    }

    let Some(default) = field.meta.default.as_ref() else {
        return Ok(None);
    };
    let default_value = toml_default_to_json_value(default);
    if json_value_is_empty(&default_value) {
        return Ok(None);
    }
    let raw = json_value_to_cell(&default_value).map_err(|_| {
        resolver_value_error(
            context,
            CF_DATA_INVALID_TYPE,
            format!(
                "table `{}` row {} field `{}` must be a vector cell value",
                context.table_name, context.row_number, context.field_path
            ),
            default_value.to_string(),
            "string, numeric array, or object vector value",
            "Use a default value that matches the field vector_format.",
        )
    })?;
    let vector = context.parse_vector_field(field, &raw)?;
    Ok(Some(context.defaulted(DataValue::Vector(vector), raw)))
}

fn resolve_list_cell_value(
    context: ValueContext<'_, '_>,
    element_type: &LinkedField<'_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<Option<ResolvedValue>> {
    let indexed_cells = indexed_list_cells(context.row, context.field_path)?;
    if let Some(raw) = non_empty_row_value(context.row, context.field_path) {
        if let Some((_, _, first_indexed_path)) = indexed_cells.first() {
            return Err(resolver_value_error(
                context,
                CF_DATA_SOURCE_SCHEMA_INVALID,
                format!(
                    "table `{}` row {} field `{}` mixes list JSON cell with expanded list element path `{}`",
                    context.table_name, context.row_number, context.field_path, first_indexed_path
                ),
                format!("{} and {}", context.field_path, first_indexed_path),
                "either one list JSON cell or expanded list element cells",
                "Use only one list source layout for the field in the same row.",
            ));
        }
        let array = context.parse_list_cell(element_type, raw, ValueSourceKind::ExplicitCell)?;
        return Ok(Some(context.explicit(DataValue::Array(array), raw)));
    }

    if !indexed_cells.is_empty() {
        let array = parse_indexed_list_cells(context, element_type, &indexed_cells)?;
        let raw = indexed_cells
            .iter()
            .map(|(_, _, path)| path.as_str())
            .collect::<Vec<_>>()
            .join(",");
        return Ok(Some(context.explicit(DataValue::Array(array), raw)));
    }

    let Some(default) = field.meta.default.as_ref() else {
        return Ok(None);
    };
    let default_value = toml_default_to_json_value(default);
    if json_value_is_empty(&default_value) {
        return Ok(None);
    }
    let raw = json_value_to_cell(&default_value).map_err(|_| {
        resolver_value_error(
            context,
            CF_DATA_INVALID_TYPE,
            format!(
                "table `{}` row {} field `{}` must be a JSON array",
                context.table_name, context.row_number, context.field_path
            ),
            default_value.to_string(),
            "JSON array",
            "Use JSON array syntax for list default values.",
        )
    })?;
    let array = context.parse_list_cell(element_type, &raw, ValueSourceKind::DefaultValue)?;
    Ok(Some(context.defaulted(DataValue::Array(array), raw)))
}

fn parse_indexed_list_cells(
    context: ValueContext<'_, '_>,
    element_type: &LinkedField<'_>,
    indexed_cells: &[(usize, String, String)],
) -> Result<Vec<ArrayElementValue>> {
    indexed_cells
        .iter()
        .enumerate()
        .map(|(expected_index, (index, raw, element_path))| {
            if *index != expected_index {
                let expected_path = list_element_path(context.field_path, expected_index);
                return Err(resolver_value_error(
                    context,
                    CF_DATA_SOURCE_SCHEMA_INVALID,
                    format!(
                        "table `{}` row {} field `{}` has non-contiguous expanded list element path `{}`; expected `{}`",
                        context.table_name,
                        context.row_number,
                        context.field_path,
                        element_path,
                        expected_path
                    ),
                    element_path.as_str(),
                    expected_path,
                    "Expanded list element paths must start at 0 and be contiguous.",
                ));
            }
            let scalar = parse_scalar_for_type(
                context.table_name,
                context.row_number,
                element_path,
                element_type,
                raw,
            )?;
            Ok(ArrayElementValue::scalar(scalar, Some(raw.clone())))
        })
        .collect()
}

fn resolve_scalar_cell_value(
    context: ValueContext<'_, '_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<Option<ResolvedValue>> {
    if let Some(raw) = non_empty_row_value(context.row, context.field_path) {
        let scalar = context.parse_scalar_field(field, raw)?;
        return Ok(Some(context.explicit(DataValue::Scalar(scalar), raw)));
    }

    let Some(default) = field.meta.default.as_ref() else {
        return Ok(None);
    };
    let default_value = toml_default_to_json_value(default);
    if json_value_is_empty(&default_value) {
        return Ok(None);
    }
    let raw = context.json_value_to_scalar_cell(&default_value)?;
    let scalar = context.parse_scalar_field(field, &raw)?;
    Ok(Some(context.defaulted(DataValue::Scalar(scalar), raw)))
}

fn resolve_struct_occurrence_value(
    context: ValueContext<'_, '_>,
    struct_type: &LinkedStructType<'_>,
) -> Result<Option<ResolvedValue>> {
    if let Some(raw) = non_empty_row_value(context.row, context.field_path) {
        let value: serde_json::Value = serde_json::from_str(raw).map_err(|source| {
            resolver_value_error(
                context,
                CF_DATA_INVALID_TYPE,
                format!(
                    "table `{}` row {} field `{}` has invalid struct JSON: {}",
                    context.table_name, context.row_number, context.field_path, source
                ),
                raw,
                format!("JSON object for struct `{}`", struct_type.meta.name),
                "Use JSON object syntax or expanded struct child columns, not malformed JSON.",
            )
        })?;
        let children = resolve_struct_json_value(context, struct_type, &value)?;
        return Ok(Some(context.explicit(DataValue::Struct(children), raw)));
    }

    if !struct_occurrence_has_effective_value(context, struct_type)? {
        return Ok(None);
    }

    let mut children = BTreeMap::new();
    for child in struct_type
        .fields
        .iter()
        .filter(|child| field_is_visible(child, context.target_tags))
    {
        let child_path = format!("{}.{}", context.field_path, child.meta.name);
        if let Some(value) = resolve_field_value(context.at(&child_path), child)? {
            children.insert(child.meta.name.clone(), value);
        }
    }

    Ok(Some(
        context.computed_empty_object(DataValue::Struct(children)),
    ))
}

fn resolve_struct_json_value(
    context: ValueContext<'_, '_>,
    struct_type: &LinkedStructType<'_>,
    value: &serde_json::Value,
) -> Result<BTreeMap<String, ResolvedValue>> {
    let object = value.as_object().ok_or_else(|| {
        resolver_value_error(
            context,
            CF_DATA_INVALID_TYPE,
            format!(
                "table `{}` row {} field `{}` must be a JSON object for struct `{}`",
                context.table_name, context.row_number, context.field_path, struct_type.meta.name
            ),
            value.to_string(),
            format!("JSON object for struct `{}`", struct_type.meta.name),
            "Use JSON object syntax for struct field values.",
        )
    })?;
    for key in object.keys() {
        if struct_child(struct_type, key).is_none() {
            return Err(resolver_value_error(
                context,
                CF_DATA_SOURCE_SCHEMA_INVALID,
                format!(
                    "table `{}` row {} field `{}` contains unknown struct key `{}`",
                    context.table_name, context.row_number, context.field_path, key
                ),
                key.as_str(),
                format!("known child of struct `{}`", struct_type.meta.name),
                "Remove the unknown key or declare it as a struct field in Meta.",
            ));
        }
    }

    let mut children = BTreeMap::new();
    for child in struct_type
        .fields
        .iter()
        .filter(|child| field_is_visible(child, context.target_tags))
    {
        let child_path = format!("{}.{}", context.field_path, child.meta.name);
        let child_context = context.at(&child_path);
        let default_value = child.meta.default.as_ref().map(toml_default_to_json_value);
        let Some(child_value) = object.get(&child.meta.name).or(default_value.as_ref()) else {
            continue;
        };
        if json_value_is_empty(child_value) {
            continue;
        }
        let source_is_default = !object.contains_key(&child.meta.name) && default_value.is_some();
        let resolved_value = match &child.linked_field {
            LinkedField::Struct(nested_struct) => {
                let children =
                    resolve_struct_json_value(child_context, nested_struct, child_value)?;
                let value = DataValue::Struct(children);
                if source_is_default {
                    child_context.defaulted(value, child_value.to_string())
                } else {
                    child_context.explicit(value, child_value.to_string())
                }
            }
            LinkedField::List(element_type) => {
                let raw = json_value_to_cell(child_value).map_err(|_| {
                    resolver_value_error(
                        child_context,
                        CF_DATA_INVALID_TYPE,
                        format!(
                            "table `{}` row {} field `{}` must be a JSON array",
                            context.table_name, context.row_number, child_path
                        ),
                        child_value.to_string(),
                        "JSON array",
                        "Use JSON array syntax for list field values.",
                    )
                })?;
                let array = child_context.parse_list_cell(
                    element_type,
                    &raw,
                    if source_is_default {
                        ValueSourceKind::DefaultValue
                    } else {
                        ValueSourceKind::ExplicitCell
                    },
                )?;
                if source_is_default {
                    child_context.defaulted(DataValue::Array(array), raw)
                } else {
                    child_context.explicit(DataValue::Array(array), raw)
                }
            }
            LinkedField::Primitive(PrimitiveFieldType::Color) => {
                let raw = json_value_to_cell(child_value)?;
                let color = child_context.parse_color_field(child, &raw)?;
                if source_is_default {
                    child_context.defaulted(DataValue::Color(color), raw)
                } else {
                    child_context.explicit(DataValue::Color(color), raw)
                }
            }
            LinkedField::Primitive(
                PrimitiveFieldType::Vector2
                | PrimitiveFieldType::Vector3
                | PrimitiveFieldType::Vector4
                | PrimitiveFieldType::Vector2Int
                | PrimitiveFieldType::Vector3Int
                | PrimitiveFieldType::Vector4Int,
            ) => {
                let raw = json_value_to_cell(child_value)?;
                let vector = child_context.parse_vector_field(child, &raw)?;
                if source_is_default {
                    child_context.defaulted(DataValue::Vector(vector), raw)
                } else {
                    child_context.explicit(DataValue::Vector(vector), raw)
                }
            }
            _ => {
                let raw = child_context.json_value_to_scalar_cell(child_value)?;
                let scalar = child_context.parse_scalar_field(child, &raw)?;
                if source_is_default {
                    child_context.defaulted(DataValue::Scalar(scalar), raw)
                } else {
                    child_context.explicit(DataValue::Scalar(scalar), raw)
                }
            }
        };
        children.insert(child.meta.name.clone(), resolved_value);
    }
    Ok(children)
}

fn struct_occurrence_has_effective_value(
    context: ValueContext<'_, '_>,
    struct_type: &LinkedStructType<'_>,
) -> Result<bool> {
    if non_empty_row_value(context.row, context.field_path).is_some() {
        return Ok(true);
    }

    for child in struct_type
        .fields
        .iter()
        .filter(|child| field_is_visible(child, context.target_tags))
    {
        let child_path = format!("{}.{}", context.field_path, child.meta.name);
        let child_context = context.at(&child_path);
        match &child.linked_field {
            LinkedField::Struct(nested_struct) => {
                if struct_occurrence_has_effective_value(child_context, nested_struct)? {
                    return Ok(true);
                }
            }
            LinkedField::List(element_type) => {
                if non_empty_row_value(context.row, &child_path).is_some()
                    || !indexed_list_cells(context.row, &child_path)?.is_empty()
                {
                    return Ok(true);
                }
                let Some(default) = child.meta.default.as_ref() else {
                    continue;
                };
                let default_value = toml_default_to_json_value(default);
                if json_value_is_empty(&default_value) {
                    continue;
                }
                let raw = json_value_to_cell(&default_value).map_err(|_| {
                    resolver_value_error(
                        child_context,
                        CF_DATA_INVALID_TYPE,
                        format!(
                            "table `{}` row {} field `{}` must be a JSON array",
                            context.table_name, context.row_number, child_path
                        ),
                        default_value.to_string(),
                        "JSON array",
                        "Use JSON array syntax for list default values.",
                    )
                })?;
                child_context.parse_list_cell(element_type, &raw, ValueSourceKind::DefaultValue)?;
                return Ok(true);
            }
            _ => {
                if non_empty_row_value(context.row, &child_path).is_some()
                    || first_vector_component_cell(child_context, child).is_some()
                {
                    return Ok(true);
                }
                let Some(default) = child.meta.default.as_ref() else {
                    continue;
                };
                let default_value = toml_default_to_json_value(default);
                if json_value_is_empty(&default_value) {
                    continue;
                }
                let raw = match child.primitive_type() {
                    Some(PrimitiveFieldType::Color)
                    | Some(PrimitiveFieldType::Vector2)
                    | Some(PrimitiveFieldType::Vector3)
                    | Some(PrimitiveFieldType::Vector4)
                    | Some(PrimitiveFieldType::Vector2Int)
                    | Some(PrimitiveFieldType::Vector3Int)
                    | Some(PrimitiveFieldType::Vector4Int) => json_value_to_cell(&default_value)?,
                    _ => child_context.json_value_to_scalar_cell(&default_value)?,
                };
                match child.primitive_type() {
                    Some(PrimitiveFieldType::Color) => {
                        child_context.parse_color_field(child, &raw)?;
                    }
                    Some(
                        PrimitiveFieldType::Vector2
                        | PrimitiveFieldType::Vector3
                        | PrimitiveFieldType::Vector4
                        | PrimitiveFieldType::Vector2Int
                        | PrimitiveFieldType::Vector3Int
                        | PrimitiveFieldType::Vector4Int,
                    ) => {
                        child_context.parse_vector_field(child, &raw)?;
                    }
                    _ => {
                        child_context.parse_scalar_field(child, &raw)?;
                    }
                }
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn validate_source_schema(
    meta: &LinkedTableMeta<'_>,
    context: &ResolveContext<'_>,
    active_fragments: &[String],
) -> Result<()> {
    for key in context.row.keys().filter(|key| key.contains('.')) {
        validate_dotted_source_key(meta, key, context, active_fragments)?;
    }
    if meta.meta.kind == TableKind::Object {
        for key in context.row.keys().filter(|key| !key.contains('.')) {
            if validate_top_level_source_key(meta, key) {
                continue;
            }
            if meta.field(key).is_none() {
                return Err(resolver_schema_error(
                    meta.meta.name.as_str(),
                    context.row_number,
                    Some(key),
                    context.row_source,
                    format!(
                        "object table `{}` row {} references unknown field key `{}`",
                        meta.meta.name, context.row_number, key
                    ),
                    key.as_str(),
                    "declared object table field key",
                    "Remove the unknown key or declare the field in Meta.",
                ));
            }
        }
    }

    for field in &meta.fields {
        if let LinkedField::Struct(struct_type) = &field.linked_field {
            validate_struct_layout_mixing_for_row(
                meta.meta.name.as_str(),
                context.row,
                context.row_number,
                context.row_source,
                field.meta.name.as_str(),
                struct_type,
            )?;
        }
    }
    for fragment_name in active_fragments {
        let Some(fragment) = meta.fragment(fragment_name) else {
            continue;
        };
        let Some(struct_type) = fragment.struct_type() else {
            continue;
        };
        validate_struct_layout_mixing_for_row(
            meta.meta.name.as_str(),
            context.row,
            context.row_number,
            context.row_source,
            fragment.meta.name.as_str(),
            struct_type,
        )?;
    }
    Ok(())
}

fn validate_top_level_source_key(meta: &LinkedTableMeta<'_>, key: &str) -> bool {
    let Some((field_name, _)) = parse_list_element_path(key) else {
        return false;
    };
    meta.field(field_name)
        .is_some_and(|field| matches!(field.linked_field, LinkedField::List(_)))
}

fn validate_dotted_source_key(
    meta: &LinkedTableMeta<'_>,
    key: &str,
    context: &ResolveContext<'_>,
    active_fragments: &[String],
) -> Result<()> {
    let Some((first_segment, rest)) = key.split_once('.') else {
        return Ok(());
    };

    if let Some(field) = meta.field(first_segment) {
        if is_expanded_vector_component(field, rest) {
            return Ok(());
        }
        let LinkedField::Struct(struct_type) = &field.linked_field else {
            return Err(resolver_schema_error(
                meta.meta.name.as_str(),
                context.row_number,
                Some(key),
                context.row_source,
                format!(
                    "table `{}` row {} field `{}` uses child path but `{}` is not a struct field",
                    meta.meta.name, context.row_number, key, first_segment
                ),
                first_segment,
                "struct field",
                "Only struct fields can use expanded child paths.",
            ));
        };
        validate_struct_child_path(
            meta.meta.name.as_str(),
            context.row_number,
            key,
            context.row_source,
            struct_type,
            rest,
        )?;
        return Ok(());
    }

    if let Some(fragment) = meta.fragment(first_segment) {
        if !active_fragments
            .iter()
            .any(|active| active == first_segment)
        {
            return Err(resolver_schema_error(
                meta.meta.name.as_str(),
                context.row_number,
                Some(key),
                context.row_source,
                format!(
                    "table `{}` row {} field `{}` belongs to inactive fragment `{}`",
                    meta.meta.name, context.row_number, key, first_segment
                ),
                first_segment,
                "active source fragment",
                "Ensure the source row declares the fragment before using its fields.",
            ));
        }
        if let Some(struct_type) = fragment.struct_type() {
            validate_struct_child_path(
                meta.meta.name.as_str(),
                context.row_number,
                key,
                context.row_source,
                struct_type,
                rest,
            )?;
        } else if fragment.field(rest).is_none() {
            return Err(resolver_schema_error(
                meta.meta.name.as_str(),
                context.row_number,
                Some(key),
                context.row_source,
                format!(
                    "table `{}` row {} field `{}` references unknown fragment field `{}`",
                    meta.meta.name, context.row_number, key, rest
                ),
                rest,
                format!("declared field of fragment `{first_segment}`"),
                "Remove the unknown fragment field or declare it in Meta.",
            ));
        }
        return Ok(());
    }

    Err(resolver_schema_error(
        meta.meta.name.as_str(),
        context.row_number,
        Some(key),
        context.row_source,
        format!(
            "table `{}` row {} field `{}` references unknown dotted field path",
            meta.meta.name, context.row_number, key
        ),
        key,
        "declared struct field path or active fragment field path",
        "Check the field path spelling and active fragment configuration.",
    ))
}

fn validate_struct_child_path(
    table_name: &str,
    row_number: usize,
    full_path: &str,
    row_source: Option<&RowSourceInfo>,
    struct_type: &LinkedStructType<'_>,
    path: &str,
) -> Result<()> {
    let Some((segment, rest)) = path.split_once('.') else {
        if struct_child(struct_type, path).is_some() {
            return Ok(());
        }
        if let Some((list_field_name, _)) = parse_list_element_path(path) {
            if struct_child(struct_type, list_field_name)
                .is_some_and(|field| matches!(field.linked_field, LinkedField::List(_)))
            {
                return Ok(());
            }
        }
        return Err(resolver_schema_error(
            table_name,
            row_number,
            Some(full_path),
            row_source,
            format!(
                "table `{}` row {} field `{}` references unknown struct child `{}`",
                table_name, row_number, full_path, path
            ),
            path,
            format!("declared child of struct `{}`", struct_type.meta.name),
            "Remove the unknown child path or declare it in the struct Meta.",
        ));
    };

    let Some(child) = struct_child(struct_type, segment) else {
        return Err(resolver_schema_error(
            table_name,
            row_number,
            Some(full_path),
            row_source,
            format!(
                "table `{}` row {} field `{}` references unknown struct child `{}`",
                table_name, row_number, full_path, segment
            ),
            segment,
            format!("declared child of struct `{}`", struct_type.meta.name),
            "Remove the unknown child path or declare it in the struct Meta.",
        ));
    };
    if is_expanded_vector_component(child, rest) {
        return Ok(());
    }
    let LinkedField::Struct(nested_struct) = &child.linked_field else {
        return Err(resolver_schema_error(
            table_name,
            row_number,
            Some(full_path),
            row_source,
            format!(
                "table `{}` row {} field `{}` uses child path but `{}` is not a struct field",
                table_name, row_number, full_path, segment
            ),
            segment,
            "struct field",
            "Only struct fields can use nested expanded child paths.",
        ));
    };
    validate_struct_child_path(
        table_name,
        row_number,
        full_path,
        row_source,
        nested_struct,
        rest,
    )
}

fn validate_struct_layout_mixing_for_row(
    table_name: &str,
    row: &DataRow,
    row_number: usize,
    row_source: Option<&RowSourceInfo>,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
) -> Result<()> {
    let parent_present = row.contains_key(field_path);
    let descendant_path = first_descendant_path(row, field_path);
    if parent_present {
        if let Some(descendant_path) = descendant_path {
            return Err(resolver_schema_error(
                table_name,
                row_number,
                Some(field_path),
                row_source,
                format!(
                    "table `{}` row {} field `{}` mixes struct JSON cell with expanded child path `{}`",
                    table_name, row_number, field_path, descendant_path
                ),
                format!("{} and {}", field_path, descendant_path),
                "either one struct JSON cell or expanded child fields",
                "Use only one struct source layout for the field in the same row.",
            ));
        }
        return Ok(());
    }

    if descendant_path.is_none() {
        return Ok(());
    }

    for child in &struct_type.fields {
        if let LinkedField::Struct(nested_struct) = &child.linked_field {
            let child_path = format!("{field_path}.{}", child.meta.name);
            validate_struct_layout_mixing_for_row(
                table_name,
                row,
                row_number,
                row_source,
                &child_path,
                nested_struct,
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
fn resolver_table_error(
    table_name: &str,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    resolver_diagnostic_error(
        CF_DATA_SOURCE_SCHEMA_INVALID,
        message,
        resolver_location(table_name, None, None, None),
        current_value,
        expected,
        hint,
    )
}

fn resolver_row_error(
    table_name: &str,
    row_number: usize,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    resolver_diagnostic_error(
        CF_DATA_SOURCE_SCHEMA_INVALID,
        message,
        resolver_location(table_name, Some(row_number), None, None),
        current_value,
        expected,
        hint,
    )
}

fn resolver_value_error(
    context: ValueContext<'_, '_>,
    code: &'static str,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    resolver_diagnostic_error(
        code,
        message,
        resolver_location(
            context.table_name,
            Some(context.row_number),
            Some(context.field_path),
            context.row_source,
        ),
        current_value,
        expected,
        hint,
    )
}

fn resolver_schema_error(
    table_name: &str,
    row_number: usize,
    field_path: Option<&str>,
    row_source: Option<&RowSourceInfo>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    resolver_diagnostic_error(
        CF_DATA_SOURCE_SCHEMA_INVALID,
        message,
        resolver_location(table_name, Some(row_number), field_path, row_source),
        current_value,
        expected,
        hint,
    )
}

fn resolver_diagnostic_error(
    code: &'static str,
    message: String,
    location: DiagnosticLocation,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(code, DiagnosticSeverity::Error, message, location)
            .with_source("data.resolver")
            .with_domain("data")
            .with_current_value(current_value)
            .with_expected(expected)
            .with_hint(hint),
    )
}

fn resolver_location(
    table_name: &str,
    row_number: Option<usize>,
    field_path: Option<&str>,
    row_source: Option<&RowSourceInfo>,
) -> DiagnosticLocation {
    DiagnosticLocation {
        source_path: row_source.map(|source| source.source_path.clone()),
        table: Some(table_name.to_owned()),
        field: field_path.map(str::to_owned),
        source_row_number: row_source
            .map(|source| source.source_row_number)
            .or(row_number),
        ..DiagnosticLocation::default()
    }
}

fn field_is_visible(field: &LinkedFieldMeta<'_>, target_tags: Option<&TargetTags>) -> bool {
    target_tags
        .map(|target_tags| field.meta.is_exported_for(target_tags))
        .unwrap_or(true)
}

fn struct_child<'meta>(
    struct_type: &'meta LinkedStructType<'meta>,
    name: &str,
) -> Option<&'meta LinkedFieldMeta<'meta>> {
    struct_type
        .fields
        .iter()
        .find(|field| field.meta.name == name)
}

fn expanded_vector_raw_value(
    context: ValueContext<'_, '_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<Option<(String, Vec<String>)>> {
    let components = field.meta.vector_component_names();
    let mut raw_values = Vec::with_capacity(components.len());
    let mut paths = Vec::with_capacity(components.len());
    let mut has_component = false;
    for component in &components {
        let path = format!("{}.{}", context.field_path, component);
        paths.push(path.clone());
        if let Some(raw) = non_empty_row_value(context.row, &path) {
            has_component = true;
            raw_values.push(raw.to_owned());
        } else {
            raw_values.push(String::new());
        }
    }
    if !has_component {
        return Ok(None);
    }
    if let Some((missing_index, missing_path)) = raw_values
        .iter()
        .zip(paths.iter())
        .enumerate()
        .find_map(|(index, (raw, path))| raw.is_empty().then_some((index, path)))
    {
        return Err(resolver_value_error(
            context,
            CF_DATA_INVALID_TYPE,
            format!(
                "table `{}` row {} field `{}` has incomplete expanded vector; missing component `{}`",
                context.table_name,
                context.row_number,
                context.field_path,
                components[missing_index]
            ),
            missing_path.as_str(),
            format!("non-empty expanded vector component `{missing_path}`"),
            "Provide all vector component columns or leave the vector field empty.",
        ));
    }
    Ok(Some((raw_values.join(","), paths)))
}

fn first_vector_component_cell(
    context: ValueContext<'_, '_>,
    field: &LinkedFieldMeta<'_>,
) -> Option<String> {
    if field.meta.vector_format != Some(VectorFormat::Expanded) {
        return None;
    }
    field
        .meta
        .vector_component_names()
        .into_iter()
        .find_map(|component| {
            let path = format!("{}.{}", context.field_path, component);
            non_empty_row_value(context.row, &path).map(|_| path)
        })
}

fn is_expanded_vector_component(field: &LinkedFieldMeta<'_>, component: &str) -> bool {
    field.meta.vector_format == Some(VectorFormat::Expanded)
        && field
            .primitive_type()
            .and_then(|primitive| primitive.vector_dimension())
            .is_some()
        && !component.contains('.')
        && field
            .meta
            .vector_component_names()
            .iter()
            .any(|name| name == component)
}

fn first_descendant_path<'a>(row: &'a DataRow, field_path: &str) -> Option<&'a str> {
    let prefix = format!("{field_path}.");
    row.keys()
        .find(|key| key.starts_with(&prefix))
        .map(String::as_str)
}

fn non_empty_row_value<'a>(row: &'a DataRow, field_path: &str) -> Option<&'a str> {
    row.get(field_path)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn indexed_list_cells(row: &DataRow, field_path: &str) -> Result<Vec<(usize, String, String)>> {
    let mut cells = row
        .iter()
        .filter_map(|(key, raw)| {
            list_index_for_field_path(key, field_path)
                .map(|index| (index, raw.clone(), key.clone()))
        })
        .collect::<Vec<_>>();
    cells.sort_by_key(|(index, _, _)| *index);
    Ok(cells)
}

fn list_index_for_field_path(key: &str, field_path: &str) -> Option<usize> {
    let suffix = key.strip_prefix(field_path)?.strip_prefix('[')?;
    let index_text = suffix.strip_suffix(']')?;
    if index_text.is_empty()
        || !index_text
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return None;
    }
    index_text.parse().ok()
}

fn parse_list_element_path(path: &str) -> Option<(&str, usize)> {
    let open = path.rfind('[')?;
    let close = path.strip_suffix(']')?;
    let field_path = &path[..open];
    let index_text = &close[open + 1..];
    if field_path.is_empty()
        || index_text.is_empty()
        || !index_text
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return None;
    }
    Some((field_path, index_text.parse().ok()?))
}

fn list_element_path(field_path: &str, index: usize) -> String {
    format!("{field_path}[{index}]")
}

fn fragment_field_key(fragment_name: &str, field_name: &str) -> String {
    format!("{fragment_name}.{field_name}")
}
