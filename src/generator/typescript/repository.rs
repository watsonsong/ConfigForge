// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    codegen::{
        CodeIndex, CodeModel, CodeTable, CodeTableKind, CodeWriterLike,
        ir::{CodeForeignKey, CodeRelationCardinality, relation_key_plan},
    },
};

use super::types::TypeScriptNames;

pub(super) fn render_table(
    content: &mut impl CodeWriterLike,
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
    if !table.indexes.is_empty() {
        content.raw("    for (let rowIndex = 0; rowIndex < rows.length; rowIndex++) {\n");
        content.raw("      const row = rows[rowIndex];\n");
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
                    "      this.{}.set(key_{}, rowIndex);\n",
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
                    "      list_{}.push(rowIndex);\n",
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
    }
    content.raw("  }\n\n");
    content.raw_fmt(format_args!(
        "  static async load(loader: RuntimeDataLoader): Promise<{}> {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw("    const root = await loader.open(this.dataFile);\n");
    if table.schema_revision.is_some() {
        content.raw("    const schemaRevision = root.schemaRevision?.();\n");
        content.raw(concat!(
            "    if (schemaRevision !== undefined && BigInt(schemaRevision) !== this.expectedSchemaRevision) {\n",
            "      throw new Error(`schema revision mismatch for ${this.dataFile}`);\n",
            "    }\n",
        ));
    }
    content.raw("    enterRequiredField(root, \"rows\");\n");
    content.raw("    const rowCount = readRowCount(root);\n");
    content.raw_fmt(format_args!(
        "    const rows: {}[] = [];\n",
        names.row_type_name(&table.name)?
    ));
    content.raw("    try {\n");
    content.raw("      for (let rowIndex = 0; rowIndex < rowCount; rowIndex++) {\n");
    content.raw("        root.enterIndex(rowIndex);\n");
    content.raw("        try {\n");
    content.raw_fmt(format_args!(
        "          rows.push({}(root));\n",
        names.reader_function_name(&names.row_type_name(&table.name)?)?
    ));
    content.raw("        } finally {\n");
    content.raw("          exitField(root);\n");
    content.raw("        }\n");
    content.raw("      }\n");
    content.raw("    } finally {\n");
    content.raw("      exitField(root);\n");
    content.raw("    }\n");
    content.raw("    root.ensureExhausted?.();\n");
    content.raw_fmt(format_args!(
        "    return new {}(rows ?? []);\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw("  }\n\n");
    content.raw_fmt(format_args!(
        "  static loadSync(loader: RuntimeDataLoader): {} {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw("    const root = openRuntimeDataSync(loader, this.dataFile);\n");
    if table.schema_revision.is_some() {
        content.raw("    const schemaRevision = root.schemaRevision?.();\n");
        content.raw(concat!(
            "    if (schemaRevision !== undefined && BigInt(schemaRevision) !== this.expectedSchemaRevision) {\n",
            "      throw new Error(`schema revision mismatch for ${this.dataFile}`);\n",
            "    }\n",
        ));
    }
    content.raw("    enterRequiredField(root, \"rows\");\n");
    content.raw("    const rowCount = readRowCount(root);\n");
    content.raw_fmt(format_args!(
        "    const rows: {}[] = [];\n",
        names.row_type_name(&table.name)?
    ));
    content.raw("    try {\n");
    content.raw("      for (let rowIndex = 0; rowIndex < rowCount; rowIndex++) {\n");
    content.raw("        root.enterIndex(rowIndex);\n");
    content.raw("        try {\n");
    content.raw_fmt(format_args!(
        "          rows.push({}(root));\n",
        names.reader_function_name(&names.row_type_name(&table.name)?)?
    ));
    content.raw("        } finally {\n");
    content.raw("          exitField(root);\n");
    content.raw("        }\n");
    content.raw("      }\n");
    content.raw("    } finally {\n");
    content.raw("      exitField(root);\n");
    content.raw("    }\n");
    content.raw("    root.ensureExhausted?.();\n");
    content.raw_fmt(format_args!(
        "    return new {}(rows ?? []);\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw("  }\n\n");
    if table.preserve_source_order {
        content.raw("  get length(): number { return this.rows_.length; }\n\n");
        content.raw_fmt(format_args!(
            "  rows(): readonly {}[] {{ return this.rows_; }}\n\n",
            names.row_type_name(&table.name)?
        ));
        content.raw_fmt(format_args!(
            "  rowAt(rowIndex: number): {} | undefined {{ return this.rows_[rowIndex]; }}\n\n",
            names.row_type_name(&table.name)?
        ));
    }
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw("  link(data: GameData): void {\n");
        content.raw("    for (const row of this.rows_) {\n");
        content.raw("      row.link(data);\n");
        content.raw("    }\n");
        content.raw("  }\n\n");
    }
    content.raw("}\n\n");
    render_row_at_helper(content, table, names)?;
    for index in table.indexes.iter().filter(|index| index.unique) {
        render_index_of_helper(content, table, index, names)?;
    }
    Ok(())
}

fn row_at_helper_name(table: &CodeTable, names: &TypeScriptNames<'_>) -> Result<String> {
    Ok(format!(
        "__{}RowAt",
        names.repository_type_name(&table.name)?
    ))
}

fn index_of_helper_name(
    table: &CodeTable,
    index: &CodeIndex,
    names: &TypeScriptNames<'_>,
) -> Result<String> {
    Ok(format!(
        "__{}{}",
        names.repository_type_name(&table.name)?,
        names.index_of_function_name(index)?
    ))
}

fn render_row_at_helper(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "function {}(table: {}, rowIndex: number): {} | undefined {{\n",
        row_at_helper_name(table, names)?,
        names.repository_type_name(&table.name)?,
        names.row_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "  return (table as unknown as {{ rows_: {}[] }}).rows_[rowIndex];\n",
        names.row_type_name(&table.name)?
    ));
    content.raw("}\n\n");
    Ok(())
}

fn render_index_function(
    content: &mut impl CodeWriterLike,
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
            "  {}({}): {} | undefined {{\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?
        ));
        content.raw_fmt(format_args!(
            "    const rowIndex = this.{}.get({});\n",
            names.index_storage_name(index)?,
            key
        ));
        content.raw("    return rowIndex === undefined ? undefined : this.rows_[rowIndex];\n");
        content.raw("  }\n\n");
    } else {
        content.raw_fmt(format_args!(
            "  {}({}): readonly {}[] {{ return (this.{}.get({}) ?? []).map((rowIndex) => this.rows_[rowIndex]); }}\n\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?,
            names.index_storage_name(index)?,
            key
        ));
    }
    Ok(())
}

fn render_index_of_helper(
    content: &mut impl CodeWriterLike,
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
    content.raw_fmt(format_args!(
        "function {}(table: {}, {}): number | undefined {{\n",
        index_of_helper_name(table, index, names)?,
        names.repository_type_name(&table.name)?,
        params
    ));
    content.raw_fmt(format_args!(
        "  const index = (table as unknown as {{ {}: Map<{}, number> }}).{};\n",
        names.index_storage_name(index)?,
        if index.fields.len() == 1 {
            names.ts_field_type(&index.fields[0])?
        } else {
            "string".to_owned()
        },
        names.index_storage_name(index)?
    ));
    content.raw_fmt(format_args!("  return index.get({key});\n"));
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_row_relation_members(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let target_table = table_by_name(model, &foreign_key.target_table)?;
        let target_row_type = names.row_type_name(&target_table.name)?;
        let function_name = names.relation_function_name(foreign_key)?;
        match foreign_key
            .linked_field
            .as_ref()
            .expect("linked field")
            .cardinality
        {
            CodeRelationCardinality::One => content.raw_fmt(format_args!(
                "  {}(data: GameData): {};\n",
                function_name, target_row_type
            )),
            CodeRelationCardinality::OptionalOne => content.raw_fmt(format_args!(
                "  {}(data: GameData): {} | undefined;\n",
                function_name, target_row_type
            )),
            CodeRelationCardinality::Many => content.raw_fmt(format_args!(
                "  {}(data: GameData): readonly {}[];\n",
                function_name, target_row_type
            )),
        }
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw("  link(data: GameData): void;\n");
    }
    Ok(())
}

pub(super) fn render_row_relation_cache_initializers(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let cache_name = names.relation_cache_field_name(foreign_key)?;
        match foreign_key
            .linked_field
            .as_ref()
            .expect("linked field")
            .cardinality
        {
            CodeRelationCardinality::Many => {
                content.raw_fmt(format_args!("    {cache_name}: undefined,\n"));
            }
            _ => {
                content.raw_fmt(format_args!("    {cache_name}: LINK_UNRESOLVED,\n"));
            }
        }
    }
    Ok(())
}

pub(super) fn render_row_relation_cache_members(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let cache_name = names.relation_cache_field_name(foreign_key)?;
        match foreign_key
            .linked_field
            .as_ref()
            .expect("linked field")
            .cardinality
        {
            CodeRelationCardinality::Many => {
                content.raw_fmt(format_args!("  {cache_name}: LinkListCache;\n"));
            }
            _ => {
                content.raw_fmt(format_args!("  {cache_name}: LinkCache;\n"));
            }
        }
    }
    Ok(())
}

pub(super) fn render_row_relation_method_initializers(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let row_type = names.row_type_name(&table.name)?;
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let function_name = names.relation_function_name(foreign_key)?;
        content.raw_fmt(format_args!(
            "    {function_name}: {function_name}{row_type},\n"
        ));
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw_fmt(format_args!("    link: link{row_type},\n"));
    }
    Ok(())
}

pub(super) fn render_row_relation_methods(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
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
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        let row_type = names.row_type_name(&table.name)?;
        content.raw_fmt(format_args!(
            "function link{}(this: {}, data: GameData): void {{\n",
            row_type, row_type
        ));
        content.raw("  const row = this;\n");
        for foreign_key in table
            .foreign_keys
            .iter()
            .filter(|foreign_key| foreign_key.linked_field.is_some())
        {
            content.raw_fmt(format_args!(
                "  row.{}(data);\n",
                names.relation_function_name(foreign_key)?
            ));
        }
        content.raw("}\n\n");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_single_relation_method(
    content: &mut impl CodeWriterLike,
    source_table: &CodeTable,
    foreign_key: &CodeForeignKey,
    target_table: &CodeTable,
    target_index: &CodeIndex,
    names: &TypeScriptNames<'_>,
    target_row_type: &str,
    function_name: &str,
    optional: bool,
) -> Result<()> {
    let source_row_type = names.row_type_name(&source_table.name)?;
    let cache_name = names.relation_cache_field_name(foreign_key)?;
    let target_data_field = names.field_name(&target_table.name)?;
    let row_at_name = row_at_helper_name(target_table, names)?;
    let index_of_name = index_of_helper_name(target_table, target_index, names)?;
    let return_type = if optional {
        format!("{target_row_type} | undefined")
    } else {
        target_row_type.to_owned()
    };
    content.raw_fmt(format_args!(
        "function {}{}(this: {}, data: GameData): {} {{\n",
        function_name, source_row_type, source_row_type, return_type
    ));
    content.raw("  const row = this;\n");
    content.raw_fmt(format_args!("  const cached = row.{cache_name};\n"));
    content.raw("  if (cached === LINK_EMPTY) {\n");
    if optional {
        content.raw("    return undefined;\n");
    } else {
        content.raw_fmt(format_args!(
            "    throw new Error({:?});\n",
            format!(
                "required relation `{}` unexpectedly cached empty",
                foreign_key.name
            )
        ));
    }
    content.raw("  }\n");
    content.raw("  if (cached >= 0) {\n");
    content.raw_fmt(format_args!(
        "    const rowValue = {row_at_name}(data.{target_data_field}, cached);\n"
    ));
    content.raw_fmt(format_args!(
        "    if (rowValue === undefined) {{ throw new Error({:?}); }}\n",
        format!(
            "cached relation `{}` row index out of bounds",
            foreign_key.name
        )
    ));
    content.raw("    return rowValue;\n");
    content.raw("  }\n");
    if optional {
        render_optional_empty_check(content, foreign_key, names, &cache_name)?;
    }
    let value_exprs = relation_value_exprs(foreign_key, names)?;
    let lookup_key = names.ts_relation_lookup_key(target_index, &value_exprs)?;
    content.raw_fmt(format_args!(
        "  const rowIndex = {index_of_name}(data.{target_data_field}, {lookup_key});\n"
    ));
    content.raw_fmt(format_args!(
        "  if (rowIndex === undefined) {{ throw new Error({:?}); }}\n",
        format!(
            "foreign key relation `{}.{}` could not resolve target `{}.{}`",
            source_table.name, foreign_key.name, target_table.name, target_index.name
        )
    ));
    content.raw_fmt(format_args!("  row.{cache_name} = rowIndex;\n"));
    content.raw_fmt(format_args!(
        "  const rowValue = {row_at_name}(data.{target_data_field}, rowIndex);\n"
    ));
    content.raw_fmt(format_args!(
        "  if (rowValue === undefined) {{ throw new Error({:?}); }}\n",
        format!(
            "resolved relation `{}` row index out of bounds",
            foreign_key.name
        )
    ));
    content.raw("  return rowValue;\n");
    content.raw("}\n\n");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_many_relation_method(
    content: &mut impl CodeWriterLike,
    source_table: &CodeTable,
    foreign_key: &CodeForeignKey,
    target_table: &CodeTable,
    target_index: &CodeIndex,
    names: &TypeScriptNames<'_>,
    target_row_type: &str,
    function_name: &str,
) -> Result<()> {
    let source_row_type = names.row_type_name(&source_table.name)?;
    let cache_name = names.relation_cache_field_name(foreign_key)?;
    let source_field = foreign_key.fields.first().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "foreign key `{}` has no source fields",
            foreign_key.name
        ))
    })?;
    let source_field_name = names.field_name(&source_field.name)?;
    let target_data_field = names.field_name(&target_table.name)?;
    let row_at_name = row_at_helper_name(target_table, names)?;
    let index_of_name = index_of_helper_name(target_table, target_index, names)?;
    content.raw_fmt(format_args!(
        "function {}{}(this: {}, data: GameData): readonly {}[] {{\n",
        function_name, source_row_type, source_row_type, target_row_type
    ));
    content.raw("  const row = this;\n");
    content.raw_fmt(format_args!("  let rowIndexes = row.{cache_name};\n"));
    content.raw("  if (rowIndexes === undefined) {\n");
    content.raw("    rowIndexes = [];\n");
    if source_field.required {
        content.raw_fmt(format_args!(
            "    for (const key of row.{source_field_name}) {{\n"
        ));
    } else {
        content.raw_fmt(format_args!(
            "    for (const key of row.{source_field_name} ?? []) {{\n"
        ));
    }
    content.raw_fmt(format_args!(
        "      const rowIndex = {index_of_name}(data.{target_data_field}, key);\n"
    ));
    content.raw_fmt(format_args!(
        "      if (rowIndex === undefined) {{ throw new Error({:?}); }}\n",
        format!(
            "foreign key relation `{}.{}` could not resolve list target `{}.{}`",
            source_table.name, foreign_key.name, target_table.name, target_index.name
        )
    ));
    content.raw("      rowIndexes.push(rowIndex);\n");
    content.raw("    }\n");
    content.raw_fmt(format_args!("    row.{cache_name} = rowIndexes;\n"));
    content.raw("  }\n");
    content.raw_fmt(format_args!("  return rowIndexes.map((rowIndex) => {{\n"));
    content.raw_fmt(format_args!(
        "    const rowValue = {row_at_name}(data.{target_data_field}, rowIndex);\n"
    ));
    content.raw_fmt(format_args!(
        "    if (rowValue === undefined) {{ throw new Error({:?}); }}\n",
        format!(
            "resolved list relation `{}` row index out of bounds",
            foreign_key.name
        )
    ));
    content.raw("    return rowValue;\n");
    content.raw("  });\n");
    content.raw("}\n\n");
    Ok(())
}

fn render_optional_empty_check(
    content: &mut impl CodeWriterLike,
    foreign_key: &CodeForeignKey,
    names: &TypeScriptNames<'_>,
    cache_name: &str,
) -> Result<()> {
    let key_plan = relation_key_plan(foreign_key);
    if key_plan.optional_key_fields.is_empty() {
        return Ok(());
    }
    let empty_conditions = key_plan
        .optional_key_fields
        .iter()
        .map(|field| {
            Ok(format!(
                "row.{} === undefined",
                names.field_name(&field.name)?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    if key_plan.all_key_fields_optional {
        content.raw_fmt(format_args!(
            "  if ({}) {{ row.{cache_name} = LINK_EMPTY; return undefined; }}\n",
            empty_conditions.join(" && ")
        ));
        if key_plan.should_report_partial_empty {
            content.raw_fmt(format_args!(
                "  if ({}) {{ throw new Error({:?}); }}\n",
                empty_conditions.join(" || "),
                format!(
                    "foreign key relation `{}` has partial empty source key",
                    foreign_key.name
                )
            ));
        }
    } else if key_plan.should_report_partial_empty {
        content.raw_fmt(format_args!(
            "  if ({}) {{ throw new Error({:?}); }}\n",
            empty_conditions.join(" || "),
            format!(
                "foreign key relation `{}` has partial empty source key",
                foreign_key.name
            )
        ));
    }
    Ok(())
}

fn relation_value_exprs(
    foreign_key: &CodeForeignKey,
    names: &TypeScriptNames<'_>,
) -> Result<Vec<String>> {
    foreign_key
        .fields
        .iter()
        .map(|field| {
            let field_name = names.field_name(&field.name)?;
            if field.required {
                Ok(format!("row.{field_name}"))
            } else {
                Ok(format!("row.{field_name}!"))
            }
        })
        .collect()
}

pub(super) fn render_game_data_link_all(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    content.raw("  linkAll(): void {\n");
    for table in &model.tables {
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            match table.kind {
                CodeTableKind::Collection => content.raw_fmt(format_args!(
                    "    this.{}.link(this);\n",
                    names.field_name(&table.name)?
                )),
                CodeTableKind::Object => content.raw_fmt(format_args!(
                    "    this.{}.link(this);\n",
                    names.field_name(&table.name)?
                )),
            }
        }
    }
    content.raw("  }\n");
    Ok(())
}

fn table_by_name<'a>(model: &'a CodeModel, name: &str) -> Result<&'a CodeTable> {
    model
        .tables
        .iter()
        .find(|table| table.name == name)
        .ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "typescript codegen foreign key references unknown target table `{name}`"
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
                "typescript codegen foreign key references unknown target index `{}.{}`",
                table.name, name
            ))
        })
}
