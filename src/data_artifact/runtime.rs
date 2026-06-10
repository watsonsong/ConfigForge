// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, path::Path};

use rayon::prelude::*;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    artifact::ArtifactSink,
    data::DataSet,
    diagnostic::code::{CF_ARTIFACT_PATH_COLLISION, CF_GENERATOR_UNSUPPORTED_DATA_INPUT},
    meta::{LinkedMetaModel, TargetTags},
    runtime_contract::RuntimeDataFile,
};

use super::RuntimeTableFormat;

pub(crate) fn export_runtime_data_to_sink(
    out_dir: &Path,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
    exporter: &dyn RuntimeTableFormat,
    pool: &rayon::ThreadPool,
    sink: &ArtifactSink,
) -> Result<Vec<RuntimeDataFile>> {
    let mut plans = Vec::new();
    for table_meta in &linked_meta.tables {
        if table_meta.meta.is_exported_for(target_tags) {
            plans.push(exporter.plan_table_artifact(table_meta)?);
        }
    }
    ensure_unique_runtime_artifact_paths(&plans)?;

    let mut generated_results = pool.install(|| {
        plans
            .par_iter()
            .enumerate()
            .map(|(index, plan)| {
                let result: Result<RuntimeDataFile> = (|| {
                    let table_meta = linked_meta.table(&plan.table_name).ok_or_else(|| {
                        runtime_input_error(
                            &plan.table_name,
                            format!("linked meta is missing table `{}`", plan.table_name),
                            &plan.table_name,
                            "table declared in linked meta",
                            "Ensure runtime artifact plans are produced from the same linked meta model.",
                        )
                    })?;
                    let table = data_set.table(&plan.table_name).ok_or_else(|| {
                        runtime_input_error(
                            &plan.table_name,
                            format!("data set is missing table `{}`", plan.table_name),
                            &plan.table_name,
                            "resolved data table for runtime export",
                            "Ensure data loading and validation completed before runtime data export.",
                        )
                    })?;
                    let file = RuntimeDataFile {
                        table_name: plan.table_name.clone(),
                        path: plan.path.to_string_lossy().into_owned(),
                        format: exporter.name().to_owned(),
                    };
                    let mut stream = sink.create(out_dir.join(&plan.path))?;
                    exporter.serialize_table_to_stream(
                        table_meta,
                        table,
                        target_tags,
                        &mut stream,
                    )?;
                    stream.finish()?;
                    Ok(file)
                })();
                (index, result)
            })
            .collect::<Vec<_>>()
    });
    generated_results.sort_by_key(|(index, _)| *index);

    let mut files = Vec::with_capacity(generated_results.len());
    for (_, artifact) in generated_results {
        files.push(artifact?);
    }
    Ok(files)
}

pub(crate) fn runtime_table_artifacts(
    out_dir: &Path,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    exporter: &dyn RuntimeTableFormat,
) -> Result<Vec<std::path::PathBuf>> {
    linked_meta
        .tables
        .iter()
        .filter(|table_meta| table_meta.meta.is_exported_for(target_tags))
        .map(|table_meta| {
            exporter
                .plan_table_artifact(table_meta)
                .map(|plan| out_dir.join(plan.path))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeArtifactPlan {
    pub table_name: String,
    pub path: std::path::PathBuf,
}

fn ensure_unique_runtime_artifact_paths(plans: &[RuntimeArtifactPlan]) -> Result<()> {
    let mut paths = BTreeSet::new();
    for plan in plans {
        if !paths.insert(plan.path.clone()) {
            return Err(runtime_path_collision_error(
                format!(
                    "multiple runtime data artifacts target `{}`",
                    plan.path.display()
                ),
                plan.path.display().to_string(),
            ));
        }
    }
    Ok(())
}

fn runtime_input_error(
    table: impl Into<String>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                table: Some(table.into()),
                ..Default::default()
            },
        )
        .with_source("generator.data.runtime")
        .with_domain("generator")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
}

fn runtime_path_collision_error(
    message: String,
    current_value: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_ARTIFACT_PATH_COLLISION,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source("generator.data.runtime")
        .with_domain("artifact")
        .with_current_value(current_value)
        .with_expected("unique runtime artifact path per exported table")
        .with_hint(
            "Adjust table names or runtime exporter file naming so output paths are unique.",
        ),
    )
}
