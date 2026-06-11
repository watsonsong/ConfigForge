// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    format::{SchemaDocFormat, table_file_name},
    model::{SchemaDoc, SchemaTableDoc},
};

pub fn render_schema_doc_index_markdown(doc: &SchemaDoc) -> String {
    let mut out = String::new();
    push_line(
        &mut out,
        &format!("# {}", escape_markdown_text(&doc.metadata.title)),
    );
    push_line(&mut out, "");
    push_line(&mut out, "| Property | Value |");
    push_line(&mut out, "| --- | --- |");
    push_kv(&mut out, "Project", doc.metadata.project_name.as_deref());
    push_kv(&mut out, "Target", doc.metadata.target.as_deref());
    push_kv(
        &mut out,
        "Tags",
        Some(&display_list_or_dash(&doc.metadata.target_tags)),
    );
    push_kv(&mut out, "Generated at", Some(&doc.metadata.generated_at));
    push_kv(
        &mut out,
        "ConfigForge version",
        Some(&doc.metadata.config_forge_version),
    );
    push_line(&mut out, "");
    push_line(&mut out, "## Tables");
    push_line(&mut out, "");
    push_line(
        &mut out,
        "| Table | Kind | Description | Fields | Indexes | Relations | Visibility |",
    );
    push_line(&mut out, "| --- | --- | --- | ---: | ---: | ---: | --- |");
    for table in &doc.tables {
        let file_name = table_file_name(table, SchemaDocFormat::Markdown);
        push_line(
            &mut out,
            &format!(
                "| [{}]({}) | {} | {} | {} | {} | {} | {} |",
                escape_markdown_table_cell(&table.name),
                escape_markdown_link_target(&file_name),
                escape_markdown_table_cell(&table.kind),
                display_cell(table.description.as_deref()),
                table.fields.len(),
                table.indexes.len(),
                table.relations.len(),
                escape_markdown_table_cell(&table.visibility),
            ),
        );
    }
    out
}

pub fn render_schema_table_markdown(doc: &SchemaDoc, table: &SchemaTableDoc) -> String {
    let mut out = String::new();
    push_line(
        &mut out,
        &format!("# {}", escape_markdown_block_text(&table.name)),
    );
    push_line(&mut out, "");
    if let Some(description) = &table.description {
        push_line(&mut out, &escape_markdown_block_text(description));
        push_line(&mut out, "");
    }
    push_line(&mut out, "| Property | Value |");
    push_line(&mut out, "| --- | --- |");
    push_kv(&mut out, "Project", doc.metadata.project_name.as_deref());
    push_kv(&mut out, "Kind", Some(&table.kind));
    push_kv(&mut out, "Source", table.source_summary.as_deref());
    push_kv(&mut out, "Visibility", Some(&table.visibility));
    push_kv(&mut out, "Row export", Some(&table.row_export_summary));
    push_kv(&mut out, "Primary index", table.primary_index.as_deref());
    push_line(&mut out, "");

    push_line(&mut out, "## Fields");
    push_line(&mut out, "");
    push_line(
        &mut out,
        "| Field | Type | Required | Default | Export | Description |",
    );
    push_line(&mut out, "| --- | --- | --- | --- | --- | --- |");
    for field in &table.fields {
        push_line(
            &mut out,
            &format!(
                "| {} | {} | {} | {} | {} | {} |",
                escape_markdown_table_cell(&field.path),
                escape_markdown_table_cell(&field.type_display),
                yes_no(field.required),
                display_cell(field.default_display.as_deref()),
                escape_markdown_table_cell(&field.visibility),
                display_cell(field.description.as_deref()),
            ),
        );
    }
    push_line(&mut out, "");

    push_line(&mut out, "## Indexes");
    push_line(&mut out, "");
    if table.indexes.is_empty() {
        push_line(&mut out, "No indexes.");
    } else {
        push_line(
            &mut out,
            "| Name | Fields | Unique | Primary | Export | Description |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- | --- |");
        for index in &table.indexes {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} | {} | {} | {} |",
                    escape_markdown_table_cell(&index.name),
                    escape_markdown_table_cell(&index.fields.join(", ")),
                    yes_no(index.unique),
                    yes_no(index.primary),
                    escape_markdown_table_cell(&index.visibility),
                    display_cell(index.description.as_deref()),
                ),
            );
        }
    }
    push_line(&mut out, "");

    push_line(&mut out, "## Relations");
    push_line(&mut out, "");
    if table.relations.is_empty() {
        push_line(&mut out, "No relations.");
    } else {
        push_line(
            &mut out,
            "| Field | Target | Cardinality | Linked Helper | Export | Description |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- | --- |");
        for relation in &table.relations {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} | {} | {} | {} |",
                    escape_markdown_table_cell(&relation.field),
                    escape_markdown_table_cell(&relation.target),
                    escape_markdown_table_cell(&relation.cardinality),
                    display_cell(relation.linked_helper.as_deref()),
                    escape_markdown_table_cell(&relation.visibility),
                    display_cell(relation.description.as_deref()),
                ),
            );
        }
    }
    push_line(&mut out, "");

    if !table.asset_fields.is_empty() {
        push_line(&mut out, "## Assets");
        push_line(&mut out, "");
        push_line(
            &mut out,
            "| Field | Asset Kind | Required | Export | Rules |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- |");
        for asset in &table.asset_fields {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} | {} | {} |",
                    escape_markdown_table_cell(&asset.field),
                    display_cell(asset.asset_kind.as_deref()),
                    yes_no(asset.required),
                    escape_markdown_table_cell(&asset.visibility),
                    display_cell(asset.rules.as_deref()),
                ),
            );
        }
        push_line(&mut out, "");
    }

    if !table.text_fields.is_empty() {
        push_line(&mut out, "## Text / POT");
        push_line(&mut out, "");
        push_line(&mut out, "| Field | Export | POT |");
        push_line(&mut out, "| --- | --- | --- |");
        for text in &table.text_fields {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} |",
                    escape_markdown_table_cell(&text.field),
                    escape_markdown_table_cell(&text.visibility),
                    yes_no(text.enters_pot),
                ),
            );
        }
        push_line(&mut out, "");
    }

    if !table.validators.is_empty()
        || table
            .fields
            .iter()
            .any(|field| !field.validators.is_empty())
    {
        push_line(&mut out, "## Validators");
        push_line(&mut out, "");
        push_line(
            &mut out,
            "| Scope | Name | Summary | Severity | Description |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- |");
        for validator in table.validators.iter().chain(
            table
                .fields
                .iter()
                .flat_map(|field| field.validators.iter()),
        ) {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} | {} | {} |",
                    escape_markdown_table_cell(&validator.scope),
                    escape_markdown_table_cell(&validator.name),
                    escape_markdown_table_cell(&validator.summary),
                    escape_markdown_table_cell(&validator.severity),
                    display_cell(validator.description.as_deref()),
                ),
            );
        }
        push_line(&mut out, "");
    }

    out
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

fn push_kv(out: &mut String, key: &str, value: Option<&str>) {
    push_line(
        out,
        &format!(
            "| {} | {} |",
            escape_markdown_table_cell(key),
            display_cell(value)
        ),
    );
}

fn display_cell(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .map(escape_markdown_table_cell)
        .unwrap_or_else(|| "-".to_owned())
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

fn escape_markdown_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\r' => {}
            '\n' => escaped.push_str("<br>"),
            '\t' => escaped.push(' '),
            ch if ch.is_ascii_control() => escaped.push(' '),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_markdown_block_text(value: &str) -> String {
    escape_markdown_text(value).replace('|', "\\|")
}

fn escape_markdown_table_cell(value: &str) -> String {
    escape_markdown_text(value).replace('|', "\\|")
}

fn escape_markdown_link_target(value: &str) -> String {
    value.replace(' ', "%20").replace(')', "%29")
}
