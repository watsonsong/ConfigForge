// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;

use crate::{
    Result,
    meta::{ExportCondition, LinkedMetaModel, TargetTags, ValidationSeverity, ValidatorMeta},
};

use super::common::{
    compare_values, field_value, linked_field, optional_bool, optional_string, required_string,
    scalar_field_kind, validate_allowed_params, validator_config_error,
};
use super::table_common::{GroupByConfig, GroupKey, GroupedRows, OrderByConfig, OrderedRow};
use crate::validation::diagnostic::ValidationIssueLocation;
use crate::validation::registry::{
    CompiledValidator, ValidatorFactory, ValidatorLocation, ValidatorScope,
};
use crate::validation::{ValidationContext, ValidationIssue};

pub struct MonotonicValidatorFactory;

impl ValidatorFactory for MonotonicValidatorFactory {
    fn kind(&self) -> &'static str {
        "monotonic"
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
                "monotonic",
                "monotonic validator must be declared on a table".to_owned(),
                "field scope",
                "table scope",
                "Move the monotonic validator to [[tables.validators]].",
            ));
        };
        let table = super::common::linked_table(linked_meta, "monotonic", table_name)?;
        let config = MonotonicConfig::compile(meta, table)?;
        Ok(Box::new(MonotonicValidator {
            table_name,
            config,
            severity: meta.severity,
            export_when: meta.export_when.clone(),
            validator_name: meta.name.clone().unwrap_or_else(|| self.kind().to_owned()),
            message: meta.message.clone(),
        }))
    }
}

struct MonotonicValidator<'meta> {
    table_name: &'meta str,
    config: MonotonicConfig,
    severity: ValidationSeverity,
    export_when: ExportCondition,
    validator_name: String,
    message: Option<String>,
}

impl<'meta> CompiledValidator<'meta> for MonotonicValidator<'meta> {
    fn kind(&self) -> &'static str {
        "monotonic"
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
            let ordered_rows = self.config.order_by.sort_rows(table_view, rows);
            let mut previous: Option<MonotonicRow> = None;
            for ordered_row in ordered_rows {
                if let Some(previous_row) = &previous {
                    let order_cmp = self
                        .config
                        .order_by
                        .compare_values(&previous_row.order_value, &ordered_row.order_value)
                        .unwrap_or(Ordering::Equal);
                    if order_cmp == Ordering::Equal && !self.config.skip_equal_order_key {
                        issues.push(self.issue(
                            ctx,
                            group_key,
                            &ordered_row,
                            "validator.monotonic_duplicate_order_key",
                            format!(
                                "duplicate order key `{}` with previous row {}",
                                ordered_row.order_value,
                                previous_row.row_number()
                            ),
                        ));
                    }
                    if order_cmp == Ordering::Equal && self.config.skip_equal_order_key {
                        previous = Some(self.row_for(table_view, ordered_row));
                        continue;
                    }

                    let current_row = self.row_for(table_view, ordered_row);
                    if !self.config.direction.matches(
                        compare_values(
                            self.config.field_kind,
                            &previous_row.value,
                            &current_row.value,
                        )
                        .unwrap_or(Ordering::Equal),
                        self.config.strict,
                    ) {
                        issues.push(self.issue(
                            ctx,
                            group_key,
                            &current_row.ordered,
                            "validator.monotonic_violation",
                            format!(
                                "value `{}` at order `{}` violates previous value `{}` at order `{}`",
                                current_row.value,
                                current_row.order_value,
                                previous_row.value,
                                previous_row.order_value
                            ),
                        ));
                    }
                    previous = Some(current_row);
                } else {
                    previous = Some(self.row_for(table_view, ordered_row));
                }
            }
        }
    }
}

impl<'meta> MonotonicValidator<'meta> {
    fn row_for(
        &self,
        table_view: &crate::validation::ExportTableView<'_, '_>,
        ordered: OrderedRow,
    ) -> MonotonicRow {
        let value = field_value(table_view, ordered.visible_row_index, &self.config.field);
        MonotonicRow {
            order_value: ordered.order_value.clone(),
            value,
            ordered,
        }
    }

    fn issue(
        &self,
        ctx: &ValidationContext<'meta, '_>,
        group_key: &GroupKey,
        row: &OrderedRow,
        code: &'static str,
        violation: String,
    ) -> ValidationIssue {
        let group_label = group_key.label();
        let message = self.message.clone().unwrap_or_else(|| {
            format!(
                "table `{}` group `{}` row {} monotonic {}",
                self.table_name,
                group_label,
                row.source_row_number.unwrap_or(row.row_index + 1),
                violation
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
                row_index: Some(row.row_index),
                source_row_number: row.source_row_number,
                ..Default::default()
            },
        );
        issue.target = ctx.target_tags.as_ref().map(format_target_tags);
        issue
    }
}

struct MonotonicRow {
    ordered: OrderedRow,
    order_value: String,
    value: String,
}

impl MonotonicRow {
    fn row_number(&self) -> usize {
        self.ordered
            .source_row_number
            .unwrap_or(self.ordered.row_index + 1)
    }
}

struct MonotonicConfig {
    field: String,
    field_kind: super::common::ScalarFieldKind,
    group_by: GroupByConfig,
    order_by: OrderByConfig,
    direction: MonotonicDirection,
    strict: bool,
    skip_equal_order_key: bool,
}

impl MonotonicConfig {
    fn compile(meta: &ValidatorMeta, table: &crate::meta::LinkedTableMeta<'_>) -> Result<Self> {
        validate_allowed_params(
            "monotonic",
            meta,
            &[
                "field",
                "group_by",
                "order_by",
                "direction",
                "strict",
                "skip_equal_order_key",
            ],
        )?;
        let field = required_string(meta, "monotonic", "field")?;
        let field_meta = linked_field(table, "monotonic", &field)?;
        let field_kind = scalar_field_kind(field_meta).ok_or_else(|| {
            validator_config_error(
                "monotonic",
                format!(
                    "table `{}` validator `monotonic` field `{field}` must be scalar",
                    table.meta.name
                ),
                &field,
                "scalar field",
                "Choose an int, float, string, text, asset, or enum field.",
            )
        })?;
        if !field_kind.is_comparable() {
            return Err(validator_config_error(
                "monotonic",
                format!(
                    "table `{}` validator `monotonic` field `{field}` must be comparable",
                    table.meta.name
                ),
                &field,
                "comparable scalar field",
                "Use an int, float, string, text, asset, or enum field for monotonic ordering.",
            ));
        }
        let order_by = OrderByConfig::compile(
            table,
            "monotonic",
            required_string(meta, "monotonic", "order_by")?,
        )?;
        let direction = MonotonicDirection::parse(
            optional_string(meta, "monotonic", "direction")?
                .as_deref()
                .unwrap_or("increasing"),
        )?;
        Ok(Self {
            field,
            field_kind,
            group_by: GroupByConfig::compile(meta, table, "monotonic")?,
            order_by,
            direction,
            strict: optional_bool(meta, "monotonic", "strict")?.unwrap_or(true),
            skip_equal_order_key: optional_bool(meta, "monotonic", "skip_equal_order_key")?
                .unwrap_or(false),
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum MonotonicDirection {
    Increasing,
    Decreasing,
}

impl MonotonicDirection {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "increasing" => Ok(Self::Increasing),
            "decreasing" => Ok(Self::Decreasing),
            _ => Err(validator_config_error(
                "monotonic",
                "validator `monotonic` parameter `direction` must be `increasing` or `decreasing`"
                    .to_owned(),
                value,
                "increasing or decreasing",
                "Use one of the supported monotonic directions.",
            )),
        }
    }

    fn matches(self, ordering: Ordering, strict: bool) -> bool {
        match (self, strict) {
            (Self::Increasing, true) => ordering == Ordering::Less,
            (Self::Increasing, false) => matches!(ordering, Ordering::Less | Ordering::Equal),
            (Self::Decreasing, true) => ordering == Ordering::Greater,
            (Self::Decreasing, false) => matches!(ordering, Ordering::Greater | Ordering::Equal),
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
