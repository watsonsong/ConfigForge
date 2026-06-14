// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use crate::{Result, diagnostic::code::CF_BUILD_INVALID_CONFIG, meta::TargetTags};

use super::{
    build_diagnostic_error,
    target::{NormalizedExportTask, NormalizedTargetConfig},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BuildDependencyPlan {
    targets: Vec<TargetBuildPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TargetBuildPlan {
    pub(super) target_name: String,
    pub(super) effective_tags: Vec<String>,
    pub(super) requires_meta: bool,
    pub(super) data_dependency: Option<DataDependencyKey>,
    pub(super) meta_only_exports: Vec<usize>,
    pub(super) meta_and_data_exports: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DataDependencyKey {
    pub(super) input: Option<PathBuf>,
    pub(super) effective_tags: Vec<String>,
}

impl BuildDependencyPlan {
    pub(super) fn for_target(
        target: &NormalizedTargetConfig,
        data_root: Option<&Path>,
        target_tags: &TargetTags,
    ) -> Self {
        Self {
            targets: vec![TargetBuildPlan::new(target, data_root, target_tags)],
        }
    }

    pub(super) fn single_target_plan(&self) -> Result<&TargetBuildPlan> {
        match self.targets.as_slice() {
            [target] => Ok(target),
            _ => Err(build_diagnostic_error(
                CF_BUILD_INVALID_CONFIG,
                "build.dependency_plan",
                "expected exactly one target in build dependency plan".to_owned(),
                self.targets.len().to_string(),
                "exactly one target build plan",
                "Check target build plan construction before executing the pipeline.",
            )),
        }
    }
}

impl TargetBuildPlan {
    pub(super) fn new(
        target: &NormalizedTargetConfig,
        data_root: Option<&Path>,
        target_tags: &TargetTags,
    ) -> Self {
        let effective_tags = target_tags.iter().map(str::to_owned).collect::<Vec<_>>();
        let mut meta_only_exports = Vec::new();
        let mut meta_and_data_exports = Vec::new();
        for (index, task) in target.exports.iter().enumerate() {
            if task.requirements.needs_data {
                meta_and_data_exports.push(index);
            } else {
                meta_only_exports.push(index);
            }
        }
        let requires_meta = !target.exports.is_empty();
        let data_dependency = (!meta_and_data_exports.is_empty()).then(|| DataDependencyKey {
            input: data_root.map(Path::to_path_buf),
            effective_tags: effective_tags.clone(),
        });
        Self {
            target_name: target.name.clone(),
            effective_tags,
            requires_meta,
            data_dependency,
            meta_only_exports,
            meta_and_data_exports,
        }
    }

    pub(super) fn requires_data(&self) -> bool {
        self.data_dependency.is_some()
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

    pub(super) fn export_indices_in_declaration_order(&self) -> Vec<usize> {
        let mut indices =
            Vec::with_capacity(self.meta_only_exports.len() + self.meta_and_data_exports.len());
        indices.extend(self.meta_only_exports.iter().copied());
        indices.extend(self.meta_and_data_exports.iter().copied());
        indices.sort_unstable();
        indices
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
    let exports = plan
        .export_indices_in_declaration_order()
        .iter()
        .map(|index| target.exports[*index].name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "target={}, mode={}, meta_only_exports={}, meta_and_data_exports={}, exports=[{}]",
        plan.target_name,
        data_mode,
        plan.meta_only_exports.len(),
        plan.meta_and_data_exports.len(),
        exports
    )
}
