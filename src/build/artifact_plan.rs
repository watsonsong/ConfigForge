// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use rayon::{ThreadPool, prelude::*};

use crate::{
    Result,
    artifact::ArtifactSink,
    generator::{GeneratorCommonInput, GeneratorCommonInputContext, GeneratorRegistry},
};

use super::target::NormalizedExportTask;

pub(super) fn plan_target_artifacts(
    export_tasks: &[&NormalizedExportTask],
    context: GeneratorCommonInputContext<'_, '_>,
    registry: &GeneratorRegistry,
    pool: &ThreadPool,
    sink: &ArtifactSink,
) -> Result<()> {
    let planned_by_task = pool.install(|| {
        export_tasks
            .par_iter()
            .map(|task| {
                registry.require_generator(&task.type_id)?.plan_artifacts(
                    &GeneratorCommonInput::from_task_context(
                        context,
                        &task.name,
                        &task.type_id,
                        &task.output,
                        &task.options,
                    ),
                )
            })
            .collect::<Vec<_>>()
    });
    let mut paths = Vec::new();
    for task_paths in planned_by_task {
        paths.extend(task_paths?);
    }
    sink.reserve_paths(paths)
}
