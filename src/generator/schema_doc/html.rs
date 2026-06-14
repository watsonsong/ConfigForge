// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::model::{SchemaDoc, SchemaRelationEdgeDoc, SchemaSourceTableEdgeDoc, SchemaTableDoc};

const STYLE: &str = r#"
body { margin: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: #1f2937; background: #f8fafc; }
main { max-width: 1120px; margin: 0 auto; padding: 32px; }
header { margin-bottom: 24px; }
h1, h2 { color: #111827; }
section { scroll-margin-top: 16px; }
table { width: 100%; border-collapse: collapse; margin: 16px 0; background: #fff; }
th, td { border: 1px solid #d1d5db; padding: 8px 10px; text-align: left; vertical-align: top; }
th { background: #f3f4f6; }
code { background: #eef2ff; border-radius: 4px; padding: 1px 4px; }
.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 12px; margin: 16px 0; }
.badge { display: inline-block; border-radius: 999px; padding: 2px 8px; background: #e0f2fe; color: #075985; font-size: 0.85em; }
.card { background: #fff; border: 1px solid #e5e7eb; border-radius: 8px; padding: 16px; margin: 16px 0; }
.hero { border-color: #c7d2fe; box-shadow: 0 10px 30px rgba(79, 70, 229, 0.08); }
.meta-bar { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; margin-top: 10px; }
.meta-item { white-space: nowrap; }
.meta-key { color: #6b7280; font-weight: 600; }
.meta-value { color: #111827; font-weight: 600; }
.quick-links { display: flex; flex-wrap: wrap; gap: 8px; margin: 14px 0 0; }
.quick-links a { border: 1px solid #bfdbfe; border-radius: 999px; background: #eff6ff; padding: 4px 10px; text-decoration: none; font-size: 0.9em; }
.summary-card { background: #fff; border: 1px solid #e5e7eb; border-radius: 8px; padding: 14px; }
.summary-card strong { display: block; font-size: 1.6em; color: #111827; }
.summary-card span { color: #6b7280; font-size: 0.9em; }
.edge-list { display: grid; gap: 10px; margin: 16px 0; }
.edge-card { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 12px; background: #fff; border: 1px solid #dbeafe; border-radius: 10px; }
.fragment-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; margin: 16px 0; }
.fragment-card { background: #fff; border: 1px solid #ddd6fe; border-radius: 10px; padding: 12px; }
.node { display: inline-block; border: 1px solid #c7d2fe; background: #eef2ff; border-radius: 8px; padding: 4px 8px; text-decoration: none; }
.source-node { background: #ecfdf5; border-color: #a7f3d0; }
.table-node { background: #eef2ff; border-color: #c7d2fe; }
.arrow { color: #374151; font-weight: 600; }
.legend { display: flex; flex-wrap: wrap; gap: 8px; margin: 10px 0; }
.muted { color: #6b7280; }
a { color: #2563eb; }
"#;

pub fn render_schema_doc_index_html(doc: &SchemaDoc, table_file_names: &[String]) -> String {
    let mut body = String::new();
    push_line(&mut body, "<header class=\"card hero\">");
    push_line(
        &mut body,
        &format!("<h1>{}</h1>", escape_html(&doc.metadata.title)),
    );
    push_metadata_bar(
        &mut body,
        &[
            ("Project", doc.metadata.project_name.as_deref(), false),
            ("Target", doc.metadata.target.as_deref(), false),
            (
                "Tags",
                Some(&display_list_or_dash(&doc.metadata.target_tags)),
                false,
            ),
            ("Generated", Some(&doc.metadata.generated_at), true),
        ],
    );
    push_quick_links(
        &mut body,
        &[
            ("Tables", "tables"),
            ("Relationship graph", "relationship-graph"),
            ("Source map", "source-map"),
            ("Assets", "asset-fields"),
            ("Text/POT", "text-pot-fields"),
            ("Relations", "relations"),
        ],
    );
    push_line(&mut body, "</header>");
    push_index_summary_cards(&mut body, doc);
    push_line(&mut body, "<section id=\"tables\">");
    push_line(&mut body, "<h2>Tables</h2>");
    push_line(&mut body, "<table>");
    push_line(
        &mut body,
        "<thead><tr><th>Table</th><th>Kind</th><th>Description</th><th>Fields</th><th>Indexes</th><th>Relations</th><th>Visibility</th></tr></thead>",
    );
    push_line(&mut body, "<tbody>");
    for (table, file_name) in doc.tables.iter().zip(table_file_names) {
        push_line(
            &mut body,
            &format!(
                "<tr><td><a href=\"{}\"><code>{}</code></a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(file_name),
                escape_html(&table.name),
                escape_html(&table.kind),
                display_html(table.description.as_deref()),
                table.fields.len(),
                table.indexes.len(),
                table.relations.len(),
                badge(&table.visibility),
            ),
        );
    }
    push_line(&mut body, "</tbody>");
    push_line(&mut body, "</table>");
    push_line(&mut body, "</section>");
    push_relationship_graph(&mut body, doc, table_file_names);
    push_source_map(&mut body, doc, table_file_names);
    push_index_asset_fields(&mut body, doc, table_file_names);
    push_index_text_fields(&mut body, doc, table_file_names);
    push_index_relations(&mut body, doc, table_file_names);
    render_page(&doc.metadata.title, &body)
}

fn push_source_map(body: &mut String, doc: &SchemaDoc, table_file_names: &[String]) {
    push_line(body, "<section id=\"source-map\">");
    push_line(body, "<h2>Source map</h2>");
    push_source_legend(body);
    if doc.source_graph.edges.is_empty() {
        push_line(body, "<p class=\"muted\">No sources.</p>");
    } else {
        push_line(body, "<div class=\"edge-list\">");
        for edge in &doc.source_graph.edges {
            push_source_edge_card(
                body,
                edge,
                table_href(&edge.table, doc, table_file_names).unwrap_or("#"),
            );
        }
        push_line(body, "</div>");
    }
    push_line(body, "</section>");
}

fn push_table_source_map(body: &mut String, doc: &SchemaDoc, table: &SchemaTableDoc) {
    push_line(body, "<section id=\"source-map\">");
    push_line(body, "<h2>Source map</h2>");
    push_source_legend(body);
    let source_edges = doc
        .source_graph
        .edges
        .iter()
        .filter(|edge| edge.table == table.name)
        .collect::<Vec<_>>();
    if source_edges.is_empty() {
        push_line(body, "<p class=\"muted\">No source declared.</p>");
    } else {
        push_line(body, "<div class=\"edge-list\">");
        for edge in source_edges {
            push_source_edge_card(body, edge, "#overview");
        }
        push_line(body, "</div>");
    }
    push_line(body, "</section>");
}

fn push_source_legend(body: &mut String) {
    push_line(body, "<div class=\"legend\" aria-label=\"Source legend\">");
    push_line(body, &badge("source node"));
    push_line(body, &badge("source → table"));
    push_line(body, &badge("override = override source"));
    push_line(body, &badge("fragment badge = active fragments"));
    push_line(body, &badge("key/value rows = ObjectTable source"));
    push_line(body, "</div>");
}

fn push_source_edge_card(body: &mut String, edge: &SchemaSourceTableEdgeDoc, table_href: &str) {
    push_line(
        body,
        &format!(
            "<article class=\"edge-card\"><span class=\"node source-node\"><code>{}</code></span><span class=\"arrow\">→</span><a class=\"node table-node\" href=\"{}\"><code>{}</code></a>{}{}{}{}{}<span class=\"muted\">rows: header {}, data {}</span></article>",
            escape_html(&edge.source_path),
            escape_html(table_href),
            escape_html(&edge.table),
            badge(&edge.provider),
            badge(&edge.role),
            badge(&edge.layout),
            source_fragments_badge(&edge.fragments),
            badge(&edge.visibility),
            edge.header_row,
            edge.data_start_row,
        ),
    );
}

fn source_fragments_badge(fragments: &[String]) -> String {
    if fragments.is_empty() {
        return String::new();
    }
    badge(&format!("fragments: {}", fragments.join(", ")))
}

fn push_index_summary_cards(body: &mut String, doc: &SchemaDoc) {
    let table_count = doc.tables.len();
    let field_count: usize = doc.tables.iter().map(|table| table.fields.len()).sum();
    let index_count: usize = doc.tables.iter().map(|table| table.indexes.len()).sum();
    let relation_count: usize = doc.tables.iter().map(|table| table.relations.len()).sum();
    let asset_count: usize = doc
        .tables
        .iter()
        .map(|table| table.asset_fields.len())
        .sum();
    let text_count: usize = doc.tables.iter().map(|table| table.text_fields.len()).sum();
    push_line(body, "<section class=\"cards\" aria-label=\"Summary\">");
    push_summary_card(body, "Tables", table_count);
    push_summary_card(body, "Fields", field_count);
    push_summary_card(body, "Indexes", index_count);
    push_summary_card(body, "Relations", relation_count);
    push_summary_card(body, "Asset fields", asset_count);
    push_summary_card(body, "Text fields", text_count);
    push_line(body, "</section>");
}

fn push_summary_card(body: &mut String, label: &str, value: usize) {
    push_line(
        body,
        &format!(
            "<div class=\"summary-card\"><strong>{}</strong><span>{}</span></div>",
            value,
            escape_html(label)
        ),
    );
}

fn push_relationship_graph(body: &mut String, doc: &SchemaDoc, table_file_names: &[String]) {
    push_line(body, "<section id=\"relationship-graph\">");
    push_line(body, "<h2>Relationship graph</h2>");
    push_relation_legend(body);
    if doc.relationship_graph.edges.is_empty() {
        push_line(body, "<p class=\"muted\">No relations.</p>");
    } else {
        push_line(body, "<div class=\"edge-list\">");
        for edge in &doc.relationship_graph.edges {
            push_relation_edge_card(
                body,
                edge,
                table_href(&edge.source_table, doc, table_file_names).unwrap_or("#"),
                table_href(&edge.target_table, doc, table_file_names).unwrap_or("#"),
            );
        }
        push_line(body, "</div>");
    }
    push_line(body, "</section>");
}

fn push_table_neighborhood(body: &mut String, table: &SchemaTableDoc) {
    push_line(body, "<section id=\"neighborhood\">");
    push_line(body, "<h2>Table neighborhood</h2>");
    push_relation_legend(body);
    if table.relations.is_empty() && table.incoming_relations.is_empty() {
        push_line(body, "<p class=\"muted\">No related tables.</p>");
    } else {
        push_line(body, "<div class=\"edge-list\">");
        for relation in &table.relations {
            let edge = SchemaRelationEdgeDoc {
                source_table: table.name.clone(),
                source_field: relation.field.clone(),
                target_table: relation.target_table.clone(),
                target_index: relation.target_index.clone(),
                target_display: relation.target.clone(),
                cardinality: relation.cardinality.clone(),
                allow_empty: relation.allow_empty,
                linked_helper: relation.linked_helper.clone(),
                visibility: relation.visibility.clone(),
                description: relation.description.clone(),
            };
            let source_href = "#overview";
            let target_href = if edge.target_table == table.name {
                "#overview".to_owned()
            } else {
                format!("{}.html", edge.target_table)
            };
            push_relation_edge_card(body, &edge, source_href, &target_href);
        }
        for edge in &table.incoming_relations {
            let source_href = if edge.source_table == table.name {
                "#overview".to_owned()
            } else {
                format!("{}.html", edge.source_table)
            };
            push_relation_edge_card(body, edge, &source_href, "#overview");
        }
        push_line(body, "</div>");
    }
    push_line(body, "</section>");
}

fn push_relation_legend(body: &mut String) {
    push_line(
        body,
        "<div class=\"legend\" aria-label=\"Relationship legend\">",
    );
    push_line(body, &badge("table node"));
    push_line(body, &badge("FK edge: source field → target index"));
    push_line(body, &badge("primary = default unique row identity"));
    push_line(body, &badge("unique = targetable index contract"));
    push_line(body, &badge("target_index = referenced index"));
    push_line(body, &badge("list-to-many = list FK"));
    push_line(body, &badge("helper = generated relation accessor"));
    push_line(body, &badge("allow_empty = optional reference"));
    push_line(body, "</div>");
}

fn push_relation_edge_card(
    body: &mut String,
    edge: &SchemaRelationEdgeDoc,
    source_href: &str,
    target_href: &str,
) {
    let helper = edge
        .linked_helper
        .as_deref()
        .map(|helper| badge(&format!("helper: {helper}")))
        .unwrap_or_default();
    let optional = edge
        .allow_empty
        .then(|| badge("allow_empty"))
        .unwrap_or_default();
    push_line(
        body,
        &format!(
            "<article class=\"edge-card\"><a class=\"node\" href=\"{}\"><code>{}</code></a><span class=\"arrow\"><code>{}</code> → target_index <code>{}</code></span><a class=\"node\" href=\"{}\"><code>{}</code></a>{}{}{}{} </article>",
            escape_html(source_href),
            escape_html(&edge.source_table),
            escape_html(&edge.source_field),
            escape_html(&edge.target_index),
            escape_html(target_href),
            escape_html(&edge.target_table),
            badge(&edge.cardinality),
            helper,
            optional,
            badge(&edge.visibility),
        ),
    );
}

fn push_asset_flow_legend(body: &mut String) {
    push_line(
        body,
        "<div class=\"legend\" aria-label=\"Asset flow legend\">",
    );
    push_line(body, &badge("asset field → asset manifest exporter"));
    push_line(body, &badge("rules describe schema contract only"));
    push_line(body, &badge("actual asset refs are not read by schema_doc"));
    push_line(body, "</div>");
}

fn push_text_flow_legend(body: &mut String) {
    push_line(
        body,
        "<div class=\"legend\" aria-label=\"Text flow legend\">",
    );
    push_line(body, &badge("text field → gettext_pot"));
    push_line(body, &badge("POT eligibility comes from schema contract"));
    push_line(
        body,
        &badge("actual text values are not read by schema_doc"),
    );
    push_line(body, "</div>");
}

fn table_href<'a>(
    table_name: &str,
    doc: &SchemaDoc,
    table_file_names: &'a [String],
) -> Option<&'a str> {
    doc.tables
        .iter()
        .position(|table| table.name == table_name)
        .and_then(|index| table_file_names.get(index))
        .map(String::as_str)
}

fn push_index_asset_fields(body: &mut String, doc: &SchemaDoc, table_file_names: &[String]) {
    if doc.tables.iter().all(|table| table.asset_fields.is_empty()) {
        return;
    }
    push_line(body, "<section id=\"asset-fields\">");
    push_line(body, "<h2>Asset fields</h2>");
    push_asset_flow_legend(body);
    push_line(body, "<table>");
    push_line(
        body,
        "<thead><tr><th>Table</th><th>Field</th><th>Asset Kind</th><th>Required</th><th>Export</th><th>Rules</th></tr></thead>",
    );
    push_line(body, "<tbody>");
    for (table, file_name) in doc.tables.iter().zip(table_file_names) {
        for asset in &table.asset_fields {
            push_line(
                body,
                &format!(
                    "<tr><td><a href=\"{}\"><code>{}</code></a></td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    escape_html(file_name),
                    escape_html(&table.name),
                    escape_html(&asset.field),
                    display_html(asset.asset_kind.as_deref()),
                    yes_no(asset.required),
                    badge(&asset.visibility),
                    display_html(asset.rules.as_deref()),
                ),
            );
        }
    }
    push_line(body, "</tbody>");
    push_line(body, "</table>");
    push_line(body, "</section>");
}

fn push_index_text_fields(body: &mut String, doc: &SchemaDoc, table_file_names: &[String]) {
    if doc.tables.iter().all(|table| table.text_fields.is_empty()) {
        return;
    }
    push_line(body, "<section id=\"text-pot-fields\">");
    push_line(body, "<h2>Text / POT fields</h2>");
    push_text_flow_legend(body);
    push_line(body, "<table>");
    push_line(
        body,
        "<thead><tr><th>Table</th><th>Field</th><th>Export</th><th>POT</th></tr></thead>",
    );
    push_line(body, "<tbody>");
    for (table, file_name) in doc.tables.iter().zip(table_file_names) {
        for text in &table.text_fields {
            push_line(
                body,
                &format!(
                    "<tr><td><a href=\"{}\"><code>{}</code></a></td><td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
                    escape_html(file_name),
                    escape_html(&table.name),
                    escape_html(&text.field),
                    badge(&text.visibility),
                    yes_no(text.enters_pot),
                ),
            );
        }
    }
    push_line(body, "</tbody>");
    push_line(body, "</table>");
    push_line(body, "</section>");
}

fn push_index_relations(body: &mut String, doc: &SchemaDoc, table_file_names: &[String]) {
    if doc.tables.iter().all(|table| table.relations.is_empty()) {
        return;
    }
    push_line(body, "<section id=\"relations\">");
    push_line(body, "<h2>Relation overview</h2>");
    push_relation_legend(body);
    push_line(body, "<table>");
    push_line(
        body,
        "<thead><tr><th>Table</th><th>Field</th><th>Target</th><th>Cardinality</th><th>Allow Empty</th><th>Export</th></tr></thead>",
    );
    push_line(body, "<tbody>");
    for (table, file_name) in doc.tables.iter().zip(table_file_names) {
        for relation in &table.relations {
            push_line(
                body,
                &format!(
                    "<tr><td><a href=\"{}\"><code>{}</code></a></td><td><code>{}</code></td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    escape_html(file_name),
                    escape_html(&table.name),
                    escape_html(&relation.field),
                    escape_html(&relation.target),
                    escape_html(&relation.cardinality),
                    yes_no(relation.allow_empty),
                    badge(&relation.visibility),
                ),
            );
        }
    }
    push_line(body, "</tbody>");
    push_line(body, "</table>");
    push_line(body, "</section>");
}

pub fn render_schema_table_html(
    doc: &SchemaDoc,
    table: &SchemaTableDoc,
    index_file: &str,
) -> String {
    let page_title = format!("{} - {}", table.name, doc.metadata.title);
    let mut body = String::new();
    push_line(
        &mut body,
        &format!(
            "<p><a href=\"{}\">← Back to index</a></p>",
            escape_html(index_file)
        ),
    );
    push_line(&mut body, "<header id=\"overview\" class=\"card hero\">");
    push_line(&mut body, &format!("<h1>{}</h1>", escape_html(&table.name)));
    if let Some(description) = &table.description {
        push_line(&mut body, &format!("<p>{}</p>", escape_html(description)));
    }
    push_metadata_bar(
        &mut body,
        &[
            ("Project", doc.metadata.project_name.as_deref(), false),
            ("Kind", Some(&table.kind), false),
            ("Source", table.source_summary.as_deref(), false),
            ("Visibility", Some(&table.visibility), false),
            ("Row export", Some(&table.row_export_summary), true),
            ("Primary index", table.primary_index.as_deref(), true),
        ],
    );
    push_quick_links(
        &mut body,
        &[
            ("Neighborhood", "neighborhood"),
            ("Source map", "source-map"),
            ("Fragments", "fragments"),
            ("Fields", "fields"),
            ("Indexes", "indexes"),
            ("Relations", "relations"),
            ("Assets", "assets"),
            ("Text/POT", "text-pot"),
            ("Validators", "validators"),
        ],
    );
    push_line(&mut body, "</header>");
    push_table_neighborhood(&mut body, table);
    push_table_source_map(&mut body, doc, table);
    push_table_fragments(&mut body, table);
    push_line(&mut body, "<section id=\"fields\">");
    push_line(&mut body, "<h2>Fields</h2>");
    push_line(&mut body, "<table>");
    push_line(
        &mut body,
        "<thead><tr><th>Field</th><th>Type</th><th>Required</th><th>Default</th><th>Export</th><th>Description</th></tr></thead>",
    );
    push_line(&mut body, "<tbody>");
    for field in &table.fields {
        push_line(
            &mut body,
            &format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td><span class=\"badge\">{}</span></td><td>{}</td></tr>",
                escape_html(&field.path),
                escape_html(&field.type_display),
                yes_no(field.required),
                display_html(field.default_display.as_deref()),
                escape_html(&field.visibility),
                display_html(field.description.as_deref()),
            ),
        );
    }
    push_line(&mut body, "</tbody>");
    push_line(&mut body, "</table>");
    push_line(&mut body, "</section>");
    push_table_indexes(&mut body, table);
    push_table_relations(&mut body, table);
    push_table_assets(&mut body, table);
    push_table_text_fields(&mut body, table);
    push_table_validators(&mut body, table);
    render_page(&page_title, &body)
}

fn push_table_fragments(body: &mut String, table: &SchemaTableDoc) {
    push_line(body, "<section id=\"fragments\">");
    push_line(body, "<h2>Fragment composition</h2>");
    let Some(fragment_graph) = &table.fragment_graph else {
        push_line(body, "<p class=\"muted\">No fragments.</p>");
        push_line(body, "</section>");
        return;
    };
    push_line(body, "<div class=\"fragment-grid\">");
    push_line(
        body,
        &format!(
            "<article class=\"fragment-card\"><h3><code>{}</code></h3>{}<p>Base fields: {}</p></article>",
            escape_html(&fragment_graph.table),
            badge("base table"),
            code_list(&fragment_graph.base_fields),
        ),
    );
    for fragment in &fragment_graph.fragments {
        let fields = fragment
            .fields
            .iter()
            .map(|field| {
                format!(
                    "{} <span class=\"muted\">{}</span>",
                    escape_html(&field.path),
                    escape_html(&field.type_display)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        push_line(
            body,
            &format!(
                "<article class=\"fragment-card\"><h3><code>{}</code></h3>{}{}{}{}<p>Fields: {}</p><p>Sources: {}</p><p>{}</p></article>",
                escape_html(&fragment.name),
                badge("fragment"),
                badge("not independent table"),
                badge(&fragment.payload),
                badge(&fragment.visibility),
                if fields.is_empty() {
                    "<span class=\"muted\">-</span>".to_owned()
                } else {
                    fields
                },
                code_list(&fragment.source_hints),
                display_html(fragment.description.as_deref()),
            ),
        );
    }
    push_line(body, "</div>");
    push_line(body, "</section>");
}

fn push_table_indexes(body: &mut String, table: &SchemaTableDoc) {
    push_line(body, "<section id=\"indexes\">");
    push_line(body, "<h2>Indexes</h2>");
    if table.indexes.is_empty() {
        push_line(body, "<p class=\"muted\">No indexes.</p>");
    } else {
        push_line(body, "<table>");
        push_line(
            body,
            "<thead><tr><th>Name</th><th>Fields</th><th>Unique</th><th>Primary</th><th>Export</th><th>Description</th></tr></thead>",
        );
        push_line(body, "<tbody>");
        for index in &table.indexes {
            push_line(
                body,
                &format!(
                    "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    escape_html(&index.name),
                    code_list(&index.fields),
                    yes_no(index.unique),
                    yes_no(index.primary),
                    badge(&index.visibility),
                    display_html(index.description.as_deref()),
                ),
            );
        }
        push_line(body, "</tbody>");
        push_line(body, "</table>");
    }
    push_line(body, "</section>");
}

fn push_table_relations(body: &mut String, table: &SchemaTableDoc) {
    push_line(body, "<section id=\"relations\">");
    push_line(body, "<h2>Relations</h2>");
    push_relation_legend(body);
    if table.relations.is_empty() {
        push_line(body, "<p class=\"muted\">No relations.</p>");
    } else {
        push_line(body, "<table>");
        push_line(
            body,
            "<thead><tr><th>Field</th><th>Target</th><th>Cardinality</th><th>Allow Empty</th><th>Linked Helper</th><th>Export</th><th>Description</th></tr></thead>",
        );
        push_line(body, "<tbody>");
        for relation in &table.relations {
            push_line(
                body,
                &format!(
                    "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    escape_html(&relation.field),
                    escape_html(&relation.target),
                    escape_html(&relation.cardinality),
                    yes_no(relation.allow_empty),
                    display_code(relation.linked_helper.as_deref()),
                    badge(&relation.visibility),
                    display_html(relation.description.as_deref()),
                ),
            );
        }
        push_line(body, "</tbody>");
        push_line(body, "</table>");
    }
    push_line(body, "</section>");
}

fn push_table_assets(body: &mut String, table: &SchemaTableDoc) {
    if table.asset_fields.is_empty() {
        return;
    }
    push_line(body, "<section id=\"assets\">");
    push_line(body, "<h2>Assets</h2>");
    push_asset_flow_legend(body);
    push_line(body, "<table>");
    push_line(
        body,
        "<thead><tr><th>Field</th><th>Asset Kind</th><th>Required</th><th>Export</th><th>Rules</th></tr></thead>",
    );
    push_line(body, "<tbody>");
    for asset in &table.asset_fields {
        push_line(
            body,
            &format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&asset.field),
                display_code(asset.asset_kind.as_deref()),
                yes_no(asset.required),
                badge(&asset.visibility),
                display_html(asset.rules.as_deref()),
            ),
        );
    }
    push_line(body, "</tbody>");
    push_line(body, "</table>");
    push_line(body, "</section>");
}

fn push_table_text_fields(body: &mut String, table: &SchemaTableDoc) {
    if table.text_fields.is_empty() {
        return;
    }
    push_line(body, "<section id=\"text-pot\">");
    push_line(body, "<h2>Text / POT</h2>");
    push_text_flow_legend(body);
    push_line(body, "<table>");
    push_line(
        body,
        "<thead><tr><th>Field</th><th>Export</th><th>POT</th></tr></thead>",
    );
    push_line(body, "<tbody>");
    for text in &table.text_fields {
        push_line(
            body,
            &format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
                escape_html(&text.field),
                badge(&text.visibility),
                yes_no(text.enters_pot),
            ),
        );
    }
    push_line(body, "</tbody>");
    push_line(body, "</table>");
    push_line(body, "</section>");
}

fn push_table_validators(body: &mut String, table: &SchemaTableDoc) {
    let validators = table
        .validators
        .iter()
        .chain(
            table
                .fields
                .iter()
                .flat_map(|field| field.validators.iter()),
        )
        .collect::<Vec<_>>();
    if validators.is_empty() {
        return;
    }
    push_line(body, "<section id=\"validators\">");
    push_line(body, "<h2>Validators</h2>");
    push_line(body, "<table>");
    push_line(
        body,
        "<thead><tr><th>Scope</th><th>Name</th><th>Summary</th><th>Severity</th><th>Description</th></tr></thead>",
    );
    push_line(body, "<tbody>");
    for validator in validators {
        push_line(
            body,
            &format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&validator.scope),
                escape_html(&validator.name),
                escape_html(&validator.summary),
                badge(&validator.severity),
                display_html(validator.description.as_deref()),
            ),
        );
    }
    push_line(body, "</tbody>");
    push_line(body, "</table>");
    push_line(body, "</section>");
}

pub(super) fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn render_page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"zh-CN\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>\n{}\n</style>\n</head>\n<body>\n<main>\n{}</main>\n</body>\n</html>\n",
        escape_html(title),
        STYLE,
        body
    )
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

fn push_metadata_bar(out: &mut String, items: &[(&str, Option<&str>, bool)]) {
    push_line(
        out,
        "<div class=\"meta-bar\" aria-label=\"Schema metadata\">",
    );
    for (key, value, muted) in items {
        let class = if *muted {
            "meta-item muted"
        } else {
            "meta-item"
        };
        push_line(
            out,
            &format!(
                "<span class=\"{}\"><span class=\"meta-key\">{}</span>: <span class=\"meta-value\">{}</span></span>",
                class,
                escape_html(key),
                display_html(*value)
            ),
        );
    }
    push_line(out, "</div>");
}

fn push_quick_links(out: &mut String, links: &[(&str, &str)]) {
    push_line(
        out,
        "<nav class=\"quick-links\" aria-label=\"Quick links\">",
    );
    for (label, anchor) in links {
        push_line(
            out,
            &format!(
                "<a href=\"#{}\">{}</a>",
                escape_html(anchor),
                escape_html(label)
            ),
        );
    }
    push_line(out, "</nav>");
}

fn display_html(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .map(escape_html)
        .unwrap_or_else(|| "<span class=\"muted\">-</span>".to_owned())
}

fn badge(value: &str) -> String {
    format!("<span class=\"badge\">{}</span>", escape_html(value))
}

fn code_list(values: &[String]) -> String {
    if values.is_empty() {
        return "<span class=\"muted\">-</span>".to_owned();
    }
    values
        .iter()
        .map(|value| format!("<code>{}</code>", escape_html(value)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_code(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .map(|value| format!("<code>{}</code>", escape_html(value)))
        .unwrap_or_else(|| "<span class=\"muted\">-</span>".to_owned())
}

fn display_list_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(", ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
