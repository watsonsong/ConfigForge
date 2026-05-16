use std::path::PathBuf;

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

    pub fn load_source(&self, source: &DataSourcePlan) -> Result<DataTable> {
        match source.provider {
            DataProvider::Csv => {
                CsvDataReader::read_table(&source.table_name, &self.data_root.join(&source.path))
            }
        }
    }
}
