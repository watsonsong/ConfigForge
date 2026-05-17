use std::collections::BTreeMap;

use crate::meta::{
    EnumMeta, EnumValueMeta, ExportCondition, FieldMeta, LinkedField, LinkedFieldMeta,
    LinkedStructType, LinkedTableMeta, MetaModel, PrimitiveFieldType, TableMeta, TargetTags,
    ValidationSeverity,
};
use crate::{ConfigForgeError, Result};

use super::*;

fn validate_error(result: Result<()>) -> String {
    match result.unwrap_err() {
        ConfigForgeError::Validation(message) => message,
        error => panic!("expected validation error, got {error}"),
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
        source: Some("items.csv".to_owned()),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "id".to_owned(),
            desc: None,
            field_type: "int".to_owned(),
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
        }],
        indexes: vec![],
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
    );
    let table = DataTable {
        name: "items".to_owned(),
        rows: vec![BTreeMap::from([("id".to_owned(), "abc".to_owned())])],
    };

    assert!(validate_table(&linked_meta, &table).is_err());
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
        source: Some("items.csv".to_owned()),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "max_stack".to_owned(),
            desc: None,
            field_type: "int".to_owned(),
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
        }],
        indexes: vec![],
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
    );
    let table = DataTable {
        name: "items".to_owned(),
        rows: vec![BTreeMap::new()],
    };

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
        source: Some("items.csv".to_owned()),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "rarity".to_owned(),
            desc: None,
            field_type: "ItemRarity".to_owned(),
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
        }],
        indexes: vec![],
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
    );
    let valid_table = DataTable {
        name: "items".to_owned(),
        rows: vec![BTreeMap::from([("rarity".to_owned(), "common".to_owned())])],
    };
    let invalid_table = DataTable {
        name: "items".to_owned(),
        rows: vec![BTreeMap::from([(
            "rarity".to_owned(),
            "legendary".to_owned(),
        )])],
    };

    assert!(validate_table(&linked_meta, &valid_table).is_ok());
    assert!(validate_table(&linked_meta, &invalid_table).is_err());
}

#[test]
fn rejects_struct_field_even_when_empty() {
    let struct_meta = crate::meta::StructMeta {
        name: "Reward".to_owned(),
        desc: None,
        fields: Vec::new(),
    };
    let meta = TableMeta {
        name: "items".to_owned(),
        kind: crate::meta::TableKind::Collection,
        desc: None,
        export: true,
        export_when: ExportCondition::default(),
        row_export: None,
        validators: vec![],
        source: Some("items.csv".to_owned()),
        sources: vec![],
        override_sources: vec![],
        fields: vec![FieldMeta {
            name: "reward".to_owned(),
            desc: None,
            field_type: "Reward".to_owned(),
            required: false,
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
        }],
        indexes: vec![],
        primary_index: None,
        preserve_source_order: false,
        polymorphic: false,
    };
    let linked_meta = LinkedTableMeta::new(
        &meta,
        vec![LinkedFieldMeta {
            meta: &meta.fields[0],
            linked_field: LinkedField::Struct(LinkedStructType {
                meta: &struct_meta,
                fields: vec![],
            }),
        }],
        vec![],
    );
    let table = DataTable {
        name: "items".to_owned(),
        rows: vec![BTreeMap::from([("reward".to_owned(), String::new())])],
    };
    let error = validate_table(&linked_meta, &table).unwrap_err();

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("struct value validation is not implemented yet"))
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
    let error = validate_data_set(&linked_meta, &data_set).unwrap_err();

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("missing table `items`"))
    );
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
                ("id".to_owned(), "1".to_owned()),
                ("exclude_tags".to_owned(), "client".to_owned()),
            ]),
        ],
    )]);
    let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

    assert!(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &normalized
    )
    .is_ok());
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

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("unknown parameter `minn`"));
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

    assert!(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &normalized
    )
    .is_ok());
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

    assert!(validate_normalized_data_set_for_target(
        &linked_meta,
        &TargetTags::new(["client"]),
        &normalized
    )
    .is_ok());

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

    let message = validate_error(validate_data_set(&linked_meta, &data_set));

    assert!(message.contains("cannot be declared at table `items`"));
}
