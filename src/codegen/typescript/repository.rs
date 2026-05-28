// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{codegen::{CodeIndex, CodeTable, CodeWriter}, exporter::BINARY_FORMAT, Result};

use super::types::TypeScriptNames;

pub(super) fn render_table(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "export class {} {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "  static readonly dataFile = {:?};\n",
        table.data_file
    ));
    if let Some(revision) = table.schema_revision {
        content.raw_fmt(format_args!(
            "  static readonly expectedSchemaRevision = {revision}n;\n"
        ));
    }
    content.raw_fmt(format_args!(
        "  private readonly rows_: {}[];\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        content.raw_fmt(format_args!(
            "  private readonly {}: {};\n",
            names.index_storage_name(index)?,
            names.ts_index_storage(index, table)?
        ));
    }
    content.newline();
    content.raw_fmt(format_args!(
        "  private constructor(rows: {}[]) {{\n",
        names.row_type_name(&table.name)?
    ));
    content.raw("    this.rows_ = rows;\n");
    for index in &table.indexes {
        content.raw_fmt(format_args!(
            "    this.{} = new Map();\n",
            names.index_storage_name(index)?
        ));
    }
    content.raw("    for (const row of rows) {\n");
    for index in &table.indexes {
        let key = names.ts_key_expr(index)?;
        if index.unique {
            content.raw_fmt(format_args!(
                "      const key_{} = {};\n",
                names.index_storage_name(index)?,
                key
            ));
            content.raw_fmt(format_args!(
                "      if (this.{}.has(key_{})) {{ throw new Error(\"Duplicate unique index key for {}.{}\"); }}\n",
                names.index_storage_name(index)?,
                names.index_storage_name(index)?,
                names.repository_type_name(&table.name)?,
                index.name
            ));
            content.raw_fmt(format_args!(
                "      this.{}.set(key_{}, row);\n",
                names.index_storage_name(index)?,
                names.index_storage_name(index)?
            ));
        } else {
            content.raw_fmt(format_args!(
                "      const list_{} = this.{}.get({}) ?? [];\n",
                names.index_storage_name(index)?,
                names.index_storage_name(index)?,
                key
            ));
            content.raw_fmt(format_args!(
                "      list_{}.push(row);\n",
                names.index_storage_name(index)?
            ));
            content.raw_fmt(format_args!(
                "      this.{}.set({}, list_{});\n",
                names.index_storage_name(index)?,
                key,
                names.index_storage_name(index)?
            ));
        }
    }
    content.raw("    }\n");
    content.raw("  }\n\n");
    content.raw_fmt(format_args!(
        "  static async load(loader: RuntimeDataLoader): Promise<{}> {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw("    const root = await loader.open(this.dataFile);\n");
    if table.data_format == BINARY_FORMAT {
        content.raw("    const schemaRevision = root.schemaRevision?.();\n");
        content.raw(concat!(
            "    if (schemaRevision === undefined || BigInt(schemaRevision) !== this.expectedSchemaRevision) {\n",
            "      throw new Error(`schema revision mismatch for ${this.dataFile}`);\n",
            "    }\n",
        ));
        content.raw("    if (!root.readRowCount) { throw new Error('binary reader does not support row count'); }\n");
        content.raw("    if (!root.ensureExhausted) { throw new Error('binary reader does not support EOF validation'); }\n");
        content.raw("    const rowCount = root.readRowCount();\n");
        content.raw_fmt(format_args!(
            "    const rows: {}[] = [];\n",
            names.row_type_name(&table.name)?
        ));
        content.raw("    for (let rowIndex = 0; rowIndex < rowCount; rowIndex++) {\n");
        content.raw_fmt(format_args!(
            "      rows.push({}(root));\n",
            names.reader_function_name(&names.row_type_name(&table.name)?)?
        ));
        content.raw("    }\n");
        content.raw("    root.ensureExhausted();\n");
        content.raw_fmt(format_args!(
            "    return new {}(rows ?? []);\n",
            names.repository_type_name(&table.name)?
        ));
        content.raw("  }\n\n");
        content.raw_fmt(format_args!(
            "  rows(): readonly {}[] {{ return this.rows_; }}\n\n",
            names.row_type_name(&table.name)?
        ));
        for index in &table.indexes {
            render_index_function(content, table, index, names)?;
        }
        content.raw("}\n\n");
        return Ok(());
    }
    content.raw(
        "    if (!root.enterField(\"rows\")) { throw new Error('missing required field rows'); }\n",
    );
    content.raw(
        "    if (root.isNull()) { root.exit(); throw new Error('missing required field rows'); }\n",
    );
    content.raw_fmt(format_args!(
        "    const rows: {}[] = [];\n",
        names.row_type_name(&table.name)?
    ));
    content.raw("    try {\n");
    content.raw("      for (let rowIndex = 0; rowIndex < root.length(); rowIndex++) {\n");
    content.raw("        root.enterIndex(rowIndex);\n");
    content.raw("        try {\n");
    content.raw_fmt(format_args!(
        "          rows.push({}(root));\n",
        names.reader_function_name(&names.row_type_name(&table.name)?)?
    ));
    content.raw("        } finally {\n");
    content.raw("          root.exit();\n");
    content.raw("        }\n");
    content.raw("      }\n");
    content.raw("    } finally {\n");
    content.raw("      root.exit();\n");
    content.raw("    }\n");
    content.raw_fmt(format_args!(
        "    return new {}(rows ?? []);\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw("  }\n\n");
    content.raw_fmt(format_args!(
        "  rows(): readonly {}[] {{ return this.rows_; }}\n\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
    }
    content.raw("}\n\n");
    Ok(())
}

fn render_index_function(
    content: &mut CodeWriter,
    table: &CodeTable,
    index: &CodeIndex,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let params = index
        .fields
        .iter()
        .map(|field| {
            Ok(format!(
                "{}: {}",
                names.field_name(&field.name)?,
                names.ts_field_type(field)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let key = names.ts_lookup_key(index)?;
    if index.unique {
        content.raw_fmt(format_args!(
            "  {}({}): {} | undefined {{ return this.{}.get({}); }}\n\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?,
            names.index_storage_name(index)?,
            key
        ));
    } else {
        content.raw_fmt(format_args!(
            "  {}({}): readonly {}[] {{ return this.{}.get({}) ?? []; }}\n\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?,
            names.index_storage_name(index)?,
            key
        ));
    }
    Ok(())
}
