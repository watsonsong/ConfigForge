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
    assert_eq!(
        data_set.table("items").unwrap().rows[0]
            .get("name")
            .map(String::as_str),
        Some("Sword")
    );
    assert!(data_set.table("missing").is_none());
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
