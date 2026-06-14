// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Result,
    code_artifact::{I18nFunctionReturn, I18nMode},
    data_artifact::{ExportedTablePayload, ExportedValue},
    diagnostic::code::{
        CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH, CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    },
    meta::{
        LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta, LinkedFragmentPayload, LinkedMetaModel,
        LinkedStructType, LinkedTableMeta, PrimitiveFieldType, TableKind, TargetTags,
    },
};

use super::{
    lua_input_error, lua_symbol_error,
    naming::{sanitize_identifier, table_accessor_name, table_require_path, text_helper_name},
    options::LuaTableOptions,
};

#[derive(Debug, Clone)]
pub(super) struct LuaFieldBinding<'meta> {
    pub(super) source_name: &'meta str,
    pub(super) lua_name: String,
    pub(super) value_type: LuaValueType<'meta>,
}

#[derive(Debug, Clone)]
pub(super) enum LuaValueType<'meta> {
    Field(&'meta LinkedField<'meta>),
    InlineFragment(&'meta [LinkedFieldMeta<'meta>]),
    StructFragment(&'meta LinkedStructType<'meta>),
}

pub(super) fn row_helper_names(
    table: &LinkedTableMeta<'_>,
    target_tags: &TargetTags,
) -> Result<Vec<String>> {
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

pub(super) fn render_root(options: &LuaTableOptions, tables: &[&LinkedTableMeta<'_>]) -> String {
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

pub(super) fn render_table_file(
    table_meta: &LinkedTableMeta<'_>,
    payload: &ExportedTablePayload,
    options: &LuaTableOptions,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
) -> Result<String> {
    match (table_meta.meta.kind, payload) {
        (TableKind::Collection, ExportedTablePayload::Collection(table)) => {
            render_collection_table(table_meta, table.rows(), options, linked_meta, target_tags)
        }
        (TableKind::Object, ExportedTablePayload::Object(object)) => {
            render_object_table(table_meta, object, options, linked_meta, target_tags)
        }
        _ => Err(lua_contract_error(
            format!(
                "lua_table payload kind does not match table `{}`",
                table_meta.meta.name
            ),
            table_meta.meta.name.clone(),
            "payload kind matching linked table kind",
            "Check runtime data preparation before Lua export.",
        )),
    }
}

fn render_collection_table(
    table_meta: &LinkedTableMeta<'_>,
    rows: &[BTreeMap<String, ExportedValue>],
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
    object: &BTreeMap<String, ExportedValue>,
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

pub(super) fn render_file_header(options: &LuaTableOptions) -> String {
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
                lua_input_error(
                    format!(
                        "lua_table foreign key `{}` references unknown target table `{}`",
                        foreign_key.name, foreign_key.target_table_name
                    ),
                    foreign_key.target_table_name,
                    "target table present in linked meta model",
                    "Check foreign key target table visibility before Lua export.",
                )
            })?;
        let target_index = target_table
            .index(foreign_key.target_index_name)
            .ok_or_else(|| {
                lua_input_error(
                    format!(
                        "lua_table foreign key `{}` references unknown target index `{}.{}`",
                        foreign_key.name,
                        foreign_key.target_table_name,
                        foreign_key.target_index_name
                    ),
                    format!(
                        "{}.{}",
                        foreign_key.target_table_name, foreign_key.target_index_name
                    ),
                    "target index present in linked meta model",
                    "Check foreign key target index visibility before Lua export.",
                )
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
                        lua_input_error(
                            format!(
                                "lua_table foreign key `{}` references field `{}` that is not exported",
                                foreign_key.name, field.meta.name
                            ),
                            format!("{}.{}", table_meta.meta.name, field.meta.name),
                            "foreign key source field exported for this target",
                            "Check target tags or foreign key field visibility before Lua export.",
                        )
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
        lua_input_error(
            format!(
                "lua_table table `{}` references unknown field `{field_name}`",
                table_meta.meta.name
            ),
            format!("{}.{}", table_meta.meta.name, field_name),
            "field present in linked table meta",
            "Check generated Lua helper field references before export.",
        )
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
                        lua_input_error(
                            format!(
                                "lua_table index `{}` references field `{}` that is not exported",
                                index.meta.name, field.meta.name
                            ),
                            format!("{}.{}", table_meta.meta.name, field.meta.name),
                            "index field exported for this target",
                            "Check target tags or index field visibility before Lua export.",
                        )
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

pub(super) fn field_bindings<'meta>(
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
        _ => Err(lua_contract_error(
            "lua_table value does not match linked field type".to_owned(),
            exported_value_kind(value),
            linked_field_kind(field),
            "Check exported payload construction before Lua rendering.",
        )),
    }
}

fn render_lua_struct(value: &ExportedValue, fields: &[LinkedFieldMeta<'_>]) -> Result<String> {
    let ExportedValue::Struct(values) = value else {
        return Err(lua_contract_error(
            "lua_table expected struct value".to_owned(),
            exported_value_kind(value),
            "struct exported value",
            "Check struct/fragment payload construction before Lua rendering.",
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

pub(super) fn render_scalar(value: &str, primitive: PrimitiveFieldType) -> Result<String> {
    match primitive {
        PrimitiveFieldType::Int => value.parse::<i64>().map(|_| value.to_owned()).map_err(|_| {
            lua_scalar_error(
                format!("lua_table invalid integer `{value}`"),
                value,
                "integer scalar",
            )
        }),
        PrimitiveFieldType::Float => {
            let parsed = value.parse::<f64>().map_err(|_| {
                lua_scalar_error(
                    format!("lua_table invalid float `{value}`"),
                    value,
                    "finite float scalar",
                )
            })?;
            if !parsed.is_finite() {
                return Err(lua_scalar_error(
                    format!("lua_table non-finite float `{value}` cannot be emitted"),
                    value,
                    "finite float scalar",
                ));
            }
            Ok(value.to_owned())
        }
        PrimitiveFieldType::Bool => match value {
            "true" | "1" => Ok("true".to_owned()),
            "false" | "0" => Ok("false".to_owned()),
            _ => Err(lua_scalar_error(
                format!("lua_table invalid bool `{value}`"),
                value,
                "boolean scalar (`true`, `false`, `1`, or `0`)",
            )),
        },
        PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset => {
            Ok(lua_string(value))
        }
        PrimitiveFieldType::Color
        | PrimitiveFieldType::Vector2
        | PrimitiveFieldType::Vector3
        | PrimitiveFieldType::Vector4
        | PrimitiveFieldType::Vector2Int
        | PrimitiveFieldType::Vector3Int
        | PrimitiveFieldType::Vector4Int => Ok(lua_string(value)),
    }
}

fn lua_scalar_error(
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    lua_symbol_error(
        CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
        message,
        current,
        expected,
        "Check typed data normalization before Lua export.",
    )
}

fn lua_contract_error(
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    lua_symbol_error(
        CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH,
        message,
        current,
        expected,
        hint,
    )
}

fn exported_value_kind(value: &ExportedValue) -> &'static str {
    match value {
        ExportedValue::Scalar(_) => "scalar exported value",
        ExportedValue::Array(_) => "array exported value",
        ExportedValue::Struct(_) => "struct exported value",
    }
}

fn linked_field_kind(field: &LinkedField<'_>) -> &'static str {
    match field {
        LinkedField::Primitive(_) | LinkedField::Enum(_) => "scalar-compatible linked field",
        LinkedField::Struct(_) => "struct linked field",
        LinkedField::List(_) => "array linked field",
    }
}

pub(super) fn lua_string(value: &str) -> String {
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
