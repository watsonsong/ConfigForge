// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use rayon::prelude::*;

use crate::{ConfigForgeError, Result};

use super::{
    CellSourceInfo, CsvDataReader, DataLoadPlan, DataProvider, DataRow, DataSet, DataSourceLayout,
    DataSourcePlan, DataTable, RawSourceData, RowSourceInfo, SourceReadPlan, SourceRole,
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
        self.read_sources_with_reader(plan, |source| self.load_source(source))
    }

    pub fn read_sources_with_pool(
        &self,
        plan: &SourceReadPlan,
        pool: &rayon::ThreadPool,
    ) -> Result<RawSourceData> {
        self.read_sources_with_reader_and_pool(plan, pool, |source| self.load_source(source))
    }

    pub(crate) fn read_sources_with_reader(
        &self,
        plan: &SourceReadPlan,
        reader: impl Fn(&DataSourcePlan) -> Result<DataTable>,
    ) -> Result<RawSourceData> {
        let mut loaded = Vec::new();
        for (source_index, source) in plan.sources().iter().enumerate() {
            loaded.push((source_index, source, reader(source)?));
        }
        Ok(RawSourceData::new(merge_loaded_sources(loaded)?))
    }

    pub(crate) fn read_sources_with_reader_and_pool(
        &self,
        plan: &SourceReadPlan,
        pool: &rayon::ThreadPool,
        reader: impl Fn(&DataSourcePlan) -> Result<DataTable> + Sync,
    ) -> Result<RawSourceData> {
        let mut loaded = pool.install(|| {
            plan.sources()
                .par_iter()
                .enumerate()
                .map(|(index, source)| (index, source, reader(source)))
                .collect::<Vec<_>>()
        });
        loaded.sort_by_key(|(index, _, _)| *index);

        let mut resolved = Vec::with_capacity(loaded.len());
        for (source_index, source, table) in loaded {
            resolved.push((source_index, source, table?));
        }
        Ok(RawSourceData::new(merge_loaded_sources(resolved)?))
    }

    pub fn load_source(&self, source: &DataSourcePlan) -> Result<DataTable> {
        let source_path = resolve_source_path(&self.data_root, source)?;
        match source.provider {
            DataProvider::Csv => match source.layout {
                DataSourceLayout::CollectionRecords => CsvDataReader::read_table_with_layout(
                    &source.table_name,
                    &source_path,
                    source.header_row,
                    source.data_start_row,
                ),
                DataSourceLayout::ObjectKeyValueRows => {
                    CsvDataReader::read_object_table_with_layout(
                        &source.table_name,
                        &source_path,
                        &source.fields,
                        source.header_row,
                        source.data_start_row,
                    )
                }
            },
        }
    }
}

fn resolve_source_path(data_root: &Path, source: &DataSourcePlan) -> Result<PathBuf> {
    let root = lexical_normalize_path(data_root);
    let joined = if source.path.is_absolute() {
        lexical_normalize_path(&source.path)
    } else {
        lexical_normalize_path(&root.join(&source.path))
    };
    if !(joined == root
        || joined.starts_with(&root)
        || filesystem_path_is_within_root(&joined, &root))
    {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` source `{}` escapes data root `{}`",
            source.table_name,
            source.path.display(),
            root.display()
        )));
    }
    let root_key = filesystem_aware_path_key(&root).unwrap_or_else(|| root.clone());
    let source_key = filesystem_aware_path_key(&joined).unwrap_or_else(|| joined.clone());
    if !(source_key == root_key || source_key.starts_with(&root_key)) {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` source `{}` escapes data root `{}` through filesystem alias",
            source.table_name,
            source.path.display(),
            root.display()
        )));
    }
    Ok(joined)
}

fn filesystem_path_is_within_root(path: &Path, root: &Path) -> bool {
    let root_key = filesystem_aware_path_key(root).unwrap_or_else(|| root.to_path_buf());
    let path_key = filesystem_aware_path_key(path).unwrap_or_else(|| path.to_path_buf());
    path_key == root_key || path_key.starts_with(root_key)
}

fn filesystem_aware_path_key(path: &Path) -> Option<PathBuf> {
    let mut suffix = Vec::new();
    let mut ancestor = path;
    while !ancestor.exists() {
        let file_name = ancestor.file_name()?;
        suffix.push(file_name.to_owned());
        ancestor = ancestor.parent()?;
    }
    let mut key = ancestor.canonicalize().ok()?;
    for component in suffix.iter().rev() {
        key.push(component);
    }
    Some(lexical_normalize_path(&key))
}

fn lexical_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => match normalized.components().next_back() {
                Some(std::path::Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(std::path::Component::ParentDir) | None => {
                    if !path.has_root() {
                        normalized.push(component.as_os_str());
                    }
                }
                Some(std::path::Component::Prefix(_))
                | Some(std::path::Component::RootDir)
                | Some(std::path::Component::CurDir) => {}
            },
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

pub(crate) fn merge_loaded_sources(
    loaded: Vec<(usize, &DataSourcePlan, DataTable)>,
) -> Result<DataSet> {
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
            tables.push(DataTable::new_with_sources_and_cells(
                table_name,
                pending.rows,
                pending.row_sources,
                pending.cell_sources,
            )?);
        }
    }
    DataSet::try_new(tables)
}

#[derive(Default)]
struct PendingTable {
    rows: Vec<DataRow>,
    row_sources: Vec<RowSourceInfo>,
    cell_sources: Vec<BTreeMap<String, CellSourceInfo>>,
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
            let cell_sources = table
                .cell_sources
                .get(row_index)
                .cloned()
                .unwrap_or_default();
            let source_row_number = table
                .row_sources
                .get(row_index)
                .map(|source| source.source_row_number)
                .unwrap_or(row_index + 1);
            let key = primary_key_for_row(source, source_row_number, &row)?;
            if let Some(key) = key {
                if self.primary_key_index.contains_key(&key) {
                    return Err(ConfigForgeError::Validation(format!(
                        "table `{}` source `{}` row {} duplicates primary key `{}` before override_sources can be applied",
                        source.table_name,
                        source.path.display(),
                        source_row_number,
                        format_key(&key)
                    )));
                }
                self.primary_key_index.insert(key, self.rows.len());
            }
            self.rows.push(row);
            self.cell_sources.push(cell_sources);
            self.row_sources.push(RowSourceInfo {
                source_index,
                source_path: source.path.clone(),
                source_row_number,
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
            let cell_sources = table
                .cell_sources
                .get(row_index)
                .cloned()
                .unwrap_or_default();
            let source_row_number = table
                .row_sources
                .get(row_index)
                .map(|source| source.source_row_number)
                .unwrap_or(row_index + 1);
            let key = primary_key_for_row(source, source_row_number, &row)?.ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "table `{}` override source `{}` row {} cannot be applied because no primary key fields were configured",
                    source.table_name,
                    source.path.display(),
                    source_row_number
                ))
            })?;
            let target_row = self.primary_key_index.get(&key).copied().ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "table `{}` override source `{}` row {} references missing base primary key `{}`",
                    source.table_name,
                    source.path.display(),
                    source_row_number,
                    format_key(&key)
                ))
            })?;
            let replaces = self.row_sources[target_row].trace();
            self.rows[target_row] = row;
            self.cell_sources[target_row] = cell_sources;
            self.row_sources[target_row] = RowSourceInfo {
                source_index,
                source_path: source.path.clone(),
                source_row_number,
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
    source_row_number: usize,
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
                source_row_number,
                field
            ))
        })?;
        if value.trim().is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` source `{}` row {} has empty primary index field `{}`",
                source.table_name,
                source.path.display(),
                source_row_number,
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
