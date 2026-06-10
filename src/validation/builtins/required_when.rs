// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    meta::{ExportCondition, LinkedMetaModel, TargetTags, ValidationSeverity, ValidatorMeta},
};

use super::common::{
    is_empty_value, linked_field, required_string, validate_allowed_params, validator_config_error,
};
use super::condition::RowCondition;
use crate::validation::diagnostic::ValidationIssueLocation;
use crate::validation::registry::{
    CompiledValidator, ValidatorFactory, ValidatorLocation, ValidatorScope,
};
use crate::validation::{ValidationContext, ValidationIssue};

pub struct RequiredWhenValidatorFactory;

impl ValidatorFactory for RequiredWhenValidatorFactory {
    fn kind(&self) -> &'static str {
        "required_when"
    }

    fn scope(&self) -> ValidatorScope {
        ValidatorScope::Record
    }

    fn compile<'meta>(
        &self,
        meta: &'meta ValidatorMeta,
        location: ValidatorLocation<'meta>,
        linked_meta: &'meta LinkedMetaModel<'meta>,
    ) -> Result<Box<dyn CompiledValidator<'meta> + 'meta>> {
        let ValidatorLocation::Table { table_name } = location else {
            return Err(validator_config_error(
                "required_when",
                "required_when validator must be declared on a table".to_owned(),
                "field scope",
                "table scope",
                "Move the required_when validator to [[tables.validators]].",
            ));
        };
        let table = super::common::linked_table(linked_meta, "required_when", table_name)?;
        let config = RequiredWhenConfig::compile(meta, table)?;
        Ok(Box::new(RequiredWhenValidator {
            table_name,
            config,
            severity: meta.severity,
            export_when: meta.export_when.clone(),
            validator_name: meta.name.clone().unwrap_or_else(|| self.kind().to_owned()),
            message: meta.message.clone(),
        }))
    }
}

struct RequiredWhenValidator<'meta> {
    table_name: &'meta str,
    config: RequiredWhenConfig,
    severity: ValidationSeverity,
    export_when: ExportCondition,
    validator_name: String,
    message: Option<String>,
}

impl<'meta> CompiledValidator<'meta> for RequiredWhenValidator<'meta> {
    fn kind(&self) -> &'static str {
        "required_when"
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
        if !table_view.is_field_visible(&self.config.field) {
            issues.push(self.config_issue(ctx));
            return;
        }
        for visible_row in &table_view.visible_rows {
            let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
                continue;
            };
            if !self.config.when.matches(row) {
                continue;
            }
            let value = row
                .get(&self.config.field)
                .map(String::as_str)
                .unwrap_or_default();
            if is_empty_value(value) {
                issues.push(self.missing_issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                ));
            }
        }
    }
}

impl<'meta> RequiredWhenValidator<'meta> {
    fn config_issue(&self, ctx: &ValidationContext<'meta, '_>) -> ValidationIssue {
        let mut issue = ValidationIssue::new(
            "validator.required_when_hidden_field",
            ValidationSeverity::Error,
            self.validator_name.clone(),
            format!(
                "table `{}` validator `required_when` targets field `{}` that is hidden for current target",
                self.table_name, self.config.field
            ),
            ValidationIssueLocation {
                table: Some(self.table_name.to_owned()),
                field: Some(self.config.field.clone()),
                ..Default::default()
            },
        );
        issue.target = ctx.target_tags.as_ref().map(format_target_tags);
        issue
    }

    fn missing_issue(
        &self,
        ctx: &ValidationContext<'meta, '_>,
        row_index: usize,
        source_row_number: Option<usize>,
    ) -> ValidationIssue {
        let row_number = source_row_number.unwrap_or(row_index + 1);
        let message = self.message.clone().unwrap_or_else(|| {
            format!(
                "table `{}` row {} field `{}` is required by required_when",
                self.table_name, row_number, self.config.field
            )
        });
        let mut issue = ValidationIssue::new(
            "validator.required_when_missing",
            self.severity,
            self.validator_name.clone(),
            message,
            ValidationIssueLocation {
                table: Some(self.table_name.to_owned()),
                field: Some(self.config.field.clone()),
                row_index: Some(row_index),
                source_row_number,
                ..Default::default()
            },
        );
        issue.target = ctx.target_tags.as_ref().map(format_target_tags);
        issue
    }
}

struct RequiredWhenConfig {
    field: String,
    when: RowCondition,
}

impl RequiredWhenConfig {
    fn compile(meta: &ValidatorMeta, table: &crate::meta::LinkedTableMeta<'_>) -> Result<Self> {
        validate_allowed_params("required_when", meta, &["field", "when"])?;
        let field = required_string(meta, "required_when", "field")?;
        linked_field(table, "required_when", &field)?;
        let Some(when) = meta.params.get("when") else {
            return Err(validator_config_error(
                "required_when",
                "validator `required_when` must set parameter `when`".to_owned(),
                "missing parameter",
                "when condition table",
                "Add a `when` table describing the condition for requiring this field.",
            ));
        };
        Ok(Self {
            field,
            when: RowCondition::compile("required_when", table, when)?,
        })
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
