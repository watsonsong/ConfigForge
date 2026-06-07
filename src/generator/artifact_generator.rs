// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{ConfigForgeError, Result, artifact::ArtifactSink};

use super::{
    GeneratorCommonInput, GeneratorDataInput, GeneratorDescriptor, GeneratorRequirements,
    GeneratorTaskInput,
};

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

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        let _ = input;
        Ok(Vec::new())
    }

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let _ = (input, sink);
        Err(ConfigForgeError::Validation(format!(
            "generator `{}` does not support Data-backed artifact generation",
            self.type_id()
        )))
    }

    fn plan_task_artifacts(&self, input: &GeneratorTaskInput<'_, '_>) -> Result<Vec<PathBuf>> {
        self.plan_artifacts(&input.common)
    }

    fn generate(&self, input: &GeneratorTaskInput<'_, '_>, sink: &ArtifactSink) -> Result<()> {
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
