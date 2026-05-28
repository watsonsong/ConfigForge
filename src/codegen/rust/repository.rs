// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{codegen::{CodeIndex, CodeTable, CodeTableKind, CodeWriter}, exporter::BINARY_FORMAT, Result};

use super::types::RustNames;

pub(super) fn render_table(content: &mut CodeWriter, table: &CodeTable, names: &RustNames<'_>) -> Result<()> {
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
    if table.data_format == BINARY_FORMAT {
        content.raw(concat!(
            "        if root.schema_revision() != Some(Self::EXPECTED_SCHEMA_REVISION) {\n",
            "            return Err(RuntimeDataError::schema_mismatch(format!(\n",
            "                \"schema revision mismatch for `{}`: expected {}, got {:?}\",\n",
            "                Self::DATA_FILE,\n",
            "                Self::EXPECTED_SCHEMA_REVISION,\n",
            "                root.schema_revision()\n",
            "            )));\n",
            "        }\n",
        ));
        content.raw("        let row_count = root.read_row_count()?;\n");
        content.raw("        let mut rows = Vec::new();\n");
        content.raw("        for _ in 0..row_count {\n");
        content.raw_fmt(format_args!(
            "            rows.push({}::read_from_reader(&root)?);\n",
            names.row_type_name(&table.name)?
        ));
        content.raw("        }\n");
        content.raw("        root.ensure_exhausted()?;\n");
        content.raw("        return Self::from_rows(rows);\n");
    }
    content.raw("        if !root.enter_field(\"rows\")? {\n");
    content.raw(
        "            return Err(RuntimeDataError::not_found(\"missing required field `rows`\"));\n",
    );
    content.raw("        }\n");
    content.raw("        if root.is_null() {\n");
    content.raw("            root.exit()?;\n");
    content.raw(
        "            return Err(RuntimeDataError::not_found(\"missing required field `rows`\"));\n",
    );
    content.raw("        }\n");
    content.raw("        let mut rows = Vec::new();\n");
    content.raw("        for row_index in 0..root.len()? {\n");
    content.raw("            root.enter_index(row_index)?;\n");
    content.raw_fmt(format_args!(
        "            rows.push({}::read_from_reader(&root)?);\n",
        names.row_type_name(&table.name)?
    ));
    content.raw("            root.exit()?;\n");
    content.raw("        }\n");
    content.raw("        root.exit()?;\n");
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
    content.raw_fmt(format_args!(
        "    pub fn rows(&self) -> &[{}] {{ &self.rows }}\n\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
    }
    content.raw("}\n\n");
    Ok(())
}

fn render_object_table(
    content: &mut CodeWriter,
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
    if table.data_format == BINARY_FORMAT {
        content.raw(concat!(
            "        if root.schema_revision() != Some(Self::EXPECTED_SCHEMA_REVISION) {\n",
            "            return Err(RuntimeDataError::schema_mismatch(format!(\n",
            "                \"schema revision mismatch for `{}`: expected {}, got {:?}\",\n",
            "                Self::DATA_FILE,\n",
            "                Self::EXPECTED_SCHEMA_REVISION,\n",
            "                root.schema_revision()\n",
            "            )));\n",
            "        }\n",
        ));
        content.raw("        let value = Self::read_from_reader(&root)?;\n");
        content.raw("        root.ensure_exhausted()?;\n");
        content.raw("        return Ok(value);\n");
    }
    content.raw("        Self::read_from_reader(&root)\n");
    content.raw("    }\n");
    content.raw("}\n\n");
    Ok(())
}

fn render_index_function(
    content: &mut CodeWriter,
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
