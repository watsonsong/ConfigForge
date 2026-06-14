// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use tempfile::tempdir;

use crate::{
    ConfigForgeError, Diagnostic,
    diagnostic::code::{
        CF_BUILD_INVALID_OPTION, CF_DATA_DUPLICATE_HEADER, CF_DATA_DUPLICATE_KEY,
        CF_DATA_INVALID_TYPE, CF_DATA_SOURCE_PATH_ESCAPE, CF_DATA_SOURCE_READ_FAILED,
        CF_DATA_SOURCE_SCHEMA_INVALID,
    },
    meta::MetaModel,
};

use super::*;

fn fragment_row_source(source_row_number: usize, fragments: &[&str]) -> RowSourceInfo {
    RowSourceInfo {
        source_index: 0,
        source_path: PathBuf::from("weapons.csv"),
        source_row_number,
        fragments: fragments
            .iter()
            .map(|fragment| (*fragment).to_owned())
            .collect(),
        source_role: SourceRole::Base,
        replaces: None,
    }
}

fn plan_source(table_name: &str, path: &str) -> DataSourcePlan {
    DataSourcePlan {
        table_name: table_name.to_owned(),
        provider: DataProvider::Csv,
        path: PathBuf::from(path),
        header_row: 1,
        data_start_row: 3,
        fragments: Vec::new(),
        layout: DataSourceLayout::CollectionRecords,
        fields: Vec::new(),
        role: SourceRole::Base,
        primary_key_fields: Vec::new(),
    }
}

fn object_plan_source(table_name: &str, path: &str) -> DataSourcePlan {
    DataSourcePlan {
        layout: DataSourceLayout::ObjectKeyValueRows,
        data_start_row: 2,
        fields: vec!["max_level".to_owned(), "gold_multiplier".to_owned()],
        ..plan_source(table_name, path)
    }
}

fn expect_single_diagnostic(error: ConfigForgeError) -> Diagnostic {
    let ConfigForgeError::ValidationReport(report) = error else {
        panic!("expected structured diagnostic report");
    };
    assert_eq!(report.diagnostics.len(), 1);
    report.diagnostics.into_iter().next().unwrap()
}

#[test]
fn data_provider_parses_supported_values() {
    assert_eq!(DataProvider::parse("csv"), Some(DataProvider::Csv));
    assert_eq!(DataProvider::parse("xlsx"), None);
    assert_eq!(DataProvider::Csv.as_str(), "csv");
}

#[test]
fn csv_reader_reads_header_and_records() {
    let dir = tempdir().expect("temp dir should exist");
    let csv_path = dir.path().join("items.csv");
    fs::write(&csv_path, "id,name\nID,Name\n1,Sword\n2,Shield\n").expect("csv should be written");

    let table = CsvDataReader::read_table("items", &csv_path).expect("csv should load");

    assert_eq!(table.name, "items");
    assert_eq!(table.row_count(), 2);
    assert!(!table.is_empty());
    assert_eq!(table.rows[0].get("id").map(String::as_str), Some("1"));
    assert_eq!(
        table.rows[1].get("name").map(String::as_str),
        Some("Shield")
    );
    let name_source = table
        .cell_source(1, "name")
        .expect("name cell source should be tracked");
    assert_eq!(name_source.source_row_number, 4);
    assert_eq!(name_source.source_column_number, 2);
    assert_eq!(name_source.column_index, 1);
    assert_eq!(name_source.source_column_name, "name");
}

#[test]
fn csv_reader_supports_custom_1_based_source_row_layout() {
    let dir = tempdir().expect("temp dir should exist");
    let csv_path = dir.path().join("items.csv");
    fs::write(&csv_path, "sheet,title\nid,name\nID,Name\n1,Sword\n")
        .expect("csv should be written");

    let table =
        CsvDataReader::read_table_with_layout("items", &csv_path, 2, 4).expect("csv should load");

    assert_eq!(table.row_count(), 1);
    assert_eq!(table.rows[0].get("id").map(String::as_str), Some("1"));
    assert_eq!(table.row_sources[0].source_row_number, 4);
}

#[test]
fn csv_reader_reports_parse_errors() {
    let error = CsvDataReader::read_table("items", &PathBuf::from("missing.csv")).unwrap_err();

    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_READ_FAILED);
    assert_eq!(diagnostic.source.as_deref(), Some("data.csv"));
    assert_eq!(diagnostic.domain.as_deref(), Some("data"));
}

#[test]
fn csv_reader_rejects_duplicate_headers_but_allows_empty_header() {
    let dir = tempdir().expect("temp dir should exist");
    let duplicate_path = dir.path().join("duplicate.csv");
    fs::write(&duplicate_path, "id,id\nID,ID\n1,2\n").expect("csv should be written");

    let error = CsvDataReader::read_table("items", &duplicate_path).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_DUPLICATE_HEADER);
    assert!(diagnostic.message.contains("duplicate header `id`"));
    assert_eq!(diagnostic.source.as_deref(), Some("data.csv"));
    assert_eq!(diagnostic.domain.as_deref(), Some("data"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("id"));
    assert_eq!(
        diagnostic.expected.as_deref(),
        Some("unique CSV header name")
    );

    let empty_header_path = dir.path().join("empty_header.csv");
    fs::write(&empty_header_path, ",name\nRaw,Name\nraw,Sword\n").expect("csv should be written");
    let table = CsvDataReader::read_table("items", &empty_header_path).expect("csv should load");

    assert_eq!(table.rows[0].get("").map(String::as_str), Some("raw"));
    assert_eq!(table.rows[0].get("name").map(String::as_str), Some("Sword"));
}

#[test]
fn csv_reader_handles_common_csv_boundaries() {
    let dir = tempdir().expect("temp dir should exist");

    let only_header_path = dir.path().join("only_header.csv");
    fs::write(&only_header_path, "id,name\nID,Name\n").expect("csv should be written");
    let table = CsvDataReader::read_table("items", &only_header_path).expect("csv should load");
    assert!(table.is_empty());

    let quoted_path = dir.path().join("quoted.csv");
    fs::write(
        &quoted_path,
        "id,desc\nID,Description\n1,\"hello, world\"\n2,\"line1\nline2\"\n",
    )
    .expect("csv should be written");
    let table = CsvDataReader::read_table("items", &quoted_path).expect("csv should load");
    assert_eq!(
        table.rows[0].get("desc").map(String::as_str),
        Some("hello, world")
    );
    assert_eq!(
        table.rows[1].get("desc").map(String::as_str),
        Some("line1\nline2")
    );

    let windows_bom_path = dir.path().join("windows_bom.csv");
    fs::write(
        &windows_bom_path,
        "\u{feff}id,name\r\nID,Name\r\n1,Sword\r\n",
    )
    .expect("csv should be written");
    let table = CsvDataReader::read_table("items", &windows_bom_path).expect("csv should load");
    assert_eq!(table.rows[0].get("id").map(String::as_str), Some("1"));
    assert_eq!(table.rows[0].get("name").map(String::as_str), Some("Sword"));
}

#[test]
fn csv_reader_reports_empty_file_and_ragged_rows() {
    let dir = tempdir().expect("temp dir should exist");

    let empty_path = dir.path().join("empty.csv");
    fs::write(&empty_path, "").expect("csv should be written");
    let diagnostic =
        expect_single_diagnostic(CsvDataReader::read_table("items", &empty_path).unwrap_err());
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_SCHEMA_INVALID);
    assert!(diagnostic.message.contains("no header row"));
    assert_eq!(diagnostic.source.as_deref(), Some("data.csv"));

    let missing_path = dir.path().join("missing.csv");
    fs::write(&missing_path, "id,name\nID,Name\n1\n").expect("csv should be written");
    let diagnostic =
        expect_single_diagnostic(CsvDataReader::read_table("items", &missing_path).unwrap_err());
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_READ_FAILED);

    let extra_path = dir.path().join("extra.csv");
    fs::write(&extra_path, "id,name\nID,Name\n1,Sword,extra\n").expect("csv should be written");
    let diagnostic =
        expect_single_diagnostic(CsvDataReader::read_table("items", &extra_path).unwrap_err());
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_READ_FAILED);
}

#[test]
fn data_load_plan_reads_legacy_and_canonical_sources() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "legacy_items"
            source = "legacy_items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "items"
            sources = [{ provider = "csv", path = "items.csv" }]

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("meta should load");

    let plan = DataLoadPlan::from_meta_model(&meta).expect("plan should build");

    assert_eq!(plan.sources().len(), 2);
    assert_eq!(plan.sources()[0].table_name, "legacy_items");
    assert_eq!(plan.sources()[0].provider, DataProvider::Csv);
    assert_eq!(plan.sources()[0].path, PathBuf::from("legacy_items.csv"));
    assert_eq!(plan.sources()[0].header_row, 1);
    assert_eq!(plan.sources()[0].data_start_row, 3);
    assert_eq!(
        plan.sources()[0].layout,
        DataSourceLayout::CollectionRecords
    );
    assert_eq!(plan.sources()[1].table_name, "items");
    assert_eq!(plan.sources()[1].path, PathBuf::from("items.csv"));
    assert_eq!(plan.sources()[1].header_row, 1);
    assert_eq!(plan.sources()[1].data_start_row, 3);
    assert_eq!(
        plan.sources()[1].layout,
        DataSourceLayout::CollectionRecords
    );
}

#[test]
fn data_load_plan_reads_source_level_row_layout() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = { path = "items.csv", header_row = 2, data_start_row = 4 }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "weapons"
            sources = [{ provider = "csv", path = "weapons.csv", header_row = 3, data_start_row = 5 }]

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("meta should load");

    let plan = DataLoadPlan::from_meta_model(&meta).expect("plan should build");

    assert_eq!(plan.sources()[0].path, PathBuf::from("items.csv"));
    assert_eq!(plan.sources()[0].header_row, 2);
    assert_eq!(plan.sources()[0].data_start_row, 4);
    assert_eq!(plan.sources()[1].path, PathBuf::from("weapons.csv"));
    assert_eq!(plan.sources()[1].header_row, 3);
    assert_eq!(plan.sources()[1].data_start_row, 5);
}

#[test]
fn data_load_plan_marks_object_tables_as_key_value_rows() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"

            [[tables.fields]]
            name = "start_gold"
            type = "int"
        "#,
    )
    .expect("meta should load");

    let plan = DataLoadPlan::from_meta_model(&meta).expect("plan should build");

    assert_eq!(plan.sources().len(), 1);
    assert_eq!(plan.sources()[0].table_name, "game_config");
    assert_eq!(
        plan.sources()[0].layout,
        DataSourceLayout::ObjectKeyValueRows
    );
    assert_eq!(
        plan.sources()[0].fields,
        vec!["max_level".to_owned(), "start_gold".to_owned()]
    );
    assert_eq!(plan.sources()[0].header_row, 1);
    assert_eq!(plan.sources()[0].data_start_row, 2);
}

#[test]
fn csv_reader_reads_object_key_value_rows_as_one_logical_row() {
    let dir = tempdir().expect("temp dir should exist");
    let csv_path = dir.path().join("game_config.csv");
    fs::write(
        &csv_path,
        "key,title,value\nmax_level,最大等级,100\nstart_gold,初始金币,500\n",
    )
    .expect("csv should be written");

    let table = CsvDataReader::read_object_table(
        "game_config",
        &csv_path,
        &["max_level".to_owned(), "start_gold".to_owned()],
    )
    .expect("object csv should load");

    assert_eq!(table.name, "game_config");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(
        table.rows[0].get("max_level").map(String::as_str),
        Some("100")
    );
    assert_eq!(
        table.rows[0].get("start_gold").map(String::as_str),
        Some("500")
    );
    assert_eq!(table.row_sources[0].source_row_number, 2);
    let start_gold_source = table
        .cell_source(0, "start_gold")
        .expect("object table field source should be tracked");
    assert_eq!(start_gold_source.source_row_number, 3);
    assert_eq!(start_gold_source.source_column_number, 3);
    assert_eq!(start_gold_source.column_index, 2);
    assert_eq!(start_gold_source.source_column_name, "value");

    let only_header_path = dir.path().join("empty_config.csv");
    fs::write(&only_header_path, "key,title,value\n").expect("csv should be written");
    let table = CsvDataReader::read_object_table("game_config", &only_header_path, &[])
        .expect("only-header object csv should load");
    assert_eq!(table.rows.len(), 1);
    assert!(table.rows[0].is_empty());

    let whitespace_header_path = dir.path().join("whitespace_header.csv");
    fs::write(
        &whitespace_header_path,
        " key , title , value \nmax_level,最大等级,100\n",
    )
    .expect("csv should be written");
    let table = CsvDataReader::read_object_table(
        "game_config",
        &whitespace_header_path,
        &["max_level".to_owned()],
    )
    .expect("whitespace around object csv header names should be accepted");
    assert_eq!(
        table.rows[0].get("max_level").map(String::as_str),
        Some("100")
    );
}

#[test]
fn csv_reader_rejects_invalid_object_key_value_rows() {
    let dir = tempdir().expect("temp dir should exist");
    let cases = [
        (
            "bad_header.csv",
            "field,title,value\nmax_level,最大等级,100\n",
            "object table header must contain",
        ),
        (
            "missing_value_header.csv",
            "key\nmax_level\n",
            "object table header must contain",
        ),
        (
            "extra_header.csv",
            "key,title,value,extra\nmax_level,最大等级,100,ignored\n",
            "object table header must contain",
        ),
        ("empty_key.csv", "key,title,value\n,空键,100\n", "empty key"),
        (
            "duplicate_key.csv",
            "key,title,value\nmax_level,最大等级,100\nmax_level,最大等级覆盖,101\n",
            "duplicates key",
        ),
    ];

    for (file_name, content, expected_message) in cases {
        let path = dir.path().join(file_name);
        fs::write(&path, content).expect("csv should be written");
        let error =
            CsvDataReader::read_object_table("game_config", &path, &["max_level".to_owned()])
                .unwrap_err();
        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, CF_DATA_SOURCE_SCHEMA_INVALID);
        assert!(
            diagnostic.message.contains(expected_message),
            "expected message containing `{expected_message}`"
        );
    }
}

#[test]
fn csv_reader_reports_object_table_ragged_rows() {
    let dir = tempdir().expect("temp dir should exist");
    let missing_path = dir.path().join("missing_value.csv");
    fs::write(&missing_path, "key,title,value\nmax_level\n").expect("csv should be written");
    let diagnostic = expect_single_diagnostic(
        CsvDataReader::read_object_table("game_config", &missing_path, &["max_level".to_owned()])
            .unwrap_err(),
    );
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_READ_FAILED);

    let extra_path = dir.path().join("extra_value.csv");
    fs::write(
        &extra_path,
        "key,title,value\nmax_level,最大等级,100,extra\n",
    )
    .expect("csv should be written");
    let diagnostic = expect_single_diagnostic(
        CsvDataReader::read_object_table("game_config", &extra_path, &["max_level".to_owned()])
            .unwrap_err(),
    );
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_READ_FAILED);
}

#[test]
fn data_resolver_resolves_expanded_structs_with_typed_scalars_and_defaults() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Attributes"

            [[structs.fields]]
            name = "attack"
            type = "int"
            default = 1

            [[structs.fields]]
            name = "max_health"
            type = "int"

            [[tables]]
            name = "characters"
            source = "characters.csv"

            [[tables.fields]]
            name = "id"
            type = "string"

            [[tables.fields]]
            name = "attributes"
            type = "Attributes"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table = DataTable::new(
        "characters",
        vec![
            DataRow::from([
                ("id".to_owned(), "hero".to_owned()),
                ("attributes.attack".to_owned(), "10".to_owned()),
                ("attributes.max_health".to_owned(), "120".to_owned()),
            ]),
            DataRow::from([
                ("id".to_owned(), "mage".to_owned()),
                ("attributes.attack".to_owned(), "".to_owned()),
                ("attributes.max_health".to_owned(), "80".to_owned()),
            ]),
        ],
    );
    let data_set = DataSet::new(vec![table]);

    let resolved = DataResolver::new(&linked)
        .resolve_required_tables(&data_set)
        .expect("data should resolve");
    let table = resolved.table("characters").expect("table should exist");

    let DataValue::Struct(attributes) = &table.rows[0]["attributes"].value else {
        panic!("attributes should resolve as struct");
    };
    assert_eq!(
        attributes["attack"].value,
        DataValue::Scalar(ScalarValue::Int(10))
    );
    assert_eq!(
        attributes["max_health"].value,
        DataValue::Scalar(ScalarValue::Int(120))
    );
    assert_eq!(
        attributes["attack"].source.kind,
        ValueSourceKind::ExplicitCell
    );
    assert_eq!(
        attributes["attack"].source.raw_field_path,
        "attributes.attack"
    );
    assert_eq!(attributes["attack"].source.raw_value.as_deref(), Some("10"));

    let DataValue::Struct(attributes_with_default) = &table.rows[1]["attributes"].value else {
        panic!("attributes should resolve as struct");
    };
    assert_eq!(
        attributes_with_default["attack"].value,
        DataValue::Scalar(ScalarValue::Int(1))
    );
    assert_eq!(
        attributes_with_default["attack"].source.kind,
        ValueSourceKind::DefaultValue
    );
    assert_eq!(
        attributes_with_default["attack"].source.raw_field_path,
        "attributes.attack"
    );
    assert_eq!(
        attributes_with_default["attack"]
            .source
            .raw_value
            .as_deref(),
        Some("1")
    );
}

#[test]
fn data_resolver_resolves_color_and_vector_builtin_values() {
    let meta = MetaModel::from_toml_str(
        r##"
            [[tables]]
            name = "effects"
            source = "effects.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "tint"
            type = "color"
            color_format = "hex_rgb"
            color_channel = "byte"
            color_space = "srgb"
            alpha = "default_opaque"

            [[tables.fields]]
            name = "spawn_position"
            type = "vector3"
            vector_format = "expanded"
            vector_components = ["x", "y", "z"]
            unit = "cm"

            [[tables.fields]]
            name = "grid"
            type = "vector2_int"
            vector_format = "array"
            default = "[4,5]"
        "##,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table = DataTable::new(
        "effects",
        vec![DataRow::from([
            ("id".to_owned(), "1".to_owned()),
            ("tint".to_owned(), "#336699".to_owned()),
            ("spawn_position.x".to_owned(), "1.5".to_owned()),
            ("spawn_position.y".to_owned(), "2.5".to_owned()),
            ("spawn_position.z".to_owned(), "3.5".to_owned()),
        ])],
    );
    let data_set = DataSet::new(vec![table]);

    let resolved = DataResolver::new(&linked)
        .resolve_required_tables(&data_set)
        .expect("data should resolve");
    let row = &resolved.table("effects").expect("table should exist").rows[0];

    let DataValue::Color(tint) = &row["tint"].value else {
        panic!("tint should resolve as color");
    };
    assert_eq!(tint.r, 0x33 as f64);
    assert_eq!(tint.g, 0x66 as f64);
    assert_eq!(tint.b, 0x99 as f64);
    assert_eq!(tint.a, 255.0);
    assert_eq!(tint.channel, crate::meta::ColorChannel::Byte);
    assert_eq!(tint.color_space, crate::meta::ColorSpace::Srgb);
    assert_eq!(row["tint"].source.raw_value.as_deref(), Some("#336699"));

    assert_eq!(
        row["spawn_position"].value,
        DataValue::Vector(VectorValue::Float(vec![1.5, 2.5, 3.5]))
    );
    assert_eq!(
        row["spawn_position"].source.raw_value.as_deref(),
        Some("spawn_position.x,spawn_position.y,spawn_position.z")
    );
    assert_eq!(
        row["grid"].value,
        DataValue::Vector(VectorValue::Int(vec![4, 5]))
    );
    assert_eq!(row["grid"].source.kind, ValueSourceKind::DefaultValue);
}

#[test]
fn data_resolver_rejects_invalid_builtin_value_layouts() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "effects"
            source = "effects.csv"

            [[tables.fields]]
            name = "spawn_position"
            type = "vector3"
            vector_format = "expanded"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let incomplete = DataSet::new(vec![DataTable::new(
        "effects",
        vec![DataRow::from([
            ("spawn_position.x".to_owned(), "1".to_owned()),
            ("spawn_position.z".to_owned(), "3".to_owned()),
        ])],
    )]);
    let error = DataResolver::new(&linked)
        .resolve_required_tables(&incomplete)
        .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_INVALID_TYPE);
    assert!(diagnostic.message.contains("incomplete expanded vector"));

    let mixed = DataSet::new(vec![DataTable::new(
        "effects",
        vec![DataRow::from([
            ("spawn_position".to_owned(), "1,2,3".to_owned()),
            ("spawn_position.x".to_owned(), "1".to_owned()),
        ])],
    )]);
    let error = DataResolver::new(&linked)
        .resolve_required_tables(&mixed)
        .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_SCHEMA_INVALID);
    assert!(diagnostic.message.contains("mixes vector cell"));
}

#[test]
fn data_resolver_resolves_object_table_json_and_expanded_structs() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Attributes"

            [[structs.fields]]
            name = "attack"
            type = "int"

            [[structs.fields]]
            name = "max_health"
            type = "int"

            [[tables]]
            name = "game_rules"
            kind = "object"
            source = "game_rules.csv"

            [[tables.fields]]
            name = "default_attributes"
            type = "Attributes"

            [[tables.fields]]
            name = "boss_attributes"
            type = "Attributes"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table = DataTable::new(
        "game_rules",
        vec![DataRow::from([
            ("default_attributes.attack".to_owned(), "5".to_owned()),
            ("default_attributes.max_health".to_owned(), "80".to_owned()),
            (
                "boss_attributes".to_owned(),
                r#"{"attack":20,"max_health":500}"#.to_owned(),
            ),
        ])],
    );
    let data_set = DataSet::new(vec![table]);

    let resolved = DataResolver::new(&linked)
        .resolve_required_tables(&data_set)
        .expect("data should resolve");
    let row = &resolved
        .table("game_rules")
        .expect("table should exist")
        .rows[0];

    let DataValue::Struct(default_attributes) = &row["default_attributes"].value else {
        panic!("default attributes should resolve as struct");
    };
    assert_eq!(
        default_attributes["attack"].value,
        DataValue::Scalar(ScalarValue::Int(5))
    );

    let DataValue::Struct(boss_attributes) = &row["boss_attributes"].value else {
        panic!("boss attributes should resolve as struct");
    };
    assert_eq!(
        boss_attributes["max_health"].value,
        DataValue::Scalar(ScalarValue::Int(500))
    );
    assert_eq!(
        row["boss_attributes"].source.kind,
        ValueSourceKind::ExplicitCell
    );
}

#[test]
fn data_resolver_rejects_schema_errors_and_target_filters_hidden_children() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Attributes"

            [[structs.fields]]
            name = "attack"
            type = "int"

            [[structs.fields]]
            name = "server_only"
            type = "int"
            [structs.fields.export_when]
            any = ["server"]

            [[tables]]
            name = "characters"
            source = "characters.csv"

            [[tables.fields]]
            name = "attributes"
            type = "Attributes"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let mixed = DataSet::new(vec![DataTable::new(
        "characters",
        vec![DataRow::from([
            ("attributes".to_owned(), r#"{"attack":1}"#.to_owned()),
            ("attributes.attack".to_owned(), "2".to_owned()),
        ])],
    )]);
    let error = DataResolver::new(&linked)
        .resolve_required_tables(&mixed)
        .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_SCHEMA_INVALID);
    assert_eq!(diagnostic.source.as_deref(), Some("data.resolver"));
    assert!(diagnostic.message.contains("mixes struct JSON cell"));
    assert_eq!(diagnostic.location.table.as_deref(), Some("characters"));
    assert_eq!(diagnostic.location.field.as_deref(), Some("attributes"));

    let hidden_only = DataSet::new(vec![DataTable::new(
        "characters",
        vec![DataRow::from([(
            "attributes.server_only".to_owned(),
            "7".to_owned(),
        )])],
    )]);
    let resolved = DataResolver::for_target(&linked, &crate::meta::TargetTags::new(["client"]))
        .resolve_required_tables(&hidden_only)
        .expect("hidden target child should not create visible occurrence");
    assert!(resolved.table("characters").unwrap().rows[0].is_empty());
}

#[test]
fn data_resolver_full_mode_includes_active_fragments() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            polymorphic = true
            sources = [
                { provider = "csv", path = "weapons.csv", fragments = ["weapon"] },
            ]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "weapon"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table = DataTable::new_with_sources(
        "items",
        vec![DataRow::from([
            ("id".to_owned(), "1".to_owned()),
            ("weapon.power".to_owned(), "10".to_owned()),
        ])],
        vec![RowSourceInfo {
            source_index: 0,
            source_path: PathBuf::from("weapons.csv"),
            source_row_number: 1,
            fragments: vec!["weapon".to_owned()],
            source_role: SourceRole::Base,
            replaces: None,
        }],
    )
    .expect("table should build");
    let data_set = DataSet::new(vec![table]);

    let resolved = DataResolver::new(&linked)
        .resolve_required_tables(&data_set)
        .expect("data should resolve");
    let row = &resolved.table("items").expect("table should exist").rows[0];
    let DataValue::Struct(weapon) = &row["weapon"].value else {
        panic!("active fragment should resolve as struct");
    };
    assert_eq!(
        weapon["power"].value,
        DataValue::Scalar(ScalarValue::Int(10))
    );
}

#[test]
fn data_resolver_resolves_typed_fragment_payload_as_struct_occurrence() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "AttributeSet"

            [[structs.fields]]
            name = "attack"
            type = "int"

            [[structs.fields]]
            name = "defence"
            type = "int"
            default = 0

            [[tables]]
            name = "items"
            polymorphic = true
            sources = [{ provider = "csv", path = "weapons.csv", fragments = ["attributes"] }]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "attributes"
            type = "AttributeSet"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table = DataTable::new_with_sources(
        "items",
        vec![DataRow::from([
            ("id".to_owned(), "1".to_owned()),
            ("attributes.attack".to_owned(), "10".to_owned()),
        ])],
        vec![RowSourceInfo {
            source_index: 0,
            source_path: PathBuf::from("weapons.csv"),
            source_row_number: 1,
            fragments: vec!["attributes".to_owned()],
            source_role: SourceRole::Base,
            replaces: None,
        }],
    )
    .expect("table should build");
    let data_set = DataSet::new(vec![table]);

    let resolved = DataResolver::new(&linked)
        .resolve_required_tables(&data_set)
        .expect("data should resolve");
    let row = &resolved.table("items").expect("table should exist").rows[0];
    let DataValue::Struct(attributes) = &row["attributes"].value else {
        panic!("typed fragment payload should resolve as struct");
    };
    assert_eq!(
        attributes["attack"].value,
        DataValue::Scalar(ScalarValue::Int(10))
    );
    assert_eq!(
        attributes["defence"].value,
        DataValue::Scalar(ScalarValue::Int(0))
    );
}

#[test]
fn data_resolver_preserves_empty_active_typed_fragment_presence() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "AttributeSet"

            [[structs.fields]]
            name = "attack"
            type = "int"

            [[tables]]
            name = "items"
            polymorphic = true
            sources = [{ provider = "csv", path = "weapons.csv", fragments = ["attributes"] }]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "attributes"
            type = "AttributeSet"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table = DataTable::new_with_sources(
        "items",
        vec![DataRow::from([("id".to_owned(), "1".to_owned())])],
        vec![RowSourceInfo {
            source_index: 0,
            source_path: PathBuf::from("weapons.csv"),
            source_row_number: 1,
            fragments: vec!["attributes".to_owned()],
            source_role: SourceRole::Base,
            replaces: None,
        }],
    )
    .expect("table should build");
    let data_set = DataSet::new(vec![table]);

    let resolved = DataResolver::new(&linked)
        .resolve_required_tables(&data_set)
        .expect("empty typed fragment should preserve active presence");
    let row = &resolved.table("items").expect("table should exist").rows[0];
    let DataValue::Struct(attributes) = &row["attributes"].value else {
        panic!("empty typed fragment payload should resolve as struct");
    };
    assert!(attributes.is_empty());
}

#[test]
fn data_resolver_rejects_typed_fragment_layout_mixing_and_invalid_nested_paths() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Offense"

            [[structs.fields]]
            name = "attack"
            type = "int"

            [[structs]]
            name = "AttributeSet"

            [[structs.fields]]
            name = "offense"
            type = "Offense"

            [[structs.fields]]
            name = "scalar"
            type = "int"

            [[tables]]
            name = "items"
            polymorphic = true
            sources = [{ provider = "csv", path = "weapons.csv", fragments = ["attributes"] }]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "attributes"
            type = "AttributeSet"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let valid_nested = DataTable::new_with_sources(
        "items",
        vec![DataRow::from([
            ("id".to_owned(), "1".to_owned()),
            ("attributes.offense.attack".to_owned(), "10".to_owned()),
        ])],
        vec![fragment_row_source(1, &["attributes"])],
    )
    .expect("table should build");
    DataResolver::new(&linked)
        .resolve_required_tables(&DataSet::new(vec![valid_nested]))
        .expect("nested typed fragment path should resolve");

    let mixed = DataTable::new_with_sources(
        "items",
        vec![DataRow::from([
            ("id".to_owned(), "1".to_owned()),
            (
                "attributes".to_owned(),
                r#"{"offense":{"attack":1}}"#.to_owned(),
            ),
            ("attributes.offense.attack".to_owned(), "10".to_owned()),
        ])],
        vec![fragment_row_source(1, &["attributes"])],
    )
    .expect("table should build");
    let error = DataResolver::new(&linked)
        .resolve_required_tables(&DataSet::new(vec![mixed]))
        .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_SCHEMA_INVALID);
    assert_eq!(diagnostic.source.as_deref(), Some("data.resolver"));
    assert!(diagnostic.message.contains("mixes struct JSON cell"));
    assert_eq!(diagnostic.location.table.as_deref(), Some("items"));
    assert_eq!(diagnostic.location.field.as_deref(), Some("attributes"));

    let invalid_nested = DataTable::new_with_sources(
        "items",
        vec![DataRow::from([
            ("id".to_owned(), "1".to_owned()),
            ("attributes.scalar.attack".to_owned(), "10".to_owned()),
        ])],
        vec![fragment_row_source(1, &["attributes"])],
    )
    .expect("table should build");
    let error = DataResolver::new(&linked)
        .resolve_required_tables(&DataSet::new(vec![invalid_nested]))
        .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_SCHEMA_INVALID);
    assert!(diagnostic.message.contains("uses child path"));
    assert!(diagnostic.message.contains("scalar"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("scalar"));
}

#[test]
fn data_load_plan_reads_polymorphic_source_fragments() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            polymorphic = true
            sources = [
                { provider = "csv", path = "weapons.csv", fragments = ["weapon"] },
                { provider = "csv", path = "armors.csv", fragments = ["armor"] },
            ]

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "weapon"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"

            [[tables.fragments]]
            name = "armor"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
        "#,
    )
    .expect("meta should load");

    let plan = DataLoadPlan::from_meta_model(&meta).expect("plan should build");

    assert_eq!(plan.sources().len(), 2);
    assert_eq!(plan.sources()[0].table_name, "items");
    assert_eq!(plan.sources()[0].path, PathBuf::from("weapons.csv"));
    assert_eq!(plan.sources()[0].fragments, vec!["weapon".to_owned()]);
    assert_eq!(plan.sources()[1].path, PathBuf::from("armors.csv"));
    assert_eq!(plan.sources()[1].fragments, vec!["armor".to_owned()]);
}

#[test]
fn data_load_plan_filters_override_sources_by_target_tags() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "materials.csv"
            override_sources = [
                { provider = "csv", path = "materials_s1.csv", export_when = { any = ["s1"] } },
                { provider = "csv", path = "materials_base_patch.csv", export_when = { none = ["s1"] } },
            ]
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#,
    )
    .expect("meta should load");

    let s1_tags = crate::meta::TargetTags::new(["s1"]);
    let s1_plan =
        DataLoadPlan::from_tables(&meta.tables, Some(&s1_tags)).expect("s1 plan should build");
    assert_eq!(s1_plan.sources().len(), 2);
    assert_eq!(s1_plan.sources()[0].path, PathBuf::from("materials.csv"));
    assert_eq!(s1_plan.sources()[0].role, SourceRole::Base);
    assert_eq!(s1_plan.sources()[1].path, PathBuf::from("materials_s1.csv"));
    assert_eq!(s1_plan.sources()[1].role, SourceRole::Override);
    assert_eq!(s1_plan.sources()[1].primary_key_fields, vec!["id"]);

    let base_tags = crate::meta::TargetTags::empty();
    let base_plan =
        DataLoadPlan::from_tables(&meta.tables, Some(&base_tags)).expect("base plan should build");
    assert_eq!(base_plan.sources().len(), 2);
    assert_eq!(
        base_plan.sources()[1].path,
        PathBuf::from("materials_base_patch.csv")
    );
}

#[test]
fn source_read_plan_filters_override_sources_without_linked_meta() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "materials.csv"
            override_sources = [
                { provider = "csv", path = "materials_s1.csv", export_when = { any = ["s1"] } },
                { provider = "csv", path = "materials_dev.csv", export_when = { any = ["dev"] } },
            ]
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should still link separately");
    let s1_tags = crate::meta::TargetTags::new(["s1"]);

    let plan = SourceReadPlan::from_tables(&meta.tables, Some(&s1_tags))
        .expect("source discovery should build from raw meta");

    assert_eq!(linked.tables.len(), 1);
    assert_eq!(plan.sources().len(), 2);
    assert_eq!(plan.sources()[0].path, PathBuf::from("materials.csv"));
    assert_eq!(plan.sources()[0].role, SourceRole::Base);
    assert_eq!(plan.sources()[1].path, PathBuf::from("materials_s1.csv"));
    assert_eq!(plan.sources()[1].role, SourceRole::Override);
    assert_eq!(plan.sources()[1].primary_key_fields, vec!["id"]);
}

#[test]
fn data_load_plan_builds_object_table_sources_and_overrides() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            sources = [
                { provider = "csv", path = "gameplay.csv" },
                { provider = "csv", path = "economy.csv", header_row = 2, data_start_row = 4 },
            ]
            override_sources = [
                { provider = "csv", path = "game_config_s1.csv", export_when = { any = ["s1"] } },
                { provider = "csv", path = "game_config_dev.csv", export_when = { any = ["dev"] } },
            ]

            [[tables.fields]]
            name = "max_level"
            type = "int"

            [[tables.fields]]
            name = "gold_multiplier"
            type = "float"
        "#,
    )
    .expect("meta should load");
    let s1_tags = crate::meta::TargetTags::new(["s1"]);

    let plan = DataLoadPlan::from_tables(&meta.tables, Some(&s1_tags))
        .expect("ObjectTable source plan should build");

    assert_eq!(plan.sources().len(), 3);
    assert_eq!(plan.sources()[0].path, PathBuf::from("gameplay.csv"));
    assert_eq!(
        plan.sources()[0].layout,
        DataSourceLayout::ObjectKeyValueRows
    );
    assert_eq!(plan.sources()[0].data_start_row, 2);
    assert_eq!(
        plan.sources()[0].fields,
        vec!["max_level", "gold_multiplier"]
    );
    assert_eq!(plan.sources()[0].role, SourceRole::Base);
    assert!(plan.sources()[0].primary_key_fields.is_empty());
    assert_eq!(plan.sources()[1].path, PathBuf::from("economy.csv"));
    assert_eq!(plan.sources()[1].header_row, 2);
    assert_eq!(plan.sources()[1].data_start_row, 4);
    assert_eq!(
        plan.sources()[1].layout,
        DataSourceLayout::ObjectKeyValueRows
    );
    assert_eq!(plan.sources()[2].path, PathBuf::from("game_config_s1.csv"));
    assert_eq!(plan.sources()[2].role, SourceRole::Override);
    assert_eq!(
        plan.sources()[2].layout,
        DataSourceLayout::ObjectKeyValueRows
    );
    assert!(plan.sources()[2].primary_key_fields.is_empty());
}

#[test]
fn data_loader_read_sources_returns_raw_source_data_before_normalize() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(dir.path().join("items.csv"), "id,name\nID,Name\n1,Sword\n").expect("items csv");
    let source_read_plan = SourceReadPlan::new(vec![plan_source("items", "items.csv")]);

    let raw_sources = DataLoader::new(dir.path())
        .read_sources(&source_read_plan)
        .expect("raw sources should load");

    assert_eq!(raw_sources.as_data_set().tables().len(), 1);
    assert_eq!(raw_sources.as_data_set().tables()[0].name, "items");
    assert_eq!(
        raw_sources.as_data_set().tables()[0].rows[0]
            .get("name")
            .map(String::as_str),
        Some("Sword")
    );
    assert_eq!(
        raw_sources.as_data_set().tables()[0].row_sources[0].source_path,
        PathBuf::from("items.csv")
    );
}

#[test]
fn data_loader_rejects_absolute_source_escape() {
    let dir = tempdir().expect("temp dir should exist");
    let data_root = dir.path().join("data");
    let outside = dir.path().join("outside.csv");
    fs::create_dir_all(&data_root).expect("data root should be created");
    fs::write(&outside, "id\nID\n1\n").expect("outside csv should be written");

    let error = DataLoader::new(&data_root)
        .load_source(&plan_source("items", &outside.display().to_string()))
        .unwrap_err();

    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_PATH_ESCAPE);
    assert!(diagnostic.message.contains("escapes data root"));
    assert_eq!(diagnostic.source.as_deref(), Some("data.loader"));
    assert_eq!(diagnostic.domain.as_deref(), Some("data"));
    assert_eq!(diagnostic.location.table.as_deref(), Some("items"));
}

#[test]
fn data_loader_rejects_parent_source_escape() {
    let dir = tempdir().expect("temp dir should exist");
    let data_root = dir.path().join("data");
    fs::create_dir_all(&data_root).expect("data root should be created");

    let error = DataLoader::new(&data_root)
        .load_source(&plan_source("items", "../outside.csv"))
        .unwrap_err();

    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_PATH_ESCAPE);
    assert!(diagnostic.message.contains("escapes data root"));
    assert_eq!(diagnostic.source.as_deref(), Some("data.loader"));
    assert_eq!(diagnostic.domain.as_deref(), Some("data"));
    assert_eq!(diagnostic.location.table.as_deref(), Some("items"));
}

#[cfg(unix)]
#[test]
fn data_loader_rejects_symlink_source_escape() {
    let dir = tempdir().expect("temp dir should exist");
    let data_root = dir.path().join("data");
    let outside_dir = dir.path().join("outside");
    let outside = outside_dir.join("items.csv");
    let source_link = data_root.join("linked_items.csv");
    fs::create_dir_all(&data_root).expect("data root should be created");
    fs::create_dir_all(&outside_dir).expect("outside dir should be created");
    fs::write(&outside, "id\nID\n1\n").expect("outside csv should be written");
    std::os::unix::fs::symlink(&outside, &source_link).expect("source symlink should be created");

    let error = DataLoader::new(&data_root)
        .load_source(&plan_source("items", "linked_items.csv"))
        .unwrap_err();

    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_PATH_ESCAPE);
    assert!(diagnostic.message.contains("filesystem alias"));
    assert_eq!(diagnostic.source.as_deref(), Some("data.loader"));
    assert_eq!(diagnostic.domain.as_deref(), Some("data"));
    assert_eq!(diagnostic.location.table.as_deref(), Some("items"));
}

#[test]
fn data_load_plan_rejects_missing_source() {
    let meta: MetaModel = toml::from_str(
        r#"
            [[tables]]
            name = "items"

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("raw meta should deserialize");

    let error = DataLoadPlan::from_meta_model(&meta).unwrap_err();

    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_SOURCE_SCHEMA_INVALID);
    assert!(diagnostic.message.contains("does not declare a source"));
    assert_eq!(diagnostic.source.as_deref(), Some("data.source_plan"));
    assert_eq!(diagnostic.domain.as_deref(), Some("data"));
    assert_eq!(diagnostic.location.table.as_deref(), Some("items"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("<missing>"));
}

#[test]
fn data_load_plan_rejects_unsupported_source_features() {
    let cases = [
        (
            r#"
                [[tables]]
                name = "items"
                source = "legacy.csv"
                sources = [{ provider = "csv", path = "items.csv" }]

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
            "both legacy `source` and `sources`",
        ),
        (
            r#"
                [[tables]]
                name = "items"
                sources = [
                    { provider = "csv", path = "items.csv" },
                    { provider = "csv", path = "items-extra.csv" },
                ]

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
            "multiple sources",
        ),
        (
            r#"
                [[tables]]
                name = "items"
                polymorphic = true
                sources = [
                    { provider = "csv", path = "items.csv" },
                    { provider = "csv", path = "items-extra.csv" },
                ]

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fragments]]
                name = "weapon"

                [[tables.fragments.fields]]
                name = "power"
                type = "int"
            "#,
            "variant source mode",
        ),
        (
            r#"
                [[tables]]
                name = "items"
                polymorphic = true
                sources = [
                    { provider = "csv", path = "items.csv" },
                    { provider = "csv", path = "items-extra.csv" },
                ]

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
            "variant source mode",
        ),
        (
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                override_sources = [{ provider = "csv", path = "override.csv" }]

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
            "override_sources",
        ),
        (
            r#"
                [[tables]]
                name = "items"
                sources = [{ provider = "xlsx", path = "items.xlsx" }]

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
            "unsupported provider",
        ),
        (
            r#"
                [[tables]]
                name = "items"
                sources = [{ provider = "csv", path = "items.csv", export_when = { any = ["s1"] } }]

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
            "base source",
        ),
    ];

    for (input, expected_message) in cases {
        let meta: MetaModel = toml::from_str(input).expect("raw meta should deserialize");
        let error = DataLoadPlan::from_meta_model(&meta).unwrap_err();
        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, CF_DATA_SOURCE_SCHEMA_INVALID);
        assert_eq!(diagnostic.source.as_deref(), Some("data.source_plan"));
        assert_eq!(diagnostic.domain.as_deref(), Some("data"));
        assert_eq!(diagnostic.location.table.as_deref(), Some("items"));
        assert!(
            diagnostic.message.contains(expected_message),
            "expected error containing `{expected_message}`"
        );
    }
}

#[test]
fn data_loader_loads_multiple_tables_into_data_set() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(dir.path().join("items.csv"), "id,name\nID,Name\n1,Sword\n").expect("items csv");
    fs::write(
        dir.path().join("levels.csv"),
        "id,name\nID,Name\n1,Forest\n",
    )
    .expect("levels csv");
    let source_read_plan = SourceReadPlan::new(vec![
        plan_source("items", "items.csv"),
        plan_source("levels", "levels.csv"),
    ]);
    let plan = DataLoadPlan::new(source_read_plan.clone());

    assert_eq!(plan.source_read_plan(), &source_read_plan);

    let data_set = DataLoader::new(dir.path())
        .load(&plan)
        .expect("data should load");

    assert_eq!(data_set.tables().len(), 2);
    assert_eq!(data_set.tables()[0].name, "items");
    assert_eq!(data_set.tables()[1].name, "levels");
    assert_eq!(
        data_set.table("items").unwrap().rows[0]
            .get("name")
            .map(String::as_str),
        Some("Sword")
    );
    assert!(data_set.table("missing").is_none());
}

#[test]
fn data_loader_load_with_pool_preserves_plan_order() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(dir.path().join("items.csv"), "id,name\nID,Name\n1,Sword\n").expect("items csv");
    fs::write(
        dir.path().join("levels.csv"),
        "id,name\nID,Name\n1,Forest\n",
    )
    .expect("levels csv");
    let source_read_plan = SourceReadPlan::new(vec![
        plan_source("items", "items.csv"),
        plan_source("levels", "levels.csv"),
    ]);
    let plan = DataLoadPlan::new(source_read_plan);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("test thread pool should build");

    let data_set = DataLoader::new(dir.path())
        .load_with_pool(&plan, &pool)
        .expect("data should load");

    assert_eq!(data_set.tables()[0].name, "items");
    assert_eq!(data_set.tables()[1].name, "levels");
}

#[test]
fn data_loader_read_sources_with_pool_merges_by_plan_order_not_completion_order() {
    let plan = SourceReadPlan::new(vec![
        plan_source("items", "slow.csv"),
        plan_source("levels", "fast.csv"),
        plan_source("quests", "medium.csv"),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(3)
        .build()
        .expect("test thread pool should build");
    let completion_order = Arc::new(Mutex::new(Vec::new()));

    let data_set = DataLoader::new(".")
        .read_sources_with_reader_and_pool(&plan, &pool, |source| {
            match source.path.to_string_lossy().as_ref() {
                "slow.csv" => thread::sleep(Duration::from_millis(30)),
                "medium.csv" => thread::sleep(Duration::from_millis(10)),
                _ => {}
            }
            completion_order
                .lock()
                .expect("completion order should be lockable")
                .push(source.table_name.clone());
            Ok(DataTable::new(
                source.table_name.clone(),
                vec![DataRow::from([(
                    "source".to_owned(),
                    source.path.display().to_string(),
                )])],
            ))
        })
        .expect("raw sources should load")
        .into_data_set();

    assert_ne!(
        *completion_order
            .lock()
            .expect("completion order should be lockable"),
        vec!["items".to_owned(), "levels".to_owned(), "quests".to_owned()]
    );
    assert_eq!(
        data_set
            .tables()
            .iter()
            .map(|table| table.name.as_str())
            .collect::<Vec<_>>(),
        vec!["items", "levels", "quests"]
    );
    assert_eq!(data_set.tables()[0].row_sources[0].source_index, 0);
    assert_eq!(data_set.tables()[1].row_sources[0].source_index, 1);
    assert_eq!(data_set.tables()[2].row_sources[0].source_index, 2);
}

#[test]
fn data_loader_read_sources_with_pool_reports_first_plan_error() {
    let plan = SourceReadPlan::new(vec![
        plan_source("items", "first.csv"),
        plan_source("levels", "second.csv"),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("test thread pool should build");

    let error = DataLoader::new(".")
        .read_sources_with_reader_and_pool(&plan, &pool, |source| {
            if source.path == std::path::Path::new("first.csv") {
                thread::sleep(Duration::from_millis(20));
                return Err(data_diagnostic_error(
                    CF_DATA_SOURCE_READ_FAILED,
                    "data.test",
                    "first failed",
                    "first.csv",
                    "readable source",
                    "test failure",
                ));
            }
            Err(data_diagnostic_error(
                CF_DATA_SOURCE_READ_FAILED,
                "data.test",
                "second failed",
                "second.csv",
                "readable source",
                "test failure",
            ))
        })
        .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_DATA_SOURCE_READ_FAILED);
    assert_eq!(diagnostic.message, "first failed");
}

#[test]
fn data_loader_read_sources_with_pool_uses_configured_worker_limit() {
    let plan = SourceReadPlan::new(vec![
        plan_source("items", "items.csv"),
        plan_source("levels", "levels.csv"),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("test thread pool should build");
    let barrier = Arc::new(Barrier::new(2));
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));

    let data_set = DataLoader::new(".")
        .read_sources_with_reader_and_pool(&plan, &pool, |source| {
            let active_now = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_active.fetch_max(active_now, Ordering::SeqCst);
            barrier.wait();
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(DataTable::new(source.table_name.clone(), Vec::new()))
        })
        .expect("raw sources should load")
        .into_data_set();

    assert_eq!(data_set.tables().len(), 2);
    assert_eq!(max_active.load(Ordering::SeqCst), 2);
}

#[test]
fn data_loader_read_sources_is_serial() {
    let plan = SourceReadPlan::new(vec![
        plan_source("items", "items.csv"),
        plan_source("levels", "levels.csv"),
    ]);
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));

    DataLoader::new(".")
        .read_sources_with_reader(&plan, |source| {
            let active_now = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_active.fetch_max(active_now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(1));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(DataTable::new(source.table_name.clone(), Vec::new()))
        })
        .expect("raw sources should load");

    assert_eq!(max_active.load(Ordering::SeqCst), 1);
}

#[test]
fn async_source_read_shell_limits_network_concurrency() {
    let plan = SourceReadPlan::new(vec![
        plan_source("items", "items.csv"),
        plan_source("levels", "levels.csv"),
        plan_source("quests", "quests.csv"),
    ]);
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let provider = FakeSourceProvider {
        resource: SourceProviderResource::Network,
        active: Some(active.clone()),
        max_active: Some(max_active.clone()),
        delay: Duration::from_millis(10),
        ..FakeSourceProvider::default()
    };
    let shell = AsyncSourceReadShell::new(
        SourceProviderConcurrency::new(1, 1, 2).expect("concurrency should be valid"),
    );

    let data_set = shell
        .read_sources(&plan, &provider)
        .expect("async shell should read sources")
        .into_data_set();

    assert_eq!(max_active.load(Ordering::SeqCst), 2);
    assert_eq!(
        data_set
            .tables()
            .iter()
            .map(|table| table.name.as_str())
            .collect::<Vec<_>>(),
        vec!["items", "levels", "quests"]
    );
}

#[test]
fn async_source_read_shell_reports_first_plan_error_not_completion_order() {
    let plan = SourceReadPlan::new(vec![
        plan_source("items", "slow-first.csv"),
        plan_source("levels", "fast-second.csv"),
    ]);
    let provider = FakeSourceProvider {
        resource: SourceProviderResource::Network,
        failures: vec![
            (PathBuf::from("slow-first.csv"), "first failed".to_owned()),
            (PathBuf::from("fast-second.csv"), "second failed".to_owned()),
        ],
        per_path_delay: vec![(PathBuf::from("slow-first.csv"), Duration::from_millis(20))],
        ..FakeSourceProvider::default()
    };
    let shell = AsyncSourceReadShell::new(
        SourceProviderConcurrency::new(1, 1, 2).expect("concurrency should be valid"),
    );

    let error = shell.read_sources(&plan, &provider).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_DATA_SOURCE_READ_FAILED);
    assert_eq!(diagnostic.message, "first failed");
}

#[test]
fn async_source_read_shell_stops_scheduling_after_failure() {
    let plan = SourceReadPlan::new(vec![
        plan_source("items", "ok.csv"),
        plan_source("levels", "fail.csv"),
        plan_source("quests", "not-started.csv"),
    ]);
    let started = Arc::new(Mutex::new(Vec::new()));
    let provider = FakeSourceProvider {
        resource: SourceProviderResource::Network,
        started: Some(started.clone()),
        failures: vec![(PathBuf::from("fail.csv"), "fail requested".to_owned())],
        ..FakeSourceProvider::default()
    };
    let shell = AsyncSourceReadShell::new(
        SourceProviderConcurrency::new(1, 1, 1).expect("concurrency should be valid"),
    );

    let error = shell.read_sources(&plan, &provider).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_DATA_SOURCE_READ_FAILED);
    assert_eq!(diagnostic.message, "fail requested");
    assert_eq!(
        *started.lock().expect("started list should be lockable"),
        vec![PathBuf::from("ok.csv"), PathBuf::from("fail.csv")]
    );
}

#[test]
fn source_provider_concurrency_rejects_zero_limits() {
    for result in [
        SourceProviderConcurrency::new(0, 1, 1),
        SourceProviderConcurrency::new(1, 0, 1),
        SourceProviderConcurrency::new(1, 1, 0),
    ] {
        let diagnostic = expect_single_diagnostic(result.unwrap_err());
        assert_eq!(diagnostic.code, CF_BUILD_INVALID_OPTION);
        assert!(diagnostic.message.contains("greater than 0"));
    }
}

#[derive(Default)]
struct FakeSourceProvider {
    resource: SourceProviderResource,
    failures: Vec<(PathBuf, String)>,
    per_path_delay: Vec<(PathBuf, Duration)>,
    delay: Duration,
    active: Option<Arc<AtomicUsize>>,
    max_active: Option<Arc<AtomicUsize>>,
    started: Option<Arc<Mutex<Vec<PathBuf>>>>,
}

impl SourceProvider for FakeSourceProvider {
    fn resource(&self, _source: &DataSourcePlan) -> SourceProviderResource {
        self.resource
    }

    fn read_source(&self, source: &DataSourcePlan) -> crate::Result<DataTable> {
        if let Some(started) = &self.started {
            started
                .lock()
                .expect("started list should be lockable")
                .push(source.path.clone());
        }
        let active_now = if let Some(active) = &self.active {
            let active_now = active.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(max_active) = &self.max_active {
                max_active.fetch_max(active_now, Ordering::SeqCst);
            }
            Some(active_now)
        } else {
            None
        };
        let delay = self
            .per_path_delay
            .iter()
            .find_map(|(path, delay)| (path == &source.path).then_some(*delay))
            .unwrap_or(self.delay);
        if !delay.is_zero() {
            thread::sleep(delay);
        }
        if let (Some(_), Some(active)) = (active_now, &self.active) {
            active.fetch_sub(1, Ordering::SeqCst);
        }
        if let Some((_, message)) = self.failures.iter().find(|(path, _)| path == &source.path) {
            return Err(data_diagnostic_error(
                CF_DATA_SOURCE_READ_FAILED,
                "data.test",
                message.clone(),
                source.path.display().to_string(),
                "readable source",
                "test failure",
            ));
        }
        Ok(DataTable::new(
            source.table_name.clone(),
            vec![DataRow::from([(
                "source".to_owned(),
                source.path.display().to_string(),
            )])],
        ))
    }
}

#[test]
fn data_set_try_new_rejects_duplicate_table_names() {
    let tables = vec![
        DataTable::new("items", Vec::new()),
        DataTable::new("items", Vec::new()),
    ];

    assert!(DataSet::has_duplicate_table_names(&tables));
    let error = DataSet::try_new(tables).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);

    assert_eq!(diagnostic.code, CF_DATA_SOURCE_SCHEMA_INVALID);
    assert!(diagnostic.message.contains("duplicate table `items`"));
}

#[test]
fn data_loader_merges_multiple_sources_for_one_table() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(
        dir.path().join("weapons.csv"),
        "id,name\nID,Name\n1,Sword\n",
    )
    .expect("weapons csv");
    fs::write(
        dir.path().join("armors.csv"),
        "id,name\nID,Name\n2,Shield\n",
    )
    .expect("armors csv");
    let plan = DataLoadPlan::new(SourceReadPlan::new(vec![
        DataSourcePlan {
            fragments: vec!["weapon".to_owned()],
            ..plan_source("items", "weapons.csv")
        },
        DataSourcePlan {
            fragments: vec!["armor".to_owned()],
            ..plan_source("items", "armors.csv")
        },
    ]));

    let data_set = DataLoader::new(dir.path()).load(&plan).unwrap();
    let table = data_set.table("items").unwrap();

    assert_eq!(data_set.tables().len(), 1);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].get("name").map(String::as_str), Some("Sword"));
    assert_eq!(
        table.rows[1].get("name").map(String::as_str),
        Some("Shield")
    );
    assert_eq!(table.row_sources[0].source_index, 0);
    assert_eq!(
        table.row_sources[0].source_path,
        PathBuf::from("weapons.csv")
    );
    assert_eq!(table.row_sources[0].fragments, vec!["weapon".to_owned()]);
    assert_eq!(table.row_sources[1].source_index, 1);
    assert_eq!(
        table.row_sources[1].source_path,
        PathBuf::from("armors.csv")
    );
    assert_eq!(table.row_sources[1].fragments, vec!["armor".to_owned()]);
}

#[test]
fn data_loader_applies_override_rows_by_primary_key() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(
        dir.path().join("materials.csv"),
        "id,name,price,stack.max_stack\nID,Name,Price,Max Stack\n2001,Iron Ore,5,999\n2002,Oak Wood,3,999\n",
    )
    .expect("materials csv");
    fs::write(
        dir.path().join("materials_s1.csv"),
        "id,name,price,stack.max_stack\nID,Name,Price,Max Stack\n2002,Oak Wood,5,999\n",
    )
    .expect("override csv");
    let primary_key_fields = vec!["id".to_owned()];
    let plan = DataLoadPlan::new(SourceReadPlan::new(vec![
        DataSourcePlan {
            fragments: vec!["stack".to_owned()],
            primary_key_fields: primary_key_fields.clone(),
            ..plan_source("items", "materials.csv")
        },
        DataSourcePlan {
            fragments: vec!["stack".to_owned()],
            role: SourceRole::Override,
            primary_key_fields,
            ..plan_source("items", "materials_s1.csv")
        },
    ]));

    let data_set = DataLoader::new(dir.path())
        .load(&plan)
        .expect("data should load");
    let table = data_set.table("items").expect("items table should exist");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(
        table.rows[1].get("name").map(String::as_str),
        Some("Oak Wood")
    );
    assert_eq!(table.rows[1].get("price").map(String::as_str), Some("5"));
    assert_eq!(table.row_sources[1].source_role, SourceRole::Override);
    assert_eq!(table.row_sources[1].source_index, 1);
    assert_eq!(
        table.row_sources[1].source_path,
        PathBuf::from("materials_s1.csv")
    );
    let replaced = table.row_sources[1]
        .replaces
        .as_ref()
        .expect("override should record replaced base row");
    assert_eq!(replaced.source_index, 0);
    assert_eq!(replaced.source_path, PathBuf::from("materials.csv"));
    assert_eq!(replaced.source_row_number, 4);
}

#[test]
fn data_loader_merges_object_table_base_sources() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(
        dir.path().join("gameplay.csv"),
        "key,title,value\nmax_level,Max Level,60\n",
    )
    .expect("gameplay csv");
    fs::write(
        dir.path().join("economy.csv"),
        "key,title,value\ngold_multiplier,Gold Multiplier,1.5\n",
    )
    .expect("economy csv");
    let plan = DataLoadPlan::new(SourceReadPlan::new(vec![
        object_plan_source("game_config", "gameplay.csv"),
        object_plan_source("game_config", "economy.csv"),
    ]));

    let data_set = DataLoader::new(dir.path())
        .load(&plan)
        .expect("object sources should merge");
    let table = data_set
        .table("game_config")
        .expect("game_config table should exist");

    assert_eq!(table.rows.len(), 1);
    assert_eq!(
        table.rows[0].get("max_level").map(String::as_str),
        Some("60")
    );
    assert_eq!(
        table.rows[0].get("gold_multiplier").map(String::as_str),
        Some("1.5")
    );
    assert_eq!(
        table.row_sources[0].source_path,
        PathBuf::from("gameplay.csv")
    );
    assert_eq!(
        table
            .cell_source(0, "gold_multiplier")
            .expect("cell source should exist")
            .source_row_number,
        2
    );
}

#[test]
fn data_loader_applies_object_table_override_field_patch() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(
        dir.path().join("gameplay.csv"),
        "key,title,value\nmax_level,Max Level,60\nfeature_flags[0],Feature A,a\nfeature_flags[1],Feature B,b\n",
    )
    .expect("gameplay csv");
    fs::write(
        dir.path().join("game_s1.csv"),
        "key,title,value\nmax_level,Max Level,70\n",
    )
    .expect("override csv");
    let plan = DataLoadPlan::new(SourceReadPlan::new(vec![
        object_plan_source("game_config", "gameplay.csv"),
        DataSourcePlan {
            role: SourceRole::Override,
            ..object_plan_source("game_config", "game_s1.csv")
        },
    ]));

    let data_set = DataLoader::new(dir.path())
        .load(&plan)
        .expect("object override should patch");
    let table = data_set
        .table("game_config")
        .expect("game_config table should exist");

    assert_eq!(table.rows.len(), 1);
    assert_eq!(
        table.rows[0].get("max_level").map(String::as_str),
        Some("70")
    );
    assert_eq!(
        table.rows[0].get("feature_flags[0]").map(String::as_str),
        Some("a")
    );
    assert_eq!(
        table
            .cell_source(0, "max_level")
            .expect("override cell source should exist")
            .source_row_number,
        2
    );
}

#[test]
fn data_loader_applies_later_object_table_override_last() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(
        dir.path().join("gameplay.csv"),
        "key,title,value\nmax_level,Max Level,60\n",
    )
    .expect("gameplay csv");
    fs::write(
        dir.path().join("game_s1.csv"),
        "key,title,value\nmax_level,Max Level,70\n",
    )
    .expect("first override csv");
    fs::write(
        dir.path().join("game_hotfix.csv"),
        "key,title,value\nmax_level,Max Level,80\n",
    )
    .expect("second override csv");
    let plan = DataLoadPlan::new(SourceReadPlan::new(vec![
        object_plan_source("game_config", "gameplay.csv"),
        DataSourcePlan {
            role: SourceRole::Override,
            ..object_plan_source("game_config", "game_s1.csv")
        },
        DataSourcePlan {
            role: SourceRole::Override,
            ..object_plan_source("game_config", "game_hotfix.csv")
        },
    ]));

    let data_set = DataLoader::new(dir.path())
        .load(&plan)
        .expect("later object override should win");
    let table = data_set.table("game_config").unwrap();

    assert_eq!(
        table.rows[0].get("max_level").map(String::as_str),
        Some("80")
    );
    assert_eq!(
        table.cell_source(0, "max_level").unwrap().source_row_number,
        2
    );
}

#[test]
fn data_loader_keeps_empty_object_table_singleton_row() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(dir.path().join("empty.csv"), "key,title,value\n").expect("empty csv");
    let plan = DataLoadPlan::new(SourceReadPlan::new(vec![object_plan_source(
        "game_config",
        "empty.csv",
    )]));

    let data_set = DataLoader::new(dir.path())
        .load(&plan)
        .expect("empty object table should load");
    let table = data_set.table("game_config").unwrap();

    assert_eq!(table.rows.len(), 1);
    assert!(table.rows[0].is_empty());
}

#[test]
fn data_loader_rejects_invalid_object_table_merge_sources() {
    let cases = [
        (
            "base_a.csv",
            "key,title,value\nmax_level,Max Level,60\n",
            "base_b.csv",
            "key,title,value\nmax_level,Max Level,70\n",
            SourceRole::Base,
            CF_DATA_DUPLICATE_KEY,
            "duplicates ObjectTable field path",
        ),
        (
            "base.csv",
            "key,title,value\nmax_level,Max Level,60\n",
            "override.csv",
            "key,title,value\ngold_multiplier,Gold Multiplier,1.5\n",
            SourceRole::Override,
            CF_DATA_SOURCE_SCHEMA_INVALID,
            "does not contain that field path",
        ),
    ];

    for (base_path, base_content, patch_path, patch_content, role, code, expected_message) in cases
    {
        let dir = tempdir().expect("temp dir should exist");
        fs::write(dir.path().join(base_path), base_content).expect("base csv");
        fs::write(dir.path().join(patch_path), patch_content).expect("patch csv");
        let plan = DataLoadPlan::new(SourceReadPlan::new(vec![
            object_plan_source("game_config", base_path),
            DataSourcePlan {
                role,
                ..object_plan_source("game_config", patch_path)
            },
        ]));

        let error = DataLoader::new(dir.path()).load(&plan).unwrap_err();
        let diagnostic = expect_single_diagnostic(error);
        assert_eq!(diagnostic.code, code);
        assert_eq!(diagnostic.source.as_deref(), Some("data.loader"));
        assert_eq!(diagnostic.domain.as_deref(), Some("data"));
        assert!(diagnostic.message.contains(expected_message));
        assert_eq!(diagnostic.location.table.as_deref(), Some("game_config"));
        assert_eq!(diagnostic.location.source_row_number, Some(2));
    }
}

#[test]
fn data_loader_rejects_invalid_override_rows() {
    let cases = [
        (
            "base.csv",
            "id,name\nID,Name\n1,Sword\n1,Shield\n",
            "override.csv",
            "id,name\nID,Name\n1,Axe\n",
            "duplicates primary key",
        ),
        (
            "base.csv",
            "id,name\nID,Name\n1,Sword\n",
            "override.csv",
            "id,name\nID,Name\n2,Axe\n",
            "references missing base primary key",
        ),
        (
            "base.csv",
            "id,name\nID,Name\n1,Sword\n",
            "override.csv",
            "id,name\nID,Name\n,Axe\n",
            "empty primary index field",
        ),
    ];

    for (base_path, base_content, override_path, override_content, expected_message) in cases {
        let dir = tempdir().expect("temp dir should exist");
        fs::write(dir.path().join(base_path), base_content).expect("base csv");
        fs::write(dir.path().join(override_path), override_content).expect("override csv");
        let primary_key_fields = vec!["id".to_owned()];
        let plan = DataLoadPlan::new(SourceReadPlan::new(vec![
            DataSourcePlan {
                primary_key_fields: primary_key_fields.clone(),
                ..plan_source("items", base_path)
            },
            DataSourcePlan {
                role: SourceRole::Override,
                primary_key_fields,
                ..plan_source("items", override_path)
            },
        ]));

        let error = DataLoader::new(dir.path()).load(&plan).unwrap_err();
        let diagnostic = expect_single_diagnostic(error);
        assert!(
            diagnostic.code == CF_DATA_DUPLICATE_KEY
                || diagnostic.code == CF_DATA_SOURCE_SCHEMA_INVALID
        );
        assert_eq!(diagnostic.source.as_deref(), Some("data.loader"));
        assert_eq!(diagnostic.domain.as_deref(), Some("data"));
        assert_eq!(diagnostic.location.table.as_deref(), Some("items"));
        assert!(
            diagnostic.message.contains(expected_message),
            "expected message containing `{expected_message}`"
        );
    }
}
