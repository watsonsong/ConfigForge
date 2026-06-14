// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data::{DataRow, DataTable},
    meta::{LinkedFieldMeta, LinkedTableMeta, TableKind},
};

use super::ExportTableView;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyPolicy {
    RawCell,
    ScalarExport,
    TypedCanonical,
}

pub(crate) fn key_value_for_view(
    policy: KeyPolicy,
    table_view: &ExportTableView<'_, '_>,
    visible_row_index: usize,
    field_name: &str,
) -> String {
    match policy {
        KeyPolicy::RawCell => table_view
            .raw_cell(visible_row_index, field_name)
            .unwrap_or_default()
            .to_owned(),
        KeyPolicy::ScalarExport | KeyPolicy::TypedCanonical => table_view
            .resolved_value(visible_row_index, field_name)
            .and_then(|value| value.scalar_export_string())
            .or_else(|| {
                table_view
                    .raw_cell(visible_row_index, field_name)
                    .map(str::to_owned)
            })
            .unwrap_or_default(),
    }
}

pub(crate) fn raw_cell_key(row: &DataRow, fields: &[LinkedFieldMeta<'_>]) -> Vec<String> {
    fields
        .iter()
        .map(|field| raw_cell_value(row, &field.meta.name))
        .collect()
}

pub(crate) fn raw_cell_value(row: &DataRow, field_name: &str) -> String {
    row.get(field_name)
        .map(String::as_str)
        .unwrap_or_default()
        .to_owned()
}

pub(crate) fn raw_record_key(
    table_meta: &LinkedTableMeta<'_>,
    row: Option<&DataRow>,
) -> Option<String> {
    let row = row?;
    let index = table_meta.primary_index()?;
    raw_labeled_key(row, &index.fields)
}

pub(crate) fn raw_record_key_or_row_fallback(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    row_index: usize,
    row: &DataRow,
) -> String {
    match table_meta.meta.kind {
        TableKind::Object => "<object>".to_owned(),
        TableKind::Collection => raw_record_key(table_meta, Some(row)).unwrap_or_else(|| {
            table
                .row_source(row_index)
                .map(|source| format!("row={}", source.source_row_number))
                .unwrap_or_else(|| format!("row={}", row_index + 1))
        }),
    }
}

fn raw_labeled_key(row: &DataRow, fields: &[LinkedFieldMeta<'_>]) -> Option<String> {
    let parts = fields
        .iter()
        .map(|field| {
            row.get(&field.meta.name)
                .map(|value| format!("{}={value}", field.meta.name))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join(","))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::{
        data::{DataTable, ResolvedTable},
        meta::MetaModel,
        validation::{ExportTableView, VisibleRow},
    };

    use super::*;

    #[test]
    fn key_policy_keeps_raw_and_scalar_export_distinct() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                primary_index = "by_id"

                [[tables.fields]]
                name = "id"
                type = "int"
                default = 1

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true
                primary = true
            "#,
        )
        .expect("meta should load");
        let linked_meta = meta.link().expect("meta should link");
        let table_meta = linked_meta.table("items").expect("items table");
        let table = DataTable::new(
            "items",
            vec![
                BTreeMap::from([("id".to_owned(), "01".to_owned())]),
                BTreeMap::new(),
            ],
        );
        let resolved_table = crate::data::DataResolver::resolve_table(table_meta, &table, None)
            .expect("table should resolve");
        let view = ExportTableView::new(
            table_meta,
            &table,
            BTreeSet::from(["id"]),
            vec![
                VisibleRow {
                    row_index: 0,
                    source_row_number: Some(1),
                },
                VisibleRow {
                    row_index: 1,
                    source_row_number: Some(2),
                },
            ],
            Some(
                ResolvedTable::new(
                    resolved_table.name,
                    resolved_table.rows,
                    resolved_table.row_sources,
                )
                .expect("resolved table should build"),
            ),
        );

        assert_eq!(key_value_for_view(KeyPolicy::RawCell, &view, 0, "id"), "01");
        assert_eq!(
            key_value_for_view(KeyPolicy::ScalarExport, &view, 0, "id"),
            "01"
        );
        assert_eq!(key_value_for_view(KeyPolicy::RawCell, &view, 1, "id"), "");
        assert_eq!(
            key_value_for_view(KeyPolicy::ScalarExport, &view, 1, "id"),
            "1"
        );
        assert_eq!(raw_cell_key(&table.rows[0], &table_meta.fields), vec!["01"]);
        assert_eq!(
            raw_record_key(table_meta, table.rows.first()).as_deref(),
            Some("id=01")
        );
    }
}
