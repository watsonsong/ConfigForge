// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{codegen::{CodeEnum, CodeField, CodeFragment, CodeStruct, CodeTable, CodeWriter}, Result};

use super::types::TypeScriptNames;

pub(super) fn render_enum_reader_function(
    content: &mut CodeWriter,
    code_enum: &CodeEnum,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_enum.name)?;
    let function_name = names.reader_function_name(&type_name)?;
    content.raw_fmt(format_args!(
        "function {function_name}(reader: RuntimeDataReader): {type_name} {{\n"
    ));
    content.raw_fmt(format_args!(
        "  if (reader.isPositional?.()) {{ return {}FromBinaryOrdinal(reader.readEnumOrdinal!()); }}\n",
        function_name
    ));
    content.raw("  const value = reader.asString();\n");
    let values = code_enum
        .values
        .iter()
        .map(|value| {
            Ok(format!(
                "value === {:?}",
                names.enum_value_name(&value.name)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(" || ");
    content.raw_fmt(format_args!(
        "  if ({values}) {{ return value as {type_name}; }}\n"
    ));
    content.raw_fmt(format_args!(
        "  throw new Error(`unknown {} value ${{value}}`);\n",
        code_enum.name
    ));
    content.raw("}\n\n");
    content.raw_fmt(format_args!(
        "function {}FromBinaryOrdinal(ordinal: number): {type_name} {{\n",
        function_name
    ));
    content.raw("  switch (ordinal) {\n");
    for (ordinal, value) in code_enum.values.iter().enumerate() {
        content.raw_fmt(format_args!(
            "    case {ordinal}: return {:?};\n",
            names.enum_value_name(&value.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "    default: throw new Error(`unknown {} ordinal ${{ordinal}}`);\n",
        code_enum.name
    ));
    content.raw("  }\n");
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_struct_reader_function(
    content: &mut CodeWriter,
    code_struct: &CodeStruct,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_struct.name)?;
    render_reader_function_body(content, &type_name, &code_struct.fields, None, names)?;
    Ok(())
}

pub(super) fn render_fragment_reader_function(
    content: &mut CodeWriter,
    table: &CodeTable,
    fragment: &CodeFragment,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.fragment_payload_type_name(table, fragment)?;
    render_reader_function_body(content, &type_name, &fragment.fields, None, names)?;
    Ok(())
}

pub(super) fn render_row_reader_function(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.row_type_name(&table.name)?;
    render_reader_function_body(content, &type_name, &table.fields, Some(table), names)?;
    Ok(())
}

fn render_reader_function_body(
    content: &mut CodeWriter,
    type_name: &str,
    fields: &[CodeField],
    table: Option<&CodeTable>,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let function_name = names.reader_function_name(type_name)?;
    let fragment_count = table.map(|table| table.fragments.len()).unwrap_or(0);
    let optional_slot_count =
        fields.iter().filter(|field| !field.required).count() + fragment_count;
    content.raw_fmt(format_args!(
        "function {function_name}(reader: RuntimeDataReader): {type_name} {{\n"
    ));
    content.raw_fmt(format_args!(
        "  reader.beginRecord?.({optional_slot_count});\n"
    ));
    content.raw("  const value = {\n");
    for field in fields {
        content.raw_fmt(format_args!(
            "    {}: {},\n",
            names.field_name(&field.name)?,
            names.ts_read_field_expr(field)?
        ));
    }
    if let Some(table) = table {
        for fragment in &table.fragments {
            let field_name = names.field_name(&fragment.name)?;
            let payload_type = names.fragment_payload_type_name(table, fragment)?;
            let reader_function = names.reader_function_name(&payload_type)?;
            content.raw_fmt(format_args!(
                concat!(
                    "    {field_name}: reader.isPositional?.()\n",
                    "      ? (reader.readOptionalPresent!() ? {reader_function}(reader) : undefined)\n",
                    "      : optionalField(reader, {source_name}, (value) => {reader_function}(value)),\n",
                ),
                field_name = field_name,
                source_name = format!("{:?}", fragment.name),
                reader_function = reader_function,
            ));
        }
    }
    content.raw("  };\n");
    content.raw("  reader.endRecord?.();\n");
    content.raw("  return value;\n");
    content.raw("}\n\n");
    Ok(())
}
