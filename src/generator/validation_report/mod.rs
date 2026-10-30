// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    Result,
    artifact::ArtifactSink,
    validation::{DesignerReportOptions, DesignerValidationReport, ValidationReport},
};

use super::{ArtifactGenerator, GeneratorCommonInput, GeneratorDescriptor, GeneratorRequirements};

mod config;

use config::validation_report_export_config_from_input;

pub(super) struct ValidationReportGenerator;

fn export_validation_report_to_sink(
    input: &GeneratorCommonInput<'_>,
    validation_report: &ValidationReport,
    sink: &ArtifactSink,
) -> Result<()> {
    let config = validation_report_export_config_from_input(input)?;
    let mut report = DesignerValidationReport::from_validation_report_with_schema(
        validation_report,
        input.meta,
        DesignerReportOptions {
            title: None,
            project_name: input.project.name.clone(),
            project_path: Some(input.project_root.clone()),
            target: Some(input.target_name.clone()),
            target_tags: input.target_tags.clone(),
            generated_at: input.generated_at.clone(),
            input_root: None,
        },
    );
    if let Some(title) = config.title {
        report.metadata.title = title;
    }
    let content = crate::validation::render_designer_validation_report(&report, config.format)?;
    sink.write_text(config.output, content)
}

impl ArtifactGenerator for ValidationReportGenerator {
    fn type_id(&self) -> &'static str {
        "validation_report"
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_and_data().with_report_only()
    }

    fn descriptor(&self) -> GeneratorDescriptor {
        GeneratorDescriptor::new(self.type_id(), self.aliases(), self.requirements())
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        Ok(vec![
            validation_report_export_config_from_input(input)?.output,
        ])
    }
    fn generate(
        &self,
        input: &crate::generator::GeneratorTaskInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let report = input.prepared.validation_report(self.type_id())?;
        export_validation_report_to_sink(&input.common, report, sink)
    }
}
