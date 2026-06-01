// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, path::Path};

use rayon::prelude::*;

use crate::{
    ConfigForgeError, Result,
    artifact::{
        Artifact, ArtifactGenerationLimiter, ArtifactPlan, ArtifactProducer, ArtifactSink,
        ArtifactWriteStream,
    },
    data::DataSet,
    meta::{LinkedMetaModel, LinkedTableMeta, TargetTags},
    runtime_contract::RuntimeDataFile,
};

use super::DataFormatExporter;

#[allow(dead_code)]
pub(crate) fn export_runtime_data(
    out_dir: &Path,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
    exporter: &dyn DataFormatExporter,
    pool: &rayon::ThreadPool,
) -> Result<Vec<RuntimeDataArtifact>> {
    let mut plans = Vec::new();
    for table_meta in &linked_meta.tables {
        if table_meta.meta.is_exported_for(target_tags) {
            plans.push(exporter.plan_table_artifact(table_meta)?);
        }
    }
    ensure_unique_runtime_artifact_paths(&plans)?;

    // Keep at most two fully serialized artifacts in memory at once. Each artifact
    // can contain a large Vec<u8>, so this intentionally does not scale linearly
    // with --jobs until streaming/temp-file export is introduced.
    const MAX_IN_FLIGHT_RUNTIME_ARTIFACTS: usize = 2;
    let max_in_flight_artifacts = pool
        .current_num_threads()
        .clamp(1, MAX_IN_FLIGHT_RUNTIME_ARTIFACTS);
    let mut artifacts = Vec::new();
    for chunk in plans.chunks(max_in_flight_artifacts) {
        let mut generated_results = pool.install(|| {
            chunk
                .par_iter()
                .enumerate()
                .map(|(index, plan)| {
                    let result: Result<GeneratedRuntimeArtifact> = (|| {
                        let table_meta = linked_meta.table(&plan.table_name).ok_or_else(|| {
                            ConfigForgeError::Validation(format!(
                                "linked meta is missing table `{}`",
                                plan.table_name
                            ))
                        })?;
                        let table = data_set.table(&plan.table_name).ok_or_else(|| {
                            ConfigForgeError::Validation(format!(
                                "data set is missing table `{}`",
                                plan.table_name
                            ))
                        })?;
                        let content = exporter.serialize_table(table_meta, table, target_tags)?;
                        Ok(GeneratedRuntimeArtifact {
                            plan: plan.clone(),
                            content,
                        })
                    })();
                    (index, result)
                })
                .collect::<Vec<_>>()
        });
        generated_results.sort_by_key(|(index, _)| *index);

        for (_, artifact) in generated_results {
            let artifact = artifact?;
            let path = out_dir.join(&artifact.plan.path);
            let file = RuntimeDataFile {
                table_name: artifact.plan.table_name,
                path: artifact.plan.path.to_string_lossy().into_owned(),
                format: exporter.name().to_owned(),
            };
            artifacts.push(RuntimeDataArtifact {
                file,
                artifact: Artifact::binary(path, artifact.content),
            });
        }
    }
    Ok(artifacts)
}

pub(crate) fn export_runtime_data_to_sink(
    out_dir: &Path,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
    exporter: &dyn DataFormatExporter,
    pool: &rayon::ThreadPool,
    sink: &ArtifactSink,
    generation_limiter: &ArtifactGenerationLimiter,
) -> Result<Vec<RuntimeDataFile>> {
    let mut plans = Vec::new();
    for table_meta in &linked_meta.tables {
        if table_meta.meta.is_exported_for(target_tags) {
            plans.push(exporter.plan_table_artifact(table_meta)?);
        }
    }
    ensure_unique_runtime_artifact_paths(&plans)?;

    const MAX_IN_FLIGHT_RUNTIME_ARTIFACTS: usize = 2;
    let max_in_flight_artifacts = pool
        .current_num_threads()
        .clamp(1, MAX_IN_FLIGHT_RUNTIME_ARTIFACTS);
    let mut files = Vec::new();
    for chunk in plans.chunks(max_in_flight_artifacts) {
        let mut generated_results = pool.install(|| {
            chunk
                .par_iter()
                .enumerate()
                .map(|(index, plan)| {
                    let result: Result<RuntimeDataFile> = (|| {
                        let table_meta = linked_meta.table(&plan.table_name).ok_or_else(|| {
                            ConfigForgeError::Validation(format!(
                                "linked meta is missing table `{}`",
                                plan.table_name
                            ))
                        })?;
                        let table = data_set.table(&plan.table_name).ok_or_else(|| {
                            ConfigForgeError::Validation(format!(
                                "data set is missing table `{}`",
                                plan.table_name
                            ))
                        })?;
                        let file = RuntimeDataFile {
                            table_name: plan.table_name.clone(),
                            path: plan.path.to_string_lossy().into_owned(),
                            format: exporter.name().to_owned(),
                        };
                        let producer = RuntimeTableArtifactProducer {
                            artifact_plan: ArtifactPlan::new(out_dir.join(&plan.path), 1),
                            table_meta,
                            table,
                            target_tags,
                            exporter,
                        };
                        generation_limiter.run_unknown(|| sink.write_producer(producer))?;
                        Ok(file)
                    })();
                    (index, result)
                })
                .collect::<Vec<_>>()
        });
        generated_results.sort_by_key(|(index, _)| *index);

        for (_, artifact) in generated_results {
            files.push(artifact?);
        }
    }
    Ok(files)
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeDataArtifact {
    pub file: RuntimeDataFile,
    pub artifact: Artifact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeArtifactPlan {
    pub table_name: String,
    pub path: std::path::PathBuf,
}

struct GeneratedRuntimeArtifact {
    plan: RuntimeArtifactPlan,
    content: Vec<u8>,
}

struct RuntimeTableArtifactProducer<'a, 'meta> {
    artifact_plan: ArtifactPlan,
    table_meta: &'a LinkedTableMeta<'meta>,
    table: &'a crate::data::DataTable,
    target_tags: &'a TargetTags,
    exporter: &'a dyn DataFormatExporter,
}

impl ArtifactProducer for RuntimeTableArtifactProducer<'_, '_> {
    fn plan(&self) -> &ArtifactPlan {
        &self.artifact_plan
    }

    fn produce(self, stream: &mut ArtifactWriteStream) -> Result<()> {
        self.exporter.serialize_table_to_stream(
            self.table_meta,
            self.table,
            self.target_tags,
            stream,
        )
    }
}

fn ensure_unique_runtime_artifact_paths(plans: &[RuntimeArtifactPlan]) -> Result<()> {
    let mut paths = BTreeSet::new();
    for plan in plans {
        if !paths.insert(plan.path.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "multiple runtime data artifacts target `{}`",
                plan.path.display()
            )));
        }
    }
    Ok(())
}
