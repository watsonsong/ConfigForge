// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    data::{
        DataRow,
        value_conversion::{
            json_value_is_empty, json_value_to_scalar_cell_for_field, toml_default_to_json_value,
        },
    },
    meta::{
        LinkedField, LinkedFieldMeta, LinkedStructType, LinkedTableMeta, PrimitiveFieldType,
        TargetTags, ValueLayout,
    },
};

pub(super) fn validate_struct_field_value_for_row(
    meta: &LinkedTableMeta<'_>,
    row: &DataRow,
    row_number: usize,
    field: &LinkedFieldMeta<'_>,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    let LinkedField::Struct(struct_type) = &field.linked_field else {
        return Ok(());
    };
    let has_effective_value = validate_struct_occurrence_value(
        meta.meta.name.as_str(),
        row,
        row_number,
        field.meta.name.as_str(),
        struct_type,
        target_tags,
    )?;
    if field.meta.required
        && !has_effective_value
        && visible_struct_child_count(struct_type, target_tags) > 0
    {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` is required",
            meta.meta.name, row_number, field.meta.name
        )));
    }
    Ok(())
}

pub(super) fn validate_struct_occurrence_value(
    table_name: &str,
    row: &DataRow,
    row_number: usize,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
    target_tags: Option<&TargetTags>,
) -> Result<bool> {
    if let Some(raw) = non_empty_row_value(row, field_path) {
        validate_struct_json_cell(
            table_name,
            row_number,
            field_path,
            struct_type,
            raw,
            target_tags,
        )?;
        return Ok(true);
    }

    let visible_children = visible_struct_child_count(struct_type, target_tags);
    if visible_children == 0 {
        return Ok(false);
    }

    let occurrence_effective = struct_occurrence_has_effective_value(
        row,
        field_path,
        struct_type,
        target_tags,
        table_name,
        row_number,
    )?;
    if !occurrence_effective {
        return Ok(false);
    }

    let mut has_effective_value = false;
    for child in struct_type
        .fields
        .iter()
        .filter(|child| child_is_visible(child, target_tags))
    {
        let child_path = format!("{field_path}.{}", child.meta.name);
        match &child.linked_field {
            LinkedField::Struct(nested_struct) => {
                let child_effective = validate_struct_occurrence_value(
                    table_name,
                    row,
                    row_number,
                    &child_path,
                    nested_struct,
                    target_tags,
                )?;
                if child.meta.required
                    && !child_effective
                    && visible_struct_child_count(nested_struct, target_tags) > 0
                {
                    return Err(ConfigForgeError::Validation(format!(
                        "table `{}` row {} field `{}` is required",
                        table_name, row_number, child_path
                    )));
                }
                has_effective_value |= child_effective;
            }
            _ => {
                let raw = non_empty_row_value(row, &child_path);
                let default_value = child.meta.default.as_ref().map(toml_default_to_json_value);
                let default_scalar = default_value
                    .as_ref()
                    .filter(|value| !json_value_is_empty(value))
                    .map(|value| {
                        json_value_to_scalar_cell_for_field(
                            value,
                            table_name,
                            row_number,
                            &child_path,
                        )
                    })
                    .transpose()?;
                let effective_value = raw.or(default_scalar.as_deref());
                if child.meta.required && effective_value.is_none() {
                    return Err(ConfigForgeError::Validation(format!(
                        "table `{}` row {} field `{}` is required",
                        table_name, row_number, child_path
                    )));
                }
                if let Some(value) = effective_value {
                    if !validate_scalar_value(child, value) {
                        return Err(ConfigForgeError::Validation(format!(
                            "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
                            table_name, row_number, child_path, value, child.meta.field_type
                        )));
                    }
                    has_effective_value = true;
                }
            }
        }
    }

    Ok(has_effective_value)
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
        .filter(|child| child_is_visible(child, target_tags))
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
                let default_value = child.meta.default.as_ref().map(toml_default_to_json_value);
                let default_scalar = default_value
                    .as_ref()
                    .filter(|value| !json_value_is_empty(value))
                    .map(|value| {
                        json_value_to_scalar_cell_for_field(
                            value,
                            table_name,
                            row_number,
                            &child_path,
                        )
                    })
                    .transpose()?;
                if default_scalar.is_some() {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

fn visible_struct_child_count(
    struct_type: &LinkedStructType<'_>,
    target_tags: Option<&TargetTags>,
) -> usize {
    struct_type
        .fields
        .iter()
        .filter(|field| child_is_visible(field, target_tags))
        .count()
}

pub(super) fn visible_required_struct_child_count(
    struct_type: &LinkedStructType<'_>,
    target_tags: Option<&TargetTags>,
) -> usize {
    struct_type
        .fields
        .iter()
        .filter(|field| field.meta.required && child_is_visible(field, target_tags))
        .count()
}

pub(super) fn validate_struct_child_path(
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

fn struct_child<'a, 'meta>(
    struct_type: &'a LinkedStructType<'meta>,
    name: &str,
) -> Option<&'a LinkedFieldMeta<'meta>> {
    struct_type
        .fields
        .iter()
        .find(|child| child.meta.name == name)
}

fn child_is_visible(field: &LinkedFieldMeta<'_>, target_tags: Option<&TargetTags>) -> bool {
    target_tags
        .map(|target_tags| field.meta.is_exported_for(target_tags))
        .unwrap_or(true)
}

fn non_empty_row_value<'a>(row: &'a DataRow, field_path: &str) -> Option<&'a str> {
    row.get(field_path)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn validate_supported_field_type(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    match (&field.linked_field, field.meta.layout) {
        (LinkedField::Struct(_), None) => Ok(()),
        (LinkedField::Struct(_), Some(_)) => Err(ConfigForgeError::Validation(format!(
            "table `{}` field `{}` uses struct type `{}` with unsupported layout; struct source layout is decided by source headers or keys",
            meta.meta.name, field.meta.name, field.meta.field_type
        ))),
        (_, None | Some(ValueLayout::ScalarCell)) => Ok(()),
        (_, Some(ValueLayout::JsonCell | ValueLayout::ExpandedValue)) => {
            Err(ConfigForgeError::Validation(format!(
                "table `{}` field `{}` uses unsupported layout; non-struct fields only support scalar_cell in this phase",
                meta.meta.name, field.meta.name
            )))
        }
    }
}

pub(super) fn validate_value_type(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
    value: &str,
    row_index: usize,
) -> Result<()> {
    validate_value_type_at_row_number(meta, field, value, row_index + 1, None)
}

pub(super) fn validate_value_type_at_row_number(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
    value: &str,
    row_number: usize,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    if let LinkedField::Struct(struct_type) = &field.linked_field {
        return validate_struct_json_cell(
            meta.meta.name.as_str(),
            row_number,
            field.meta.name.as_str(),
            struct_type,
            value,
            target_tags,
        );
    }

    if !validate_scalar_value(field, value) {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
            meta.meta.name, row_number, field.meta.name, value, field.meta.field_type
        )));
    }

    Ok(())
}

fn validate_scalar_value(field: &LinkedFieldMeta<'_>, value: &str) -> bool {
    match &field.linked_field {
        LinkedField::Primitive(PrimitiveFieldType::Int) => value.parse::<i64>().is_ok(),
        LinkedField::Primitive(PrimitiveFieldType::Float) => parse_finite_float(value).is_some(),
        LinkedField::Primitive(PrimitiveFieldType::Bool) => {
            matches!(value, "true" | "false" | "1" | "0")
        }
        LinkedField::Primitive(
            PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
        ) => true,
        LinkedField::Enum(enum_meta) => enum_meta
            .values
            .iter()
            .any(|enum_value| enum_value.name == value),
        LinkedField::Struct(_) => false,
    }
}

fn parse_finite_float(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn validate_struct_json_cell(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
    raw: &str,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|source| {
        ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` has invalid struct JSON: {}",
            table_name, row_number, field_path, source
        ))
    })?;
    let object = value.as_object().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` must be a JSON object for struct `{}`",
            table_name, row_number, field_path, struct_type.meta.name
        ))
    })?;

    for key in object.keys() {
        if !struct_type
            .fields
            .iter()
            .any(|field| field.meta.name == key.as_str())
        {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` contains unknown struct key `{}`",
                table_name, row_number, field_path, key
            )));
        }
    }

    for child in &struct_type.fields {
        if let Some(target_tags) = target_tags {
            if !child.meta.is_exported_for(target_tags) {
                continue;
            }
        }
        let child_path = format!("{field_path}.{}", child.meta.name);
        let child_value = object.get(&child.meta.name);
        let default_value = child.meta.default.as_ref().map(toml_default_to_json_value);
        let effective_value = child_value.or(default_value.as_ref());
        let is_empty = effective_value.map(json_value_is_empty).unwrap_or(true);

        if child.meta.required && is_empty {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` is required",
                table_name, row_number, child_path
            )));
        }
        if is_empty {
            continue;
        }

        match &child.linked_field {
            LinkedField::Struct(nested_struct) => {
                let raw = effective_value.expect("checked non-empty").to_string();
                validate_struct_json_cell(
                    table_name,
                    row_number,
                    &child_path,
                    nested_struct,
                    &raw,
                    target_tags,
                )?;
            }
            _ => {
                let scalar = json_value_to_scalar_cell_for_field(
                    effective_value.expect("checked non-empty"),
                    table_name,
                    row_number,
                    &child_path,
                )?;
                if !validate_scalar_value(child, &scalar) {
                    return Err(ConfigForgeError::Validation(format!(
                        "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
                        table_name, row_number, child_path, scalar, child.meta.field_type
                    )));
                }
            }
        }
    }

    Ok(())
}
