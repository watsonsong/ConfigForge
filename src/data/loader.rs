// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use rayon::prelude::*;

use crate::{ConfigForgeError, Result};

use super::{
    CsvDataReader, DataLoadPlan, DataProvider, DataRow, DataSet, DataSourceLayout, DataSourcePlan,
    DataTable, RawSourceData, RowSourceInfo, SourceReadPlan, SourceRole,
};

#[derive(Debug, Clone)]
pub struct DataLoader {
    data_root: PathBuf,
}

impl DataLoader {
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        Self {
            data_root: data_root.into(),
        }
    }

    pub fn load(&self, plan: &DataLoadPlan) -> Result<DataSet> {
        Ok(self.read_sources(plan.source_read_plan())?.into_data_set())
    }

    pub fn load_with_pool(&self, plan: &DataLoadPlan, pool: &rayon::ThreadPool) -> Result<DataSet> {
        Ok(self
            .read_sources_with_pool(plan.source_read_plan(), pool)?
            .into_data_set())
    }

    pub fn read_sources(&self, plan: &SourceReadPlan) -> Result<RawSourceData> {
        let mut loaded = Vec::new();
        for (source_index, source) in plan.sources().iter().enumerate() {
            loaded.push((source_index, source, self.load_source(source)?));
        }
        Ok(RawSourceData::new(Self::merge_loaded_sources(loaded)?))
    }

    pub fn read_sources_with_pool(
        &self,
        plan: &SourceReadPlan,
        pool: &rayon::ThreadPool,
    ) -> Result<RawSourceData> {
        let mut loaded = pool.install(|| {
            plan.sources()
                .par_iter()
                .enumerate()
                .map(|(index, source)| (index, source, self.load_source(source)))
                .collect::<Vec<_>>()
        });
        loaded.sort_by_key(|(index, _, _)| *index);

        let mut resolved = Vec::with_capacity(loaded.len());
        for (source_index, source, table) in loaded {
            resolved.push((source_index, source, table?));
        }
        Ok(RawSourceData::new(Self::merge_loaded_sources(resolved)?))
    }

    pub fn load_source(&self, source: &DataSourcePlan) -> Result<DataTable> {
        match source.provider {
            DataProvider::Csv => match source.layout {
                DataSourceLayout::CollectionRecords => CsvDataReader::read_table(
                    &source.table_name,
                    &self.data_root.join(&source.path),
                ),
                DataSourceLayout::ObjectKeyValueRows => CsvDataReader::read_object_table(
                    &source.table_name,
                    &self.data_root.join(&source.path),
                    &source.fields,
                ),
            },
        }
    }

    fn merge_loaded_sources(loaded: Vec<(usize, &DataSourcePlan, DataTable)>) -> Result<DataSet> {
        let mut table_order = Vec::new();
        let mut merged: BTreeMap<String, PendingTable> = BTreeMap::new();
        for (source_index, source, table) in loaded {
            if !merged.contains_key(&source.table_name) {
                table_order.push(source.table_name.clone());
            }
            let pending = merged.entry(source.table_name.clone()).or_default();
            pending.apply_source(source_index, source, table)?;
        }

        let mut tables = Vec::with_capacity(table_order.len());
        for table_name in table_order {
            if let Some(pending) = merged.remove(&table_name) {
                tables.push(DataTable::new_with_sources(
                    table_name,
                    pending.rows,
                    pending.row_sources,
                )?);
            }
        }
        DataSet::try_new(tables)
    }
}

#[derive(Default)]
struct PendingTable {
    rows: Vec<DataRow>,
    row_sources: Vec<RowSourceInfo>,
    primary_key_index: BTreeMap<Vec<String>, usize>,
}

impl PendingTable {
    fn apply_source(
        &mut self,
        source_index: usize,
        source: &DataSourcePlan,
        table: DataTable,
    ) -> Result<()> {
        match source.role {
            SourceRole::Base => self.append_base_rows(source_index, source, table),
            SourceRole::Override => self.apply_override_rows(source_index, source, table),
        }
    }

    fn append_base_rows(
        &mut self,
        source_index: usize,
        source: &DataSourcePlan,
        table: DataTable,
    ) -> Result<()> {
        for (row_index, row) in table.rows.into_iter().enumerate() {
            let key = primary_key_for_row(source, row_index, &row)?;
            if let Some(key) = key {
                if self.primary_key_index.contains_key(&key) {
                    return Err(ConfigForgeError::Validation(format!(
                        "table `{}` source `{}` row {} duplicates primary key `{}` before override_sources can be applied",
                        source.table_name,
                        source.path.display(),
                        row_index + 1,
                        format_key(&key)
                    )));
                }
                self.primary_key_index.insert(key, self.rows.len());
            }
            self.rows.push(row);
            self.row_sources.push(RowSourceInfo {
                source_index,
                source_path: source.path.clone(),
                source_row_number: row_index + 1,
                fragments: source.fragments.clone(),
                source_role: SourceRole::Base,
                replaces: None,
            });
        }
        Ok(())
    }

    fn apply_override_rows(
        &mut self,
        source_index: usize,
        source: &DataSourcePlan,
        table: DataTable,
    ) -> Result<()> {
        for (row_index, row) in table.rows.into_iter().enumerate() {
            let key = primary_key_for_row(source, row_index, &row)?.ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "table `{}` override source `{}` row {} cannot be applied because no primary key fields were configured",
                    source.table_name,
                    source.path.display(),
                    row_index + 1
                ))
            })?;
            let target_row = self.primary_key_index.get(&key).copied().ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "table `{}` override source `{}` row {} references missing base primary key `{}`",
                    source.table_name,
                    source.path.display(),
                    row_index + 1,
                    format_key(&key)
                ))
            })?;
            let replaces = self.row_sources[target_row].trace();
            self.rows[target_row] = row;
            self.row_sources[target_row] = RowSourceInfo {
                source_index,
                source_path: source.path.clone(),
                source_row_number: row_index + 1,
                fragments: source.fragments.clone(),
                source_role: SourceRole::Override,
                replaces: Some(replaces),
            };
        }
        Ok(())
    }
}

fn primary_key_for_row(
    source: &DataSourcePlan,
    row_index: usize,
    row: &DataRow,
) -> Result<Option<Vec<String>>> {
    if source.primary_key_fields.is_empty() {
        return Ok(None);
    }
    let mut key = Vec::with_capacity(source.primary_key_fields.len());
    for field in &source.primary_key_fields {
        let value = row.get(field).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "table `{}` source `{}` row {} is missing primary index field `{}`",
                source.table_name,
                source.path.display(),
                row_index + 1,
                field
            ))
        })?;
        if value.trim().is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` source `{}` row {} has empty primary index field `{}`",
                source.table_name,
                source.path.display(),
                row_index + 1,
                field
            )));
        }
        key.push(value.clone());
    }
    Ok(Some(key))
}

fn format_key(key: &[String]) -> String {
    key.join(",")
}
