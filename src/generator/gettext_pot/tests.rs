// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::PathBuf};

use tempfile::tempdir;

use crate::{
    artifact::{ArtifactWriteMode, ArtifactWriter},
    code_artifact::tests::expect_single_diagnostic,
    data::{DataSet, DataTable},
    data_artifact::TargetDataConfig,
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    meta::{MetaModel, TargetTags},
};

use super::{render::deduplicate_entries, render::escape_po, *};

#[test]
fn escapes_po_strings() {
    assert_eq!(escape_po("a\\b\"c\n"), "a\\\\b\\\"c\\n");
}

#[test]
fn deduplicates_by_context_and_msgid() {
    let entries = vec![
        TextExtractionEntry {
            msgid: "Iron Ore".to_owned(),
            msgctxt: Some("items.name".to_owned()),
            table_name: "items".to_owned(),
            field_path: "name".to_owned(),
            record_key: Some("id=1".to_owned()),
            source_reference: Some("items.csv:2".to_owned()),
            translator_comments: vec!["Name".to_owned()],
        },
        TextExtractionEntry {
            msgid: "Iron Ore".to_owned(),
            msgctxt: Some("items.name".to_owned()),
            table_name: "items".to_owned(),
            field_path: "name".to_owned(),
            record_key: Some("id=2".to_owned()),
            source_reference: Some("items.csv:3".to_owned()),
            translator_comments: vec!["Name".to_owned(), "Second".to_owned()],
        },
    ];

    let deduped = deduplicate_entries(&entries);

    assert_eq!(deduped.len(), 1);
    assert_eq!(deduped[0].translator_comments, vec!["Name", "Second"]);
    assert_eq!(
        deduped[0].source_reference.as_deref(),
        Some("items.csv:2 items.csv:3")
    );
}

#[test]
fn export_to_sink_writes_pot_content_directly() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "name"
                type = "text"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let data_set = DataSet::new(vec![DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "2001".to_owned()),
            ("name".to_owned(), "Iron Ore".to_owned()),
        ])],
    )]);
    let task = TargetDataConfig {
        name: Some("pot".to_owned()),
        output: PathBuf::from("out"),
        format: FORMAT.to_owned(),
        options: BTreeMap::new(),
    };
    let temp_dir = tempdir().expect("temp dir");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();

    GettextPotExporter
        .export_to_sink(
            temp_dir.path(),
            &task,
            &linked,
            &TargetTags::empty(),
            &data_set,
            &sink,
        )
        .expect("POT should export to sink");
    sink.finish().expect("sink should finish");

    let content = fs::read_to_string(temp_dir.path().join("messages.pot"))
        .expect("POT artifact should be written");
    assert!(content.contains("msgctxt \"items.name\""));
    assert!(content.contains("msgid \"Iron Ore\""));
}

#[test]
fn generate_to_sink_reports_structured_contract_error() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"

                [[tables.fields]]
                name = "name"
                type = "text"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let input = crate::generator::GeneratorCommonInput {
        project: crate::ArtifactProjectInfo::default(),
        project_root: PathBuf::from("."),
        target_name: "client".to_owned(),
        target_tags: TargetTags::empty(),
        meta: &linked,
        output: PathBuf::from("out"),
        type_id: FORMAT.to_owned(),
        task_name: "pot".to_owned(),
        options: BTreeMap::new(),
        generated_at: "now".to_owned(),
    };
    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

    let input = crate::generator::GeneratorTaskInput {
        common: input,
        prepared: crate::generator::PreparedGeneratorInputs::MetaOnly,
    };
    let error =
        <GettextPotExporter as ArtifactGenerator>::generate(&GettextPotExporter, &input, &sink)
            .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_DATA_INPUT);
    assert_eq!(diagnostic.source.as_deref(), Some("generator.input"));
}

#[test]
fn missing_table_reports_structured_pot_input_error() {
    let meta = MetaModel::from_toml_str(
        r#"
                [[tables]]
                name = "items"

                [[tables.fields]]
                name = "name"
                type = "text"
            "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let task = TargetDataConfig {
        name: Some("pot".to_owned()),
        output: PathBuf::from("out"),
        format: FORMAT.to_owned(),
        options: BTreeMap::new(),
    };
    let error = render_pot_export(
        &task,
        &linked,
        &TargetTags::empty(),
        &DataSet::new(Vec::new()),
    )
    .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_DATA_INPUT);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.gettext_pot.input")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("items"));
}
