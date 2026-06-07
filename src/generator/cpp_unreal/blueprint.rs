// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{Result, codegen::CodeTableKind};

use super::{
    CodeIndex, CodeModel, CodeTable, CodeWriterLike, CppUnrealCodegenOptions,
    CppUnrealSupportLevel,
    symbols::CppUnrealNames,
    types::{api_qualified_type_name, blueprint_category},
};

pub(super) fn render_blueprint_facade_declaration(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    if !matches!(options.support, CppUnrealSupportLevel::Blueprint) {
        return Ok(());
    }
    let library_type = names.blueprint_library_type_name()?;
    content.raw("UCLASS()\n");
    content.raw_fmt(format_args!(
        "class {} : public UBlueprintFunctionLibrary {{\n",
        api_qualified_type_name(&library_type, options)
    ));
    content.raw("    GENERATED_BODY()\n\n");
    content.raw("public:\n");
    for table in &model.tables {
        if !matches!(table.kind, CodeTableKind::Collection) {
            continue;
        }
        for index in &table.indexes {
            render_blueprint_index_function_declaration(content, table, index, names, options)?;
        }
    }
    content.raw("};\n\n");
    Ok(())
}

fn render_blueprint_index_function_declaration(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppUnrealNames<'_>,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    let category = blueprint_category(options);
    let row_type = names.row_type_name(&table.name)?;
    let table_type = names.repository_type_name(&table.name)?;
    let params = names.cpp_index_params(index)?;
    let function_name = names.blueprint_index_function_name(table, index)?;
    if index.unique {
        content.raw_fmt(format_args!(
            "    UFUNCTION(BlueprintPure, Category={category:?})\n"
        ));
        content.raw_fmt(format_args!(
            "    static bool {function_name}(const {table_type}& Table, {params}, {row_type}& OutRow);\n"
        ));
    } else {
        content.raw_fmt(format_args!(
            "    UFUNCTION(BlueprintPure, Category={category:?})\n"
        ));
        content.raw_fmt(format_args!(
            "    static void {function_name}(const {table_type}& Table, {params}, TArray<{row_type}>& OutRows);\n"
        ));
    }
    if index.ordered {
        let range_name = names.blueprint_range_index_function_name(table, index)?;
        let range_params = names.cpp_range_index_params(table, index)?;
        content.raw_fmt(format_args!(
            "    UFUNCTION(BlueprintPure, Category={category:?})\n"
        ));
        content.raw_fmt(format_args!(
            "    static void {range_name}(const {table_type}& Table, {range_params}, TArray<{row_type}>& OutRows);\n"
        ));
    }
    Ok(())
}

pub(super) fn render_blueprint_facade_source(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    if !matches!(options.support, CppUnrealSupportLevel::Blueprint) {
        return Ok(());
    }
    for table in &model.tables {
        if !matches!(table.kind, CodeTableKind::Collection) {
            continue;
        }
        for index in &table.indexes {
            render_blueprint_index_function_source(content, table, index, names)?;
        }
    }
    Ok(())
}

fn render_blueprint_index_function_source(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let library_type = names.blueprint_library_type_name()?;
    let row_type = names.row_type_name(&table.name)?;
    let table_type = names.repository_type_name(&table.name)?;
    let params = names.cpp_index_params(index)?;
    let args = index_argument_names(index, names)?;
    let function_name = names.blueprint_index_function_name(table, index)?;
    let native_name = names.index_function_name(index)?;
    if index.unique {
        content.raw_fmt(format_args!(
            "bool {library_type}::{function_name}(const {table_type}& Table, {params}, {row_type}& OutRow) {{\n"
        ));
        content.raw_fmt(format_args!(
            "    const {row_type}* Row = Table.{native_name}({args});\n"
        ));
        content.raw("    if (Row == nullptr) { return false; }\n");
        content.raw("    OutRow = *Row;\n");
        content.raw("    return true;\n");
        content.raw("}\n\n");
    } else {
        content.raw_fmt(format_args!(
            "void {library_type}::{function_name}(const {table_type}& Table, {params}, TArray<{row_type}>& OutRows) {{\n"
        ));
        content.raw("    OutRows.Reset();\n");
        content.raw_fmt(format_args!(
            "    TArray<const {row_type}*> Rows = Table.{native_name}({args});\n"
        ));
        content.raw("    OutRows.Reserve(Rows.Num());\n");
        content.raw("    for (const auto* Row : Rows) {\n");
        content.raw("        if (Row != nullptr) { OutRows.Add(*Row); }\n");
        content.raw("    }\n");
        content.raw("}\n\n");
    }
    if index.ordered {
        let range_name = names.blueprint_range_index_function_name(table, index)?;
        let native_range_name = names.range_index_function_name(index)?;
        let range_params = names.cpp_range_index_params(table, index)?;
        content.raw_fmt(format_args!(
            "void {library_type}::{range_name}(const {table_type}& Table, {range_params}, TArray<{row_type}>& OutRows) {{\n"
        ));
        content.raw("    OutRows.Reset();\n");
        content.raw_fmt(format_args!(
            "    TArray<const {row_type}*> Rows = Table.{native_range_name}(Lower, Upper);\n"
        ));
        content.raw("    OutRows.Reserve(Rows.Num());\n");
        content.raw("    for (const auto* Row : Rows) {\n");
        content.raw("        if (Row != nullptr) { OutRows.Add(*Row); }\n");
        content.raw("    }\n");
        content.raw("}\n\n");
    }
    Ok(())
}

fn index_argument_names(index: &CodeIndex, names: &CppUnrealNames<'_>) -> Result<String> {
    index
        .fields
        .iter()
        .map(|field| names.field_name(field))
        .collect::<Result<Vec<_>>>()
        .map(|args| args.join(", "))
}
