// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    data::{ColorValue, typed_value::parse_color_field_value},
    meta::{
        ColorChannel, ColorSpace, ExportCondition, LinkedMetaModel, PrimitiveFieldType, TargetTags,
        ValidationSeverity, ValidatorMeta,
    },
    validation::diagnostic::ValidationIssueLocation,
    validation::registry::{
        CompiledValidator, ValidatorFactory, ValidatorLocation, ValidatorScope,
    },
    validation::{ValidationContext, ValidationIssue},
};

use super::common::{
    is_empty_value, linked_field, linked_table, optional_bool, optional_number, optional_string,
    optional_string_array, row_value, validate_allowed_params, validator_config_error,
};

pub struct ColorValidatorFactory;

impl ValidatorFactory for ColorValidatorFactory {
    fn kind(&self) -> &'static str {
        "color"
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
                "color",
                "color validator can only be declared on a field".to_owned(),
                "table scope",
                "field scope",
                "Move the color validator to [[tables.fields.validators]].",
            ));
        };

        let table = linked_table(linked_meta, "color", table_name)?;
        let field = linked_field(table, "color", field_name)?;
        if field.primitive_type() != Some(PrimitiveFieldType::Color) {
            return Err(validator_config_error(
                "color",
                format!(
                    "table `{table_name}` field `{field_name}` validator `color` can only be used on color fields"
                ),
                field_name,
                "color field",
                "Attach color validator to a builtin color field.",
            ));
        }
        let config = ColorValidatorConfig::compile(meta)?;
        config.validate_field_metadata(
            table_name,
            field_name,
            field.meta.color_channel,
            field.meta.color_space.unwrap_or_default(),
        )?;
        if !config.has_any_bound() {
            return Err(validator_config_error(
                "color",
                format!(
                    "table `{table_name}` field `{field_name}` validator `color` must set at least one channel or alpha bound"
                ),
                "missing bounds",
                "min/max or alpha_min/alpha_max",
                "Configure at least one color validation bound.",
            ));
        }

        Ok(Box::new(ColorValidator {
            table_name,
            field_name,
            field,
            config,
            severity: meta.severity,
            export_when: meta.export_when.clone(),
            validator_name: meta.name.clone().unwrap_or_else(|| self.kind().to_owned()),
            message: meta.message.clone(),
        }))
    }
}

struct ColorValidator<'meta> {
    table_name: &'meta str,
    field_name: &'meta str,
    field: &'meta crate::meta::LinkedFieldMeta<'meta>,
    config: ColorValidatorConfig,
    severity: ValidationSeverity,
    export_when: ExportCondition,
    validator_name: String,
    message: Option<String>,
}

impl<'meta> CompiledValidator<'meta> for ColorValidator<'meta> {
    fn kind(&self) -> &'static str {
        "color"
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
            let value = row_value(row, self.field_name);
            if is_empty_value(value) && self.config.allow_empty {
                continue;
            }
            if is_empty_value(value) {
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    "must not be empty".to_owned(),
                ));
                continue;
            }
            let row_number = visible_row
                .source_row_number
                .unwrap_or(visible_row.row_index + 1);
            let Ok(parsed) = parse_color_field_value(
                self.table_name,
                row_number,
                self.field_name,
                self.field,
                value,
            ) else {
                continue;
            };
            if let Some(violation) = self.config.channel_violation(&parsed) {
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    violation,
                ));
                continue;
            }
            if let Some(violation) = self.config.alpha_violation(&parsed) {
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    violation,
                ));
            }
        }
    }
}

impl<'meta> ColorValidator<'meta> {
    fn should_run(&self, target_tags: Option<&TargetTags>) -> bool {
        if let Some(target_tags) = target_tags {
            self.export_when.matches(target_tags)
        } else {
            self.export_when.matches(&TargetTags::empty())
        }
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
                "table `{}` row {} field `{}` color value {}",
                self.table_name, row_number, self.field_name, violation
            )
        });
        let mut issue = ValidationIssue::new(
            "field.color_violation",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorComponent {
    R,
    G,
    B,
    A,
}

impl ColorComponent {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "r" => Some(Self::R),
            "g" => Some(Self::G),
            "b" => Some(Self::B),
            "a" => Some(Self::A),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::R => "r",
            Self::G => "g",
            Self::B => "b",
            Self::A => "a",
        }
    }

    fn value(self, color: &ColorValue) -> f64 {
        match self {
            Self::R => color.r,
            Self::G => color.g,
            Self::B => color.b,
            Self::A => color.a,
        }
    }
}

#[derive(Debug, Clone)]
struct ColorValidatorConfig {
    channels: Vec<ColorComponent>,
    min: Option<f64>,
    max: Option<f64>,
    alpha_min: Option<f64>,
    alpha_max: Option<f64>,
    expected_channel: Option<ColorChannel>,
    expected_space: Option<ColorSpace>,
    allow_empty: bool,
}

impl ColorValidatorConfig {
    fn compile(meta: &ValidatorMeta) -> Result<Self> {
        validate_allowed_params(
            "color",
            meta,
            &[
                "channels",
                "min",
                "max",
                "alpha_min",
                "alpha_max",
                "channel",
                "space",
                "allow_empty",
            ],
        )?;
        let channels = optional_string_array(meta, "color", "channels")?
            .unwrap_or_else(|| {
                vec![
                    "r".to_owned(),
                    "g".to_owned(),
                    "b".to_owned(),
                    "a".to_owned(),
                ]
            })
            .into_iter()
            .map(|component| {
                ColorComponent::parse(&component).ok_or_else(|| {
                    validator_config_error(
                        "color",
                        format!("validator `color` references unknown color channel `{component}`"),
                        component,
                        "one of: r, g, b, a",
                        "Use canonical color channel names.",
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let config = Self {
            channels,
            min: optional_number(meta, "color", "min")?,
            max: optional_number(meta, "color", "max")?,
            alpha_min: optional_number(meta, "color", "alpha_min")?,
            alpha_max: optional_number(meta, "color", "alpha_max")?,
            expected_channel: optional_string(meta, "color", "channel")?
                .map(|value| parse_color_channel(&value))
                .transpose()?,
            expected_space: optional_string(meta, "color", "space")?
                .map(|value| parse_color_space(&value))
                .transpose()?,
            allow_empty: optional_bool(meta, "color", "allow_empty")?.unwrap_or(true),
        };
        validate_bound_pair("color", "min", config.min, "max", config.max)?;
        validate_bound_pair(
            "color",
            "alpha_min",
            config.alpha_min,
            "alpha_max",
            config.alpha_max,
        )?;
        Ok(config)
    }

    fn validate_field_metadata(
        &self,
        table_name: &str,
        field_name: &str,
        actual_channel: Option<ColorChannel>,
        actual_space: ColorSpace,
    ) -> Result<()> {
        if let Some(expected_channel) = self.expected_channel {
            if actual_channel != Some(expected_channel) {
                return Err(validator_config_error(
                    "color",
                    format!(
                        "table `{table_name}` field `{field_name}` validator `color` expects color_channel `{}`",
                        expected_channel.as_str()
                    ),
                    actual_channel
                        .map(ColorChannel::as_str)
                        .unwrap_or("<missing>"),
                    expected_channel.as_str(),
                    "Update field color_channel or the validator channel constraint.",
                ));
            }
        }
        if let Some(expected_space) = self.expected_space {
            if actual_space != expected_space {
                return Err(validator_config_error(
                    "color",
                    format!(
                        "table `{table_name}` field `{field_name}` validator `color` expects color_space `{}`",
                        expected_space.as_str()
                    ),
                    actual_space.as_str(),
                    expected_space.as_str(),
                    "Update field color_space or the validator space constraint.",
                ));
            }
        }
        Ok(())
    }

    fn has_any_bound(&self) -> bool {
        self.min.is_some()
            || self.max.is_some()
            || self.alpha_min.is_some()
            || self.alpha_max.is_some()
    }

    fn channel_violation(&self, color: &ColorValue) -> Option<String> {
        for channel in &self.channels {
            let value = channel.value(color);
            if let Some(min) = self.min {
                if value < min {
                    return Some(format!(
                        "channel `{}` is below min `{min}`",
                        channel.as_str()
                    ));
                }
            }
            if let Some(max) = self.max {
                if value > max {
                    return Some(format!(
                        "channel `{}` exceeds max `{max}`",
                        channel.as_str()
                    ));
                }
            }
        }
        None
    }

    fn alpha_violation(&self, color: &ColorValue) -> Option<String> {
        if let Some(min) = self.alpha_min {
            if color.a < min {
                return Some(format!("alpha is below min `{min}`"));
            }
        }
        if let Some(max) = self.alpha_max {
            if color.a > max {
                return Some(format!("alpha exceeds max `{max}`"));
            }
        }
        None
    }
}

fn parse_color_channel(value: &str) -> Result<ColorChannel> {
    match value {
        "byte" => Ok(ColorChannel::Byte),
        "normalized" => Ok(ColorChannel::Normalized),
        _ => Err(validator_config_error(
            "color",
            format!("validator `color` parameter `channel` has unknown value `{value}`"),
            value,
            "byte or normalized",
            "Use a supported color channel policy.",
        )),
    }
}

fn parse_color_space(value: &str) -> Result<ColorSpace> {
    match value {
        "srgb" => Ok(ColorSpace::Srgb),
        "linear" => Ok(ColorSpace::Linear),
        _ => Err(validator_config_error(
            "color",
            format!("validator `color` parameter `space` has unknown value `{value}`"),
            value,
            "srgb or linear",
            "Use a supported color space.",
        )),
    }
}

fn validate_bound_pair(
    kind: &str,
    min_key: &str,
    min: Option<f64>,
    max_key: &str,
    max: Option<f64>,
) -> Result<()> {
    if let (Some(min), Some(max)) = (min, max) {
        if min > max {
            return Err(validator_config_error(
                kind,
                format!("validator `{kind}` has {min_key} greater than {max_key}"),
                format!("{min_key}={min}, {max_key}={max}"),
                format!("{min_key} less than or equal to {max_key}"),
                "Adjust validator bounds so they are satisfiable.",
            ));
        }
    }
    Ok(())
}

fn format_target_tags(target_tags: &TargetTags) -> String {
    let tags = target_tags.iter().collect::<Vec<_>>();
    if tags.is_empty() {
        "<empty>".to_owned()
    } else {
        tags.join(",")
    }
}
