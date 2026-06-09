// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt, path::PathBuf, str::FromStr};

use serde::Serialize;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticReport, DiagnosticSeverity, Result, meta::TargetTags,
};

use super::{ValidationIssue, ValidationReport};

const UNASSIGNED_OWNER: &str = "<unassigned>";
const UNCATEGORIZED_DOMAIN: &str = "<uncategorized>";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignerReportFormat {
    Html,
    Markdown,
    Json,
    Csv,
}

impl DesignerReportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Markdown => "md",
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }
}

impl fmt::Display for DesignerReportFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Html => "html",
            Self::Markdown => "markdown",
            Self::Json => "json",
            Self::Csv => "csv",
        })
    }
}

impl FromStr for DesignerReportFormat {
    type Err = ConfigForgeError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "html" => Ok(Self::Html),
            "markdown" | "md" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            "csv" => Ok(Self::Csv),
            other => Err(ConfigForgeError::Validation(format!(
                "unknown validation report format `{other}`; expected html, markdown, json or csv"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignerReportOptions {
    pub title: Option<String>,
    pub project_name: Option<String>,
    pub project_path: Option<PathBuf>,
    pub target: Option<String>,
    pub target_tags: TargetTags,
    pub generated_at: String,
    pub input_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReportExportConfig {
    pub format: DesignerReportFormat,
    pub output: PathBuf,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DesignerValidationReport {
    pub metadata: ReportMetadata,
    pub summary: ReportSummary,
    pub groups: Vec<IssueGroup>,
    pub issues: Vec<DesignerValidationIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportMetadata {
    pub title: String,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub target: Option<String>,
    pub target_tags: Vec<String>,
    pub generated_at: String,
    pub config_forge_version: String,
    pub input_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportSummary {
    pub error_count: usize,
    pub warning_count: usize,
    pub blocking: bool,
    pub tables_with_issues: usize,
    pub owners_with_issues: usize,
    pub filtered_context_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IssueGroup {
    pub kind: String,
    pub key: String,
    pub error_count: usize,
    pub warning_count: usize,
    pub issue_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DesignerValidationIssue {
    pub id: String,
    pub severity: DiagnosticSeverity,
    pub blocking: bool,
    pub code: String,
    pub validator: String,
    pub message: String,
    pub friendly_title: String,
    pub explanation: Option<String>,
    pub hint: Option<String>,
    pub table: Option<String>,
    pub field: Option<String>,
    pub row_index: Option<usize>,
    pub source_row_number: Option<usize>,
    pub source_path: Option<String>,
    pub source_column: Option<String>,
    pub current_value: Option<String>,
    pub expected: Option<String>,
    pub target: Option<String>,
    pub export_visibility: String,
    pub owner: String,
    pub domain: String,
}

#[derive(Debug, Clone, Copy)]
struct ReportCodeInfo {
    friendly_title: &'static str,
    explanation: &'static str,
    hint: &'static str,
}

impl DesignerValidationReport {
    pub fn from_validation_report(
        validation_report: &ValidationReport,
        options: DesignerReportOptions,
    ) -> Self {
        let mut source_issues = validation_report.issues.clone();
        source_issues.sort_by(compare_validation_issues_for_report);

        let metadata = ReportMetadata::from_options(options);
        let issues = enrich_issues(source_issues);
        let summary = summarize_issues(&issues);
        let groups = build_groups(&issues);
        Self {
            metadata,
            summary,
            groups,
            issues,
        }
    }

    pub fn from_diagnostic_report(
        diagnostic_report: &DiagnosticReport<Diagnostic>,
        options: DesignerReportOptions,
    ) -> Self {
        let validation_report = ValidationReport {
            issues: diagnostic_report
                .diagnostics
                .iter()
                .map(|diagnostic| ValidationIssue {
                    code: diagnostic.code.clone(),
                    severity: diagnostic.severity,
                    message: diagnostic.message.clone(),
                    location: diagnostic.location.clone(),
                    validator: diagnostic
                        .source
                        .clone()
                        .unwrap_or_else(|| "diagnostic".to_owned()),
                    target: diagnostic.target.clone(),
                    hint: diagnostic.hint.clone(),
                    current_value: diagnostic.current_value.clone(),
                    expected: diagnostic.expected.clone(),
                    explanation: diagnostic.explanation.clone(),
                    domain: diagnostic.domain.clone(),
                    owner: diagnostic.owner.clone(),
                })
                .collect(),
        };
        Self::from_validation_report(&validation_report, options)
    }

    pub fn status_label(&self) -> &'static str {
        if self.summary.blocking {
            "BLOCKED"
        } else if self.summary.warning_count > 0 {
            "PASS WITH WARNINGS"
        } else {
            "PASS"
        }
    }
}

impl ReportMetadata {
    fn from_options(options: DesignerReportOptions) -> Self {
        let target_label = options.target.as_deref().unwrap_or("<full>");
        let title = options.title.unwrap_or_else(|| {
            let project = options.project_name.as_deref().unwrap_or("ConfigForge");
            format!("{project} validation report: {target_label}")
        });
        Self {
            title,
            project_name: options.project_name,
            project_path: options
                .project_path
                .as_ref()
                .map(|path| path.display().to_string()),
            target: options.target,
            target_tags: options.target_tags.iter().map(str::to_owned).collect(),
            generated_at: options.generated_at,
            config_forge_version: env!("CARGO_PKG_VERSION").to_owned(),
            input_root: options
                .input_root
                .as_ref()
                .map(|path| path.display().to_string()),
        }
    }
}

pub fn render_designer_validation_report(
    report: &DesignerValidationReport,
    format: DesignerReportFormat,
) -> Result<String> {
    match format {
        DesignerReportFormat::Html => Ok(render_html(report)),
        DesignerReportFormat::Markdown => Ok(render_markdown(report)),
        DesignerReportFormat::Json => serde_json::to_string_pretty(report).map_err(|source| {
            ConfigForgeError::Validation(format!(
                "failed to serialize validation report JSON: {source}"
            ))
        }),
        DesignerReportFormat::Csv => Ok(render_csv(report)),
    }
}

fn enrich_issues(source_issues: Vec<ValidationIssue>) -> Vec<DesignerValidationIssue> {
    let mut error_index = 0usize;
    let mut warning_index = 0usize;
    source_issues
        .into_iter()
        .map(|issue| {
            let severity = DiagnosticSeverity::from(issue.severity);
            let id = match severity {
                DiagnosticSeverity::Error => {
                    error_index += 1;
                    format!("E{error_index:04}")
                }
                DiagnosticSeverity::Warning => {
                    warning_index += 1;
                    format!("W{warning_index:04}")
                }
            };
            let catalog = code_catalog(&issue.code);
            let friendly_title = catalog
                .map(|info| info.friendly_title.to_owned())
                .unwrap_or_else(|| issue.message.clone());
            let explanation = issue
                .explanation
                .clone()
                .or_else(|| catalog.map(|info| info.explanation.to_owned()));
            let hint = issue
                .hint
                .clone()
                .or_else(|| catalog.map(|info| info.hint.to_owned()));
            let owner = issue
                .owner
                .clone()
                .unwrap_or_else(|| UNASSIGNED_OWNER.to_owned());
            let domain = issue
                .domain
                .clone()
                .unwrap_or_else(|| UNCATEGORIZED_DOMAIN.to_owned());

            DesignerValidationIssue {
                id,
                severity,
                blocking: severity == DiagnosticSeverity::Error,
                code: issue.code,
                validator: issue.validator,
                message: issue.message,
                friendly_title,
                explanation,
                hint,
                table: issue.location.table,
                field: issue.location.field,
                row_index: issue.location.row_index,
                source_row_number: issue.location.source_row_number,
                source_path: issue
                    .location
                    .source_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                source_column: None,
                current_value: issue.current_value,
                expected: issue.expected,
                target: issue.target,
                export_visibility: "visible".to_owned(),
                owner,
                domain,
            }
        })
        .collect()
}

fn summarize_issues(issues: &[DesignerValidationIssue]) -> ReportSummary {
    let error_count = issues
        .iter()
        .filter(|issue| issue.severity == DiagnosticSeverity::Error)
        .count();
    let warning_count = issues.len() - error_count;
    let tables = issues
        .iter()
        .filter_map(|issue| issue.table.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    let owners = issues
        .iter()
        .map(|issue| issue.owner.as_str())
        .filter(|owner| *owner != UNASSIGNED_OWNER)
        .collect::<std::collections::BTreeSet<_>>();
    ReportSummary {
        error_count,
        warning_count,
        blocking: error_count > 0,
        tables_with_issues: tables.len(),
        owners_with_issues: owners.len(),
        filtered_context_count: issues
            .iter()
            .filter(|issue| issue.export_visibility != "visible")
            .count(),
    }
}

fn build_groups(issues: &[DesignerValidationIssue]) -> Vec<IssueGroup> {
    let mut groups = Vec::new();
    append_groups(&mut groups, "severity", issues, |issue| {
        severity_label(issue.severity).to_owned()
    });
    append_groups(&mut groups, "table", issues, |issue| {
        issue
            .table
            .clone()
            .unwrap_or_else(|| "<unknown>".to_owned())
    });
    append_groups(&mut groups, "owner", issues, |issue| issue.owner.clone());
    append_groups(&mut groups, "domain", issues, |issue| issue.domain.clone());
    append_groups(&mut groups, "code", issues, |issue| issue.code.clone());
    groups
}

fn append_groups(
    groups: &mut Vec<IssueGroup>,
    kind: &str,
    issues: &[DesignerValidationIssue],
    key_for: impl Fn(&DesignerValidationIssue) -> String,
) {
    let mut by_key: BTreeMap<String, Vec<&DesignerValidationIssue>> = BTreeMap::new();
    for issue in issues {
        by_key.entry(key_for(issue)).or_default().push(issue);
    }
    for (key, issues) in by_key {
        groups.push(IssueGroup {
            kind: kind.to_owned(),
            key,
            error_count: issues
                .iter()
                .filter(|issue| issue.severity == DiagnosticSeverity::Error)
                .count(),
            warning_count: issues
                .iter()
                .filter(|issue| issue.severity == DiagnosticSeverity::Warning)
                .count(),
            issue_ids: issues.iter().map(|issue| issue.id.clone()).collect(),
        });
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
    output.push_str("| ID | Severity | Code | Table | Field | Source row | Message | Hint |\n| --- | --- | --- | --- | --- | ---: | --- | --- |\n");
    for issue in &report.issues {
        output.push_str(&format!(
            "| {} | {} | `{}` | {} | {} | {} | {} | {} |\n",
            issue.id,
            severity_label(issue.severity),
            escape_markdown(&issue.code),
            escape_markdown(issue.table.as_deref().unwrap_or("")),
            escape_markdown(issue.field.as_deref().unwrap_or("")),
            issue
                .source_row_number
                .map(|value| value.to_string())
                .unwrap_or_default(),
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
        "source_path",
        "target",
        "owner",
        "domain",
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
            issue.source_path.clone().unwrap_or_default(),
            issue.target.clone().unwrap_or_default(),
            issue.owner.clone(),
            issue.domain.clone(),
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
    output.push_str("</section><h2>Issues</h2><table><thead><tr><th>ID</th><th>Severity</th><th>Code</th><th>Location</th><th>Message</th><th>Hint</th></tr></thead><tbody>");
    for issue in &report.issues {
        let severity_class = match issue.severity {
            DiagnosticSeverity::Error => "sev-error",
            DiagnosticSeverity::Warning => "sev-warning",
        };
        output.push_str(&format!(
            "<tr><td>{}</td><td class=\"{}\">{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&issue.id),
            severity_class,
            severity_label(issue.severity),
            escape_html(&issue.code),
            escape_html(&format_issue_location(issue)),
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
    if let Some(field) = &issue.field {
        parts.push(format!("field {field}"));
    }
    parts.join(", ")
}

fn compare_validation_issues_for_report(
    left: &ValidationIssue,
    right: &ValidationIssue,
) -> std::cmp::Ordering {
    (
        DiagnosticSeverity::from(left.severity),
        left.location.table.as_deref(),
        left.location
            .source_path
            .as_ref()
            .map(|path| path.as_os_str()),
        left.location.source_row_number,
        left.location.row_index,
        left.location.field.as_deref(),
        left.code.as_str(),
        left.message.as_str(),
    )
        .cmp(&(
            DiagnosticSeverity::from(right.severity),
            right.location.table.as_deref(),
            right
                .location
                .source_path
                .as_ref()
                .map(|path| path.as_os_str()),
            right.location.source_row_number,
            right.location.row_index,
            right.location.field.as_deref(),
            right.code.as_str(),
            right.message.as_str(),
        ))
}

fn code_catalog(code: &str) -> Option<ReportCodeInfo> {
    let info = match code {
        "field.range_violation" | "validator.range_violation" => ReportCodeInfo {
            friendly_title: "数值超出允许范围",
            explanation: "该字段值不满足 range validator 配置的上下界。",
            hint: "检查当前值、min/max/exclusive_min/exclusive_max 配置，确认数据是否填错或规则是否需要调整。",
        },
        "validator.regex_mismatch" => ReportCodeInfo {
            friendly_title: "文本格式不符合命名规则",
            explanation: "该字段值没有匹配 regex validator 配置的正则表达式。",
            hint: "按规则调整字段文本，或确认 pattern/match_mode/case_sensitive 配置是否正确。",
        },
        "validator.string_too_short" => ReportCodeInfo {
            friendly_title: "文本长度过短",
            explanation: "该字段文本长度小于 string_length validator 配置的最小长度。",
            hint: "补充文本内容，或调整 min / exact 配置。",
        },
        "validator.string_too_long" => ReportCodeInfo {
            friendly_title: "文本长度过长",
            explanation: "该字段文本长度大于 string_length validator 配置的最大长度。",
            hint: "缩短文本内容，或调整 max / exact 配置。",
        },
        "validator.string_length_mismatch" => ReportCodeInfo {
            friendly_title: "文本长度不等于期望值",
            explanation: "该字段文本长度不等于 string_length validator 配置的 exact 长度。",
            hint: "调整文本长度，或确认 exact 配置是否符合需求。",
        },
        "validator.required_when_missing" => ReportCodeInfo {
            friendly_title: "条件满足时缺少必填字段",
            explanation: "required_when validator 的 when 条件已满足，但目标字段为空。",
            hint: "补充目标字段，或检查 when 条件与字段可见性配置。",
        },
        "validator.compare_failed" => ReportCodeInfo {
            friendly_title: "字段间比较关系不成立",
            explanation: "compare validator 检查的两个字段不满足配置的比较操作。",
            hint: "检查左右字段值，确认 op 方向和空值策略是否正确。",
        },
        "validator.compare_operand_missing" => ReportCodeInfo {
            friendly_title: "字段比较缺少操作数",
            explanation: "compare validator 需要参与比较的字段值，但当前行缺少对应值。",
            hint: "补齐参与比较的字段，或启用 skip_if_either_empty。",
        },
        "validator.sum_mismatch" => ReportCodeInfo {
            friendly_title: "分组总和不等于期望值",
            explanation: "sum validator 计算出的字段总和与 equals 配置不一致。",
            hint: "检查该组每行数值、group_by 字段和 epsilon 配置。",
        },
        "validator.sum_too_small" => ReportCodeInfo {
            friendly_title: "分组总和过小",
            explanation: "sum validator 计算出的字段总和小于 min 配置。",
            hint: "增加该组数值，或检查 min / group_by 配置。",
        },
        "validator.sum_too_large" => ReportCodeInfo {
            friendly_title: "分组总和过大",
            explanation: "sum validator 计算出的字段总和大于 max 配置。",
            hint: "降低该组数值，或检查 max / group_by 配置。",
        },
        "validator.monotonic_violation" => ReportCodeInfo {
            friendly_title: "序列单调性被破坏",
            explanation: "monotonic validator 按 order_by 排序后发现字段值不满足递增/递减规则。",
            hint: "检查当前行和上一行的排序键与字段值，确认 direction/strict 配置。",
        },
        "validator.monotonic_duplicate_order_key" => ReportCodeInfo {
            friendly_title: "排序键重复",
            explanation: "monotonic validator 遇到重复 order_by 值，且未允许 skip_equal_order_key。",
            hint: "修正重复排序键，或确认是否应该启用 skip_equal_order_key。",
        },
        "field.required" => ReportCodeInfo {
            friendly_title: "必填字段缺失",
            explanation: "字段声明 required 且没有 default，但当前数据未提供值。",
            hint: "补齐该字段，或在 Meta 中配置 default / 调整 required。",
        },
        "field.type" => ReportCodeInfo {
            friendly_title: "字段类型解析失败",
            explanation: "源数据无法按 Meta 字段类型解析。",
            hint: "检查单元格内容是否符合字段类型、枚举值或复合值格式。",
        },
        "field.enum" => ReportCodeInfo {
            friendly_title: "枚举值非法",
            explanation: "字段值不在 Meta 声明的 enum values 中。",
            hint: "改为合法枚举值，或确认 Meta enum 是否需要新增值。",
        },
        "field.unique" | "index.unique" => ReportCodeInfo {
            friendly_title: "唯一性约束冲突",
            explanation: "字段或索引要求唯一，但当前数据存在重复 key。",
            hint: "修改重复行的 key 字段，或确认 unique/index 配置。",
        },
        "foreign_key.missing" | "foreign_key.invalid" => ReportCodeInfo {
            friendly_title: "外键引用无效",
            explanation: "当前字段引用的目标表记录不存在或不满足 target-aware 外键规则。",
            hint: "检查引用值、目标表主键/唯一索引，以及当前 target 下目标记录是否可见。",
        },
        "asset.reference" => ReportCodeInfo {
            friendly_title: "资源引用无效",
            explanation: "asset 字段无法通过 asset_kind 和 search_paths 解析到合法资源。",
            hint: "检查资源路径、文件是否存在、asset_kind 规则和 target tags。",
        },
        _ => return None,
    };
    Some(info)
}

fn severity_label(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagnosticLocation, meta::ValidationSeverity};

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
}
