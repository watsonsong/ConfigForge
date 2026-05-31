// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

#[cfg(test)]
use std::collections::HashMap;

use crate::{ConfigForgeError, Result};

use super::RowSourceInfo;

#[derive(Debug, Clone, PartialEq)]
#[cfg(test)]
pub struct ResolvedDataSet {
    pub tables: Vec<ResolvedTable>,
    table_index: HashMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTable {
    pub name: String,
    pub rows: Vec<ResolvedRow>,
    pub row_sources: Vec<RowSourceInfo>,
}

pub type ResolvedRow = BTreeMap<String, ResolvedValue>;

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedValue {
    pub value: DataValue,
    pub source: ValueSourceTrace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrayElementValue {
    pub value: DataValue,
    pub raw_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataValue {
    Scalar(ScalarValue),
    Struct(BTreeMap<String, ResolvedValue>),
    Array(Vec<ArrayElementValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Enum(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSourceKind {
    ExplicitCell,
    DefaultValue,
    ComputedEmptyObject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueSourceTrace {
    pub raw_field_path: String,
    pub kind: ValueSourceKind,
    pub source_index: Option<usize>,
    pub source_path: Option<PathBuf>,
    pub source_row_number: Option<usize>,
    pub raw_value: Option<String>,
}

#[cfg(test)]
impl ResolvedDataSet {
    pub fn try_new(tables: Vec<ResolvedTable>) -> Result<Self> {
        let mut table_index = HashMap::new();
        for (index, table) in tables.iter().enumerate() {
            if table_index.insert(table.name.clone(), index).is_some() {
                return Err(ConfigForgeError::Validation(format!(
                    "resolved data set declares duplicate table `{}`",
                    table.name
                )));
            }
        }

        Ok(Self {
            tables,
            table_index,
        })
    }

    pub fn table(&self, name: &str) -> Option<&ResolvedTable> {
        self.table_index
            .get(name)
            .and_then(|index| self.tables.get(*index))
    }
}

impl ResolvedTable {
    pub fn new(
        name: impl Into<String>,
        rows: Vec<ResolvedRow>,
        row_sources: Vec<RowSourceInfo>,
    ) -> Result<Self> {
        let name = name.into();
        if rows.len() != row_sources.len() {
            return Err(ConfigForgeError::Validation(format!(
                "resolved table `{}` has {} rows but {} row source records",
                name,
                rows.len(),
                row_sources.len()
            )));
        }
        Ok(Self {
            name,
            rows,
            row_sources,
        })
    }
}

impl ResolvedValue {
    pub fn explicit(
        raw_field_path: impl Into<String>,
        value: DataValue,
        raw_value: impl Into<String>,
        row_source: Option<&RowSourceInfo>,
    ) -> Self {
        Self {
            value,
            source: ValueSourceTrace::new(
                raw_field_path,
                ValueSourceKind::ExplicitCell,
                Some(raw_value.into()),
                row_source,
            ),
        }
    }

    pub fn defaulted(
        raw_field_path: impl Into<String>,
        value: DataValue,
        raw_value: impl Into<String>,
        row_source: Option<&RowSourceInfo>,
    ) -> Self {
        Self {
            value,
            source: ValueSourceTrace::new(
                raw_field_path,
                ValueSourceKind::DefaultValue,
                Some(raw_value.into()),
                row_source,
            ),
        }
    }

    pub fn computed_empty_object(
        raw_field_path: impl Into<String>,
        value: DataValue,
        row_source: Option<&RowSourceInfo>,
    ) -> Self {
        Self {
            value,
            source: ValueSourceTrace::new(
                raw_field_path,
                ValueSourceKind::ComputedEmptyObject,
                None,
                row_source,
            ),
        }
    }

    pub fn scalar_export_string(&self) -> Option<String> {
        self.source.raw_value.clone().or_else(|| match &self.value {
            DataValue::Scalar(value) => Some(value.to_export_string()),
            _ => None,
        })
    }
}

impl ArrayElementValue {
    pub fn new(value: DataValue, raw_value: Option<String>) -> Self {
        Self { value, raw_value }
    }

    pub fn scalar(scalar: ScalarValue, raw_value: Option<String>) -> Self {
        Self::new(DataValue::Scalar(scalar), raw_value)
    }

    pub fn scalar_export_string(&self) -> Option<String> {
        self.raw_value.clone().or_else(|| match &self.value {
            DataValue::Scalar(value) => Some(value.to_export_string()),
            _ => None,
        })
    }
}

impl ScalarValue {
    pub fn to_export_string(&self) -> String {
        match self {
            Self::String(value) | Self::Enum(value) => value.clone(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
        }
    }
}

impl ValueSourceTrace {
    pub fn new(
        raw_field_path: impl Into<String>,
        kind: ValueSourceKind,
        raw_value: Option<String>,
        row_source: Option<&RowSourceInfo>,
    ) -> Self {
        Self {
            raw_field_path: raw_field_path.into(),
            kind,
            source_index: row_source.map(|source| source.source_index),
            source_path: row_source.map(|source| source.source_path.clone()),
            source_row_number: row_source.map(|source| source.source_row_number),
            raw_value,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;

    #[test]
    fn array_element_is_lighter_than_full_resolved_value() {
        assert!(size_of::<ArrayElementValue>() < size_of::<ResolvedValue>());
    }

    #[test]
    fn array_element_keeps_scalar_export_fallback_without_source_trace() {
        let explicit = ArrayElementValue::scalar(ScalarValue::Int(7), Some("007".to_owned()));
        assert_eq!(explicit.scalar_export_string().as_deref(), Some("007"));

        let fallback = ArrayElementValue::scalar(ScalarValue::Bool(true), None);
        assert_eq!(fallback.scalar_export_string().as_deref(), Some("true"));
    }
}
