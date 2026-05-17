use std::path::PathBuf;

use rayon::prelude::*;

use crate::Result;

use super::{CsvDataReader, DataLoadPlan, DataProvider, DataSet, DataSourcePlan, DataTable};

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
        let mut tables = Vec::new();
        for source in plan.sources() {
            tables.push(self.load_source(source)?);
        }
        DataSet::try_new(tables)
    }

    pub fn load_with_pool(&self, plan: &DataLoadPlan, pool: &rayon::ThreadPool) -> Result<DataSet> {
        let mut loaded = pool.install(|| {
            plan.sources()
                .par_iter()
                .enumerate()
                .map(|(index, source)| (index, self.load_source(source)))
                .collect::<Vec<_>>()
        });
        loaded.sort_by_key(|(index, _)| *index);

        let mut tables = Vec::with_capacity(loaded.len());
        for (_, table) in loaded {
            tables.push(table?);
        }
        DataSet::try_new(tables)
    }

    pub fn load_source(&self, source: &DataSourcePlan) -> Result<DataTable> {
        match source.provider {
            DataProvider::Csv => {
                CsvDataReader::read_table(&source.table_name, &self.data_root.join(&source.path))
            }
        }
    }
}
