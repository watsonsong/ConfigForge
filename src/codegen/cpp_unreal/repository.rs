// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ConfigForgeError, Result};

use super::{
    CodeForeignKey, CodeIndex, CodeModel, CodeRelationCardinality, CodeTable, CodeWriterLike,
    symbols::CppUnrealNames,
};

pub(super) fn render_index_function_declaration(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let row_type = names.row_type_name(&table.name)?;
    let function_name = names.index_function_name(index)?;
    let params = names.cpp_index_params(index)?;
    if index.unique {
        content.raw_fmt(format_args!(
            "    const {row_type}* {function_name}({params}) const;\n"
        ));
    } else {
        content.raw_fmt(format_args!(
            "    TArray<const {row_type}*> {function_name}({params}) const;\n"
        ));
    }
    if index.ordered {
        content.raw_fmt(format_args!(
            "    const {row_type}* {}({params}) const;\n",
            names.lower_bound_index_function_name(index)?
        ));
        content.raw_fmt(format_args!(
            "    const {row_type}* {}({params}) const;\n",
            names.upper_bound_index_function_name(index)?
        ));
        content.raw_fmt(format_args!(
            "    TArray<const {row_type}*> {}({}) const;\n",
            names.range_index_function_name(index)?,
            names.cpp_range_index_params(table, index)?
        ));
    }
    Ok(())
}
pub(super) fn render_table_load_source(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let repo_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    let load_name = names.load_function_name()?;
    content.raw_fmt(format_args!(
        "TConfigForgeResult<{repo_type}> {repo_type}::{load_name}(IConfigForgeRuntimeDataLoader& Loader) {{\n"
    ));
    content.raw_fmt(format_args!(
        "    auto ReaderResult = Loader.Open({});\n",
        names.data_file_constant_name()?
    ));
    content.raw_fmt(format_args!(
        "    if (!ReaderResult.IsOk()) {{ return TConfigForgeResult<{repo_type}>::Failure(ReaderResult.Error.Message); }}\n"
    ));
    content.raw("    IConfigForgeRuntimeDataReader& Reader = *ReaderResult.Value;\n");
    content.raw("    const int64 RowCount = Reader.ReadRowCount();\n");
    content.raw_fmt(format_args!("    TArray<{row_type}> Rows;\n"));
    content.raw("    Rows.Reserve(RowCount);\n");
    content.raw("    for (int64 RowIndex = 0; RowIndex < RowCount; ++RowIndex) {\n");
    content.raw("        Reader.EnterIndex(RowIndex);\n");
    content.raw_fmt(format_args!(
        "        auto RowResult = {row_type}::{}(Reader);\n",
        names.read_row_function_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "        if (!RowResult.IsOk()) {{ return TConfigForgeResult<{repo_type}>::Failure(RowResult.Error.Message); }}\n"
    ));
    content.raw("        Rows.Add(RowResult.Value);\n");
    content.raw("    }\n");
    content.raw_fmt(format_args!("    return FromRows(Rows);\n"));
    content.raw("}\n\n");

    content.raw_fmt(format_args!(
        "TConfigForgeResult<{repo_type}> {repo_type}::FromRows(const TArray<{row_type}>& InRows) {{\n"
    ));
    content.raw_fmt(format_args!("    {repo_type} Table;\n"));
    content.raw("    Table.Rows = InRows;\n");
    if !table.indexes.is_empty() {
        content.raw("    for (int32 RowIndex = 0; RowIndex < Table.Rows.Num(); ++RowIndex) {\n");
        content.raw("        const auto& Row = Table.Rows[RowIndex];\n");
        for index in &table.indexes {
            let storage_name = names.index_storage_name(index)?;
            let key = names.cpp_row_key_expr(table, index, "Row")?;
            if index.ordered {
                let key_type = names.cpp_key_type(table, index)?;
                content.raw_fmt(format_args!(
                    "        Table.{storage_name}.Add(TPair<{key_type}, int32>({key}, RowIndex));\n"
                ));
            } else if index.unique {
                content.raw_fmt(format_args!(
                    "        if (Table.{storage_name}.Contains({key})) {{ return TConfigForgeResult<{repo_type}>::Failure(TEXT(\"Duplicate unique index key for {repo_type}.{}\")); }}\n",
                    index.name
                ));
                content.raw_fmt(format_args!(
                    "        Table.{storage_name}.Add({key}, RowIndex);\n"
                ));
            } else {
                content.raw_fmt(format_args!(
                    "        Table.{storage_name}.FindOrAdd({key}).Add(RowIndex);\n"
                ));
            }
        }
        content.raw("    }\n");
        for index in table.indexes.iter().filter(|index| index.ordered) {
            let storage_name = names.index_storage_name(index)?;
            content.raw_fmt(format_args!(
                "    Table.{storage_name}.StableSort([](const auto& Left, const auto& Right) {{ return Left.Key < Right.Key; }});\n"
            ));
            if index.unique {
                content.raw_fmt(format_args!(
                    "    for (int32 Index = 1; Index < Table.{storage_name}.Num(); ++Index) {{\n"
                ));
                content.raw_fmt(format_args!(
                    "        if (Table.{storage_name}[Index - 1].Key == Table.{storage_name}[Index].Key) {{ return TConfigForgeResult<{repo_type}>::Failure(TEXT(\"Duplicate unique index key for {repo_type}.{}\")); }}\n",
                    index.name
                ));
                content.raw("    }\n");
            }
        }
    }
    content.raw_fmt(format_args!(
        "    return TConfigForgeResult<{repo_type}>::Success(Table);\n"
    ));
    content.raw("}\n\n");
    for index in &table.indexes {
        render_index_function_source(content, table, index, names)?;
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw_fmt(format_args!(
            "void {repo_type}::Link(const {}& Data) const {{\n",
            names.game_data_access_interface_name()?
        ));
        content.raw("    for (const auto& Row : Rows) {\n");
        content.raw("        Row.Link(Data);\n");
        content.raw("    }\n");
        content.raw("}\n\n");
    }
    Ok(())
}

fn render_index_function_source(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let repo_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    let function_name = names.index_function_name(index)?;
    let params = names.cpp_index_params(index)?;
    let storage_name = names.index_storage_name(index)?;
    let key = names.cpp_lookup_key_expr(table, index)?;
    if index.ordered {
        render_ordered_index_function_source(content, table, index, names)?;
    } else if index.unique {
        content.raw_fmt(format_args!(
            "const {row_type}* {repo_type}::{function_name}({params}) const {{\n"
        ));
        content.raw_fmt(format_args!(
            "    const int32* RowIndex = {storage_name}.Find({key});\n"
        ));
        content.raw("    if (RowIndex == nullptr) { return nullptr; }\n");
        content.raw("    return Rows.IsValidIndex(*RowIndex) ? &Rows[*RowIndex] : nullptr;\n");
        content.raw("}\n\n");
    } else {
        content.raw_fmt(format_args!(
            "TArray<const {row_type}*> {repo_type}::{function_name}({params}) const {{\n"
        ));
        content.raw_fmt(format_args!(
            "    const TArray<int32>* RowIndexes = {storage_name}.Find({key});\n"
        ));
        content.raw_fmt(format_args!("    TArray<const {row_type}*> Result;\n"));
        content.raw("    if (RowIndexes == nullptr) { return Result; }\n");
        content.raw("    Result.Reserve(RowIndexes->Num());\n");
        content.raw("    for (int32 RowIndex : *RowIndexes) {\n");
        content.raw("        if (Rows.IsValidIndex(RowIndex)) { Result.Add(&Rows[RowIndex]); }\n");
        content.raw("    }\n");
        content.raw("    return Result;\n");
        content.raw("}\n\n");
    }
    Ok(())
}

fn render_ordered_index_function_source(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let repo_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    let storage_name = names.index_storage_name(index)?;
    let key = names.cpp_lookup_key_expr(table, index)?;
    let params = names.cpp_index_params(index)?;
    let function_name = names.index_function_name(index)?;
    let lower_name = names.lower_bound_index_function_name(index)?;
    let upper_name = names.upper_bound_index_function_name(index)?;
    let range_name = names.range_index_function_name(index)?;
    let range_params = names.cpp_range_index_params(table, index)?;

    if index.unique {
        content.raw_fmt(format_args!(
            "const {row_type}* {repo_type}::{function_name}({params}) const {{\n"
        ));
        content.raw_fmt(format_args!(
            "    int32 EntryIndex = Algo::LowerBoundBy({storage_name}, {key}, [](const auto& Entry) {{ return Entry.Key; }});\n"
        ));
        content.raw_fmt(format_args!(
            "    if (!{storage_name}.IsValidIndex(EntryIndex) || !({storage_name}[EntryIndex].Key == {key})) {{ return nullptr; }}\n"
        ));
        content.raw_fmt(format_args!(
            "    const int32 RowIndex = {storage_name}[EntryIndex].Value;\n"
        ));
        content.raw("    return Rows.IsValidIndex(RowIndex) ? &Rows[RowIndex] : nullptr;\n");
        content.raw("}\n\n");
    } else {
        content.raw_fmt(format_args!(
            "TArray<const {row_type}*> {repo_type}::{function_name}({params}) const {{\n"
        ));
        content.raw_fmt(format_args!("    return {range_name}({key}, {key});\n"));
        content.raw("}\n\n");
    }

    content.raw_fmt(format_args!(
        "const {row_type}* {repo_type}::{lower_name}({params}) const {{\n"
    ));
    content.raw_fmt(format_args!(
        "    int32 EntryIndex = Algo::LowerBoundBy({storage_name}, {key}, [](const auto& Entry) {{ return Entry.Key; }});\n"
    ));
    content.raw_fmt(format_args!(
        "    if (!{storage_name}.IsValidIndex(EntryIndex)) {{ return nullptr; }}\n"
    ));
    content.raw_fmt(format_args!(
        "    const int32 RowIndex = {storage_name}[EntryIndex].Value;\n"
    ));
    content.raw("    return Rows.IsValidIndex(RowIndex) ? &Rows[RowIndex] : nullptr;\n");
    content.raw("}\n\n");

    content.raw_fmt(format_args!(
        "const {row_type}* {repo_type}::{upper_name}({params}) const {{\n"
    ));
    content.raw_fmt(format_args!(
        "    int32 EntryIndex = Algo::UpperBoundBy({storage_name}, {key}, [](const auto& Entry) {{ return Entry.Key; }});\n"
    ));
    content.raw_fmt(format_args!(
        "    if (!{storage_name}.IsValidIndex(EntryIndex)) {{ return nullptr; }}\n"
    ));
    content.raw_fmt(format_args!(
        "    const int32 RowIndex = {storage_name}[EntryIndex].Value;\n"
    ));
    content.raw("    return Rows.IsValidIndex(RowIndex) ? &Rows[RowIndex] : nullptr;\n");
    content.raw("}\n\n");

    content.raw_fmt(format_args!(
        "TArray<const {row_type}*> {repo_type}::{range_name}({range_params}) const {{\n"
    ));
    content.raw_fmt(format_args!(
        "    const int32 Begin = Algo::LowerBoundBy({storage_name}, Lower, [](const auto& Entry) {{ return Entry.Key; }});\n"
    ));
    content.raw_fmt(format_args!(
        "    const int32 End = Algo::UpperBoundBy({storage_name}, Upper, [](const auto& Entry) {{ return Entry.Key; }});\n"
    ));
    content.raw_fmt(format_args!("    TArray<const {row_type}*> Result;\n"));
    content.raw("    Result.Reserve(End - Begin);\n");
    content.raw("    for (int32 EntryIndex = Begin; EntryIndex < End; ++EntryIndex) {\n");
    content.raw_fmt(format_args!(
        "        const int32 RowIndex = {storage_name}[EntryIndex].Value;\n"
    ));
    content.raw("        if (Rows.IsValidIndex(RowIndex)) { Result.Add(&Rows[RowIndex]); }\n");
    content.raw("    }\n");
    content.raw("    return Result;\n");
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_row_relation_source(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    table: &CodeTable,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let target_table = table_by_name(model, &foreign_key.target_table)?;
        let target_index = index_by_name(target_table, &foreign_key.target_index)?;
        let source_row_type = names.row_type_name(&table.name)?;
        let target_row_type = names.row_type_name(&target_table.name)?;
        let function_name = names.relation_function_name(foreign_key)?;
        match foreign_key
            .linked_field
            .as_ref()
            .expect("linked field")
            .cardinality
        {
            CodeRelationCardinality::One | CodeRelationCardinality::OptionalOne => {
                content.raw_fmt(format_args!(
                    "const {target_row_type}* {source_row_type}::{function_name}(const {}& Data) const {{\n",
                    names.game_data_access_interface_name()?
                ));
                render_optional_relation_empty_check(content, foreign_key, names)?;
                content.raw_fmt(format_args!(
                    "    auto TargetTableResult = Data.{}();\n",
                    names.table_accessor_name(&target_table.name)?
                ));
                content.raw(
                    "    if (!TargetTableResult.IsOk() || TargetTableResult.Value == nullptr) { return nullptr; }\n",
                );
                content.raw_fmt(format_args!(
                    "    return TargetTableResult.Value->{}({});\n",
                    names.index_function_name(target_index)?,
                    relation_value_exprs(foreign_key, names)?.join(", ")
                ));
                content.raw("}\n\n");
            }
            CodeRelationCardinality::Many => {
                let source_field = foreign_key.fields.first().ok_or_else(|| {
                    ConfigForgeError::Validation(format!(
                        "cpp_unreal codegen foreign key `{}` has no source fields",
                        foreign_key.name
                    ))
                })?;
                let source_field_name = names.field_name(source_field)?;
                content.raw_fmt(format_args!(
                    "TArray<const {target_row_type}*> {source_row_type}::{function_name}(const {}& Data) const {{\n",
                    names.game_data_access_interface_name()?
                ));
                content.raw_fmt(format_args!(
                    "    TArray<const {target_row_type}*> Result;\n"
                ));
                content.raw_fmt(format_args!(
                    "    auto TargetTableResult = Data.{}();\n",
                    names.table_accessor_name(&target_table.name)?
                ));
                content.raw(
                    "    if (!TargetTableResult.IsOk() || TargetTableResult.Value == nullptr) { return Result; }\n",
                );
                if source_field.required {
                    content.raw_fmt(format_args!(
                        "    Result.Reserve({source_field_name}.Num());\n"
                    ));
                    content.raw_fmt(format_args!(
                        "    for (const auto& Key : {source_field_name}) {{\n"
                    ));
                } else {
                    content.raw_fmt(format_args!(
                        "    if (!{source_field_name}.IsSet()) {{ return Result; }}\n"
                    ));
                    content.raw_fmt(format_args!(
                        "    Result.Reserve({source_field_name}.GetValue().Num());\n"
                    ));
                    content.raw_fmt(format_args!(
                        "    for (const auto& Key : {source_field_name}.GetValue()) {{\n"
                    ));
                }
                content.raw_fmt(format_args!(
                    "        const auto* Row = TargetTableResult.Value->{}(Key);\n",
                    names.index_function_name(target_index)?
                ));
                content.raw("        if (Row != nullptr) { Result.Add(Row); }\n");
                content.raw("    }\n");
                content.raw("    return Result;\n");
                content.raw("}\n\n");
            }
        }
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        let row_type = names.row_type_name(&table.name)?;
        content.raw_fmt(format_args!(
            "void {row_type}::Link(const {}& Data) const {{\n",
            names.game_data_access_interface_name()?
        ));
        for foreign_key in table
            .foreign_keys
            .iter()
            .filter(|foreign_key| foreign_key.linked_field.is_some())
        {
            content.raw_fmt(format_args!(
                "    static_cast<void>({}(Data));\n",
                names.relation_function_name(foreign_key)?
            ));
        }
        content.raw("}\n\n");
    }
    Ok(())
}

fn render_optional_relation_empty_check(
    content: &mut impl CodeWriterLike,
    foreign_key: &CodeForeignKey,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let optional_fields = foreign_key
        .fields
        .iter()
        .filter(|field| !field.required)
        .collect::<Vec<_>>();
    if optional_fields.is_empty() {
        return Ok(());
    }
    let empty_conditions = optional_fields
        .iter()
        .map(|field| Ok(format!("!{}.IsSet()", names.field_name(field)?)))
        .collect::<Result<Vec<_>>>()?;
    content.raw_fmt(format_args!(
        "    if ({}) {{ return nullptr; }}\n",
        empty_conditions.join(" || ")
    ));
    Ok(())
}

fn relation_value_exprs(
    foreign_key: &CodeForeignKey,
    names: &CppUnrealNames<'_>,
) -> Result<Vec<String>> {
    foreign_key
        .fields
        .iter()
        .map(|field| {
            let field_name = names.field_name(field)?;
            if field.required {
                Ok(field_name)
            } else {
                Ok(format!("{field_name}.GetValue()"))
            }
        })
        .collect()
}

fn table_by_name<'a>(model: &'a CodeModel, name: &str) -> Result<&'a CodeTable> {
    model
        .tables
        .iter()
        .find(|table| table.name == name)
        .ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "cpp_unreal codegen foreign key references unknown target table `{name}`"
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
                "cpp_unreal codegen foreign key references unknown target index `{}.{}`",
                table.name, name
            ))
        })
}
