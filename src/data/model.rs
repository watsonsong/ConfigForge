// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use crate::{ConfigForgeError, Result};

pub type DataRow = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSet {
    pub tables: Vec<DataTable>,
    table_index: HashMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataTable {
    pub name: String,
    pub rows: Vec<DataRow>,
    pub row_sources: Vec<RowSourceInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSourceData {
    data_set: DataSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSourceInfo {
    pub source_index: usize,
    pub source_path: PathBuf,
    pub source_row_number: usize,
    pub fragments: Vec<String>,
    pub source_role: SourceRole,
    pub replaces: Option<RowSourceTrace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRole {
    Base,
    Override,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSourceTrace {
    pub source_index: usize,
    pub source_path: PathBuf,
    pub source_row_number: usize,
}

impl RowSourceInfo {
    pub fn trace(&self) -> RowSourceTrace {
        RowSourceTrace {
            source_index: self.source_index,
            source_path: self.source_path.clone(),
            source_row_number: self.source_row_number,
        }
    }
}

impl DataSet {
    pub fn new(tables: Vec<DataTable>) -> Self {
        Self::try_new(tables).expect("data set should not contain duplicate table names")
    }

    pub fn try_new(tables: Vec<DataTable>) -> Result<Self> {
        let mut table_index = HashMap::new();
        for (index, table) in tables.iter().enumerate() {
            if table_index.insert(table.name.clone(), index).is_some() {
                return Err(ConfigForgeError::Validation(format!(
                    "data set declares duplicate table `{}`",
                    table.name
                )));
            }
        }

        Ok(Self {
            tables,
            table_index,
        })
    }

    pub fn has_duplicate_table_names(tables: &[DataTable]) -> bool {
        let table_index = tables
            .iter()
            .map(|table| table.name.as_str())
            .collect::<std::collections::HashSet<_>>();
        table_index.len() != tables.len()
    }

    pub fn table(&self, name: &str) -> Option<&DataTable> {
        self.table_index
            .get(name)
            .and_then(|index| self.tables.get(*index))
    }

    pub fn tables(&self) -> &[DataTable] {
        &self.tables
    }
}

impl RawSourceData {
    pub fn new(data_set: DataSet) -> Self {
        Self { data_set }
    }

    pub fn as_data_set(&self) -> &DataSet {
        &self.data_set
    }

    pub fn into_data_set(self) -> DataSet {
        self.data_set
    }
}

impl DataTable {
    pub fn new(name: impl Into<String>, rows: Vec<DataRow>) -> Self {
        let row_sources = default_row_sources(rows.len());
        Self {
            name: name.into(),
            rows,
            row_sources,
        }
    }

    pub fn new_with_sources(
        name: impl Into<String>,
        rows: Vec<DataRow>,
        row_sources: Vec<RowSourceInfo>,
    ) -> Result<Self> {
        let name = name.into();
        if rows.len() != row_sources.len() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` has {} rows but {} row source records",
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

    pub fn row_source(&self, row_index: usize) -> Option<&RowSourceInfo> {
        self.row_sources.get(row_index)
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn default_row_sources(row_count: usize) -> Vec<RowSourceInfo> {
    (0..row_count)
        .map(|row_index| RowSourceInfo {
            source_index: 0,
            source_path: PathBuf::from("<memory>"),
            source_row_number: row_index + 1,
            fragments: Vec::new(),
            source_role: SourceRole::Base,
            replaces: None,
        })
        .collect()
}
