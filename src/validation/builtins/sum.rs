// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    meta::{ExportCondition, LinkedMetaModel, TargetTags, ValidationSeverity, ValidatorMeta},
};

use super::common::{
    linked_field, optional_bool, optional_number, required_string, scalar_field_kind,
    validate_allowed_params, validator_config_error,
};
use super::table_common::{GroupByConfig, GroupKey, GroupedRows, parse_numeric_value};
use crate::validation::diagnostic::ValidationIssueLocation;
use crate::validation::registry::{
    CompiledValidator, ValidatorFactory, ValidatorLocation, ValidatorScope,
};
use crate::validation::{ValidationContext, ValidationIssue};

pub struct SumValidatorFactory;

impl ValidatorFactory for SumValidatorFactory {
    fn kind(&self) -> &'static str {
        "sum"
    }

    fn scope(&self) -> ValidatorScope {
        ValidatorScope::Table
    }

    fn compile<'meta>(
        &self,
        meta: &'meta ValidatorMeta,
        location: ValidatorLocation<'meta>,
        linked_meta: &'meta LinkedMetaModel<'meta>,
    ) -> Result<Box<dyn CompiledValidator<'meta> + 'meta>> {
        let ValidatorLocation::Table { table_name } = location else {
            return Err(validator_config_error(
                "sum",
                "sum validator must be declared on a table".to_owned(),
                "field scope",
                "table scope",
                "Move the sum validator to [[tables.validators]].",
            ));
        };
        let table = super::common::linked_table(linked_meta, "sum", table_name)?;
        let config = SumConfig::compile(meta, table)?;
        Ok(Box::new(SumValidator {
            table_name,
            config,
            severity: meta.severity,
            export_when: meta.export_when.clone(),
            validator_name: meta.name.clone().unwrap_or_else(|| self.kind().to_owned()),
            message: meta.message.clone(),
        }))
    }
}

struct SumValidator<'meta> {
    table_name: &'meta str,
    config: SumConfig,
    severity: ValidationSeverity,
    export_when: ExportCondition,
    validator_name: String,
    message: Option<String>,
}

impl<'meta> CompiledValidator<'meta> for SumValidator<'meta> {
    fn kind(&self) -> &'static str {
        "sum"
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

        let grouped_rows = GroupedRows::from_table_view(table_view, &self.config.group_by);
        for (group_key, rows) in grouped_rows.iter() {
            let mut actual = 0.0;
            for grouped_row in rows {
                let Some(row) = table_view.table.rows.get(grouped_row.row_index) else {
                    continue;
                };
                let value = row
                    .get(&self.config.field)
                    .map(String::as_str)
                    .unwrap_or_default();
                match parse_numeric_value("sum", &self.config.field, value, self.config.skip_empty)
                {
                    Ok(Some(value)) => actual += value,
                    Ok(None) => {}
                    Err(error) => {
                        issues.push(self.issue(
                            ctx,
                            group_key,
                            "validator.sum_mismatch",
                            actual,
                            error.to_string(),
                        ));
                        continue;
                    }
                }
            }

            if let Some(equals) = self.config.equals {
                if (actual - equals).abs() > self.config.epsilon {
                    issues.push(self.issue(
                        ctx,
                        group_key,
                        "validator.sum_mismatch",
                        actual,
                        format!("expected sum `{equals}`"),
                    ));
                }
                continue;
            }
            if let Some(min) = self.config.min {
                if actual + self.config.epsilon < min {
                    issues.push(self.issue(
                        ctx,
                        group_key,
                        "validator.sum_too_small",
                        actual,
                        format!("expected sum at least `{min}`"),
                    ));
                    continue;
                }
            }
            if let Some(max) = self.config.max {
                if actual - self.config.epsilon > max {
                    issues.push(self.issue(
                        ctx,
                        group_key,
                        "validator.sum_too_large",
                        actual,
                        format!("expected sum at most `{max}`"),
                    ));
                }
            }
        }
    }
}

impl<'meta> SumValidator<'meta> {
    fn issue(
        &self,
        ctx: &ValidationContext<'meta, '_>,
        group_key: &GroupKey,
        code: &'static str,
        actual: f64,
        violation: String,
    ) -> ValidationIssue {
        let group_label = group_key.label();
        let message = self.message.clone().unwrap_or_else(|| {
            format!(
                "table `{}` group `{}` field `{}` sum {} actual `{actual}`",
                self.table_name, group_label, self.config.field, violation
            )
        });
        let mut issue = ValidationIssue::new(
            code,
            self.severity,
            self.validator_name.clone(),
            message,
            ValidationIssueLocation {
                table: Some(self.table_name.to_owned()),
                field: Some(format!("{} group {}", self.config.field, group_label)),
                ..Default::default()
            },
        );
        issue.target = ctx.target_tags.as_ref().map(format_target_tags);
        issue
    }
}

struct SumConfig {
    field: String,
    group_by: GroupByConfig,
    equals: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    epsilon: f64,
    skip_empty: bool,
}

impl SumConfig {
    fn compile(meta: &ValidatorMeta, table: &crate::meta::LinkedTableMeta<'_>) -> Result<Self> {
        validate_allowed_params(
            "sum",
            meta,
            &[
                "field",
                "group_by",
                "equals",
                "min",
                "max",
                "epsilon",
                "skip_empty",
            ],
        )?;
        let field = required_string(meta, "sum", "field")?;
        let field_meta = linked_field(table, "sum", &field)?;
        let Some(field_kind) = scalar_field_kind(field_meta) else {
            return Err(validator_config_error(
                "sum",
                format!(
                    "table `{}` validator `sum` field `{field}` must be scalar",
                    table.meta.name
                ),
                &field,
                "numeric scalar field",
                "Choose an int or float field for sum.",
            ));
        };
        if !field_kind.is_numeric() {
            return Err(validator_config_error(
                "sum",
                format!(
                    "table `{}` validator `sum` field `{field}` must be int or float",
                    table.meta.name
                ),
                &field,
                "int or float field",
                "Choose a numeric field for sum.",
            ));
        }

        let equals = optional_number(meta, "sum", "equals")?;
        let min = optional_number(meta, "sum", "min")?;
        let max = optional_number(meta, "sum", "max")?;
        if equals.is_some() && (min.is_some() || max.is_some()) {
            return Err(validator_config_error(
                "sum",
                "validator `sum` parameter `equals` cannot be combined with `min` or `max`"
                    .to_owned(),
                "equals with min/max",
                "equals alone, or min/max without equals",
                "Remove equals or remove min/max from the validator config.",
            ));
        }
        if equals.is_none() && min.is_none() && max.is_none() {
            return Err(validator_config_error(
                "sum",
                "validator `sum` must set `equals` or at least one of `min`/`max`".to_owned(),
                "missing sum constraint",
                "equals, min, or max",
                "Configure a target sum or an allowed sum range.",
            ));
        }
        if let (Some(min), Some(max)) = (min, max) {
            if min > max {
                return Err(validator_config_error(
                    "sum",
                    "validator `sum` parameter `min` must be less than or equal to `max`"
                        .to_owned(),
                    format!("min={min}, max={max}"),
                    "min less than or equal to max",
                    "Lower min or raise max so the bounds are satisfiable.",
                ));
            }
        }
        let epsilon = optional_number(meta, "sum", "epsilon")?.unwrap_or(0.0);
        if epsilon < 0.0 {
            return Err(validator_config_error(
                "sum",
                "validator `sum` parameter `epsilon` must be non-negative".to_owned(),
                epsilon.to_string(),
                "non-negative number",
                "Use zero or a positive tolerance value.",
            ));
        }

        Ok(Self {
            field,
            group_by: GroupByConfig::compile(meta, table, "sum")?,
            equals,
            min,
            max,
            epsilon,
            skip_empty: optional_bool(meta, "sum", "skip_empty")?.unwrap_or(false),
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
