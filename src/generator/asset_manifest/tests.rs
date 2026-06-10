// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    artifact::{ArtifactWriteMode, ArtifactWriter},
    code_artifact::tests::expect_single_diagnostic,
    data::{DataSet, DataTable},
    data_artifact::{AggregateDataExporter, TargetDataConfig},
    diagnostic::code::{
        CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH, CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    },
    meta::{MetaModel, TargetTags},
};

use super::*;

fn linked_meta() -> crate::meta::LinkedMetaModel<'static> {
    let meta: &'static MetaModel = Box::leak(Box::new(
        toml::from_str(
            r#"
                [[tables]]
                name = "items"
                kind = "collection"
                primary_index = "by_id"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "asset"
                type = "asset"
                asset_kind = "unreal_skill_class"
                export_when = { any = ["client"] }

                [[tables.fields]]
                name = "tags"
                type = "string"
                export = false

                [tables.row_export]
                excluded_tags_field = "tags"

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true
                primary = true

                [[asset_kinds]]
                name = "unreal_skill_class"
                search_paths = ["game"]
                input = "path"
                path_style = "mounted_object"
                extensions = [".uasset"]
                "#,
        )
        .unwrap(),
    ));
    meta.link().unwrap()
}

fn data_set() -> DataSet {
    let rows = vec![
        BTreeMap::from([
            ("id".to_owned(), "2".to_owned()),
            (
                "asset".to_owned(),
                "/Game/Skills/GA_Fireball.GA_Fireball_C".to_owned(),
            ),
            ("tags".to_owned(), "client".to_owned()),
        ]),
        BTreeMap::from([
            ("id".to_owned(), "1".to_owned()),
            (
                "asset".to_owned(),
                "/Game/Skills/GA_Slash.GA_Slash_C".to_owned(),
            ),
        ]),
        BTreeMap::from([
            ("id".to_owned(), "3".to_owned()),
            (
                "asset".to_owned(),
                "/Game/Skills/GA_Slash.GA_Slash_C".to_owned(),
            ),
        ]),
    ];
    DataSet::new(vec![DataTable::new("items", rows)])
}

fn task_with_options(options: BTreeMap<String, toml::Value>) -> TargetDataConfig {
    TargetDataConfig {
        name: Some("assets".to_owned()),
        output: PathBuf::from("out"),
        format: FORMAT.to_owned(),
        options,
    }
}

#[test]
fn renders_deterministic_json_manifest_for_visible_assets() {
    let linked = linked_meta();
    let task = task_with_options(BTreeMap::new());
    let (_, output) =
        render_asset_manifest(&task, &linked, &TargetTags::new(["client"]), &data_set()).unwrap();
    assert!(output.contains("\"version\": 1"));
    assert!(output.contains("\"path\": \"/Game/Skills/GA_Slash.GA_Slash_C\""));
    assert!(output.contains("\"row\": \"id=1\""));
    assert!(output.contains("\"row\": \"id=3\""));
    assert!(!output.contains("GA_Fireball"));
    assert_eq!(
        output.matches("/Game/Skills/GA_Slash.GA_Slash_C").count(),
        1
    );
}

#[test]
fn renders_unreal_datatable_csv_manifest() {
    let linked = linked_meta();
    let task = task_with_options(BTreeMap::from([(
        "format".to_owned(),
        toml::Value::String("csv".to_owned()),
    )]));
    let (_, output) =
        render_asset_manifest(&task, &linked, &TargetTags::new(["client"]), &data_set()).unwrap();
    assert!(
        output
            .starts_with("Name,AssetRef,AssetKind,ReferencedBy,Tables,Fields,RecordKeys,Sources\n")
    );
    assert!(output.contains("unreal_skill_class_Game_Skills_GA_Slash_GA_Slash_C"));
    assert!(output.contains("/Game/Skills/GA_Slash.GA_Slash_C"));
    assert!(output.contains("items[id=1].asset|items[id=3].asset"));
}

#[test]
fn renders_simple_csv_manifest_with_deduplicated_asset_refs_only() {
    let linked = linked_meta();
    let task = task_with_options(BTreeMap::from([
        ("format".to_owned(), toml::Value::String("csv".to_owned())),
        (
            "csv_mode".to_owned(),
            toml::Value::String("simple".to_owned()),
        ),
        ("deduplicate".to_owned(), toml::Value::Boolean(false)),
    ]));
    let (_, output) =
        render_asset_manifest(&task, &linked, &TargetTags::new(["client"]), &data_set()).unwrap();
    assert_eq!(output, "AssetRef\n/Game/Skills/GA_Slash.GA_Slash_C\n");
}

#[test]
fn exporter_plans_format_specific_default_file_name() {
    let exporter = AssetManifestExporter;
    let task = task_with_options(BTreeMap::from([(
        "format".to_owned(),
        toml::Value::String("csv".to_owned()),
    )]));
    let paths = AggregateDataExporter::plan_artifacts(
        &exporter,
        Path::new("Generated/Assets"),
        &task,
        &linked_meta(),
        &TargetTags::empty(),
    )
    .unwrap();
    assert_eq!(
        paths,
        [PathBuf::from("Generated/Assets/asset_manifest.csv")]
    );
}

#[test]
fn generate_to_sink_reports_structured_contract_error() {
    let exporter = AssetManifestExporter;
    let linked = linked_meta();
    let input = crate::generator::GeneratorCommonInput {
        project: crate::ArtifactProjectInfo::default(),
        project_root: Path::new(".").to_path_buf(),
        target_name: "client".to_owned(),
        target_tags: TargetTags::empty(),
        meta: &linked,
        output: PathBuf::from("out"),
        type_id: FORMAT.to_owned(),
        task_name: "assets".to_owned(),
        options: BTreeMap::new(),
        generated_at: "now".to_owned(),
    };
    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();
    let error =
        <AssetManifestExporter as ArtifactGenerator>::generate_to_sink(&exporter, &input, &sink)
            .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.asset_manifest.contract")
    );
}

#[test]
fn missing_table_reports_structured_manifest_input_error() {
    let linked = linked_meta();
    let task = task_with_options(BTreeMap::new());
    let data_set = DataSet::new(Vec::new());
    let error =
        render_asset_manifest(&task, &linked, &TargetTags::new(["client"]), &data_set).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_UNSUPPORTED_DATA_INPUT);
    assert_eq!(
        diagnostic.source.as_deref(),
        Some("generator.asset_manifest.input")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("items"));
}
