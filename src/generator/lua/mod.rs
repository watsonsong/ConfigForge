// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod naming;
mod options;
mod render;

#[cfg(test)]
mod tests;

use std::{collections::BTreeSet, path::PathBuf};

use crate::{
    ConfigForgeError, Result,
    artifact::{Artifact, ArtifactSink},
    data::DataSet,
    data_artifact::{AggregateDataExporter, TargetDataConfig, build_exported_table_payload},
    meta::{LinkedMetaModel, LinkedTableMeta, TargetTags},
};

use super::{ArtifactGenerator, GeneratorCommonInput, GeneratorDataInput, GeneratorRequirements};
use naming::{root_path, table_file_name, table_path};
use options::{LuaTableOptions, options_from_task, validate_options};
use render::{field_bindings, render_root, render_table_file, row_helper_names};

pub const FORMAT: &str = "lua_table";

pub struct LuaTableExporter;

impl AggregateDataExporter for LuaTableExporter {
    fn name(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["lua"]
    }

    fn plan_artifacts(
        &self,
        out_dir: &std::path::Path,
        task: &TargetDataConfig,
        linked_meta: &LinkedMetaModel<'_>,
        target_tags: &TargetTags,
    ) -> Result<Vec<PathBuf>> {
        let options = options_from_task(task)?;
        validate_options(&options)?;
        let tables = visible_tables(linked_meta, target_tags).collect::<Vec<_>>();
        validate_table_symbols(&tables, &options, target_tags)?;
        let mut paths = vec![out_dir.join(root_path(&options))];
        for table_meta in tables {
            paths.push(out_dir.join(table_path(&options, table_meta)));
        }
        Ok(paths)
    }

    fn export(
        &self,
        out_dir: &std::path::Path,
        task: &TargetDataConfig,
        linked_meta: &LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
    ) -> Result<Vec<Artifact>> {
        let options = options_from_task(task)?;
        validate_options(&options)?;
        let tables = visible_tables(linked_meta, target_tags).collect::<Vec<_>>();
        validate_table_symbols(&tables, &options, target_tags)?;

        let mut artifacts = vec![Artifact::text(
            out_dir.join(root_path(&options)),
            render_root(&options, &tables),
        )];
        for table_meta in tables {
            let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "data set is missing table `{}`",
                    table_meta.meta.name
                ))
            })?;
            let payload = build_exported_table_payload(table_meta, table, target_tags)?;
            let content =
                render_table_file(table_meta, &payload, &options, linked_meta, target_tags)?;
            artifacts.push(Artifact::text(
                out_dir.join(table_path(&options, table_meta)),
                content,
            ));
        }
        Ok(artifacts)
    }

    fn export_to_sink(
        &self,
        out_dir: &std::path::Path,
        task: &TargetDataConfig,
        linked_meta: &LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let options = options_from_task(task)?;
        validate_options(&options)?;
        let tables = visible_tables(linked_meta, target_tags).collect::<Vec<_>>();
        validate_table_symbols(&tables, &options, target_tags)?;

        let root_content = render_root(&options, &tables);
        sink.write_text(out_dir.join(root_path(&options)), root_content)?;

        for table_meta in tables {
            let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "data set is missing table `{}`",
                    table_meta.meta.name
                ))
            })?;
            let payload = build_exported_table_payload(table_meta, table, target_tags)?;
            let content =
                render_table_file(table_meta, &payload, &options, linked_meta, target_tags)?;
            sink.write_text(out_dir.join(table_path(&options, table_meta)), content)?;
        }
        Ok(())
    }
}

impl ArtifactGenerator for LuaTableExporter {
    fn type_id(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["lua"]
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_and_data()
    }

    fn generate_to_sink(
        &self,
        _input: &GeneratorCommonInput<'_>,
        _sink: &ArtifactSink,
    ) -> Result<()> {
        Err(ConfigForgeError::Validation(format!(
            "generator `{}` requires Data-backed artifact input",
            FORMAT
        )))
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        let task = input.target_data_config();
        <Self as AggregateDataExporter>::plan_artifacts(
            self,
            &input.output,
            &task,
            input.meta,
            &input.target_tags,
        )
    }

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let task = input.common.target_data_config();
        <Self as AggregateDataExporter>::export_to_sink(
            self,
            &input.common.output,
            &task,
            input.common.meta,
            &input.common.target_tags,
            input.data_set,
            sink,
        )
    }
}

fn visible_tables<'a>(
    linked_meta: &'a LinkedMetaModel<'_>,
    target_tags: &'a TargetTags,
) -> impl Iterator<Item = &'a LinkedTableMeta<'a>> {
    linked_meta
        .tables
        .iter()
        .filter(|table| table.meta.is_exported_for(target_tags))
}

fn validate_table_symbols(
    tables: &[&LinkedTableMeta<'_>],
    options: &LuaTableOptions,
    target_tags: &TargetTags,
) -> Result<()> {
    let mut files = BTreeSet::new();
    for table in tables {
        let file = table_file_name(options, table);
        if !files.insert(file.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "lua_table table file `{file}` is generated by multiple tables"
            )));
        }
        let mut names = BTreeSet::new();
        for binding in field_bindings(table, options, target_tags)? {
            if !names.insert(binding.lua_name.clone()) {
                return Err(ConfigForgeError::Validation(format!(
                    "lua_table table `{}` generates duplicate field `{}`",
                    table.meta.name, binding.lua_name
                )));
            }
        }
        for helper_name in row_helper_names(table, target_tags)? {
            if !names.insert(helper_name.clone()) {
                return Err(ConfigForgeError::Validation(format!(
                    "lua_table table `{}` generates duplicate row helper `{}`",
                    table.meta.name, helper_name
                )));
            }
        }
    }
    Ok(())
}
