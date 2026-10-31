// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    meta::{ExportCondition, LinkedMetaModel, TargetTags, ValidationSeverity, ValidatorMeta},
};

use super::common::{
    field_value, optional_bool, optional_string, optional_usize, scalar_field_kind,
    validate_allowed_params, validator_config_error,
};
use crate::validation::diagnostic::ValidationIssueLocation;
use crate::validation::registry::{
    CompiledValidator, ValidatorFactory, ValidatorLocation, ValidatorScope,
};
use crate::validation::{ValidationContext, ValidationIssue};

pub struct StringLengthValidatorFactory;

impl ValidatorFactory for StringLengthValidatorFactory {
    fn kind(&self) -> &'static str {
        "string_length"
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
                "string_length",
                "string_length validator can only be declared on a field".to_owned(),
                "table scope",
                "field scope",
                "Move the string_length validator to [[tables.fields.validators]].",
            ));
        };
        let table = super::common::linked_table(linked_meta, "string_length", table_name)?;
        let field = super::common::linked_field(table, "string_length", field_name)?;
        let Some(field_kind) = scalar_field_kind(field) else {
            return Err(validator_config_error(
                "string_length",
                format!(
                    "table `{table_name}` field `{field_name}` validator `string_length` can only be used on string, text or asset fields"
                ),
                field_name,
                "string, text, or asset field",
                "Attach string_length to a string-like scalar field.",
            ));
        };
        if !field_kind.is_string_like() {
            return Err(validator_config_error(
                "string_length",
                format!(
                    "table `{table_name}` field `{field_name}` validator `string_length` can only be used on string, text or asset fields"
                ),
                field_name,
                "string, text, or asset field",
                "Attach string_length to a string-like scalar field.",
            ));
        }

        let config = StringLengthValidatorConfig::compile(meta)?;
        Ok(Box::new(StringLengthValidator {
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

struct StringLengthValidator<'meta> {
    table_name: &'meta str,
    field_name: &'meta str,
    config: StringLengthValidatorConfig,
    severity: ValidationSeverity,
    export_when: ExportCondition,
    validator_name: String,
    message: Option<String>,
}

impl<'meta> CompiledValidator<'meta> for StringLengthValidator<'meta> {
    fn kind(&self) -> &'static str {
        "string_length"
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
            let raw_value = field_value(table_view, visible_row_index, self.field_name);
            let raw_value = raw_value.as_str();
            if raw_value.trim().is_empty() && self.config.allow_empty {
                continue;
            }
            let value = if self.config.trim_before_count {
                raw_value.trim()
            } else {
                raw_value
            };
            let actual = self.config.unit.len(value);
            if let Some(exact) = self.config.exact {
                if actual != exact {
                    issues.push(self.issue(
                        ctx,
                        visible_row.row_index,
                        visible_row.source_row_number,
                        "validator.string_length_mismatch",
                        format!("length is `{actual}`, expected exactly `{exact}`"),
                    ));
                }
                continue;
            }
            if let Some(min) = self.config.min
                && actual < min
            {
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    "validator.string_too_short",
                    format!("length is `{actual}`, below min `{min}`"),
                ));
                continue;
            }
            if let Some(max) = self.config.max
                && actual > max
            {
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    "validator.string_too_long",
                    format!("length is `{actual}`, exceeds max `{max}`"),
                ));
            }
        }
    }
}

impl<'meta> StringLengthValidator<'meta> {
    fn issue(
        &self,
        ctx: &ValidationContext<'meta, '_>,
        row_index: usize,
        source_row_number: Option<usize>,
        code: &'static str,
        violation: String,
    ) -> ValidationIssue {
        let row_number = source_row_number.unwrap_or(row_index + 1);
        let message = self.message.clone().unwrap_or_else(|| {
            format!(
                "table `{}` row {} field `{}` {}",
                self.table_name, row_number, self.field_name, violation
            )
        });
        let mut issue = ValidationIssue::new(
            code,
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
enum StringLengthUnit {
    UnicodeScalar,
    Utf8Bytes,
}

impl StringLengthUnit {
    fn len(self, value: &str) -> usize {
        match self {
            Self::UnicodeScalar => value.chars().count(),
            Self::Utf8Bytes => value.len(),
        }
    }
}

struct StringLengthValidatorConfig {
    min: Option<usize>,
    max: Option<usize>,
    exact: Option<usize>,
    allow_empty: bool,
    unit: StringLengthUnit,
    trim_before_count: bool,
}

impl StringLengthValidatorConfig {
    fn compile(meta: &ValidatorMeta) -> Result<Self> {
        validate_allowed_params(
            "string_length",
            meta,
            &[
                "min",
                "max",
                "exact",
                "allow_empty",
                "unit",
                "trim_before_count",
            ],
        )?;
        let min = optional_usize(meta, "string_length", "min")?;
        let max = optional_usize(meta, "string_length", "max")?;
        let exact = optional_usize(meta, "string_length", "exact")?;
        if exact.is_some() && (min.is_some() || max.is_some()) {
            return Err(validator_config_error(
                "string_length",
                "validator `string_length` parameter `exact` cannot be used with `min` or `max`"
                    .to_owned(),
                "exact with min/max",
                "exact alone, or min/max without exact",
                "Remove exact or remove min/max from the validator config.",
            ));
        }
        if exact.is_none() && min.is_none() && max.is_none() {
            return Err(validator_config_error(
                "string_length",
                "validator `string_length` must set at least one of `min`, `max` or `exact`"
                    .to_owned(),
                "missing bounds",
                "min, max, or exact",
                "Configure at least one length bound.",
            ));
        }
        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            return Err(validator_config_error(
                "string_length",
                "validator `string_length` has min greater than max".to_owned(),
                format!("min={min}, max={max}"),
                "min less than or equal to max",
                "Lower min or raise max so the bounds are satisfiable.",
            ));
        }
        let unit = match optional_string(meta, "string_length", "unit")?
            .unwrap_or_else(|| "unicode_scalar".to_owned())
            .as_str()
        {
            "unicode_scalar" => StringLengthUnit::UnicodeScalar,
            "utf8_bytes" => StringLengthUnit::Utf8Bytes,
            other => {
                return Err(validator_config_error(
                    "string_length",
                    format!(
                        "validator `string_length` parameter `unit` must be `unicode_scalar` or `utf8_bytes`, got `{other}`"
                    ),
                    other,
                    "unicode_scalar or utf8_bytes",
                    "Use one of the supported string length units.",
                ));
            }
        };
        Ok(Self {
            min,
            max,
            exact,
            allow_empty: optional_bool(meta, "string_length", "allow_empty")?.unwrap_or(true),
            unit,
            trim_before_count: optional_bool(meta, "string_length", "trim_before_count")?
                .unwrap_or(false),
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
