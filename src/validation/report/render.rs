// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::CF_BUILD_EXPORT_TASK_FAILED,
};

use super::{
    DesignerReportFormat, DesignerValidationIssue, DesignerValidationReport, model::severity_label,
};

fn report_render_error(message: String, current: impl Into<String>) -> ConfigForgeError {
    super::super::validation_diagnostic_error(
        CF_BUILD_EXPORT_TASK_FAILED,
        "validation.report.render",
        message,
        DiagnosticLocation::default(),
        current,
        "serializable validation report model",
        "Check validation report data for JSON serialization compatibility.",
    )
}

pub fn render_designer_validation_report(
    report: &DesignerValidationReport,
    format: DesignerReportFormat,
) -> Result<String> {
    match format {
        DesignerReportFormat::Html => Ok(render_html(report)),
        DesignerReportFormat::Markdown => Ok(render_markdown(report)),
        DesignerReportFormat::Json => serde_json::to_string_pretty(report).map_err(|source| {
            report_render_error(
                format!("failed to serialize validation report JSON: {source}"),
                "json",
            )
        }),
        DesignerReportFormat::Csv => Ok(render_csv(report)),
    }
}

fn render_markdown(report: &DesignerValidationReport) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "# {}\n\n",
        escape_markdown(&report.metadata.title)
    ));
    output.push_str(&format!("- Status: **{}**\n", report.status_label()));
    output.push_str(&format!(
        "- Target: `{}`\n",
        report.metadata.target.as_deref().unwrap_or("<full>")
    ));
    output.push_str(&format!(
        "- Target tags: `{}`\n",
        report.metadata.target_tags.join(",")
    ));
    output.push_str(&format!(
        "- Generated at: `{}`\n",
        report.metadata.generated_at
    ));
    output.push_str(&format!("- Errors: **{}**\n", report.summary.error_count));
    output.push_str(&format!(
        "- Warnings: **{}**\n\n",
        report.summary.warning_count
    ));

    output.push_str("## Summary by code\n\n");
    output.push_str("| Code | Errors | Warnings | Issues |\n| --- | ---: | ---: | --- |\n");
    for group in report.groups.iter().filter(|group| group.kind == "code") {
        output.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            escape_markdown(&group.key),
            group.error_count,
            group.warning_count,
            escape_markdown(&group.issue_ids.join(", "))
        ));
    }

    output.push_str("\n## Issues\n\n");
    output.push_str("| ID | Severity | Code | Table | Field | Source | Source row | Source column | Current | Expected | Schema | Message | Hint |\n| --- | --- | --- | --- | --- | --- | ---: | --- | --- | --- | --- | --- | --- |\n");
    for issue in &report.issues {
        output.push_str(&format!(
            "| {} | {} | `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            issue.id,
            severity_label(issue.severity),
            escape_markdown(&issue.code),
            escape_markdown(issue.table.as_deref().unwrap_or("")),
            escape_markdown(issue.field.as_deref().unwrap_or("")),
            escape_markdown(&format_issue_location(issue)),
            issue
                .source_row_number
                .map(|value| value.to_string())
                .unwrap_or_default(),
            escape_markdown(issue.source_column.as_deref().unwrap_or("")),
            escape_markdown(issue.current_value.as_deref().unwrap_or("")),
            escape_markdown(issue.expected.as_deref().unwrap_or("")),
            escape_markdown(&issue.schema_metadata.join("; ")),
            escape_markdown(&issue.message),
            escape_markdown(issue.hint.as_deref().unwrap_or("")),
        ));
    }
    output
}

fn render_csv(report: &DesignerValidationReport) -> String {
    let mut output = String::new();
    let header = [
        "id",
        "severity",
        "blocking",
        "code",
        "friendly_title",
        "message",
        "hint",
        "table",
        "field",
        "row_index",
        "source_row_number",
        "source_column",
        "source_path",
        "source_sheet",
        "current_value",
        "expected",
        "schema_metadata",
        "explanation",
        "target",
        "owner",
        "domain",
        "record_key",
        "context",
        "related_locations",
    ];
    output.push_str(&header.join(","));
    output.push('\n');
    for issue in &report.issues {
        let row = [
            issue.id.clone(),
            severity_label(issue.severity).to_owned(),
            issue.blocking.to_string(),
            issue.code.clone(),
            issue.friendly_title.clone(),
            issue.message.clone(),
            issue.hint.clone().unwrap_or_default(),
            issue.table.clone().unwrap_or_default(),
            issue.field.clone().unwrap_or_default(),
            issue
                .row_index
                .map(|value| value.to_string())
                .unwrap_or_default(),
            issue
                .source_row_number
                .map(|value| value.to_string())
                .unwrap_or_default(),
            issue.source_column.clone().unwrap_or_default(),
            issue.source_path.clone().unwrap_or_default(),
            issue.source_sheet.clone().unwrap_or_default(),
            issue.current_value.clone().unwrap_or_default(),
            issue.expected.clone().unwrap_or_default(),
            issue.schema_metadata.join("; "),
            issue.explanation.clone().unwrap_or_default(),
            issue.target.clone().unwrap_or_default(),
            issue.owner.clone(),
            issue.domain.clone(),
            issue.record_key.clone().unwrap_or_default(),
            issue
                .context
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join("; "),
            issue
                .related_locations
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        ];
        output.push_str(
            &row.iter()
                .map(|value| escape_csv(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push('\n');
    }
    output
}

fn render_html(report: &DesignerValidationReport) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
    output.push_str(&format!(
        "<title>{}</title>",
        escape_html(&report.metadata.title)
    ));
    output.push_str("<style>body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI','PingFang SC','Microsoft YaHei',sans-serif;margin:0;background:#f6f8fb;color:#1f2937;line-height:1.6}main{max-width:1200px;margin:0 auto;padding:24px}.hero,.card{background:#fff;border:1px solid #dbe4ee;border-radius:16px;box-shadow:0 10px 30px rgba(15,23,42,.08)}.hero{padding:28px;margin-bottom:18px}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:12px}.card{padding:16px}.status{display:inline-block;border-radius:999px;padding:4px 10px;font-weight:700;background:#fee2e2;color:#991b1b}.status.ok{background:#dcfce7;color:#166534}.status.warn{background:#fef3c7;color:#92400e}table{width:100%;border-collapse:collapse;background:#fff;border-radius:12px;overflow:hidden}th,td{border-bottom:1px solid #e5edf5;padding:8px;vertical-align:top;text-align:left}th{background:#f8fafc}.sev-error{color:#dc2626;font-weight:700}.sev-warning{color:#d97706;font-weight:700}code{background:#eef2f7;border-radius:4px;padding:1px 4px}</style></head><body><main>");
    let status_class = if report.summary.blocking {
        "status"
    } else if report.summary.warning_count > 0 {
        "status warn"
    } else {
        "status ok"
    };
    output.push_str(&format!(
        "<section class=\"hero\"><h1>{}</h1><p><span class=\"{}\">{}</span></p><p>Target: <code>{}</code> · Tags: <code>{}</code> · Generated: <code>{}</code></p></section>",
        escape_html(&report.metadata.title),
        status_class,
        report.status_label(),
        escape_html(report.metadata.target.as_deref().unwrap_or("<full>")),
        escape_html(&report.metadata.target_tags.join(",")),
        escape_html(&report.metadata.generated_at),
    ));
    output.push_str("<section class=\"grid\">");
    output.push_str(&format!(
        "<div class=\"card\"><h2>{}</h2><p>Errors</p></div>",
        report.summary.error_count
    ));
    output.push_str(&format!(
        "<div class=\"card\"><h2>{}</h2><p>Warnings</p></div>",
        report.summary.warning_count
    ));
    output.push_str(&format!(
        "<div class=\"card\"><h2>{}</h2><p>Tables</p></div>",
        report.summary.tables_with_issues
    ));
    output.push_str(&format!(
        "<div class=\"card\"><h2>{}</h2><p>Owners</p></div>",
        report.summary.owners_with_issues
    ));
    output.push_str("</section><h2>Issues</h2><table><thead><tr><th>ID</th><th>Severity</th><th>Code</th><th>Location</th><th>Current</th><th>Expected</th><th>Schema</th><th>Message</th><th>Hint</th></tr></thead><tbody>");
    for issue in &report.issues {
        let severity_class = match issue.severity {
            DiagnosticSeverity::Error => "sev-error",
            DiagnosticSeverity::Warning => "sev-warning",
        };
        output.push_str(&format!(
            "<tr><td>{}</td><td class=\"{}\">{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&issue.id),
            severity_class,
            severity_label(issue.severity),
            escape_html(&issue.code),
            escape_html(&format_issue_location(issue)),
            escape_html(issue.current_value.as_deref().unwrap_or("")),
            escape_html(issue.expected.as_deref().unwrap_or("")),
            escape_html(&issue.schema_metadata.join("; ")),
            escape_html(&issue.message),
            escape_html(issue.hint.as_deref().unwrap_or("")),
        ));
    }
    output.push_str("</tbody></table><h2>Groups</h2><table><thead><tr><th>Kind</th><th>Key</th><th>Errors</th><th>Warnings</th><th>Issues</th></tr></thead><tbody>");
    for group in &report.groups {
        output.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&group.kind),
            escape_html(&group.key),
            group.error_count,
            group.warning_count,
            escape_html(&group.issue_ids.join(", ")),
        ));
    }
    output.push_str("</tbody></table></main></body></html>\n");
    output
}

fn format_issue_location(issue: &DesignerValidationIssue) -> String {
    let mut parts = Vec::new();
    if let Some(sheet) = &issue.source_sheet {
        parts.push(format!("sheet `{sheet}`"));
    }
    if let Some(source_path) = &issue.source_path {
        parts.push(source_path.clone());
    }
    if let Some(table) = &issue.table {
        parts.push(format!("table {table}"));
    }
    if let Some(row) = issue.source_row_number {
        parts.push(format!("row {row}"));
    } else if let Some(row_index) = issue.row_index {
        parts.push(format!("row {}", row_index + 1));
    }
    if let Some(column) = &issue.source_column {
        parts.push(format!("column {column}"));
    }
    if let Some(field) = &issue.field {
        parts.push(format!("field {field}"));
    }
    if let Some(key) = &issue.record_key {
        parts.push(format!("record {key}"));
    }
    for (key, value) in &issue.context {
        parts.push(format!("{key}={value}"));
    }
    for related in &issue.related_locations {
        parts.push(format!("related: {related}"));
    }
    parts.join(", ")
}

fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn escape_markdown(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "<br>")
        .replace('\r', "")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
