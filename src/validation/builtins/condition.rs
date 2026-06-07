// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ConfigForgeError, Result, data::DataRow, meta::LinkedTableMeta};

use super::common::{
    is_empty_value, linked_field, literal_array_to_strings, literal_to_string, row_value,
};

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
            return Err(ConfigForgeError::Validation(format!(
                "validator `{kind}` parameter `when` must be a table"
            )));
        };

        let composite_keys = ["all", "any", "none"]
            .into_iter()
            .filter(|key| table_value.contains_key(*key))
            .collect::<Vec<_>>();
        if !composite_keys.is_empty() {
            if composite_keys.len() != 1 || table_value.len() != 1 {
                return Err(ConfigForgeError::Validation(format!(
                    "validator `{kind}` parameter `when` cannot mix composite and leaf conditions"
                )));
            }
            let key = composite_keys[0];
            let Some(items) = table_value.get(key).and_then(toml::Value::as_array) else {
                return Err(ConfigForgeError::Validation(format!(
                    "validator `{kind}` parameter `when.{key}` must be an array"
                )));
            };
            if items.is_empty() {
                return Err(ConfigForgeError::Validation(format!(
                    "validator `{kind}` parameter `when.{key}` must not be empty"
                )));
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

    pub(super) fn matches(&self, row: &DataRow) -> bool {
        match self {
            Self::Leaf(leaf) => leaf.matches(row),
            Self::All(children) => children.iter().all(|child| child.matches(row)),
            Self::Any(children) => children.iter().any(|child| child.matches(row)),
            Self::None(children) => children.iter().all(|child| !child.matches(row)),
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
            return Err(ConfigForgeError::Validation(format!(
                "validator `{kind}` parameter `when.field` must be string"
            )));
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
            return Err(ConfigForgeError::Validation(format!(
                "validator `{kind}` parameter `when` must set exactly one condition operation"
            )));
        }
        for key in value.keys() {
            if key != "field" && !operation_keys.contains(&key.as_str()) {
                return Err(ConfigForgeError::Validation(format!(
                    "validator `{kind}` parameter `when` has unknown key `{key}`"
                )));
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
                ConfigForgeError::Validation(format!(
                    "validator `{kind}` parameter `when.is_empty` must be bool"
                ))
            })?),
            "not_empty" => LeafOperation::NotEmpty(raw.as_bool().ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "validator `{kind}` parameter `when.not_empty` must be bool"
                ))
            })?),
            _ => unreachable!(),
        };

        Ok(Self {
            field: field.to_owned(),
            operation,
        })
    }

    fn matches(&self, row: &DataRow) -> bool {
        let value = row_value(row, &self.field);
        match &self.operation {
            LeafOperation::Equals(expected) => value == expected,
            LeafOperation::NotEquals(expected) => value != expected,
            LeafOperation::In(expected) => expected.iter().any(|expected| expected == value),
            LeafOperation::NotIn(expected) => expected.iter().all(|expected| expected != value),
            LeafOperation::IsEmpty(expected) => is_empty_value(value) == *expected,
            LeafOperation::NotEmpty(expected) => !is_empty_value(value) == *expected,
        }
    }
}
