// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Result,
    data::value_conversion::{
        json_value_is_empty, json_value_to_scalar_cell_for_field, toml_default_to_json_value,
    },
    meta::{
        LinkedField, LinkedFieldMeta, LinkedMetaModel, LinkedStructType, LinkedTableMeta,
        PrimitiveFieldType, TableKind, TargetTags,
    },
};

use super::{
    DataRow, DataSet, DataTable, DataValue, ResolvedDataSet, ResolvedRow, ResolvedTable,
    ResolvedValue, RowSourceInfo, ScalarValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructSourceLayout {
    JsonCell,
    ExpandedValue,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructOccurrenceLayout {
    pub parent_path: String,
    pub layout: StructSourceLayout,
}

pub struct DataResolver<'meta> {
    meta: &'meta LinkedMetaModel<'meta>,
    target_tags: Option<TargetTags>,
}

impl<'meta> DataResolver<'meta> {
    pub fn new(meta: &'meta LinkedMetaModel<'meta>) -> Self {
        Self {
            meta,
            target_tags: None,
        }
    }

    pub fn for_target(meta: &'meta LinkedMetaModel<'meta>, target_tags: &TargetTags) -> Self {
        Self {
            meta,
            target_tags: Some(target_tags.clone()),
        }
    }

    pub fn resolve(&self, data_set: &DataSet) -> Result<ResolvedDataSet> {
        let mut tables = Vec::with_capacity(data_set.tables().len());
        for table in data_set.tables() {
            let table_meta = self.meta.table(&table.name).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "data set contains table `{}` missing from linked meta",
                    table.name
                ))
            })?;
            tables.push(Self::resolve_table(
                table_meta,
                table,
                self.target_tags.as_ref(),
            )?);
        }
        ResolvedDataSet::try_new(tables)
    }

    pub fn resolve_required_tables(&self, data_set: &DataSet) -> Result<ResolvedDataSet> {
        let mut tables = Vec::with_capacity(self.meta.tables.len());
        for table_meta in &self.meta.tables {
            let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "data set is missing table `{}`",
                    table_meta.meta.name
                ))
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
                ConfigForgeError::Validation(format!(
                    "table `{}` is missing row source for row {}",
                    table.name,
                    row_index + 1
                ))
            })?;
            row_sources.push(row_source);
        }

        ResolvedTable::new(table.name.clone(), rows, row_sources)
    }

    pub fn validate_table_source_schema(
        table_meta: &LinkedTableMeta<'meta>,
        table: &DataTable,
    ) -> Result<()> {
        for (row_index, row) in table.rows.iter().enumerate() {
            let row_source = table.row_source(row_index);
            let row_number = row_source
                .map(|source| source.source_row_number)
                .unwrap_or(row_index + 1);
            let active_fragments = row_source
                .map(|source| source.fragments.as_slice())
                .unwrap_or(&[]);
            validate_source_schema(table_meta, row, row_number, active_fragments)?;
        }
        Ok(())
    }
}

fn resolve_row(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: Option<&TargetTags>,
    row_index: usize,
) -> Result<ResolvedRow> {
    let row = table.rows.get(row_index).ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "table `{}` is missing row {}",
            table.name,
            row_index + 1
        ))
    })?;
    let row_source = table.row_source(row_index);
    let row_number = row_source
        .map(|source| source.source_row_number)
        .unwrap_or(row_index + 1);
    let active_fragments = row_source
        .map(|source| source.fragments.as_slice())
        .unwrap_or(&[]);
    validate_source_schema(table_meta, row, row_number, active_fragments)?;

    let mut resolved_row = ResolvedRow::new();
    for field in table_meta
        .fields
        .iter()
        .filter(|field| field_is_visible(field, target_tags))
    {
        if let Some(value) = resolve_field_value(
            table_meta.meta.name.as_str(),
            row,
            row_source,
            row_number,
            field.meta.name.as_str(),
            field,
            target_tags,
        )? {
            resolved_row.insert(field.meta.name.clone(), value);
        }
    }

    if table_meta.meta.kind == TableKind::Collection {
        resolve_active_fragments(
            table_meta,
            row,
            row_source,
            row_number,
            active_fragments,
            target_tags,
            &mut resolved_row,
        )?;
    }

    Ok(resolved_row)
}

fn resolve_active_fragments(
    table_meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    row_source: Option<&RowSourceInfo>,
    row_number: usize,
    active_fragments: &[String],
    target_tags: Option<&TargetTags>,
    resolved_row: &mut ResolvedRow,
) -> Result<()> {
    for fragment_name in active_fragments {
        let Some(fragment) = table_meta.fragment(fragment_name) else {
            continue;
        };
        if target_tags.is_some_and(|target_tags| !fragment.meta.is_exported_for(target_tags)) {
            continue;
        }
        if let Some(struct_type) = fragment.struct_type() {
            let value = resolve_struct_occurrence_value(
                table_meta.meta.name.as_str(),
                row,
                row_source,
                row_number,
                fragment.meta.name.as_str(),
                struct_type,
                target_tags,
            )?
            .unwrap_or_else(|| {
                ResolvedValue::computed_empty_object(
                    fragment.meta.name.clone(),
                    DataValue::Struct(BTreeMap::new()),
                    row_source,
                )
            });
            resolved_row.insert(fragment.meta.name.clone(), value);
            continue;
        }

        let mut fragment_object = BTreeMap::new();
        for field in fragment
            .fields
            .iter()
            .filter(|field| field_is_visible(field, target_tags))
        {
            let field_path =
                fragment_field_key(fragment.meta.name.as_str(), field.meta.name.as_str());
            if let Some(value) = resolve_field_value(
                table_meta.meta.name.as_str(),
                row,
                row_source,
                row_number,
                &field_path,
                field,
                target_tags,
            )? {
                fragment_object.insert(field.meta.name.clone(), value);
            }
        }
        resolved_row.insert(
            fragment.meta.name.clone(),
            ResolvedValue::computed_empty_object(
                fragment.meta.name.clone(),
                DataValue::Struct(fragment_object),
                row_source,
            ),
        );
    }
    Ok(())
}

fn resolve_field_value(
    table_name: &str,
    row: &DataRow,
    row_source: Option<&RowSourceInfo>,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    target_tags: Option<&TargetTags>,
) -> Result<Option<ResolvedValue>> {
    match &field.linked_field {
        LinkedField::Struct(struct_type) => resolve_struct_occurrence_value(
            table_name,
            row,
            row_source,
            row_number,
            field_path,
            struct_type,
            target_tags,
        ),
        _ => resolve_scalar_cell_value(table_name, row, row_source, row_number, field_path, field),
    }
}

fn resolve_scalar_cell_value(
    table_name: &str,
    row: &DataRow,
    row_source: Option<&RowSourceInfo>,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
) -> Result<Option<ResolvedValue>> {
    if let Some(raw) = non_empty_row_value(row, field_path) {
        let scalar = parse_scalar_value(table_name, row_number, field_path, field, raw)?;
        return Ok(Some(ResolvedValue::explicit(
            field_path,
            DataValue::Scalar(scalar),
            raw,
            row_source,
        )));
    }

    let Some(default) = field.meta.default.as_ref() else {
        return Ok(None);
    };
    let default_value = toml_default_to_json_value(default);
    if json_value_is_empty(&default_value) {
        return Ok(None);
    }
    let raw =
        json_value_to_scalar_cell_for_field(&default_value, table_name, row_number, field_path)?;
    let scalar = parse_scalar_value(table_name, row_number, field_path, field, &raw)?;
    Ok(Some(ResolvedValue::defaulted(
        field_path,
        DataValue::Scalar(scalar),
        raw,
        row_source,
    )))
}

fn resolve_struct_occurrence_value(
    table_name: &str,
    row: &DataRow,
    row_source: Option<&RowSourceInfo>,
    row_number: usize,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
    target_tags: Option<&TargetTags>,
) -> Result<Option<ResolvedValue>> {
    if let Some(raw) = non_empty_row_value(row, field_path) {
        let value: serde_json::Value = serde_json::from_str(raw).map_err(|source| {
            ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` has invalid struct JSON: {}",
                table_name, row_number, field_path, source
            ))
        })?;
        let children = resolve_struct_json_value(
            table_name,
            row_source,
            row_number,
            field_path,
            struct_type,
            &value,
            target_tags,
        )?;
        return Ok(Some(ResolvedValue::explicit(
            field_path,
            DataValue::Struct(children),
            raw,
            row_source,
        )));
    }

    if !struct_occurrence_has_effective_value(
        row,
        field_path,
        struct_type,
        target_tags,
        table_name,
        row_number,
    )? {
        return Ok(None);
    }

    let mut children = BTreeMap::new();
    for child in struct_type
        .fields
        .iter()
        .filter(|child| field_is_visible(child, target_tags))
    {
        let child_path = format!("{field_path}.{}", child.meta.name);
        if let Some(value) = resolve_field_value(
            table_name,
            row,
            row_source,
            row_number,
            &child_path,
            child,
            target_tags,
        )? {
            children.insert(child.meta.name.clone(), value);
        }
    }

    Ok(Some(ResolvedValue::computed_empty_object(
        field_path,
        DataValue::Struct(children),
        row_source,
    )))
}

fn resolve_struct_json_value(
    table_name: &str,
    row_source: Option<&RowSourceInfo>,
    row_number: usize,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
    value: &serde_json::Value,
    target_tags: Option<&TargetTags>,
) -> Result<BTreeMap<String, ResolvedValue>> {
    let object = value.as_object().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` must be a JSON object for struct `{}`",
            table_name, row_number, field_path, struct_type.meta.name
        ))
    })?;
    for key in object.keys() {
        if struct_child(struct_type, key).is_none() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` contains unknown struct key `{}`",
                table_name, row_number, field_path, key
            )));
        }
    }

    let mut children = BTreeMap::new();
    for child in struct_type
        .fields
        .iter()
        .filter(|child| field_is_visible(child, target_tags))
    {
        let child_path = format!("{field_path}.{}", child.meta.name);
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
                let children = resolve_struct_json_value(
                    table_name,
                    row_source,
                    row_number,
                    &child_path,
                    nested_struct,
                    child_value,
                    target_tags,
                )?;
                let value = DataValue::Struct(children);
                if source_is_default {
                    ResolvedValue::defaulted(
                        &child_path,
                        value,
                        child_value.to_string(),
                        row_source,
                    )
                } else {
                    ResolvedValue::explicit(&child_path, value, child_value.to_string(), row_source)
                }
            }
            _ => {
                let raw = json_value_to_scalar_cell_for_field(
                    child_value,
                    table_name,
                    row_number,
                    &child_path,
                )?;
                let scalar = parse_scalar_value(table_name, row_number, &child_path, child, &raw)?;
                if source_is_default {
                    ResolvedValue::defaulted(
                        &child_path,
                        DataValue::Scalar(scalar),
                        raw,
                        row_source,
                    )
                } else {
                    ResolvedValue::explicit(&child_path, DataValue::Scalar(scalar), raw, row_source)
                }
            }
        };
        children.insert(child.meta.name.clone(), resolved_value);
    }
    Ok(children)
}

fn struct_occurrence_has_effective_value(
    row: &DataRow,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
    target_tags: Option<&TargetTags>,
    table_name: &str,
    row_number: usize,
) -> Result<bool> {
    if non_empty_row_value(row, field_path).is_some() {
        return Ok(true);
    }

    for child in struct_type
        .fields
        .iter()
        .filter(|child| field_is_visible(child, target_tags))
    {
        let child_path = format!("{field_path}.{}", child.meta.name);
        match &child.linked_field {
            LinkedField::Struct(nested_struct) => {
                if struct_occurrence_has_effective_value(
                    row,
                    &child_path,
                    nested_struct,
                    target_tags,
                    table_name,
                    row_number,
                )? {
                    return Ok(true);
                }
            }
            _ => {
                if non_empty_row_value(row, &child_path).is_some() {
                    return Ok(true);
                }
                let Some(default) = child.meta.default.as_ref() else {
                    continue;
                };
                let default_value = toml_default_to_json_value(default);
                if json_value_is_empty(&default_value) {
                    continue;
                }
                let raw = json_value_to_scalar_cell_for_field(
                    &default_value,
                    table_name,
                    row_number,
                    &child_path,
                )?;
                parse_scalar_value(table_name, row_number, &child_path, child, &raw)?;
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn validate_source_schema(
    meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    row_number: usize,
    active_fragments: &[String],
) -> Result<()> {
    for key in row.keys().filter(|key| key.contains('.')) {
        validate_dotted_source_key(meta, key, row_number, active_fragments)?;
    }
    if meta.meta.kind == TableKind::Object {
        for key in row.keys().filter(|key| !key.contains('.')) {
            if meta.field(key).is_none() {
                return Err(ConfigForgeError::Validation(format!(
                    "object table `{}` row {} references unknown field key `{}`",
                    meta.meta.name, row_number, key
                )));
            }
        }
    }

    for field in &meta.fields {
        if let LinkedField::Struct(struct_type) = &field.linked_field {
            validate_struct_layout_mixing_for_row(
                meta.meta.name.as_str(),
                row,
                row_number,
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
            row,
            row_number,
            fragment.meta.name.as_str(),
            struct_type,
        )?;
    }
    Ok(())
}

fn validate_dotted_source_key(
    meta: &LinkedTableMeta<'_>,
    key: &str,
    row_number: usize,
    active_fragments: &[String],
) -> Result<()> {
    let Some((first_segment, rest)) = key.split_once('.') else {
        return Ok(());
    };

    if let Some(field) = meta.field(first_segment) {
        let LinkedField::Struct(struct_type) = &field.linked_field else {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` uses child path but `{}` is not a struct field",
                meta.meta.name, row_number, key, first_segment
            )));
        };
        validate_struct_child_path(meta.meta.name.as_str(), row_number, key, struct_type, rest)?;
        return Ok(());
    }

    if let Some(fragment) = meta.fragment(first_segment) {
        if !active_fragments
            .iter()
            .any(|active| active == first_segment)
        {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` belongs to inactive fragment `{}`",
                meta.meta.name, row_number, key, first_segment
            )));
        }
        if let Some(struct_type) = fragment.struct_type() {
            validate_struct_child_path(
                meta.meta.name.as_str(),
                row_number,
                key,
                struct_type,
                rest,
            )?;
        } else if fragment.field(rest).is_none() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` references unknown fragment field `{}`",
                meta.meta.name, row_number, key, rest
            )));
        }
        return Ok(());
    }

    Err(ConfigForgeError::Validation(format!(
        "table `{}` row {} field `{}` references unknown dotted field path",
        meta.meta.name, row_number, key
    )))
}

fn validate_struct_child_path(
    table_name: &str,
    row_number: usize,
    full_path: &str,
    struct_type: &LinkedStructType<'_>,
    path: &str,
) -> Result<()> {
    let Some((segment, rest)) = path.split_once('.') else {
        if struct_child(struct_type, path).is_some() {
            return Ok(());
        }
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` references unknown struct child `{}`",
            table_name, row_number, full_path, path
        )));
    };

    let Some(child) = struct_child(struct_type, segment) else {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` references unknown struct child `{}`",
            table_name, row_number, full_path, segment
        )));
    };
    let LinkedField::Struct(nested_struct) = &child.linked_field else {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` uses child path but `{}` is not a struct field",
            table_name, row_number, full_path, segment
        )));
    };
    validate_struct_child_path(table_name, row_number, full_path, nested_struct, rest)
}

fn validate_struct_layout_mixing_for_row(
    table_name: &str,
    row: &DataRow,
    row_number: usize,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
) -> Result<()> {
    let parent_present = row.contains_key(field_path);
    let descendant_path = first_descendant_path(row, field_path);
    if parent_present {
        if let Some(descendant_path) = descendant_path {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` mixes struct JSON cell with expanded child path `{}`",
                table_name, row_number, field_path, descendant_path
            )));
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
                &child_path,
                nested_struct,
            )?;
        }
    }

    Ok(())
}

fn parse_scalar_value(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    value: &str,
) -> Result<ScalarValue> {
    match &field.linked_field {
        LinkedField::Primitive(PrimitiveFieldType::Int) => value
            .parse::<i64>()
            .map(ScalarValue::Int)
            .map_err(|_| scalar_type_error(table_name, row_number, field_path, field, value)),
        LinkedField::Primitive(PrimitiveFieldType::Float) => value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(ScalarValue::Float)
            .ok_or_else(|| scalar_type_error(table_name, row_number, field_path, field, value)),
        LinkedField::Primitive(PrimitiveFieldType::Bool) => match value {
            "true" | "1" => Ok(ScalarValue::Bool(true)),
            "false" | "0" => Ok(ScalarValue::Bool(false)),
            _ => Err(scalar_type_error(
                table_name, row_number, field_path, field, value,
            )),
        },
        LinkedField::Primitive(
            PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
        ) => Ok(ScalarValue::String(value.to_owned())),
        LinkedField::Enum(enum_meta) => {
            if enum_meta
                .values
                .iter()
                .any(|enum_value| enum_value.name == value)
            {
                Ok(ScalarValue::Enum(value.to_owned()))
            } else {
                Err(scalar_type_error(
                    table_name, row_number, field_path, field, value,
                ))
            }
        }
        LinkedField::Struct(_) => Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` expected scalar field, got struct type `{}`",
            table_name, row_number, field_path, field.meta.field_type
        ))),
    }
}

fn scalar_type_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    value: &str,
) -> ConfigForgeError {
    ConfigForgeError::Validation(format!(
        "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
        table_name, row_number, field_path, value, field.meta.field_type
    ))
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

fn fragment_field_key(fragment_name: &str, field_name: &str) -> String {
    format!("{fragment_name}.{field_name}")
}
