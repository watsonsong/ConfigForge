// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    data::{
        DataRow, DataValue, ResolvedValue, ValueSourceKind,
        typed_value::{
            parse_list_cell_value, parse_list_json_value, parse_scalar_field_value,
            parse_scalar_for_type,
        },
        value_conversion::{
            json_value_is_empty, json_value_to_cell, json_value_to_scalar_cell_for_field,
            toml_default_to_json_value,
        },
    },
    diagnostic::{
        DiagnosticLocation,
        code::{
            CF_DATA_INVALID_TYPE, CF_DATA_MISSING_REQUIRED_FIELD, CF_VALIDATION_INVALID_CONFIG,
        },
    },
    meta::{
        LinkedField, LinkedFieldMeta, LinkedStructType, LinkedTableMeta, TargetTags, ValueLayout,
        VectorFormat,
    },
};

fn struct_value_error(
    code: &'static str,
    source: &'static str,
    table_name: &str,
    row_number: usize,
    field_path: &str,
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    super::validation_diagnostic_error(
        code,
        source,
        message,
        DiagnosticLocation {
            table: Some(table_name.to_owned()),
            field: Some(field_path.to_owned()),
            source_row_number: Some(row_number),
            ..DiagnosticLocation::default()
        },
        current,
        expected,
        "Check struct/list value layout, expanded paths, and field defaults.",
    )
}

fn struct_required_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    message: String,
) -> ConfigForgeError {
    struct_value_error(
        CF_DATA_MISSING_REQUIRED_FIELD,
        "validation.struct.required",
        table_name,
        row_number,
        field_path,
        message,
        "missing or empty struct field value",
        "non-empty required struct child value or configured default",
    )
}

fn struct_type_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    struct_value_error(
        CF_DATA_INVALID_TYPE,
        "validation.struct.type",
        table_name,
        row_number,
        field_path,
        message,
        current,
        expected,
    )
}

fn struct_config_error(
    table_name: &str,
    field_path: &str,
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    super::validation_diagnostic_error(
        CF_VALIDATION_INVALID_CONFIG,
        "validation.struct.config",
        message,
        DiagnosticLocation {
            table: Some(table_name.to_owned()),
            field: Some(field_path.to_owned()),
            ..DiagnosticLocation::default()
        },
        current,
        expected,
        "Check field type layout configuration for struct/list/scalar fields.",
    )
}

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
        return Err(struct_required_error(
            meta.meta.name.as_str(),
            row_number,
            field.meta.name.as_str(),
            format!(
                "table `{}` row {} field `{}` is required",
                meta.meta.name, row_number, field.meta.name
            ),
        ));
    }
    Ok(())
}

pub(super) fn validate_struct_field_value_for_resolved_row(
    meta: &LinkedTableMeta<'_>,
    row_number: usize,
    field: &LinkedFieldMeta<'_>,
    resolved_value: Option<&ResolvedValue>,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    let LinkedField::Struct(struct_type) = &field.linked_field else {
        return Ok(());
    };

    if let Some(resolved_value) = resolved_value {
        validate_resolved_struct_value(
            meta.meta.name.as_str(),
            row_number,
            field.meta.name.as_str(),
            struct_type,
            resolved_value,
            target_tags,
        )?;
        return Ok(());
    }

    if field.meta.required && visible_struct_child_count(struct_type, target_tags) > 0 {
        return Err(struct_required_error(
            meta.meta.name.as_str(),
            row_number,
            field.meta.name.as_str(),
            format!(
                "table `{}` row {} field `{}` is required",
                meta.meta.name, row_number, field.meta.name
            ),
        ));
    }
    Ok(())
}

fn validate_resolved_struct_value(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
    resolved_value: &ResolvedValue,
    target_tags: Option<&TargetTags>,
) -> Result<()> {
    let DataValue::Struct(children) = &resolved_value.value else {
        return Err(struct_type_error(
            table_name,
            row_number,
            field_path,
            format!(
                "table `{}` row {} field `{}` must be a JSON object for struct `{}`",
                table_name, row_number, field_path, struct_type.meta.name
            ),
            resolved_value
                .source
                .raw_value
                .clone()
                .unwrap_or_else(|| resolved_value.source.raw_field_path.clone()),
            format!("JSON object for struct `{}`", struct_type.meta.name),
        ));
    };

    for child in struct_type
        .fields
        .iter()
        .filter(|child| child_is_visible(child, target_tags))
    {
        let child_path = format!("{field_path}.{}", child.meta.name);
        let child_value = children.get(&child.meta.name);
        match &child.linked_field {
            LinkedField::Struct(nested_struct) => {
                if let Some(child_value) = child_value {
                    validate_resolved_struct_value(
                        table_name,
                        row_number,
                        &child_path,
                        nested_struct,
                        child_value,
                        target_tags,
                    )?;
                } else if child.meta.required
                    && visible_struct_child_count(nested_struct, target_tags) > 0
                {
                    return Err(struct_required_error(
                        table_name,
                        row_number,
                        &child_path,
                        format!(
                            "table `{}` row {} field `{}` is required",
                            table_name, row_number, child_path
                        ),
                    ));
                }
            }
            _ => {
                if child.meta.required && child_value.is_none() {
                    return Err(struct_required_error(
                        table_name,
                        row_number,
                        &child_path,
                        format!(
                            "table `{}` row {} field `{}` is required",
                            table_name, row_number, child_path
                        ),
                    ));
                }
            }
        }
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
                    return Err(struct_required_error(
                        table_name,
                        row_number,
                        &child_path,
                        format!(
                            "table `{}` row {} field `{}` is required",
                            table_name, row_number, child_path
                        ),
                    ));
                }
                has_effective_value |= child_effective;
            }
            LinkedField::List(element_type) => {
                let indexed_cells = indexed_list_cells(row, &child_path);
                if let Some(raw) = non_empty_row_value(row, &child_path) {
                    if let Some((_, _, first_indexed_path)) = indexed_cells.first() {
                        return Err(struct_type_error(
                            table_name,
                            row_number,
                            &child_path,
                            format!(
                                "table `{}` row {} field `{}` mixes list JSON cell with expanded list element path `{}`",
                                table_name, row_number, child_path, first_indexed_path
                            ),
                            format!("{child_path} and {first_indexed_path}"),
                            "either list JSON cell or expanded list element paths, not both",
                        ));
                    }
                    parse_list_cell_value(
                        table_name,
                        row_number,
                        &child_path,
                        element_type,
                        raw,
                        None,
                        ValueSourceKind::ExplicitCell,
                    )?;
                    has_effective_value = true;
                    continue;
                }
                if !indexed_cells.is_empty() {
                    validate_indexed_list_cells(
                        table_name,
                        row_number,
                        &child_path,
                        element_type,
                        &indexed_cells,
                    )?;
                    has_effective_value = true;
                    continue;
                }

                let effective_value =
                    effective_list_cell_value(table_name, row_number, &child_path, child, row)?;
                if child.meta.required && effective_value.is_none() {
                    return Err(struct_required_error(
                        table_name,
                        row_number,
                        &child_path,
                        format!(
                            "table `{}` row {} field `{}` is required",
                            table_name, row_number, child_path
                        ),
                    ));
                }
                if let Some(value) = effective_value {
                    parse_list_cell_value(
                        table_name,
                        row_number,
                        &child_path,
                        element_type,
                        &value,
                        None,
                        ValueSourceKind::ExplicitCell,
                    )?;
                    has_effective_value = true;
                }
            }
            _ => {
                let effective_value =
                    effective_scalar_cell_value(table_name, row_number, &child_path, child, row)?;
                if child.meta.required && effective_value.is_none() {
                    return Err(struct_required_error(
                        table_name,
                        row_number,
                        &child_path,
                        format!(
                            "table `{}` row {} field `{}` is required",
                            table_name, row_number, child_path
                        ),
                    ));
                }
                if let Some(value) = effective_value {
                    parse_scalar_field_value(table_name, row_number, &child_path, child, &value)?;
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
            LinkedField::List(_) => {
                if effective_list_cell_value(table_name, row_number, &child_path, child, row)?
                    .is_some()
                    || !indexed_list_cells(row, &child_path).is_empty()
                {
                    return Ok(true);
                }
            }
            _ => {
                if effective_scalar_cell_value(table_name, row_number, &child_path, child, row)?
                    .is_some()
                {
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
        if let Some((list_field_name, _)) = parse_list_element_path(path) {
            if struct_child(struct_type, list_field_name)
                .is_some_and(|field| matches!(field.linked_field, LinkedField::List(_)))
            {
                return Ok(());
            }
        }
        return Err(struct_type_error(
            table_name,
            row_number,
            full_path,
            format!(
                "table `{}` row {} field `{}` references unknown struct child `{}`",
                table_name, row_number, full_path, path
            ),
            path,
            "declared struct child field",
        ));
    };

    let Some(child) = struct_child(struct_type, segment) else {
        return Err(struct_type_error(
            table_name,
            row_number,
            full_path,
            format!(
                "table `{}` row {} field `{}` references unknown struct child `{}`",
                table_name, row_number, full_path, segment
            ),
            segment,
            "declared struct child field",
        ));
    };
    if is_expanded_vector_component(child, rest) {
        return Ok(());
    }
    let LinkedField::Struct(nested_struct) = &child.linked_field else {
        return Err(struct_type_error(
            table_name,
            row_number,
            full_path,
            format!(
                "table `{}` row {} field `{}` uses child path but `{}` is not a struct field",
                table_name, row_number, full_path, segment
            ),
            segment,
            "struct field before nested child path",
        ));
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

fn is_expanded_vector_component(field: &LinkedFieldMeta<'_>, component: &str) -> bool {
    field.meta.vector_format == Some(VectorFormat::Expanded)
        && field
            .primitive_type()
            .is_some_and(|primitive| primitive.vector_dimension().is_some())
        && !component.contains('.')
        && field
            .meta
            .vector_component_names()
            .iter()
            .any(|name| name == component)
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
        (LinkedField::Struct(_), Some(layout)) => Err(struct_config_error(
            meta.meta.name.as_str(),
            field.meta.name.as_str(),
            format!(
                "table `{}` field `{}` uses struct type `{}` with unsupported layout; struct source layout is decided by source headers or keys",
                meta.meta.name, field.meta.name, field.meta.field_type
            ),
            format!("struct layout {layout:?}"),
            "struct field without explicit source layout",
        )),
        (LinkedField::List(_), None | Some(ValueLayout::JsonCell)) => Ok(()),
        (LinkedField::List(_), Some(ValueLayout::ScalarCell | ValueLayout::ExpandedValue)) => {
            Err(struct_config_error(
                meta.meta.name.as_str(),
                field.meta.name.as_str(),
                format!(
                    "table `{}` field `{}` uses unsupported layout; list fields must use JSON cell values in this phase",
                    meta.meta.name, field.meta.name
                ),
                format!("list layout {:?}", field.meta.layout),
                "JSON cell layout for list fields",
            ))
        }
        (_, None | Some(ValueLayout::ScalarCell)) => Ok(()),
        (_, Some(ValueLayout::JsonCell | ValueLayout::ExpandedValue)) => Err(struct_config_error(
            meta.meta.name.as_str(),
            field.meta.name.as_str(),
            format!(
                "table `{}` field `{}` uses unsupported layout; non-struct fields only support scalar_cell in this phase",
                meta.meta.name, field.meta.name
            ),
            format!("scalar layout {:?}", field.meta.layout),
            "scalar_cell layout for non-struct fields",
        )),
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
    match &field.linked_field {
        LinkedField::Struct(struct_type) => {
            return validate_struct_json_cell(
                meta.meta.name.as_str(),
                row_number,
                field.meta.name.as_str(),
                struct_type,
                value,
                target_tags,
            );
        }
        LinkedField::List(element_type) => {
            return parse_list_cell_value(
                meta.meta.name.as_str(),
                row_number,
                field.meta.name.as_str(),
                element_type,
                value,
                None,
                ValueSourceKind::ExplicitCell,
            )
            .map(|_| ());
        }
        _ => {}
    }

    parse_scalar_field_value(
        meta.meta.name.as_str(),
        row_number,
        field.meta.name.as_str(),
        field,
        value,
    )
    .map(|_| ())
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
        struct_type_error(
            table_name,
            row_number,
            field_path,
            format!(
                "table `{}` row {} field `{}` has invalid struct JSON: {}",
                table_name, row_number, field_path, source
            ),
            raw,
            "valid JSON object for struct field",
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        struct_type_error(
            table_name,
            row_number,
            field_path,
            format!(
                "table `{}` row {} field `{}` must be a JSON object for struct `{}`",
                table_name, row_number, field_path, struct_type.meta.name
            ),
            value.to_string(),
            format!("JSON object for struct `{}`", struct_type.meta.name),
        )
    })?;

    for key in object.keys() {
        if !struct_type
            .fields
            .iter()
            .any(|field| field.meta.name == key.as_str())
        {
            return Err(struct_type_error(
                table_name,
                row_number,
                field_path,
                format!(
                    "table `{}` row {} field `{}` contains unknown struct key `{}`",
                    table_name, row_number, field_path, key
                ),
                key,
                format!("declared key in struct `{}`", struct_type.meta.name),
            ));
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
            return Err(struct_required_error(
                table_name,
                row_number,
                &child_path,
                format!(
                    "table `{}` row {} field `{}` is required",
                    table_name, row_number, child_path
                ),
            ));
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
            LinkedField::List(element_type) => {
                parse_list_json_value(
                    table_name,
                    row_number,
                    &child_path,
                    element_type,
                    effective_value.expect("checked non-empty"),
                    None,
                    ValueSourceKind::ExplicitCell,
                )?;
            }
            _ => {
                let scalar = json_value_to_scalar_cell_for_field(
                    effective_value.expect("checked non-empty"),
                    table_name,
                    row_number,
                    &child_path,
                )?;
                parse_scalar_field_value(table_name, row_number, &child_path, child, &scalar)?;
            }
        }
    }

    Ok(())
}

fn effective_scalar_cell_value(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    row: &DataRow,
) -> Result<Option<String>> {
    if let Some(raw) = non_empty_row_value(row, field_path) {
        return Ok(Some(raw.to_owned()));
    }
    let Some(default) = field.meta.default.as_ref() else {
        return Ok(None);
    };
    let default_value = toml_default_to_json_value(default);
    if json_value_is_empty(&default_value) {
        return Ok(None);
    }
    json_value_to_scalar_cell_for_field(&default_value, table_name, row_number, field_path)
        .map(Some)
}

fn effective_list_cell_value(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    row: &DataRow,
) -> Result<Option<String>> {
    if let Some(raw) = non_empty_row_value(row, field_path) {
        return Ok(Some(raw.to_owned()));
    }
    let Some(default) = field.meta.default.as_ref() else {
        return Ok(None);
    };
    let default_value = toml_default_to_json_value(default);
    if json_value_is_empty(&default_value) {
        return Ok(None);
    }
    json_value_to_cell(&default_value).map(Some).map_err(|_| {
        struct_type_error(
            table_name,
            row_number,
            field_path,
            format!(
                "table `{}` row {} field `{}` must be a JSON array",
                table_name, row_number, field_path
            ),
            default_value.to_string(),
            "JSON array for list field default",
        )
    })
}

fn validate_indexed_list_cells(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    element_type: &LinkedField<'_>,
    indexed_cells: &[(usize, String, String)],
) -> Result<()> {
    for (expected_index, (index, raw, element_path)) in indexed_cells.iter().enumerate() {
        if *index != expected_index {
            return Err(struct_type_error(
                table_name,
                row_number,
                field_path,
                format!(
                    "table `{}` row {} field `{}` has non-contiguous expanded list element path `{}`; expected `{}`",
                    table_name,
                    row_number,
                    field_path,
                    element_path,
                    list_element_path(field_path, expected_index)
                ),
                element_path,
                list_element_path(field_path, expected_index),
            ));
        }
        parse_scalar_for_type(table_name, row_number, element_path, element_type, raw).map_err(
            |_| {
                struct_type_error(
                    table_name,
                    row_number,
                    element_path,
                    format!(
                        "table `{}` row {} field `{}` value `{}` does not match list element type",
                        table_name, row_number, element_path, raw
                    ),
                    raw,
                    "value matching list element type",
                )
            },
        )?;
    }
    Ok(())
}

fn indexed_list_cells(row: &DataRow, field_path: &str) -> Vec<(usize, String, String)> {
    let mut cells = row
        .iter()
        .filter_map(|(key, raw)| {
            list_index_for_field_path(key, field_path)
                .map(|index| (index, raw.clone(), key.clone()))
        })
        .collect::<Vec<_>>();
    cells.sort_by_key(|(index, _, _)| *index);
    cells
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
