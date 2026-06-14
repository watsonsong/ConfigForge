// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{Result, meta::LinkedTableMeta};

use super::common::{
    field_value, is_empty_value, linked_field, literal_array_to_strings, literal_to_string,
    validator_config_error,
};
use crate::validation::ExportTableView;

pub(super) enum RowCondition {
    Leaf(LeafCondition),
    All(Vec<RowCondition>),
    Any(Vec<RowCondition>),
    None(Vec<RowCondition>),
}

impl RowCondition {
    pub(super) fn compile(
        kind: &str,
        table: &LinkedTableMeta<'_>,
        value: &toml::Value,
    ) -> Result<Self> {
        let Some(table_value) = value.as_table() else {
            return Err(validator_config_error(
                kind,
                format!("validator `{kind}` parameter `when` must be a table"),
                value.to_string(),
                "condition table",
                "Use a TOML inline table or table value for `when`.",
            ));
        };

        let composite_keys = ["all", "any", "none"]
            .into_iter()
            .filter(|key| table_value.contains_key(*key))
            .collect::<Vec<_>>();
        if !composite_keys.is_empty() {
            if composite_keys.len() != 1 || table_value.len() != 1 {
                return Err(validator_config_error(
                    kind,
                    format!(
                        "validator `{kind}` parameter `when` cannot mix composite and leaf conditions"
                    ),
                    table_value.keys().cloned().collect::<Vec<_>>().join(","),
                    "one composite key or one leaf condition",
                    "Use only one of `all`, `any`, `none`, or a single leaf condition.",
                ));
            }
            let key = composite_keys[0];
            let Some(items) = table_value.get(key).and_then(toml::Value::as_array) else {
                return Err(validator_config_error(
                    kind,
                    format!("validator `{kind}` parameter `when.{key}` must be an array"),
                    table_value
                        .get(key)
                        .map_or_else(|| "missing value".to_owned(), ToString::to_string),
                    "array of conditions",
                    "Provide a non-empty array of nested conditions.",
                ));
            };
            if items.is_empty() {
                return Err(validator_config_error(
                    kind,
                    format!("validator `{kind}` parameter `when.{key}` must not be empty"),
                    "empty array",
                    "non-empty array of conditions",
                    "Add at least one nested condition.",
                ));
            }
            let conditions = items
                .iter()
                .map(|item| Self::compile(kind, table, item))
                .collect::<Result<Vec<_>>>()?;
            return Ok(match key {
                "all" => Self::All(conditions),
                "any" => Self::Any(conditions),
                "none" => Self::None(conditions),
                _ => unreachable!(),
            });
        }

        Ok(Self::Leaf(LeafCondition::compile(
            kind,
            table,
            table_value,
        )?))
    }

    pub(super) fn matches(
        &self,
        table_view: &ExportTableView<'_, '_>,
        visible_row_index: usize,
    ) -> bool {
        match self {
            Self::Leaf(leaf) => leaf.matches(table_view, visible_row_index),
            Self::All(children) => children
                .iter()
                .all(|child| child.matches(table_view, visible_row_index)),
            Self::Any(children) => children
                .iter()
                .any(|child| child.matches(table_view, visible_row_index)),
            Self::None(children) => children
                .iter()
                .all(|child| !child.matches(table_view, visible_row_index)),
        }
    }
}

enum LeafOperation {
    Equals(String),
    NotEquals(String),
    In(Vec<String>),
    NotIn(Vec<String>),
    IsEmpty(bool),
    NotEmpty(bool),
}

pub(super) struct LeafCondition {
    field: String,
    operation: LeafOperation,
}

impl LeafCondition {
    fn compile(
        kind: &str,
        table: &LinkedTableMeta<'_>,
        value: &toml::value::Table,
    ) -> Result<Self> {
        let Some(field) = value.get("field").and_then(toml::Value::as_str) else {
            return Err(validator_config_error(
                kind,
                format!("validator `{kind}` parameter `when.field` must be string"),
                value
                    .get("field")
                    .map_or_else(|| "missing parameter".to_owned(), ToString::to_string),
                "field name string",
                "Set `when.field` to a field name declared on the table.",
            ));
        };
        linked_field(table, kind, field)?;

        let operation_keys = [
            "equals",
            "not_equals",
            "in",
            "not_in",
            "is_empty",
            "not_empty",
        ]
        .into_iter()
        .filter(|key| value.contains_key(*key))
        .collect::<Vec<_>>();
        if operation_keys.len() != 1 {
            return Err(validator_config_error(
                kind,
                format!(
                    "validator `{kind}` parameter `when` must set exactly one condition operation"
                ),
                operation_keys.join(","),
                "exactly one condition operation",
                "Set one of equals, not_equals, in, not_in, is_empty, or not_empty.",
            ));
        }
        for key in value.keys() {
            if key != "field" && !operation_keys.contains(&key.as_str()) {
                return Err(validator_config_error(
                    kind,
                    format!("validator `{kind}` parameter `when` has unknown key `{key}`"),
                    key,
                    "field or a supported condition operation",
                    "Remove the unknown condition key or fix its spelling.",
                ));
            }
        }
        let key = operation_keys[0];
        let raw = value.get(key).expect("operation key exists");
        let operation = match key {
            "equals" => LeafOperation::Equals(literal_to_string(kind, "when.equals", raw)?),
            "not_equals" => {
                LeafOperation::NotEquals(literal_to_string(kind, "when.not_equals", raw)?)
            }
            "in" => LeafOperation::In(literal_array_to_strings(kind, "when.in", raw)?),
            "not_in" => LeafOperation::NotIn(literal_array_to_strings(kind, "when.not_in", raw)?),
            "is_empty" => LeafOperation::IsEmpty(raw.as_bool().ok_or_else(|| {
                validator_config_error(
                    kind,
                    format!("validator `{kind}` parameter `when.is_empty` must be bool"),
                    raw.to_string(),
                    "bool",
                    "Use true or false for `when.is_empty`.",
                )
            })?),
            "not_empty" => LeafOperation::NotEmpty(raw.as_bool().ok_or_else(|| {
                validator_config_error(
                    kind,
                    format!("validator `{kind}` parameter `when.not_empty` must be bool"),
                    raw.to_string(),
                    "bool",
                    "Use true or false for `when.not_empty`.",
                )
            })?),
            _ => unreachable!(),
        };

        Ok(Self {
            field: field.to_owned(),
            operation,
        })
    }

    fn matches(&self, table_view: &ExportTableView<'_, '_>, visible_row_index: usize) -> bool {
        let value = field_value(table_view, visible_row_index, &self.field);
        let value = value.as_str();
        match &self.operation {
            LeafOperation::Equals(expected) => value == expected,
            LeafOperation::NotEquals(expected) => value != expected,
            LeafOperation::In(expected) => expected.iter().any(|expected| expected == value),
            LeafOperation::NotIn(expected) => expected.iter().all(|expected| expected != value),
            LeafOperation::IsEmpty(expected) => is_empty_value(value) == *expected,
            LeafOperation::NotEmpty(expected) => is_empty_value(value) != *expected,
        }
    }
}
