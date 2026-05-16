use std::{collections::BTreeMap, path::Path};

use crate::{ConfigForgeError, Result};

pub type DataRow = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataTable {
    pub name: String,
    pub rows: Vec<DataRow>,
}

impl DataTable {
    pub fn from_csv_path(name: impl Into<String>, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut reader =
            csv::Reader::from_path(path).map_err(|source| ConfigForgeError::ParseCsv {
                path: path.to_path_buf(),
                source,
            })?;

        let headers = reader
            .headers()
            .map_err(|source| ConfigForgeError::ParseCsv {
                path: path.to_path_buf(),
                source,
            })?
            .iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();

        let mut rows = Vec::new();
        for record in reader.records() {
            let record = record.map_err(|source| ConfigForgeError::ParseCsv {
                path: path.to_path_buf(),
                source,
            })?;
            let mut row = DataRow::new();
            for (header, value) in headers.iter().zip(record.iter()) {
                row.insert(header.clone(), value.to_owned());
            }
            rows.push(row);
        }

        Ok(Self {
            name: name.into(),
            rows,
        })
    }
}
