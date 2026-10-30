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
    generator::{ArtifactGenerator, GeneratorCommonInput},
    meta::{MetaModel, TargetTags},
};

use super::{
    SchemaDoc, SchemaDocExportConfig, SchemaDocFormat, SchemaDocGenerator, SchemaDocOptions,
    format::{
        render_schema_doc_artifacts, schema_doc_export_config_from_options,
        schema_doc_output_paths, write_schema_doc_to_sink,
    },
    html::{escape_html, render_schema_doc_index_html, render_schema_table_html},
    model::{SchemaDocMetadata, SchemaSourceGraph, SchemaTableDoc},
    render_schema_doc_index_markdown, render_schema_table_markdown,
};

#[test]
fn schema_doc_does_not_depend_on_project_module() {
    for (path, source) in [
        (
            "src/generator/schema_doc/format.rs",
            include_str!("format.rs"),
        ),
        ("src/generator/schema_doc/html.rs", include_str!("html.rs")),
        (
            "src/generator/schema_doc/markdown.rs",
            include_str!("markdown.rs"),
        ),
        (
            "src/generator/schema_doc/model.rs",
            include_str!("model.rs"),
        ),
        ("src/generator/schema_doc/mod.rs", include_str!("mod.rs")),
    ] {
        assert!(
            !source.contains("crate::project") && !source.contains("project::"),
            "{path} should not depend on the project module"
        );
    }
}

fn build_doc(input: &str, tags: &[&str]) -> SchemaDoc {
    build_doc_with_options(
        input,
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

fn build_doc_with_options(input: &str, options: SchemaDocOptions) -> SchemaDoc {
    let meta = MetaModel::from_toml_str(input).expect("meta should parse");
    let linked = meta.link().expect("meta should link");
    SchemaDoc::from_meta(
        &linked,
        &ArtifactProjectInfo {
            name: Some("demo".to_owned()),
            desc: None,
            copyright: None,
        },
        options,
    )
}

fn schema_doc_options(tags: &[&str]) -> SchemaDocOptions {
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
    }
}

fn linked_meta(input: &str) -> crate::meta::LinkedMetaModel<'static> {
    let meta = Box::leak(Box::new(
        MetaModel::from_toml_str(input).expect("meta should parse"),
    ));
    meta.link().expect("meta should link")
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
                name = "tint"
                type = "color"
                color_format = "hex_rgba"
                color_channel = "byte"
                color_space = "srgb"

                [[tables.fields]]
                name = "spawn_position"
                type = "vector3"
                vector_format = "tuple"
                vector_components = ["x", "y", "z"]
                unit = "meter"

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
    assert_eq!(items.fields.len(), 7);
    assert_eq!(items.indexes[0].name, "primary");
    assert_eq!(items.relations[0].target, "classes.id");
    assert_eq!(items.relations[0].target_table, "classes");
    assert_eq!(items.relations[0].target_index, "primary");
    assert_eq!(
        items.asset_fields[0].rules.as_deref(),
        Some("extensions .png/.webp, prefix UI_")
    );
    assert_eq!(items.text_fields[0].field, "name");
    assert!(items.fields.iter().any(|field| {
        field.path == "tint"
            && field
                .type_display
                .contains("color(format=hex_rgba, channel=byte, space=srgb")
    }));
    assert!(items.fields.iter().any(|field| {
        field.path == "spawn_position"
            && field
                .type_display
                .contains("vector3(format=tuple, components=x/y/z, unit=meter)")
    }));
    assert!(
        items
            .fields
            .iter()
            .any(|field| !field.validators.is_empty())
    );
    assert_eq!(doc.relationship_graph.nodes.len(), 2);
    assert_eq!(doc.relationship_graph.edges.len(), 1);
    assert_eq!(doc.relationship_graph.edges[0].source_table, "items");
    assert_eq!(doc.relationship_graph.edges[0].target_table, "classes");
    assert_eq!(doc.source_graph.nodes.len(), 2);
    assert_eq!(doc.source_graph.edges.len(), 2);
    assert!(doc.source_graph.edges.iter().any(|edge| {
        edge.source_path == "items.csv"
            && edge.table == "items"
            && edge.role == "base"
            && edge.layout == "collection records"
    }));
    let classes = doc
        .tables
        .iter()
        .find(|table| table.name == "classes")
        .unwrap();
    assert_eq!(classes.incoming_relations.len(), 1);
    assert_eq!(classes.incoming_relations[0].source_table, "items");

    let markdown = render_schema_table_markdown(&doc, items);
    let index_markdown = render_schema_doc_index_markdown(&doc);
    assert!(index_markdown.contains("道具\\|表"));
    assert!(index_markdown.contains("## Source map"));
    assert!(
        index_markdown
            .contains("| items.csv | csv | items | base | - | collection records | all |")
    );
    assert!(index_markdown.contains("## Relationship overview"));
    assert!(
        index_markdown
            .contains("| items | class_id | classes | primary | many-to-one | yes | class | all |")
    );
    assert!(markdown.contains("## Related tables"));
    assert!(markdown.contains("### Outgoing relations"));
    assert!(markdown.contains("## Assets"));
    assert!(markdown.contains("## Text / POT"));
    assert!(markdown.contains("range(max=999, min=1)"));
}

#[test]
fn source_graph_captures_variant_overrides_and_object_table_layout() {
    let doc = build_doc(
        r#"
                [[tables]]
                name = "game_rules"
                kind = "object"
                source = "game_rules.csv"

                [[tables.fields]]
                name = "key"
                type = "string"

                [[tables.fields]]
                name = "value"
                type = "string"

                [[tables]]
                name = "items"
                polymorphic = true
                sources = [
                    { provider = "csv", path = "materials.csv", fragments = ["stack"] },
                    { provider = "csv", path = "weapons.csv", fragments = ["attributes", "skill"] },
                    { provider = "csv", path = "armors.csv", fragments = ["attributes"] },
                ]
                override_sources = [
                    { provider = "csv", path = "materials_s1.csv", fragments = ["stack"], export_when = { any = ["s1"] } },
                ]

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "name"
                type = "string"

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables.fragments]]
                name = "stack"

                [[tables.fragments.fields]]
                name = "max_stack"
                type = "int"

                [[tables.fragments]]
                name = "attributes"

                [[tables.fragments.fields]]
                name = "attack"
                type = "int"

                [[tables.fragments]]
                name = "skill"

                [[tables.fragments.fields]]
                name = "skill_id"
                type = "string"
            "#,
        &["client", "s1"],
    );

    assert_eq!(doc.source_graph.edges.len(), 5);
    assert!(doc.source_graph.edges.iter().any(|edge| {
        edge.source_path == "materials.csv"
            && edge.table == "items"
            && edge.fragments == vec!["stack".to_owned()]
            && edge.role == "base"
    }));
    assert!(doc.source_graph.edges.iter().any(|edge| {
        edge.source_path == "materials_s1.csv"
            && edge.table == "items"
            && edge.role == "override"
            && edge.visibility == "s1"
    }));
    assert!(doc.source_graph.edges.iter().any(|edge| {
        edge.source_path == "game_rules.csv"
            && edge.table == "game_rules"
            && edge.layout == "key/value rows"
            && edge.data_start_row == 2
    }));
    let items = doc
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table");
    let fragment_graph = items.fragment_graph.as_ref().expect("fragment graph");
    assert_eq!(fragment_graph.base_fields, vec!["id", "name"]);
    assert_eq!(fragment_graph.fragments.len(), 3);
    assert!(fragment_graph.fragments.iter().any(|fragment| {
        fragment.name == "stack"
            && fragment.fields[0].path == "max_stack"
            && fragment.source_hints
                == vec![
                    "materials.csv (base)".to_owned(),
                    "materials_s1.csv (override)".to_owned(),
                ]
    }));

    let markdown = render_schema_doc_index_markdown(&doc);
    assert!(
        markdown
            .contains("| materials.csv | csv | items | base | stack | collection records | all |")
    );
    assert!(markdown.contains(
        "| weapons.csv | csv | items | base | attributes, skill | collection records | all |"
    ));
    assert!(markdown.contains(
        "| materials_s1.csv | csv | items | override | stack | collection records | s1 |"
    ));
    assert!(
        markdown
            .contains("| game_rules.csv | csv | game_rules | base | - | key/value rows | all |")
    );
    let table_markdown = render_schema_table_markdown(&doc, items);
    assert!(table_markdown.contains("## Fragments"));
    assert!(table_markdown.contains(
        "| stack | inline fields | max_stack | materials.csv (base), materials_s1.csv (override) | all | - |"
    ));
}

#[test]
fn markdown_renderer_escapes_newlines_tabs_pipes_and_control_chars() {
    let doc = build_doc(
        "\
                [[tables]]\n\
                name = \"items\"\n\
                source = \"items.csv\"\n\
                desc = \"line one\\nline|two\\tindent\"\n\
\n\
                [[tables.fields]]\n\
                name = \"id\"\n\
                type = \"string\"\n\
                required = true\n\
                default = \"bad\\u0007value\"\n\
                desc = \"field one\\nfield|two\"\n\
\n\
                [[tables.fields.validators]]\n\
                kind = \"regex\"\n\
                pattern = \"^item\\n(id|name)$\"\n\
            ",
        &["client"],
    );
    let table = &doc.tables[0];
    let markdown = render_schema_table_markdown(&doc, table);

    assert!(markdown.contains("line one<br>line\\|two indent"));
    assert!(markdown.contains("field one<br>field\\|two"));
    assert!(markdown.contains("bad value"));
    assert!(!markdown.contains('\u{0007}'));
    assert!(markdown.contains("regex(pattern=^item<br>(id\\|name)$)"));
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
        include_hidden: false,
        include_internal: false,
        include_struct_details: true,
        include_asset_rules: true,
        include_i18n: true,
    };
    let paths = schema_doc_output_paths(&doc, &config).expect("paths should plan");
    assert_eq!(paths.index_path, PathBuf::from("out/docs/index.md"));
    assert_eq!(paths.table_paths, vec![PathBuf::from("out/docs/items.md")]);
    assert_eq!(paths.table_file_names, vec!["items.md"]);
}

#[test]
fn schema_doc_filters_and_includes_hidden_internal_content() {
    let input = r#"
                [[asset_kinds]]
                name = "ui_icon"
                extensions = [".png"]

                [[structs]]
                name = "Reward"

                [[structs.fields]]
                name = "amount"
                type = "int"

                [[tables]]
                name = "classes"
                source = "classes.csv"

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
                name = "server_classes"
                source = "server_classes.csv"
                export_when = { any = ["server"] }

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
                name = "support_table"
                source = "support.csv"
                export = false

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "items"
                source = "items.csv"
                polymorphic = true

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "name"
                type = "text"

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"

                [[tables.fields]]
                name = "reward"
                type = "Reward"

                [[tables.fields]]
                name = "class_id"
                type = "int"
                foreign_key = { target_table = "classes", target_index = "primary", linked_field = "class" }

                [[tables.fields]]
                name = "server_note"
                type = "string"
                export_when = { any = ["server"] }

                [[tables.fields.validators]]
                kind = "regex"
                pattern = "^server"

                [[tables.fields]]
                name = "server_icon"
                type = "asset"
                asset_kind = "ui_icon"
                export_when = { any = ["server"] }

                [[tables.fields]]
                name = "server_class_id"
                type = "int"
                export_when = { any = ["server"] }
                foreign_key = { target_table = "server_classes", target_index = "primary", linked_field = "server_class", export_when = { any = ["server"] } }

                [[tables.fields]]
                name = "internal_note"
                type = "string"
                export = false

                [[tables.fields.validators]]
                kind = "regex"
                pattern = "^internal"

                [[tables.fields]]
                name = "internal_text"
                type = "text"
                export = false

                [[tables.fields]]
                name = "internal_class_id"
                type = "int"
                export = false
                foreign_key = { target_table = "classes", target_index = "primary", linked_field = "internal_class", export = false }

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables.indexes]]
                name = "by_server_note"
                fields = ["server_note"]
                export_when = { any = ["server"] }

                [[tables.indexes]]
                name = "by_internal_note"
                fields = ["internal_note"]
                export = false

                [[tables.fragments]]
                name = "visible_fragment"

                [[tables.fragments.fields]]
                name = "visible_fragment_field"
                type = "int"

                [[tables.fragments]]
                name = "server_fragment"
                export_when = { any = ["server"] }

                [[tables.fragments.fields]]
                name = "server_fragment_field"
                type = "int"

                [[tables.fragments]]
                name = "internal_fragment"
                export = false

                [[tables.fragments.fields]]
                name = "internal_fragment_field"
                type = "int"
            "#;

    let default_doc = build_doc(input, &["client"]);
    assert_eq!(
        default_doc
            .tables
            .iter()
            .map(|table| table.name.as_str())
            .collect::<Vec<_>>(),
        vec!["classes", "items"]
    );
    assert_eq!(default_doc.relationship_graph.nodes.len(), 2);
    assert_eq!(default_doc.relationship_graph.edges.len(), 1);
    assert!(
        !default_doc
            .source_graph
            .edges
            .iter()
            .any(|edge| edge.source_path == "server_classes.csv"
                || edge.source_path == "support.csv")
    );
    let items = default_doc
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table");
    assert!(!items.fields.iter().any(|field| field.path == "server_note"));
    assert!(
        !items
            .fields
            .iter()
            .any(|field| field.path == "internal_note")
    );
    assert!(
        !items
            .indexes
            .iter()
            .any(|index| index.name == "by_server_note")
    );
    assert!(
        !items
            .indexes
            .iter()
            .any(|index| index.name == "by_internal_note")
    );
    assert!(
        !items
            .relations
            .iter()
            .any(|relation| relation.field == "server_class_id")
    );
    assert!(
        !items
            .relations
            .iter()
            .any(|relation| relation.field == "internal_class_id")
    );
    assert!(
        !items
            .asset_fields
            .iter()
            .any(|field| field.field == "server_icon")
    );
    assert!(
        !items
            .text_fields
            .iter()
            .any(|field| field.field == "internal_text")
    );
    assert!(
        !items
            .fragment_graph
            .as_ref()
            .expect("visible fragment graph")
            .fragments
            .iter()
            .any(|fragment| fragment.name == "server_fragment"
                || fragment.name == "internal_fragment")
    );

    let mut include_options = schema_doc_options(&["client"]);
    include_options.include_hidden = true;
    include_options.include_internal = true;
    let included_doc = build_doc_with_options(input, include_options);
    assert_eq!(
        included_doc
            .tables
            .iter()
            .map(|table| (table.name.as_str(), table.visibility.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("classes", "all"),
            ("server_classes", "hidden"),
            ("support_table", "internal"),
            ("items", "all"),
        ]
    );
    assert_eq!(included_doc.relationship_graph.nodes.len(), 4);
    assert!(
        included_doc
            .source_graph
            .edges
            .iter()
            .any(|edge| edge.source_path == "server_classes.csv" && edge.visibility == "hidden")
    );
    assert!(
        included_doc
            .source_graph
            .edges
            .iter()
            .any(|edge| edge.source_path == "support.csv" && edge.table == "support_table")
    );
    let items = included_doc
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table");
    assert!(
        items
            .fields
            .iter()
            .any(|field| field.path == "server_note" && field.visibility == "hidden")
    );
    assert!(
        items
            .fields
            .iter()
            .any(|field| field.path == "internal_note" && field.visibility == "internal")
    );
    assert!(
        items
            .indexes
            .iter()
            .any(|index| index.name == "by_server_note" && index.visibility == "hidden")
    );
    assert!(
        items
            .indexes
            .iter()
            .any(|index| index.name == "by_internal_note" && index.visibility == "internal")
    );
    assert!(
        items
            .relations
            .iter()
            .any(|relation| relation.field == "server_class_id" && relation.visibility == "hidden")
    );
    assert!(
        items
            .relations
            .iter()
            .any(|relation| relation.field == "internal_class_id"
                && relation.visibility == "internal")
    );
    assert!(
        items
            .fields
            .iter()
            .find(|field| field.path == "server_note")
            .is_some_and(|field| !field.validators.is_empty())
    );
    assert!(
        items
            .fields
            .iter()
            .find(|field| field.path == "internal_note")
            .is_some_and(|field| !field.validators.is_empty())
    );
    assert!(
        items
            .asset_fields
            .iter()
            .any(|field| field.field == "server_icon")
    );
    assert!(
        items
            .text_fields
            .iter()
            .all(|field| field.field != "internal_text")
    );
    assert!(
        items
            .fragment_graph
            .as_ref()
            .expect("fragment graph")
            .fragments
            .iter()
            .any(|fragment| fragment.name == "server_fragment" && fragment.visibility == "hidden")
    );
    assert!(
        items
            .fragment_graph
            .as_ref()
            .expect("fragment graph")
            .fragments
            .iter()
            .any(|fragment| fragment.name == "internal_fragment"
                && fragment.visibility == "internal")
    );

    let markdown = render_schema_table_markdown(&included_doc, items);
    assert!(markdown.contains("| server_note | string | no | - | hidden | - |"));
    assert!(markdown.contains("| internal_note | string | no | - | internal | - |"));
    assert!(markdown.contains("server_class_id"));
    assert!(markdown.contains("server_class"));
    let html = render_schema_table_html(&included_doc, items, "index.html");
    assert!(html.contains("<span class=\"badge\">hidden</span>"));
    assert!(html.contains("<span class=\"badge\">internal</span>"));
}

#[test]
fn schema_doc_detail_toggles_hide_struct_asset_rules_and_i18n_sections() {
    let input = r#"
                [[asset_kinds]]
                name = "ui_icon"
                extensions = [".png"]

                [[structs]]
                name = "Reward"

                [[structs.fields]]
                name = "amount"
                type = "int"

                [[tables]]
                name = "items"
                source = "items.csv"
                polymorphic = true

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "name"
                type = "text"

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"

                [[tables.fields]]
                name = "reward"
                type = "Reward"

                [[tables.fragments]]
                name = "bonus"

                [[tables.fragments.fields]]
                name = "bonus_value"
                type = "int"
            "#;
    let mut options = schema_doc_options(&["client"]);
    options.include_struct_details = false;
    options.include_asset_rules = false;
    options.include_i18n = false;
    let doc = build_doc_with_options(input, options);
    let items = &doc.tables[0];

    assert!(items.fields.iter().any(|field| field.path == "reward"));
    assert!(
        !items
            .fields
            .iter()
            .any(|field| field.path == "reward.amount")
    );
    assert!(
        !items
            .fields
            .iter()
            .any(|field| field.path == "bonus.bonus_value")
    );
    assert!(items.fragment_graph.is_none());
    assert_eq!(items.asset_fields[0].asset_kind.as_deref(), Some("ui_icon"));
    assert!(items.asset_fields[0].rules.is_none());
    assert!(items.text_fields.is_empty());

    let markdown = render_schema_table_markdown(&doc, items);
    assert!(!markdown.contains("## Fragments"));
    assert!(markdown.contains("| icon | ui_icon | no | all | - |"));
    assert!(!markdown.contains("## Text / POT"));
    let html = render_schema_table_html(&doc, items, "index.html");
    assert!(!html.contains("<section id=\"fragments\">"));
    assert!(!html.contains("<h2>Text / POT</h2>"));
}

#[test]
fn schema_doc_large_graph_order_and_rendered_paths_are_deterministic() {
    let input = r#"
                [[tables]]
                name = "classes"
                source = "classes.csv"

                [[tables.fields]]
                name = "class_id"
                type = "int"

                [[tables.indexes]]
                name = "by_id"
                fields = ["class_id"]
                unique = true
                primary = true

                [[tables]]
                name = "quests"
                source = "quests.csv"

                [[tables.fields]]
                name = "quest_id"
                type = "int"

                [[tables.fields]]
                name = "required_class_id"
                type = "int"
                foreign_key = { target_table = "classes", target_index = "by_id", linked_field = "required_class", allow_empty = true }

                [[tables.fields]]
                name = "prerequisite_quest_ids"
                type = "list<int>"
                foreign_key = { target_table = "quests", target_index = "primary", linked_field = "prerequisite_quests", allow_empty = true }

                [[tables.indexes]]
                name = "primary"
                fields = ["quest_id"]
                unique = true
                primary = true

                [[tables]]
                name = "game_rules"
                kind = "object"
                source = "game_rules.csv"

                [[tables.fields]]
                name = "default_class_id"
                type = "int"
                foreign_key = { target_table = "classes", target_index = "by_id", linked_field = "default_class" }

                [[tables.fields]]
                name = "featured_quest_ids"
                type = "list<int>"
                foreign_key = { target_table = "quests", target_index = "primary", linked_field = "featured_quests" }

                [[tables]]
                name = "items"
                polymorphic = true
                sources = [
                    { provider = "csv", path = "materials.csv", fragments = ["stack"] },
                    { provider = "csv", path = "weapons.csv", fragments = ["stats", "skill"] },
                ]
                override_sources = [
                    { provider = "csv", path = "weapons_s1.csv", fragments = ["stats", "skill"], export_when = { any = ["s1"] } },
                ]

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables.fragments]]
                name = "stack"

                [[tables.fragments.fields]]
                name = "max_stack"
                type = "int"

                [[tables.fragments]]
                name = "stats"

                [[tables.fragments.fields]]
                name = "attack"
                type = "int"

                [[tables.fragments]]
                name = "skill"

                [[tables.fragments.fields]]
                name = "skill_id"
                type = "int"
            "#;
    let doc_a = build_doc(input, &["client", "s1"]);
    let doc_b = build_doc(input, &["client", "s1"]);

    let table_names = doc_a
        .tables
        .iter()
        .map(|table| table.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        table_names,
        vec!["classes", "quests", "game_rules", "items"]
    );
    assert_eq!(
        doc_a
            .relationship_graph
            .nodes
            .iter()
            .map(|node| node.table.as_str())
            .collect::<Vec<_>>(),
        table_names
    );
    assert_eq!(
        doc_a
            .relationship_graph
            .edges
            .iter()
            .map(|edge| {
                (
                    edge.source_table.as_str(),
                    edge.source_field.as_str(),
                    edge.target_table.as_str(),
                    edge.cardinality.as_str(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("quests", "required_class_id", "classes", "many-to-one"),
            ("quests", "prerequisite_quest_ids", "quests", "list-to-many",),
            ("game_rules", "default_class_id", "classes", "many-to-one"),
            ("game_rules", "featured_quest_ids", "quests", "list-to-many",),
        ]
    );
    assert_eq!(
        doc_a
            .source_graph
            .edges
            .iter()
            .map(|edge| (
                edge.source_path.as_str(),
                edge.table.as_str(),
                edge.role.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("classes.csv", "classes", "base"),
            ("quests.csv", "quests", "base"),
            ("game_rules.csv", "game_rules", "base"),
            ("materials.csv", "items", "base"),
            ("weapons.csv", "items", "base"),
            ("weapons_s1.csv", "items", "override"),
        ]
    );
    let quests = doc_a
        .tables
        .iter()
        .find(|table| table.name == "quests")
        .expect("quests table");
    assert_eq!(quests.incoming_relations.len(), 2);
    assert_eq!(quests.incoming_relations[0].source_table, "quests");
    assert_eq!(quests.incoming_relations[1].source_table, "game_rules");
    let items = doc_a
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table");
    let fragment_graph = items.fragment_graph.as_ref().expect("fragment graph");
    assert_eq!(
        fragment_graph
            .fragments
            .iter()
            .map(|fragment| fragment.name.as_str())
            .collect::<Vec<_>>(),
        vec!["stack", "stats", "skill"]
    );

    let config = SchemaDocExportConfig {
        format: SchemaDocFormat::Html,
        output: PathBuf::from("out/docs"),
        index_file: "index.html".to_owned(),
        title: None,
        include_hidden: false,
        include_internal: false,
        include_struct_details: true,
        include_asset_rules: true,
        include_i18n: true,
    };
    let artifacts_a = render_schema_doc_artifacts(&doc_a, &config).expect("render a");
    let artifacts_b = render_schema_doc_artifacts(&doc_b, &config).expect("render b");
    assert_eq!(artifacts_a, artifacts_b);
    assert_eq!(
        artifacts_a
            .iter()
            .map(|artifact| artifact.path.clone())
            .collect::<Vec<_>>(),
        vec![
            PathBuf::from("out/docs/index.html"),
            PathBuf::from("out/docs/classes.html"),
            PathBuf::from("out/docs/quests.html"),
            PathBuf::from("out/docs/game_rules.html"),
            PathBuf::from("out/docs/items.html"),
        ]
    );
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
    assert!(!html_config.include_hidden);
    assert!(!html_config.include_internal);
    assert!(html_config.include_struct_details);
    assert!(html_config.include_asset_rules);
    assert!(html_config.include_i18n);

    let configured = schema_doc_export_config_from_options(
        PathBuf::from("out/docs"),
        BTreeMap::from([
            ("include_hidden".to_owned(), toml::Value::Boolean(true)),
            ("include_internal".to_owned(), toml::Value::Boolean(true)),
            (
                "include_struct_details".to_owned(),
                toml::Value::Boolean(false),
            ),
            (
                "include_asset_rules".to_owned(),
                toml::Value::Boolean(false),
            ),
            ("include_i18n".to_owned(), toml::Value::Boolean(false)),
        ]),
    )
    .expect("schema_doc include options should parse");
    assert!(configured.include_hidden);
    assert!(configured.include_internal);
    assert!(!configured.include_struct_details);
    assert!(!configured.include_asset_rules);
    assert!(!configured.include_i18n);

    let error = schema_doc_export_config_from_options(
        PathBuf::from("out/docs"),
        BTreeMap::from([(
            "include_hidden".to_owned(),
            toml::Value::String("yes".to_owned()),
        )]),
    )
    .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(diagnostic.current_value.as_deref(), Some("include_hidden"));
    assert_eq!(diagnostic.expected.as_deref(), Some("boolean value"));

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
        include_hidden: false,
        include_internal: false,
        include_struct_details: true,
        include_asset_rules: true,
        include_i18n: true,
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
fn schema_doc_generator_plans_markdown_and_html_artifacts() {
    let meta = linked_meta(
        r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
    );
    let base_input = GeneratorCommonInput {
        meta: &meta,
        target_tags: TargetTags::new(["client"]),
        project: ArtifactProjectInfo {
            name: Some("demo".to_owned()),
            desc: None,
            copyright: None,
        },
        project_root: PathBuf::from("."),
        target_name: "client".to_owned(),
        task_name: "schema".to_owned(),
        type_id: "schema_doc".to_owned(),
        output: PathBuf::from("out/docs"),
        options: BTreeMap::new(),
        generated_at: "2026-06-07T00:00:00Z".to_owned(),
    };

    let markdown_paths = SchemaDocGenerator
        .plan_artifacts(&base_input)
        .expect("markdown paths should plan");
    assert_eq!(
        markdown_paths,
        vec![
            PathBuf::from("out/docs/index.md"),
            PathBuf::from("out/docs/items.md")
        ]
    );

    let mut html_input = base_input.clone();
    html_input.options =
        BTreeMap::from([("format".to_owned(), toml::Value::String("html".to_owned()))]);
    let html_paths = SchemaDocGenerator
        .plan_artifacts(&html_input)
        .expect("html paths should plan");
    assert_eq!(
        html_paths,
        vec![
            PathBuf::from("out/docs/index.html"),
            PathBuf::from("out/docs/items.html")
        ]
    );

    for format in ["markdown", "html"] {
        for include_hidden in [false, true] {
            for include_internal in [false, true] {
                let meta = linked_meta(
                    r#"
                    [[tables]]
                    name = "visible"
                    [[tables]]
                    name = "hidden"
                    export_when = { any = ["server"] }
                    [[tables]]
                    name = "internal"
                    export = false
                "#,
                );
                let mut input = base_input.clone();
                input.meta = &meta;
                input.options = BTreeMap::from([
                    ("format".into(), toml::Value::String(format.into())),
                    (
                        "include_hidden".into(),
                        toml::Value::Boolean(include_hidden),
                    ),
                    (
                        "include_internal".into(),
                        toml::Value::Boolean(include_internal),
                    ),
                ]);
                let paths = SchemaDocGenerator.plan_artifacts(&input).unwrap();
                assert_eq!(
                    paths.len(),
                    2 + usize::from(include_hidden) + usize::from(include_internal)
                );
                let sink = crate::artifact::ArtifactWriter::new(
                    crate::artifact::ArtifactWriteMode::DryRun,
                )
                .open_sink();
                sink.reserve_paths(paths).unwrap();
                SchemaDocGenerator
                    .generate(
                        &crate::generator::GeneratorTaskInput {
                            common: input,
                            prepared: crate::generator::PreparedGeneratorInputs::MetaOnly,
                        },
                        &sink,
                    )
                    .unwrap();
                sink.finish().unwrap();
            }
        }
    }
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
        include_hidden: false,
        include_internal: false,
        include_struct_details: true,
        include_asset_rules: true,
        include_i18n: true,
    };
    let artifacts = render_schema_doc_artifacts(&doc, &config).expect("html artifacts render");
    assert_eq!(artifacts[0].path, PathBuf::from("out/docs/index.html"));
    assert_eq!(artifacts[1].path, PathBuf::from("out/docs/items.html"));
    assert!(artifacts[0].content.contains("<!doctype html>"));
}

#[test]
fn schema_doc_output_quality_uses_relative_links_and_stable_labels() {
    let doc = build_doc(
        r#"
                [[tables]]
                name = "classes"
                source = "classes.csv"

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

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "class_id"
                type = "int"
                foreign_key = { target_table = "classes", target_index = "primary", linked_field = "class", allow_empty = true }
            "#,
        &["client"],
    );
    let items = doc
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table");
    let index_markdown = render_schema_doc_index_markdown(&doc);
    let table_markdown = render_schema_table_markdown(&doc, items);
    let index_html =
        render_schema_doc_index_html(&doc, &["classes.html".to_owned(), "items.html".to_owned()]);
    let table_html = render_schema_table_html(&doc, items, "index.html");

    assert!(index_markdown.contains("[items](items.md)"));
    assert!(
        table_markdown.contains("| class_id | classes.id | many-to-one | yes | class | all | - |")
    );
    assert!(
        index_markdown
            .contains("| items | class_id | classes | primary | many-to-one | yes | class | all |")
    );
    assert!(!index_markdown.contains("many:one"));
    assert!(!table_markdown.contains("many:one"));

    for html in [&index_html, &table_html] {
        assert!(html.contains("<style>"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
        assert!(!html.contains("rel=\"stylesheet\""));
    }
    assert!(index_html.contains("<a href=\"items.html\"><code>items</code></a>"));
    assert!(table_html.contains("<a href=\"index.html\">← Back to index</a>"));
    assert!(table_html.contains("many-to-one"));
    assert!(table_html.contains("allow_empty = optional reference"));
}

#[test]
fn renders_html_index_metadata_summary_and_cross_table_sections() {
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
                desc = "道具表"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "name"
                type = "text"
                export_when = { any = ["client"] }

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"
                export_when = { any = ["client"] }

                [[tables.fields]]
                name = "class_id"
                type = "int"
                foreign_key = { target_table = "classes", target_index = "primary", linked_field = "class" }

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true
            "#,
        &["client"],
    );
    let html =
        render_schema_doc_index_html(&doc, &["classes.html".to_owned(), "items.html".to_owned()]);

    assert!(html.contains(
        "<span class=\"meta-key\">Project</span>: <span class=\"meta-value\">demo</span>"
    ));
    assert!(html.contains(
        "<span class=\"meta-key\">Target</span>: <span class=\"meta-value\">client</span>"
    ));
    assert!(html.contains(
        "<span class=\"meta-key\">Tags</span>: <span class=\"meta-value\">client</span>"
    ));
    assert!(html.contains("<nav class=\"quick-links\" aria-label=\"Quick links\">"));
    assert!(html.contains("<section id=\"tables\">"));
    assert!(html.contains("<section id=\"relationship-graph\">"));
    assert!(html.contains("<section id=\"source-map\">"));
    assert!(html.contains("<span class=\"node source-node\"><code>items.csv</code></span>"));
    assert!(html.contains("source → table"));
    assert!(html.contains("primary = default unique row identity"));
    assert!(html.contains("asset field → asset manifest exporter"));
    assert!(html.contains("text field → gettext_pot"));
    assert!(html.contains("actual asset refs are not read by schema_doc"));
    assert!(html.contains("actual text values are not read by schema_doc"));
    assert!(html.contains("<span>Tables</span>"));
    assert!(html.contains("<span>Fields</span>"));
    assert!(html.contains("<a href=\"items.html\"><code>items</code></a>"));
    assert!(html.contains("<th>Indexes</th><th>Relations</th>"));
    assert!(html.contains("<h2>Asset fields</h2>"));
    assert!(html.contains("<code>icon</code>"));
    assert!(html.contains("extensions .png/.webp, prefix UI_"));
    assert!(html.contains("<h2>Text / POT fields</h2>"));
    assert!(html.contains("<code>name</code>"));
    assert!(html.contains("<h2>Relation overview</h2>"));
    assert!(html.contains("<code>classes.id</code>"));
}

#[test]
fn renders_html_table_sections_for_schema_details() {
    let doc = build_doc(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                extensions = [".png"]

                [[tables]]
                name = "classes"
                source = "classes.csv"

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
                desc = "道具表"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "name"
                type = "text"
                required = true

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"

                [[tables.fields]]
                name = "class_id"
                type = "int"
                foreign_key = { target_table = "classes", target_index = "primary", linked_field = "class" }

                [[tables.fields.validators]]
                kind = "range"
                min = 1
                max = 999

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true
            "#,
        &["client"],
    );
    let table = doc
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table");
    let html = render_schema_table_html(&doc, table, "index.html");

    assert!(html.contains("<a href=\"index.html\">← Back to index</a>"));
    assert!(html.contains("<header id=\"overview\" class=\"card hero\">"));
    assert!(html.contains("<section id=\"neighborhood\">"));
    assert!(html.contains("<section id=\"source-map\">"));
    assert!(html.contains("<span class=\"node source-node\"><code>items.csv</code></span>"));
    assert!(
        html.contains("<a class=\"node table-node\" href=\"#overview\"><code>items</code></a>")
    );
    assert!(html.contains("<h2>Fields</h2>"));
    assert!(html.contains("<section id=\"fields\">"));
    assert!(html.contains("<h2>Indexes</h2>"));
    assert!(html.contains("<section id=\"indexes\">"));
    assert!(html.contains("<code>primary</code>"));
    assert!(html.contains("<h2>Relations</h2>"));
    assert!(html.contains("allow_empty = optional reference"));
    assert!(html.contains("<code>classes.id</code>"));
    assert!(html.contains("<code>class</code>"));
    assert!(html.contains("<h2>Assets</h2>"));
    assert!(html.contains("asset field → asset manifest exporter"));
    assert!(html.contains("<code>ui_icon</code>"));
    assert!(html.contains("<h2>Text / POT</h2>"));
    assert!(html.contains("text field → gettext_pot"));
    assert!(html.contains("<code>name</code>"));
    assert!(html.contains("<h2>Validators</h2>"));
    assert!(html.contains("range(max=999, min=1)"));
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

    let error = schema_doc_export_config_from_options(
        PathBuf::from("out"),
        BTreeMap::from([(
            "index_file".to_owned(),
            toml::Value::String("../index.md".to_owned()),
        )]),
    )
    .unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(diagnostic.current_value.as_deref(), Some("../index.md"));
    assert_eq!(
        diagnostic.expected.as_deref(),
        Some("plain file name without path separators")
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
        relationship_graph: super::model::SchemaRelationshipGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
        },
        source_graph: SchemaSourceGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
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
            incoming_relations: Vec::new(),
            fragment_graph: None,
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
        include_hidden: false,
        include_internal: false,
        include_struct_details: true,
        include_asset_rules: true,
        include_i18n: true,
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

    let bad_table_doc = SchemaDoc {
        metadata: SchemaDocMetadata {
            title: "Schema".to_owned(),
            project_name: None,
            project_path: None,
            target: None,
            target_tags: Vec::new(),
            generated_at: "now".to_owned(),
            config_forge_version: "test".to_owned(),
        },
        relationship_graph: super::model::SchemaRelationshipGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
        },
        source_graph: SchemaSourceGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
        },
        tables: vec![SchemaTableDoc {
            name: "../items".to_owned(),
            kind: "CollectionTable".to_owned(),
            description: None,
            source_summary: None,
            visibility: "all".to_owned(),
            row_export_summary: "no".to_owned(),
            primary_index: None,
            fields: Vec::new(),
            indexes: Vec::new(),
            relations: Vec::new(),
            incoming_relations: Vec::new(),
            fragment_graph: None,
            asset_fields: Vec::new(),
            text_fields: Vec::new(),
            validators: Vec::new(),
        }],
    };
    let error = schema_doc_output_paths(&bad_table_doc, &config).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_GENERATOR_INVALID_OPTION);
    assert_eq!(diagnostic.current_value.as_deref(), Some("../items.md"));

    let mut duplicate_doc = bad_table_doc.clone();
    duplicate_doc.tables[0].name = "items".to_owned();
    duplicate_doc.tables.push(duplicate_doc.tables[0].clone());
    let error = schema_doc_output_paths(&duplicate_doc, &config).unwrap_err();
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_ARTIFACT_PATH_COLLISION);
    assert!(
        diagnostic
            .current_value
            .is_some_and(|value| value.contains("items.md"))
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
        include_hidden: false,
        include_internal: false,
        include_struct_details: true,
        include_asset_rules: true,
        include_i18n: true,
    };
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    write_schema_doc_to_sink(&doc, &config, &sink).expect("write docs");
    sink.finish().expect("finish sink");
    assert!(temp_dir.path().join("docs/index.md").exists());
    assert!(temp_dir.path().join("docs/items.md").exists());
}
