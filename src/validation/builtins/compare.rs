// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;

use crate::{
    ConfigForgeError, Result,
    meta::{ExportCondition, LinkedMetaModel, TargetTags, ValidationSeverity, ValidatorMeta},
};

use super::common::{
    compare_values, is_empty_value, linked_field, optional_bool, required_string,
    scalar_field_kind, validate_allowed_params,
};
use crate::validation::diagnostic::ValidationIssueLocation;
use crate::validation::registry::{
    CompiledValidator, ValidatorFactory, ValidatorLocation, ValidatorScope,
};
use crate::validation::{ValidationContext, ValidationIssue};

pub struct CompareValidatorFactory;

impl ValidatorFactory for CompareValidatorFactory {
    fn kind(&self) -> &'static str {
        "compare"
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
            return Err(ConfigForgeError::Validation(
                "compare validator must be declared on a table".to_owned(),
            ));
        };
        let table = super::common::linked_table(linked_meta, "compare", table_name)?;
        let config = CompareConfig::compile(meta, table)?;
        Ok(Box::new(CompareValidator {
            table_name,
            config,
            severity: meta.severity,
            export_when: meta.export_when.clone(),
            validator_name: meta.name.clone().unwrap_or_else(|| self.kind().to_owned()),
            message: meta.message.clone(),
        }))
    }
}

struct CompareValidator<'meta> {
    table_name: &'meta str,
    config: CompareConfig,
    severity: ValidationSeverity,
    export_when: ExportCondition,
    validator_name: String,
    message: Option<String>,
}

impl<'meta> CompiledValidator<'meta> for CompareValidator<'meta> {
    fn kind(&self) -> &'static str {
        "compare"
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
        for visible_row in &table_view.visible_rows {
            let Some(row) = table_view.table.rows.get(visible_row.row_index) else {
                continue;
            };
            let left = row
                .get(&self.config.left)
                .map(String::as_str)
                .unwrap_or_default();
            let right = row
                .get(&self.config.right)
                .map(String::as_str)
                .unwrap_or_default();
            if is_empty_value(left) || is_empty_value(right) {
                if self.config.skip_if_either_empty {
                    continue;
                }
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    "validator.compare_operand_missing",
                    "has empty operand".to_owned(),
                ));
                continue;
            }
            let Some(ordering) = compare_values(self.config.kind, left, right) else {
                continue;
            };
            if !self.config.op.matches(ordering) {
                issues.push(self.issue(
                    ctx,
                    visible_row.row_index,
                    visible_row.source_row_number,
                    "validator.compare_failed",
                    format!(
                        "expected `{}` {} `{}`, got `{left}` and `{right}`",
                        self.config.left,
                        self.config.op.as_str(),
                        self.config.right
                    ),
                ));
            }
        }
    }
}

impl<'meta> CompareValidator<'meta> {
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
                "table `{}` row {} compare {}",
                self.table_name, row_number, violation
            )
        });
        let mut issue = ValidationIssue::new(
            code,
            self.severity,
            self.validator_name.clone(),
            message,
            ValidationIssueLocation {
                table: Some(self.table_name.to_owned()),
                field: Some(format!("{},{}", self.config.left, self.config.right)),
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
enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CompareOp {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "==" => Some(Self::Eq),
            "!=" => Some(Self::Ne),
            "<" => Some(Self::Lt),
            "<=" => Some(Self::Le),
            ">" => Some(Self::Gt),
            ">=" => Some(Self::Ge),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }

    fn matches(self, ordering: Ordering) -> bool {
        match self {
            Self::Eq => ordering == Ordering::Equal,
            Self::Ne => ordering != Ordering::Equal,
            Self::Lt => ordering == Ordering::Less,
            Self::Le => matches!(ordering, Ordering::Less | Ordering::Equal),
            Self::Gt => ordering == Ordering::Greater,
            Self::Ge => matches!(ordering, Ordering::Greater | Ordering::Equal),
        }
    }
}

struct CompareConfig {
    left: String,
    right: String,
    op: CompareOp,
    kind: super::common::ScalarFieldKind,
    skip_if_either_empty: bool,
}

impl CompareConfig {
    fn compile(meta: &ValidatorMeta, table: &crate::meta::LinkedTableMeta<'_>) -> Result<Self> {
        validate_allowed_params(
            meta.kind.as_str(),
            meta,
            &["left", "op", "right", "skip_if_either_empty"],
        )?;
        let left = required_string(meta, "compare", "left")?;
        let right = required_string(meta, "compare", "right")?;
        let op = CompareOp::parse(&required_string(meta, "compare", "op")?).ok_or_else(|| {
            ConfigForgeError::Validation(
                "validator `compare` parameter `op` must be one of ==, !=, <, <=, >, >=".to_owned(),
            )
        })?;
        let left_field = linked_field(table, "compare", &left)?;
        let right_field = linked_field(table, "compare", &right)?;
        let left_kind = scalar_field_kind(left_field).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "table `{}` field `{left}` validator `compare` only supports scalar fields",
                table.meta.name
            ))
        })?;
        let right_kind = scalar_field_kind(right_field).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "table `{}` field `{right}` validator `compare` only supports scalar fields",
                table.meta.name
            ))
        })?;
        if left_kind != right_kind || !left_kind.is_comparable() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` validator `compare` requires compatible comparable field types",
                table.meta.name
            )));
        }
        Ok(Self {
            left,
            right,
            op,
            kind: left_kind,
            skip_if_either_empty: optional_bool(meta, "compare", "skip_if_either_empty")?
                .unwrap_or(true),
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
