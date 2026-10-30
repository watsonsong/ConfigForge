// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use rayon::prelude::*;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::{
        CF_DATA_DUPLICATE_KEY, CF_DATA_SOURCE_PATH_ESCAPE, CF_DATA_SOURCE_SCHEMA_INVALID,
    },
    path_util::{
        filesystem_aware_path_key, filesystem_path_is_within_root, lexical_normalize_path,
    },
};

use super::{
    CellSourceInfo, CsvDataReader, DataLoadPlan, DataProvider, DataRow, DataSet, DataSourceLayout,
    DataSourcePlan, DataTable, RawSourceData, RowSourceInfo, SourceReadPlan, SourceRole,
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
        Ok(self.read_sources(plan.source_read_plan())?.into_data_set())
    }

    pub fn load_with_pool(&self, plan: &DataLoadPlan, pool: &rayon::ThreadPool) -> Result<DataSet> {
        Ok(self
            .read_sources_with_pool(plan.source_read_plan(), pool)?
            .into_data_set())
    }

    pub fn read_sources(&self, plan: &SourceReadPlan) -> Result<RawSourceData> {
        self.read_sources_with_reader(plan, |source| self.load_source(source))
    }

    pub fn read_sources_with_pool(
        &self,
        plan: &SourceReadPlan,
        pool: &rayon::ThreadPool,
    ) -> Result<RawSourceData> {
        self.read_sources_with_reader_and_pool(plan, pool, |source| self.load_source(source))
    }

    pub(crate) fn read_sources_with_reader(
        &self,
        plan: &SourceReadPlan,
        reader: impl Fn(&DataSourcePlan) -> Result<DataTable>,
    ) -> Result<RawSourceData> {
        let mut loaded = Vec::new();
        for (source_index, source) in plan.sources().iter().enumerate() {
            loaded.push((source_index, source, reader(source)?));
        }
        Ok(RawSourceData::new(merge_loaded_sources(loaded)?))
    }

    pub(crate) fn read_sources_with_reader_and_pool(
        &self,
        plan: &SourceReadPlan,
        pool: &rayon::ThreadPool,
        reader: impl Fn(&DataSourcePlan) -> Result<DataTable> + Sync,
    ) -> Result<RawSourceData> {
        let loaded = pool.install(|| {
            plan.sources()
                .par_iter()
                .enumerate()
                .map(|(index, source)| (index, source, reader(source)))
                .collect::<Vec<_>>()
        });

        let mut resolved = Vec::with_capacity(loaded.len());
        for (source_index, source, table) in loaded {
            resolved.push((source_index, source, table?));
        }
        Ok(RawSourceData::new(merge_loaded_sources(resolved)?))
    }

    pub fn load_source(&self, source: &DataSourcePlan) -> Result<DataTable> {
        let source_path = resolve_source_path(&self.data_root, source)?;
        match source.provider {
            DataProvider::Csv => match source.layout {
                DataSourceLayout::CollectionRecords => CsvDataReader::read_table_with_layout(
                    &source.table_name,
                    &source_path,
                    source.header_row,
                    source.data_start_row,
                ),
                DataSourceLayout::ObjectKeyValueRows => {
                    CsvDataReader::read_object_table_with_layout(
                        &source.table_name,
                        &source_path,
                        &source.fields,
                        source.header_row,
                        source.data_start_row,
                    )
                }
            },
        }
    }
}

fn resolve_source_path(data_root: &Path, source: &DataSourcePlan) -> Result<PathBuf> {
    let root = lexical_normalize_path(data_root);
    let joined = if source.path.is_absolute() {
        lexical_normalize_path(&source.path)
    } else {
        lexical_normalize_path(&root.join(&source.path))
    };
    if !(joined == root
        || joined.starts_with(&root)
        || filesystem_path_is_within_root(&joined, &root))
    {
        return Err(source_path_escape_error(
            source,
            &root,
            format!(
                "table `{}` source `{}` escapes data root `{}`",
                source.table_name,
                source.path.display(),
                root.display()
            ),
            "path within data root",
        ));
    }
    let root_key = filesystem_aware_path_key(&root).unwrap_or_else(|| root.clone());
    let source_key = filesystem_aware_path_key(&joined).unwrap_or_else(|| joined.clone());
    if !(source_key == root_key || source_key.starts_with(&root_key)) {
        return Err(source_path_escape_error(
            source,
            &root,
            format!(
                "table `{}` source `{}` escapes data root `{}` through filesystem alias",
                source.table_name,
                source.path.display(),
                root.display()
            ),
            "filesystem path within data root without alias escape",
        ));
    }
    Ok(joined)
}

pub(crate) fn merge_loaded_sources(
    loaded: Vec<(usize, &DataSourcePlan, DataTable)>,
) -> Result<DataSet> {
    let mut table_order = Vec::new();
    let mut merged: BTreeMap<String, PendingTable> = BTreeMap::new();
    for (source_index, source, table) in loaded {
        if !merged.contains_key(&source.table_name) {
            table_order.push(source.table_name.clone());
        }
        let pending = merged.entry(source.table_name.clone()).or_default();
        pending.apply_source(source_index, source, table)?;
    }

    let mut tables = Vec::with_capacity(table_order.len());
    for table_name in table_order {
        if let Some(pending) = merged.remove(&table_name) {
            tables.push(DataTable::new_with_sources_and_cells(
                table_name,
                pending.rows,
                pending.row_sources,
                pending.cell_sources,
            )?);
        }
    }
    DataSet::try_new(tables)
}

#[derive(Default)]
struct PendingTable {
    rows: Vec<DataRow>,
    row_sources: Vec<RowSourceInfo>,
    cell_sources: Vec<BTreeMap<String, CellSourceInfo>>,
    primary_key_index: BTreeMap<Vec<String>, usize>,
}

impl PendingTable {
    fn apply_source(
        &mut self,
        source_index: usize,
        source: &DataSourcePlan,
        table: DataTable,
    ) -> Result<()> {
        match (source.layout, source.role) {
            (DataSourceLayout::CollectionRecords, SourceRole::Base) => {
                self.append_base_rows(source_index, source, table)
            }
            (DataSourceLayout::CollectionRecords, SourceRole::Override) => {
                self.apply_override_rows(source_index, source, table)
            }
            (DataSourceLayout::ObjectKeyValueRows, SourceRole::Base) => {
                self.merge_object_base(source_index, source, table)
            }
            (DataSourceLayout::ObjectKeyValueRows, SourceRole::Override) => {
                self.apply_object_override(source_index, source, table)
            }
        }
    }

    fn append_base_rows(
        &mut self,
        source_index: usize,
        source: &DataSourcePlan,
        table: DataTable,
    ) -> Result<()> {
        for (row_index, row) in table.rows.into_iter().enumerate() {
            let cell_sources = table
                .cell_sources
                .get(row_index)
                .cloned()
                .unwrap_or_default();
            let cell_sources = retag_cell_sources(cell_sources, source);
            let source_row_number = table
                .row_sources
                .get(row_index)
                .map(|source| source.source_row_number)
                .unwrap_or(row_index + 1);
            let key = primary_key_for_row(source, source_row_number, &row)?;
            if let Some(key) = key {
                if self.primary_key_index.contains_key(&key) {
                    return Err(source_duplicate_key_error(
                        source,
                        source_row_number,
                        format!(
                            "table `{}` source `{}` row {} duplicates primary key `{}` before override_sources can be applied",
                            source.table_name,
                            source.path.display(),
                            source_row_number,
                            format_key(&key)
                        ),
                        format_key(&key),
                    ));
                }
                self.primary_key_index.insert(key, self.rows.len());
            }
            self.rows.push(row);
            self.cell_sources.push(cell_sources);
            self.row_sources.push(RowSourceInfo {
                source_index,
                source_path: source.path.clone(),
                source_row_number,
                fragments: source.fragments.clone(),
                source_role: SourceRole::Base,
                replaces: None,
            });
        }
        Ok(())
    }

    fn apply_override_rows(
        &mut self,
        source_index: usize,
        source: &DataSourcePlan,
        table: DataTable,
    ) -> Result<()> {
        for (row_index, row) in table.rows.into_iter().enumerate() {
            let cell_sources = table
                .cell_sources
                .get(row_index)
                .cloned()
                .unwrap_or_default();
            let cell_sources = retag_cell_sources(cell_sources, source);
            let source_row_number = table
                .row_sources
                .get(row_index)
                .map(|source| source.source_row_number)
                .unwrap_or(row_index + 1);
            let key = primary_key_for_row(source, source_row_number, &row)?.ok_or_else(|| {
                source_schema_error(
                    source,
                    Some(source_row_number),
                    format!(
                        "table `{}` override source `{}` row {} cannot be applied because no primary key fields were configured",
                        source.table_name,
                        source.path.display(),
                        source_row_number
                    ),
                    "<missing primary key fields>",
                    "primary index fields for override_sources",
                )
            })?;
            let target_row = self.primary_key_index.get(&key).copied().ok_or_else(|| {
                source_schema_error(
                    source,
                    Some(source_row_number),
                    format!(
                        "table `{}` override source `{}` row {} references missing base primary key `{}`",
                        source.table_name,
                        source.path.display(),
                        source_row_number,
                        format_key(&key)
                    ),
                    format_key(&key),
                    "existing base primary key",
                )
            })?;
            let replaces = self.row_sources[target_row].trace();
            self.rows[target_row] = row;
            self.cell_sources[target_row] = cell_sources;
            self.row_sources[target_row] = RowSourceInfo {
                source_index,
                source_path: source.path.clone(),
                source_row_number,
                fragments: source.fragments.clone(),
                source_role: SourceRole::Override,
                replaces: Some(replaces),
            };
        }
        Ok(())
    }

    fn merge_object_base(
        &mut self,
        source_index: usize,
        source: &DataSourcePlan,
        table: DataTable,
    ) -> Result<()> {
        let (row, source_row_number, cell_sources) = object_singleton_parts(table, source)?;
        let cell_sources = retag_cell_sources(cell_sources, source);
        if self.rows.is_empty() {
            self.rows.push(DataRow::new());
            self.cell_sources.push(BTreeMap::new());
            self.row_sources.push(RowSourceInfo {
                source_index,
                source_path: source.path.clone(),
                source_row_number,
                fragments: source.fragments.clone(),
                source_role: SourceRole::Base,
                replaces: None,
            });
        }

        for (field_path, value) in row {
            if self.rows[0].contains_key(&field_path) {
                let duplicate_row_number = cell_sources
                    .get(&field_path)
                    .map(|cell| cell.source_row_number)
                    .unwrap_or(source_row_number);
                return Err(object_duplicate_key_error(
                    source,
                    duplicate_row_number,
                    &field_path,
                ));
            }
            if let Some(cell_source) = cell_sources.get(&field_path).cloned() {
                self.cell_sources[0].insert(field_path.clone(), cell_source);
            }
            self.rows[0].insert(field_path, value);
        }
        Ok(())
    }

    fn apply_object_override(
        &mut self,
        source_index: usize,
        source: &DataSourcePlan,
        table: DataTable,
    ) -> Result<()> {
        let (row, source_row_number, cell_sources) = object_singleton_parts(table, source)?;
        let cell_sources = retag_cell_sources(cell_sources, source);
        if self.rows.is_empty() {
            self.rows.push(DataRow::new());
            self.cell_sources.push(BTreeMap::new());
            self.row_sources.push(RowSourceInfo {
                source_index,
                source_path: source.path.clone(),
                source_row_number,
                fragments: source.fragments.clone(),
                source_role: SourceRole::Override,
                replaces: None,
            });
        }

        for (field_path, value) in row {
            if !self.rows[0].contains_key(&field_path) {
                let missing_row_number = cell_sources
                    .get(&field_path)
                    .map(|cell| cell.source_row_number)
                    .unwrap_or(source_row_number);
                return Err(object_source_schema_error(
                    source,
                    Some(missing_row_number),
                    format!(
                        "table `{}` override source `{}` field path `{}` cannot be applied because the base ObjectTable does not contain that field path",
                        source.table_name,
                        source.path.display(),
                        field_path
                    ),
                    field_path,
                    "existing base ObjectTable field path",
                    "Move new ObjectTable keys to base sources, or add an explicit upsert policy before using override_sources to add keys.",
                ));
            }
            if let Some(cell_source) = cell_sources.get(&field_path).cloned() {
                self.cell_sources[0].insert(field_path.clone(), cell_source);
            }
            self.rows[0].insert(field_path, value);
        }
        Ok(())
    }
}

fn object_singleton_parts(
    table: DataTable,
    source: &DataSourcePlan,
) -> Result<(DataRow, usize, BTreeMap<String, CellSourceInfo>)> {
    let DataTable {
        rows,
        row_sources,
        cell_sources,
        ..
    } = table;
    let row_count = rows.len();
    let mut rows = rows.into_iter();
    let row = rows.next().ok_or_else(|| {
        object_source_schema_error(
            source,
            None,
            format!(
                "table `{}` source `{}` produced no ObjectTable singleton row",
                source.table_name,
                source.path.display()
            ),
            "0 rows",
            "one ObjectTable singleton row",
            "Check ObjectTable CSV reader output.",
        )
    })?;
    if row_count != 1 {
        return Err(object_source_schema_error(
            source,
            None,
            format!(
                "table `{}` source `{}` produced {} rows for ObjectTable key/value layout",
                source.table_name,
                source.path.display(),
                row_count
            ),
            format!("{row_count} rows"),
            "one ObjectTable singleton row",
            "Check ObjectTable source layout and reader output.",
        ));
    }
    let source_row_number = row_sources
        .first()
        .map(|source| source.source_row_number)
        .unwrap_or(source.data_start_row);
    let cell_sources = cell_sources.into_iter().next().unwrap_or_default();
    Ok((row, source_row_number, cell_sources))
}

fn retag_cell_sources(
    mut cell_sources: BTreeMap<String, CellSourceInfo>,
    source: &DataSourcePlan,
) -> BTreeMap<String, CellSourceInfo> {
    for cell_source in cell_sources.values_mut() {
        cell_source.source_path = source.path.clone();
    }
    cell_sources
}

fn primary_key_for_row(
    source: &DataSourcePlan,
    source_row_number: usize,
    row: &DataRow,
) -> Result<Option<Vec<String>>> {
    if source.primary_key_fields.is_empty() {
        return Ok(None);
    }
    let mut key = Vec::with_capacity(source.primary_key_fields.len());
    for field in &source.primary_key_fields {
        let value = row.get(field).ok_or_else(|| {
            source_schema_error(
                source,
                Some(source_row_number),
                format!(
                    "table `{}` source `{}` row {} is missing primary index field `{}`",
                    source.table_name,
                    source.path.display(),
                    source_row_number,
                    field
                ),
                "<missing>",
                format!("primary index field `{field}`"),
            )
        })?;
        if value.trim().is_empty() {
            return Err(source_schema_error(
                source,
                Some(source_row_number),
                format!(
                    "table `{}` source `{}` row {} has empty primary index field `{}`",
                    source.table_name,
                    source.path.display(),
                    source_row_number,
                    field
                ),
                "<empty>",
                format!("non-empty primary index field `{field}`"),
            ));
        }
        key.push(value.clone());
    }
    Ok(Some(key))
}

fn format_key(key: &[String]) -> String {
    key.join(",")
}

fn source_path_escape_error(
    source: &DataSourcePlan,
    root: &Path,
    message: String,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_SOURCE_PATH_ESCAPE,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                source_path: Some(source.path.clone()),
                table: Some(source.table_name.clone()),
                ..DiagnosticLocation::default()
            },
        )
        .with_source("data.loader")
        .with_domain("data")
        .with_current_value(source.path.display().to_string())
        .with_expected(expected)
        .with_hint(format!(
            "Use a source path under data root `{}`.",
            root.display()
        )),
    )
}

fn source_schema_error(
    source: &DataSourcePlan,
    source_row_number: Option<usize>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_SOURCE_SCHEMA_INVALID,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                source_path: Some(source.path.clone()),
                table: Some(source.table_name.clone()),
                source_row_number,
                ..DiagnosticLocation::default()
            },
        )
        .with_source("data.loader")
        .with_domain("data")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint("Check source primary key and override source configuration."),
    )
}

fn object_source_schema_error(
    source: &DataSourcePlan,
    source_row_number: Option<usize>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_SOURCE_SCHEMA_INVALID,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                source_path: Some(source.path.clone()),
                table: Some(source.table_name.clone()),
                source_row_number,
                ..DiagnosticLocation::default()
            },
        )
        .with_source("data.loader")
        .with_domain("data")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
}

fn source_duplicate_key_error(
    source: &DataSourcePlan,
    source_row_number: usize,
    message: String,
    current_value: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_DUPLICATE_KEY,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                source_path: Some(source.path.clone()),
                table: Some(source.table_name.clone()),
                source_row_number: Some(source_row_number),
                ..DiagnosticLocation::default()
            },
        )
        .with_source("data.loader")
        .with_domain("data")
        .with_current_value(current_value)
        .with_expected("unique base primary key before override_sources")
        .with_hint("Remove duplicate base rows or adjust the primary index."),
    )
}

fn object_duplicate_key_error(
    source: &DataSourcePlan,
    source_row_number: usize,
    field_path: &str,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_DUPLICATE_KEY,
            DiagnosticSeverity::Error,
            format!(
                "table `{}` source `{}` row {} duplicates ObjectTable field path `{}` from an earlier base source",
                source.table_name,
                source.path.display(),
                source_row_number,
                field_path
            ),
            DiagnosticLocation {
                source_path: Some(source.path.clone()),
                table: Some(source.table_name.clone()),
                source_row_number: Some(source_row_number),
                ..DiagnosticLocation::default()
            },
        )
        .with_source("data.loader")
        .with_domain("data")
        .with_current_value(field_path)
        .with_expected("unique ObjectTable field path across base sources")
        .with_hint("Move replacement values to override_sources instead of duplicating base ObjectTable keys."),
    )
}
