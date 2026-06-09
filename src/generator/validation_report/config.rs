// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::{CF_GENERATOR_INVALID_OPTION, CF_GENERATOR_UNSUPPORTED_OPTION},
    validation::DesignerReportFormat,
};

use super::super::GeneratorCommonInput;

pub(super) struct ValidationReportGeneratorConfig {
    pub(super) format: DesignerReportFormat,
    pub(super) output: PathBuf,
    pub(super) title: Option<String>,
}

pub(super) fn validation_report_export_config_from_input(
    input: &GeneratorCommonInput<'_>,
) -> Result<ValidationReportGeneratorConfig> {
    let mut options = input.options.clone();
    let format = take_optional_string(input, &mut options, "format")?
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(DesignerReportFormat::Html);
    let file_name = take_optional_string(input, &mut options, "file_name")?
        .unwrap_or_else(|| format!("validation-report.{}", format.extension()));
    let title = take_optional_string(input, &mut options, "title")?;
    let unsupported = options.keys().cloned().collect::<Vec<_>>();
    if let Some(key) = unsupported.first() {
        return Err(validation_report_option_error(
            CF_GENERATOR_UNSUPPORTED_OPTION,
            input,
            key,
            format!(
                "validation_report export task `{}` has unknown option `{key}`",
                input.task_name
            ),
            "supported validation_report option",
        ));
    }
    Ok(ValidationReportGeneratorConfig {
        format,
        output: input.output.join(file_name),
        title,
    })
}

fn take_optional_string(
    input: &GeneratorCommonInput<'_>,
    options: &mut BTreeMap<String, toml::Value>,
    key: &str,
) -> Result<Option<String>> {
    options
        .remove(key)
        .map(|value| {
            value.try_into().map_err(|source| {
                validation_report_option_error(
                    CF_GENERATOR_INVALID_OPTION,
                    input,
                    key,
                    format!("invalid validation_report option `{key}`: {source}"),
                    "string value",
                )
            })
        })
        .transpose()
}

fn validation_report_option_error(
    code: &'static str,
    input: &GeneratorCommonInput<'_>,
    key: &str,
    message: String,
    expected: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            code,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source("generator.validation_report")
        .with_target(input.target_name.clone())
        .with_current_value(key)
        .with_expected(expected),
    )
}
