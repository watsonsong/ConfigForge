// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::{
    Result,
    data::{
        CellSourceInfo, DataResolver, DataSet, DataTable, ResolvedRow, ResolvedTable,
        ResolvedValue, ValueSourceTrace,
    },
    diagnostic::{DiagnosticLocation, code::CF_DATA_SOURCE_SCHEMA_INVALID},
    meta::{LinkedMetaModel, LinkedTableMeta, TargetTags},
};

use rayon::prelude::*;

pub struct ValidationContext<'meta, 'data> {
    pub linked_meta: &'meta LinkedMetaModel<'meta>,
    pub target_tags: Option<TargetTags>,
    pub full_data_set: &'data DataSet,
    pub export_view: ExportValidationView<'meta, 'data>,
}

impl<'meta, 'data> ValidationContext<'meta, 'data> {
    pub fn new(
        linked_meta: &'meta LinkedMetaModel<'meta>,
        target_tags: Option<TargetTags>,
        full_data_set: &'data DataSet,
        export_view: ExportValidationView<'meta, 'data>,
    ) -> Self {
        Self {
            linked_meta,
            target_tags,
            full_data_set,
            export_view,
        }
    }
}

pub struct ExportValidationView<'meta, 'data> {
    pub tables: Vec<ExportTableView<'meta, 'data>>,
}

impl<'meta, 'data> ExportValidationView<'meta, 'data> {
    pub fn full(meta: &'meta LinkedMetaModel<'meta>, data_set: &'data DataSet) -> Self {
        Self {
            tables: meta
                .tables
                .iter()
                .filter_map(|table_meta| {
                    data_set.table(&table_meta.meta.name).map(|table| {
                        let visible_fields = table_meta
                            .fields
                            .iter()
                            .map(|field| field.meta.name.as_str())
                            .collect();
                        let visible_rows: Vec<VisibleRow> = (0..table.rows.len())
                            .map(|row_index| visible_row_for(table, row_index))
                            .collect();
                        let resolved_table = resolved_table_for(
                            table_meta,
                            table,
                            None,
                            visible_rows.iter().map(|row| row.row_index),
                        );
                        ExportTableView::new(
                            table_meta,
                            table,
                            visible_fields,
                            visible_rows,
                            resolved_table,
                        )
                    })
                })
                .collect(),
        }
    }

    pub fn for_target(
        meta: &'meta LinkedMetaModel<'meta>,
        data_set: &'data DataSet,
        target_tags: &TargetTags,
    ) -> Result<Self> {
        let mut tables = Vec::new();
        for table_meta in &meta.tables {
            if !table_meta.meta.is_exported_for(target_tags) {
                continue;
            }

            let table = data_set
                .table(&table_meta.meta.name)
                .ok_or_else(|| missing_export_table_error(table_meta))?;
            let visible_fields = table_meta
                .fields
                .iter()
                .filter(|field| field.meta.is_exported_for(target_tags))
                .map(|field| field.meta.name.as_str())
                .collect();
            let visible_rows: Vec<VisibleRow> = table
                .rows
                .iter()
                .enumerate()
                .filter(|(_, row)| {
                    table_meta.meta.is_row_exported_for(target_tags, |field| {
                        row.get(field).map(String::as_str)
                    })
                })
                .map(|(row_index, _)| visible_row_for(table, row_index))
                .collect();
            let resolved_table = resolved_table_for(
                table_meta,
                table,
                Some(target_tags),
                visible_rows.iter().map(|row| row.row_index),
            );
            tables.push(ExportTableView::new(
                table_meta,
                table,
                visible_fields,
                visible_rows,
                resolved_table,
            ));
        }

        Ok(Self { tables })
    }

    pub fn for_target_with_pool(
        meta: &'meta LinkedMetaModel<'meta>,
        data_set: &'data DataSet,
        target_tags: &TargetTags,
        pool: &rayon::ThreadPool,
    ) -> Result<Self> {
        let mut table_views = pool.install(|| {
            meta.tables
                .par_iter()
                .enumerate()
                .map(|(index, table_meta)| {
                    (
                        index,
                        target_table_view(meta, table_meta, data_set, target_tags),
                    )
                })
                .collect::<Vec<_>>()
        });
        table_views.sort_by_key(|(index, _)| *index);

        let mut tables = Vec::new();
        for (_, table_view) in table_views {
            if let Some(table_view) = table_view? {
                tables.push(table_view);
            }
        }

        Ok(Self { tables })
    }

    pub fn table(&self, name: &str) -> Option<&ExportTableView<'meta, 'data>> {
        self.tables
            .iter()
            .find(|table| table.meta.meta.name == name)
    }
}

fn target_table_view<'meta, 'data>(
    _meta: &'meta LinkedMetaModel<'meta>,
    table_meta: &'meta LinkedTableMeta<'meta>,
    data_set: &'data DataSet,
    target_tags: &TargetTags,
) -> Result<Option<ExportTableView<'meta, 'data>>> {
    if !table_meta.meta.is_exported_for(target_tags) {
        return Ok(None);
    }

    let table = data_set
        .table(&table_meta.meta.name)
        .ok_or_else(|| missing_export_table_error(table_meta))?;
    let visible_fields = table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .map(|field| field.meta.name.as_str())
        .collect();
    let visible_rows: Vec<VisibleRow> = table
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            table_meta
                .meta
                .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
        })
        .map(|(row_index, _)| visible_row_for(table, row_index))
        .collect();
    let resolved_table = resolved_table_for(
        table_meta,
        table,
        Some(target_tags),
        visible_rows.iter().map(|row| row.row_index),
    );
    Ok(Some(ExportTableView::new(
        table_meta,
        table,
        visible_fields,
        visible_rows,
        resolved_table,
    )))
}

fn missing_export_table_error(table_meta: &LinkedTableMeta<'_>) -> crate::ConfigForgeError {
    super::validation_diagnostic_error(
        CF_DATA_SOURCE_SCHEMA_INVALID,
        "validation.export_view",
        format!("data set is missing table `{}`", table_meta.meta.name),
        DiagnosticLocation {
            table: Some(table_meta.meta.name.clone()),
            ..DiagnosticLocation::default()
        },
        "missing table",
        format!("data table `{}`", table_meta.meta.name),
        "Ensure data loading prepares every table exported for the current target.",
    )
}

pub struct ExportTableView<'meta, 'data> {
    pub meta: &'meta LinkedTableMeta<'meta>,
    pub table: &'data DataTable,
    pub visible_fields: BTreeSet<&'meta str>,
    pub visible_rows: Vec<VisibleRow>,
    resolved_table: Option<ResolvedTable>,
}

impl<'meta, 'data> ExportTableView<'meta, 'data> {
    pub fn new(
        meta: &'meta LinkedTableMeta<'meta>,
        table: &'data DataTable,
        visible_fields: BTreeSet<&'meta str>,
        visible_rows: Vec<VisibleRow>,
        resolved_table: Option<ResolvedTable>,
    ) -> Self {
        Self {
            meta,
            table,
            visible_fields,
            visible_rows,
            resolved_table,
        }
    }

    pub fn is_field_visible(&self, field_name: &str) -> bool {
        self.visible_fields.contains(field_name)
    }

    pub fn resolved_table(&self) -> Option<&ResolvedTable> {
        self.resolved_table.as_ref()
    }

    pub fn visible_raw_row(&self, visible_row_index: usize) -> Option<&'data crate::data::DataRow> {
        self.visible_rows
            .get(visible_row_index)
            .and_then(|visible_row| self.table.rows.get(visible_row.row_index))
    }

    pub fn visible_resolved_row(&self, visible_row_index: usize) -> Option<&ResolvedRow> {
        self.resolved_table
            .as_ref()
            .and_then(|resolved_table| resolved_table.rows.get(visible_row_index))
    }

    pub fn resolved_value(
        &self,
        visible_row_index: usize,
        field_name: &str,
    ) -> Option<&ResolvedValue> {
        self.visible_resolved_row(visible_row_index)
            .and_then(|row| row.get(field_name))
    }

    pub fn raw_cell(&self, visible_row_index: usize, field_path: &str) -> Option<&'data str> {
        self.visible_raw_row(visible_row_index)
            .and_then(|row| row.get(field_path).map(String::as_str))
    }

    pub fn field_source(
        &self,
        visible_row_index: usize,
        field_path: &str,
    ) -> Option<ResolvedFieldSource<'_>> {
        let visible_row = self.visible_rows.get(visible_row_index)?;
        let cell_source = self.table.cell_source(visible_row.row_index, field_path);
        let resolved_trace = self
            .resolved_value(visible_row_index, field_path)
            .map(|value| &value.source);
        if cell_source.is_none() && resolved_trace.is_none() {
            return None;
        }
        Some(ResolvedFieldSource {
            resolved_trace,
            cell_source,
        })
    }
}

pub struct ResolvedFieldSource<'data> {
    pub resolved_trace: Option<&'data ValueSourceTrace>,
    pub cell_source: Option<&'data CellSourceInfo>,
}

fn resolved_table_for(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: Option<&TargetTags>,
    row_indices: impl IntoIterator<Item = usize>,
) -> Option<ResolvedTable> {
    DataResolver::resolve_table_rows(table_meta, table, target_tags, row_indices).ok()
}

pub fn visible_row_for(table: &DataTable, row_index: usize) -> VisibleRow {
    VisibleRow {
        row_index,
        source_row_number: Some(
            table
                .row_source(row_index)
                .map(|source| source.source_row_number)
                .unwrap_or(row_index + 1),
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleRow {
    pub row_index: usize,
    pub source_row_number: Option<usize>,
}
