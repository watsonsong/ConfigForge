use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Result,
    meta::{LinkedMetaModel, ValidatorMeta},
};

use super::{
    ValidationContext, ValidationIssue, ValidationReport, builtins::RangeValidatorFactory,
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
            .ok_or_else(|| ConfigForgeError::Validation(format!("unknown validator kind `{kind}`")))
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
    Err(ConfigForgeError::Validation(format!(
        "validator `{}` with scope `{:?}` cannot be declared at {location_name}",
        validator.kind, scope
    )))
}
