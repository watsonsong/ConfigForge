// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, path::PathBuf};

use serde::Deserialize;

use crate::{
    ConfigForgeError, Result,
    artifact::{Artifact, ArtifactSink},
    codegen::{
        CaseStyle, CodeArtifactOptions, CodeWriterConfig, I18nFunctionReturn, I18nMode,
        I18nOptions, KeywordConflictPolicy, NamingConfig,
    },
    data::DataSet,
    exporter::{
        ExportedTablePayload, ExportedValue, TargetDataConfig, build_exported_table_payload,
    },
    meta::{
        LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta, LinkedFragmentPayload, LinkedMetaModel,
        LinkedStructType, LinkedTableMeta, PrimitiveFieldType, TableKind, TargetTags,
    },
};

use super::AggregateDataExporter;

pub const FORMAT: &str = "lua_table";

pub struct LuaTableExporter;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct LuaTableOptions {
    file_header: Option<String>,
    module_name: String,
    root_file: String,
    table_file_naming: NamingMode,
    row_layout: RowLayout,
    object_layout: ObjectLayout,
    index_key: IndexKeyMode,
    lazy_require: bool,
    emit_schema: bool,
    readonly: bool,
    i18n: I18nOptions,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum NamingMode {
    SnakeCase,
    Preserve,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RowLayout {
    ArrayWithMetatable,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ObjectLayout {
    KeyedTable,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum IndexKeyMode {
    NestedTable,
}

#[derive(Debug, Clone)]
struct LuaFieldBinding<'meta> {
    source_name: &'meta str,
    lua_name: String,
    value_type: LuaValueType<'meta>,
}

#[derive(Debug, Clone)]
enum LuaValueType<'meta> {
    Field(&'meta LinkedField<'meta>),
    InlineFragment(&'meta [LinkedFieldMeta<'meta>]),
    StructFragment(&'meta LinkedStructType<'meta>),
}

impl Default for LuaTableOptions {
    fn default() -> Self {
        Self {
            file_header: None,
            module_name: "config".to_owned(),
            root_file: "init.lua".to_owned(),
            table_file_naming: NamingMode::SnakeCase,
            row_layout: RowLayout::ArrayWithMetatable,
            object_layout: ObjectLayout::KeyedTable,
            index_key: IndexKeyMode::NestedTable,
            lazy_require: true,
            emit_schema: false,
            readonly: true,
            i18n: I18nOptions::default(),
        }
    }
}

impl LuaTableOptions {
    fn code_artifact_options(&self) -> CodeArtifactOptions {
        CodeArtifactOptions {
            file_header: self.file_header.clone(),
            keyword_conflict: KeywordConflictPolicy::SuffixUnderscore,
            i18n: self.i18n.clone(),
            naming: NamingConfig {
                type_name: CaseStyle::Preserve,
                enum_value: CaseStyle::Preserve,
                field: match self.table_file_naming {
                    NamingMode::SnakeCase => CaseStyle::SnakeCase,
                    NamingMode::Preserve => CaseStyle::Preserve,
                },
                function: CaseStyle::SnakeCase,
                constant: CaseStyle::ScreamingSnakeCase,
            },
            formatting: CodeWriterConfig::default(),
            language_options: std::collections::HashMap::new(),
        }
    }
}

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
        let mut root_stream = sink.create(out_dir.join(root_path(&options)))?;
        root_stream.write_all_artifact(root_content.as_bytes())?;
        root_stream.finish()?;

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
            let mut table_stream = sink.create(out_dir.join(table_path(&options, table_meta)))?;
            table_stream.write_all_artifact(content.as_bytes())?;
            table_stream.finish()?;
        }
        Ok(())
    }
}

fn options_from_task(task: &TargetDataConfig) -> Result<LuaTableOptions> {
    if let Some(value) = task.options.get("lua") {
        return value.clone().try_into().map_err(|error| {
            ConfigForgeError::Validation(format!("invalid lua_table options: {error}"))
        });
    }
    if task.options.is_empty() {
        return Ok(LuaTableOptions::default());
    }
    toml::Value::Table(task.options.clone().into_iter().collect())
        .try_into()
        .map_err(|error| {
            ConfigForgeError::Validation(format!("invalid lua_table options: {error}"))
        })
}

fn validate_options(options: &LuaTableOptions) -> Result<()> {
    if options.module_name.trim().is_empty() {
        return Err(ConfigForgeError::Validation(
            "lua_table module_name must not be empty".to_owned(),
        ));
    }
    if options.root_file.trim().is_empty()
        || options.root_file.contains('/')
        || options.root_file.contains('\\')
    {
        return Err(ConfigForgeError::Validation(
            "lua_table root_file must be a file name".to_owned(),
        ));
    }
    if options.module_name.split('.').any(str::is_empty) {
        return Err(ConfigForgeError::Validation(format!(
            "lua_table module_name `{}` contains an empty segment",
            options.module_name
        )));
    }
    if let Some(file_header) = &options.file_header
        && file_header.chars().any(|character| character == '\0')
    {
        return Err(ConfigForgeError::Validation(
            "lua_table file_header must not contain NUL bytes".to_owned(),
        ));
    }
    if matches!(options.i18n.mode, I18nMode::GlobalFunction) {
        let Some(function) = &options.i18n.function else {
            return Err(ConfigForgeError::Validation(
                "lua_table i18n global_function mode requires function".to_owned(),
            ));
        };
        if function.trim().is_empty() || function.chars().any(char::is_control) {
            return Err(ConfigForgeError::Validation(format!(
                "invalid lua_table i18n function `{function}`; value must be non-empty and single-line"
            )));
        }
    }
    Ok(())
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

fn row_helper_names(table: &LinkedTableMeta<'_>, target_tags: &TargetTags) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for field in table
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .filter(|field| field.primitive_type() == Some(PrimitiveFieldType::Text))
    {
        names.push(text_helper_name(&field.meta.name));
    }
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.is_exported_for(target_tags))
        .filter_map(|foreign_key| foreign_key.linked_field)
    {
        names.push(sanitize_identifier(foreign_key));
    }
    Ok(names)
}

fn root_path(options: &LuaTableOptions) -> PathBuf {
    let mut path = module_dir(options);
    path.push(&options.root_file);
    path
}

fn table_path(options: &LuaTableOptions, table_meta: &LinkedTableMeta<'_>) -> PathBuf {
    let mut path = module_dir(options);
    path.push(table_file_name(options, table_meta));
    path
}

fn module_dir(options: &LuaTableOptions) -> PathBuf {
    options.module_name.split('.').collect::<PathBuf>()
}

fn table_file_name(options: &LuaTableOptions, table_meta: &LinkedTableMeta<'_>) -> String {
    format!("{}.lua", table_module_segment(options, table_meta))
}

fn table_require_path(options: &LuaTableOptions, table_meta: &LinkedTableMeta<'_>) -> String {
    format!(
        "{}.{}",
        options.module_name,
        table_module_segment(options, table_meta)
    )
}

fn table_module_segment(options: &LuaTableOptions, table_meta: &LinkedTableMeta<'_>) -> String {
    match options.table_file_naming {
        NamingMode::SnakeCase => snake_case(&table_meta.meta.name),
        NamingMode::Preserve => sanitize_identifier(&table_meta.meta.name),
    }
}

fn render_root(options: &LuaTableOptions, tables: &[&LinkedTableMeta<'_>]) -> String {
    let mut content = String::new();
    content.push_str(&render_file_header(options));
    content.push_str("local M = {}\n");
    if options.lazy_require {
        content.push_str("local cache = {}\n\n");
    } else {
        content.push('\n');
    }
    for table in tables {
        let lua_name = table_accessor_name(table);
        let require_path = table_require_path(options, table);
        if options.lazy_require {
            content.push_str(&format!(
                "local function load_{lua_name}()\n  local value = cache.{lua_name}\n  if value == nil then\n    value = require({})\n    cache.{lua_name} = value\n  end\n  return value\nend\n\n",
                lua_string(&require_path)
            ));
            content.push_str(&format!(
                "function M.{lua_name}()\n  return load_{lua_name}()\nend\n\n"
            ));
        } else {
            content.push_str(&format!(
                "local {lua_name}_module = require({})\n\nfunction M.{lua_name}()\n  return {lua_name}_module\nend\n\n",
                lua_string(&require_path)
            ));
        }
    }
    content.push_str("function M.preload_all()\n");
    for table in tables {
        let lua_name = table_accessor_name(table);
        content.push_str(&format!("  M.{lua_name}()\n"));
    }
    content.push_str("  return M\nend\n\nreturn M\n");
    content
}

fn render_table_file(
    table_meta: &LinkedTableMeta<'_>,
    payload: &ExportedTablePayload,
    options: &LuaTableOptions,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
) -> Result<String> {
    match (table_meta.meta.kind, payload) {
        (TableKind::Collection, ExportedTablePayload::Collection(table)) => {
            render_collection_table(table_meta, &table.rows, options, linked_meta, target_tags)
        }
        (TableKind::Object, ExportedTablePayload::Object(object)) => {
            render_object_table(table_meta, object, options, linked_meta, target_tags)
        }
        _ => Err(ConfigForgeError::Validation(format!(
            "lua_table payload kind does not match table `{}`",
            table_meta.meta.name
        ))),
    }
}

fn render_collection_table(
    table_meta: &LinkedTableMeta<'_>,
    rows: &[std::collections::BTreeMap<String, ExportedValue>],
    options: &LuaTableOptions,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
) -> Result<String> {
    let bindings = field_bindings(table_meta, options, target_tags)?;
    let mut content = String::new();
    content.push_str(&render_file_header(options));
    content.push_str("local FIELD = {\n");
    for (index, binding) in bindings.iter().enumerate() {
        content.push_str(&format!("  {} = {},\n", binding.lua_name, index + 1));
    }
    content.push_str("}\n\nlocal ROW_METHODS = {}\nlocal ROW_MT = {\n  __index = function(row, key)\n    local index = FIELD[key]\n    if index ~= nil then\n      return row[index]\n    end\n    return ROW_METHODS[key]\n  end,\n}\n\nlocal rows = {\n");
    for row in rows {
        content.push_str("  setmetatable({");
        for (index, binding) in bindings.iter().enumerate() {
            let value = row.get(binding.source_name);
            let rendered = render_optional_lua_value(value, &binding.value_type)?;
            content.push_str(&format!(" [{}] = {},", index + 1, rendered));
        }
        content.push_str(" }, ROW_MT),\n");
    }
    content.push_str("}\n\nlocal T = { rows = rows }");
    if options.emit_schema {
        content.push_str("\nT.FIELD = FIELD");
    }
    content.push_str("\n\n");
    content.push_str(&render_text_helpers(
        table_meta,
        options,
        target_tags,
        RowValueAccess::Collection,
    )?);
    content.push_str(&render_relation_helpers(
        table_meta,
        linked_meta,
        &bindings,
        target_tags,
        RowValueAccess::Collection,
    )?);
    content.push_str(&render_indexes(table_meta, &bindings, target_tags)?);
    content.push_str("return T\n");
    Ok(content)
}

fn render_object_table(
    table_meta: &LinkedTableMeta<'_>,
    object: &std::collections::BTreeMap<String, ExportedValue>,
    options: &LuaTableOptions,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
) -> Result<String> {
    let bindings = field_bindings(table_meta, options, target_tags)?;
    let mut content = String::new();
    content.push_str(&render_file_header(options));
    content.push_str("local T = {\n");
    for binding in &bindings {
        let value = object.get(binding.source_name);
        let rendered = render_optional_lua_value(value, &binding.value_type)?;
        content.push_str(&format!("  {} = {},\n", binding.lua_name, rendered));
    }
    content.push_str("}\n\n");
    content.push_str(&render_text_helpers(
        table_meta,
        options,
        target_tags,
        RowValueAccess::Object,
    )?);
    content.push_str(&render_relation_helpers(
        table_meta,
        linked_meta,
        &bindings,
        target_tags,
        RowValueAccess::Object,
    )?);
    content.push_str("return T\n");
    Ok(content)
}

fn render_file_header(options: &LuaTableOptions) -> String {
    let code_options = options.code_artifact_options();
    if let Some(header) = &code_options.file_header {
        let mut header = header.clone();
        if !header.ends_with('\n') {
            header.push('\n');
        }
        return header;
    }
    "-- Copyright (c) 2026 ConfigForge contributors\n-- SPDX-License-Identifier: Apache-2.0\n\n-- Generated by config-forge. Do not edit.\n".to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowValueAccess {
    Collection,
    Object,
}

fn render_text_helpers(
    table_meta: &LinkedTableMeta<'_>,
    options: &LuaTableOptions,
    target_tags: &TargetTags,
    access: RowValueAccess,
) -> Result<String> {
    let mut content = String::new();
    for field in table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .filter(|field| field.primitive_type() == Some(PrimitiveFieldType::Text))
    {
        let helper_name = text_helper_name(&field.meta.name);
        let value_expr = value_expr_for_field(table_meta, &field.meta.name, access)?;
        let context = format!("{}.{}", table_meta.meta.name, field.meta.name);
        match access {
            RowValueAccess::Collection => {
                content.push_str(&format!("function ROW_METHODS.{helper_name}(row"));
                if matches!(options.i18n.mode, I18nMode::Generic) {
                    content.push_str(", translator");
                }
                content.push_str(")\n");
            }
            RowValueAccess::Object => {
                content.push_str(&format!("function T.{helper_name}("));
                if matches!(options.i18n.mode, I18nMode::Generic) {
                    content.push_str("translator");
                }
                content.push_str(")\n");
            }
        }
        render_text_helper_body(&mut content, &value_expr, &context, options);
        content.push_str("end\n\n");
    }
    Ok(content)
}

fn render_text_helper_body(
    content: &mut String,
    value_expr: &str,
    context: &str,
    options: &LuaTableOptions,
) {
    content.push_str(&format!("  local value = {value_expr}\n"));
    match options.i18n.mode {
        I18nMode::Generic => {
            content.push_str("  if translator ~= nil then\n");
            content.push_str(&format!(
                "    local translated = translator({}, value)\n",
                lua_string(context)
            ));
            content.push_str("    if translated ~= nil then return translated end\n  end\n");
            content.push_str("  return value\n");
        }
        I18nMode::GlobalFunction => {
            let function = options
                .i18n
                .function
                .as_deref()
                .expect("validated lua i18n function");
            match options.i18n.returns {
                I18nFunctionReturn::String => {
                    content.push_str(&format!(
                        "  return {function}({}, value)\n",
                        lua_string(context)
                    ));
                }
                I18nFunctionReturn::Optional => {
                    content.push_str(&format!(
                        "  local translated = {function}({}, value)\n",
                        lua_string(context)
                    ));
                    content.push_str(
                        "  if translated ~= nil then return translated end\n  return value\n",
                    );
                }
            }
        }
    }
}

fn render_relation_helpers(
    table_meta: &LinkedTableMeta<'_>,
    linked_meta: &LinkedMetaModel<'_>,
    bindings: &[LuaFieldBinding<'_>],
    target_tags: &TargetTags,
    access: RowValueAccess,
) -> Result<String> {
    let mut content = String::new();
    for foreign_key in table_meta
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.is_exported_for(target_tags))
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let helper_name = sanitize_identifier(foreign_key.linked_field.expect("linked field"));
        let target_table = linked_meta
            .table(foreign_key.target_table_name)
            .ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "lua_table foreign key `{}` references unknown target table `{}`",
                    foreign_key.name, foreign_key.target_table_name
                ))
            })?;
        let target_index = target_table
            .index(foreign_key.target_index_name)
            .ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "lua_table foreign key `{}` references unknown target index `{}.{}`",
                    foreign_key.name, foreign_key.target_table_name, foreign_key.target_index_name
                ))
            })?;
        let target_accessor = table_accessor_name(target_table);
        let target_index_accessor = sanitize_identifier(&format!("get_{}", target_index.meta.name));
        let source_exprs = relation_source_exprs(table_meta, foreign_key, bindings, access)?;
        let is_many = foreign_key.fields.len() == 1
            && matches!(&foreign_key.fields[0].linked_field, LinkedField::List(_));
        match access {
            RowValueAccess::Collection => {
                content.push_str(&format!("function ROW_METHODS.{helper_name}(row, data)\n"));
            }
            RowValueAccess::Object => {
                content.push_str(&format!("function T.{helper_name}(data)\n"));
            }
        }
        content.push_str(&format!("  local target = data.{target_accessor}()\n"));
        if is_many {
            content.push_str(&format!("  local keys = {}\n", source_exprs[0]));
            content.push_str("  if keys == nil then return {} end\n  local resolved = {}\n  for _, key in ipairs(keys) do\n");
            content.push_str(&format!(
                "    local value = target.{target_index_accessor}(key)\n"
            ));
            content.push_str(&format!(
                "    if value == nil then error({}) end\n",
                lua_string(&format!(
                    "foreign key relation `{}.{}` could not resolve target `{}.{}`",
                    table_meta.meta.name,
                    foreign_key.name,
                    target_table.meta.name,
                    target_index.meta.name
                ))
            ));
            content
                .push_str("    resolved[#resolved + 1] = value\n  end\n  return resolved\nend\n\n");
            continue;
        }
        if foreign_key.allow_empty {
            render_optional_relation_empty_check(&mut content, foreign_key, &source_exprs);
        }
        content.push_str(&format!(
            "  local value = target.{target_index_accessor}({})\n",
            source_exprs.join(", ")
        ));
        if foreign_key.allow_empty {
            content.push_str("  return value\nend\n\n");
        } else {
            content.push_str(&format!(
                "  if value == nil then error({}) end\n  return value\nend\n\n",
                lua_string(&format!(
                    "foreign key relation `{}.{}` could not resolve target `{}.{}`",
                    table_meta.meta.name,
                    foreign_key.name,
                    target_table.meta.name,
                    target_index.meta.name
                ))
            ));
        }
    }
    Ok(content)
}

fn relation_source_exprs(
    table_meta: &LinkedTableMeta<'_>,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    bindings: &[LuaFieldBinding<'_>],
    access: RowValueAccess,
) -> Result<Vec<String>> {
    foreign_key
        .fields
        .iter()
        .map(|field| {
            if access == RowValueAccess::Collection {
                let index = bindings
                    .iter()
                    .position(|binding| binding.source_name == field.meta.name)
                    .map(|position| position + 1)
                    .ok_or_else(|| {
                        ConfigForgeError::Validation(format!(
                            "lua_table foreign key `{}` references field `{}` that is not exported",
                            foreign_key.name, field.meta.name
                        ))
                    })?;
                Ok(format!("row[{index}]"))
            } else {
                value_expr_for_field(table_meta, &field.meta.name, access)
            }
        })
        .collect()
}

fn render_optional_relation_empty_check(
    content: &mut String,
    foreign_key: &LinkedForeignKeyMeta<'_>,
    source_exprs: &[String],
) {
    if source_exprs.len() == 1 {
        content.push_str(&format!(
            "  if {} == nil then return nil end\n",
            source_exprs[0]
        ));
        return;
    }
    content.push_str("  local empty_count = 0\n");
    for expr in source_exprs {
        content.push_str(&format!(
            "  if {expr} == nil then empty_count = empty_count + 1 end\n"
        ));
    }
    content.push_str(&format!(
        "  if empty_count == {} then return nil end\n",
        source_exprs.len()
    ));
    content.push_str(&format!(
        "  if empty_count ~= 0 then error({}) end\n",
        lua_string(&format!(
            "foreign key relation `{}` has partial empty source key",
            foreign_key.name
        ))
    ));
}

fn value_expr_for_field(
    table_meta: &LinkedTableMeta<'_>,
    field_name: &str,
    access: RowValueAccess,
) -> Result<String> {
    let field = table_meta.field(field_name).ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "lua_table table `{}` references unknown field `{field_name}`",
            table_meta.meta.name
        ))
    })?;
    let lua_name = sanitize_identifier(&field.meta.name);
    Ok(match access {
        RowValueAccess::Collection => format!("row[FIELD.{lua_name}]"),
        RowValueAccess::Object => format!("T.{lua_name}"),
    })
}

fn render_indexes(
    table_meta: &LinkedTableMeta<'_>,
    bindings: &[LuaFieldBinding<'_>],
    target_tags: &TargetTags,
) -> Result<String> {
    let mut content = String::new();
    for index in table_meta
        .indexes
        .iter()
        .filter(|index| index.meta.is_exported_for(target_tags))
    {
        let index_name = sanitize_identifier(&index.meta.name);
        let function_name = sanitize_identifier(&format!("get_{}", index.meta.name));
        content.push_str(&format!("local {index_name} = {{}}\n"));
        content.push_str("for _, row in ipairs(rows) do\n");
        let field_indices = index
            .fields
            .iter()
            .map(|field| {
                bindings
                    .iter()
                    .position(|binding| binding.source_name == field.meta.name)
                    .map(|position| position + 1)
                    .ok_or_else(|| {
                        ConfigForgeError::Validation(format!(
                            "lua_table index `{}` references field `{}` that is not exported",
                            index.meta.name, field.meta.name
                        ))
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        for (level, field_index) in field_indices.iter().enumerate() {
            content.push_str(&format!("  local k{} = row[{}]\n", level + 1, field_index));
        }
        if index.meta.unique || index.meta.primary {
            render_unique_index_insert(&mut content, &index_name, field_indices.len());
        } else {
            render_multi_index_insert(&mut content, &index_name, field_indices.len());
        }
        content.push_str("end\n\n");
        render_index_accessor(
            &mut content,
            &function_name,
            &index_name,
            &index
                .fields
                .iter()
                .map(|field| sanitize_identifier(&field.meta.name))
                .collect::<Vec<_>>(),
        );
    }
    Ok(content)
}

fn render_unique_index_insert(content: &mut String, index_name: &str, key_count: usize) {
    if key_count == 1 {
        content.push_str(&format!("  {index_name}[k1] = row\n"));
        return;
    }
    render_nested_prefix(content, index_name, key_count);
    content.push_str(&format!(
        "  level_{}[k{}] = row\n",
        key_count - 1,
        key_count
    ));
}

fn render_multi_index_insert(content: &mut String, index_name: &str, key_count: usize) {
    if key_count == 1 {
        content.push_str(&format!(
            "  local bucket = {index_name}[k1]\n  if bucket == nil then\n    bucket = {{}}\n    {index_name}[k1] = bucket\n  end\n  bucket[#bucket + 1] = row\n"
        ));
        return;
    }
    render_nested_prefix(content, index_name, key_count);
    content.push_str(&format!(
        "  local bucket = level_{}[k{}]\n  if bucket == nil then\n    bucket = {{}}\n    level_{}[k{}] = bucket\n  end\n  bucket[#bucket + 1] = row\n",
        key_count - 1,
        key_count,
        key_count - 1,
        key_count
    ));
}

fn render_nested_prefix(content: &mut String, index_name: &str, key_count: usize) {
    content.push_str(&format!("  local level_1 = {index_name}[k1]\n"));
    content.push_str(&format!(
        "  if level_1 == nil then\n    level_1 = {{}}\n    {index_name}[k1] = level_1\n  end\n"
    ));
    for level in 2..key_count {
        content.push_str(&format!(
            "  local level_{level} = level_{}[k{level}]\n",
            level - 1
        ));
        content.push_str(&format!(
            "  if level_{level} == nil then\n    level_{level} = {{}}\n    level_{}[k{level}] = level_{level}\n  end\n",
            level - 1
        ));
    }
}

fn render_index_accessor(
    content: &mut String,
    function_name: &str,
    index_name: &str,
    args: &[String],
) {
    content.push_str(&format!(
        "function T.{function_name}({})\n",
        args.join(", ")
    ));
    if args.len() == 1 {
        content.push_str(&format!("  return {index_name}[{}]\nend\n\n", args[0]));
        return;
    }
    content.push_str(&format!("  local level_1 = {index_name}[{}]\n", args[0]));
    content.push_str("  if level_1 == nil then return nil end\n");
    for (position, arg) in args
        .iter()
        .enumerate()
        .skip(1)
        .take(args.len().saturating_sub(2))
    {
        content.push_str(&format!(
            "  local level_{} = level_{}[{arg}]\n",
            position + 1,
            position
        ));
        content.push_str(&format!(
            "  if level_{} == nil then return nil end\n",
            position + 1
        ));
    }
    content.push_str(&format!(
        "  return level_{}[{}]\nend\n\n",
        args.len() - 1,
        args[args.len() - 1]
    ));
}

fn field_bindings<'meta>(
    table_meta: &'meta LinkedTableMeta<'meta>,
    _options: &LuaTableOptions,
    target_tags: &TargetTags,
) -> Result<Vec<LuaFieldBinding<'meta>>> {
    let mut bindings = Vec::new();
    for field in table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
    {
        bindings.push(LuaFieldBinding {
            source_name: &field.meta.name,
            lua_name: sanitize_identifier(&field.meta.name),
            value_type: LuaValueType::Field(&field.linked_field),
        });
    }
    if table_meta.meta.kind == TableKind::Collection {
        for fragment in table_meta
            .fragments
            .iter()
            .filter(|fragment| fragment.meta.is_exported_for(target_tags))
        {
            bindings.push(LuaFieldBinding {
                source_name: &fragment.meta.name,
                lua_name: sanitize_identifier(&fragment.meta.name),
                value_type: match &fragment.payload {
                    LinkedFragmentPayload::InlineFields => {
                        LuaValueType::InlineFragment(&fragment.fields)
                    }
                    LinkedFragmentPayload::Struct(struct_type) => {
                        LuaValueType::StructFragment(struct_type)
                    }
                },
            });
        }
    }
    Ok(bindings)
}

fn render_optional_lua_value(
    value: Option<&ExportedValue>,
    value_type: &LuaValueType<'_>,
) -> Result<String> {
    value
        .map(|value| render_lua_value(value, value_type))
        .unwrap_or_else(|| Ok("nil".to_owned()))
}

fn render_lua_value(value: &ExportedValue, value_type: &LuaValueType<'_>) -> Result<String> {
    match value_type {
        LuaValueType::Field(field) => render_lua_value_for_field(value, field),
        LuaValueType::InlineFragment(fields) => render_lua_struct(value, fields),
        LuaValueType::StructFragment(struct_type) => render_lua_struct(value, &struct_type.fields),
    }
}

fn render_lua_value_for_field(value: &ExportedValue, field: &LinkedField<'_>) -> Result<String> {
    match (value, field) {
        (ExportedValue::Scalar(value), LinkedField::Primitive(primitive)) => {
            render_scalar(value, *primitive)
        }
        (ExportedValue::Scalar(value), LinkedField::Enum(_)) => Ok(lua_string(value)),
        (ExportedValue::Struct(_), LinkedField::Struct(struct_type)) => {
            render_lua_struct(value, &struct_type.fields)
        }
        (ExportedValue::Array(values), LinkedField::List(element)) => values
            .iter()
            .map(|value| render_lua_value_for_field(value, element))
            .collect::<Result<Vec<_>>>()
            .map(|values| format!("{{ {} }}", values.join(", "))),
        _ => Err(ConfigForgeError::Validation(
            "lua_table value does not match linked field type".to_owned(),
        )),
    }
}

fn render_lua_struct(value: &ExportedValue, fields: &[LinkedFieldMeta<'_>]) -> Result<String> {
    let ExportedValue::Struct(values) = value else {
        return Err(ConfigForgeError::Validation(
            "lua_table expected struct value".to_owned(),
        ));
    };
    let mut entries = Vec::new();
    for field in fields {
        let lua_name = sanitize_identifier(&field.meta.name);
        if let Some(value) = values.get(&field.meta.name) {
            entries.push(format!(
                "{} = {}",
                lua_name,
                render_lua_value_for_field(value, &field.linked_field)?
            ));
        }
    }
    Ok(format!("{{ {} }}", entries.join(", ")))
}

fn render_scalar(value: &str, primitive: PrimitiveFieldType) -> Result<String> {
    match primitive {
        PrimitiveFieldType::Int => value.parse::<i64>().map(|_| value.to_owned()).map_err(|_| {
            ConfigForgeError::Validation(format!("lua_table invalid integer `{value}`"))
        }),
        PrimitiveFieldType::Float => {
            let parsed = value.parse::<f64>().map_err(|_| {
                ConfigForgeError::Validation(format!("lua_table invalid float `{value}`"))
            })?;
            if !parsed.is_finite() {
                return Err(ConfigForgeError::Validation(format!(
                    "lua_table non-finite float `{value}` cannot be emitted"
                )));
            }
            Ok(value.to_owned())
        }
        PrimitiveFieldType::Bool => match value {
            "true" | "1" => Ok("true".to_owned()),
            "false" | "0" => Ok("false".to_owned()),
            _ => Err(ConfigForgeError::Validation(format!(
                "lua_table invalid bool `{value}`"
            ))),
        },
        PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset => {
            Ok(lua_string(value))
        }
    }
}

fn lua_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\{}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn snake_case(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = true;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if character.is_ascii_uppercase() && !previous_was_separator && !output.ends_with('_') {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            output.push('_');
            previous_was_separator = true;
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        "_".to_owned()
    } else {
        output
    }
}

fn sanitize_identifier(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_alphanumeric() || character == '_' {
            if index == 0 && character.is_ascii_digit() {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push('_');
        }
    }
    if output.is_empty() {
        output.push('_');
    }
    if lua_keywords().contains(&output.as_str()) {
        output.push('_');
    }
    output
}

fn table_accessor_name(table_meta: &LinkedTableMeta<'_>) -> String {
    sanitize_identifier(&snake_case(&table_meta.meta.name))
}

fn text_helper_name(field_name: &str) -> String {
    sanitize_identifier(&format!("{field_name}_text"))
}

fn lua_keywords() -> &'static [&'static str] {
    &[
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if",
        "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
    ]
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::PathBuf};

    use tempfile::tempdir;

    use crate::{
        artifact::{ArtifactContent, ArtifactWriteMode, ArtifactWriter},
        data::{DataSet, DataTable},
        exporter::{AggregateDataExporter, TargetDataConfig},
        meta::{MetaModel, TargetTags},
    };

    use super::*;

    fn lua_task() -> TargetDataConfig {
        TargetDataConfig {
            name: Some("lua".to_owned()),
            output: PathBuf::from("out"),
            format: FORMAT.to_owned(),
            options: BTreeMap::from([(
                "module_name".to_owned(),
                toml::Value::String("game.config".to_owned()),
            )]),
        }
    }

    fn lua_task_with_options(options: BTreeMap<String, toml::Value>) -> TargetDataConfig {
        TargetDataConfig {
            options,
            ..lua_task()
        }
    }

    #[test]
    fn escapes_lua_strings_and_rejects_non_finite_float() {
        assert_eq!(lua_string("a\n\"b\\c"), r#""a\n\"b\\c""#);
        assert!(render_scalar("NaN", PrimitiveFieldType::Float).is_err());
    }

    #[test]
    fn plans_root_and_table_module_artifacts() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let paths = LuaTableExporter
            .plan_artifacts(
                PathBuf::from("out").as_path(),
                &lua_task(),
                &linked,
                &TargetTags::empty(),
            )
            .expect("paths should plan");
        assert_eq!(
            paths,
            vec![
                PathBuf::from("out/game/config/init.lua"),
                PathBuf::from("out/game/config/items.lua")
            ]
        );
    }

    #[test]
    fn exports_collection_rows_with_metatable_and_indexes() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Stack"

                [[structs.fields]]
                name = "max_stack"
                type = "int"

                [[tables]]
                name = "items"
                source = "items.csv"
                primary_index = "by_id"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "name"
                type = "text"
                required = true

                [[tables.fields]]
                name = "stack"
                type = "Stack"

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true
                primary = true
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([
                ("id".to_owned(), "2001".to_owned()),
                ("name".to_owned(), "Iron Ore".to_owned()),
                ("stack".to_owned(), r#"{"max_stack":999}"#.to_owned()),
            ])],
        )]);
        let artifacts = LuaTableExporter
            .export(
                PathBuf::from("out").as_path(),
                &lua_task(),
                &linked,
                &TargetTags::empty(),
                &data_set,
            )
            .expect("lua should export");
        let items = artifacts
            .iter()
            .find(|artifact| artifact.path.ends_with("items.lua"))
            .expect("items artifact");
        let ArtifactContent::Text(content) = &items.content else {
            panic!("text artifact")
        };
        assert!(content.contains("local FIELD ="));
        assert!(content.contains("name = 2"));
        assert!(content.contains(
            "setmetatable({ [1] = 2001, [2] = \"Iron Ore\", [3] = { max_stack = 999 }, }, ROW_MT)"
        ));
        assert!(content.contains("function T.get_by_id(id)"));
        assert!(content.contains("return by_id[id]"));
    }

    #[test]
    fn export_to_sink_writes_root_and_table_files() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "name"
                type = "text"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([
                ("id".to_owned(), "2001".to_owned()),
                ("name".to_owned(), "Iron Ore".to_owned()),
            ])],
        )]);
        let temp_dir = tempdir().expect("temp dir");
        let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();

        LuaTableExporter
            .export_to_sink(
                temp_dir.path(),
                &lua_task(),
                &linked,
                &TargetTags::empty(),
                &data_set,
                &sink,
            )
            .expect("Lua should export to sink");
        sink.finish().expect("sink should finish");

        let root = fs::read_to_string(temp_dir.path().join("game/config/init.lua"))
            .expect("root Lua file should be written");
        let items = fs::read_to_string(temp_dir.path().join("game/config/items.lua"))
            .expect("table Lua file should be written");
        assert!(root.contains("require(\"game.config.items\")"));
        assert!(items.contains("[2] = \"Iron Ore\""));
    }

    #[test]
    fn emits_standard_file_header_and_supports_custom_header() {
        assert!(render_file_header(&LuaTableOptions::default()).starts_with(
            "-- Copyright (c) 2026 ConfigForge contributors\n-- SPDX-License-Identifier: Apache-2.0\n"
        ));

        let options = LuaTableOptions {
            file_header: Some("-- custom header".to_owned()),
            ..LuaTableOptions::default()
        };
        assert_eq!(render_file_header(&options), "-- custom header\n");
        assert_eq!(
            options.code_artifact_options().file_header.as_deref(),
            Some("-- custom header")
        );
    }

    #[test]
    fn exports_text_helpers_with_global_i18n() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "name"
                type = "text"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([
                ("id".to_owned(), "2001".to_owned()),
                ("name".to_owned(), "Iron Ore".to_owned()),
            ])],
        )]);
        let task = lua_task_with_options(BTreeMap::from([
            (
                "module_name".to_owned(),
                toml::Value::String("game.config".to_owned()),
            ),
            (
                "i18n".to_owned(),
                toml::Value::Table(toml::Table::from_iter([
                    (
                        "mode".to_owned(),
                        toml::Value::String("global_function".to_owned()),
                    ),
                    (
                        "function".to_owned(),
                        toml::Value::String("globalTranslate".to_owned()),
                    ),
                    (
                        "returns".to_owned(),
                        toml::Value::String("optional".to_owned()),
                    ),
                ])),
            ),
        ]));

        let artifacts = LuaTableExporter
            .export(
                PathBuf::from("out").as_path(),
                &task,
                &linked,
                &TargetTags::empty(),
                &data_set,
            )
            .expect("lua should export");
        let items = artifacts
            .iter()
            .find(|artifact| artifact.path.ends_with("items.lua"))
            .expect("items artifact");
        let ArtifactContent::Text(content) = &items.content else {
            panic!("text artifact")
        };

        assert!(content.contains("function ROW_METHODS.name_text(row)"));
        assert!(content.contains("globalTranslate(\"items.name\", value)"));
        assert!(content.contains("if translated ~= nil then return translated end"));
        let options = options_from_task(&task).expect("lua options should parse");
        assert_eq!(
            options.code_artifact_options().i18n.mode,
            I18nMode::GlobalFunction
        );
    }

    #[test]
    fn exports_foreign_key_link_helpers() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                primary_index = "by_id"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true
                primary = true

                [[tables]]
                name = "game_rules"
                kind = "object"
                source = "game_rules.csv"

                [[tables.fields]]
                name = "default_item_id"
                type = "int"
                foreign_key = { name = "default_item", target_table = "items", target_index = "by_id", linked_field = "default_item" }

                [[tables.fields]]
                name = "starter_item_ids"
                type = "int[]"
                foreign_key = { name = "starter_item", target_table = "items", target_index = "by_id", linked_field = "starter_items" }
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![
            DataTable::new(
                "items",
                vec![BTreeMap::from([("id".to_owned(), "2001".to_owned())])],
            ),
            DataTable::new(
                "game_rules",
                vec![BTreeMap::from([
                    ("default_item_id".to_owned(), "2001".to_owned()),
                    ("starter_item_ids".to_owned(), "[2001]".to_owned()),
                ])],
            ),
        ]);

        let artifacts = LuaTableExporter
            .export(
                PathBuf::from("out").as_path(),
                &lua_task(),
                &linked,
                &TargetTags::empty(),
                &data_set,
            )
            .expect("lua should export");
        let rules = artifacts
            .iter()
            .find(|artifact| artifact.path.ends_with("game_rules.lua"))
            .expect("game rules artifact");
        let ArtifactContent::Text(content) = &rules.content else {
            panic!("text artifact")
        };

        assert!(content.contains("function T.default_item(data)"));
        assert!(content.contains("local target = data.items()"));
        assert!(content.contains("local value = target.get_by_id(T.default_item_id)"));
        assert!(content.contains("function T.starter_items(data)"));
        assert!(content.contains("for _, key in ipairs(keys) do"));
    }

    #[test]
    fn exports_object_table_as_keyed_table() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "game_rules"
                kind = "object"
                source = "game_rules.csv"

                [[tables.fields]]
                name = "max_player_level"
                type = "int"

                [[tables.fields]]
                name = "pvp_enabled"
                type = "bool"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![DataTable::new(
            "game_rules",
            vec![BTreeMap::from([
                ("max_player_level".to_owned(), "60".to_owned()),
                ("pvp_enabled".to_owned(), "true".to_owned()),
            ])],
        )]);
        let artifacts = LuaTableExporter
            .export(
                PathBuf::from("out").as_path(),
                &lua_task(),
                &linked,
                &TargetTags::empty(),
                &data_set,
            )
            .expect("lua should export");
        let rules = artifacts
            .iter()
            .find(|artifact| artifact.path.ends_with("game_rules.lua"))
            .expect("game rules artifact");
        let ArtifactContent::Text(content) = &rules.content else {
            panic!("text artifact")
        };
        assert!(content.contains("max_player_level = 60"));
        assert!(content.contains("pvp_enabled = true"));
    }

    #[test]
    fn rejects_duplicate_generated_table_files() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "item-types"
                source = "a.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "item_types"
                source = "b.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");

        let error = LuaTableExporter
            .plan_artifacts(
                PathBuf::from("out").as_path(),
                &lua_task(),
                &linked,
                &TargetTags::empty(),
            )
            .unwrap_err();

        assert!(
            matches!(error, crate::ConfigForgeError::Validation(message) if message.contains("table file `item_types.lua`"))
        );
    }

    #[test]
    fn exports_non_unique_and_composite_indexes() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "class_levels"
                source = "class_levels.csv"

                [[tables.fields]]
                name = "class_id"
                type = "int"

                [[tables.fields]]
                name = "level"
                type = "int"

                [[tables.fields]]
                name = "label"
                type = "string"

                [[tables.indexes]]
                name = "by_class_level"
                fields = ["class_id", "level"]
                unique = true

                [[tables.indexes]]
                name = "by_class"
                fields = ["class_id"]
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![DataTable::new(
            "class_levels",
            vec![
                BTreeMap::from([
                    ("class_id".to_owned(), "1".to_owned()),
                    ("level".to_owned(), "1".to_owned()),
                    ("label".to_owned(), "Warrior 1".to_owned()),
                ]),
                BTreeMap::from([
                    ("class_id".to_owned(), "1".to_owned()),
                    ("level".to_owned(), "2".to_owned()),
                    ("label".to_owned(), "Warrior 2".to_owned()),
                ]),
            ],
        )]);

        let artifacts = LuaTableExporter
            .export(
                PathBuf::from("out").as_path(),
                &lua_task(),
                &linked,
                &TargetTags::empty(),
                &data_set,
            )
            .expect("lua should export");
        let class_levels = artifacts
            .iter()
            .find(|artifact| artifact.path.ends_with("class_levels.lua"))
            .expect("class levels artifact");
        let ArtifactContent::Text(content) = &class_levels.content else {
            panic!("text artifact")
        };
        assert!(content.contains("function T.get_by_class_level(class_id, level)"));
        assert!(content.contains("return level_1[level]"));
        assert!(content.contains("function T.get_by_class(class_id)"));
        assert!(content.contains("bucket[#bucket + 1] = row"));
    }
}
