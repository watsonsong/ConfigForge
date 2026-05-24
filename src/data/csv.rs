// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, path::Path};

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
        if headers.is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "csv `{}` has no header row",
                path.display()
            )));
        }
        validate_unique_headers(path, &headers)?;

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

    pub fn read_object_table(
        name: impl Into<String>,
        path: &Path,
        _fields: &[String],
    ) -> Result<DataTable> {
        let table_name = name.into();
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
        validate_object_headers(path, &headers)?;
        let mut seen_keys = BTreeSet::new();
        let mut row = DataRow::new();
        for (record_index, record) in reader.records().enumerate() {
            let record = record.map_err(|source| ConfigForgeError::ParseCsv {
                path: path.to_path_buf(),
                source,
            })?;
            let row_number = record_index + 2;
            let key = record.get(0).unwrap_or_default().trim();
            if key.is_empty() {
                return Err(ConfigForgeError::Validation(format!(
                    "csv `{}` object table `{}` row {} has empty key",
                    path.display(),
                    table_name,
                    row_number
                )));
            }
            if !seen_keys.insert(key.to_owned()) {
                return Err(ConfigForgeError::Validation(format!(
                    "csv `{}` object table `{}` row {} duplicates key `{}`",
                    path.display(),
                    table_name,
                    row_number,
                    key
                )));
            }
            row.insert(key.to_owned(), record.get(1).unwrap_or_default().to_owned());
        }

        Ok(DataTable::new(table_name, vec![row]))
    }
}

fn validate_unique_headers(path: &Path, headers: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for header in headers {
        if !seen.insert(header.as_str()) {
            return Err(ConfigForgeError::Validation(format!(
                "csv `{}` has duplicate header `{}`",
                path.display(),
                header
            )));
        }
    }

    Ok(())
}

fn validate_object_headers(path: &Path, headers: &[String]) -> Result<()> {
    if headers.len() != 2 {
        return Err(ConfigForgeError::Validation(format!(
            "csv `{}` object table header must contain exactly two columns named `key` and `value`",
            path.display()
        )));
    }
    let key = headers[0].trim();
    let value = headers[1].trim();
    if key != "key" || value != "value" {
        return Err(ConfigForgeError::Validation(format!(
            "csv `{}` object table header must contain exactly two columns named `key` and `value`, got `{},{}`",
            path.display(),
            headers[0],
            headers[1]
        )));
    }
    Ok(())
}
