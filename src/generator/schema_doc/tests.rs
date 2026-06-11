// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    ArtifactProjectInfo,
    artifact::{ArtifactWriteMode, ArtifactWriter},
    code_artifact::tests::expect_single_diagnostic,
    diagnostic::code::{
        CF_ARTIFACT_PATH_COLLISION, CF_GENERATOR_INVALID_OPTION, CF_GENERATOR_UNSUPPORTED_OPTION,
    },
    meta::{MetaModel, TargetTags},
};

use super::{
    SchemaDoc, SchemaDocExportConfig, SchemaDocFormat, SchemaDocOptions,
    format::{
        render_schema_doc_artifacts, schema_doc_export_config_from_options,
        schema_doc_output_paths, write_schema_doc_to_sink,
    },
    html::{escape_html, render_schema_doc_index_html, render_schema_table_html},
    model::{SchemaDocMetadata, SchemaTableDoc},
    render_schema_doc_index_markdown, render_schema_table_markdown,
};

fn build_doc(input: &str, tags: &[&str]) -> SchemaDoc {
    let meta = MetaModel::from_toml_str(input).expect("meta should parse");
    let linked = meta.link().expect("meta should link");
    SchemaDoc::from_meta(
        &linked,
        &ArtifactProjectInfo {
            name: Some("demo".to_owned()),
            desc: None,
            copyright: None,
        },
        SchemaDocOptions {
            title: None,
            project_name: None,
            project_path: None,
            target: Some("client".to_owned()),
            target_tags: TargetTags::new(tags.iter().copied()),
            generated_at: "2026-06-07T00:00:00Z".to_owned(),
            include_hidden: false,
            include_internal: false,
            include_struct_details: true,
            include_asset_rules: true,
            include_i18n: true,
        },
    )
}

#[test]
fn builds_schema_doc_with_fields_indexes_relations_and_enrichment() {
    let doc = build_doc(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                extensions = [".png", ".webp"]
                name_prefixes = ["UI_"]

                [[tables]]
                name = "classes"
                source = "classes.csv"
                desc = "职业表"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables]]
                name = "items"
                source = "items.csv"
                desc = "道具|表"
                row_export = { required_tags_field = "tags" }

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
                desc = "道具 ID"

                [[tables.fields]]
                name = "name"
                type = "text"
                required = true
                export_when = { any = ["client"] }

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"
                export_when = { any = ["client"] }

                [[tables.fields]]
                name = "tags"
                type = "string"

                [[tables.fields]]
                name = "class_id"
                type = "int"
                foreign_key = { target_table = "classes", target_index = "primary", linked_field = "class" }

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables.fields.validators]]
                kind = "range"
                min = 1
                max = 999
            "#,
        &["client"],
    );

    assert_eq!(doc.tables.len(), 2);
    let items = doc
        .tables
        .iter()
        .find(|table| table.name == "items")
        .unwrap();
    assert_eq!(items.fields.len(), 5);
    assert_eq!(items.indexes[0].name, "primary");
    assert_eq!(items.relations[0].target, "classes.id");
    assert_eq!(
        items.asset_fields[0].rules.as_deref(),
        Some("extensions .png/.webp, prefix UI_")
    );
    assert_eq!(items.text_fields[0].field, "name");
    assert!(
        items
            .fields
            .iter()
            .any(|field| !field.validators.is_empty())
    );

    let markdown = render_schema_table_markdown(&doc, items);
    let index_markdown = render_schema_doc_index_markdown(&doc);
    assert!(index_markdown.contains("道具\\|表"));
    assert!(markdown.contains("## Assets"));
    assert!(markdown.contains("## Text / POT"));
    assert!(markdown.contains("range(max=999, min=1)"));
}

#[test]
fn filters_by_target_and_keeps_docs_only_artifact_paths_deterministic() {
    let doc = build_doc(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "server_note"
                type = "string"
                export_when = { any = ["server"] }

                [[tables.fields]]
                name = "internal"
                type = "string"
                export = false
            "#,
        &["client"],
    );
    let items = &doc.tables[0];
    assert_eq!(items.fields.len(), 1);
    assert_eq!(items.fields[0].path, "id");

    let config = SchemaDocExportConfig {
        format: SchemaDocFormat::Markdown,
        output: PathBuf::from("out/docs"),
        index_file: "index.md".to_owned(),
        title: None,
    };
    let paths = schema_doc_output_paths(&doc, &config).expect("paths should plan");
    assert_eq!(paths.index_path, PathBuf::from("out/docs/index.md"));
    assert_eq!(paths.table_paths, vec![PathBuf::from("out/docs/items.md")]);
    assert_eq!(paths.table_file_names, vec!["items.md"]);
}

#[test]
fn parses_html_schema_doc_format_and_uses_html_defaults() {
    let html_config = schema_doc_export_config_from_options(
        PathBuf::from("out/docs"),
        BTreeMap::from([("format".to_owned(), toml::Value::String("html".to_owned()))]),
    )
    .expect("html config should parse");
    assert_eq!(html_config.format, SchemaDocFormat::Html);
    assert_eq!(html_config.index_file, "index.html");

    let markdown_config =
        schema_doc_export_config_from_options(PathBuf::from("out/docs"), BTreeMap::new())
            .expect("default markdown config should parse");
    assert_eq!(markdown_config.format, SchemaDocFormat::Markdown);
    assert_eq!(markdown_config.index_file, "index.md");

    let doc = build_doc(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        &["client"],
    );
    let paths = schema_doc_output_paths(&doc, &html_config).expect("html paths should plan");
    assert_eq!(paths.index_path, PathBuf::from("out/docs/index.html"));
    assert_eq!(
        paths.table_paths,
        vec![PathBuf::from("out/docs/items.html")]
    );
    assert_eq!(paths.table_file_names, vec!["items.html"]);
}

#[test]
fn renders_markdown_artifact_list_from_shared_output_plan() {
    let doc = build_doc(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        &["client"],
    );
    let config = SchemaDocExportConfig {
        format: SchemaDocFormat::Markdown,
        output: PathBuf::from("out/docs"),
        index_file: "index.md".to_owned(),
        title: None,
    };

    let artifacts = render_schema_doc_artifacts(&doc, &config).expect("artifacts should render");
    let paths = artifacts
        .iter()
        .map(|artifact| artifact.path.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            PathBuf::from("out/docs/index.md"),
            PathBuf::from("out/docs/items.md")
        ]
    );
    assert!(artifacts[0].content.contains("[items](items.md)"));
    assert!(artifacts[1].content.contains("# items"));
}

#[test]
fn renders_static_html_shell_and_escapes_user_text() {
    assert_eq!(escape_html("&<>\"'"), "&amp;&lt;&gt;&quot;&#39;".to_owned());

    let doc = build_doc(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                desc = "<script>alert('x')</script>"

                [[tables.fields]]
                name = "id"
                type = "int"
                desc = "field <script>bad()</script>"
            "#,
        &["client"],
    );
    let table = &doc.tables[0];
    let index_html = render_schema_doc_index_html(&doc, &["items.html".to_owned()]);
    let table_html = render_schema_table_html(&doc, table, "index.html");

    assert!(index_html.contains("<!doctype html>"));
    assert!(index_html.contains("<html lang=\"zh-CN\">"));
    assert!(index_html.contains("<meta charset=\"utf-8\">"));
    assert!(index_html.contains("<style>"));
    assert!(index_html.contains("<title>demo Schema</title>"));
    assert!(index_html.contains("items.html"));

    assert!(!table_html.contains("<script>"));
    assert!(table_html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"));
    assert!(table_html.contains("field &lt;script&gt;bad()&lt;/script&gt;"));

    let config = SchemaDocExportConfig {
        format: SchemaDocFormat::Html,
        output: PathBuf::from("out/docs"),
        index_file: "index.html".to_owned(),
        title: None,
    };
    let artifacts = render_schema_doc_artifacts(&doc, &config).expect("html artifacts render");
    assert_eq!(artifacts[0].path, PathBuf::from("out/docs/index.html"));
    assert_eq!(artifacts[1].path, PathBuf::from("out/docs/items.html"));
    assert!(artifacts[0].content.contains("<!doctype html>"));
}

#[test]
fn rejects_invalid_schema_doc_options_and_duplicate_paths() {
    let error = schema_doc_export_config_from_options(
        PathBuf::from("out"),
        BTreeMap::from([("format".to_owned(), toml::Value::String("pdf".to_owned()))]),
    )
    .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_OPTION);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.schema_doc.options")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("pdf"));
    assert_eq!(diagnostic.expected.as_deref(), Some("markdown or html"));

    let error = schema_doc_export_config_from_options(
        PathBuf::from("out"),
        BTreeMap::from([
            ("format".to_owned(), toml::Value::String("html".to_owned())),
            (
                "index_file".to_owned(),
                toml::Value::String("index.md".to_owned()),
            ),
        ]),
    )
    .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(diagnostic.current_value.as_deref(), Some("index.md"));
    assert_eq!(
        diagnostic.expected.as_deref(),
        Some("plain .html file name")
    );

    let doc = SchemaDoc {
        metadata: SchemaDocMetadata {
            title: "Schema".to_owned(),
            project_name: None,
            project_path: None,
            target: None,
            target_tags: Vec::new(),
            generated_at: "now".to_owned(),
            config_forge_version: "test".to_owned(),
        },
        tables: vec![SchemaTableDoc {
            name: "index".to_owned(),
            kind: "CollectionTable".to_owned(),
            description: None,
            source_summary: None,
            visibility: "all".to_owned(),
            row_export_summary: "no".to_owned(),
            primary_index: None,
            fields: Vec::new(),
            indexes: Vec::new(),
            relations: Vec::new(),
            asset_fields: Vec::new(),
            text_fields: Vec::new(),
            validators: Vec::new(),
        }],
    };
    let config = SchemaDocExportConfig {
        format: SchemaDocFormat::Markdown,
        output: PathBuf::from("out"),
        index_file: "index.md".to_owned(),
        title: None,
    };
    let error = schema_doc_output_paths(&doc, &config).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_ARTIFACT_PATH_COLLISION);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.schema_doc.output")
    );
    assert!(
        diagnostic
            .current_value
            .is_some_and(|value| value.contains("index.md"))
    );
}

#[test]
fn writes_markdown_artifacts_to_sink() {
    let doc = build_doc(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        &["client"],
    );
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let config = SchemaDocExportConfig {
        format: SchemaDocFormat::Markdown,
        output: temp_dir.path().join("docs"),
        index_file: "index.md".to_owned(),
        title: None,
    };
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    write_schema_doc_to_sink(&doc, &config, &sink).expect("write docs");
    sink.finish().expect("finish sink");
    assert!(temp_dir.path().join("docs/index.md").exists());
    assert!(temp_dir.path().join("docs/items.md").exists());
}
