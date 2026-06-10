// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, path::Path};

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::{
        CF_DATA_DUPLICATE_HEADER, CF_DATA_SOURCE_READ_FAILED, CF_DATA_SOURCE_SCHEMA_INVALID,
    },
};

use super::{CellSourceInfo, DataRow, DataTable, RowSourceInfo, SourceRole};

pub struct CsvDataReader;

impl CsvDataReader {
    pub fn read_table(name: impl Into<String>, path: &Path) -> Result<DataTable> {
        Self::read_table_with_layout(name, path, 1, 3)
    }

    pub fn read_table_with_layout(
        name: impl Into<String>,
        path: &Path,
        header_row: usize,
        data_start_row: usize,
    ) -> Result<DataTable> {
        validate_csv_row_layout(path, header_row, data_start_row)?;
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_path(path)
            .map_err(|source| csv_read_error(path, source))?;

        let mut headers = None;
        let mut rows = Vec::new();
        let mut row_sources = Vec::new();
        let mut cell_sources = Vec::new();
        for (record_index, record) in reader.records().enumerate() {
            let source_row_number = record_index + 1;
            let record = record.map_err(|source| csv_read_error(path, source))?;

            if source_row_number == header_row {
                let parsed_headers = record.iter().map(str::to_owned).collect::<Vec<_>>();
                if parsed_headers.is_empty() {
                    return Err(csv_schema_error(
                        path,
                        Some(header_row),
                        None,
                        format!("csv `{}` has no header row", path.display()),
                        "<empty>",
                        "non-empty CSV header row",
                    ));
                }
                validate_unique_headers(path, &parsed_headers)?;
                headers = Some(parsed_headers);
                continue;
            }

            if source_row_number < data_start_row {
                continue;
            }

            let headers = headers.as_ref().ok_or_else(|| {
                csv_schema_error(
                    path,
                    Some(header_row),
                    None,
                    format!(
                        "csv `{}` has no header row at 1-based row {}",
                        path.display(),
                        header_row
                    ),
                    "<missing>",
                    "CSV header row before data rows",
                )
            })?;
            let mut row = DataRow::new();
            let mut row_cell_sources = std::collections::BTreeMap::new();
            for (column_index, (header, value)) in headers.iter().zip(record.iter()).enumerate() {
                row.insert(header.clone(), value.to_owned());
                row_cell_sources.insert(
                    header.clone(),
                    CellSourceInfo {
                        source_row_number,
                        source_column_number: column_index + 1,
                        source_column_name: header.clone(),
                        column_index,
                    },
                );
            }
            rows.push(row);
            row_sources.push(default_csv_row_source(path, source_row_number));
            cell_sources.push(row_cell_sources);
        }

        if headers.is_none() {
            return Err(csv_schema_error(
                path,
                Some(header_row),
                None,
                format!(
                    "csv `{}` has no header row at 1-based row {}",
                    path.display(),
                    header_row
                ),
                "<missing>",
                "CSV header row before data rows",
            ));
        }

        DataTable::new_with_sources_and_cells(name, rows, row_sources, cell_sources)
    }

    pub fn read_object_table(
        name: impl Into<String>,
        path: &Path,
        fields: &[String],
    ) -> Result<DataTable> {
        Self::read_object_table_with_layout(name, path, fields, 1, 2)
    }

    pub fn read_object_table_with_layout(
        name: impl Into<String>,
        path: &Path,
        _fields: &[String],
        header_row: usize,
        data_start_row: usize,
    ) -> Result<DataTable> {
        validate_csv_row_layout(path, header_row, data_start_row)?;
        let table_name = name.into();
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_path(path)
            .map_err(|source| csv_read_error(path, source))?;

        let mut value_column_index = None;
        let mut seen_keys = BTreeSet::new();
        let mut row = DataRow::new();
        let mut row_cell_sources = std::collections::BTreeMap::new();
        for (record_index, record) in reader.records().enumerate() {
            let row_number = record_index + 1;
            let record = record.map_err(|source| csv_read_error(path, source))?;

            if row_number == header_row {
                let headers = record.iter().map(str::to_owned).collect::<Vec<_>>();
                value_column_index = Some(validate_object_headers(path, &headers)?);
                continue;
            }

            if row_number < data_start_row {
                continue;
            }

            let value_column_index = value_column_index.ok_or_else(|| {
                csv_schema_error(
                    path,
                    Some(header_row),
                    None,
                    format!(
                        "csv `{}` has no header row at 1-based row {}",
                        path.display(),
                        header_row
                    ),
                    "<missing>",
                    "ObjectTable key/value CSV header row",
                )
            })?;

            if record.len() <= value_column_index {
                return Err(csv_schema_error(
                    path,
                    Some(row_number),
                    Some((value_column_index, "value")),
                    format!(
                        "csv `{}` object table `{}` row {} has no value column `{}`",
                        path.display(),
                        table_name,
                        row_number,
                        value_column_index + 1
                    ),
                    format!("{} columns", record.len()),
                    "ObjectTable value column",
                ));
            }
            let key = record.get(0).unwrap_or_default().trim();
            if key.is_empty() {
                return Err(csv_schema_error(
                    path,
                    Some(row_number),
                    Some((0, "key")),
                    format!(
                        "csv `{}` object table `{}` row {} has empty key",
                        path.display(),
                        table_name,
                        row_number
                    ),
                    "<empty>",
                    "non-empty ObjectTable key",
                ));
            }
            if !seen_keys.insert(key.to_owned()) {
                return Err(csv_schema_error(
                    path,
                    Some(row_number),
                    Some((0, "key")),
                    format!(
                        "csv `{}` object table `{}` row {} duplicates key `{}`",
                        path.display(),
                        table_name,
                        row_number,
                        key
                    ),
                    key,
                    "unique ObjectTable key",
                ));
            }
            row.insert(
                key.to_owned(),
                record
                    .get(value_column_index)
                    .unwrap_or_default()
                    .to_owned(),
            );
            row_cell_sources.insert(
                key.to_owned(),
                CellSourceInfo {
                    source_row_number: row_number,
                    source_column_number: value_column_index + 1,
                    source_column_name: "value".to_owned(),
                    column_index: value_column_index,
                },
            );
        }

        if value_column_index.is_none() {
            return Err(csv_schema_error(
                path,
                Some(header_row),
                None,
                format!(
                    "csv `{}` has no header row at 1-based row {}",
                    path.display(),
                    header_row
                ),
                "<missing>",
                "ObjectTable key/value CSV header row",
            ));
        }

        let source_row_number = if row.is_empty() {
            header_row
        } else {
            data_start_row
        };
        DataTable::new_with_sources_and_cells(
            table_name,
            vec![row],
            vec![default_csv_row_source(path, source_row_number)],
            vec![row_cell_sources],
        )
    }
}

fn validate_csv_row_layout(path: &Path, header_row: usize, data_start_row: usize) -> Result<()> {
    if header_row == 0 {
        return Err(csv_schema_error(
            path,
            None,
            None,
            format!(
                "csv `{}` declares `header_row = 0`, but source row positions are 1-based",
                path.display()
            ),
            "header_row = 0",
            "1-based header_row",
        ));
    }
    if data_start_row == 0 {
        return Err(csv_schema_error(
            path,
            None,
            None,
            format!(
                "csv `{}` declares `data_start_row = 0`, but source row positions are 1-based",
                path.display()
            ),
            "data_start_row = 0",
            "1-based data_start_row",
        ));
    }
    if data_start_row <= header_row {
        return Err(csv_schema_error(
            path,
            Some(data_start_row),
            None,
            format!(
                "csv `{}` declares `data_start_row = {data_start_row}` before or at `header_row = {header_row}`; data rows must start after the header row",
                path.display()
            ),
            format!("data_start_row = {data_start_row}, header_row = {header_row}"),
            "data_start_row greater than header_row",
        ));
    }
    Ok(())
}

fn default_csv_row_source(path: &Path, source_row_number: usize) -> RowSourceInfo {
    RowSourceInfo {
        source_index: 0,
        source_path: path.to_path_buf(),
        source_row_number,
        fragments: Vec::new(),
        source_role: SourceRole::Base,
        replaces: None,
    }
}

fn validate_unique_headers(path: &Path, headers: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (column_index, header) in headers.iter().enumerate() {
        if !seen.insert(header.as_str()) {
            return Err(csv_duplicate_header_error(path, column_index, header));
        }
    }

    Ok(())
}

fn validate_object_headers(path: &Path, headers: &[String]) -> Result<usize> {
    if headers.len() != 2 && headers.len() != 3 {
        return Err(csv_schema_error(
            path,
            Some(1),
            None,
            format!(
                "csv `{}` object table header must contain either `key,value` or `key,title,value` columns",
                path.display()
            ),
            headers.join(","),
            "`key,value` or `key,title,value` header",
        ));
    }
    let key = headers[0].trim();
    let value_index = headers.len() - 1;
    let value = headers[value_index].trim();
    if key != "key" || value != "value" {
        return Err(csv_schema_error(
            path,
            Some(1),
            None,
            format!(
                "csv `{}` object table header must contain either `key,value` or `key,title,value` columns, got `{}`",
                path.display(),
                headers.join(",")
            ),
            headers.join(","),
            "`key,value` or `key,title,value` header",
        ));
    }
    Ok(value_index)
}

fn csv_read_error(path: &Path, source: csv::Error) -> ConfigForgeError {
    let source_row_number = source.position().map(|position| position.line() as usize);
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_SOURCE_READ_FAILED,
            DiagnosticSeverity::Error,
            format!("failed to read csv `{}`: {source}", path.display()),
            DiagnosticLocation {
                source_path: Some(path.to_path_buf()),
                source_row_number,
                ..DiagnosticLocation::default()
            },
        )
        .with_source("data.csv")
        .with_domain("data")
        .with_current_value(source.to_string())
        .with_expected("valid CSV source"),
    )
}

fn csv_schema_error(
    path: &Path,
    source_row_number: Option<usize>,
    column: Option<(usize, &str)>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    let mut location = DiagnosticLocation {
        source_path: Some(path.to_path_buf()),
        source_row_number,
        ..DiagnosticLocation::default()
    };
    if let Some((column_index, column_name)) = column {
        location.column_index = Some(column_index);
        location.source_column_number = Some(column_index + 1);
        location.source_column_name = Some(column_name.to_owned());
    }
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_SOURCE_SCHEMA_INVALID,
            DiagnosticSeverity::Error,
            message,
            location,
        )
        .with_source("data.csv")
        .with_domain("data")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint("Check source row layout and CSV header conventions."),
    )
}

fn csv_duplicate_header_error(path: &Path, column_index: usize, header: &str) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_DUPLICATE_HEADER,
            DiagnosticSeverity::Error,
            format!("csv `{}` has duplicate header `{header}`", path.display()),
            DiagnosticLocation {
                source_path: Some(path.to_path_buf()),
                source_row_number: Some(1),
                source_column_number: Some(column_index + 1),
                source_column_name: Some(header.to_owned()),
                column_index: Some(column_index),
                ..DiagnosticLocation::default()
            },
        )
        .with_source("data.csv")
        .with_domain("data")
        .with_current_value(header)
        .with_expected("unique CSV header name")
        .with_hint("Rename or remove duplicate CSV columns."),
    )
}
