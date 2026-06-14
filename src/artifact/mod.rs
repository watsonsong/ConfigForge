// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod model;
mod path;
mod writer;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use model::{
    Artifact, ArtifactContent, ArtifactPlan, ArtifactWriteEntry, ArtifactWriteMode,
    ArtifactWriteResult, ArtifactWriter,
};
#[allow(unused_imports)]
pub use writer::{ArtifactProducer, ArtifactSink, ArtifactWriteStream};
