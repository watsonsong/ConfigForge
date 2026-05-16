use std::path::Path;

use crate::{ConfigForgeError, Result};

use super::{DataRow, DataTable};

pub struct CsvDataReader;

impl CsvDataReader {
    pub fn read_table(name: impl Into<String>, path: &Path) -> Result<DataTable> {
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

        Ok(DataTable::new(name, rows))
    }
}
