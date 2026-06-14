// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::CF_VALIDATION_INVALID_CONFIG,
    meta::{LinkedMetaModel, ValidatorMeta},
};

use super::{
    ValidationContext, ValidationIssue, ValidationReport, builtins::color::ColorValidatorFactory,
    builtins::compare::CompareValidatorFactory, builtins::monotonic::MonotonicValidatorFactory,
    builtins::range::RangeValidatorFactory, builtins::regex::RegexValidatorFactory,
    builtins::required_when::RequiredWhenValidatorFactory,
    builtins::string_length::StringLengthValidatorFactory, builtins::sum::SumValidatorFactory,
    builtins::vector::VectorValidatorFactory,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatorScope {
    Field,
    Record,
    Table,
    DataSet,
}

#[derive(Debug, Clone, Copy)]
pub enum ValidatorLocation<'meta> {
    Field {
        table_name: &'meta str,
        field_name: &'meta str,
    },
    Table {
        table_name: &'meta str,
    },
    DataSet,
}

pub trait ValidatorFactory: Send + Sync {
    fn kind(&self) -> &'static str;
    fn scope(&self) -> ValidatorScope;

    fn compile<'meta>(
        &self,
        meta: &'meta ValidatorMeta,
        location: ValidatorLocation<'meta>,
        linked_meta: &'meta LinkedMetaModel<'meta>,
    ) -> Result<Box<dyn CompiledValidator<'meta> + 'meta>>;
}

pub trait CompiledValidator<'meta>: Send + Sync {
    fn kind(&self) -> &'static str;
    fn validate<'data>(
        &self,
        ctx: &ValidationContext<'meta, 'data>,
        issues: &mut Vec<ValidationIssue>,
    );
}

pub struct ValidatorRegistry {
    factories: BTreeMap<String, Box<dyn ValidatorFactory>>,
}

impl ValidatorRegistry {
    pub fn new() -> Self {
        Self {
            factories: BTreeMap::new(),
        }
    }

    pub fn with_builtin_validators() -> Self {
        let mut registry = Self::new();
        registry.register(RangeValidatorFactory);
        registry.register(ColorValidatorFactory);
        registry.register(VectorValidatorFactory);
        registry.register(RegexValidatorFactory);
        registry.register(StringLengthValidatorFactory);
        registry.register(RequiredWhenValidatorFactory);
        registry.register(CompareValidatorFactory);
        registry.register(SumValidatorFactory);
        registry.register(MonotonicValidatorFactory);
        registry
    }

    pub fn register(&mut self, factory: impl ValidatorFactory + 'static) {
        self.factories
            .insert(factory.kind().to_owned(), Box::new(factory));
    }

    pub fn require(&self, kind: &str) -> Result<&dyn ValidatorFactory> {
        self.factories
            .get(kind)
            .map(|factory| factory.as_ref())
            .ok_or_else(|| {
                validator_config_error(
                    kind,
                    format!("unknown validator kind `{kind}`"),
                    kind,
                    "registered validator kind",
                    "Use one of the built-in validator kinds or register a custom validator factory.",
                )
            })
    }
}

impl Default for ValidatorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CompiledValidationPlan<'meta> {
    validators: Vec<Box<dyn CompiledValidator<'meta> + 'meta>>,
}

impl<'meta> CompiledValidationPlan<'meta> {
    pub fn compile(
        linked_meta: &'meta LinkedMetaModel<'meta>,
        registry: &ValidatorRegistry,
    ) -> Result<Self> {
        let mut validators = Vec::new();
        for table in &linked_meta.tables {
            for validator in &table.meta.validators {
                let factory = registry.require(&validator.kind)?;
                let location = ValidatorLocation::Table {
                    table_name: table.meta.name.as_str(),
                };
                validate_scope(factory, location, validator)?;
                validators.push(factory.compile(validator, location, linked_meta)?);
            }
            for field in &table.fields {
                for validator in &field.meta.validators {
                    let factory = registry.require(&validator.kind)?;
                    let location = ValidatorLocation::Field {
                        table_name: table.meta.name.as_str(),
                        field_name: field.meta.name.as_str(),
                    };
                    validate_scope(factory, location, validator)?;
                    validators.push(factory.compile(validator, location, linked_meta)?);
                }
            }
        }

        Ok(Self { validators })
    }

    pub fn run<'data>(&self, ctx: &ValidationContext<'meta, 'data>) -> ValidationReport {
        let mut report = ValidationReport::new();
        for validator in &self.validators {
            validator.validate(ctx, &mut report.issues);
        }
        report
    }
}

fn validate_scope(
    factory: &dyn ValidatorFactory,
    location: ValidatorLocation<'_>,
    validator: &ValidatorMeta,
) -> Result<()> {
    let scope = factory.scope();
    let valid = matches!(
        (scope, location),
        (ValidatorScope::Field, ValidatorLocation::Field { .. })
            | (
                ValidatorScope::Table | ValidatorScope::Record,
                ValidatorLocation::Table { .. }
            )
            | (ValidatorScope::DataSet, ValidatorLocation::DataSet)
    );
    if valid {
        return Ok(());
    }

    let location_name = match location {
        ValidatorLocation::Field {
            table_name,
            field_name,
        } => format!("table `{table_name}` field `{field_name}`"),
        ValidatorLocation::Table { table_name } => format!("table `{table_name}`"),
        ValidatorLocation::DataSet => "data set".to_owned(),
    };
    Err(validator_config_error(
        &validator.kind,
        format!(
            "validator `{}` with scope `{:?}` cannot be declared at {location_name}",
            validator.kind, scope
        ),
        declaration_scope(location),
        expected_scope(scope),
        "Move the validator declaration to a location that matches the validator scope.",
    ))
}

fn declaration_scope(location: ValidatorLocation<'_>) -> &'static str {
    match location {
        ValidatorLocation::Field { .. } => "field scope",
        ValidatorLocation::Table { .. } => "table scope",
        ValidatorLocation::DataSet => "data set scope",
    }
}

fn expected_scope(scope: ValidatorScope) -> &'static str {
    match scope {
        ValidatorScope::Field => "field scope",
        ValidatorScope::Record | ValidatorScope::Table => "table scope",
        ValidatorScope::DataSet => "data set scope",
    }
}

fn validator_config_error(
    kind: &str,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_VALIDATION_INVALID_CONFIG,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(format!("validator.{kind}"))
        .with_domain("validation")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
}
