// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    artifact::ArtifactSink, diagnostic::code::CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH,
};

use super::{
    CodeGenerator, GeneratorCommonInput, GeneratorDataInput, GeneratorDescriptor,
    GeneratorRequirements, GeneratorTaskInput, GeneratorValidationReportInput,
};

pub(crate) fn generate_code_artifact_to_sink(
    generator: &dyn CodeGenerator,
    input: &GeneratorCommonInput<'_>,
    sink: &ArtifactSink,
) -> Result<()> {
    let code_input = input.code_export_input(generator)?;
    generator.generate_to_sink(&code_input, sink)
}

pub(crate) trait ArtifactGenerator: Send + Sync {
    fn type_id(&self) -> &'static str;

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn requirements(&self) -> GeneratorRequirements;

    fn descriptor(&self) -> GeneratorDescriptor {
        GeneratorDescriptor::new(self.type_id(), self.aliases(), self.requirements())
    }

    fn generate_to_sink(&self, input: &GeneratorCommonInput<'_>, sink: &ArtifactSink)
    -> Result<()>;

    /// Declares every output path, including auxiliary files, without writing artifacts.
    /// Paths must depend only on schema, target tags and task options, never row contents.
    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>>;

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let _ = (input, sink);
        Err(generator_contract_error(
            self.type_id(),
            format!(
                "generator `{}` does not support Data-backed artifact generation",
                self.type_id()
            ),
            "data-backed generation call",
            "meta-only generation pipeline",
            "Route meta-only generators through generate_to_sink instead of generate_with_data_to_sink.",
        ))
    }

    fn generate_with_validation_report_to_sink(
        &self,
        input: &GeneratorValidationReportInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let _ = (input, sink);
        Err(generator_contract_error(
            self.type_id(),
            format!(
                "generator `{}` does not support validation-report-only artifact generation",
                self.type_id()
            ),
            "validation-report-only generation call",
            "data-backed generation pipeline",
            "Route validation-report-only fallback only to generators that declare validation report support.",
        ))
    }

    fn plan_task_artifacts(&self, input: &GeneratorTaskInput<'_, '_>) -> Result<Vec<PathBuf>> {
        self.plan_artifacts(&input.common)
    }

    fn generate(&self, input: &GeneratorTaskInput<'_, '_>, sink: &ArtifactSink) -> Result<()> {
        if input.prepared.is_validation_report_only() && self.requirements().needs_validation_report
        {
            let report_input = GeneratorValidationReportInput {
                common: input.common.clone(),
                validation_report: input.prepared.validation_report(self.type_id())?,
            };
            return self.generate_with_validation_report_to_sink(&report_input, sink);
        }
        if self.requirements().needs_data {
            let data_input = GeneratorDataInput {
                common: input.common.clone(),
                data_set: input.prepared.data_set(self.type_id())?,
                validation_report: input.prepared.validation_report(self.type_id())?,
                pool: input.prepared.pool(self.type_id())?,
            };
            self.generate_with_data_to_sink(&data_input, sink)
        } else {
            self.generate_to_sink(&input.common, sink)
        }
    }
}

pub(crate) fn generator_contract_error(
    generator_type: impl Into<String>,
    message: impl Into<String>,
    current: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    let generator_type = generator_type.into();
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(format!("generator.{generator_type}.contract"))
        .with_current_value(current)
        .with_expected(expected)
        .with_hint(hint),
    )
}
