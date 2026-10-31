use super::*;
use crate::{
    Diagnostic, DiagnosticLocation, DiagnosticReport, DiagnosticSeverity,
    meta::{MetaModel, TargetTags, ValidationSeverity},
    validation::{ValidationIssue, ValidationReport},
};
use std::path::PathBuf;

#[test]
fn report_visibility_paths_and_related_context_are_structured() {
    let meta = MetaModel::from_toml_str(
        r#"
        [[tables]]
        name = "items"
        source = "items.csv"
        [[tables.fields]]
        name = "value"
        type = "int"
        export = false
    "#,
    )
    .unwrap();
    let linked = meta.link().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut options = options();
    options.project_path = Some(temp.path().join("ConfigForge.toml"));
    let mut source = issue("custom.code", ValidationSeverity::Error, "items", 3);
    source.location.source_path = Some(temp.path().join("items.csv"));
    source.related_locations.push(source.location.clone());
    source.record_key = Some("id=7".to_owned());
    source
        .context
        .insert("group".to_owned(), "pool=common".to_owned());
    let mut validation = ValidationReport::new();
    validation.push(source);
    let report = DesignerValidationReport::from_validation_report_with_schema(
        &validation,
        &linked,
        options.clone(),
    );
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.issues[0].export_visibility, "field_filtered");
    assert_eq!(report.summary.filtered_context_count, 1);
    assert_eq!(report.issues[0].source_path.as_deref(), Some("items.csv"));
    assert_eq!(
        report.issues[0].related_locations[0].source_path.as_deref(),
        Some(std::path::Path::new("items.csv"))
    );
    for format in [
        DesignerReportFormat::Html,
        DesignerReportFormat::Markdown,
        DesignerReportFormat::Csv,
        DesignerReportFormat::Json,
    ] {
        let rendered = render_designer_validation_report(&report, format).unwrap();
        assert!(rendered.contains("id=7"));
        assert!(rendered.contains("pool=common"));
        assert!(rendered.contains("items.csv"));
    }
    let report = DesignerValidationReport::from_validation_report(&validation, options);
    assert_eq!(report.issues[0].export_visibility, "not_applicable");
    assert_eq!(report.summary.filtered_context_count, 0);
}

fn options() -> DesignerReportOptions {
    DesignerReportOptions {
        title: None,
        project_name: Some("demo".to_owned()),
        project_path: Some(PathBuf::from("ConfigForge.toml")),
        target: Some("client".to_owned()),
        target_tags: TargetTags::new(["client".to_owned(), "shipping".to_owned()]),
        generated_at: "2026-06-07T00:00:00Z".to_owned(),
        input_root: Some(PathBuf::from("Data")),
    }
}

#[test]
fn visibility_resolves_nested_fields_and_does_not_guess_unknown_paths() {
    let meta = MetaModel::from_toml_str(
        r#"
        [[structs]]
        name = "Stats"
        [[structs.fields]]
        name = "secret"
        type = "int"
        export = false
        [[tables]]
        name = "items"
        source = "items.csv"
        [[tables.fields]]
        name = "stats"
        type = "Stats"
    "#,
    )
    .unwrap();
    let linked = meta.link().unwrap();
    for (path, expected) in [
        ("stats.secret", "field_filtered"),
        ("stats.missing", "not_applicable"),
        ("missing", "not_applicable"),
        ("stats", "visible"),
    ] {
        let mut validation = ValidationReport::new();
        let mut source = issue("custom.code", ValidationSeverity::Error, "items", 3);
        source.location.field = Some(path.to_owned());
        validation.push(source);
        let report = DesignerValidationReport::from_validation_report_with_schema(
            &validation,
            &linked,
            options(),
        );
        assert_eq!(report.issues[0].export_visibility, expected, "{path}");
    }
}

fn issue(code: &str, severity: ValidationSeverity, table: &str, row: usize) -> ValidationIssue {
    ValidationIssue::new(
        code,
        severity,
        "validator",
        format!("message for {code}"),
        DiagnosticLocation {
            source_path: Some(PathBuf::from(format!("{table}.csv"))),
            table: Some(table.to_owned()),
            field: Some("value".to_owned()),
            row_index: Some(row - 2),
            source_row_number: Some(row),
            ..DiagnosticLocation::default()
        },
    )
}

#[test]
fn validation_report_enrichment_uses_issue_fields() {
    let mut validation = ValidationReport::new();
    validation.push(
        issue("custom.code", ValidationSeverity::Error, "items", 2)
            .with_hint("fix the value")
            .with_current_value("abc")
            .with_expected("integer")
            .with_explanation("value must parse as an integer")
            .with_domain("items")
            .with_owner("systems"),
    );

    let report = DesignerValidationReport::from_validation_report(&validation, options());

    let issue = &report.issues[0];
    assert_eq!(issue.current_value.as_deref(), Some("abc"));
    assert_eq!(issue.expected.as_deref(), Some("integer"));
    assert_eq!(
        issue.explanation.as_deref(),
        Some("value must parse as an integer")
    );
    assert_eq!(issue.hint.as_deref(), Some("fix the value"));
    assert_eq!(issue.domain, "items");
    assert_eq!(issue.owner, "systems");
}

#[test]
fn diagnostic_report_enrichment_survives_report_conversion() {
    let diagnostic = Diagnostic::new(
        "custom.code",
        DiagnosticSeverity::Error,
        "invalid value",
        DiagnosticLocation::default(),
    )
    .with_source("validation.custom")
    .with_current_value("bad")
    .with_expected("good")
    .with_explanation("custom explanation")
    .with_domain("custom-domain")
    .with_owner("custom-owner");
    let diagnostics = DiagnosticReport {
        diagnostics: vec![diagnostic],
    };

    let report = DesignerValidationReport::from_diagnostic_report(&diagnostics, options());

    let issue = &report.issues[0];
    assert_eq!(issue.validator, "validation.custom");
    assert_eq!(issue.current_value.as_deref(), Some("bad"));
    assert_eq!(issue.expected.as_deref(), Some("good"));
    assert_eq!(issue.explanation.as_deref(), Some("custom explanation"));
    assert_eq!(issue.domain, "custom-domain");
    assert_eq!(issue.owner, "custom-owner");
}

#[test]
fn builds_empty_report() {
    let report =
        DesignerValidationReport::from_validation_report(&ValidationReport::new(), options());

    assert_eq!(report.summary.error_count, 0);
    assert_eq!(report.summary.warning_count, 0);
    assert_eq!(report.status_label(), "PASS");
    assert!(report.issues.is_empty());
}

#[test]
fn assigns_stable_ids_after_deterministic_sort() {
    let mut validation = ValidationReport::new();
    validation.push(issue(
        "validator.regex_mismatch",
        ValidationSeverity::Warning,
        "z",
        3,
    ));
    validation.push(issue(
        "validator.sum_mismatch",
        ValidationSeverity::Error,
        "a",
        4,
    ));
    validation.push(issue(
        "validator.compare_failed",
        ValidationSeverity::Error,
        "a",
        3,
    ));

    let report = DesignerValidationReport::from_validation_report(&validation, options());

    assert_eq!(report.summary.error_count, 2);
    assert_eq!(report.summary.warning_count, 1);
    assert_eq!(report.issues[0].id, "E0001");
    assert_eq!(report.issues[0].code, "validator.compare_failed");
    assert_eq!(report.issues[1].id, "E0002");
    assert_eq!(report.issues[2].id, "W0001");
}

#[test]
fn uses_catalog_and_issue_hint_priority() {
    let mut validation = ValidationReport::new();
    let mut known = issue(
        "validator.regex_mismatch",
        ValidationSeverity::Error,
        "items",
        2,
    );
    known.hint = Some("custom hint".to_owned());
    validation.push(known);
    validation.push(issue(
        "unknown.code",
        ValidationSeverity::Warning,
        "items",
        3,
    ));

    let report = DesignerValidationReport::from_validation_report(&validation, options());

    assert_eq!(report.issues[0].friendly_title, "文本格式不符合命名规则");
    assert_eq!(report.issues[0].hint.as_deref(), Some("custom hint"));
    assert_eq!(report.issues[1].friendly_title, "message for unknown.code");
    assert_eq!(report.issues[1].hint, None);
}

#[test]
fn renders_all_formats_with_escaping() {
    let mut validation = ValidationReport::new();
    let mut item = issue(
        "validator.regex_mismatch",
        ValidationSeverity::Error,
        "items",
        2,
    );
    item.message = "bad | <value>\nnext".to_owned();
    validation.push(item);
    let report = DesignerValidationReport::from_validation_report(&validation, options());

    let json = render_designer_validation_report(&report, DesignerReportFormat::Json).unwrap();
    assert!(json.contains("\"metadata\""));
    assert!(json.contains("validator.regex_mismatch"));

    let markdown =
        render_designer_validation_report(&report, DesignerReportFormat::Markdown).unwrap();
    assert!(markdown.contains("bad \\| <value><br>next"));

    let csv = render_designer_validation_report(&report, DesignerReportFormat::Csv).unwrap();
    assert!(csv.starts_with("id,severity,blocking,code"));
    assert!(csv.contains('"'));

    let html = render_designer_validation_report(&report, DesignerReportFormat::Html).unwrap();
    assert!(html.contains("&lt;value&gt;"));
    assert!(!html.contains("https://"));
}

#[test]
fn renderers_include_structured_enrichment_fields() {
    let mut validation = ValidationReport::new();
    validation.push(
        issue("custom.code", ValidationSeverity::Error, "items", 2)
            .with_current_value("abc")
            .with_expected("integer"),
    );
    validation.issues[0].location.source_column_name = Some("level".to_owned());
    validation.issues[0].explanation = Some("parse as integer".to_owned());
    let report = DesignerValidationReport::from_validation_report(&validation, options());

    let markdown =
        render_designer_validation_report(&report, DesignerReportFormat::Markdown).unwrap();
    assert!(markdown.contains("Source column"));
    assert!(markdown.contains("abc"));
    assert!(markdown.contains("integer"));

    let csv = render_designer_validation_report(&report, DesignerReportFormat::Csv).unwrap();
    assert!(csv.starts_with(
            "id,severity,blocking,code,friendly_title,message,hint,table,field,row_index,source_row_number,source_column"
        ));
    assert!(csv.contains(",level,"));
    assert!(csv.contains(",abc,integer,,parse as integer,"));

    let html = render_designer_validation_report(&report, DesignerReportFormat::Html).unwrap();
    assert!(html.contains("abc"));
    assert!(html.contains("integer"));
    assert!(html.contains("column level"));
}

#[test]
fn validation_report_with_schema_includes_builtin_metadata() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "themes"
            source = "themes.csv"

            [[tables.fields]]
            name = "tint"
            type = "color"
            color_format = "hex_rgba"
            color_channel = "byte"
            color_space = "srgb"

            [[tables.fields]]
            name = "position"
            type = "vector3"
            vector_format = "tuple"
            vector_components = ["right", "up", "forward"]
            unit = "meter"
        "#,
    )
    .expect("meta should load");
    let linked_meta = meta.link().expect("meta should link");
    let mut validation = ValidationReport::new();
    let mut color_issue = issue("custom.color", ValidationSeverity::Error, "themes", 2);
    color_issue.location.field = Some("tint".to_owned());
    validation.push(color_issue);
    let mut vector_issue = issue("custom.vector", ValidationSeverity::Error, "themes", 2);
    vector_issue.location.field = Some("position".to_owned());
    validation.push(vector_issue);

    let report = DesignerValidationReport::from_validation_report_with_schema(
        &validation,
        &linked_meta,
        options(),
    );

    let color_issue = report
        .issues
        .iter()
        .find(|issue| issue.field.as_deref() == Some("tint"))
        .expect("color issue");
    let vector_issue = report
        .issues
        .iter()
        .find(|issue| issue.field.as_deref() == Some("position"))
        .expect("vector issue");

    assert!(
        color_issue
            .schema_metadata
            .contains(&"type=color(format=hex_rgba)".to_owned())
    );
    assert!(
        color_issue
            .schema_metadata
            .contains(&"channel=byte".to_owned())
    );
    assert!(
        vector_issue
            .schema_metadata
            .contains(&"components=right/up/forward".to_owned())
    );
    assert!(
        vector_issue
            .schema_metadata
            .contains(&"unit=meter".to_owned())
    );

    let markdown =
        render_designer_validation_report(&report, DesignerReportFormat::Markdown).unwrap();
    assert!(markdown.contains("Schema"));
    assert!(markdown.contains("channel=byte"));
    let csv = render_designer_validation_report(&report, DesignerReportFormat::Csv).unwrap();
    assert!(csv.contains("schema_metadata"));
    assert!(csv.contains("components=right/up/forward"));
    let html = render_designer_validation_report(&report, DesignerReportFormat::Html).unwrap();
    assert!(html.contains("Schema"));
    assert!(html.contains("unit=meter"));
}
