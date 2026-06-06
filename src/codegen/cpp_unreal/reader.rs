// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Result;

use super::{
    CodeField, CodeStruct, CodeTable, CodeTypeRef, CodeWriterLike, symbols::CppUnrealNames,
};

pub(super) fn render_struct_reader_source(
    content: &mut impl CodeWriterLike,
    code_struct: &CodeStruct,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let type_name = names.struct_type_name(&code_struct.name)?;
    content.raw_fmt(format_args!(
        "TConfigForgeResult<{type_name}> {type_name}::{}(IConfigForgeRuntimeDataReader& Reader) {{\n",
        names.read_struct_function_name(&code_struct.name)?
    ));
    content.raw_fmt(format_args!("    {type_name} Value;\n"));
    for field in &code_struct.fields {
        render_field_read(content, field, names, &type_name)?;
    }
    content.raw_fmt(format_args!(
        "    return TConfigForgeResult<{type_name}>::Success(Value);\n"
    ));
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_table_reader_source(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let row_type = names.row_type_name(&table.name)?;
    content.raw_fmt(format_args!(
        "TConfigForgeResult<{row_type}> {row_type}::{}(IConfigForgeRuntimeDataReader& Reader) {{\n",
        names.read_row_function_name(&table.name)?
    ));
    content.raw_fmt(format_args!("    {row_type} Value;\n"));
    for field in &table.fields {
        render_field_read(content, field, names, &row_type)?;
    }
    content.raw_fmt(format_args!(
        "    return TConfigForgeResult<{row_type}>::Success(Value);\n"
    ));
    content.raw("}\n\n");
    Ok(())
}

fn render_field_read(
    content: &mut impl CodeWriterLike,
    field: &CodeField,
    names: &CppUnrealNames<'_>,
    result_type: &str,
) -> Result<()> {
    let field_name = names.field_name(field)?;
    if field.required {
        content.raw_fmt(format_args!(
            "    Reader.EnterRequiredField(TEXT({:?}));\n",
            field.name
        ));
        render_value_read(
            content,
            &format!("Value.{field_name}"),
            &field.code_type,
            names,
            result_type,
            "    ",
        )?;
        content.raw("    Reader.ExitField();\n");
    } else {
        content.raw_fmt(format_args!(
            "    if (Reader.EnterOptionalField(TEXT({:?}))) {{\n",
            field.name
        ));
        render_value_read(
            content,
            &format!("Value.{field_name}"),
            &field.code_type,
            names,
            result_type,
            "        ",
        )?;
        content.raw("        Reader.ExitField();\n");
        content.raw("    }\n");
    }
    Ok(())
}

fn render_value_read(
    content: &mut impl CodeWriterLike,
    target: &str,
    code_type: &CodeTypeRef,
    names: &CppUnrealNames<'_>,
    result_type: &str,
    indent: &str,
) -> Result<()> {
    match code_type {
        CodeTypeRef::Int => content.raw_fmt(format_args!("{indent}{target} = Reader.AsInt64();\n")),
        CodeTypeRef::Float => {
            content.raw_fmt(format_args!("{indent}{target} = Reader.AsDouble();\n"))
        }
        CodeTypeRef::Bool => content.raw_fmt(format_args!("{indent}{target} = Reader.AsBool();\n")),
        CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
            content.raw_fmt(format_args!("{indent}{target} = Reader.AsString();\n"));
        }
        CodeTypeRef::Enum { name } => {
            content.raw_fmt(format_args!(
                "{indent}switch (Reader.ReadEnumIndex(TArray<FString>{{"
            ));
            // enum values are not available from CodeTypeRef; reader adapters may map by current meta order later.
            content.raw("})) { default: break; }\n");
            content.raw_fmt(format_args!(
                "{indent}{target} = static_cast<{}>(0);\n",
                names.enum_type_name(name)?
            ));
        }
        CodeTypeRef::Struct { name } => {
            let struct_type = names.struct_type_name(name)?;
            let read_name = names.read_struct_function_name(name)?;
            let result_name = result_variable_name(target);
            content.raw_fmt(format_args!(
                "{indent}auto {result_name} = {struct_type}::{read_name}(Reader);\n"
            ));
            content.raw_fmt(format_args!("{indent}if (!{result_name}.IsOk()) {{ return TConfigForgeResult<{result_type}>::Failure({result_name}.Error.Message); }}\n"));
            content.raw_fmt(format_args!("{indent}{target} = {result_name}.Value;\n"));
        }
        CodeTypeRef::List(inner) => {
            content.raw_fmt(format_args!("{indent}{{\n"));
            content.raw_fmt(format_args!(
                "{indent}    const int64 Length = Reader.ReadListLength();\n"
            ));
            content.raw_fmt(format_args!("{indent}    {target}.Reserve(Length);\n"));
            content.raw_fmt(format_args!(
                "{indent}    for (int64 Index = 0; Index < Length; ++Index) {{\n"
            ));
            content.raw_fmt(format_args!("{indent}        Reader.EnterIndex(Index);\n"));
            let element_target = "Element";
            content.raw_fmt(format_args!(
                "{indent}        {} {element_target}{{}};\n",
                names.cpp_type(inner)?
            ));
            render_value_read(
                content,
                element_target,
                inner,
                names,
                result_type,
                &format!("{indent}        "),
            )?;
            content.raw_fmt(format_args!(
                "{indent}        {target}.Add({element_target});\n"
            ));
            content.raw_fmt(format_args!("{indent}    }}\n"));
            content.raw_fmt(format_args!("{indent}}}\n"));
        }
    }
    Ok(())
}

fn result_variable_name(target: &str) -> String {
    let mut name = target
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    name.push_str("Result");
    name
}
