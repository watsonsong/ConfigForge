// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{Result, validation::DesignerReportFormat};

use super::super::{GeneratorCommonInput, GeneratorOptionParser};

pub(super) struct ValidationReportGeneratorConfig {
    pub(super) format: DesignerReportFormat,
    pub(super) output: PathBuf,
    pub(super) title: Option<String>,
}

pub(super) fn validation_report_export_config_from_input(
    input: &GeneratorCommonInput<'_>,
) -> Result<ValidationReportGeneratorConfig> {
    let mut parser =
        GeneratorOptionParser::new(input.options.clone(), "generator.validation_report")
            .with_target(input.target_name.clone());
    let format = parser
        .take_string("format", "validation_report")?
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(DesignerReportFormat::Html);
    let file_name = parser
        .take_string("file_name", "validation_report")?
        .unwrap_or_else(|| format!("validation-report.{}", format.extension()));
    let title = parser.take_string("title", "validation_report")?;
    parser.reject_unknown(
        |key| {
            format!(
                "validation_report export task `{}` has unknown option `{key}`",
                input.task_name
            )
        },
        "supported validation_report option",
        None,
    )?;
    Ok(ValidationReportGeneratorConfig {
        format,
        output: input.output.join(file_name),
        title,
    })
}
