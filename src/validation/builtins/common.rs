// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    data::DataValue,
    diagnostic::code::CF_VALIDATION_INVALID_CONFIG,
    meta::{
        LinkedField, LinkedFieldMeta, LinkedMetaModel, LinkedTableMeta, PrimitiveFieldType,
        ValidatorMeta,
    },
};

use crate::validation::{
    ExportTableView,
    key_policy::{KeyPolicy, key_value_for_view},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScalarFieldKind {
    Int,
    Float,
    Bool,
    String,
    Text,
    Asset,
    Enum,
}

impl ScalarFieldKind {
    pub(super) fn is_numeric(self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }

    pub(super) fn is_string_like(self) -> bool {
        matches!(self, Self::String | Self::Text | Self::Asset)
    }

    pub(super) fn supports_regex(self) -> bool {
        self.is_string_like() || matches!(self, Self::Enum)
    }

    pub(super) fn is_comparable(self) -> bool {
        matches!(
            self,
            Self::Int | Self::Float | Self::String | Self::Text | Self::Asset | Self::Enum
        )
    }

    #[allow(dead_code)]
    pub(super) fn is_stable_key_capable(self) -> bool {
        matches!(
            self,
            Self::Int
                | Self::Float
                | Self::Bool
                | Self::String
                | Self::Text
                | Self::Asset
                | Self::Enum
        )
    }

    #[allow(dead_code)]
    pub(super) fn is_stable_sort_capable(self) -> bool {
        self.is_comparable()
    }
}

pub(super) fn validate_allowed_params(
    kind: &str,
    meta: &ValidatorMeta,
    allowed: &[&str],
) -> Result<()> {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    for key in meta.params.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(validator_config_error(
                kind,
                format!("validator `{kind}` has unknown parameter `{key}`"),
                key,
                format!(
                    "one of: {}",
                    allowed.iter().copied().collect::<Vec<_>>().join(", ")
                ),
                "Remove the unknown parameter or fix the parameter spelling.",
            ));
        }
    }
    Ok(())
}

pub(super) fn required_string(meta: &ValidatorMeta, kind: &str, key: &str) -> Result<String> {
    optional_string(meta, kind, key)?.ok_or_else(|| {
        validator_config_error(
            kind,
            format!("validator `{kind}` must set string parameter `{key}`"),
            "missing parameter",
            format!("string parameter `{key}`"),
            "Add the required string parameter to the validator config.",
        )
    })
}

pub(super) fn optional_string(
    meta: &ValidatorMeta,
    kind: &str,
    key: &str,
) -> Result<Option<String>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    value
        .as_str()
        .map(|value| Some(value.to_owned()))
        .ok_or_else(|| {
            validator_config_error(
                kind,
                format!("validator `{kind}` parameter `{key}` must be string"),
                value.to_string(),
                "string",
                "Use a TOML string value for this validator parameter.",
            )
        })
}

#[allow(dead_code)]
pub(super) fn optional_string_array(
    meta: &ValidatorMeta,
    kind: &str,
    key: &str,
) -> Result<Option<Vec<String>>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    let Some(values) = value.as_array() else {
        return Err(validator_config_error(
            kind,
            format!("validator `{kind}` parameter `{key}` must be an array of strings"),
            value.to_string(),
            "array of strings",
            "Use a TOML array containing only string values.",
        ));
    };
    values
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                validator_config_error(
                    kind,
                    format!("validator `{kind}` parameter `{key}` must be an array of strings"),
                    value.to_string(),
                    "string array element",
                    "Use only TOML string values in this array.",
                )
            })
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

pub(super) fn optional_bool(meta: &ValidatorMeta, kind: &str, key: &str) -> Result<Option<bool>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    value.as_bool().map(Some).ok_or_else(|| {
        validator_config_error(
            kind,
            format!("validator `{kind}` parameter `{key}` must be bool"),
            value.to_string(),
            "bool",
            "Use true or false for this validator parameter.",
        )
    })
}

pub(super) fn optional_number(meta: &ValidatorMeta, kind: &str, key: &str) -> Result<Option<f64>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    if let Some(value) = value.as_integer() {
        return Ok(Some(value as f64));
    }
    if let Some(value) = value.as_float()
        && value.is_finite()
    {
        return Ok(Some(value));
    }
    Err(validator_config_error(
        kind,
        format!("validator `{kind}` parameter `{key}` must be finite integer or float"),
        value.to_string(),
        "finite integer or float",
        "Use a finite numeric value for this validator parameter.",
    ))
}

pub(super) fn optional_usize(meta: &ValidatorMeta, kind: &str, key: &str) -> Result<Option<usize>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    let Some(value) = value.as_integer() else {
        return Err(validator_config_error(
            kind,
            format!("validator `{kind}` parameter `{key}` must be non-negative integer"),
            value.to_string(),
            "non-negative integer",
            "Use a non-negative integer for this validator parameter.",
        ));
    };
    usize::try_from(value).map(Some).map_err(|_| {
        validator_config_error(
            kind,
            format!("validator `{kind}` parameter `{key}` must be non-negative integer"),
            value.to_string(),
            "non-negative integer",
            "Use a non-negative integer for this validator parameter.",
        )
    })
}

pub(super) fn linked_table<'meta>(
    linked_meta: &'meta LinkedMetaModel<'meta>,
    validator_kind: &str,
    table_name: &str,
) -> Result<&'meta LinkedTableMeta<'meta>> {
    linked_meta.table(table_name).ok_or_else(|| {
        validator_config_error(
            validator_kind,
            format!("{validator_kind} validator references unknown table `{table_name}`"),
            table_name,
            "declared table name",
            "Check the validator table reference and Meta table declarations.",
        )
    })
}

pub(super) fn linked_field<'meta>(
    table: &'meta LinkedTableMeta<'meta>,
    validator_kind: &str,
    field_name: &str,
) -> Result<&'meta LinkedFieldMeta<'meta>> {
    table.field(field_name).ok_or_else(|| {
        validator_config_error(
            validator_kind,
            format!(
                "table `{}` {validator_kind} validator references unknown field `{field_name}`",
                table.meta.name
            ),
            field_name,
            format!("declared field in table `{}`", table.meta.name),
            "Check the validator field reference and table field declarations.",
        )
    })
}

pub(super) fn scalar_field_kind(field: &LinkedFieldMeta<'_>) -> Option<ScalarFieldKind> {
    match &field.linked_field {
        LinkedField::Primitive(PrimitiveFieldType::Int) => Some(ScalarFieldKind::Int),
        LinkedField::Primitive(PrimitiveFieldType::Float) => Some(ScalarFieldKind::Float),
        LinkedField::Primitive(PrimitiveFieldType::Bool) => Some(ScalarFieldKind::Bool),
        LinkedField::Primitive(PrimitiveFieldType::String) => Some(ScalarFieldKind::String),
        LinkedField::Primitive(PrimitiveFieldType::Text) => Some(ScalarFieldKind::Text),
        LinkedField::Primitive(PrimitiveFieldType::Asset) => Some(ScalarFieldKind::Asset),
        LinkedField::Enum(_) => Some(ScalarFieldKind::Enum),
        LinkedField::Primitive(
            PrimitiveFieldType::Color
            | PrimitiveFieldType::Vector2
            | PrimitiveFieldType::Vector3
            | PrimitiveFieldType::Vector4
            | PrimitiveFieldType::Vector2Int
            | PrimitiveFieldType::Vector3Int
            | PrimitiveFieldType::Vector4Int,
        ) => None,
        LinkedField::Struct(_) | LinkedField::List(_) => None,
    }
}

pub(super) fn literal_to_string(kind: &str, key: &str, value: &toml::Value) -> Result<String> {
    if let Some(value) = value.as_str() {
        return Ok(value.to_owned());
    }
    if let Some(value) = value.as_integer() {
        return Ok(value.to_string());
    }
    if let Some(value) = value.as_float()
        && value.is_finite()
    {
        return Ok(value.to_string());
    }
    if let Some(value) = value.as_bool() {
        return Ok(value.to_string());
    }
    Err(validator_config_error(
        kind,
        format!("validator `{kind}` parameter `{key}` must be string, integer, float or bool"),
        value.to_string(),
        "string, integer, float or bool",
        "Use a scalar TOML value for this validator parameter.",
    ))
}

pub(super) fn literal_array_to_strings(
    kind: &str,
    key: &str,
    value: &toml::Value,
) -> Result<Vec<String>> {
    let Some(values) = value.as_array() else {
        return Err(validator_config_error(
            kind,
            format!("validator `{kind}` parameter `{key}` must be an array"),
            value.to_string(),
            "array",
            "Use a TOML array for this validator parameter.",
        ));
    };
    values
        .iter()
        .map(|value| literal_to_string(kind, key, value))
        .collect()
}

pub(super) fn validator_config_error(
    kind: &str,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_VALIDATION_INVALID_CONFIG,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(format!("validator.{kind}"))
        .with_domain("validation")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
}

pub(super) fn resolved_data_value<'a>(
    table_view: &'a ExportTableView<'_, '_>,
    visible_row_index: usize,
    field_name: &str,
) -> Option<&'a DataValue> {
    table_view
        .resolved_value(visible_row_index, field_name)
        .map(|value| &value.value)
}

pub(super) fn field_value(
    table_view: &ExportTableView<'_, '_>,
    visible_row_index: usize,
    field_name: &str,
) -> String {
    key_value_for_view(
        KeyPolicy::ScalarExport,
        table_view,
        visible_row_index,
        field_name,
    )
}

pub(super) fn is_empty_value(value: &str) -> bool {
    value.trim().is_empty()
}

pub(super) fn compare_values(kind: ScalarFieldKind, left: &str, right: &str) -> Option<Ordering> {
    match kind {
        ScalarFieldKind::Int => Some(left.parse::<i64>().ok()?.cmp(&right.parse::<i64>().ok()?)),
        ScalarFieldKind::Float => left
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())?
            .partial_cmp(
                &right
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())?,
            ),
        ScalarFieldKind::String
        | ScalarFieldKind::Text
        | ScalarFieldKind::Asset
        | ScalarFieldKind::Enum => Some(left.cmp(right)),
        ScalarFieldKind::Bool => None,
    }
}
