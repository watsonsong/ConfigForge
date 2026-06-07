// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{cmp::Ordering, collections::BTreeMap};

use crate::{
    ConfigForgeError, Result,
    meta::{LinkedTableMeta, ValidatorMeta},
};

use super::common::{
    ScalarFieldKind, compare_values, is_empty_value, linked_field, optional_string_array,
    scalar_field_kind,
};
use crate::validation::ExportTableView;

#[derive(Debug, Clone)]
pub(crate) struct GroupByConfig {
    fields: Vec<GroupField>,
}

impl GroupByConfig {
    pub(crate) fn compile(
        meta: &ValidatorMeta,
        table: &LinkedTableMeta<'_>,
        kind: &str,
    ) -> Result<Self> {
        let fields = optional_string_array(meta, kind, "group_by")?.unwrap_or_default();
        let fields = fields
            .into_iter()
            .map(|field| {
                let field_meta = linked_field(table, kind, &field)?;
                let field_kind = scalar_field_kind(field_meta).ok_or_else(|| {
                    ConfigForgeError::Validation(format!(
                        "table `{}` validator `{kind}` group_by field `{field}` must be scalar",
                        table.meta.name
                    ))
                })?;
                if !field_kind.is_stable_key_capable() {
                    return Err(ConfigForgeError::Validation(format!(
                        "table `{}` validator `{kind}` group_by field `{field}` must be stable-key capable",
                        table.meta.name
                    )));
                }
                Ok(GroupField { name: field })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { fields })
    }

    pub(crate) fn key_for(&self, row: &crate::data::DataRow) -> GroupKey {
        if self.fields.is_empty() {
            return GroupKey::WholeTable;
        }
        GroupKey::Fields(
            self.fields
                .iter()
                .map(|field| GroupKeyPart {
                    field: field.name.clone(),
                    value: row.get(&field.name).cloned().unwrap_or_default(),
                })
                .collect(),
        )
    }
}

#[derive(Debug, Clone)]
struct GroupField {
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GroupKey {
    WholeTable,
    Fields(Vec<GroupKeyPart>),
}

impl GroupKey {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::WholeTable => "<all>".to_owned(),
            Self::Fields(parts) => parts
                .iter()
                .map(|part| format!("{}={}", part.field, part.value))
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GroupKeyPart {
    field: String,
    value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GroupedRows {
    groups: BTreeMap<GroupKey, Vec<GroupedRow>>,
}

impl GroupedRows {
    pub(crate) fn from_table_view(
        table_view: &ExportTableView<'_, '_>,
        group_by: &GroupByConfig,
    ) -> Self {
        let mut groups = BTreeMap::new();
        for visible_row in &table_view.visible_rows {
            let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
                continue;
            };
            groups
                .entry(group_by.key_for(row))
                .or_insert_with(Vec::new)
                .push(GroupedRow {
                    row_index: visible_row.row_index,
                    source_row_number: visible_row.source_row_number,
                });
        }
        Self { groups }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&GroupKey, &[GroupedRow])> {
        self.groups.iter().map(|(key, rows)| (key, rows.as_slice()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupedRow {
    pub(crate) row_index: usize,
    pub(crate) source_row_number: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct OrderByConfig {
    field: String,
    kind: ScalarFieldKind,
}

impl OrderByConfig {
    pub(crate) fn compile(table: &LinkedTableMeta<'_>, kind: &str, field: String) -> Result<Self> {
        let field_meta = linked_field(table, kind, &field)?;
        let field_kind = scalar_field_kind(field_meta).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "table `{}` validator `{kind}` order_by field `{field}` must be scalar",
                table.meta.name
            ))
        })?;
        if !field_kind.is_stable_sort_capable() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` validator `{kind}` order_by field `{field}` must be stable-sort capable",
                table.meta.name
            )));
        }
        Ok(Self {
            field,
            kind: field_kind,
        })
    }

    pub(crate) fn field(&self) -> &str {
        &self.field
    }

    pub(crate) fn compare_values(&self, left: &str, right: &str) -> Option<Ordering> {
        compare_values(self.kind, left, right)
    }

    pub(crate) fn sort_rows(
        &self,
        table_view: &ExportTableView<'_, '_>,
        rows: &[GroupedRow],
    ) -> Vec<OrderedRow> {
        let mut rows = rows
            .iter()
            .filter_map(|grouped_row| {
                let row = table_view.table.rows.get(grouped_row.row_index)?;
                let value = row.get(&self.field).cloned().unwrap_or_default();
                Some(OrderedRow {
                    row_index: grouped_row.row_index,
                    source_row_number: grouped_row.source_row_number,
                    order_value: value,
                })
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            compare_values(self.kind, &left.order_value, &right.order_value)
                .unwrap_or(Ordering::Equal)
                .then(left.row_index.cmp(&right.row_index))
        });
        rows
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OrderedRow {
    pub(crate) row_index: usize,
    pub(crate) source_row_number: Option<usize>,
    pub(crate) order_value: String,
}

pub(crate) fn parse_numeric_value(
    kind: &str,
    field: &str,
    value: &str,
    skip_empty: bool,
) -> Result<Option<f64>> {
    if is_empty_value(value) {
        if skip_empty {
            return Ok(None);
        }
        return Err(ConfigForgeError::Validation(format!(
            "validator `{kind}` field `{field}` value must not be empty"
        )));
    }
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(Some)
        .ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "validator `{kind}` field `{field}` value `{value}` must be finite number"
            ))
        })
}
