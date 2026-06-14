// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use regex::Regex;

use crate::{
    Result,
    meta::{ExportCondition, LinkedMetaModel, TargetTags, ValidationSeverity, ValidatorMeta},
};

use super::common::{
    field_value, optional_bool, optional_string, required_string, scalar_field_kind,
    validate_allowed_params, validator_config_error,
};
use crate::validation::diagnostic::ValidationIssueLocation;
use crate::validation::registry::{
    CompiledValidator, ValidatorFactory, ValidatorLocation, ValidatorScope,
};
use crate::validation::{ValidationContext, ValidationIssue};

pub struct RegexValidatorFactory;

impl ValidatorFactory for RegexValidatorFactory {
    fn kind(&self) -> &'static str {
        "regex"
    }

    fn scope(&self) -> ValidatorScope {
        ValidatorScope::Field
    }

    fn compile<'meta>(
        &self,
        meta: &'meta ValidatorMeta,
        location: ValidatorLocation<'meta>,
        linked_meta: &'meta LinkedMetaModel<'meta>,
    ) -> Result<Box<dyn CompiledValidator<'meta> + 'meta>> {
        let ValidatorLocation::Field {
            table_name,
            field_name,
        } = location
        else {
            return Err(validator_config_error(
                "regex",
                "regex validator can only be declared on a field".to_owned(),
                "table scope",
                "field scope",
                "Move the regex validator to [[tables.fields.validators]].",
            ));
        };
        let table = super::common::linked_table(linked_meta, "regex", table_name)?;
        let field = super::common::linked_field(table, "regex", field_name)?;
        let Some(field_kind) = scalar_field_kind(field) else {
            return Err(validator_config_error(
                "regex",
                format!(
                    "table `{table_name}` field `{field_name}` validator `regex` can only be used on string, text, asset or enum fields"
                ),
                field_name,
                "string, text, asset, or enum field",
                "Attach regex to a regex-compatible scalar field.",
            ));
        };
        if !field_kind.supports_regex() {
            return Err(validator_config_error(
                "regex",
                format!(
                    "table `{table_name}` field `{field_name}` validator `regex` can only be used on string, text, asset or enum fields"
                ),
                field_name,
                "string, text, asset, or enum field",
                "Attach regex to a regex-compatible scalar field.",
            ));
        }

        let config = RegexValidatorConfig::compile(meta)?;
        Ok(Box::new(RegexValidator {
            table_name,
            field_name,
            config,
            severity: meta.severity,
            export_when: meta.export_when.clone(),
            validator_name: meta.name.clone().unwrap_or_else(|| self.kind().to_owned()),
            message: meta.message.clone(),
        }))
    }
}

struct RegexValidator<'meta> {
    table_name: &'meta str,
    field_name: &'meta str,
    config: RegexValidatorConfig,
    severity: ValidationSeverity,
    export_when: ExportCondition,
    validator_name: String,
    message: Option<String>,
}

impl<'meta> CompiledValidator<'meta> for RegexValidator<'meta> {
    fn kind(&self) -> &'static str {
        "regex"
    }

    fn validate<'data>(
        &self,
        ctx: &ValidationContext<'meta, 'data>,
        issues: &mut Vec<ValidationIssue>,
    ) {
        if !should_run(&self.export_when, ctx.target_tags.as_ref()) {
            return;
        }
        let Some(table_view) = ctx.export_view.table(self.table_name) else {
            return;
        };
        if !table_view.is_field_visible(self.field_name) {
            return;
        }

        for (visible_row_index, visible_row) in table_view.visible_rows.iter().enumerate() {
            if table_view.table.rows.get(visible_row.row_index).is_none() {
                continue;
            }
            let value = field_value(table_view, visible_row_index, self.field_name);
            let value = value.as_str();
            if value.trim().is_empty() && self.config.allow_empty {
                continue;
            }
            if !self.config.is_match(value) {
                issues.push(self.issue(ctx, visible_row.row_index, visible_row.source_row_number));
            }
        }
    }
}

impl<'meta> RegexValidator<'meta> {
    fn issue(
        &self,
        ctx: &ValidationContext<'meta, '_>,
        row_index: usize,
        source_row_number: Option<usize>,
    ) -> ValidationIssue {
        let row_number = source_row_number.unwrap_or(row_index + 1);
        let message = self.message.clone().unwrap_or_else(|| {
            format!(
                "table `{}` row {} field `{}` value does not match regex `{}`",
                self.table_name, row_number, self.field_name, self.config.pattern
            )
        });
        let mut issue = ValidationIssue::new(
            "validator.regex_mismatch",
            self.severity,
            self.validator_name.clone(),
            message,
            ValidationIssueLocation {
                table: Some(self.table_name.to_owned()),
                field: Some(self.field_name.to_owned()),
                row_index: Some(row_index),
                source_row_number,
                ..Default::default()
            },
        );
        issue.target = ctx.target_tags.as_ref().map(format_target_tags);
        issue
    }
}

#[derive(Debug, Clone, Copy)]
enum RegexMatchMode {
    Full,
    Search,
}

struct RegexValidatorConfig {
    pattern: String,
    regex: Regex,
    match_mode: RegexMatchMode,
    allow_empty: bool,
}

impl RegexValidatorConfig {
    fn compile(meta: &ValidatorMeta) -> Result<Self> {
        validate_allowed_params(
            "regex",
            meta,
            &["pattern", "case_sensitive", "allow_empty", "match_mode"],
        )?;
        let pattern = required_string(meta, "regex", "pattern")?;
        let case_sensitive = optional_bool(meta, "regex", "case_sensitive")?.unwrap_or(true);
        let allow_empty = optional_bool(meta, "regex", "allow_empty")?.unwrap_or(true);
        let match_mode = match optional_string(meta, "regex", "match_mode")?
            .unwrap_or_else(|| "full".to_owned())
            .as_str()
        {
            "full" => RegexMatchMode::Full,
            "search" => RegexMatchMode::Search,
            other => {
                return Err(validator_config_error(
                    "regex",
                    format!(
                        "validator `regex` parameter `match_mode` must be `full` or `search`, got `{other}`"
                    ),
                    other,
                    "full or search",
                    "Use one of the supported regex match modes.",
                ));
            }
        };
        let effective_pattern = match match_mode {
            RegexMatchMode::Full => format!("^(?:{pattern})$"),
            RegexMatchMode::Search => pattern.clone(),
        };
        let effective_pattern = if case_sensitive {
            effective_pattern
        } else {
            format!("(?i:{effective_pattern})")
        };
        let regex = Regex::new(&effective_pattern).map_err(|error| {
            validator_config_error(
                "regex",
                format!("validator `regex` parameter `pattern` is invalid: {error}"),
                &pattern,
                "valid Rust regex pattern",
                "Fix the regex pattern syntax.",
            )
        })?;
        Ok(Self {
            pattern,
            regex,
            match_mode,
            allow_empty,
        })
    }

    fn is_match(&self, value: &str) -> bool {
        match self.match_mode {
            RegexMatchMode::Full | RegexMatchMode::Search => self.regex.is_match(value),
        }
    }
}

fn should_run(export_when: &ExportCondition, target_tags: Option<&TargetTags>) -> bool {
    if let Some(target_tags) = target_tags {
        export_when.matches(target_tags)
    } else {
        export_when.matches(&TargetTags::empty())
    }
}

fn format_target_tags(target_tags: &TargetTags) -> String {
    let tags = target_tags.iter().collect::<Vec<_>>();
    if tags.is_empty() {
        "<empty>".to_owned()
    } else {
        tags.join(",")
    }
}
