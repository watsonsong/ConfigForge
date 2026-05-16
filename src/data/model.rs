use std::collections::{BTreeMap, HashMap};

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

impl DataTable {
    pub fn new(name: impl Into<String>, rows: Vec<DataRow>) -> Self {
        Self {
            name: name.into(),
            rows,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}
