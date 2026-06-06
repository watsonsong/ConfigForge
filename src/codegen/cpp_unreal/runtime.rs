// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{Result, codegen::CodeTableKind};

use super::{
    CodeModel, CodeTable, CodeWriterLike, CppUnrealCodegenOptions, symbols::CppUnrealNames,
    types::render_struct_prefix,
};

pub(super) fn render_runtime_declarations(content: &mut impl CodeWriterLike) {
    content.raw(concat!(
        "struct FConfigForgeError {\n",
        "    FString Message;\n",
        "};\n\n",
        "template <typename T>\n",
        "struct TConfigForgeResult {\n",
        "    T Value{};\n",
        "    FConfigForgeError Error{};\n",
        "    bool bOk = true;\n\n",
        "    static TConfigForgeResult Success(const T& InValue) {\n",
        "        TConfigForgeResult Result;\n",
        "        Result.Value = InValue;\n",
        "        return Result;\n",
        "    }\n\n",
        "    static TConfigForgeResult Failure(const FString& Message) {\n",
        "        TConfigForgeResult Result;\n",
        "        Result.bOk = false;\n",
        "        Result.Error.Message = Message;\n",
        "        return Result;\n",
        "    }\n\n",
        "    bool IsOk() const { return bOk; }\n",
        "};\n\n",
        "class IConfigForgeRuntimeDataReader {\n",
        "public:\n",
        "    virtual ~IConfigForgeRuntimeDataReader() = default;\n",
        "    virtual void EnterIndex(int64 Index) = 0;\n",
        "    virtual int64 AsInt64() const = 0;\n",
        "    virtual double AsDouble() const = 0;\n",
        "    virtual bool AsBool() const = 0;\n",
        "    virtual FString AsString() const = 0;\n",
        "    virtual void EnterRequiredField(const TCHAR* Name) = 0;\n",
        "    virtual bool EnterOptionalField(const TCHAR* Name) = 0;\n",
        "    virtual void ExitField() = 0;\n",
        "    virtual int64 ReadEnumIndex(const TArray<FString>& Variants) = 0;\n",
        "    virtual int64 ReadRowCount() = 0;\n",
        "    virtual int64 ReadListLength() = 0;\n",
        "};\n\n",
        "class IConfigForgeRuntimeDataLoader {\n",
        "public:\n",
        "    virtual ~IConfigForgeRuntimeDataLoader() = default;\n",
        "    virtual TConfigForgeResult<TUniquePtr<IConfigForgeRuntimeDataReader>> Open(const FString& Path) = 0;\n",
        "};\n\n"
    ));
}

pub(super) fn render_lazy_game_data_declaration(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let lazy_type = names.lazy_game_data_type_name()?;
    let access_type = names.game_data_access_interface_name()?;
    let game_data_type = names.struct_type_name("game_data")?;
    render_struct_prefix(
        content,
        &lazy_type,
        &CppUnrealCodegenOptions::default(),
        false,
    );
    content.raw_fmt(format_args!(
        "struct {lazy_type} : public {access_type} {{\n"
    ));
    content.raw_fmt(format_args!(
        "    explicit {lazy_type}(IConfigForgeRuntimeDataLoader& InLoader) : Loader(InLoader) {{}}\n"
    ));
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "    TConfigForgeResult<const {}*> {}() const override;\n",
            names.table_data_type_name(table)?,
            names.table_accessor_name(&table.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "    TConfigForgeResult<{game_data_type}> PreloadAll() const;\n"
    ));
    content.raw("private:\n");
    content.raw("    IConfigForgeRuntimeDataLoader& Loader;\n");
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "    mutable TOptional<{}> {};\n",
            names.table_data_type_name(table)?,
            names.member_name(&table.name)?
        ));
    }
    content.raw("};\n");
    Ok(())
}

pub(super) fn render_game_data_access_interface(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let access_type = names.game_data_access_interface_name()?;
    content.raw_fmt(format_args!("class {access_type} {{\n"));
    content.raw("public:\n");
    content.raw_fmt(format_args!("    virtual ~{access_type}() = default;\n"));
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "    virtual TConfigForgeResult<const {}*> {}() const = 0;\n",
            names.table_data_type_name(table)?,
            names.table_accessor_name(&table.name)?
        ));
    }
    content.raw("};\n\n");
    Ok(())
}

pub(super) fn render_game_data_load_source(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let game_data_type = names.struct_type_name("game_data")?;
    let load_name = names.load_function_name()?;
    content.raw_fmt(format_args!(
        "TConfigForgeResult<{game_data_type}> {game_data_type}::{load_name}(IConfigForgeRuntimeDataLoader& Loader) {{\n"
    ));
    content.raw_fmt(format_args!("    {game_data_type} Data;\n"));
    for table in &model.tables {
        let member_name = names.member_name(&table.name)?;
        match table.kind {
            CodeTableKind::Collection => {
                let repo_type = names.repository_type_name(&table.name)?;
                content.raw_fmt(format_args!(
                    "    auto {member_name}Result = {repo_type}::{load_name}(Loader);\n"
                ));
                content.raw_fmt(format_args!("    if (!{member_name}Result.IsOk()) {{ return TConfigForgeResult<{game_data_type}>::Failure({member_name}Result.Error.Message); }}\n"));
                content.raw_fmt(format_args!(
                    "    Data.{member_name} = {member_name}Result.Value;\n"
                ));
            }
            CodeTableKind::Object => {
                render_object_table_load_source(
                    content,
                    table,
                    names,
                    &game_data_type,
                    &member_name,
                )?;
            }
        }
    }
    content.raw("    Data.LinkAll();\n");
    content.raw_fmt(format_args!(
        "    return TConfigForgeResult<{game_data_type}>::Success(Data);\n"
    ));
    content.raw("}\n\n");
    render_game_data_link_all_source(content, model, names)?;
    render_game_data_access_source(content, model, names)?;
    Ok(())
}

fn render_object_table_load_source(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &CppUnrealNames<'_>,
    result_type: &str,
    member_name: &str,
) -> Result<()> {
    let row_type = names.row_type_name(&table.name)?;
    content.raw_fmt(format_args!(
        "    auto {member_name}ReaderResult = Loader.Open({row_type}::{});\n",
        names.data_file_constant_name()?
    ));
    content.raw_fmt(format_args!(
        "    if (!{member_name}ReaderResult.IsOk()) {{ return TConfigForgeResult<{result_type}>::Failure({member_name}ReaderResult.Error.Message); }}\n"
    ));
    content.raw_fmt(format_args!(
        "    auto {member_name}Result = {row_type}::{}(*{member_name}ReaderResult.Value);\n",
        names.read_row_function_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    if (!{member_name}Result.IsOk()) {{ return TConfigForgeResult<{result_type}>::Failure({member_name}Result.Error.Message); }}\n"
    ));
    content.raw_fmt(format_args!(
        "    Data.{member_name} = {member_name}Result.Value;\n"
    ));
    Ok(())
}

fn render_game_data_link_all_source(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let game_data_type = names.struct_type_name("game_data")?;
    content.raw_fmt(format_args!("void {game_data_type}::LinkAll() const {{\n"));
    for table in &model.tables {
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            content.raw_fmt(format_args!(
                "    {}.Link(*this);\n",
                names.member_name(&table.name)?
            ));
        }
    }
    content.raw("}\n\n");
    Ok(())
}

fn render_game_data_access_source(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let game_data_type = names.struct_type_name("game_data")?;
    for table in &model.tables {
        let table_type = names.table_data_type_name(table)?;
        let accessor_name = names.table_accessor_name(&table.name)?;
        let member_name = names.member_name(&table.name)?;
        content.raw_fmt(format_args!(
            "TConfigForgeResult<const {table_type}*> {game_data_type}::{accessor_name}() const {{\n"
        ));
        content.raw_fmt(format_args!(
            "    return TConfigForgeResult<const {table_type}*>::Success(&{member_name});\n"
        ));
        content.raw("}\n\n");
    }
    Ok(())
}

pub(super) fn render_lazy_game_data_source(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let lazy_type = names.lazy_game_data_type_name()?;
    let game_data_type = names.struct_type_name("game_data")?;
    for table in &model.tables {
        let table_type = names.table_data_type_name(table)?;
        let accessor_name = names.table_accessor_name(&table.name)?;
        let member_name = names.member_name(&table.name)?;
        content.raw_fmt(format_args!(
            "TConfigForgeResult<const {table_type}*> {lazy_type}::{accessor_name}() const {{\n"
        ));
        content.raw_fmt(format_args!("    if (!{member_name}.IsSet()) {{\n"));
        match table.kind {
            CodeTableKind::Collection => {
                let repo_type = names.repository_type_name(&table.name)?;
                content.raw_fmt(format_args!(
                    "        auto LoadResult = {repo_type}::{}(Loader);\n",
                    names.load_function_name()?
                ));
                content.raw_fmt(format_args!(
                    "        if (!LoadResult.IsOk()) {{ return TConfigForgeResult<const {table_type}*>::Failure(LoadResult.Error.Message); }}\n"
                ));
                content.raw_fmt(format_args!("        {member_name} = LoadResult.Value;\n"));
            }
            CodeTableKind::Object => {
                let row_type = names.row_type_name(&table.name)?;
                content.raw_fmt(format_args!(
                    "        auto ReaderResult = Loader.Open({row_type}::{});\n",
                    names.data_file_constant_name()?
                ));
                content.raw_fmt(format_args!(
                    "        if (!ReaderResult.IsOk()) {{ return TConfigForgeResult<const {table_type}*>::Failure(ReaderResult.Error.Message); }}\n"
                ));
                content.raw_fmt(format_args!(
                    "        auto RowResult = {row_type}::{}(*ReaderResult.Value);\n",
                    names.read_row_function_name(&table.name)?
                ));
                content.raw_fmt(format_args!(
                    "        if (!RowResult.IsOk()) {{ return TConfigForgeResult<const {table_type}*>::Failure(RowResult.Error.Message); }}\n"
                ));
                content.raw_fmt(format_args!("        {member_name} = RowResult.Value;\n"));
            }
        }
        content.raw("    }\n");
        content.raw_fmt(format_args!(
            "    return TConfigForgeResult<const {table_type}*>::Success(&{member_name}.GetValue());\n"
        ));
        content.raw("}\n\n");
    }
    content.raw_fmt(format_args!(
        "TConfigForgeResult<{game_data_type}> {lazy_type}::PreloadAll() const {{\n"
    ));
    content.raw_fmt(format_args!("    {game_data_type} Data;\n"));
    for table in &model.tables {
        let table_type = names.table_data_type_name(table)?;
        let accessor_name = names.table_accessor_name(&table.name)?;
        let member_name = names.member_name(&table.name)?;
        content.raw_fmt(format_args!(
            "    auto {member_name}Result = {accessor_name}();\n"
        ));
        content.raw_fmt(format_args!(
            "    if (!{member_name}Result.IsOk()) {{ return TConfigForgeResult<{game_data_type}>::Failure({member_name}Result.Error.Message); }}\n"
        ));
        content.raw_fmt(format_args!(
            "    if ({member_name}Result.Value == nullptr) {{ return TConfigForgeResult<{game_data_type}>::Failure(TEXT(\"Lazy table accessor returned null for {member_name}\")); }}\n"
        ));
        content.raw_fmt(format_args!(
            "    Data.{member_name} = *{member_name}Result.Value;\n"
        ));
        content.raw_fmt(format_args!(
            "    static_cast<void>(sizeof({table_type}));\n"
        ));
    }
    content.raw("    Data.LinkAll();\n");
    content.raw_fmt(format_args!(
        "    return TConfigForgeResult<{game_data_type}>::Success(Data);\n"
    ));
    content.raw("}\n\n");
    Ok(())
}
