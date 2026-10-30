// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    code_artifact::{
        CodeEnum, CodeField, CodeFragment, CodeStruct, CodeTable, CodeWriterLike, ReaderShapePlan,
    },
};

use super::{
    repository::{render_row_relation_cache_initializers, render_row_relation_method_initializers},
    types::TypeScriptNames,
};

pub(super) fn render_enum_reader_function(
    content: &mut impl CodeWriterLike,
    code_enum: &CodeEnum,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_enum.name)?;
    let function_name = names.reader_function_name(&type_name)?;
    content.raw_fmt(format_args!(
        "function {function_name}(reader: RuntimeDataReader): {type_name} {{\n"
    ));
    content.raw("  switch (readEnumIndex(reader, [\n");
    for value in &code_enum.values {
        content.raw_fmt(format_args!(
            "    {:?},\n",
            names.enum_value_name(&value.name)?
        ));
    }
    content.raw("  ])) {\n");
    for (ordinal, value) in code_enum.values.iter().enumerate() {
        content.raw_fmt(format_args!(
            "    case {ordinal}: return {:?};\n",
            names.enum_value_name(&value.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "    default: throw RuntimeDataError.decode('unknown {} enum index');\n",
        code_enum.name
    ));
    content.raw("  }\n");
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_struct_reader_function(
    content: &mut impl CodeWriterLike,
    code_struct: &CodeStruct,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_struct.name)?;
    render_reader_function_body(content, &type_name, &code_struct.fields, None, names)?;
    Ok(())
}

pub(super) fn render_fragment_reader_function(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    fragment: &CodeFragment,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.fragment_payload_type_name(table, fragment)?;
    render_reader_function_body(content, &type_name, &fragment.fields, None, names)?;
    Ok(())
}

pub(super) fn render_row_reader_function(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.row_type_name(&table.name)?;
    render_reader_function_body(content, &type_name, &table.fields, Some(table), names)?;
    Ok(())
}

fn render_reader_function_body(
    content: &mut impl CodeWriterLike,
    type_name: &str,
    fields: &[CodeField],
    table: Option<&CodeTable>,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let function_name = names.reader_function_name(type_name)?;
    let reader_shape = table
        .map(ReaderShapePlan::for_table)
        .unwrap_or_else(|| ReaderShapePlan::for_fields(fields));
    content.raw_fmt(format_args!(
        "function {function_name}(reader: RuntimeDataReader): {type_name} {{\n"
    ));
    content.raw_fmt(format_args!(
        "  reader.beginRecord?.({});\n",
        reader_shape.optional_slot_count
    ));
    content.raw_fmt(format_args!("  const value: {type_name} = {{\n"));
    for field in fields {
        content.raw_fmt(format_args!(
            "    {}: {},\n",
            names.field_name(&field.name)?,
            names.ts_read_field_expr(field)?
        ));
    }
    if let Some(table) = table {
        for fragment in reader_shape.fragments {
            let field_name = names.field_name(&fragment.name)?;
            let payload_type = names.fragment_payload_type_name(table, fragment)?;
            let reader_function = names.reader_function_name(&payload_type)?;
            content.raw_fmt(format_args!(
                concat!(
                    "    {field_name}: optionalField(reader, {source_name:?}, (value) => {reader_function}(value)),\n",
                ),
                field_name = field_name,
                source_name = fragment.name,
                reader_function = reader_function,
            ));
        }
        render_row_relation_cache_initializers(content, table, names)?;
        render_row_relation_method_initializers(content, table, names)?;
    }
    content.raw("  };\n");
    content.raw("  reader.endRecord?.();\n");
    content.raw("  return value;\n");
    content.raw("}\n\n");
    Ok(())
}
