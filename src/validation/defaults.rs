// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    data::{DataSet, DataTable, value_conversion::default_value_to_cell},
    meta::{LinkedMetaModel, LinkedTableMeta},
};

use rayon::prelude::*;

use super::fragment_field_key;

pub fn normalize_data_set(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<DataSet> {
    normalize_data_set_with_normalizer(data_set, |table| normalize_table_for_data_set(meta, table))
}

pub fn normalize_data_set_with_pool(
    meta: &LinkedMetaModel<'_>,
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
) -> Result<DataSet> {
    normalize_data_set_with_normalizer_and_pool(data_set, pool, |table| {
        normalize_table_for_data_set(meta, table)
    })
}

pub fn normalize_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<DataTable> {
    apply_field_defaults(meta, table)
}

fn normalize_table_for_data_set(
    meta: &LinkedMetaModel<'_>,
    table: &DataTable,
) -> Result<DataTable> {
    if let Some(table_meta) = meta.table(&table.name) {
        normalize_table(table_meta, table)
    } else {
        Ok(table.clone())
    }
}

fn normalize_data_set_with_normalizer(
    data_set: &DataSet,
    normalizer: impl Fn(&DataTable) -> Result<DataTable>,
) -> Result<DataSet> {
    let mut tables = Vec::with_capacity(data_set.tables().len());
    for table in data_set.tables() {
        tables.push(normalizer(table)?);
    }
    DataSet::try_new(tables)
}

pub(crate) fn normalize_data_set_with_normalizer_and_pool(
    data_set: &DataSet,
    pool: &rayon::ThreadPool,
    normalizer: impl Fn(&DataTable) -> Result<DataTable> + Sync,
) -> Result<DataSet> {
    let mut normalized = pool.install(|| {
        data_set
            .tables()
            .par_iter()
            .enumerate()
            .map(|(index, table)| (index, normalizer(table)))
            .collect::<Vec<_>>()
    });
    normalized.sort_by_key(|(index, _)| *index);

    let mut tables = Vec::with_capacity(normalized.len());
    for (_, table) in normalized {
        tables.push(table?);
    }
    DataSet::try_new(tables)
}

pub fn apply_field_defaults(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<DataTable> {
    let mut table = table.clone();
    for row_index in 0..table.rows.len() {
        let active_fragments = table
            .row_source(row_index)
            .map(|source| source.fragments.clone())
            .unwrap_or_default();
        let row = &mut table.rows[row_index];
        for field in &meta.fields {
            let needs_default = row
                .get(&field.meta.name)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true);
            if !needs_default {
                continue;
            }

            if let Some(default) = &field.meta.default {
                row.insert(
                    field.meta.name.clone(),
                    default_value_to_cell(&meta.meta.name, &field.meta.name, default)?,
                );
            }
        }

        for fragment_name in active_fragments {
            let Some(fragment) = meta.fragment(&fragment_name) else {
                continue;
            };
            if fragment.struct_type().is_some() {
                continue;
            }
            for field in &fragment.fields {
                let key = fragment_field_key(fragment.meta.name.as_str(), field.meta.name.as_str());
                let needs_default = row
                    .get(&key)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true);
                if !needs_default {
                    continue;
                }

                if let Some(default) = &field.meta.default {
                    row.insert(
                        key,
                        default_value_to_cell(&meta.meta.name, &field.meta.name, default)?,
                    );
                }
            }
        }
    }

    Ok(table)
}
