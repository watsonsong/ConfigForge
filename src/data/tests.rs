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
    assert_eq!(
        plan.sources()[0].layout,
        DataSourceLayout::CollectionRecords
    );
    assert_eq!(plan.sources()[1].table_name, "items");
    assert_eq!(plan.sources()[1].path, PathBuf::from("items.csv"));
    assert_eq!(
        plan.sources()[1].layout,
        DataSourceLayout::CollectionRecords
    );
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
}

#[test]
fn csv_reader_reads_object_key_value_rows_as_one_logical_row() {
    let dir = tempdir().expect("temp dir should exist");
    let csv_path = dir.path().join("game_config.csv");
    fs::write(&csv_path, "key,value\nmax_level,100\nstart_gold,500\n")
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

    let only_header_path = dir.path().join("empty_config.csv");
    fs::write(&only_header_path, "key,value\n").expect("csv should be written");
    let table = CsvDataReader::read_object_table("game_config", &only_header_path, &[])
        .expect("only-header object csv should load");
    assert_eq!(table.rows.len(), 1);
    assert!(table.rows[0].is_empty());

    let whitespace_header_path = dir.path().join("whitespace_header.csv");
    fs::write(&whitespace_header_path, " key , value \nmax_level,100\n")
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
            "field,value\nmax_level,100\n",
            "header must contain exactly two columns named `key` and `value`",
        ),
        (
            "missing_value_header.csv",
            "key\nmax_level\n",
            "header must contain exactly two columns named `key` and `value`",
        ),
        (
            "extra_header.csv",
            "key,value,extra\nmax_level,100,ignored\n",
            "header must contain exactly two columns named `key` and `value`",
        ),
        ("empty_key.csv", "key,value\n,100\n", "empty key"),
        (
            "duplicate_key.csv",
            "key,value\nmax_level,100\nmax_level,101\n",
            "duplicates key",
        ),
    ];

    for (file_name, content, expected_message) in cases {
        let path = dir.path().join(file_name);
        fs::write(&path, content).expect("csv should be written");
        let error =
            CsvDataReader::read_object_table("game_config", &path, &["max_level".to_owned()])
                .unwrap_err();
        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains(expected_message)),
            "expected message containing `{expected_message}`"
        );
    }
}

#[test]
fn csv_reader_reports_object_table_ragged_rows() {
    let dir = tempdir().expect("temp dir should exist");
    let missing_path = dir.path().join("missing_value.csv");
    fs::write(&missing_path, "key,value\nmax_level\n").expect("csv should be written");
    assert!(matches!(
        CsvDataReader::read_object_table("game_config", &missing_path, &["max_level".to_owned()])
            .unwrap_err(),
        ConfigForgeError::ParseCsv { .. }
    ));

    let extra_path = dir.path().join("extra_value.csv");
    fs::write(&extra_path, "key,value\nmax_level,100,extra\n").expect("csv should be written");
    assert!(matches!(
        CsvDataReader::read_object_table("game_config", &extra_path, &["max_level".to_owned()])
            .unwrap_err(),
        ConfigForgeError::ParseCsv { .. }
    ));
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
    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("mixes struct JSON cell"))
    );

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
                kind = "object"
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
                fragments: vec![],
                layout: DataSourceLayout::CollectionRecords,
                fields: vec![],
            },
            DataSourcePlan {
                table_name: "levels".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("levels.csv"),
                fragments: vec![],
                layout: DataSourceLayout::CollectionRecords,
                fields: vec![],
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
                fragments: vec![],
                layout: DataSourceLayout::CollectionRecords,
                fields: vec![],
            },
            DataSourcePlan {
                table_name: "levels".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("levels.csv"),
                fragments: vec![],
                layout: DataSourceLayout::CollectionRecords,
                fields: vec![],
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
fn data_loader_merges_multiple_sources_for_one_table() {
    let dir = tempdir().expect("temp dir should exist");
    fs::write(dir.path().join("weapons.csv"), "id,name\n1,Sword\n").expect("weapons csv");
    fs::write(dir.path().join("armors.csv"), "id,name\n2,Shield\n").expect("armors csv");
    let plan = DataLoadPlan {
        sources: vec![
            DataSourcePlan {
                table_name: "items".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("weapons.csv"),
                fragments: vec!["weapon".to_owned()],
                layout: DataSourceLayout::CollectionRecords,
                fields: vec![],
            },
            DataSourcePlan {
                table_name: "items".to_owned(),
                provider: DataProvider::Csv,
                path: PathBuf::from("armors.csv"),
                fragments: vec!["armor".to_owned()],
                layout: DataSourceLayout::CollectionRecords,
                fields: vec![],
            },
        ],
    };

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
