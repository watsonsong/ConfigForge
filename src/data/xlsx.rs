// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use calamine::{Data, DataRef, Reader, Xlsx};

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::{
        CF_DATA_DUPLICATE_HEADER, CF_DATA_SOURCE_READ_FAILED, CF_DATA_SOURCE_SCHEMA_INVALID,
    },
};

use super::{
    CellSourceInfo, DataRow, DataSourceLayout, DataSourcePlan, DataTable, RowSourceInfo, SourceRole,
};

type Workbook = Xlsx<BufReader<File>>;
type SharedWorkbook = Arc<Mutex<Option<Workbook>>>;
type SheetCells = BTreeMap<usize, BTreeMap<usize, SheetCell>>;

// Scoped to one source-read operation so subsequent builds observe saved workbooks.
#[derive(Default)]
pub(super) struct WorkbookCache {
    workbooks: Mutex<BTreeMap<PathBuf, SharedWorkbook>>,
}

struct SheetCell {
    value: Data,
    formula: bool,
}

impl WorkbookCache {
    pub(super) fn read(&self, source: &DataSourcePlan, path: &Path) -> Result<DataTable> {
        let cache_key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let workbook = {
            let mut workbooks = self.workbooks.lock().expect("workbook cache lock poisoned");
            Arc::clone(workbooks.entry(cache_key).or_default())
        };
        let (sheet, cells) = {
            let mut workbook = workbook.lock().expect("workbook lock poisoned");
            if workbook.is_none() {
                *workbook =
                    Some(calamine::open_workbook(path).map_err(|error| {
                        read_error(source, path, source.sheet.as_deref(), error)
                    })?);
            }
            let workbook = workbook.as_mut().expect("workbook opened");
            let names = workbook.sheet_names();
            let sheet = match source.sheet.as_deref() {
                Some(sheet) if names.iter().any(|name| name == sheet) => sheet.to_owned(),
                None if names.len() == 1 => names[0].clone(),
                _ => {
                    return Err(schema_error(
                        source,
                        path,
                        source.sheet.as_deref(),
                        None,
                        None,
                        format!(
                            "select an existing sheet explicitly when the workbook has multiple sheets; available sheets: {}",
                            names.join(", ")
                        ),
                    ));
                }
            };
            let mut reader = workbook
                .worksheet_cells_reader(&sheet)
                .map_err(|error| read_error(source, path, Some(&sheet), error))?;
            let mut cells = SheetCells::new();
            while let Some(cell) = reader
                .next_cell_with_formula_metadata()
                .map_err(|error| read_error(source, path, Some(&sheet), error))?
            {
                let row = cell.pos.0 as usize + 1;
                if row == source.header_row || row >= source.data_start_row {
                    cells.entry(row).or_default().insert(
                        cell.pos.1 as usize,
                        SheetCell {
                            value: owned_value(cell.value),
                            formula: cell.formula.is_some(),
                        },
                    );
                }
            }
            (sheet, cells)
        };
        read_table(source, path, &sheet, &cells)
    }
}

fn owned_value(value: DataRef<'_>) -> Data {
    match value {
        DataRef::SharedString(value) => Data::String(value.to_owned()),
        value => value.into(),
    }
}

fn read_table(
    source: &DataSourcePlan,
    path: &Path,
    sheet: &str,
    cells: &SheetCells,
) -> Result<DataTable> {
    if source.header_row == 0 || source.data_start_row <= source.header_row {
        return Err(schema_error(
            source,
            path,
            Some(sheet),
            None,
            None,
            "header_row must be 1-based and data_start_row must follow it",
        ));
    }
    let mut headers = Vec::new();
    let mut seen = BTreeSet::new();
    for (&column, cell) in cells.get(&source.header_row).into_iter().flatten() {
        let name = cell_text(source, path, sheet, source.header_row, column, Some(cell))?;
        if name.trim().is_empty() {
            continue;
        }
        if !seen.insert(name.clone()) {
            let mut diagnostic = xlsx_diagnostic(
                source,
                path,
                Some(sheet),
                Some(source.header_row),
                Some(column),
                format!("duplicate XLSX header `{name}`"),
            );
            diagnostic.code = CF_DATA_DUPLICATE_HEADER.to_owned();
            return Err(ConfigForgeError::from_diagnostic(diagnostic));
        }
        headers.push((column, name));
    }
    if headers.is_empty() {
        return Err(schema_error(
            source,
            path,
            Some(sheet),
            Some(source.header_row),
            None,
            "missing non-empty XLSX header row",
        ));
    }
    let object = source.layout == DataSourceLayout::ObjectKeyValueRows;
    if object {
        let names = headers
            .iter()
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>();
        if names != ["key", "value"] && names != ["key", "title", "value"] {
            return Err(schema_error(
                source,
                path,
                Some(sheet),
                Some(source.header_row),
                None,
                "ObjectTable headers must be key,value or key,title,value",
            ));
        }
    }
    if object {
        headers.retain(|(_, name)| name != "title");
    }
    let mut rows = Vec::new();
    let mut row_sources = Vec::new();
    let mut cell_sources = Vec::new();
    let mut object_row = DataRow::new();
    let mut object_cells = BTreeMap::new();
    let mut first_data_row = source.header_row;
    for (&row_number, row_cells) in cells.range(source.data_start_row..) {
        // Formatting-only cells and entirely empty rows do not create records.
        if !headers.iter().any(|(column, _)| {
            row_cells.get(column).is_some_and(|cell| {
                cell.formula
                    || match &cell.value {
                        Data::Empty => false,
                        Data::String(value) => !value.is_empty(),
                        _ => true,
                    }
            })
        }) {
            continue;
        }
        let mut row = DataRow::new();
        let mut sources = BTreeMap::new();
        for (column, name) in &headers {
            let value = cell_text(
                source,
                path,
                sheet,
                row_number,
                *column,
                row_cells.get(column),
            )?;
            row.insert(name.clone(), value);
            sources.insert(
                name.clone(),
                CellSourceInfo {
                    source_path: path.to_path_buf(),
                    source_sheet: Some(sheet.to_owned()),
                    source_row_number: row_number,
                    source_column_number: column + 1,
                    source_column_name: name.clone(),
                    column_index: *column,
                },
            );
        }
        if object {
            let key = row["key"].trim().to_owned();
            if key.is_empty() || object_row.contains_key(&key) {
                return Err(schema_error(
                    source,
                    path,
                    Some(sheet),
                    Some(row_number),
                    Some(headers[0].0),
                    format!("ObjectTable key `{key}` must be non-empty and unique"),
                ));
            }
            if object_row.is_empty() {
                first_data_row = row_number;
            }
            object_row.insert(
                key.to_owned(),
                row.remove("value").expect("value header validated"),
            );
            object_cells.insert(
                key.to_owned(),
                sources.remove("value").expect("value source recorded"),
            );
        } else {
            rows.push(row);
            row_sources.push(row_source(path, sheet, row_number));
            cell_sources.push(sources);
        }
    }
    if object {
        rows.push(object_row);
        row_sources.push(row_source(path, sheet, first_data_row));
        cell_sources.push(object_cells);
    }
    DataTable::new_with_sources_and_cells(&source.table_name, rows, row_sources, cell_sources)
}

fn row_source(path: &Path, sheet: &str, row: usize) -> RowSourceInfo {
    RowSourceInfo {
        source_index: 0,
        source_path: path.to_path_buf(),
        source_sheet: Some(sheet.to_owned()),
        source_row_number: row,
        fragments: Vec::new(),
        source_role: SourceRole::Base,
        replaces: None,
    }
}

fn cell_text(
    source: &DataSourcePlan,
    path: &Path,
    sheet: &str,
    row: usize,
    column: usize,
    cell: Option<&SheetCell>,
) -> Result<String> {
    let Some(cell) = cell else {
        return Ok(String::new());
    };
    let value = match &cell.value {
        Data::Empty if cell.formula => {
            return Err(schema_error(
                source,
                path,
                Some(sheet),
                Some(row),
                Some(column),
                "formula has no cached value; recalculate and save the workbook before exporting",
            ));
        }
        Data::Empty => String::new(),
        Data::String(value) => value.clone(),
        Data::Bool(value) => value.to_string(),
        Data::Int(value) => value.to_string(),
        Data::Float(value) if value.is_finite() => value.to_string(),
        value => {
            return Err(schema_error(
                source,
                path,
                Some(sheet),
                Some(row),
                Some(column),
                format!(
                    "unsupported XLSX cell value `{value}`; use text for dates/durations and fix Excel errors"
                ),
            ));
        }
    };
    Ok(value)
}

fn xlsx_diagnostic(
    source: &DataSourcePlan,
    path: &Path,
    sheet: Option<&str>,
    row: Option<usize>,
    column: Option<usize>,
    message: impl Into<String>,
) -> Diagnostic {
    let location = DiagnosticLocation {
        source_path: Some(path.to_path_buf()),
        source_sheet: sheet.map(str::to_owned),
        table: Some(source.table_name.clone()),
        source_row_number: row,
        source_column_number: column.map(|column| column + 1),
        column_index: column,
        ..DiagnosticLocation::default()
    };
    Diagnostic::new(CF_DATA_SOURCE_SCHEMA_INVALID, DiagnosticSeverity::Error, message, location)
        .with_source("data.xlsx")
        .with_domain("data")
        .with_expected("valid XLSX source with an unambiguous sheet and supported cell values")
        .with_hint("Check sheet/header/data row settings; recalculate and save formulas in a spreadsheet editor.")
}

fn schema_error(
    source: &DataSourcePlan,
    path: &Path,
    sheet: Option<&str>,
    row: Option<usize>,
    column: Option<usize>,
    message: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(xlsx_diagnostic(source, path, sheet, row, column, message))
}

fn read_error(
    source: &DataSourcePlan,
    path: &Path,
    sheet: Option<&str>,
    error: impl std::fmt::Display,
) -> ConfigForgeError {
    let mut diagnostic = xlsx_diagnostic(
        source,
        path,
        sheet,
        None,
        None,
        format!("failed to read XLSX: {error}"),
    );
    diagnostic.code = CF_DATA_SOURCE_READ_FAILED.to_owned();
    ConfigForgeError::from_diagnostic(diagnostic)
}

#[cfg(test)]
mod tests;
