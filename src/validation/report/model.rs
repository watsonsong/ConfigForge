// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt, path::PathBuf, str::FromStr};

use serde::Serialize;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticReport, DiagnosticSeverity, Result,
    diagnostic::code::{CF_BUILD_INVALID_CONFIG, metadata_for},
    meta::{LinkedFieldMeta, LinkedMetaModel, PrimitiveFieldType, TargetTags},
    validation::{ValidationIssue, ValidationReport},
};

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
            other => Err(report_config_error(
                format!(
                    "unknown validation report format `{other}`; expected html, markdown, json or csv"
                ),
                other,
                "html, markdown, json or csv",
            )),
        }
    }
}

fn report_config_error(
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    super::super::validation_diagnostic_error(
        CF_BUILD_INVALID_CONFIG,
        "validation.report.config",
        message,
        DiagnosticLocation::default(),
        current,
        expected,
        "Check validation report export format and output options.",
    )
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
    pub schema_version: u32,
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
    pub related_locations: Vec<DiagnosticLocation>,
    pub record_key: Option<String>,
    pub context: BTreeMap<String, String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_sheet: Option<String>,
    pub source_path: Option<String>,
    pub source_column: Option<String>,
    pub current_value: Option<String>,
    pub expected: Option<String>,
    pub schema_metadata: Vec<String>,
    pub target: Option<String>,
    pub export_visibility: String,
    pub owner: String,
    pub domain: String,
}

impl DesignerValidationReport {
    pub fn from_validation_report(
        validation_report: &ValidationReport,
        options: DesignerReportOptions,
    ) -> Self {
        let mut source_issues = validation_report.issues.clone();
        source_issues.sort_by(compare_validation_issues_for_report);

        let metadata = ReportMetadata::from_options(options);
        let issues = enrich_issues(source_issues, None, &metadata);
        let summary = summarize_issues(&issues);
        let groups = build_groups(&issues);
        Self {
            schema_version: 1,
            metadata,
            summary,
            groups,
            issues,
        }
    }

    pub fn from_validation_report_with_schema(
        validation_report: &ValidationReport,
        linked_meta: &LinkedMetaModel<'_>,
        options: DesignerReportOptions,
    ) -> Self {
        let mut source_issues = validation_report.issues.clone();
        source_issues.sort_by(compare_validation_issues_for_report);

        let metadata = ReportMetadata::from_options(options);
        let issues = enrich_issues(source_issues, Some(linked_meta), &metadata);
        let summary = summarize_issues(&issues);
        let groups = build_groups(&issues);
        Self {
            schema_version: 1,
            metadata,
            summary,
            groups,
            issues,
        }
    }

    #[allow(dead_code)]
    pub fn from_diagnostic_report(
        diagnostic_report: &DiagnosticReport<Diagnostic>,
        options: DesignerReportOptions,
    ) -> Self {
        let validation_report = ValidationReport {
            issues: diagnostic_report
                .diagnostics
                .iter()
                .map(|diagnostic| ValidationIssue {
                    related_locations: diagnostic.related_locations.clone(),
                    record_key: diagnostic.record_key.clone(),
                    context: diagnostic.context.clone(),
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

fn enrich_issues(
    source_issues: Vec<ValidationIssue>,
    linked_meta: Option<&LinkedMetaModel<'_>>,
    metadata: &ReportMetadata,
) -> Vec<DesignerValidationIssue> {
    let roots = metadata
        .project_path
        .as_deref()
        .map(std::path::Path::new)
        .map(|project| {
            let root = if project
                .extension()
                .is_some_and(|extension| extension == "toml")
            {
                project.parent().unwrap_or(project)
            } else {
                project
            };
            let absolute = if root.is_absolute() {
                root.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_default().join(root)
            };
            let absolute = crate::path_util::lexical_normalize_path(&absolute);
            let canonical = std::fs::canonicalize(&absolute).unwrap_or_else(|_| absolute.clone());
            vec![absolute, canonical]
        })
        .unwrap_or_default();
    let mut error_index = 0usize;
    let mut warning_index = 0usize;
    source_issues
        .into_iter()
        .map(|issue| {
            let severity = issue.severity;
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
            let catalog = metadata_for(&issue.code);
            let friendly_title = catalog
                .map(|info| info.title.to_owned())
                .unwrap_or_else(|| issue.message.clone());
            let explanation = issue
                .explanation
                .clone()
                .or_else(|| catalog.map(|info| info.explanation.to_owned()));
            let hint = issue
                .hint
                .clone()
                .or_else(|| catalog.and_then(|info| info.default_hint.map(str::to_owned)));
            let owner = issue
                .owner
                .clone()
                .unwrap_or_else(|| UNASSIGNED_OWNER.to_owned());
            let domain = issue
                .domain
                .clone()
                .or_else(|| catalog.map(|info| info.domain.to_owned()))
                .unwrap_or_else(|| UNCATEGORIZED_DOMAIN.to_owned());
            let source_column = source_column_label(&issue.location);
            let schema_metadata = schema_metadata_for_issue(&issue, linked_meta);
            let export_visibility = visibility_for_issue(&issue, linked_meta, metadata);
            let mut related_locations = issue.related_locations;
            for related in &mut related_locations {
                related.source_path = related
                    .source_path
                    .as_ref()
                    .map(|path| report_path(path, &roots));
            }

            DesignerValidationIssue {
                related_locations,
                record_key: issue.record_key,
                context: issue.context,
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
                source_sheet: issue.location.source_sheet,
                source_path: issue
                    .location
                    .source_path
                    .as_ref()
                    .map(|path| report_path(path, &roots).display().to_string()),
                source_column,
                current_value: issue.current_value,
                expected: issue.expected,
                schema_metadata,
                target: issue.target,
                export_visibility,
                owner,
                domain,
            }
        })
        .collect()
}

fn report_path(path: &std::path::Path, roots: &[PathBuf]) -> PathBuf {
    roots
        .iter()
        .find_map(|root| path.strip_prefix(root).ok())
        .unwrap_or(path)
        .to_path_buf()
}

fn visibility_for_issue(
    issue: &ValidationIssue,
    meta: Option<&LinkedMetaModel<'_>>,
    metadata: &ReportMetadata,
) -> String {
    let Some(meta) = meta else {
        return "not_applicable".to_owned();
    };
    let Some(table) = issue
        .location
        .table
        .as_deref()
        .and_then(|name| meta.table(name))
    else {
        return "not_applicable".to_owned();
    };
    if metadata.target.is_none() {
        return "not_applicable".to_owned();
    }
    let tags = TargetTags::new(metadata.target_tags.iter().map(String::as_str));
    if !table.meta.is_exported_for(&tags) {
        return "table_filtered".to_owned();
    }
    if let Some(path) = issue.location.field.as_deref() {
        let mut segments = path.split('.');
        let name = segments.next().unwrap_or(path);
        let mut field = table.field(name);
        if field.is_none()
            && let Some(fragment) = table.fragment(name)
        {
            if !fragment.meta.is_exported_for(&tags) {
                return "field_filtered".to_owned();
            }
            field = segments.next().and_then(|name| fragment.field(name));
        }
        let Some(mut field) = field else {
            return "not_applicable".to_owned();
        };
        loop {
            if !field.meta.is_exported_for(&tags) {
                return "field_filtered".to_owned();
            }
            let Some(segment) = segments.next() else {
                break;
            };
            let crate::meta::LinkedField::Struct(structure) = &field.linked_field else {
                return "not_applicable".to_owned();
            };
            let Some(child) = structure
                .fields
                .iter()
                .find(|child| child.meta.name == segment)
            else {
                return "not_applicable".to_owned();
            };
            field = child;
        }
    }
    "visible".to_owned()
}

fn schema_metadata_for_issue(
    issue: &ValidationIssue,
    linked_meta: Option<&LinkedMetaModel<'_>>,
) -> Vec<String> {
    let Some(linked_meta) = linked_meta else {
        return Vec::new();
    };
    let (Some(table_name), Some(field_name)) = (
        issue.location.table.as_deref(),
        issue.location.field.as_deref(),
    ) else {
        return Vec::new();
    };
    let Some(table) = linked_meta.table(table_name) else {
        return Vec::new();
    };
    let Some(field) = table.field(field_name) else {
        return Vec::new();
    };
    builtin_schema_metadata(field)
}

fn builtin_schema_metadata(field: &LinkedFieldMeta<'_>) -> Vec<String> {
    let Some(primitive) = field.primitive_type() else {
        return Vec::new();
    };
    match primitive {
        PrimitiveFieldType::Color => vec![
            format!(
                "type=color(format={})",
                field.meta.color_format.unwrap_or_default().as_str()
            ),
            format!(
                "channel={}",
                field
                    .meta
                    .color_channel
                    .map(|channel| channel.as_str())
                    .unwrap_or("<missing>")
            ),
            format!(
                "space={}",
                field.meta.color_space.unwrap_or_default().as_str()
            ),
            format!(
                "alpha={}",
                field.meta.color_alpha.unwrap_or_default().as_str()
            ),
        ],
        primitive if primitive.vector_dimension().is_some() => vec![
            format!(
                "type={}(format={})",
                primitive.as_str(),
                field.meta.vector_format.unwrap_or_default().as_str()
            ),
            format!(
                "components={}",
                field.meta.vector_component_names().join("/")
            ),
            format!("unit={}", field.meta.unit.as_deref().unwrap_or("-")),
        ],
        _ => Vec::new(),
    }
}

fn source_column_label(location: &DiagnosticLocation) -> Option<String> {
    location
        .source_column_name
        .clone()
        .or_else(|| {
            location
                .source_column_number
                .map(|column| column.to_string())
        })
        .or_else(|| location.column_index.map(|column| (column + 1).to_string()))
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
            .filter(|issue| issue.export_visibility.ends_with("_filtered"))
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

fn compare_validation_issues_for_report(
    left: &ValidationIssue,
    right: &ValidationIssue,
) -> std::cmp::Ordering {
    (
        left.severity,
        left.location.table.as_deref(),
        left.location
            .source_path
            .as_ref()
            .map(|path| path.as_os_str()),
        left.location.source_sheet.as_deref(),
        left.location.source_row_number,
        left.location.row_index,
        left.location.field.as_deref(),
        left.code.as_str(),
        left.message.as_str(),
    )
        .cmp(&(
            right.severity,
            right.location.table.as_deref(),
            right
                .location
                .source_path
                .as_ref()
                .map(|path| path.as_os_str()),
            right.location.source_sheet.as_deref(),
            right.location.source_row_number,
            right.location.row_index,
            right.location.field.as_deref(),
            right.code.as_str(),
            right.message.as_str(),
        ))
}

pub(super) fn severity_label(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
    }
}
