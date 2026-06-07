// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    codegen::{
        CodeIndex, CodeModel, CodeTable, CodeTableKind, CodeWriterLike,
        ir::{CodeForeignKey, CodeRelationCardinality, relation_key_plan},
    },
};

use super::types::RustNames;

pub(super) fn render_table(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &RustNames<'_>,
) -> Result<()> {
    if table.kind == CodeTableKind::Object {
        return render_object_table(content, table, names);
    }

    content.raw_fmt(format_args!(
        "pub struct {} {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    rows: Vec<{}>,\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        content.raw_fmt(format_args!(
            "    {}: {},\n",
            names.index_storage_name(index)?,
            names.rust_index_storage(index)?
        ));
    }
    content.raw("}\n\n");

    content.raw_fmt(format_args!(
        "impl {} {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    pub const DATA_FILE: &'static str = {:?};\n\n",
        table.data_file
    ));
    if let Some(revision) = table.schema_revision {
        content.raw_fmt(format_args!(
            "    pub const EXPECTED_SCHEMA_REVISION: u64 = {revision};\n\n"
        ));
    }
    content.raw("    pub fn load(loader: &impl RuntimeDataLoader) -> RuntimeDataResult<Self> {\n");
    content.raw("        let root = loader.open(Self::DATA_FILE)?;\n");
    if table.schema_revision.is_some() {
        content.raw(concat!(
            "        if let Some(schema_revision) = root.schema_revision() {\n",
            "            if schema_revision != Self::EXPECTED_SCHEMA_REVISION {\n",
            "                return Err(RuntimeDataError::schema_mismatch(format!(\n",
            "                    \"schema revision mismatch for `{}`: expected {}, got {:?}\",\n",
            "                    Self::DATA_FILE,\n",
            "                    Self::EXPECTED_SCHEMA_REVISION,\n",
            "                    schema_revision\n",
            "                )));\n",
            "            }\n",
            "        }\n",
        ));
    }
    content.raw("        root.enter_required_field(\"rows\")?;\n");
    content.raw("        let row_count = root.read_row_count()?;\n");
    content.raw("        let mut rows = Vec::new();\n");
    content.raw("        for row_index in 0..row_count {\n");
    content.raw("            root.enter_index(row_index)?;\n");
    content.raw_fmt(format_args!(
        "            rows.push({}::read_from_reader(&root)?);\n",
        names.row_type_name(&table.name)?
    ));
    content.raw("            root.exit_field()?;\n");
    content.raw("        }\n");
    content.raw("        root.exit_field()?;\n");
    content.raw("        root.ensure_exhausted()?;\n");
    content.raw("        Self::from_rows(rows)\n");
    content.raw("    }\n\n");
    content.raw_fmt(format_args!(
        "    pub fn from_rows(rows: Vec<{}>) -> RuntimeDataResult<Self> {{\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        content.raw_fmt(format_args!(
            "        let mut {} = HashMap::new();\n",
            names.index_storage_name(index)?
        ));
    }
    if !table.indexes.is_empty() {
        content.raw("        for (row_index, row) in rows.iter().enumerate() {\n");
        for index in &table.indexes {
            let key = names.rust_key_expr(index)?;
            if index.unique {
                content.raw_fmt(format_args!(
                    "            if {}.insert({}, row_index).is_some() {{ return Err(RuntimeDataError::schema_mismatch(format!(\"duplicate unique index {}\"))); }}\n",
                    names.index_storage_name(index)?,
                    key,
                    index.name
                ));
            } else {
                content.raw_fmt(format_args!(
                    "            {}.entry({}).or_insert_with(Vec::new).push(row_index);\n",
                    names.index_storage_name(index)?,
                    key
                ));
            }
        }
        content.raw("        }\n");
    }
    content.raw("        Ok(Self {\n");
    content.raw("            rows,\n");
    for index in &table.indexes {
        content.raw_fmt(format_args!(
            "            {},\n",
            names.index_storage_name(index)?
        ));
    }
    content.raw("        })\n");
    content.raw("    }\n\n");
    if table.preserve_source_order {
        content.raw("    pub fn len(&self) -> usize { self.rows.len() }\n\n");
        content.raw("    pub fn is_empty(&self) -> bool { self.rows.is_empty() }\n\n");
        content.raw_fmt(format_args!(
            "    pub fn rows(&self) -> &[{}] {{ &self.rows }}\n\n",
            names.row_type_name(&table.name)?
        ));
        content.raw_fmt(format_args!(
            "    pub fn row_at(&self, row_index: usize) -> Option<&{}> {{ self.rows.get(row_index) }}\n\n",
            names.row_type_name(&table.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "    fn row_by_handle(&self, row_handle: usize) -> Option<&{}> {{ self.rows.get(row_handle) }}\n\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
        if index.unique {
            render_row_handle_function(content, index, names)?;
        }
    }
    render_table_link_function(content, table, names)?;
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_row_relation_methods(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    table: &CodeTable,
    names: &RustNames<'_>,
) -> Result<()> {
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let target_table = table_by_name(model, &foreign_key.target_table)?;
        let target_index = index_by_name(target_table, &foreign_key.target_index)?;
        let target_row_type = names.row_type_name(&target_table.name)?;
        let function_name = names.relation_function_name(foreign_key)?;
        match foreign_key
            .linked_field
            .as_ref()
            .expect("linked field")
            .cardinality
        {
            CodeRelationCardinality::One => render_single_relation_method(
                content,
                table,
                foreign_key,
                target_table,
                target_index,
                names,
                &target_row_type,
                &function_name,
                false,
            )?,
            CodeRelationCardinality::OptionalOne => render_single_relation_method(
                content,
                table,
                foreign_key,
                target_table,
                target_index,
                names,
                &target_row_type,
                &function_name,
                true,
            )?,
            CodeRelationCardinality::Many => render_many_relation_method(
                content,
                table,
                foreign_key,
                target_table,
                target_index,
                names,
                &target_row_type,
                &function_name,
            )?,
        }
    }
    Ok(())
}

pub(super) fn render_game_data_link_methods(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &RustNames<'_>,
) -> Result<()> {
    content.raw("\n    pub fn link_all(&self) -> RuntimeDataResult<()> {\n");
    for table in &model.tables {
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            content.raw_fmt(format_args!(
                "        self.{}.link(self)?;\n",
                names.field_name(&table.name)?
            ));
        }
    }
    content.raw("        Ok(())\n");
    content.raw("    }\n");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_single_relation_method(
    content: &mut impl CodeWriterLike,
    source_table: &CodeTable,
    foreign_key: &CodeForeignKey,
    target_table: &CodeTable,
    target_index: &CodeIndex,
    names: &RustNames<'_>,
    target_row_type: &str,
    function_name: &str,
    optional: bool,
) -> Result<()> {
    let return_type = if optional {
        format!("Option<&'a {target_row_type}>")
    } else {
        format!("&'a {target_row_type}")
    };
    let cache_name = names.relation_cache_field_name(foreign_key)?;
    let target_data_field = names.field_name(&target_table.name)?;
    let row_handle_name = row_handle_function_name(target_index, names)?;
    content.raw_fmt(format_args!(
        "    pub fn {function_name}<'a>(&self, data: &'a impl GameDataAccess) -> RuntimeDataResult<{return_type}> {{\n"
    ));
    content.raw_fmt(format_args!(
        "        let cached = self.{}.get();\n",
        cache_name
    ));
    content.raw("        match cached {\n");
    if optional {
        content.raw("            LinkCache::Empty => return Ok(None),\n");
    } else {
        content.raw_fmt(format_args!(
            "            LinkCache::Empty => return Err(RuntimeDataError::decode({:?}.to_owned())),\n",
            format!("required relation `{}` unexpectedly cached empty", foreign_key.name)
        ));
    }
    if optional {
        content.raw_fmt(format_args!(
            "            LinkCache::RowIndex(row_index) => return data.{target_data_field}()?.row_by_handle(row_index).map(Some).ok_or_else(|| RuntimeDataError::decode({:?}.to_owned())),\n",
            format!("cached relation `{}` row index out of bounds", foreign_key.name)
        ));
    } else {
        content.raw_fmt(format_args!(
            "            LinkCache::RowIndex(row_index) => return data.{target_data_field}()?.row_by_handle(row_index).ok_or_else(|| RuntimeDataError::decode({:?}.to_owned())),\n",
            format!("cached relation `{}` row index out of bounds", foreign_key.name)
        ));
    }
    content.raw("            LinkCache::Unresolved => {}\n");
    content.raw("        }\n");
    if optional {
        render_optional_empty_check(content, foreign_key, names, "        ")?;
    }
    let raw_value_exprs = foreign_key
        .fields
        .iter()
        .map(|field| Ok(format!("self.{}.clone()", names.field_name(&field.name)?)))
        .collect::<Result<Vec<_>>>()?;
    let key_plan = relation_key_plan(foreign_key);
    if optional && key_plan.should_report_partial_empty {
        let partial_conditions = key_plan
            .optional_key_fields
            .iter()
            .map(|field| Ok(format!("self.{}.is_none()", names.field_name(&field.name)?)))
            .collect::<Result<Vec<_>>>()?;
        let any_empty = partial_conditions.join(" || ");
        let all_empty = if key_plan.all_key_fields_optional {
            partial_conditions.join(" && ")
        } else {
            "false".to_owned()
        };
        content.raw_fmt(format_args!(
            "        if ({any_empty}) && !({all_empty}) {{ return Err(RuntimeDataError::decode({:?}.to_owned())); }}\n",
            format!("foreign key relation `{}` has partial empty source key", foreign_key.name)
        ));
    }
    let value_exprs = foreign_key
        .fields
        .iter()
        .zip(raw_value_exprs)
        .map(|(field, expr)| {
            if field.required {
                Ok(expr)
            } else {
                Ok(format!(
                    "{expr}.ok_or_else(|| RuntimeDataError::decode({:?}.to_owned()))?",
                    format!(
                        "foreign key relation `{}` has partial empty source key",
                        foreign_key.name
                    )
                ))
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let lookup_key = names.rust_relation_lookup_key(foreign_key, target_index, &value_exprs)?;
    content.raw_fmt(format_args!(
        "        let row_index = data.{target_data_field}()?.{row_handle_name}({lookup_key}).ok_or_else(|| RuntimeDataError::decode({:?}.to_owned()))?;\n",
        format!(
            "foreign key relation `{}.{}` could not resolve target `{}.{}`",
            source_table.name, foreign_key.name, target_table.name, target_index.name
        )
    ));
    content.raw_fmt(format_args!(
        "        self.{cache_name}.set(LinkCache::RowIndex(row_index));\n"
    ));
    if optional {
        content.raw_fmt(format_args!(
            "        data.{target_data_field}()?.row_by_handle(row_index).map(Some).ok_or_else(|| RuntimeDataError::decode({:?}.to_owned()))\n",
            format!("resolved relation `{}` row index out of bounds", foreign_key.name)
        ));
    } else {
        content.raw_fmt(format_args!(
            "        data.{target_data_field}()?.row_by_handle(row_index).ok_or_else(|| RuntimeDataError::decode({:?}.to_owned()))\n",
            format!("resolved relation `{}` row index out of bounds", foreign_key.name)
        ));
    }
    content.raw("    }\n");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_many_relation_method(
    content: &mut impl CodeWriterLike,
    source_table: &CodeTable,
    foreign_key: &CodeForeignKey,
    target_table: &CodeTable,
    target_index: &CodeIndex,
    names: &RustNames<'_>,
    target_row_type: &str,
    function_name: &str,
) -> Result<()> {
    let cache_name = names.relation_cache_field_name(foreign_key)?;
    let source_field = foreign_key.fields.first().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "foreign key `{}` has no source fields",
            foreign_key.name
        ))
    })?;
    let source_field_name = names.field_name(&source_field.name)?;
    let target_data_field = names.field_name(&target_table.name)?;
    let row_handle_name = row_handle_function_name(target_index, names)?;
    content.raw_fmt(format_args!(
        "    pub fn {function_name}<'a>(&self, data: &'a impl GameDataAccess) -> RuntimeDataResult<Vec<&'a {target_row_type}>> {{\n"
    ));
    content.raw_fmt(format_args!(
        "        if self.{cache_name}.borrow().row_indexes.is_none() {{\n"
    ));
    content.raw_fmt(format_args!(
        "            let mut row_indexes = Vec::with_capacity(self.{source_field_name}.len());\n"
    ));
    content.raw_fmt(format_args!(
        "            for key in &self.{source_field_name} {{\n"
    ));
    content.raw_fmt(format_args!(
        "                let row_index = data.{target_data_field}()?.{row_handle_name}(key.clone()).ok_or_else(|| RuntimeDataError::decode({:?}.to_owned()))?;\n",
        format!(
            "foreign key relation `{}.{}` could not resolve list target `{}.{}`",
            source_table.name, foreign_key.name, target_table.name, target_index.name
        )
    ));
    content.raw("                row_indexes.push(row_index);\n");
    content.raw("            }\n");
    content.raw_fmt(format_args!(
        "            self.{cache_name}.borrow_mut().row_indexes = Some(row_indexes);\n"
    ));
    content.raw("        }\n");
    content.raw_fmt(format_args!(
        "        let cache = self.{cache_name}.borrow();\n"
    ));
    content.raw("        let row_indexes = cache.row_indexes.as_deref().unwrap_or(&[]);\n");
    content.raw("        let mut rows = Vec::with_capacity(row_indexes.len());\n");
    content.raw("        for &row_index in row_indexes {\n");
    content.raw_fmt(format_args!(
        "            rows.push(data.{target_data_field}()?.row_by_handle(row_index).ok_or_else(|| RuntimeDataError::decode({:?}.to_owned()))?);\n",
        format!("resolved list relation `{}` row index out of bounds", foreign_key.name)
    ));
    content.raw("        }\n");
    content.raw("        Ok(rows)\n");
    content.raw("    }\n");
    Ok(())
}

fn render_optional_empty_check(
    content: &mut impl CodeWriterLike,
    foreign_key: &CodeForeignKey,
    names: &RustNames<'_>,
    indent: &str,
) -> Result<()> {
    let key_plan = relation_key_plan(foreign_key);
    if !key_plan.all_key_fields_optional {
        return Ok(());
    }
    let conditions = key_plan
        .optional_key_fields
        .iter()
        .map(|field| Ok(format!("self.{}.is_none()", names.field_name(&field.name)?)))
        .collect::<Result<Vec<_>>>()?;
    if conditions.is_empty() {
        return Ok(());
    }
    content.raw_fmt(format_args!("{indent}if {} {{\n", conditions.join(" && ")));
    content.raw_fmt(format_args!(
        "{indent}    self.{}.set(LinkCache::Empty);\n",
        names.relation_cache_field_name(foreign_key)?
    ));
    content.raw_fmt(format_args!("{indent}    return Ok(None);\n"));
    content.raw_fmt(format_args!("{indent}}}\n"));
    Ok(())
}

fn render_table_link_function(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &RustNames<'_>,
) -> Result<()> {
    if !table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        return Ok(());
    }
    content.raw("    pub fn link(&self, data: &impl GameDataAccess) -> RuntimeDataResult<()> {\n");
    match table.kind {
        CodeTableKind::Collection => {
            content.raw("        for row in &self.rows {\n");
            for foreign_key in table
                .foreign_keys
                .iter()
                .filter(|foreign_key| foreign_key.linked_field.is_some())
            {
                content.raw_fmt(format_args!(
                    "            let _ = row.{}(data)?;\n",
                    names.relation_function_name(foreign_key)?
                ));
            }
            content.raw("        }\n");
        }
        CodeTableKind::Object => {
            for foreign_key in table
                .foreign_keys
                .iter()
                .filter(|foreign_key| foreign_key.linked_field.is_some())
            {
                content.raw_fmt(format_args!(
                    "        let _ = self.{}(data)?;\n",
                    names.relation_function_name(foreign_key)?
                ));
            }
        }
    }
    content.raw("        Ok(())\n");
    content.raw("    }\n\n");
    Ok(())
}

fn row_handle_function_name(index: &CodeIndex, names: &RustNames<'_>) -> Result<String> {
    Ok(names
        .index_of_function_name(index)?
        .replacen("index_of_by_", "row_handle_by_", 1))
}

fn render_row_handle_function(
    content: &mut impl CodeWriterLike,
    index: &CodeIndex,
    names: &RustNames<'_>,
) -> Result<()> {
    let params = index
        .fields
        .iter()
        .map(|field| {
            Ok(format!(
                "{}: {}",
                names.field_name(&field.name)?,
                names.rust_field_type(field)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let key = names.rust_lookup_key(index)?;
    content.raw_fmt(format_args!(
        "    fn {}(&self, {}) -> Option<usize> {{\n",
        row_handle_function_name(index, names)?,
        params
    ));
    content.raw_fmt(format_args!(
        "        self.{}.get(&{}).copied()\n",
        names.index_storage_name(index)?,
        key
    ));
    content.raw("    }\n\n");
    Ok(())
}

fn table_by_name<'a>(model: &'a CodeModel, name: &str) -> Result<&'a CodeTable> {
    model
        .tables
        .iter()
        .find(|table| table.name == name)
        .ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "codegen foreign key references unknown target table `{name}`"
            ))
        })
}

fn index_by_name<'a>(table: &'a CodeTable, name: &str) -> Result<&'a CodeIndex> {
    table
        .indexes
        .iter()
        .find(|index| index.name == name)
        .ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "codegen foreign key references unknown target index `{}.{}`",
                table.name, name
            ))
        })
}

fn render_object_table(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &RustNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "impl {} {{\n",
        names.row_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    pub const DATA_FILE: &'static str = {:?};\n\n",
        table.data_file
    ));
    if let Some(revision) = table.schema_revision {
        content.raw_fmt(format_args!(
            "    pub const EXPECTED_SCHEMA_REVISION: u64 = {revision};\n\n"
        ));
    }
    content.raw("    pub fn load(loader: &impl RuntimeDataLoader) -> RuntimeDataResult<Self> {\n");
    content.raw("        let root = loader.open(Self::DATA_FILE)?;\n");
    if table.schema_revision.is_some() {
        content.raw(concat!(
            "        if let Some(schema_revision) = root.schema_revision() {\n",
            "            if schema_revision != Self::EXPECTED_SCHEMA_REVISION {\n",
            "                return Err(RuntimeDataError::schema_mismatch(format!(\n",
            "                    \"schema revision mismatch for `{}`: expected {}, got {:?}\",\n",
            "                    Self::DATA_FILE,\n",
            "                    Self::EXPECTED_SCHEMA_REVISION,\n",
            "                    schema_revision\n",
            "                )));\n",
            "            }\n",
            "        }\n",
        ));
    }
    content.raw("        let value = Self::read_from_reader(&root)?;\n");
    content.raw("        root.ensure_exhausted()?;\n");
    content.raw("        Ok(value)\n");
    content.raw("    }\n");
    render_table_link_function(content, table, names)?;
    content.raw("}\n\n");
    Ok(())
}

fn render_index_function(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    index: &CodeIndex,
    names: &RustNames<'_>,
) -> Result<()> {
    let params = index
        .fields
        .iter()
        .map(|field| {
            Ok(format!(
                "{}: {}",
                names.field_name(&field.name)?,
                names.rust_field_type(field)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let key = names.rust_lookup_key(index)?;
    if index.unique {
        content.raw_fmt(format_args!(
            "    pub fn {}(&self, {}) -> Option<&{}> {{\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?
        ));
        content.raw_fmt(format_args!(
            "        self.{}.get(&{}).and_then(|index| self.rows.get(*index))\n",
            names.index_storage_name(index)?,
            key
        ));
        content.raw("    }\n\n");
    } else {
        content.raw_fmt(format_args!(
            "    pub fn {}(&self, {}) -> Vec<&{}> {{\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?
        ));
        content.raw_fmt(format_args!(
            concat!(
                "        self.{}\n",
                "            .get(&{})\n",
                "            .map(|indexes| indexes.iter().filter_map(|index| self.rows.get(*index)).collect())\n",
                "            .unwrap_or_default()\n",
            ),
            names.index_storage_name(index)?, key
        ));
        content.raw("    }\n\n");
    }
    Ok(())
}
