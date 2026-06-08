// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use crate::{ConfigForgeError, Result, validation::DesignerReportFormat};

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
    let format = take_optional_string(&mut options, "format")?
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(DesignerReportFormat::Html);
    let file_name = take_optional_string(&mut options, "file_name")?
        .unwrap_or_else(|| format!("validation-report.{}", format.extension()));
    let title = take_optional_string(&mut options, "title")?;
    let unsupported = options.keys().cloned().collect::<Vec<_>>();
    if let Some(key) = unsupported.first() {
        return Err(ConfigForgeError::Validation(format!(
            "validation_report export task `{}` has unknown option `{key}`",
            input.task_name
        )));
    }
    Ok(ValidationReportGeneratorConfig {
        format,
        output: input.output.join(file_name),
        title,
    })
}

fn take_optional_string(
    options: &mut BTreeMap<String, toml::Value>,
    key: &str,
) -> Result<Option<String>> {
    options
        .remove(key)
        .map(|value| {
            value.try_into().map_err(|source| {
                ConfigForgeError::Validation(format!(
                    "invalid validation_report option `{key}`: {source}"
                ))
            })
        })
        .transpose()
}
