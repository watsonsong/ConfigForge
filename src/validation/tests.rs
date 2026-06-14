// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use crate::{
    data::{CellSourceInfo, RowSourceInfo, SourceRole},
    diagnostic::{
        Diagnostic, DiagnosticLocation, DiagnosticSeverity,
        code::{
            CF_DATA_DUPLICATE_KEY, CF_DATA_INVALID_ASSET_REF, CF_DATA_INVALID_FOREIGN_KEY,
            CF_DATA_INVALID_TYPE, CF_DATA_MISSING_REQUIRED_FIELD, CF_VALIDATION_INVALID_CONFIG,
        },
    },
};

use crate::meta::{
    EnumMeta, EnumValueMeta, ExportCondition, FieldMeta, LinkedField, LinkedFieldMeta,
    LinkedTableMeta, MetaModel, PrimitiveFieldType, TableMeta, TargetTags, ValidationSeverity,
};
use crate::project::{AssetSearchPathConfig, AssetsConfig};
use crate::{ConfigForgeError, Result};

use super::defaults::normalize_data_set_with_normalizer_and_pool;
use super::diagnostic::error_issues_from_result;
use super::*;

fn validate_error(result: Result<()>) -> String {
    match result.unwrap_err() {
        ConfigForgeError::Validation(message) => message,
        ConfigForgeError::ValidationReport(report) => report.error_messages().join("; "),
        error => panic!("expected validation error, got {error}"),
    }
}

fn validation_report_error<T>(result: Result<T>) -> ValidationReport {
    match result {
        Err(ConfigForgeError::ValidationReport(report)) => ValidationReport {
            issues: report
                .diagnostics
                .into_iter()
                .map(|diagnostic| ValidationIssue {
                    code: diagnostic.code,
                    severity: diagnostic.severity,
                    message: diagnostic.message,
                    location: diagnostic.location,
                    validator: diagnostic.source.unwrap_or_default(),
                    target: diagnostic.target,
                    hint: diagnostic.hint,
                    current_value: diagnostic.current_value,
                    expected: diagnostic.expected,
                    explanation: diagnostic.explanation,
                    domain: diagnostic.domain,
                    owner: diagnostic.owner,
                })
                .collect(),
        },
        Err(error) => panic!("expected validation report error, got {error}"),
        Ok(_) => panic!("expected validation report error, got ok"),
    }
}

fn expect_validator_config_issue<T>(result: Result<T>, source: &str) -> ValidationIssue {
    let report = validation_report_error(result);
    assert_eq!(report.issues.len(), 1);
    let issue = report.issues.into_iter().next().expect("one issue");
    assert_eq!(issue.code, CF_VALIDATION_INVALID_CONFIG);
    assert_eq!(issue.validator, source);
    assert_eq!(issue.domain.as_deref(), Some("validation"));
    assert!(issue.current_value.is_some());
    assert!(issue.expected.is_some());
    assert!(issue.hint.is_some());
    issue
}

#[test]
fn validation_outcome_preserves_error_report_before_error_gate() {
    let issue = ValidationIssue::error(
        "table.missing",
        "required",
        "data set is missing table `items`",
        DiagnosticLocation {
            table: Some("items".to_owned()),
            ..DiagnosticLocation::default()
        },
    )
    .with_hint("add items.csv");
    let mut report = ValidationReport::new();
    report.push(issue.clone());

    let outcome = report.into_outcome();

    assert!(outcome.is_blocking());
    assert_eq!(outcome.report().issues, [issue.clone()]);
    let preserved_report = outcome.clone().into_report();
    assert_eq!(preserved_report.issues, [issue.clone()]);
    let diagnostic_report = match outcome.into_result() {
        Err(ConfigForgeError::ValidationReport(report)) => report,
        Err(error) => panic!("expected validation report error, got {error}"),
        Ok(_) => panic!("expected blocking validation outcome"),
    };
    assert_eq!(diagnostic_report.diagnostics.len(), 1);
    assert_eq!(diagnostic_report.diagnostics[0].code, issue.code);
    assert_eq!(
        diagnostic_report.diagnostics[0].location.table.as_deref(),
        Some("items")
    );
    assert_eq!(
        diagnostic_report.diagnostics[0].hint.as_deref(),
        Some("add items.csv")
    );
}

#[test]
fn validation_outcome_warning_only_is_not_blocking() {
    let issue = ValidationIssue::new(
        "validator.warning",
        ValidationSeverity::Warning,
        "custom_validator",
        "warning only",
        DiagnosticLocation::default(),
    );
    let mut report = ValidationReport::new();
    report.push(issue.clone());

    let outcome = report.into_outcome();

    assert!(!outcome.is_blocking());
    let returned_report = outcome.into_result().expect("warning-only outcome passes");
    assert_eq!(returned_report.issues, [issue]);
}

#[test]
fn error_issues_from_result_preserves_all_report_diagnostics() {
    let fallback_location = DiagnosticLocation {
        table: Some("items".to_owned()),
        ..DiagnosticLocation::default()
    };
    let result = Err(ConfigForgeError::ValidationReport(
        crate::diagnostic::DiagnosticReport {
            diagnostics: vec![
                Diagnostic::new(
                    "first.issue",
                    DiagnosticSeverity::Error,
                    "first failed",
                    DiagnosticLocation::default(),
                )
                .with_source("first_validator")
                .with_target("client")
                .with_hint("fix the first value")
                .with_current_value("bad")
                .with_expected("good")
                .with_explanation("first explanation")
                .with_domain("data")
                .with_owner("designer"),
                Diagnostic::new(
                    "second.issue",
                    DiagnosticSeverity::Warning,
                    "second failed",
                    DiagnosticLocation {
                        table: Some("classes".to_owned()),
                        field: Some("id".to_owned()),
                        ..DiagnosticLocation::default()
                    },
                ),
            ],
        },
    ));

    let issues = error_issues_from_result(
        "fallback.issue",
        "fallback_validator",
        result,
        fallback_location.clone(),
    );

    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].code, "first.issue");
    assert_eq!(issues[0].severity, DiagnosticSeverity::Error);
    assert_eq!(issues[0].validator, "first_validator");
    assert_eq!(issues[0].message, "first failed");
    assert_eq!(issues[0].location, fallback_location);
    assert_eq!(issues[0].target.as_deref(), Some("client"));
    assert_eq!(issues[0].hint.as_deref(), Some("fix the first value"));
    assert_eq!(issues[0].current_value.as_deref(), Some("bad"));
    assert_eq!(issues[0].expected.as_deref(), Some("good"));
    assert_eq!(issues[0].explanation.as_deref(), Some("first explanation"));
    assert_eq!(issues[0].domain.as_deref(), Some("data"));
    assert_eq!(issues[0].owner.as_deref(), Some("designer"));

    assert_eq!(issues[1].code, "second.issue");
    assert_eq!(issues[1].severity, DiagnosticSeverity::Warning);
    assert_eq!(issues[1].validator, "fallback_validator");
    assert_eq!(issues[1].message, "second failed");
    assert_eq!(issues[1].location.table.as_deref(), Some("classes"));
    assert_eq!(issues[1].location.field.as_deref(), Some("id"));
}

fn fragment_row_source(source_row_number: usize, fragments: &[&str]) -> RowSourceInfo {
    RowSourceInfo {
        source_index: 0,
        source_path: PathBuf::from("items.csv"),
        source_row_number,
        fragments: fragments
            .iter()
            .map(|fragment| (*fragment).to_owned())
            .collect(),
        source_role: SourceRole::Base,
        replaces: None,
    }
}

#[test]
fn rejects_invalid_type() {
    let meta = TableMeta {
        name: "items".to_owned(),
        kind: crate::meta::TableKind::Collection,
        desc: None,
        export: true,
        export_when: ExportCondition::default(),
        row_export: None,
        validators: vec![],
        source: Some(crate::meta::SourceRef::csv("items.csv")),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "id".to_owned(),
            desc: None,
            field_type: "int".to_owned(),
            asset_kind: None,
            required: true,
            default: None,
            layout: None,
            export: true,
            export_when: ExportCondition::default(),
            validators: vec![],
            tool_only: None,
            filter_field: None,
            validator_only: None,
            field_number: None,
            unique: true,
            foreign_key: None,
            color_format: None,
            color_channel: None,
            color_space: None,
            color_alpha: None,
            vector_format: None,
            vector_components: None,
            unit: None,
        }],
        fragments: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        primary_index: None,
        preserve_source_order: false,
        polymorphic: false,
    };
    let linked_meta = LinkedTableMeta::new(
        &meta,
        vec![LinkedFieldMeta {
            meta: &meta.fields[0],
            linked_field: LinkedField::Primitive(PrimitiveFieldType::Int),
        }],
        vec![],
        vec![],
        vec![],
    );
    let table = DataTable::new(
        "items",
        vec![BTreeMap::from([("id".to_owned(), "abc".to_owned())])],
    );

    let report = validation_report_error(validate_table(&linked_meta, &table));
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.code == CF_DATA_INVALID_TYPE)
        .expect("invalid type diagnostic");
    assert_eq!(issue.location.table.as_deref(), Some("items"));
    assert_eq!(issue.location.field.as_deref(), Some("id"));
    assert_eq!(issue.current_value.as_deref(), Some("abc"));
    assert_eq!(issue.expected.as_deref(), Some("int"));
}

#[test]
fn applies_defaults_before_required_and_type_validation() {
    let meta = TableMeta {
        name: "items".to_owned(),
        kind: crate::meta::TableKind::Collection,
        desc: None,
        export: true,
        export_when: ExportCondition::default(),
        row_export: None,
        validators: vec![],
        source: Some(crate::meta::SourceRef::csv("items.csv")),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "max_stack".to_owned(),
            desc: None,
            field_type: "int".to_owned(),
            asset_kind: None,
            required: true,
            default: Some(toml::Value::Integer(1)),
            layout: None,
            export: true,
            export_when: ExportCondition::default(),
            validators: vec![],
            tool_only: None,
            filter_field: None,
            validator_only: None,
            field_number: None,
            unique: false,
            foreign_key: None,
            color_format: None,
            color_channel: None,
            color_space: None,
            color_alpha: None,
            vector_format: None,
            vector_components: None,
            unit: None,
        }],
        fragments: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        primary_index: None,
        preserve_source_order: false,
        polymorphic: false,
    };
    let linked_meta = LinkedTableMeta::new(
        &meta,
        vec![LinkedFieldMeta {
            meta: &meta.fields[0],
            linked_field: LinkedField::Primitive(PrimitiveFieldType::Int),
        }],
        vec![],
        vec![],
        vec![],
    );
    let table = DataTable::new("items", vec![BTreeMap::new()]);

    assert!(validate_table(&linked_meta, &table).is_ok());
    let table = apply_field_defaults(&linked_meta, &table).unwrap();
    assert_eq!(
        table.rows[0].get("max_stack").map(String::as_str),
        Some("1")
    );
}

#[test]
fn allows_missing_optional_field_without_default() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "note"
                type = "string"
                required = false
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn validates_builtin_values_through_resolver_and_report_trace() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "effects"
            source = "effects.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "spawn_position"
            type = "vector3"
            vector_format = "expanded"
            required = true

            [[tables.fields]]
            name = "tint"
            type = "color"
            color_format = "hex_rgb"
            color_channel = "byte"
            alpha = "default_opaque"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("effects").expect("table should exist");
    let table = crate::data::DataTable::new_with_sources(
        "effects",
        vec![crate::data::DataRow::from([
            ("id".to_owned(), "1".to_owned()),
            ("spawn_position.x".to_owned(), "1".to_owned()),
            ("spawn_position.z".to_owned(), "3".to_owned()),
            ("tint".to_owned(), "#336699".to_owned()),
        ])],
        vec![RowSourceInfo {
            source_index: 0,
            source_path: PathBuf::from("effects.csv"),
            source_row_number: 7,
            fragments: Vec::new(),
            source_role: SourceRole::Base,
            replaces: None,
        }],
    )
    .expect("table should build");

    let report = super::table::validate_normalized_table_with_report(table_meta, &table);

    assert!(report.has_error());
    assert!(report.issues.iter().any(|issue| {
        issue.code == CF_DATA_INVALID_TYPE
            && issue.location.table.as_deref() == Some("effects")
            && issue.location.field.as_deref() == Some("spawn_position")
            && issue.location.source_row_number == Some(7)
            && issue.message.contains("incomplete expanded vector")
    }));
}

#[test]
fn rejects_builtin_value_fields_as_unique_index_keys() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "effects"
            source = "effects.csv"

            [[tables.fields]]
            name = "tint"
            type = "color"
            color_format = "hex_rgba"
            color_channel = "normalized"

            [[tables.indexes]]
            name = "by_tint"
            fields = ["tint"]
            unique = true
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let table_meta = linked.table("effects").expect("table should exist");
    let table = crate::data::DataTable::new(
        "effects",
        vec![crate::data::DataRow::from([(
            "tint".to_owned(),
            "0.1,0.2,0.3,1.0".to_owned(),
        )])],
    );

    let report = super::table::validate_normalized_table_with_report(table_meta, &table);

    assert!(report.has_error());
    assert!(report.issues.iter().any(|issue| {
        issue.code == CF_DATA_INVALID_TYPE
            && issue.location.field.as_deref() == Some("tint")
            && issue.message.contains("unsupported builtin value key type")
    }));
}

#[test]
fn validates_enum_values() {
    let enum_meta = EnumMeta {
        name: "ItemRarity".to_owned(),
        desc: None,
        values: vec![EnumValueMeta {
            name: "common".to_owned(),
            desc: None,
        }],
    };
    let meta = TableMeta {
        name: "items".to_owned(),
        kind: crate::meta::TableKind::Collection,
        desc: None,
        export: true,
        export_when: ExportCondition::default(),
        row_export: None,
        validators: vec![],
        source: Some(crate::meta::SourceRef::csv("items.csv")),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "rarity".to_owned(),
            desc: None,
            field_type: "ItemRarity".to_owned(),
            asset_kind: None,
            required: true,
            default: None,
            layout: None,
            export: true,
            export_when: ExportCondition::default(),
            validators: vec![],
            tool_only: None,
            filter_field: None,
            validator_only: None,
            field_number: None,
            unique: false,
            foreign_key: None,
            color_format: None,
            color_channel: None,
            color_space: None,
            color_alpha: None,
            vector_format: None,
            vector_components: None,
            unit: None,
        }],
        fragments: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        primary_index: None,
        preserve_source_order: false,
        polymorphic: false,
    };
    let linked_meta = LinkedTableMeta::new(
        &meta,
        vec![LinkedFieldMeta {
            meta: &meta.fields[0],
            linked_field: LinkedField::Enum(&enum_meta),
        }],
        vec![],
        vec![],
        vec![],
    );
    let valid_table = DataTable::new(
        "items",
        vec![BTreeMap::from([("rarity".to_owned(), "common".to_owned())])],
    );
    let invalid_table = DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "rarity".to_owned(),
            "legendary".to_owned(),
        )])],
    );

    assert!(validate_table(&linked_meta, &valid_table).is_ok());
    assert!(validate_table(&linked_meta, &invalid_table).is_err());
}

#[test]
fn validates_fragment_scoped_fields_and_defaults() {
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
            required = true

            [[tables.fragments]]
            name = "weapon"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
            required = true
            default = 1

            [[tables.fragments]]
            name = "armor"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let table_meta = linked_meta.table("items").expect("linked table");
    let table = DataTable::new_with_sources(
        "items",
        vec![
            BTreeMap::from([("id".to_owned(), "1".to_owned())]),
            BTreeMap::from([
                ("id".to_owned(), "2".to_owned()),
                ("armor.power".to_owned(), "10".to_owned()),
            ]),
        ],
        vec![
            RowSourceInfo {
                source_index: 0,
                source_path: PathBuf::from("weapons.csv"),
                source_row_number: 1,
                fragments: vec!["weapon".to_owned()],
                source_role: SourceRole::Base,
                replaces: None,
            },
            RowSourceInfo {
                source_index: 1,
                source_path: PathBuf::from("armors.csv"),
                source_row_number: 1,
                fragments: vec!["armor".to_owned()],
                source_role: SourceRole::Base,
                replaces: None,
            },
        ],
    )
    .expect("table should build");

    assert!(validate_table(table_meta, &table).is_ok());
    let normalized = apply_field_defaults(table_meta, &table).expect("defaults should apply");
    assert_eq!(
        normalized.rows[0].get("weapon.power").map(String::as_str),
        Some("1")
    );

    let inactive_fragment_table = DataTable::new_with_sources(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "1".to_owned()),
            ("armor.power".to_owned(), "10".to_owned()),
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
    let message = validate_error(validate_table(table_meta, &inactive_fragment_table));
    assert!(message.contains("inactive fragment `armor`"));
}

fn foreign_key_meta() -> MetaModel {
    MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            required = true
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load")
}

fn foreign_key_data(drop_item_id: &str) -> DataSet {
    DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([
                ("id".to_owned(), "100".to_owned()),
                ("drop_item_id".to_owned(), drop_item_id.to_owned()),
            ])],
        ),
    ])
}

#[test]
fn validates_foreign_key_reference_integrity() {
    let meta = foreign_key_meta();
    let linked_meta = meta.link().expect("meta should link");

    assert!(validate_data_set(&linked_meta, &foreign_key_data("1")).is_ok());

    let message = validate_error(validate_data_set(&linked_meta, &foreign_key_data("2")));
    assert!(message.contains("foreign key `drop_item` references missing `items.by_id` key"));

    let report = validation_report_error(validate_data_set(&linked_meta, &foreign_key_data("2")));
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.code == CF_DATA_INVALID_FOREIGN_KEY)
        .expect("foreign key diagnostic");
    assert_eq!(issue.location.table.as_deref(), Some("monsters"));
    assert_eq!(issue.location.field.as_deref(), Some("drop_item_id"));
    assert_eq!(issue.current_value.as_deref(), Some("[\"2\"]"));
    assert!(
        issue
            .expected
            .as_deref()
            .is_some_and(|expected| expected.contains("items") && expected.contains("by_id"))
    );
}

#[test]
fn uses_raw_string_foreign_key_semantics() {
    let meta = foreign_key_meta();
    let linked_meta = meta.link().expect("meta should link");

    let message = validate_error(validate_data_set(&linked_meta, &foreign_key_data("01")));
    assert!(message.contains("references missing `items.by_id` key `[\"01\"]`"));
}

#[test]
fn validates_list_foreign_key_defaults_through_shared_list_parser() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_ids"
            type = "int[]"
            default = [1]
            foreign_key = { name = "drop_items", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let items = DataTable::new(
        "items",
        vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
    );
    let valid_default = DataSet::new(vec![
        items.clone(),
        DataTable::new("monsters", vec![BTreeMap::new()]),
    ]);

    assert!(validate_data_set(&linked_meta, &valid_default).is_ok());

    let missing_default_target = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "2".to_owned())])],
        ),
        DataTable::new("monsters", vec![BTreeMap::new()]),
    ]);
    let message = validate_error(validate_data_set(&linked_meta, &missing_default_target));
    assert!(message.contains("foreign key `drop_items` source field `drop_item_ids[0]`"));
    assert!(message.contains("references missing `items.by_id` key `[\"1\"]`"));
}

fn object_foreign_key_meta(required: bool, allow_empty: Option<bool>) -> MetaModel {
    let allow_empty = allow_empty
        .map(|value| format!(", allow_empty = {value}"))
        .unwrap_or_default();
    MetaModel::from_toml_str(&format!(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "game_rules"
            kind = "object"
            source = "game_rules.csv"

            [[tables.fields]]
            name = "starter_item_id"
            type = "int"
            required = {required}
            foreign_key = {{ name = "starter_item", target_table = "items"{allow_empty} }}
        "#
    ))
    .expect("meta should load")
}

fn object_foreign_key_data(starter_item_id: &str) -> DataSet {
    DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "game_rules",
            vec![BTreeMap::from([(
                "starter_item_id".to_owned(),
                starter_item_id.to_owned(),
            )])],
        ),
    ])
}

#[test]
fn validates_object_table_foreign_key_reference_integrity() {
    let meta = object_foreign_key_meta(true, None);
    let linked_meta = meta.link().expect("meta should link");

    assert!(validate_data_set(&linked_meta, &object_foreign_key_data("1")).is_ok());

    let message = validate_error(validate_data_set(
        &linked_meta,
        &object_foreign_key_data("2"),
    ));
    assert!(message.contains(
        "table `game_rules` row 1 foreign key `starter_item` references missing `items.by_id` key"
    ));
}

#[test]
fn validates_object_table_foreign_key_allow_empty() {
    let optional_meta = object_foreign_key_meta(false, None);
    let linked_optional = optional_meta.link().expect("meta should link");
    assert!(validate_data_set(&linked_optional, &object_foreign_key_data("")).is_ok());

    let required_meta = object_foreign_key_meta(false, Some(false));
    let linked_required = required_meta.link().expect("meta should link");
    let message = validate_error(validate_data_set(
        &linked_required,
        &object_foreign_key_data(""),
    ));
    assert!(message.contains("foreign key `starter_item` is empty but `allow_empty = false`"));
}

#[test]
fn handles_foreign_key_empty_and_partial_keys() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "region"
            type = "string"
            required = true

            [[tables.indexes]]
            name = "by_id_region"
            fields = ["id", "region"]
            unique = true

            [[tables]]
            name = "shops"
            source = "shops.csv"

            [[tables.fields]]
            name = "item_id"
            type = "int"
            required = false

            [[tables.fields]]
            name = "region"
            type = "string"
            required = false

            [[tables.foreign_keys]]
            name = "item_region"
            fields = ["item_id", "region"]
            target_table = "items"
            target_index = "by_id_region"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let items = DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "1".to_owned()),
            ("region".to_owned(), "cn".to_owned()),
        ])],
    );

    let empty = DataSet::new(vec![
        items.clone(),
        DataTable::new("shops", vec![BTreeMap::new()]),
    ]);
    assert!(validate_data_set(&linked_meta, &empty).is_ok());

    let partial = DataSet::new(vec![
        items,
        DataTable::new(
            "shops",
            vec![BTreeMap::from([("item_id".to_owned(), "1".to_owned())])],
        ),
    ]);
    let message = validate_error(validate_data_set(&linked_meta, &partial));
    assert!(message.contains("partial empty key"));
}

#[test]
fn target_validation_rejects_non_visible_foreign_key_target_table() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"
            export_when = { any = ["server"] }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            foreign_key = { target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "1".to_owned(),
            )])],
        ),
    ]);

    let message = validate_error(validate_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &data_set,
    ));
    assert!(message.contains("target table `items` which is not exported"));
}

#[test]
fn target_validation_rejects_foreign_key_to_filtered_target_row() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"
            row_export = { excluded_tags_field = "exclude_tags" }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "exclude_tags"
            type = "string"
            export = false

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![
                BTreeMap::from([
                    ("id".to_owned(), "1".to_owned()),
                    ("exclude_tags".to_owned(), String::new()),
                ]),
                BTreeMap::from([
                    ("id".to_owned(), "2".to_owned()),
                    ("exclude_tags".to_owned(), "client".to_owned()),
                ]),
            ],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "2".to_owned(),
            )])],
        ),
    ]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
    let message = validate_error(validate_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &data_set,
    ));
    assert!(message.contains("references missing `items.by_id` key `[\"2\"]`"));
}

#[test]
fn target_validation_skips_foreign_key_when_export_when_does_not_match() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"
            export_when = { any = ["server"] }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            export_when = { any = ["server"] }
            foreign_key = { name = "drop_item", target_table = "items", export_when = { any = ["server"] } }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "1".to_owned(),
            )])],
        ),
    ]);

    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["client"]), &data_set,)
            .is_ok()
    );
}

#[test]
fn target_validation_reports_hidden_foreign_key_source_and_target_fields() {
    let source_hidden = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            export_when = { any = ["server"] }
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked_source_hidden = source_hidden.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "1".to_owned(),
            )])],
        ),
    ]);
    let source_message = validate_error(validate_data_set_for_target(
        &linked_source_hidden,
        &TargetTags::new(["client"]),
        &data_set,
    ));
    assert!(source_message.contains("non-exported source field `drop_item_id`"));

    let target_hidden = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_pk"

            [[tables.fields]]
            name = "pk"
            type = "int"

            [[tables.fields]]
            name = "id"
            type = "int"
            export_when = { any = ["server"] }

            [[tables.indexes]]
            name = "by_pk"
            fields = ["pk"]
            unique = true
            primary = true

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            foreign_key = { name = "drop_item", target_table = "items", target_index = "by_id" }
        "#,
    )
    .expect("meta should load");
    let linked_target_hidden = target_hidden.link().expect("meta should link");
    let target_data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([
                ("pk".to_owned(), "1".to_owned()),
                ("id".to_owned(), "1".to_owned()),
            ])],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "1".to_owned(),
            )])],
        ),
    ]);
    let target_message = validate_error(validate_data_set_for_target(
        &linked_target_hidden,
        &TargetTags::new(["client"]),
        &target_data_set,
    ));
    assert!(target_message.contains("non-exported target index field `items.id`"));
}

#[test]
fn target_fragment_validation_uses_visible_rows() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            polymorphic = true
            row_export = { excluded_tags_field = "exclude_tags" }

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true

            [[tables.fields]]
            name = "exclude_tags"
            type = "string"
            export = false

            [[tables.fragments]]
            name = "weapon"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");

    let data_set = DataSet::new(vec![
        DataTable::new_with_sources(
            "items",
            vec![
                BTreeMap::from([
                    ("id".to_owned(), "1".to_owned()),
                    ("exclude_tags".to_owned(), String::new()),
                    ("weapon.power".to_owned(), "10".to_owned()),
                ]),
                BTreeMap::from([
                    ("id".to_owned(), "2".to_owned()),
                    ("exclude_tags".to_owned(), "client".to_owned()),
                ]),
                BTreeMap::from([
                    ("id".to_owned(), "3".to_owned()),
                    ("exclude_tags".to_owned(), "client".to_owned()),
                    ("weapon.power".to_owned(), "bad".to_owned()),
                ]),
            ],
            vec![
                fragment_row_source(1, &["weapon"]),
                fragment_row_source(2, &["weapon"]),
                fragment_row_source(3, &["weapon"]),
            ],
        )
        .expect("table should build"),
    ]);

    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["client"]), &data_set).is_ok()
    );
    assert!(validate_data_set(&linked_meta, &data_set).is_err());

    let visible_invalid = DataSet::new(vec![
        DataTable::new_with_sources(
            "items",
            vec![BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                ("exclude_tags".to_owned(), String::new()),
            ])],
            vec![fragment_row_source(1, &["weapon"])],
        )
        .expect("table should build"),
    ]);
    let message = validate_error(validate_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &visible_invalid,
    ));
    assert!(message.contains("weapon.power` is required"));
}

#[test]
fn validates_struct_json_cell_values() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "item_id"
            type = "string"
            required = true

            [[structs.fields]]
            name = "count"
            type = "int"
            required = true
            default = 1

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward"
            type = "Reward"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward".to_owned(),
            r#"{"item_id":"coin"}"#.to_owned(),
        )])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn rejects_invalid_struct_json_cell_values() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "item_id"
            type = "string"
            required = true

            [[structs.fields]]
            name = "count"
            type = "int"
            required = true

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward"
            type = "Reward"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let cases = [
        (r#"{"item_id":"coin"}"#, "reward.count` is required"),
        (
            r#"{"item_id":"coin","count":"many"}"#,
            "reward.count` value `many` does not match type",
        ),
        (
            r#"{"item_id":"coin","count":1,"extra":true}"#,
            "unknown struct key `extra`",
        ),
        ("not json", "invalid struct JSON"),
    ];

    for (raw, expected) in cases {
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([("reward".to_owned(), raw.to_owned())])],
        )]);
        let message = validate_error(validate_data_set(&linked_meta, &data_set));
        assert!(
            message.contains(expected),
            "expected `{expected}` from `{raw}`, got {message}"
        );
    }
}

#[test]
fn validates_struct_expanded_value_columns() {
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
            required = true
            default = 100

            [[tables]]
            name = "characters"
            source = "characters.csv"

            [[tables.fields]]
            name = "id"
            type = "string"

            [[tables.fields]]
            name = "attributes"
            type = "Attributes"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([
            ("id".to_owned(), "hero".to_owned()),
            ("attributes.attack".to_owned(), "10".to_owned()),
        ])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn validates_expanded_list_element_paths_in_object_table_structs() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "FixedReward"

            [[structs.fields]]
            name = "gold"
            type = "int"
            required = true

            [[structs.fields]]
            name = "item_ids"
            type = "int[]"
            required = true

            [[tables]]
            name = "game_rules"
            kind = "object"
            source = "game_rules.csv"

            [[tables.fields]]
            name = "starter_reward"
            type = "FixedReward"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "game_rules",
        vec![BTreeMap::from([
            ("starter_reward.gold".to_owned(), "500".to_owned()),
            ("starter_reward.item_ids[0]".to_owned(), "2001".to_owned()),
            ("starter_reward.item_ids[1]".to_owned(), "2002".to_owned()),
        ])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());

    let sparse = DataSet::new(vec![DataTable::new(
        "game_rules",
        vec![BTreeMap::from([
            ("starter_reward.gold".to_owned(), "500".to_owned()),
            ("starter_reward.item_ids[1]".to_owned(), "2002".to_owned()),
        ])],
    )]);
    let error = validate_error(validate_data_set(&linked_meta, &sparse));
    assert!(error.contains("non-contiguous expanded list element path"));
}

#[test]
fn struct_expanded_value_uses_child_defaults_for_effective_existence() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Attributes"

            [[structs.fields]]
            name = "max_health"
            type = "int"
            default = 100

            [[tables]]
            name = "characters"
            source = "characters.csv"

            [[tables.fields]]
            name = "attributes"
            type = "Attributes"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new("characters", vec![BTreeMap::new()])]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn optional_struct_expanded_empty_cells_do_not_create_occurrence() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "item_id"
            type = "string"
            required = true

            [[structs.fields]]
            name = "count"
            type = "int"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward"
            type = "Reward"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let empty_optional = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward.item_id".to_owned(),
            String::new(),
        )])],
    )]);
    assert!(validate_data_set(&linked_meta, &empty_optional).is_ok());

    let present_but_incomplete = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("reward.item_id".to_owned(), String::new()),
            ("reward.count".to_owned(), "1".to_owned()),
        ])],
    )]);
    let error = validate_error(validate_data_set(&linked_meta, &present_but_incomplete));
    assert!(error.contains("reward.item_id` is required"));
}

#[test]
fn rejects_struct_expanded_value_schema_and_mixing_errors() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Attributes"

            [[structs.fields]]
            name = "attack"
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
    let linked_meta = meta.link().expect("meta should link");

    let mixed = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([
            ("id".to_owned(), "hero".to_owned()),
            ("attributes".to_owned(), r#"{"attack":10}"#.to_owned()),
            ("attributes.attack".to_owned(), "10".to_owned()),
        ])],
    )]);
    let error = validate_error(validate_data_set(&linked_meta, &mixed));
    assert!(error.contains("mixes struct JSON cell with expanded child path"));

    let unknown_child = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([
            ("id".to_owned(), "hero".to_owned()),
            ("attributes.power".to_owned(), "10".to_owned()),
        ])],
    )]);
    let error = validate_error(validate_data_set(&linked_meta, &unknown_child));
    assert!(error.contains("unknown struct child `power`"));
}

#[test]
fn validates_recursive_struct_expanded_value_and_json_boundary() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Offense"

            [[structs.fields]]
            name = "attack"
            type = "int"
            required = true

            [[structs]]
            name = "Attributes"

            [[structs.fields]]
            name = "offense"
            type = "Offense"
            required = true

            [[tables]]
            name = "characters"
            source = "characters.csv"

            [[tables.fields]]
            name = "attributes"
            type = "Attributes"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");

    let expanded = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([(
            "attributes.offense.attack".to_owned(),
            "10".to_owned(),
        )])],
    )]);
    assert!(validate_data_set(&linked_meta, &expanded).is_ok());

    let mixed_nested = DataSet::new(vec![DataTable::new(
        "characters",
        vec![BTreeMap::from([
            (
                "attributes.offense".to_owned(),
                r#"{"attack":10}"#.to_owned(),
            ),
            ("attributes.offense.attack".to_owned(), "10".to_owned()),
        ])],
    )]);
    let error = validate_error(validate_data_set(&linked_meta, &mixed_nested));
    assert!(error.contains("attributes.offense` mixes struct JSON cell"));
}

#[test]
fn target_validation_skips_non_exported_struct_children() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "item_id"
            type = "string"
            required = true

            [[structs.fields]]
            name = "server_count"
            type = "int"
            required = true
            export_when = { any = ["server"] }

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward"
            type = "Reward"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward".to_owned(),
            r#"{"item_id":"coin"}"#.to_owned(),
        )])],
    )]);

    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["client"]), &data_set).is_ok()
    );
    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["server"]), &data_set)
            .is_err()
    );
}

#[test]
fn target_validation_ignores_hidden_expanded_descendants_for_occurrence_existence() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "client_item"
            type = "string"
            required = true
            export_when = { any = ["client"] }

            [[structs.fields]]
            name = "server_count"
            type = "int"
            export_when = { any = ["server"] }

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "reward"
            type = "Reward"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "reward.server_count".to_owned(),
            "10".to_owned(),
        )])],
    )]);

    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["client"]), &data_set).is_ok()
    );
    assert!(
        validate_data_set_for_target(&linked_meta, &TargetTags::new(["server"]), &data_set).is_ok()
    );
}

#[test]
fn validates_complete_data_set() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn target_asset_validation_checks_visible_asset_fields() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"
                export_when = { any = ["client"] }

                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                require_existing = true
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "1".to_owned()),
            ("icon".to_owned(), "Missing.png".to_owned()),
        ])],
    )]);
    let assets = AssetsConfig {
        search_paths: vec![AssetSearchPathConfig {
            name: "game".to_owned(),
            root: temp_dir.path().to_path_buf(),
            kind: "filesystem".to_owned(),
            mount: None,
        }],
        ..AssetsConfig::default()
    };

    let report = validation_report_error(
        validate_normalized_data_set_for_target_with_assets_and_report(
            &linked_meta,
            &assets,
            &TargetTags::new(["client"]),
            &data_set,
        ),
    );

    let issue = report
        .issues
        .iter()
        .find(|issue| issue.code == CF_DATA_INVALID_ASSET_REF)
        .expect("asset validation should emit a structured diagnostic");
    assert_eq!(issue.location.table.as_deref(), Some("items"));
    assert_eq!(issue.location.field.as_deref(), Some("icon"));
    assert_eq!(issue.target.as_deref(), Some("client"));
    assert_eq!(issue.current_value.as_deref(), Some("Missing.png"));
    assert_eq!(issue.expected.as_deref(), Some("existing asset file"));
    assert_eq!(issue.domain.as_deref(), Some("data"));
    assert!(
        issue
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("asset file"))
    );
    assert!(issue.message.contains("missing file"));
    validate_normalized_data_set_for_target_with_assets_and_report(
        &linked_meta,
        &assets,
        &TargetTags::new(["server"]),
        &data_set,
    )
    .expect("server target should skip client-only asset field");
}

#[test]
fn target_asset_validation_skips_filtered_rows() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let asset_path = temp_dir.path().join("Icon_Sword.png");
    std::fs::write(&asset_path, []).expect("write asset");
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                row_export = { required_tags_field = "tags" }

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "tags"
                type = "string"

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"

                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                require_existing = true
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![
            BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                ("tags".to_owned(), "client".to_owned()),
                ("icon".to_owned(), "Icon_Sword.png".to_owned()),
            ]),
            BTreeMap::from([
                ("id".to_owned(), "2".to_owned()),
                ("tags".to_owned(), "server".to_owned()),
                ("icon".to_owned(), "Missing.png".to_owned()),
            ]),
        ],
    )]);
    let assets = AssetsConfig {
        search_paths: vec![AssetSearchPathConfig {
            name: "game".to_owned(),
            root: temp_dir.path().to_path_buf(),
            kind: "filesystem".to_owned(),
            mount: None,
        }],
        ..AssetsConfig::default()
    };

    validate_normalized_data_set_for_target_with_assets_and_report(
        &linked_meta,
        &assets,
        &TargetTags::new(["client"]),
        &data_set,
    )
    .expect("client target should skip filtered server row");
}

#[test]
fn rejects_data_set_missing_required_table() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![]);
    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("missing table `items`"));
}

#[test]
fn normalizes_data_set_before_validation() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "max_stack"
                type = "int"
                required = true
                default = 1
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new("items", vec![BTreeMap::new()])]);

    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(validate_normalized_data_set(&linked_meta, &normalized).is_ok());
    assert_eq!(
        normalized.table("items").unwrap().rows[0]
            .get("max_stack")
            .map(String::as_str),
        Some("1")
    );
}

#[test]
fn normalize_data_set_with_pool_preserves_table_order() {
    let data_set = DataSet::new(vec![
        DataTable::new("slow", vec![BTreeMap::new()]),
        DataTable::new("fast", vec![BTreeMap::new()]),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("pool should build");

    let normalized = normalize_data_set_with_normalizer_and_pool(&data_set, &pool, |table| {
        if table.name == "slow" {
            thread::sleep(Duration::from_millis(20));
        }
        Ok(table.clone())
    })
    .expect("parallel normalize should succeed");

    assert_eq!(
        normalized
            .tables()
            .iter()
            .map(|table| table.name.as_str())
            .collect::<Vec<_>>(),
        vec!["slow", "fast"]
    );
}

#[test]
fn normalize_data_set_with_pool_reports_first_table_error_by_plan_order() {
    let data_set = DataSet::new(vec![
        DataTable::new("first", vec![BTreeMap::new()]),
        DataTable::new("second", vec![BTreeMap::new()]),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("pool should build");

    let error = normalize_data_set_with_normalizer_and_pool(&data_set, &pool, |table| {
        if table.name == "first" {
            thread::sleep(Duration::from_millis(20));
            return Err(test_validation_error("first failed", table.name.as_str()));
        }
        Err(test_validation_error("second failed", table.name.as_str()))
    })
    .unwrap_err();

    let ConfigForgeError::ValidationReport(report) = error else {
        panic!("expected structured diagnostic report");
    };
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].message, "first failed");
}

fn test_validation_error(message: &str, table_name: &str) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_VALIDATION_INVALID_CONFIG,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                table: Some(table_name.to_owned()),
                ..DiagnosticLocation::default()
            },
        )
        .with_source("validation.test")
        .with_current_value(table_name)
        .with_expected("successful test normalizer"),
    )
}

#[test]
fn normalize_data_set_with_pool_runs_table_tasks_concurrently() {
    let data_set = DataSet::new(vec![
        DataTable::new("items", vec![BTreeMap::new()]),
        DataTable::new("levels", vec![BTreeMap::new()]),
        DataTable::new("quests", vec![BTreeMap::new()]),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(3)
        .build()
        .expect("pool should build");
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));

    normalize_data_set_with_normalizer_and_pool(&data_set, &pool, |table| {
        let active_now = active.fetch_add(1, Ordering::SeqCst) + 1;
        max_active.fetch_max(active_now, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(20));
        active.fetch_sub(1, Ordering::SeqCst);
        Ok(table.clone())
    })
    .expect("parallel normalize should succeed");

    assert!(max_active.load(Ordering::SeqCst) > 1);
}

#[test]
fn full_data_set_validation_with_pool_runs_table_tasks_concurrently() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "levels"
            source = "levels.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "quests"
            source = "quests.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "levels",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "quests",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(3)
        .build()
        .expect("pool should build");
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));

    super::entry::validate_normalized_data_set_with_table_validator_and_pool(
        &linked_meta,
        &data_set,
        &pool,
        |_, _| {
            let active_now = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_active.fetch_max(active_now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(20));
            active.fetch_sub(1, Ordering::SeqCst);
            ValidationReport::new()
        },
    )
    .expect("parallel full data set validation should succeed");

    assert!(max_active.load(Ordering::SeqCst) > 1);
}

#[test]
fn full_data_set_validation_with_pool_sorts_merged_table_reports_stably() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "z_slow"
            source = "z_slow.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "a_fast"
            source = "a_fast.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "z_slow",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "a_fast",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("pool should build");

    let report = super::entry::validate_normalized_data_set_with_table_validator_and_pool(
        &linked_meta,
        &data_set,
        &pool,
        |table_meta, _| {
            if table_meta.meta.name == "z_slow" {
                thread::sleep(Duration::from_millis(20));
            }
            let mut report = ValidationReport::new();
            report.push(ValidationIssue::new(
                "test.warning",
                ValidationSeverity::Warning,
                "test",
                format!("{} warning", table_meta.meta.name),
                DiagnosticLocation {
                    table: Some(table_meta.meta.name.clone()),
                    ..DiagnosticLocation::default()
                },
            ));
            report
        },
    )
    .expect("warning-only validation should succeed");

    assert_eq!(
        report
            .issues
            .iter()
            .map(|issue| issue.location.table.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("a_fast"), Some("z_slow")]
    );
}

#[test]
fn target_view_validation_with_pool_runs_visible_table_tasks_concurrently() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "client_items"
            source = "client_items.csv"
            export_when = { any = ["client"] }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "client_levels"
            source = "client_levels.csv"
            export_when = { any = ["client"] }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "server_only"
            source = "server_only.csv"
            export_when = { any = ["server"] }

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "client_items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "client_levels",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "server_only",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("pool should build");
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));

    super::entry::validate_normalized_data_set_for_target_with_table_view_validator_and_pool(
        &linked_meta,
        &TargetTags::new(["client"]),
        &data_set,
        &pool,
        |table_view| {
            assert_ne!(table_view.meta.meta.name, "server_only");
            let active_now = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_active.fetch_max(active_now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(20));
            active.fetch_sub(1, Ordering::SeqCst);
            ValidationReport::new()
        },
    )
    .expect("parallel target view validation should succeed");

    assert!(max_active.load(Ordering::SeqCst) > 1);
}

#[test]
fn target_view_validation_with_pool_sorts_merged_reports_stably() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "z_slow"
            source = "z_slow.csv"
            export_when = { any = ["client"] }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables]]
            name = "a_fast"
            source = "a_fast.csv"
            export_when = { any = ["client"] }

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "z_slow",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
        DataTable::new(
            "a_fast",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        ),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("pool should build");

    let report =
        super::entry::validate_normalized_data_set_for_target_with_table_view_validator_and_pool(
            &linked_meta,
            &TargetTags::new(["client"]),
            &data_set,
            &pool,
            |table_view| {
                if table_view.meta.meta.name == "z_slow" {
                    thread::sleep(Duration::from_millis(20));
                }
                let mut report = ValidationReport::new();
                report.push(ValidationIssue::new(
                    "test.warning",
                    ValidationSeverity::Warning,
                    "test",
                    format!("{} warning", table_view.meta.meta.name),
                    DiagnosticLocation {
                        table: Some(table_view.meta.meta.name.clone()),
                        ..DiagnosticLocation::default()
                    },
                ));
                report
            },
        )
        .expect("warning-only target validation should succeed");

    assert_eq!(
        report
            .issues
            .iter()
            .map(|issue| issue.location.table.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("a_fast"), Some("z_slow")]
    );
}

#[test]
fn target_validation_outcome_matches_report_backed_error_gate() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"
            row_export = { excluded_tags_field = "exclude_tags" }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "exclude_tags"
            type = "string"
            export = false

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![
        DataTable::new(
            "items",
            vec![
                BTreeMap::from([
                    ("id".to_owned(), "1".to_owned()),
                    ("exclude_tags".to_owned(), String::new()),
                ]),
                BTreeMap::from([
                    ("id".to_owned(), "2".to_owned()),
                    ("exclude_tags".to_owned(), "client".to_owned()),
                ]),
            ],
        ),
        DataTable::new(
            "monsters",
            vec![BTreeMap::from([(
                "drop_item_id".to_owned(),
                "2".to_owned(),
            )])],
        ),
    ]);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("pool should build");
    let target_tags = TargetTags::new(["client"]);

    let legacy_report = validation_report_error(
        validate_normalized_data_set_for_target_with_assets_report_and_pool(
            &linked_meta,
            &AssetsConfig::default(),
            &target_tags,
            &data_set,
            &pool,
        ),
    );
    let outcome = validate_normalized_data_set_for_target_with_assets_outcome_and_pool(
        &linked_meta,
        &AssetsConfig::default(),
        &target_tags,
        &data_set,
        &pool,
    )
    .expect("outcome should preserve validation report");

    assert!(outcome.is_blocking());
    assert_eq!(outcome.report().issues, legacy_report.issues);
    assert_eq!(
        outcome
            .report()
            .issues
            .iter()
            .map(|issue| (&issue.code, issue.location.table.as_deref()))
            .collect::<Vec<_>>(),
        legacy_report
            .issues
            .iter()
            .map(|issue| (&issue.code, issue.location.table.as_deref()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn object_table_validation_uses_single_logical_row_and_field_rules() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"
            required = true
            default = 100

            [[tables.fields]]
            name = "server_note"
            type = "string"
            required = true
            export_when = { any = ["server"] }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new("game_config", vec![BTreeMap::new()])]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(
        validate_normalized_data_set_for_target(
            &linked_meta,
            &TargetTags::new(["client"]),
            &normalized
        )
        .is_ok()
    );
    assert!(validate_normalized_data_set(&linked_meta, &normalized).is_err());
    assert_eq!(
        normalized.table("game_config").unwrap().rows[0]
            .get("max_level")
            .map(String::as_str),
        Some("100")
    );
}

#[test]
fn object_table_validation_rejects_non_singleton_rows() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let table_meta = linked_meta.table("game_config").expect("linked table");
    let table = DataTable::new(
        "game_config",
        vec![
            BTreeMap::from([("max_level".to_owned(), "100".to_owned())]),
            BTreeMap::from([("max_level".to_owned(), "101".to_owned())]),
        ],
    );

    let message = validate_error(validate_table(table_meta, &table));

    assert!(message.contains("must normalize to exactly one logical row"));
}

#[test]
fn object_table_only_header_required_without_default_fails() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new("game_config", vec![BTreeMap::new()])]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("missing field `max_level`"));

    let report = validation_report_error(validate_data_set(&linked_meta, &data_set));
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.code == CF_DATA_MISSING_REQUIRED_FIELD)
        .expect("missing required field diagnostic");
    assert_eq!(issue.location.table.as_deref(), Some("game_config"));
    assert_eq!(issue.location.field.as_deref(), Some("max_level"));
    assert_eq!(
        issue.expected.as_deref(),
        Some("non-empty required field value or configured default")
    );
}

#[test]
fn object_table_field_type_enum_and_struct_errors_still_apply() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[enums]]
            name = "Difficulty"

            [[enums.values]]
            name = "normal"

            [[structs]]
            name = "Reward"

            [[structs.fields]]
            name = "count"
            type = "int"

            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"

            [[tables.fields]]
            name = "difficulty"
            type = "Difficulty"

            [[tables.fields]]
            name = "daily_reward"
            type = "Reward"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");

    let cases = [
        (
            BTreeMap::from([
                ("max_level".to_owned(), "bad".to_owned()),
                ("difficulty".to_owned(), "normal".to_owned()),
                ("daily_reward".to_owned(), r#"{"count":1}"#.to_owned()),
            ]),
            "field `max_level` value `bad` does not match type",
        ),
        (
            BTreeMap::from([
                ("max_level".to_owned(), "60".to_owned()),
                ("difficulty".to_owned(), "nightmare".to_owned()),
                ("daily_reward".to_owned(), r#"{"count":1}"#.to_owned()),
            ]),
            "field `difficulty` value `nightmare` does not match type",
        ),
        (
            BTreeMap::from([
                ("max_level".to_owned(), "60".to_owned()),
                ("difficulty".to_owned(), "normal".to_owned()),
                ("daily_reward".to_owned(), "not json".to_owned()),
            ]),
            "field `daily_reward` has invalid struct JSON",
        ),
    ];

    for (row, expected_message) in cases {
        let data_set = DataSet::new(vec![DataTable::new("game_config", vec![row])]);
        let message = validate_error(validate_data_set(&linked_meta, &data_set));
        assert!(
            message.contains(expected_message),
            "expected `{expected_message}` in `{message}`"
        );
    }
}

#[test]
fn object_table_validation_uses_override_cell_source_location() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            sources = [{ provider = "csv", path = "gameplay.csv" }]
            override_sources = [{ provider = "csv", path = "game_s1.csv" }]

            [[tables.fields]]
            name = "max_level"
            type = "int"
            required = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let table_meta = linked_meta.table("game_config").expect("linked table");
    let table = DataTable::new_with_sources_and_cells(
        "game_config",
        vec![BTreeMap::from([(
            "max_level".to_owned(),
            "oops".to_owned(),
        )])],
        vec![RowSourceInfo {
            source_index: 0,
            source_path: PathBuf::from("gameplay.csv"),
            source_row_number: 2,
            fragments: Vec::new(),
            source_role: SourceRole::Base,
            replaces: None,
        }],
        vec![BTreeMap::from([(
            "max_level".to_owned(),
            CellSourceInfo {
                source_path: PathBuf::from("game_s1.csv"),
                source_row_number: 4,
                source_column_number: 3,
                source_column_name: "value".to_owned(),
                column_index: 2,
            },
        )])],
    )
    .expect("object table should build");

    let report = validation_report_error(validate_table(table_meta, &table));
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.code == CF_DATA_INVALID_TYPE && issue.validator == "validation.type")
        .expect("invalid type issue should be reported");
    assert_eq!(
        issue.location.source_path,
        Some(PathBuf::from("game_s1.csv"))
    );
    assert_eq!(issue.location.source_row_number, Some(4));
    assert_eq!(issue.location.source_column_number, Some(3));
    assert_eq!(issue.location.source_column_name.as_deref(), Some("value"));
}

#[test]
fn object_table_field_validator_still_applies() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 100
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "game_config",
        vec![BTreeMap::from([("max_level".to_owned(), "101".to_owned())])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("exceeds max `100`"));
}

#[test]
fn target_validation_skips_non_exported_tables_fields_and_rows() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                primary_index = "by_id"
                row_export = { excluded_tags_field = "exclude_tags" }

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "server_value"
                type = "int"
                required = true
                export_when = { any = ["server"] }

                [[tables.fields]]
                name = "exclude_tags"
                type = "string"
                export = false

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true
                primary = true

                [[tables]]
                name = "server_only"
                source = "server_only.csv"
                export_when = { any = ["server"] }

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
            "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![
            BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                ("exclude_tags".to_owned(), String::new()),
            ]),
            BTreeMap::from([
                ("id".to_owned(), "bad".to_owned()),
                ("exclude_tags".to_owned(), "client".to_owned()),
            ]),
        ],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(
        validate_normalized_data_set_for_target(
            &linked_meta,
            &TargetTags::new(["client"]),
            &normalized
        )
        .is_ok()
    );
    assert!(validate_normalized_data_set(&linked_meta, &normalized).is_err());
}

#[test]
fn range_validator_accepts_values_inside_bounds() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "5".to_owned())])],
    )]);

    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn range_validator_rejects_values_outside_bounds() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "11".to_owned())])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("exceeds max `10`"));
}

#[test]
fn validation_report_preserves_diagnostic_order_for_multiple_range_issues() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            name = "level_range"
            min = 1
            max = 10

            [[tables.fields]]
            name = "weight"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            name = "weight_range"
            min = 1
            max = 100
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![
            BTreeMap::from([
                ("level".to_owned(), "11".to_owned()),
                ("weight".to_owned(), "101".to_owned()),
            ]),
            BTreeMap::from([
                ("level".to_owned(), "0".to_owned()),
                ("weight".to_owned(), "0".to_owned()),
            ]),
        ],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");
    let registry = ValidatorRegistry::with_builtin_validators();
    let plan =
        CompiledValidationPlan::compile(&linked_meta, &registry).expect("plan should compile");
    let context = ValidationContext::new(
        &linked_meta,
        None,
        &normalized,
        ExportValidationView::full(&linked_meta, &normalized),
    );

    let report = plan.run(&context);

    assert_eq!(
        report
            .issues
            .iter()
            .map(|issue| {
                (
                    issue.validator.as_str(),
                    issue.location.row_index,
                    issue.message.as_str(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                "level_range",
                Some(0),
                "table `items` row 1 field `level` value exceeds max `10`",
            ),
            (
                "level_range",
                Some(1),
                "table `items` row 2 field `level` value is below min `1`",
            ),
            (
                "weight_range",
                Some(0),
                "table `items` row 1 field `weight` value exceeds max `100`",
            ),
            (
                "weight_range",
                Some(1),
                "table `items` row 2 field `weight` value is below min `1`",
            ),
        ]
    );
    assert_eq!(report.error_messages().len(), 4);
}

#[test]
fn validation_report_collects_multiple_basic_errors_before_failing() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
            unique = true

            [[tables.fields]]
            name = "level"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![
            BTreeMap::from([
                ("id".to_owned(), "same".to_owned()),
                ("level".to_owned(), "bad".to_owned()),
            ]),
            BTreeMap::from([
                ("id".to_owned(), "same".to_owned()),
                ("level".to_owned(), "also_bad".to_owned()),
            ]),
        ],
    )]);

    let report = validation_report_error(validate_data_set(&linked_meta, &data_set));
    let messages = report.error_messages().join("; ");

    assert!(messages.contains("field `id`"));
    assert!(messages.contains("duplicates unique value"));
    assert!(messages.contains("field `level`"));
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == CF_DATA_DUPLICATE_KEY
                && issue.current_value.as_deref() == Some("same")
                && issue.expected.as_deref() == Some("unique field/index key value"))
    );
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == CF_DATA_INVALID_TYPE
                && issue.current_value.as_deref() == Some("bad")
                && issue.expected.as_deref() == Some("int"))
    );
    assert!(report.error_messages().len() >= 3);
}

#[test]
fn validation_report_keeps_warnings_but_error_gate_blocks_result() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            name = "recommended_level"
            severity = "warning"
            min = 1
            max = 10

            [[tables.fields]]
            name = "weight"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            name = "weight_limit"
            min = 1
            max = 100
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("level".to_owned(), "11".to_owned()),
            ("weight".to_owned(), "101".to_owned()),
        ])],
    )]);

    let report = validation_report_error(validate_data_set(&linked_meta, &data_set));

    assert!(report.has_error());
    assert_eq!(report.warning_messages().len(), 1);
    assert_eq!(report.error_messages().len(), 1);
    assert!(report.warning_messages()[0].contains("recommended_level"));
    assert!(report.error_messages()[0].contains("weight_limit"));
}

#[test]
fn range_validator_rejects_unknown_parameter() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            minn = 1
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "5".to_owned())])],
    )]);

    let issue = expect_validator_config_issue(
        validate_data_set(&linked_meta, &data_set),
        "validator.range",
    );

    assert!(issue.message.contains("unknown parameter `minn`"));
    assert_eq!(issue.current_value.as_deref(), Some("minn"));
    assert!(issue.expected.as_deref().unwrap().contains("min"));
}

#[test]
fn range_validator_respects_target_field_visibility() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"
            export_when = { any = ["server"] }

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "100".to_owned())])],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(
        validate_normalized_data_set_for_target(
            &linked_meta,
            &TargetTags::new(["client"]),
            &normalized
        )
        .is_ok()
    );
}

#[test]
fn range_validator_rejects_empty_value_when_disallowed() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            allow_empty = false
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), String::new())])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("must not be empty"));
}

#[test]
fn vector_validator_checks_components_magnitude_and_custom_names() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "spawns"
            source = "spawns.csv"

            [[tables.fields]]
            name = "position"
            type = "vector3"
            vector_format = "tuple"
            vector_components = ["right", "up", "forward"]

            [[tables.fields.validators]]
            kind = "vector"
            name = "position_bounds"
            components = ["up"]
            min = -1
            max = 1
            magnitude_max = 4.9
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "spawns",
        vec![BTreeMap::from([(
            "position".to_owned(),
            "3,4,0".to_owned(),
        )])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("component `up` exceeds max `1`"));
}

#[test]
fn vector_validator_checks_magnitude_when_components_pass() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "spawns"
            source = "spawns.csv"

            [[tables.fields]]
            name = "position"
            type = "vector3"
            vector_format = "tuple"

            [[tables.fields.validators]]
            kind = "vector"
            max = 10
            magnitude_max = 4.9
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "spawns",
        vec![BTreeMap::from([(
            "position".to_owned(),
            "3,4,0".to_owned(),
        )])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("magnitude `5` exceeds max `4.9`"));
}

#[test]
fn color_validator_checks_alpha_channel_bounds_and_metadata() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "themes"
            source = "themes.csv"

            [[tables.fields]]
            name = "tint"
            type = "color"
            color_format = "rgba_tuple"
            color_channel = "normalized"
            color_space = "srgb"

            [[tables.fields.validators]]
            kind = "color"
            channels = ["r", "g", "b"]
            min = 0.0
            max = 1.0
            alpha_min = 0.5
            channel = "normalized"
            space = "srgb"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "themes",
        vec![BTreeMap::from([(
            "tint".to_owned(),
            "0.2,0.3,0.4,0.1".to_owned(),
        )])],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("alpha is below min `0.5`"));
}

#[test]
fn color_and_vector_validators_reject_unknown_or_invalid_config() {
    let vector_meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "spawns"
            source = "spawns.csv"

            [[tables.fields]]
            name = "position"
            type = "vector2"
            vector_format = "tuple"

            [[tables.fields.validators]]
            kind = "vector"
            components = ["missing"]
            max = 1
        "#,
    )
    .expect("meta should load");
    let linked_meta = vector_meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "spawns",
        vec![BTreeMap::from([("position".to_owned(), "0,0".to_owned())])],
    )]);
    let issue = expect_validator_config_issue(
        validate_data_set(&linked_meta, &data_set),
        "validator.vector",
    );
    assert!(issue.message.contains("unknown component `missing`"));

    let color_meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "themes"
            source = "themes.csv"

            [[tables.fields]]
            name = "tint"
            type = "color"
            color_format = "hex_rgba"
            color_channel = "byte"

            [[tables.fields.validators]]
            kind = "color"
            alpha_min = 128
            channel = "normalized"
        "#,
    )
    .expect("meta should load");
    let linked_meta = color_meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "themes",
        vec![BTreeMap::from([(
            "tint".to_owned(),
            "#FFFFFFFF".to_owned(),
        )])],
    )]);
    let issue = expect_validator_config_issue(
        validate_data_set(&linked_meta, &data_set),
        "validator.color",
    );
    assert!(issue.message.contains("expects color_channel `normalized`"));
}

#[test]
fn range_validator_does_not_panic_on_fractional_int_when_run_directly() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "1.5".to_owned())])],
    )]);
    let registry = ValidatorRegistry::with_builtin_validators();
    let plan =
        CompiledValidationPlan::compile(&linked_meta, &registry).expect("plan should compile");
    let context = ValidationContext::new(
        &linked_meta,
        None,
        &data_set,
        ExportValidationView::full(&linked_meta, &data_set),
    );

    let report = plan.run(&context);

    assert!(report.issues.is_empty());
}

#[test]
fn warning_range_validator_does_not_fail_validation() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            name = "recommended_level"
            severity = "warning"
            min = 1
            max = 10
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "11".to_owned())])],
    )]);
    let registry = ValidatorRegistry::with_builtin_validators();
    let plan =
        CompiledValidationPlan::compile(&linked_meta, &registry).expect("plan should compile");
    let context = ValidationContext::new(
        &linked_meta,
        None,
        &data_set,
        ExportValidationView::full(&linked_meta, &data_set),
    );

    let report = plan.run(&context);

    assert!(!report.has_error());
    assert_eq!(report.issues[0].severity, ValidationSeverity::Warning);
    assert_eq!(report.issues[0].validator, "recommended_level");
    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn range_validator_export_when_filters_validator_itself() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "range"
            min = 1
            max = 10
            export_when = { any = ["server"] }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "100".to_owned())])],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(
        validate_normalized_data_set_for_target(
            &linked_meta,
            &TargetTags::new(["client"]),
            &normalized
        )
        .is_ok()
    );

    let message = validate_error(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["server"]),
        &normalized,
    ));
    assert!(message.contains("exceeds max `10`"));
}

#[test]
fn range_validator_supports_exclusive_bounds_and_float_values() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "ratio"
            type = "float"

            [[tables.fields.validators]]
            kind = "range"
            min = 0.5
            max = 1.5
            exclusive_min = true
            exclusive_max = true
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("ratio".to_owned(), "1.0".to_owned())])],
    )]);
    let min_edge = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("ratio".to_owned(), "0.5".to_owned())])],
    )]);
    let max_edge = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("ratio".to_owned(), "1.5".to_owned())])],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    assert!(validate_error(validate_data_set(&linked_meta, &min_edge)).contains("below min `0.5`"));
    assert!(
        validate_error(validate_data_set(&linked_meta, &max_edge)).contains("exceeds max `1.5`")
    );
}

#[test]
fn rejects_non_finite_float_values() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "ratio"
            type = "float"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");

    for value in ["NaN", "inf", "-inf"] {
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([("ratio".to_owned(), value.to_owned())])],
        )]);

        let message = validate_error(validate_data_set(&linked_meta, &data_set));
        assert!(message.contains("does not match type"));
    }

    let scientific = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("ratio".to_owned(), "1e-3".to_owned())])],
    )]);
    assert!(validate_data_set(&linked_meta, &scientific).is_ok());
}

#[test]
fn range_validator_declared_on_table_reports_scope_error() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.validators]]
            kind = "range"
            min = 1

            [[tables.fields]]
            name = "level"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "5".to_owned())])],
    )]);

    let issue = expect_validator_config_issue(
        validate_data_set(&linked_meta, &data_set),
        "validator.range",
    );

    assert!(
        issue
            .message
            .contains("cannot be declared at table `items`")
    );
    assert_eq!(issue.current_value.as_deref(), Some("table scope"));
    assert_eq!(issue.expected.as_deref(), Some("field scope"));
}

#[test]
fn regex_validator_accepts_full_match_and_rejects_mismatch() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "key"
            type = "string"

            [[tables.fields.validators]]
            kind = "regex"
            pattern = "[a-z][a-z0-9_]*"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "key".to_owned(),
            "iron_sword".to_owned(),
        )])],
    )]);
    let invalid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "key".to_owned(),
            "Iron Sword".to_owned(),
        )])],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    let message = validate_error(validate_data_set(&linked_meta, &invalid));
    assert!(message.contains("does not match regex"));
}

#[test]
fn regex_validator_supports_search_case_insensitive_and_empty_policy() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "path"
            type = "asset"
            asset_kind = "unreal_asset"

            [[tables.fields.validators]]
            kind = "regex"
            pattern = "skills"
            match_mode = "search"
            case_sensitive = false
            allow_empty = false

            [[asset_kinds]]
            name = "unreal_asset"
            input = "path"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "path".to_owned(),
            "/Game/Skills/GA_Slash.GA_Slash_C".to_owned(),
        )])],
    )]);
    let invalid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("path".to_owned(), String::new())])],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    let message = validate_error(validate_data_set(&linked_meta, &invalid));
    assert!(message.contains("does not match regex"));
}

#[test]
fn regex_validator_rejects_invalid_pattern_and_unsupported_field_type() {
    let invalid_pattern = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "key"
            type = "string"

            [[tables.fields.validators]]
            kind = "regex"
            pattern = "["
        "#,
    )
    .expect("meta should load");
    let linked_meta = invalid_pattern.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("key".to_owned(), "x".to_owned())])],
    )]);
    let issue = expect_validator_config_issue(
        validate_data_set(&linked_meta, &data_set),
        "validator.regex",
    );
    assert!(issue.message.contains("parameter `pattern` is invalid"));
    assert_eq!(issue.current_value.as_deref(), Some("["));
    assert_eq!(issue.expected.as_deref(), Some("valid Rust regex pattern"));

    let unsupported_type = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "regex"
            pattern = "^[0-9]+$"
        "#,
    )
    .expect("meta should load");
    let linked_meta = unsupported_type.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "1".to_owned())])],
    )]);
    let issue = expect_validator_config_issue(
        validate_data_set(&linked_meta, &data_set),
        "validator.regex",
    );
    assert!(
        issue
            .message
            .contains("can only be used on string, text, asset or enum fields")
    );
    assert_eq!(issue.current_value.as_deref(), Some("level"));
    assert_eq!(
        issue.expected.as_deref(),
        Some("string, text, asset, or enum field")
    );
}

#[test]
fn string_length_validator_accepts_and_rejects_unicode_scalar_lengths() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "name"
            type = "text"

            [[tables.fields.validators]]
            kind = "string_length"
            min = 2
            max = 4
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("name".to_owned(), "长剑".to_owned())])],
    )]);
    let invalid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([(
            "name".to_owned(),
            "超长名字配置".to_owned(),
        )])],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    let message = validate_error(validate_data_set(&linked_meta, &invalid));
    assert!(message.contains("exceeds max `4`"));
}

#[test]
fn string_length_validator_supports_utf8_bytes_exact_trim_and_empty_policy() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "code"
            type = "string"

            [[tables.fields.validators]]
            kind = "string_length"
            exact = 3
            unit = "utf8_bytes"
            trim_before_count = true
            allow_empty = false
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("code".to_owned(), " abc ".to_owned())])],
    )]);
    let invalid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("code".to_owned(), "中文".to_owned())])],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    let message = validate_error(validate_data_set(&linked_meta, &invalid));
    assert!(message.contains("expected exactly `3`"));
}

#[test]
fn string_length_validator_rejects_ambiguous_config_and_unsupported_type() {
    let ambiguous = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "name"
            type = "string"

            [[tables.fields.validators]]
            kind = "string_length"
            exact = 3
            max = 4
        "#,
    )
    .expect("meta should load");
    let linked_meta = ambiguous.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("name".to_owned(), "abc".to_owned())])],
    )]);
    let issue = expect_validator_config_issue(
        validate_data_set(&linked_meta, &data_set),
        "validator.string_length",
    );
    assert!(issue.message.contains("cannot be used with `min` or `max`"));
    assert_eq!(issue.current_value.as_deref(), Some("exact with min/max"));
    assert_eq!(
        issue.expected.as_deref(),
        Some("exact alone, or min/max without exact")
    );

    let unsupported_type = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields.validators]]
            kind = "string_length"
            max = 4
        "#,
    )
    .expect("meta should load");
    let linked_meta = unsupported_type.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([("level".to_owned(), "1".to_owned())])],
    )]);
    let issue = expect_validator_config_issue(
        validate_data_set(&linked_meta, &data_set),
        "validator.string_length",
    );
    assert!(
        issue
            .message
            .contains("can only be used on string, text or asset fields")
    );
    assert_eq!(issue.current_value.as_deref(), Some("level"));
    assert_eq!(
        issue.expected.as_deref(),
        Some("string, text, or asset field")
    );
}

#[test]
fn required_when_validator_requires_field_when_leaf_condition_matches() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.validators]]
            kind = "required_when"
            field = "skill_id"
            when = { field = "kind", equals = "weapon" }

            [[tables.fields]]
            name = "kind"
            type = "string"
            export = false

            [[tables.fields]]
            name = "skill_id"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![
            BTreeMap::from([
                ("kind".to_owned(), "weapon".to_owned()),
                ("skill_id".to_owned(), String::new()),
            ]),
            BTreeMap::from([
                ("kind".to_owned(), "material".to_owned()),
                ("skill_id".to_owned(), String::new()),
            ]),
        ],
    )]);

    let message = validate_error(validate_data_set(&linked_meta, &data_set));
    assert!(message.contains("required by required_when"));
}

#[test]
fn required_when_validator_supports_composite_conditions() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.validators]]
            kind = "required_when"
            field = "price"
            when = { all = [ { field = "kind", equals = "shop_item" }, { field = "currency", not_empty = true } ] }

            [[tables.fields]]
            name = "kind"
            type = "string"

            [[tables.fields]]
            name = "currency"
            type = "string"

            [[tables.fields]]
            name = "price"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let invalid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("kind".to_owned(), "shop_item".to_owned()),
            ("currency".to_owned(), "gold".to_owned()),
            ("price".to_owned(), String::new()),
        ])],
    )]);
    let valid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("kind".to_owned(), "shop_item".to_owned()),
            ("currency".to_owned(), "".to_owned()),
            ("price".to_owned(), String::new()),
        ])],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    let message = validate_error(validate_data_set(&linked_meta, &invalid));
    assert!(message.contains("required by required_when"));
}

#[test]
fn required_when_validator_reports_hidden_target_field_when_active() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.validators]]
            kind = "required_when"
            field = "skill_id"
            when = { field = "kind", equals = "weapon" }

            [[tables.fields]]
            name = "kind"
            type = "string"

            [[tables.fields]]
            name = "skill_id"
            type = "int"
            export_when = { any = ["server"] }
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("kind".to_owned(), "weapon".to_owned()),
            ("skill_id".to_owned(), "1".to_owned()),
        ])],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    let message = validate_error(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &normalized,
    ));
    assert!(message.contains("hidden for current target"));
}

#[test]
fn compare_validator_checks_numeric_field_order() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.validators]]
            kind = "compare"
            left = "min_level"
            op = "<="
            right = "max_level"

            [[tables.fields]]
            name = "min_level"
            type = "int"

            [[tables.fields]]
            name = "max_level"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("min_level".to_owned(), "1".to_owned()),
            ("max_level".to_owned(), "10".to_owned()),
        ])],
    )]);
    let invalid = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("min_level".to_owned(), "11".to_owned()),
            ("max_level".to_owned(), "10".to_owned()),
        ])],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    let message = validate_error(validate_data_set(&linked_meta, &invalid));
    assert!(message.contains("compare expected"));
}

#[test]
fn compare_validator_handles_empty_operands_and_type_errors() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.validators]]
            kind = "compare"
            left = "start"
            op = "<"
            right = "end"
            skip_if_either_empty = false

            [[tables.fields]]
            name = "start"
            type = "string"

            [[tables.fields]]
            name = "end"
            type = "string"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("start".to_owned(), "".to_owned()),
            ("end".to_owned(), "z".to_owned()),
        ])],
    )]);
    let message = validate_error(validate_data_set(&linked_meta, &data_set));
    assert!(message.contains("empty operand"));

    let incompatible = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.validators]]
            kind = "compare"
            left = "a"
            op = "=="
            right = "b"

            [[tables.fields]]
            name = "a"
            type = "string"

            [[tables.fields]]
            name = "b"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = incompatible.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("a".to_owned(), "1".to_owned()),
            ("b".to_owned(), "1".to_owned()),
        ])],
    )]);
    let issue = expect_validator_config_issue(
        validate_data_set(&linked_meta, &data_set),
        "validator.compare",
    );
    assert!(
        issue
            .message
            .contains("requires compatible comparable field types")
    );
    assert_eq!(
        issue.expected.as_deref(),
        Some("matching comparable scalar field types")
    );
}

#[test]
fn table_group_helper_groups_visible_rows_with_hidden_support_field() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "drops"
            source = "drops.csv"
            row_export = { excluded_tags_field = "exclude_tags" }

            [[tables.fields]]
            name = "pool"
            type = "string"
            export = false

            [[tables.fields]]
            name = "weight"
            type = "int"

            [[tables.fields]]
            name = "exclude_tags"
            type = "string"
            export = false
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "drops",
        vec![
            BTreeMap::from([
                ("pool".to_owned(), "common".to_owned()),
                ("weight".to_owned(), "10".to_owned()),
                ("exclude_tags".to_owned(), String::new()),
            ]),
            BTreeMap::from([
                ("pool".to_owned(), "rare".to_owned()),
                ("weight".to_owned(), "1".to_owned()),
                ("exclude_tags".to_owned(), "client".to_owned()),
            ]),
        ],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");
    let view =
        ExportValidationView::for_target(&linked_meta, &normalized, &TargetTags::new(["client"]))
            .expect("target view");
    let table_view = view.table("drops").expect("drops table view");
    let validator_meta = crate::meta::ValidatorMeta {
        kind: "sum".to_owned(),
        name: None,
        desc: None,
        severity: ValidationSeverity::Error,
        message: None,
        export_when: ExportCondition::default(),
        params: BTreeMap::from([(
            "group_by".to_owned(),
            toml::Value::Array(vec![toml::Value::String("pool".to_owned())]),
        )]),
    };
    let group_by = super::builtins::table_common::GroupByConfig::compile(
        &validator_meta,
        table_view.meta,
        "sum",
    )
    .expect("group_by compiles");
    let grouped =
        super::builtins::table_common::GroupedRows::from_table_view(table_view, &group_by);
    let groups = grouped
        .iter()
        .map(|(key, rows)| (key.label(), rows.len()))
        .collect::<Vec<_>>();

    assert_eq!(groups, vec![("pool=common".to_owned(), 1)]);
}

#[test]
fn table_order_helper_stably_sorts_rows_by_scalar_field() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "levels"
            source = "levels.csv"

            [[tables.fields]]
            name = "chapter"
            type = "string"

            [[tables.fields]]
            name = "level"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "levels",
        vec![
            BTreeMap::from([
                ("chapter".to_owned(), "main".to_owned()),
                ("level".to_owned(), "2".to_owned()),
            ]),
            BTreeMap::from([
                ("chapter".to_owned(), "main".to_owned()),
                ("level".to_owned(), "1".to_owned()),
            ]),
            BTreeMap::from([
                ("chapter".to_owned(), "main".to_owned()),
                ("level".to_owned(), "1".to_owned()),
            ]),
        ],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");
    let view = ExportValidationView::full(&linked_meta, &normalized);
    let table_view = view.table("levels").expect("levels table view");
    let table_meta = linked_meta.table("levels").expect("levels table meta");
    let order_by = super::builtins::table_common::OrderByConfig::compile(
        table_meta,
        "monotonic",
        "level".to_owned(),
    )
    .expect("order_by compiles");
    let group_by = super::builtins::table_common::GroupByConfig::compile(
        &crate::meta::ValidatorMeta {
            kind: "monotonic".to_owned(),
            name: None,
            desc: None,
            severity: ValidationSeverity::Error,
            message: None,
            export_when: ExportCondition::default(),
            params: BTreeMap::new(),
        },
        table_view.meta,
        "monotonic",
    )
    .expect("group_by compiles");
    let grouped =
        super::builtins::table_common::GroupedRows::from_table_view(table_view, &group_by);
    let (_, rows) = grouped.iter().next().expect("whole table group");
    let ordered = order_by.sort_rows(table_view, rows);
    let ordered_indexes = ordered.iter().map(|row| row.row_index).collect::<Vec<_>>();

    assert_eq!(order_by.field(), "level");
    assert_eq!(ordered_indexes, vec![1, 2, 0]);
}

#[test]
fn sum_validator_checks_grouped_int_totals() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "drops"
            source = "drops.csv"

            [[tables.validators]]
            kind = "sum"
            field = "weight"
            group_by = ["pool"]
            equals = 100

            [[tables.fields]]
            name = "pool"
            type = "string"
            export = false

            [[tables.fields]]
            name = "weight"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "drops",
        vec![
            BTreeMap::from([
                ("pool".to_owned(), "common".to_owned()),
                ("weight".to_owned(), "40".to_owned()),
            ]),
            BTreeMap::from([
                ("pool".to_owned(), "common".to_owned()),
                ("weight".to_owned(), "60".to_owned()),
            ]),
        ],
    )]);
    let invalid = DataSet::new(vec![DataTable::new(
        "drops",
        vec![
            BTreeMap::from([
                ("pool".to_owned(), "common".to_owned()),
                ("weight".to_owned(), "40".to_owned()),
            ]),
            BTreeMap::from([
                ("pool".to_owned(), "common".to_owned()),
                ("weight".to_owned(), "59".to_owned()),
            ]),
        ],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    let message = validate_error(validate_data_set(&linked_meta, &invalid));
    assert!(message.contains("validator.sum_mismatch"));
    assert!(message.contains("group `pool=common`"));
}

#[test]
fn sum_validator_supports_float_epsilon_and_target_row_filter() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "rates"
            source = "rates.csv"
            row_export = { excluded_tags_field = "exclude_tags" }

            [[tables.validators]]
            kind = "sum"
            field = "rate"
            min = 0.99
            max = 1.01
            epsilon = 0.001

            [[tables.fields]]
            name = "rate"
            type = "float"

            [[tables.fields]]
            name = "exclude_tags"
            type = "string"
            export = false
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "rates",
        vec![
            BTreeMap::from([
                ("rate".to_owned(), "0.4".to_owned()),
                ("exclude_tags".to_owned(), String::new()),
            ]),
            BTreeMap::from([
                ("rate".to_owned(), "0.59".to_owned()),
                ("exclude_tags".to_owned(), String::new()),
            ]),
            BTreeMap::from([
                ("rate".to_owned(), "100".to_owned()),
                ("exclude_tags".to_owned(), "client".to_owned()),
            ]),
        ],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(
        validate_normalized_data_set_for_target(
            &linked_meta,
            &TargetTags::new(["client"]),
            &normalized,
        )
        .is_ok()
    );
    let message = validate_error(validate_normalized_data_set(&linked_meta, &normalized));
    assert!(message.contains("validator.sum_too_large"));
}

#[test]
fn sum_validator_handles_skip_empty_and_config_errors() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "drops"
            source = "drops.csv"

            [[tables.validators]]
            kind = "sum"
            field = "weight"
            equals = 10
            skip_empty = true

            [[tables.fields]]
            name = "weight"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "drops",
        vec![
            BTreeMap::from([("weight".to_owned(), "10".to_owned())]),
            BTreeMap::from([("weight".to_owned(), String::new())]),
        ],
    )]);
    assert!(validate_data_set(&linked_meta, &data_set).is_ok());

    let invalid_config = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "drops"
            source = "drops.csv"

            [[tables.validators]]
            kind = "sum"
            field = "weight"
            equals = 10
            min = 1

            [[tables.fields]]
            name = "weight"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = invalid_config.link().expect("meta should link");
    let issue =
        expect_validator_config_issue(validate_data_set(&linked_meta, &data_set), "validator.sum");
    assert!(issue.message.contains("cannot be combined"));
    assert_eq!(issue.current_value.as_deref(), Some("equals with min/max"));
    assert_eq!(
        issue.expected.as_deref(),
        Some("equals alone, or min/max without equals")
    );
}

#[test]
fn monotonic_validator_checks_increasing_strict_sequence() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "levels"
            source = "levels.csv"

            [[tables.validators]]
            kind = "monotonic"
            field = "exp"
            order_by = "level"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields]]
            name = "exp"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "levels",
        vec![
            BTreeMap::from([
                ("level".to_owned(), "1".to_owned()),
                ("exp".to_owned(), "100".to_owned()),
            ]),
            BTreeMap::from([
                ("level".to_owned(), "2".to_owned()),
                ("exp".to_owned(), "200".to_owned()),
            ]),
        ],
    )]);
    let invalid = DataSet::new(vec![DataTable::new(
        "levels",
        vec![
            BTreeMap::from([
                ("level".to_owned(), "1".to_owned()),
                ("exp".to_owned(), "200".to_owned()),
            ]),
            BTreeMap::from([
                ("level".to_owned(), "2".to_owned()),
                ("exp".to_owned(), "100".to_owned()),
            ]),
        ],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
    let message = validate_error(validate_data_set(&linked_meta, &invalid));
    assert!(message.contains("validator.monotonic_violation"));
}

#[test]
fn monotonic_validator_supports_grouped_decreasing_non_strict() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "curves"
            source = "curves.csv"

            [[tables.validators]]
            kind = "monotonic"
            field = "value"
            group_by = ["curve"]
            order_by = "step"
            direction = "decreasing"
            strict = false

            [[tables.fields]]
            name = "curve"
            type = "string"
            export = false

            [[tables.fields]]
            name = "step"
            type = "int"

            [[tables.fields]]
            name = "value"
            type = "float"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let valid = DataSet::new(vec![DataTable::new(
        "curves",
        vec![
            BTreeMap::from([
                ("curve".to_owned(), "a".to_owned()),
                ("step".to_owned(), "1".to_owned()),
                ("value".to_owned(), "3.0".to_owned()),
            ]),
            BTreeMap::from([
                ("curve".to_owned(), "a".to_owned()),
                ("step".to_owned(), "2".to_owned()),
                ("value".to_owned(), "3.0".to_owned()),
            ]),
            BTreeMap::from([
                ("curve".to_owned(), "b".to_owned()),
                ("step".to_owned(), "1".to_owned()),
                ("value".to_owned(), "1.0".to_owned()),
            ]),
        ],
    )]);

    assert!(validate_data_set(&linked_meta, &valid).is_ok());
}

#[test]
fn monotonic_validator_reports_duplicate_order_key_unless_skipped() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "levels"
            source = "levels.csv"

            [[tables.validators]]
            kind = "monotonic"
            field = "exp"
            order_by = "level"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields]]
            name = "exp"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "levels",
        vec![
            BTreeMap::from([
                ("level".to_owned(), "1".to_owned()),
                ("exp".to_owned(), "100".to_owned()),
            ]),
            BTreeMap::from([
                ("level".to_owned(), "1".to_owned()),
                ("exp".to_owned(), "200".to_owned()),
            ]),
        ],
    )]);
    let message = validate_error(validate_data_set(&linked_meta, &data_set));
    assert!(message.contains("validator.monotonic_duplicate_order_key"));

    let skip_meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "levels"
            source = "levels.csv"

            [[tables.validators]]
            kind = "monotonic"
            field = "exp"
            order_by = "level"
            skip_equal_order_key = true

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields]]
            name = "exp"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked_meta = skip_meta.link().expect("meta should link");
    assert!(validate_data_set(&linked_meta, &data_set).is_ok());
}

#[test]
fn monotonic_validator_respects_target_row_filter() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "levels"
            source = "levels.csv"
            row_export = { excluded_tags_field = "exclude_tags" }

            [[tables.validators]]
            kind = "monotonic"
            field = "exp"
            order_by = "level"

            [[tables.fields]]
            name = "level"
            type = "int"

            [[tables.fields]]
            name = "exp"
            type = "int"

            [[tables.fields]]
            name = "exclude_tags"
            type = "string"
            export = false
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "levels",
        vec![
            BTreeMap::from([
                ("level".to_owned(), "1".to_owned()),
                ("exp".to_owned(), "100".to_owned()),
                ("exclude_tags".to_owned(), String::new()),
            ]),
            BTreeMap::from([
                ("level".to_owned(), "2".to_owned()),
                ("exp".to_owned(), "50".to_owned()),
                ("exclude_tags".to_owned(), "client".to_owned()),
            ]),
            BTreeMap::from([
                ("level".to_owned(), "3".to_owned()),
                ("exp".to_owned(), "200".to_owned()),
                ("exclude_tags".to_owned(), String::new()),
            ]),
        ],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(
        validate_normalized_data_set_for_target(
            &linked_meta,
            &TargetTags::new(["client"]),
            &normalized,
        )
        .is_ok()
    );
    let message = validate_error(validate_normalized_data_set(&linked_meta, &normalized));
    assert!(message.contains("validator.monotonic_violation"));
}
