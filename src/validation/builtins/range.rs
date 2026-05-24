use std::collections::BTreeSet;

use crate::{
    meta::{
        ExportCondition, LinkedField, LinkedMetaModel, PrimitiveFieldType, TargetTags,
        ValidationSeverity, ValidatorMeta,
    },
    ConfigForgeError, Result,
};

use crate::validation::diagnostic::ValidationIssueLocation;
use crate::validation::registry::{
    CompiledValidator, ValidatorFactory, ValidatorLocation, ValidatorScope,
};
use crate::validation::{ValidationContext, ValidationIssue};

pub struct RangeValidatorFactory;

impl ValidatorFactory for RangeValidatorFactory {
    fn kind(&self) -> &'static str {
        "range"
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
            return Err(ConfigForgeError::Validation(
                "range validator can only be declared on a field".to_owned(),
            ));
        };

        let table = linked_meta.table(table_name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "range validator references unknown table `{table_name}`"
            ))
        })?;
        let field = table.field(field_name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "table `{table_name}` range validator references unknown field `{field_name}`"
            ))
        })?;
        let field_type = match field.linked_field {
            LinkedField::Primitive(PrimitiveFieldType::Int) => RangeFieldType::Int,
            LinkedField::Primitive(PrimitiveFieldType::Float) => RangeFieldType::Float,
            _ => {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{table_name}` field `{field_name}` validator `range` can only be used on int or float fields"
                )));
            }
        };
        let config = RangeValidatorConfig::compile(meta)?;
        if config.min.is_none() && config.max.is_none() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{table_name}` field `{field_name}` validator `range` must set at least one of `min` or `max`"
            )));
        }
        if let (Some(min), Some(max)) = (config.min, config.max) {
            if min > max {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{table_name}` field `{field_name}` validator `range` has min greater than max"
                )));
            }
        }

        Ok(Box::new(RangeValidator {
            table_name,
            field_name,
            field_type,
            config,
            severity: meta.severity,
            export_when: meta.export_when.clone(),
            validator_name: meta.name.clone().unwrap_or_else(|| self.kind().to_owned()),
            message: meta.message.clone(),
        }))
    }
}

struct RangeValidator<'meta> {
    table_name: &'meta str,
    field_name: &'meta str,
    field_type: RangeFieldType,
    config: RangeValidatorConfig,
    severity: ValidationSeverity,
    export_when: ExportCondition,
    validator_name: String,
    message: Option<String>,
}

impl<'meta> CompiledValidator<'meta> for RangeValidator<'meta> {
    fn kind(&self) -> &'static str {
        "range"
    }

    fn validate<'data>(
        &self,
        ctx: &ValidationContext<'meta, 'data>,
        issues: &mut Vec<ValidationIssue>,
    ) {
        if !self.should_run(ctx.target_tags.as_ref()) {
            return;
        }

        let Some(table_view) = ctx.export_view.table(self.table_name) else {
            return;
        };
        if !table_view.is_field_visible(self.field_name) {
            return;
        }

        for visible_row in &table_view.visible_rows {
            let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
                continue;
            };
            let value = row
                .get(self.field_name)
                .map(String::as_str)
                .unwrap_or_default();
            if value.trim().is_empty() && self.config.allow_empty {
                continue;
            }
            if value.trim().is_empty() {
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    "must not be empty".to_owned(),
                ));
                continue;
            }

            let Ok(parsed_value) = self.field_type.parse(value) else {
                continue;
            };

            if let Some(min) = self.config.min {
                let violates = if self.config.exclusive_min {
                    parsed_value <= min
                } else {
                    parsed_value < min
                };
                if violates {
                    issues.push(self.issue(
                        ctx,
                        visible_row.row_index,
                        visible_row.source_row_number,
                        format!("is below min `{min}`"),
                    ));
                    continue;
                }
            }

            if let Some(max) = self.config.max {
                let violates = if self.config.exclusive_max {
                    parsed_value >= max
                } else {
                    parsed_value > max
                };
                if violates {
                    issues.push(self.issue(
                        ctx,
                        visible_row.row_index,
                        visible_row.source_row_number,
                        format!("exceeds max `{max}`"),
                    ));
                }
            }
        }
    }
}

impl<'meta> RangeValidator<'meta> {
    fn should_run(&self, target_tags: Option<&TargetTags>) -> bool {
        if let Some(target_tags) = target_tags {
            return self.export_when.matches(target_tags);
        }

        self.export_when.matches(&TargetTags::empty())
    }

    fn issue(
        &self,
        ctx: &ValidationContext<'meta, '_>,
        row_index: usize,
        source_row_number: Option<usize>,
        violation: String,
    ) -> ValidationIssue {
        let row_number = source_row_number.unwrap_or(row_index + 1);
        let message = self.message.clone().unwrap_or_else(|| {
            format!(
                "table `{}` row {} field `{}` value {}",
                self.table_name, row_number, self.field_name, violation
            )
        });
        let mut issue = ValidationIssue::new(
            "field.range_violation",
            self.severity,
            self.validator_name.clone(),
            message,
            ValidationIssueLocation {
                table: Some(self.table_name.to_owned()),
                field: Some(self.field_name.to_owned()),
                row_index: Some(row_index),
                source_row_number,
            },
        );
        issue.target = ctx.target_tags.as_ref().map(format_target_tags);
        issue
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RangeFieldType {
    Int,
    Float,
}

impl RangeFieldType {
    fn parse(self, value: &str) -> std::result::Result<f64, ()> {
        match self {
            Self::Int => value
                .parse::<i64>()
                .map(|value| value as f64)
                .map_err(|_| ()),
            Self::Float => value
                .parse::<f64>()
                .map_err(|_| ())
                .and_then(|value| if value.is_finite() { Ok(value) } else { Err(()) }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RangeValidatorConfig {
    min: Option<f64>,
    max: Option<f64>,
    exclusive_min: bool,
    exclusive_max: bool,
    allow_empty: bool,
}

impl RangeValidatorConfig {
    fn compile(meta: &ValidatorMeta) -> Result<Self> {
        let allowed = [
            "min",
            "max",
            "exclusive_min",
            "exclusive_max",
            "allow_empty",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        for key in meta.params.keys() {
            if !allowed.contains(key.as_str()) {
                return Err(ConfigForgeError::Validation(format!(
                    "validator `range` has unknown parameter `{key}`"
                )));
            }
        }

        Ok(Self {
            min: optional_number(meta, "min")?,
            max: optional_number(meta, "max")?,
            exclusive_min: optional_bool(meta, "exclusive_min")?.unwrap_or(false),
            exclusive_max: optional_bool(meta, "exclusive_max")?.unwrap_or(false),
            allow_empty: optional_bool(meta, "allow_empty")?.unwrap_or(true),
        })
    }
}

fn optional_number(meta: &ValidatorMeta, key: &str) -> Result<Option<f64>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    if let Some(value) = value.as_integer() {
        return Ok(Some(value as f64));
    }
    if let Some(value) = value.as_float() {
        if value.is_finite() {
            return Ok(Some(value));
        }
    }
    Err(ConfigForgeError::Validation(format!(
        "validator `range` parameter `{key}` must be finite integer or float"
    )))
}

fn optional_bool(meta: &ValidatorMeta, key: &str) -> Result<Option<bool>> {
    let Some(value) = meta.params.get(key) else {
        return Ok(None);
    };
    value.as_bool().map(Some).ok_or_else(|| {
        ConfigForgeError::Validation(format!("validator `range` parameter `{key}` must be bool"))
    })
}
