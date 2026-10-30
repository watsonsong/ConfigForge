// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::target::{NormalizedExportTask, NormalizedTargetConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TargetBuildPlan {
    pub(super) meta_only_exports: Vec<usize>,
    pub(super) meta_and_data_exports: Vec<usize>,
}

impl TargetBuildPlan {
    pub(super) fn new(target: &NormalizedTargetConfig) -> Self {
        let mut meta_only_exports = Vec::new();
        let mut meta_and_data_exports = Vec::new();
        for (index, task) in target.exports.iter().enumerate() {
            if task.requirements.needs_data {
                meta_and_data_exports.push(index);
            } else {
                meta_only_exports.push(index);
            }
        }
        Self {
            meta_only_exports,
            meta_and_data_exports,
        }
    }

    pub(super) fn requires_data(&self) -> bool {
        !self.meta_and_data_exports.is_empty()
    }

    pub(super) fn meta_only_export_tasks<'a>(
        &'a self,
        target: &'a NormalizedTargetConfig,
    ) -> Vec<(usize, &'a NormalizedExportTask)> {
        self.export_tasks_by_indices(target, &self.meta_only_exports)
    }

    pub(super) fn meta_and_data_export_tasks<'a>(
        &'a self,
        target: &'a NormalizedTargetConfig,
    ) -> Vec<(usize, &'a NormalizedExportTask)> {
        self.export_tasks_by_indices(target, &self.meta_and_data_exports)
    }

    fn export_tasks_by_indices<'a>(
        &'a self,
        target: &'a NormalizedTargetConfig,
        indices: &[usize],
    ) -> Vec<(usize, &'a NormalizedExportTask)> {
        indices
            .iter()
            .map(|index| (*index, &target.exports[*index]))
            .collect()
    }
}

pub(super) fn build_execution_view_detail(
    target: &NormalizedTargetConfig,
    plan: &TargetBuildPlan,
) -> String {
    let data_mode = if plan.requires_data() {
        "with_data"
    } else {
        "meta_only"
    };
    let exports = target
        .exports
        .iter()
        .map(|task| task.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "target={}, mode={}, meta_only_exports={}, meta_and_data_exports={}, exports=[{}]",
        target.name,
        data_mode,
        plan.meta_only_exports.len(),
        plan.meta_and_data_exports.len(),
        exports
    )
}
