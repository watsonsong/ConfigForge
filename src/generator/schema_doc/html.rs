// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::model::{SchemaDoc, SchemaTableDoc};

const STYLE: &str = r#"
body { margin: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: #1f2937; background: #f8fafc; }
main { max-width: 1120px; margin: 0 auto; padding: 32px; }
header { margin-bottom: 24px; }
h1, h2 { color: #111827; }
table { width: 100%; border-collapse: collapse; margin: 16px 0; background: #fff; }
th, td { border: 1px solid #d1d5db; padding: 8px 10px; text-align: left; vertical-align: top; }
th { background: #f3f4f6; }
code { background: #eef2ff; border-radius: 4px; padding: 1px 4px; }
.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 12px; margin: 16px 0; }
.badge { display: inline-block; border-radius: 999px; padding: 2px 8px; background: #e0f2fe; color: #075985; font-size: 0.85em; }
.card { background: #fff; border: 1px solid #e5e7eb; border-radius: 8px; padding: 16px; margin: 16px 0; }
.summary-card { background: #fff; border: 1px solid #e5e7eb; border-radius: 8px; padding: 14px; }
.summary-card strong { display: block; font-size: 1.6em; color: #111827; }
.summary-card span { color: #6b7280; font-size: 0.9em; }
.muted { color: #6b7280; }
a { color: #2563eb; }
"#;

pub fn render_schema_doc_index_html(doc: &SchemaDoc, table_file_names: &[String]) -> String {
    let mut body = String::new();
    push_line(&mut body, "<header>");
    push_line(
        &mut body,
        &format!("<h1>{}</h1>", escape_html(&doc.metadata.title)),
    );
    push_line(&mut body, "</header>");
    push_line(&mut body, "<section class=\"card\">");
    push_line(&mut body, "<dl>");
    push_kv(&mut body, "Project", doc.metadata.project_name.as_deref());
    push_kv(&mut body, "Target", doc.metadata.target.as_deref());
    push_kv(
        &mut body,
        "Tags",
        Some(&display_list_or_dash(&doc.metadata.target_tags)),
    );
    push_kv(&mut body, "Generated at", Some(&doc.metadata.generated_at));
    push_line(&mut body, "</dl>");
    push_line(&mut body, "</section>");
    push_index_summary_cards(&mut body, doc);
    push_line(&mut body, "<section>");
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
    push_index_asset_fields(&mut body, doc, table_file_names);
    push_index_text_fields(&mut body, doc, table_file_names);
    push_index_relations(&mut body, doc, table_file_names);
    render_page(&doc.metadata.title, &body)
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

fn push_index_asset_fields(body: &mut String, doc: &SchemaDoc, table_file_names: &[String]) {
    if doc.tables.iter().all(|table| table.asset_fields.is_empty()) {
        return;
    }
    push_line(body, "<section>");
    push_line(body, "<h2>Asset fields</h2>");
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
    push_line(body, "<section>");
    push_line(body, "<h2>Text / POT fields</h2>");
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
    push_line(body, "<section>");
    push_line(body, "<h2>Relation overview</h2>");
    push_line(body, "<table>");
    push_line(
        body,
        "<thead><tr><th>Table</th><th>Field</th><th>Target</th><th>Cardinality</th><th>Export</th></tr></thead>",
    );
    push_line(body, "<tbody>");
    for (table, file_name) in doc.tables.iter().zip(table_file_names) {
        for relation in &table.relations {
            push_line(
                body,
                &format!(
                    "<tr><td><a href=\"{}\"><code>{}</code></a></td><td><code>{}</code></td><td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
                    escape_html(file_name),
                    escape_html(&table.name),
                    escape_html(&relation.field),
                    escape_html(&relation.target),
                    escape_html(&relation.cardinality),
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
    push_line(&mut body, "<header>");
    push_line(&mut body, &format!("<h1>{}</h1>", escape_html(&table.name)));
    if let Some(description) = &table.description {
        push_line(&mut body, &format!("<p>{}</p>", escape_html(description)));
    }
    push_line(&mut body, "</header>");
    push_line(&mut body, "<section class=\"card\">");
    push_line(&mut body, "<dl>");
    push_kv(&mut body, "Project", doc.metadata.project_name.as_deref());
    push_kv(&mut body, "Kind", Some(&table.kind));
    push_kv(&mut body, "Source", table.source_summary.as_deref());
    push_kv(&mut body, "Visibility", Some(&table.visibility));
    push_kv(&mut body, "Row export", Some(&table.row_export_summary));
    push_kv(&mut body, "Primary index", table.primary_index.as_deref());
    push_line(&mut body, "</dl>");
    push_line(&mut body, "</section>");
    push_line(&mut body, "<section>");
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
    render_page(&page_title, &body)
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

fn push_kv(out: &mut String, key: &str, value: Option<&str>) {
    push_line(
        out,
        &format!(
            "<dt>{}</dt><dd>{}</dd>",
            escape_html(key),
            display_html(value)
        ),
    );
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
