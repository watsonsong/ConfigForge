// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{ConfigForgeError, Result, artifact::ArtifactSink};

use super::{
    CaseStyle, CheckedStreamingCodeWriter, CodeEnum, CodeExportInput, CodeField, CodeGenerator,
    CodeIndex, CodeModel, CodeStruct, CodeTable, CodeTypeRef, CodeWriterConfig, CodeWriterLike,
    NamingConfig, build_code_model,
    ir::{CodeForeignKey, CodeRelationCardinality, require_non_empty_index_fields},
};

mod symbols;
mod types;

use symbols::validate_generated_symbols;
use types::CppUnrealNames;

#[cfg(test)]
mod tests;

pub const LANGUAGE: &str = "cpp_unreal";

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CppUnrealSupportLevel {
    #[default]
    Containers,
    Blueprint,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CppUnrealCodegenOptions {
    #[serde(default)]
    pub support: CppUnrealSupportLevel,
    #[serde(default)]
    pub api_macro: Option<String>,
    #[serde(default)]
    pub blueprint_category: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
}

pub(crate) struct CppUnrealCodeGenerator;

impl CodeGenerator for CppUnrealCodeGenerator {
    fn language(&self) -> &'static str {
        LANGUAGE
    }

    fn default_naming(&self) -> NamingConfig {
        NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::PascalCase,
            field: CaseStyle::PascalCase,
            function: CaseStyle::PascalCase,
            constant: CaseStyle::PascalCase,
        }
    }

    fn default_formatting(&self) -> CodeWriterConfig {
        CodeWriterConfig::default().with_indent_width(4)
    }

    #[cfg(test)]
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<super::GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let options = cpp_unreal_options(input)?;
        let names = CppUnrealNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;

        let header_file_name = input.target.single_file_name("generated", "h");
        let source_file_name = format!("{}.cpp", input.target.file_stem("generated"));
        let mut header = super::CodeWriter::new(input.target.options.formatting);
        render_header_file(self, &mut header, input, &model, &options)?;
        let mut source = super::CodeWriter::new(input.target.options.formatting);
        render_source_file(self, &mut source, input, &header_file_name, &options)?;

        Ok(vec![
            super::GeneratedFile {
                path: input.target.out_dir.join(header_file_name),
                content: header.finish(),
            },
            super::GeneratedFile {
                path: input.target.out_dir.join(source_file_name),
                content: source.finish(),
            },
        ])
    }

    fn generate_to_sink(&self, input: &CodeExportInput<'_>, sink: &ArtifactSink) -> Result<()> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let options = cpp_unreal_options(input)?;
        let names = CppUnrealNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;

        let header_file_name = input.target.single_file_name("generated", "h");
        let source_file_name = format!("{}.cpp", input.target.file_stem("generated"));
        let header_path = input.target.out_dir.join(&header_file_name);
        let source_path = input.target.out_dir.join(&source_file_name);

        let header_stream = sink.create(header_path.clone())?;
        let mut header =
            CheckedStreamingCodeWriter::new(header_stream, input.target.options.formatting);
        render_header_file(self, &mut header, input, &model, &options)?;
        let header_stream = header
            .finish()
            .map_err(|source| ConfigForgeError::WriteFile {
                path: header_path,
                source,
            })?;
        header_stream.finish()?;

        let source_stream = sink.create(source_path.clone())?;
        let mut source =
            CheckedStreamingCodeWriter::new(source_stream, input.target.options.formatting);
        render_source_file(self, &mut source, input, &header_file_name, &options)?;
        let source_stream = source
            .finish()
            .map_err(|source| ConfigForgeError::WriteFile {
                path: source_path,
                source,
            })?;
        source_stream.finish()?;
        Ok(())
    }
}

fn cpp_unreal_options(input: &CodeExportInput<'_>) -> Result<CppUnrealCodegenOptions> {
    let options = input
        .target
        .options
        .language_options::<CppUnrealCodegenOptions>(LANGUAGE)?;
    validate_api_macro(options.api_macro.as_deref())?;
    Ok(options)
}

fn validate_api_macro(api_macro: Option<&str>) -> Result<()> {
    if let Some(api_macro) = api_macro {
        let is_valid = !api_macro.is_empty()
            && api_macro
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
            && api_macro
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_uppercase() || *byte == b'_');
        if !is_valid {
            return Err(ConfigForgeError::Validation(format!(
                "failed to parse {LANGUAGE} codegen options: api_macro `{api_macro}` must be an uppercase C++ macro identifier"
            )));
        }
    }
    Ok(())
}

fn render_header_file(
    generator: &dyn CodeGenerator,
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
    model: &CodeModel,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    let names = CppUnrealNames::new(&input.target.options);
    write_file_header(generator, content, input);
    content.raw("#pragma once\n\n");
    content.raw("#include \"CoreMinimal.h\"\n\n");
    render_runtime_declarations(content);
    render_namespace_open(content, input, options);
    content.raw_fmt(format_args!(
        "struct {};\n\n",
        names.struct_type_name("game_data")?
    ));
    for code_enum in &model.enums {
        render_enum_declaration(content, code_enum, &names)?;
    }
    for code_struct in &model.structs {
        render_struct_declaration(content, code_struct, &names)?;
    }
    render_compound_index_key_structs(content, model, &names)?;
    for table in &model.tables {
        render_table_declaration(content, table, &names)?;
    }
    content.raw_fmt(format_args!(
        "struct {} {{\n",
        names.struct_type_name("game_data")?
    ));
    content.raw("    static constexpr const TCHAR* Generator = TEXT(\"ConfigForge\");\n");
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "    {} {};\n",
            names.table_data_type_name(table)?,
            names.member_name(&table.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "    static TConfigForgeResult<{}> {}(IConfigForgeRuntimeDataLoader& Loader);\n",
        names.struct_type_name("game_data")?,
        names.load_function_name()?
    ));
    content.raw("    void LinkAll() const;\n");
    content.raw("};\n\n");
    render_lazy_game_data_declaration(content, model, &names)?;
    render_namespace_close(content, input, options);
    Ok(())
}

fn render_lazy_game_data_declaration(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    content.raw("struct FLazyGameData {\n");
    content.raw("    explicit FLazyGameData(IConfigForgeRuntimeDataLoader& InLoader) : Loader(InLoader) {}\n");
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "    TConfigForgeResult<{}> {}() const;\n",
            names.table_data_type_name(table)?,
            names.table_accessor_name(&table.name)?
        ));
    }
    content.raw("    TConfigForgeResult<FGameData> PreloadAll() const;\n");
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

fn render_runtime_declarations(content: &mut impl CodeWriterLike) {
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

fn render_enum_declaration(
    content: &mut impl CodeWriterLike,
    code_enum: &CodeEnum,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "enum class {} {{\n",
        names.enum_type_name(&code_enum.name)?
    ));
    for value in &code_enum.values {
        content.raw_fmt(format_args!(
            "    {},\n",
            names.enum_variant_name(&value.name)?
        ));
    }
    content.raw("};\n\n");
    Ok(())
}

fn render_struct_declaration(
    content: &mut impl CodeWriterLike,
    code_struct: &CodeStruct,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "struct {} {{\n",
        names.struct_type_name(&code_struct.name)?
    ));
    for field in &code_struct.fields {
        content.raw_fmt(format_args!(
            "    {} {};\n",
            names.cpp_field_type(field)?,
            names.field_name(field)?
        ));
    }
    render_text_helper_declarations(content, &code_struct.name, &code_struct.fields, names)?;
    content.raw_fmt(format_args!(
        "    static TConfigForgeResult<{}> {}(IConfigForgeRuntimeDataReader& Reader);\n",
        names.struct_type_name(&code_struct.name)?,
        names.read_struct_function_name(&code_struct.name)?
    ));
    content.raw("};\n\n");
    Ok(())
}

fn render_table_declaration(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "struct {} {{\n",
        names.row_type_name(&table.name)?
    ));
    if matches!(table.kind, super::CodeTableKind::Object) {
        content.raw_fmt(format_args!(
            "    static constexpr const TCHAR* {} = TEXT({:?});\n",
            names.data_file_constant_name()?,
            table.data_file
        ));
    }
    for field in &table.fields {
        content.raw_fmt(format_args!(
            "    {} {};\n",
            names.cpp_field_type(field)?,
            names.field_name(field)?
        ));
    }
    render_text_helper_declarations(content, &table.name, &table.fields, names)?;
    render_row_relation_declarations(content, table, names)?;
    content.raw_fmt(format_args!(
        "    static TConfigForgeResult<{}> {}(IConfigForgeRuntimeDataReader& Reader);\n",
        names.row_type_name(&table.name)?,
        names.read_row_function_name(&table.name)?
    ));
    content.raw("};\n\n");
    if matches!(table.kind, super::CodeTableKind::Collection) {
        content.raw_fmt(format_args!(
            "struct {} {{\n",
            names.repository_type_name(&table.name)?
        ));
        content.raw_fmt(format_args!(
            "    TArray<{}> Rows;\n",
            names.row_type_name(&table.name)?
        ));
        for index in &table.indexes {
            content.raw_fmt(format_args!(
                "    {} {};\n",
                names.cpp_index_storage(table, index)?,
                names.index_storage_name(index)?
            ));
        }
        content.raw_fmt(format_args!(
            "    static constexpr const TCHAR* {} = TEXT({:?});\n",
            names.data_file_constant_name()?,
            table.data_file
        ));
        content.raw_fmt(format_args!(
            "    static TConfigForgeResult<{}> {}(IConfigForgeRuntimeDataLoader& Loader);\n",
            names.repository_type_name(&table.name)?,
            names.load_function_name()?
        ));
        content.raw_fmt(format_args!(
            "    static TConfigForgeResult<{}> FromRows(const TArray<{}>& InRows);\n",
            names.repository_type_name(&table.name)?,
            names.row_type_name(&table.name)?
        ));
        for index in &table.indexes {
            render_index_function_declaration(content, table, index, names)?;
        }
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            content.raw("    void Link(const FGameData& Data) const;\n");
        }
        content.raw("};\n\n");
    }
    Ok(())
}

fn render_text_helper_declarations(
    content: &mut impl CodeWriterLike,
    context_prefix: &str,
    fields: &[CodeField],
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    for field in fields
        .iter()
        .filter(|field| matches!(field.code_type, CodeTypeRef::Text))
    {
        let function_name = names.text_function_name(&field.name)?;
        let field_name = names.field_name(field)?;
        let context = format!("{context_prefix}.{}", field.name);
        if field.required {
            content.raw_fmt(format_args!(
                "    FString {function_name}(const TFunction<TOptional<FString>(const TCHAR*, const FString&)>& Translator) const {{\n"
            ));
            content.raw_fmt(format_args!(
                "        auto Translated = Translator(TEXT({context:?}), {field_name});\n"
            ));
            content.raw_fmt(format_args!(
                "        return Translated.IsSet() ? Translated.GetValue() : {field_name};\n"
            ));
            content.raw("    }\n");
        } else {
            content.raw_fmt(format_args!(
                "    TOptional<FString> {function_name}(const TFunction<TOptional<FString>(const TCHAR*, const FString&)>& Translator) const {{\n"
            ));
            content.raw_fmt(format_args!(
                "        if (!{field_name}.IsSet()) {{ return TOptional<FString>(); }}\n"
            ));
            content.raw_fmt(format_args!(
                "        auto Translated = Translator(TEXT({context:?}), {field_name}.GetValue());\n"
            ));
            content.raw_fmt(format_args!(
                "        return Translated.IsSet() ? Translated : {field_name};\n"
            ));
            content.raw("    }\n");
        }
    }
    Ok(())
}

fn render_row_relation_declarations(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let target_row_type = names.row_type_name(&foreign_key.target_table)?;
        let function_name = names.relation_function_name(foreign_key)?;
        match foreign_key
            .linked_field
            .as_ref()
            .expect("linked field")
            .cardinality
        {
            CodeRelationCardinality::One => content.raw_fmt(format_args!(
                "    const {target_row_type}* {function_name}(const FGameData& Data) const;\n"
            )),
            CodeRelationCardinality::OptionalOne => content.raw_fmt(format_args!(
                "    const {target_row_type}* {function_name}(const FGameData& Data) const;\n"
            )),
            CodeRelationCardinality::Many => content.raw_fmt(format_args!(
                "    TArray<const {target_row_type}*> {function_name}(const FGameData& Data) const;\n"
            )),
        }
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw("    void Link(const FGameData& Data) const;\n");
    }
    Ok(())
}

fn render_compound_index_key_structs(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    for table in &model.tables {
        for index in &table.indexes {
            if index.fields.len() <= 1 {
                continue;
            }
            let key_type = names.compound_index_key_type_name(table, index)?;
            content.raw_fmt(format_args!("struct {key_type} {{\n"));
            for field in &index.fields {
                content.raw_fmt(format_args!(
                    "    {} {};\n",
                    names.cpp_field_type(field)?,
                    names.field_name(field)?
                ));
            }
            content.raw("\n");
            content.raw_fmt(format_args!(
                "    bool operator==(const {key_type}& Other) const {{\n"
            ));
            content.raw("        return ");
            content.raw(
                index
                    .fields
                    .iter()
                    .map(|field| {
                        let field_name = names.field_name(field)?;
                        Ok(format!("{field_name} == Other.{field_name}"))
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(" && "),
            );
            content.raw(";\n");
            content.raw("    }\n");
            content.raw_fmt(format_args!(
                "    bool operator<(const {key_type}& Other) const {{\n"
            ));
            content.raw("        ");
            for (field_index, field) in index.fields.iter().enumerate() {
                let field_name = names.field_name(field)?;
                if field_index > 0 {
                    content.raw("        ");
                }
                content.raw_fmt(format_args!(
                    "if ({field_name} < Other.{field_name}) {{ return true; }}\n"
                ));
                content.raw_fmt(format_args!(
                    "        if (Other.{field_name} < {field_name}) {{ return false; }}\n"
                ));
            }
            content.raw("        return false;\n");
            content.raw("    }\n");
            content.raw("};\n\n");
            content.raw_fmt(format_args!(
                "FORCEINLINE uint32 GetTypeHash(const {key_type}& Value) {{\n"
            ));
            content.raw("    uint32 Hash = 0;\n");
            for field in &index.fields {
                content.raw_fmt(format_args!(
                    "    Hash = HashCombine(Hash, ::GetTypeHash(Value.{}));\n",
                    names.field_name(field)?
                ));
            }
            content.raw("    return Hash;\n");
            content.raw("}\n\n");
        }
    }
    Ok(())
}

fn render_index_function_declaration(
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

fn render_source_file(
    generator: &dyn CodeGenerator,
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
    header_file_name: &str,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    write_file_header(generator, content, input);
    content.raw_fmt(format_args!("#include {:?}\n\n", header_file_name));
    render_namespace_open(content, input, options);
    let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
    let names = CppUnrealNames::new(&input.target.options);
    for code_struct in &model.structs {
        render_struct_reader_source(content, code_struct, &names)?;
    }
    for table in &model.tables {
        render_table_reader_source(content, table, &names)?;
    }
    for table in &model.tables {
        if matches!(table.kind, super::CodeTableKind::Collection) {
            render_table_load_source(content, table, &names)?;
        }
    }
    for table in &model.tables {
        render_row_relation_source(content, &model, table, &names)?;
    }
    render_game_data_load_source(content, &model, &names)?;
    render_lazy_game_data_source(content, &model, &names)?;
    render_namespace_close(content, input, options);
    Ok(())
}

fn render_struct_reader_source(
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

fn render_table_reader_source(
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
) -> Result<()> {
    match code_type {
        CodeTypeRef::Int => content.raw_fmt(format_args!("    {target} = Reader.AsInt64();\n")),
        CodeTypeRef::Float => content.raw_fmt(format_args!("    {target} = Reader.AsDouble();\n")),
        CodeTypeRef::Bool => content.raw_fmt(format_args!("    {target} = Reader.AsBool();\n")),
        CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
            content.raw_fmt(format_args!("    {target} = Reader.AsString();\n"));
        }
        CodeTypeRef::Enum { name } => {
            content.raw_fmt(format_args!(
                "    switch (Reader.ReadEnumIndex(TArray<FString>{{"
            ));
            // enum values are not available from CodeTypeRef; reader adapters may map by current meta order later.
            content.raw("})) { default: break; }\n");
            content.raw_fmt(format_args!(
                "    {target} = static_cast<{}>(0);\n",
                names.enum_type_name(name)?
            ));
        }
        CodeTypeRef::Struct { name } => {
            let struct_type = names.struct_type_name(name)?;
            let read_name = names.read_struct_function_name(name)?;
            let result_name = result_variable_name(target);
            content.raw_fmt(format_args!(
                "    auto {result_name} = {struct_type}::{read_name}(Reader);\n"
            ));
            content.raw_fmt(format_args!("    if (!{result_name}.IsOk()) {{ return TConfigForgeResult<{result_type}>::Failure({result_name}.Error.Message); }}\n"));
            content.raw_fmt(format_args!("    {target} = {result_name}.Value;\n"));
        }
        CodeTypeRef::List(inner) => {
            content.raw("    {\n");
            content.raw("        const int64 Length = Reader.ReadListLength();\n");
            content.raw_fmt(format_args!("        {target}.Reserve(Length);\n"));
            content.raw("        for (int64 Index = 0; Index < Length; ++Index) {\n");
            content.raw("            Reader.EnterIndex(Index);\n");
            let element_target = "Element";
            content.raw_fmt(format_args!(
                "            {} {element_target}{{}};\n",
                names.cpp_type(inner)?
            ));
            render_value_read_with_indent(content, element_target, inner, names, "            ")?;
            content.raw_fmt(format_args!(
                "            {target}.Add({element_target});\n"
            ));
            content.raw("        }\n");
            content.raw("    }\n");
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

fn render_value_read_with_indent(
    content: &mut impl CodeWriterLike,
    target: &str,
    code_type: &CodeTypeRef,
    names: &CppUnrealNames<'_>,
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
        CodeTypeRef::Enum { name } => content.raw_fmt(format_args!(
            "{indent}{target} = static_cast<{}>(Reader.ReadEnumIndex(TArray<FString>{{}}));\n",
            names.enum_type_name(name)?
        )),
        CodeTypeRef::Struct { name } => {
            let struct_type = names.struct_type_name(name)?;
            let read_name = names.read_struct_function_name(name)?;
            content.raw_fmt(format_args!(
                "{indent}auto ElementResult = {struct_type}::{read_name}(Reader);\n"
            ));
            content.raw_fmt(format_args!("{indent}{target} = ElementResult.Value;\n"));
        }
        CodeTypeRef::List(_) => content.raw_fmt(format_args!(
            "{indent}// Nested lists are not supported by Meta.\n"
        )),
    }
    Ok(())
}

fn render_table_load_source(
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
            "void {repo_type}::Link(const FGameData& Data) const {{\n"
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

fn render_row_relation_source(
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
                    "const {target_row_type}* {source_row_type}::{function_name}(const FGameData& Data) const {{\n"
                ));
                render_optional_relation_empty_check(content, foreign_key, names)?;
                content.raw_fmt(format_args!(
                    "    return Data.{}.{}({});\n",
                    names.member_name(&target_table.name)?,
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
                    "TArray<const {target_row_type}*> {source_row_type}::{function_name}(const FGameData& Data) const {{\n"
                ));
                content.raw_fmt(format_args!(
                    "    TArray<const {target_row_type}*> Result;\n"
                ));
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
                    "        const auto* Row = Data.{}.{}(Key);\n",
                    names.member_name(&target_table.name)?,
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
            "void {row_type}::Link(const FGameData& Data) const {{\n"
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

fn render_game_data_load_source(
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
            super::CodeTableKind::Collection => {
                let repo_type = names.repository_type_name(&table.name)?;
                content.raw_fmt(format_args!(
                    "    auto {member_name}Result = {repo_type}::{load_name}(Loader);\n"
                ));
                content.raw_fmt(format_args!("    if (!{member_name}Result.IsOk()) {{ return TConfigForgeResult<{game_data_type}>::Failure({member_name}Result.Error.Message); }}\n"));
                content.raw_fmt(format_args!(
                    "    Data.{member_name} = {member_name}Result.Value;\n"
                ));
            }
            super::CodeTableKind::Object => {
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

fn render_lazy_game_data_source(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    for table in &model.tables {
        let table_type = names.table_data_type_name(table)?;
        let accessor_name = names.table_accessor_name(&table.name)?;
        let member_name = names.member_name(&table.name)?;
        content.raw_fmt(format_args!(
            "TConfigForgeResult<{table_type}> FLazyGameData::{accessor_name}() const {{\n"
        ));
        content.raw_fmt(format_args!("    if (!{member_name}.IsSet()) {{\n"));
        match table.kind {
            super::CodeTableKind::Collection => {
                let repo_type = names.repository_type_name(&table.name)?;
                content.raw_fmt(format_args!(
                    "        auto LoadResult = {repo_type}::{}(Loader);\n",
                    names.load_function_name()?
                ));
                content.raw_fmt(format_args!(
                    "        if (!LoadResult.IsOk()) {{ return TConfigForgeResult<{table_type}>::Failure(LoadResult.Error.Message); }}\n"
                ));
                content.raw_fmt(format_args!("        {member_name} = LoadResult.Value;\n"));
            }
            super::CodeTableKind::Object => {
                let row_type = names.row_type_name(&table.name)?;
                content.raw_fmt(format_args!(
                    "        auto ReaderResult = Loader.Open({row_type}::{});\n",
                    names.data_file_constant_name()?
                ));
                content.raw_fmt(format_args!(
                    "        if (!ReaderResult.IsOk()) {{ return TConfigForgeResult<{table_type}>::Failure(ReaderResult.Error.Message); }}\n"
                ));
                content.raw_fmt(format_args!(
                    "        auto RowResult = {row_type}::{}(*ReaderResult.Value);\n",
                    names.read_row_function_name(&table.name)?
                ));
                content.raw_fmt(format_args!(
                    "        if (!RowResult.IsOk()) {{ return TConfigForgeResult<{table_type}>::Failure(RowResult.Error.Message); }}\n"
                ));
                content.raw_fmt(format_args!("        {member_name} = RowResult.Value;\n"));
            }
        }
        content.raw("    }\n");
        content.raw_fmt(format_args!(
            "    return TConfigForgeResult<{table_type}>::Success({member_name}.GetValue());\n"
        ));
        content.raw("}\n\n");
    }
    content.raw("TConfigForgeResult<FGameData> FLazyGameData::PreloadAll() const {\n");
    content.raw("    FGameData Data;\n");
    for table in &model.tables {
        let table_type = names.table_data_type_name(table)?;
        let accessor_name = names.table_accessor_name(&table.name)?;
        let member_name = names.member_name(&table.name)?;
        content.raw_fmt(format_args!(
            "    auto {member_name}Result = {accessor_name}();\n"
        ));
        content.raw_fmt(format_args!(
            "    if (!{member_name}Result.IsOk()) {{ return TConfigForgeResult<FGameData>::Failure({member_name}Result.Error.Message); }}\n"
        ));
        content.raw_fmt(format_args!(
            "    Data.{member_name} = {member_name}Result.Value;\n"
        ));
        content.raw_fmt(format_args!(
            "    static_cast<void>(sizeof({table_type}));\n"
        ));
    }
    content.raw("    Data.LinkAll();\n");
    content.raw("    return TConfigForgeResult<FGameData>::Success(Data);\n");
    content.raw("}\n\n");
    Ok(())
}

fn configured_namespace<'a>(
    input: &'a CodeExportInput<'_>,
    options: &'a CppUnrealCodegenOptions,
) -> Option<&'a str> {
    options
        .namespace
        .as_deref()
        .or(input.target.namespace.as_deref())
}

fn render_namespace_open(
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
    options: &CppUnrealCodegenOptions,
) {
    if let Some(namespace) = configured_namespace(input, options) {
        content.raw_fmt(format_args!("namespace {namespace} {{\n\n"));
    }
}

fn render_namespace_close(
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
    options: &CppUnrealCodegenOptions,
) {
    if let Some(namespace) = configured_namespace(input, options) {
        content.raw_fmt(format_args!("\n}} // namespace {namespace}\n"));
    }
}

fn write_file_header(
    generator: &dyn CodeGenerator,
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
) {
    if let Some(template) = &input.target.options.file_header {
        let project = &input.project;
        let mut output = template.to_owned();
        output = output.replace("{{project.name}}", project.name.as_deref().unwrap_or(""));
        output = output.replace("{{project.desc}}", project.desc.as_deref().unwrap_or(""));
        output = output.replace(
            "{{project.copyright}}",
            project.copyright.as_deref().unwrap_or(""),
        );
        output = output.replace("{{datetime}}", &input.generated_at);
        if !output.ends_with('\n') {
            output.push('\n');
        }
        content.raw(output);
        return;
    }

    let prefix = generator.line_comment_prefix();
    content.line(format!(
        "{prefix} Copyright (c) 2026 ConfigForge contributors"
    ));
    content.line(format!("{prefix} SPDX-License-Identifier: Apache-2.0"));
    content.blank_line();
    content.line(format!("{prefix} Generated by config-forge. Do not edit."));
}
