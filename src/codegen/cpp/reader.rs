// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    codegen::{CodeEnum, CodeField, CodeFragment, CodeStruct, CodeTable, CodeWriter},
};

use super::types::CppNames;

pub(super) fn render_enum_reader_source(
    content: &mut CodeWriter,
    code_enum: &CodeEnum,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_enum.name)?;
    let function_name = names.reader_function_name(&type_name)?;
    content.raw_fmt(format_args!(
        "{} {}(IRuntimeDataReader& reader) {{\n",
        type_name, function_name
    ));
    content.raw("    switch (reader.ReadEnumIndex({\n");
    for value in &code_enum.values {
        content.raw_fmt(format_args!("        {:?},\n", value.name));
    }
    content.raw("    })) {\n");
    for (ordinal, value) in code_enum.values.iter().enumerate() {
        content.raw_fmt(format_args!(
            "    case {ordinal}: return {}::{};\n",
            type_name,
            names.enum_variant_name(&value.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "    default: throw std::runtime_error(\"unknown {} enum index\");\n",
        code_enum.name
    ));
    content.raw("    }\n");
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_struct_reader_source(
    content: &mut CodeWriter,
    code_struct: &CodeStruct,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_struct.name)?;
    render_reader_function_source(content, &type_name, &code_struct.fields, None, names)
}

pub(super) fn render_fragment_reader_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    fragment: &CodeFragment,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.fragment_payload_type_name(table, fragment)?;
    render_reader_function_source(content, &type_name, &fragment.fields, None, names)
}

pub(super) fn render_row_reader_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.row_type_name(&table.name)?;
    render_reader_function_source(content, &type_name, &table.fields, Some(table), names)
}

fn render_reader_function_source(
    content: &mut CodeWriter,
    type_name: &str,
    fields: &[CodeField],
    table: Option<&CodeTable>,
    names: &CppNames<'_>,
) -> Result<()> {
    let function_name = names.reader_function_name(type_name)?;
    let fragment_count = table.map(|table| table.fragments.len()).unwrap_or(0);
    let optional_slot_count =
        fields.iter().filter(|field| !field.required).count() + fragment_count;
    content.raw_fmt(format_args!(
        "{} {}(IRuntimeDataReader& reader) {{\n",
        type_name, function_name
    ));
    content.raw_fmt(format_args!(
        "    reader.BeginRecord({optional_slot_count});\n"
    ));
    content.raw_fmt(format_args!("    {} value{{}};\n", type_name));
    for field in fields {
        content.raw_fmt(format_args!(
            "    value.{} = {};\n",
            names.field_name(&field.name)?,
            names.cpp_read_field_expr(field)?
        ));
    }
    if let Some(table) = table {
        for fragment in &table.fragments {
            let field_name = names.field_name(&fragment.name)?;
            let payload_type = names.fragment_payload_type_name(table, fragment)?;
            let reader_function = names.reader_function_name(&payload_type)?;
            content.raw_fmt(format_args!(
                concat!(
                    "    if (reader.EnterOptionalField({:?})) {{\n",
                    "        ReaderScope fragment_scope(reader);\n",
                    "        value.{field_name} = {reader_function}(reader);\n",
                    "    }}\n",
                ),
                fragment.name,
                field_name = field_name,
                reader_function = reader_function
            ));
        }
    }
    content.raw("    reader.EndRecord();\n");
    content.raw("    return value;\n");
    content.raw("}\n\n");
    Ok(())
}
