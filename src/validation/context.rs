use std::collections::BTreeSet;

use crate::{
    ConfigForgeError, Result,
    data::{DataSet, DataTable},
    meta::{LinkedMetaModel, LinkedTableMeta, TargetTags},
};

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
                        let visible_rows = (0..table.rows.len())
                            .map(|row_index| visible_row_for(table, row_index))
                            .collect();
                        ExportTableView::new(table_meta, table, visible_fields, visible_rows)
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

            let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "data set is missing table `{}`",
                    table_meta.meta.name
                ))
            })?;
            let visible_fields = table_meta
                .fields
                .iter()
                .filter(|field| field.meta.is_exported_for(target_tags))
                .map(|field| field.meta.name.as_str())
                .collect();
            let visible_rows = table
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
            tables.push(ExportTableView::new(
                table_meta,
                table,
                visible_fields,
                visible_rows,
            ));
        }

        Ok(Self { tables })
    }

    pub fn table(&self, name: &str) -> Option<&ExportTableView<'meta, 'data>> {
        self.tables
            .iter()
            .find(|table| table.meta.meta.name == name)
    }
}

pub struct ExportTableView<'meta, 'data> {
    pub meta: &'meta LinkedTableMeta<'meta>,
    pub table: &'data DataTable,
    pub visible_fields: BTreeSet<&'meta str>,
    pub visible_rows: Vec<VisibleRow>,
}

impl<'meta, 'data> ExportTableView<'meta, 'data> {
    pub fn new(
        meta: &'meta LinkedTableMeta<'meta>,
        table: &'data DataTable,
        visible_fields: BTreeSet<&'meta str>,
        visible_rows: Vec<VisibleRow>,
    ) -> Self {
        Self {
            meta,
            table,
            visible_fields,
            visible_rows,
        }
    }

    pub fn is_field_visible(&self, field_name: &str) -> bool {
        self.visible_fields.contains(field_name)
    }
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
