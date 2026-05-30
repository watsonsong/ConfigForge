// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ConfigForgeError, Result};

pub(crate) fn toml_default_to_json_value(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(value) => serde_json::Value::String(value.clone()),
        toml::Value::Integer(value) => serde_json::Value::Number((*value).into()),
        toml::Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(value) => serde_json::Value::Bool(*value),
        toml::Value::Array(values) => {
            serde_json::Value::Array(values.iter().map(toml_default_to_json_value).collect())
        }
        _ => serde_json::Value::Null,
    }
}

pub(crate) fn json_value_is_empty(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::Null)
        || value.as_str().map(str::trim).is_some_and(str::is_empty)
}

pub(crate) fn json_value_to_scalar_cell(value: &serde_json::Value) -> Result<String> {
    match value {
        serde_json::Value::String(value) => Ok(value.clone()),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        serde_json::Value::Bool(value) => Ok(value.to_string()),
        _ => Err(ConfigForgeError::Validation(
            "struct scalar field value must be string, number, or bool".to_owned(),
        )),
    }
}

pub(crate) fn json_value_to_scalar_cell_for_field(
    value: &serde_json::Value,
    table_name: &str,
    row_number: usize,
    field_path: &str,
) -> Result<String> {
    json_value_to_scalar_cell(value).map_err(|_| {
        ConfigForgeError::Validation(format!(
            "table `{table_name}` row {row_number} field `{field_path}` must be a scalar value"
        ))
    })
}

pub(crate) fn json_value_to_cell(value: &serde_json::Value) -> Result<String> {
    match value {
        serde_json::Value::String(value) => Ok(value.clone()),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        serde_json::Value::Bool(value) => Ok(value.to_string()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => Ok(value.to_string()),
        serde_json::Value::Null => Err(ConfigForgeError::Validation(
            "field value must not be null".to_owned(),
        )),
    }
}

pub(crate) fn default_value_to_cell(
    table_name: &str,
    field_name: &str,
    default: &toml::Value,
) -> Result<String> {
    json_value_to_cell(&toml_default_to_json_value(default)).map_err(|_| {
        ConfigForgeError::Validation(format!(
            "table `{table_name}` field `{field_name}` has unsupported default value `{default}`"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_toml_scalar_defaults_to_json_values() {
        assert_eq!(
            toml_default_to_json_value(&toml::Value::String("hello".to_owned())),
            serde_json::Value::String("hello".to_owned())
        );
        assert_eq!(
            toml_default_to_json_value(&toml::Value::Integer(42)),
            serde_json::json!(42)
        );
        assert_eq!(
            toml_default_to_json_value(&toml::Value::Float(1.5)),
            serde_json::json!(1.5)
        );
        assert_eq!(
            toml_default_to_json_value(&toml::Value::Boolean(true)),
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn treats_null_and_blank_strings_as_empty() {
        assert!(json_value_is_empty(&serde_json::Value::Null));
        assert!(json_value_is_empty(&serde_json::Value::String(
            "  ".to_owned()
        )));
        assert!(!json_value_is_empty(&serde_json::json!(0)));
        assert!(!json_value_is_empty(&serde_json::Value::String(
            "x".to_owned()
        )));
    }

    #[test]
    fn converts_json_scalar_values_to_cells() {
        assert_eq!(
            json_value_to_scalar_cell(&serde_json::Value::String("x".to_owned())).unwrap(),
            "x"
        );
        assert_eq!(
            json_value_to_scalar_cell(&serde_json::json!(42)).unwrap(),
            "42"
        );
        assert_eq!(
            json_value_to_scalar_cell(&serde_json::Value::Bool(false)).unwrap(),
            "false"
        );
        assert!(json_value_to_scalar_cell(&serde_json::json!({ "x": 1 })).is_err());
    }

    #[test]
    fn default_value_to_cell_converts_list_defaults_to_json_cells() {
        let cell = default_value_to_cell(
            "items",
            "ids",
            &toml::Value::Array(vec![toml::Value::Integer(1), toml::Value::Integer(2)]),
        )
        .unwrap();

        assert_eq!(cell, "[1,2]");
    }

    #[test]
    fn default_value_to_cell_reports_context_for_unsupported_defaults() {
        let error = default_value_to_cell(
            "items",
            "stats",
            &toml::Value::Datetime("2026-01-01".parse().expect("valid datetime")),
        )
        .unwrap_err();

        assert!(error.to_string().contains("items"));
        assert!(error.to_string().contains("stats"));
    }
}
