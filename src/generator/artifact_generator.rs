// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    CodeGenerator, GeneratorCommonInput, GeneratorDescriptor, GeneratorRequirements,
    GeneratorTaskInput,
};
use crate::{Result, artifact::ArtifactSink};
use std::path::PathBuf;

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
    /// Declares all paths from schema, tags and options before generation starts.
    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>>;
    fn generate(&self, input: &GeneratorTaskInput<'_, '_>, sink: &ArtifactSink) -> Result<()>;
}
