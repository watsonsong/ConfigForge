// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::{
    ConfigForgeError, Result,
    meta::{
        LinkedField, LinkedFieldMeta, LinkedMetaModel, LinkedTableMeta, PrimitiveFieldType,
        ValidatorMeta,
    },
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
}

pub(super) fn validate_allowed_params(
    kind: &str,
    meta: &ValidatorMeta,
    allowed: &[&str],
) -> Result<()> {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    for key in meta.params.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(ConfigForgeError::Validation(format!(
                "validator `{kind}` has unknown parameter `{key}`"
            )));
        }
    }
    Ok(())
}

pub(super) fn required_string(meta: &ValidatorMeta, kind: &str, key: &str) -> Result<String> {
    optional_string(meta, kind, key)?.ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "validator `{kind}` must set string parameter `{key}`"
        ))
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
            ConfigForgeError::Validation(format!(
                "validator `{kind}` parameter `{key}` must be string"
            ))
        })
}

pub(super) fn optional_bool(meta: &ValidatorMeta, kind: &str, key: &str) -> Result<Option<bool>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    value.as_bool().map(Some).ok_or_else(|| {
        ConfigForgeError::Validation(format!("validator `{kind}` parameter `{key}` must be bool"))
    })
}

pub(super) fn optional_number(meta: &ValidatorMeta, kind: &str, key: &str) -> Result<Option<f64>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    if let Some(value) = value.as_integer() {
        return Ok(Some(value as f64));
    }
    if let Some(value) = value.as_float() {
        if value.is_finite() {
            return Ok(Some(value));
        }
    }
    Err(ConfigForgeError::Validation(format!(
        "validator `{kind}` parameter `{key}` must be finite integer or float"
    )))
}

pub(super) fn optional_usize(meta: &ValidatorMeta, kind: &str, key: &str) -> Result<Option<usize>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    let Some(value) = value.as_integer() else {
        return Err(ConfigForgeError::Validation(format!(
            "validator `{kind}` parameter `{key}` must be non-negative integer"
        )));
    };
    usize::try_from(value).map(Some).map_err(|_| {
        ConfigForgeError::Validation(format!(
            "validator `{kind}` parameter `{key}` must be non-negative integer"
        ))
    })
}

pub(super) fn linked_table<'meta>(
    linked_meta: &'meta LinkedMetaModel<'meta>,
    validator_kind: &str,
    table_name: &str,
) -> Result<&'meta LinkedTableMeta<'meta>> {
    linked_meta.table(table_name).ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "{validator_kind} validator references unknown table `{table_name}`"
        ))
    })
}

pub(super) fn linked_field<'meta>(
    table: &'meta LinkedTableMeta<'meta>,
    validator_kind: &str,
    field_name: &str,
) -> Result<&'meta LinkedFieldMeta<'meta>> {
    table.field(field_name).ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "table `{}` {validator_kind} validator references unknown field `{field_name}`",
            table.meta.name
        ))
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
        LinkedField::Struct(_) | LinkedField::List(_) => None,
    }
}
