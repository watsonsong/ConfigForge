use std::{fs, path::PathBuf};

use tempfile::tempdir;

use crate::{meta::MetaModel, ConfigForgeError};

use super::*;

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
    fs::write(&csv_path, "id,name\n1,Sword\n2,Shield\n").expect("csv should be written");

    let table = CsvDataReader::read_table("items", &csv_path).expect("csv should load");

    assert_eq!(table.name, "items");
    assert_eq!(table.row_count(), 2);
    assert!(!table.is_empty());
    assert_eq!(table.rows[0].get("id").map(String::as_str), Some("1"));
    assert_eq!(
        table.rows[1].get("name").map(String::as_str),
        Some("Shield")
    );
}

#[test]
fn csv_reader_reports_parse_errors() {
    let error = CsvDataReader::read_table("items", &PathBuf::from("missing.csv")).unwrap_err();

    assert!(matches!(error, ConfigForgeError::ParseCsv { .. }));
}

#[test]
fn csv_reader_rejects_duplicate_headers_but_allows_empty_header() {
    let dir = tempdir().expect("temp dir should exist");
    let duplicate_path = dir.path().join("duplicate.csv");
    fs::write(&duplicate_path, "id,id\n1,2\n").expect("csv should be written");

    let error = CsvDataReader::read_table("items", &duplicate_path).unwrap_err();
    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("duplicate header `id`"))
    );

    let empty_header_path = dir.path().join("empty_header.csv");
    fs::write(&empty_header_path, ",name\nraw,Sword\n").expect("csv should be written");
    let table = CsvDataReader::read_table("items", &empty_header_path).expect("csv should load");

    assert_eq!(table.rows[0].get("").map(String::as_str), Some("raw"));
    assert_eq!(table.rows[0].get("name").map(String::as_str), Some("Sword"));
}

#[test]
fn csv_reader_handles_common_csv_boundaries() {
    let dir = tempdir().expect("temp dir should exist");

    let only_header_path = dir.path().join("only_header.csv");
    fs::write(&only_header_path, "id,name\n").expect("csv should be written");
    let table = CsvDataReader::read_table("items", &only_header_path).expect("csv should load");
    assert!(table.is_empty());

    let quoted_path = dir.path().join("quoted.csv");
    fs::write(
        &quoted_path,
        "id,desc\n1,\"hello, world\"\n2,\"line1\nline2\"\n",
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
    fs::write(&windows_bom_path, "\u{feff}id,name\r\n1,Sword\r\n").expect("csv should be written");
    let table = CsvDataReader::read_table("items", &windows_bom_path).expect("csv should load");
    assert_eq!(table.rows[0].get("id").map(String::as_str), Some("1"));
    assert_eq!(table.rows[0].get("name").map(String::as_str), Some("Sword"));
}

#[test]
fn csv_reader_reports_empty_file_and_ragged_rows() {
    let dir = tempdir().expect("temp dir should exist");

    let empty_path = dir.path().join("empty.csv");
    fs::write(&empty_path, "").expect("csv should be written");
    assert!(
        matches!(CsvDataReader::read_table("items", &empty_path).unwrap_err(), ConfigForgeError::Validation(message) if message.contains("no header row"))
    );

    let missing_path = dir.path().join("missing.csv");
    fs::write(&missing_path, "id,name\n1\n").expect("csv should be written");
    assert!(matches!(
        CsvDataReader::read_table("items", &missing_path).unwrap_err(),
        ConfigForgeError::ParseCsv { .. }
    ));

    let extra_path = dir.path().join("extra.csv");
    fs::write(&extra_path, "id,name\n1,Sword,extra\n").expect("csv should be written");
    assert!(matches!(
        CsvDataReader::read_table("items", &extra_path).unwrap_err(),
        ConfigForgeError::ParseCsv { .. }
    ));
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
    assert_eq!(plan.sources()[1].table_name, "items");
    assert_eq!(plan.sources()[1].path, PathBuf::from("items.csv"));
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

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("does not declare a source"))
    );
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
                sources = [{ provider = "csv", path = "items.csv", fragments = ["stats"] }]

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
            "fragments",
        ),
    ];

    for (input, expected_message) in cases {
        let meta: MetaModel = toml::from_str(input).expect("raw meta should deserialize");
        let error = DataLoadPlan::from_meta_model(&meta).unwrap_err();
        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains(expected_message)),
            "expected error containing `{expected_message}`"
        );
    }
}

#[test]
fn data_loader_loads_multiple_tables_into_data_set() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(dir.path().join("items.csv"), "id,name\n1,Sword\n").expect("items csv");
    fs::write(dir.path().join("levels.csv"), "id,name\n1,Forest\n").expect("levels csv");
    let plan = DataLoadPlan {
        sources: vec![
            DataSourcePlan {
                table_name: "items".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("items.csv"),
            },
            DataSourcePlan {
                table_name: "levels".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("levels.csv"),
            },
        ],
    };

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
    fs::write(dir.path().join("items.csv"), "id,name\n1,Sword\n").expect("items csv");
    fs::write(dir.path().join("levels.csv"), "id,name\n1,Forest\n").expect("levels csv");
    let plan = DataLoadPlan {
        sources: vec![
            DataSourcePlan {
                table_name: "items".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("items.csv"),
            },
            DataSourcePlan {
                table_name: "levels".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("levels.csv"),
            },
        ],
    };
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
fn data_set_try_new_rejects_duplicate_table_names() {
    let tables = vec![
        DataTable::new("items", Vec::new()),
        DataTable::new("items", Vec::new()),
    ];

    assert!(DataSet::has_duplicate_table_names(&tables));
    let error = DataSet::try_new(tables).unwrap_err();

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("duplicate table `items`"))
    );
}

#[test]
fn data_loader_rejects_duplicate_table_names() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(dir.path().join("items.csv"), "id,name\n1,Sword\n").expect("items csv");
    let plan = DataLoadPlan {
        sources: vec![
            DataSourcePlan {
                table_name: "items".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("items.csv"),
            },
            DataSourcePlan {
                table_name: "items".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("items.csv"),
            },
        ],
    };

    let error = DataLoader::new(dir.path()).load(&plan).unwrap_err();

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("duplicate table `items`"))
    );
}
