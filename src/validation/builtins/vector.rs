// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    data::{DataValue, VectorValue, typed_value::parse_vector_field_value},
    meta::{ExportCondition, LinkedMetaModel, TargetTags, ValidationSeverity, ValidatorMeta},
    validation::diagnostic::ValidationIssueLocation,
    validation::registry::{
        CompiledValidator, ValidatorFactory, ValidatorLocation, ValidatorScope,
    },
    validation::{ValidationContext, ValidationIssue},
};

use super::common::{
    field_value, is_empty_value, linked_field, linked_table, optional_bool, optional_number,
    optional_string_array, resolved_data_value, validate_allowed_params, validator_config_error,
};

pub struct VectorValidatorFactory;

impl ValidatorFactory for VectorValidatorFactory {
    fn kind(&self) -> &'static str {
        "vector"
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
                "vector",
                "vector validator can only be declared on a field".to_owned(),
                "table scope",
                "field scope",
                "Move the vector validator to [[tables.fields.validators]].",
            ));
        };

        let table = linked_table(linked_meta, "vector", table_name)?;
        let field = linked_field(table, "vector", field_name)?;
        let Some(primitive) = field
            .primitive_type()
            .filter(|primitive| primitive.vector_dimension().is_some())
        else {
            return Err(validator_config_error(
                "vector",
                format!(
                    "table `{table_name}` field `{field_name}` validator `vector` can only be used on vector fields"
                ),
                field_name,
                "vector2/vector3/vector4 or vector*_int field",
                "Attach vector validator to a builtin vector field.",
            ));
        };
        let config = VectorValidatorConfig::compile(meta, field.meta.vector_component_names())?;
        if !config.has_any_bound() {
            return Err(validator_config_error(
                "vector",
                format!(
                    "table `{table_name}` field `{field_name}` validator `vector` must set at least one component or magnitude bound"
                ),
                "missing bounds",
                "min/max or magnitude_min/magnitude_max",
                "Configure at least one vector validation bound.",
            ));
        }

        Ok(Box::new(VectorValidator {
            table_name,
            field_name,
            field,
            dimension: primitive.vector_dimension().unwrap_or(0),
            config,
            severity: meta.severity,
            export_when: meta.export_when.clone(),
            validator_name: meta.name.clone().unwrap_or_else(|| self.kind().to_owned()),
            message: meta.message.clone(),
        }))
    }
}

struct VectorValidator<'meta> {
    table_name: &'meta str,
    field_name: &'meta str,
    field: &'meta crate::meta::LinkedFieldMeta<'meta>,
    dimension: usize,
    config: VectorValidatorConfig,
    severity: ValidationSeverity,
    export_when: ExportCondition,
    validator_name: String,
    message: Option<String>,
}

impl<'meta> CompiledValidator<'meta> for VectorValidator<'meta> {
    fn kind(&self) -> &'static str {
        "vector"
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

        for (visible_row_index, visible_row) in table_view.visible_rows.iter().enumerate() {
            if table_view.table.rows.get(visible_row.row_index).is_none() {
                continue;
            }
            let value = field_value(table_view, visible_row_index, self.field_name);
            let value = value.as_str();
            let resolved_value =
                resolved_data_value(table_view, visible_row_index, self.field_name);
            if resolved_value.is_none() && is_empty_value(value) && self.config.allow_empty {
                continue;
            }
            if resolved_value.is_none() && is_empty_value(value) {
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    "must not be empty".to_owned(),
                ));
                continue;
            }
            let parsed = match resolved_value {
                Some(DataValue::Vector(vector)) => vector.clone(),
                _ => {
                    let row_number = visible_row
                        .source_row_number
                        .unwrap_or(visible_row.row_index + 1);
                    let Ok(parsed) = parse_vector_field_value(
                        self.table_name,
                        row_number,
                        self.field_name,
                        self.field,
                        value,
                    ) else {
                        continue;
                    };
                    parsed
                }
            };
            let components = vector_components_as_f64(&parsed);
            if components.len() != self.dimension {
                continue;
            }
            if let Some(violation) = self.config.component_violation(&components) {
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    violation,
                ));
                continue;
            }
            if let Some(violation) = self.config.magnitude_violation(&components) {
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

impl<'meta> VectorValidator<'meta> {
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
                "table `{}` row {} field `{}` vector value {}",
                self.table_name, row_number, self.field_name, violation
            )
        });
        let mut issue = ValidationIssue::new(
            "field.vector_violation",
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

#[derive(Debug, Clone)]
struct VectorValidatorConfig {
    component_indexes: Vec<usize>,
    component_names: Vec<String>,
    min: Option<f64>,
    max: Option<f64>,
    exclusive_min: bool,
    exclusive_max: bool,
    magnitude_min: Option<f64>,
    magnitude_max: Option<f64>,
    exclusive_magnitude_min: bool,
    exclusive_magnitude_max: bool,
    allow_empty: bool,
}

impl VectorValidatorConfig {
    fn compile(meta: &ValidatorMeta, all_components: Vec<String>) -> Result<Self> {
        validate_allowed_params(
            "vector",
            meta,
            &[
                "components",
                "min",
                "max",
                "exclusive_min",
                "exclusive_max",
                "magnitude_min",
                "magnitude_max",
                "exclusive_magnitude_min",
                "exclusive_magnitude_max",
                "allow_empty",
            ],
        )?;
        let selected =
            optional_string_array(meta, "vector", "components")?.unwrap_or(all_components.clone());
        let mut component_indexes = Vec::with_capacity(selected.len());
        for component in &selected {
            let Some(index) = all_components
                .iter()
                .position(|candidate| candidate == component)
            else {
                return Err(validator_config_error(
                    "vector",
                    format!("validator `vector` references unknown component `{component}`"),
                    component,
                    format!("one of: {}", all_components.join(", ")),
                    "Use component names declared by vector_components or the default x/y/z/w names.",
                ));
            };
            component_indexes.push(index);
        }
        let config = Self {
            component_indexes,
            component_names: selected,
            min: optional_number(meta, "vector", "min")?,
            max: optional_number(meta, "vector", "max")?,
            exclusive_min: optional_bool(meta, "vector", "exclusive_min")?.unwrap_or(false),
            exclusive_max: optional_bool(meta, "vector", "exclusive_max")?.unwrap_or(false),
            magnitude_min: optional_number(meta, "vector", "magnitude_min")?,
            magnitude_max: optional_number(meta, "vector", "magnitude_max")?,
            exclusive_magnitude_min: optional_bool(meta, "vector", "exclusive_magnitude_min")?
                .unwrap_or(false),
            exclusive_magnitude_max: optional_bool(meta, "vector", "exclusive_magnitude_max")?
                .unwrap_or(false),
            allow_empty: optional_bool(meta, "vector", "allow_empty")?.unwrap_or(true),
        };
        validate_bound_pair("vector", "min", config.min, "max", config.max)?;
        validate_bound_pair(
            "vector",
            "magnitude_min",
            config.magnitude_min,
            "magnitude_max",
            config.magnitude_max,
        )?;
        Ok(config)
    }

    fn has_any_bound(&self) -> bool {
        self.min.is_some()
            || self.max.is_some()
            || self.magnitude_min.is_some()
            || self.magnitude_max.is_some()
    }

    fn component_violation(&self, components: &[f64]) -> Option<String> {
        for (name, index) in self.component_names.iter().zip(&self.component_indexes) {
            let value = components.get(*index).copied()?;
            if let Some(min) = self.min {
                let violates = if self.exclusive_min {
                    value <= min
                } else {
                    value < min
                };
                if violates {
                    return Some(format!("component `{name}` is below min `{min}`"));
                }
            }
            if let Some(max) = self.max {
                let violates = if self.exclusive_max {
                    value >= max
                } else {
                    value > max
                };
                if violates {
                    return Some(format!("component `{name}` exceeds max `{max}`"));
                }
            }
        }
        None
    }

    fn magnitude_violation(&self, components: &[f64]) -> Option<String> {
        let magnitude = components
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        if let Some(min) = self.magnitude_min {
            let violates = if self.exclusive_magnitude_min {
                magnitude <= min
            } else {
                magnitude < min
            };
            if violates {
                return Some(format!("magnitude `{magnitude}` is below min `{min}`"));
            }
        }
        if let Some(max) = self.magnitude_max {
            let violates = if self.exclusive_magnitude_max {
                magnitude >= max
            } else {
                magnitude > max
            };
            if violates {
                return Some(format!("magnitude `{magnitude}` exceeds max `{max}`"));
            }
        }
        None
    }
}

fn vector_components_as_f64(value: &VectorValue) -> Vec<f64> {
    match value {
        VectorValue::Float(values) => values.clone(),
        VectorValue::Int(values) => values.iter().map(|value| *value as f64).collect(),
    }
}

fn validate_bound_pair(
    kind: &str,
    min_key: &str,
    min: Option<f64>,
    max_key: &str,
    max: Option<f64>,
) -> Result<()> {
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        return Err(validator_config_error(
            kind,
            format!("validator `{kind}` has {min_key} greater than {max_key}"),
            format!("{min_key}={min}, {max_key}={max}"),
            format!("{min_key} less than or equal to {max_key}"),
            "Adjust validator bounds so they are satisfiable.",
        ));
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
