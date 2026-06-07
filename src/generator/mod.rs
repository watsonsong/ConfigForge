// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

mod adapter;
mod artifact_generator;
mod input;
mod registry;
mod requirements;
mod validation_report;

pub(crate) use artifact_generator::ArtifactGenerator;
pub(crate) use input::{
    GeneratorCommonInput, GeneratorDataInput, GeneratorTaskInput, PreparedGeneratorInputs,
};
pub(crate) use registry::GeneratorRegistry;
pub(crate) use requirements::{GeneratorDescriptor, GeneratorRequirements};
