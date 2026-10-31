// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    data::{DataLoader, DataProvider, SourceReadPlan},
    meta::MetaModel,
};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/xlsx")
}

fn source(sheet: &str) -> DataSourcePlan {
    DataSourcePlan {
        table_name: "items".to_owned(),
        provider: DataProvider::Xlsx,
        path: "sources.xlsx".into(),
        sheet: Some(sheet.to_owned()),
        header_row: 1,
        data_start_row: 3,
        fragments: Vec::new(),
        layout: DataSourceLayout::CollectionRecords,
        fields: Vec::new(),
        role: SourceRole::Base,
        primary_key_fields: Vec::new(),
    }
}

fn diagnostic(error: ConfigForgeError) -> Diagnostic {
    let ConfigForgeError::ValidationReport(report) = error else {
        panic!("expected structured error")
    };
    report.diagnostics.into_iter().next().unwrap()
}

#[test]
fn xlsx_preserves_values_sparse_coordinates_and_cached_formulas() {
    let mut source = source("物品");
    source.header_row = 3;
    source.data_start_row = 5;
    let table = DataLoader::new(fixture_root())
        .load_source(&source)
        .unwrap();
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0]["name"], "0012");
    assert_eq!(table.rows[1]["name"], "药水");
    assert_eq!(table.rows[0]["enabled"], "true");
    assert_eq!(table.rows[1]["enabled"], "false");
    assert_eq!(table.rows[0]["amount"], "12.5");
    assert_eq!(table.rows[0]["computed"], "3");
    assert_eq!(table.rows[0]["empty"], "");
    assert_eq!(table.row_sources[1].source_row_number, 7);
    let cell = table.cell_source(0, "name").unwrap();
    assert_eq!(cell.source_sheet.as_deref(), Some("物品"));
    assert_eq!(cell.source_column_number, 4);
    assert_eq!(cell.source_row_number, 5);
}

#[test]
fn xlsx_sheet_selection_is_unambiguous() {
    let loader = DataLoader::new(fixture_root());
    let mut source = source("absent");
    assert!(loader.load_source(&source).is_err());
    source.sheet = None;
    assert!(loader.load_source(&source).is_err());
    source.path = "single.xlsx".into();
    let result = loader
        .read_sources(&SourceReadPlan::new(vec![source]))
        .unwrap()
        .into_data_set();
    assert_eq!(result.tables[0].rows[0]["id"], "1");
    assert_eq!(
        result.tables[0].row_sources[0].source_sheet.as_deref(),
        Some("only")
    );
}

#[test]
fn xlsx_parallel_sheets_match_serial_and_preserve_source_order() {
    let mut first = source("override");
    first.table_name = "first".to_owned();
    let mut second = source("settings");
    second.table_name = "second".to_owned();
    second.layout = DataSourceLayout::ObjectKeyValueRows;
    second.header_row = 2;
    second.data_start_row = 4;
    let plan = SourceReadPlan::new(vec![first, second]);
    let loader = DataLoader::new(fixture_root());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    let expected = loader.read_sources(&plan).unwrap();
    assert_eq!(
        expected,
        loader.read_sources_with_pool(&plan, &pool).unwrap()
    );
    assert_eq!(
        expected.as_data_set().tables[1]
            .cell_source(0, "title")
            .unwrap()
            .source_row_number,
        6
    );
}

#[test]
fn xlsx_reports_bad_cells_and_headers_at_physical_coordinates() {
    let loader = DataLoader::new(fixture_root());
    for sheet in ["missing_cache", "error", "date", "serial_date", "duration"] {
        let issue = diagnostic(loader.load_source(&source(sheet)).unwrap_err());
        assert_eq!(issue.code, CF_DATA_SOURCE_SCHEMA_INVALID);
        assert_eq!(issue.location.source_sheet.as_deref(), Some(sheet));
        assert_eq!(issue.location.source_row_number, Some(3));
        assert_eq!(issue.location.source_column_number, Some(1));
    }
    let mut duplicate = source("duplicate");
    duplicate.header_row = 2;
    let issue = diagnostic(loader.load_source(&duplicate).unwrap_err());
    assert_eq!(issue.code, CF_DATA_DUPLICATE_HEADER);
    assert_eq!(issue.location.source_row_number, Some(2));
    assert_eq!(issue.location.source_column_number, Some(4));
    assert!(loader.load_source(&source("empty")).is_err());
    let mut invalid_layout = source("override");
    invalid_layout.header_row = 0;
    assert!(loader.load_source(&invalid_layout).is_err());
    invalid_layout.header_row = 3;
    assert!(loader.load_source(&invalid_layout).is_err());
}

#[test]
fn xlsx_corrupt_missing_and_escaping_paths_fail() {
    let temp = tempfile::tempdir().unwrap();
    let loader = DataLoader::new(temp.path());
    let mut source = source("items");
    assert_eq!(
        diagnostic(loader.load_source(&source).unwrap_err()).code,
        CF_DATA_SOURCE_READ_FAILED
    );
    std::fs::write(temp.path().join("sources.xlsx"), "not a workbook").unwrap();
    assert_eq!(
        diagnostic(loader.load_source(&source).unwrap_err()).code,
        CF_DATA_SOURCE_READ_FAILED
    );
    source.path = "../sources.xlsx".into();
    assert_eq!(
        diagnostic(loader.load_source(&source).unwrap_err()).code,
        crate::diagnostic::code::CF_DATA_SOURCE_PATH_ESCAPE
    );
}

#[test]
fn xlsx_object_merge_override_and_parallel_reads_share_existing_semantics() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::copy(
        fixture_root().join("sources.xlsx"),
        temp.path().join("sources.xlsx"),
    )
    .unwrap();
    std::fs::write(temp.path().join("base.csv"), "key,value\nother,7\n").unwrap();
    std::fs::write(temp.path().join("override.csv"), "key,value\nlimit,99\n").unwrap();
    let mut xlsx = source("settings");
    xlsx.layout = DataSourceLayout::ObjectKeyValueRows;
    xlsx.header_row = 2;
    xlsx.data_start_row = 4;
    let mut csv = xlsx.clone();
    csv.provider = DataProvider::Csv;
    csv.path = "base.csv".into();
    csv.sheet = None;
    csv.header_row = 1;
    csv.data_start_row = 2;
    let mut patch = csv.clone();
    patch.role = SourceRole::Override;
    patch.path = "override.csv".into();
    let plan = SourceReadPlan::new(vec![csv, xlsx, patch]);
    let loader = DataLoader::new(temp.path());
    let serial = loader.read_sources(&plan).unwrap().into_data_set();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(3)
        .build()
        .unwrap();
    assert_eq!(
        serial,
        loader
            .read_sources_with_pool(&plan, &pool)
            .unwrap()
            .into_data_set()
    );
    assert_eq!(serial.tables[0].rows[0]["other"], "7");
    assert_eq!(serial.tables[0].rows[0]["limit"], "99");
    assert_eq!(
        serial.tables[0]
            .cell_source(0, "limit")
            .unwrap()
            .source_sheet,
        None
    );
    assert_eq!(
        serial.tables[0]
            .cell_source(0, "title")
            .unwrap()
            .source_sheet
            .as_deref(),
        Some("settings")
    );
    let mut bad = source("object_duplicate");
    bad.layout = DataSourceLayout::ObjectKeyValueRows;
    bad.data_start_row = 2;
    assert!(loader.load_source(&bad).is_err());
    bad.sheet = Some("override".to_owned());
    assert!(loader.load_source(&bad).is_err());
    bad.sheet = Some("empty_key".to_owned());
    assert!(loader.load_source(&bad).is_err());
    bad.sheet = Some("titled_object".to_owned());
    let table = loader.load_source(&bad).unwrap();
    assert_eq!(table.rows[0]["x"], "10");
    assert_eq!(table.cell_source(0, "x").unwrap().source_column_number, 6);
}

#[test]
fn xlsx_collection_override_keeps_replaced_sheet_trace_and_cache_is_scoped() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::copy(
        fixture_root().join("sources.xlsx"),
        temp.path().join("sources.xlsx"),
    )
    .unwrap();
    std::fs::write(temp.path().join("patch.csv"), "id,name\nID,Name\n1,csv\n").unwrap();
    let mut base = source("override");
    base.primary_key_fields = vec!["id".to_owned()];
    let mut patch = base.clone();
    patch.provider = DataProvider::Csv;
    patch.sheet = None;
    patch.path = "patch.csv".into();
    patch.role = SourceRole::Override;
    let plan = SourceReadPlan::new(vec![base.clone(), patch]);
    let loader = DataLoader::new(temp.path());
    let result = loader.read_sources(&plan).unwrap().into_data_set();
    assert_eq!(result.tables[0].rows[0]["name"], "csv");
    assert_eq!(
        result.tables[0].row_sources[0]
            .replaces
            .as_ref()
            .unwrap()
            .source_sheet
            .as_deref(),
        Some("override")
    );
    let mut xlsx_patch = base.clone();
    xlsx_patch.role = SourceRole::Override;
    let plan = SourceReadPlan::new(vec![plan.sources()[1].clone(), xlsx_patch]);
    let mut sources = plan.sources().to_vec();
    sources[0].role = SourceRole::Base;
    let result = loader
        .read_sources(&SourceReadPlan::new(sources))
        .unwrap()
        .into_data_set();
    assert_eq!(result.tables[0].rows[0]["name"], "updated");
    assert_eq!(
        result.tables[0].row_sources[0].source_sheet.as_deref(),
        Some("override")
    );
    loader
        .read_sources(&SourceReadPlan::new(vec![base.clone()]))
        .unwrap();
    std::fs::write(
        temp.path().join("sources.xlsx"),
        "changed after previous build",
    )
    .unwrap();
    assert!(
        loader
            .read_sources(&SourceReadPlan::new(vec![base]))
            .is_err()
    );
}

#[test]
fn xlsx_meta_accepts_sheet_and_rejects_invalid_declarations() {
    for declaration in [
        r#"provider = "xlsx", sheet = "items""#,
        r#"provider = "xlsx""#,
    ] {
        let text = format!(
            r#"[[tables]]
name = "items"
source = {{ path = "data.xlsx", {declaration} }}
[[tables.fields]]
name = "id"
type = "int"
"#
        );
        let model = MetaModel::from_toml_str(&text).unwrap();
        let plans = SourceReadPlan::from_meta_model(&model).unwrap();
        assert_eq!(plans.sources()[0].provider, DataProvider::Xlsx);
        assert_eq!(
            plans.sources()[0].sheet,
            model.tables[0].source.as_ref().unwrap().sheet
        );
    }
    for declaration in [
        r#"provider = "xlsx", sheet = " ""#,
        r#"provider = "csv", sheet = "items""#,
    ] {
        let text = format!(
            r#"[[tables]]
name = "items"
source = {{ path = "data.xlsx", {declaration} }}
[[tables.fields]]
name = "id"
type = "int"
"#
        );
        assert!(MetaModel::from_toml_str(&text).is_err());
    }
}
