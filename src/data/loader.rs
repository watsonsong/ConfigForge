use std::{collections::BTreeMap, path::PathBuf};

use rayon::prelude::*;

use crate::Result;

use super::{
    CsvDataReader, DataLoadPlan, DataProvider, DataRow, DataSet, DataSourceLayout, DataSourcePlan,
    DataTable, RowSourceInfo,
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
        let mut loaded = Vec::new();
        for (source_index, source) in plan.sources().iter().enumerate() {
            loaded.push((source_index, source, self.load_source(source)?));
        }
        Self::merge_loaded_sources(loaded)
    }

    pub fn load_with_pool(&self, plan: &DataLoadPlan, pool: &rayon::ThreadPool) -> Result<DataSet> {
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
        Self::merge_loaded_sources(resolved)
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
        let mut merged: BTreeMap<String, (Vec<DataRow>, Vec<RowSourceInfo>)> = BTreeMap::new();
        for (source_index, source, table) in loaded {
            if !merged.contains_key(&source.table_name) {
                table_order.push(source.table_name.clone());
            }
            let (rows, row_sources) = merged.entry(source.table_name.clone()).or_default();
            for (row_index, row) in table.rows.into_iter().enumerate() {
                rows.push(row);
                row_sources.push(RowSourceInfo {
                    source_index,
                    source_path: source.path.clone(),
                    source_row_number: row_index + 1,
                    fragments: source.fragments.clone(),
                });
            }
        }

        let mut tables = Vec::with_capacity(table_order.len());
        for table_name in table_order {
            let (rows, row_sources) = merged
                .remove(&table_name)
                .expect("table was inserted into merged map");
            tables.push(DataTable::new_with_sources(table_name, rows, row_sources)?);
        }
        DataSet::try_new(tables)
    }
}
