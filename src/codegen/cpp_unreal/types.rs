// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{Result, codegen::CodeTableKind};

use super::{
    CodeEnum, CodeField, CodeModel, CodeRelationCardinality, CodeStruct, CodeTable, CodeTypeRef,
    CodeWriterLike, CppUnrealCodegenOptions, CppUnrealSupportLevel,
    repository::render_index_function_declaration, symbols::CppUnrealNames,
};

pub(super) fn render_struct_prefix(
    content: &mut impl CodeWriterLike,
    _type_name: &str,
    options: &CppUnrealCodegenOptions,
    reflected: bool,
) {
    if reflected && matches!(options.support, CppUnrealSupportLevel::Blueprint) {
        content.raw("USTRUCT(BlueprintType)\n");
    }
}

pub(super) fn api_qualified_type_name(
    type_name: &str,
    options: &CppUnrealCodegenOptions,
) -> String {
    if matches!(options.support, CppUnrealSupportLevel::Blueprint)
        && let Some(api_macro) = options.api_macro.as_deref()
    {
        return format!("{api_macro} {type_name}");
    }
    type_name.to_owned()
}

fn render_generated_body(content: &mut impl CodeWriterLike, options: &CppUnrealCodegenOptions) {
    if matches!(options.support, CppUnrealSupportLevel::Blueprint) {
        content.raw("    GENERATED_BODY()\n\n");
    }
}

fn render_blueprint_property(
    content: &mut impl CodeWriterLike,
    field: &CodeField,
    options: &CppUnrealCodegenOptions,
) {
    if matches!(options.support, CppUnrealSupportLevel::Blueprint) && field.required {
        let category = blueprint_category(options);
        content.raw_fmt(format_args!(
            "    UPROPERTY(BlueprintReadOnly, Category={category:?})\n"
        ));
    }
}

pub(super) fn blueprint_category(options: &CppUnrealCodegenOptions) -> &str {
    options
        .blueprint_category
        .as_deref()
        .unwrap_or("ConfigForge")
}

pub(super) fn render_enum_declaration(
    content: &mut impl CodeWriterLike,
    code_enum: &CodeEnum,
    names: &CppUnrealNames<'_>,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    if matches!(options.support, CppUnrealSupportLevel::Blueprint) {
        content.raw("UENUM(BlueprintType)\n");
    }
    let underlying_type = if matches!(options.support, CppUnrealSupportLevel::Blueprint) {
        " : uint8"
    } else {
        ""
    };
    content.raw_fmt(format_args!(
        "enum class {}{underlying_type} {{\n",
        names.enum_type_name(&code_enum.name)?,
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

pub(super) fn render_struct_declaration(
    content: &mut impl CodeWriterLike,
    code_struct: &CodeStruct,
    names: &CppUnrealNames<'_>,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    let type_name = names.struct_type_name(&code_struct.name)?;
    render_struct_prefix(content, &type_name, options, true);
    content.raw_fmt(format_args!(
        "struct {} {{\n",
        api_qualified_type_name(&type_name, options)
    ));
    render_generated_body(content, options);
    for field in &code_struct.fields {
        render_blueprint_property(content, field, options);
        content.raw_fmt(format_args!(
            "    {} {};\n",
            names.cpp_field_type(field)?,
            names.field_name(field)?
        ));
    }
    render_text_helper_declarations(content, &code_struct.name, &code_struct.fields, names)?;
    content.raw_fmt(format_args!(
        "    static TConfigForgeResult<{}> {}(IConfigForgeRuntimeDataReader& Reader);\n",
        type_name,
        names.read_struct_function_name(&code_struct.name)?
    ));
    content.raw("};\n\n");
    Ok(())
}

pub(super) fn render_table_declaration(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &CppUnrealNames<'_>,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    let row_type = names.row_type_name(&table.name)?;
    render_struct_prefix(content, &row_type, options, true);
    content.raw_fmt(format_args!(
        "struct {} {{\n",
        api_qualified_type_name(&row_type, options)
    ));
    render_generated_body(content, options);
    if matches!(table.kind, CodeTableKind::Object) {
        content.raw_fmt(format_args!(
            "    static constexpr const TCHAR* {} = TEXT({:?});\n",
            names.data_file_constant_name()?,
            table.data_file
        ));
    }
    for field in &table.fields {
        render_blueprint_property(content, field, options);
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
        row_type,
        names.read_row_function_name(&table.name)?
    ));
    content.raw("};\n\n");
    if matches!(table.kind, CodeTableKind::Collection) {
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
            content.raw_fmt(format_args!(
                "    void Link(const {}& Data) const;\n",
                names.game_data_access_interface_name()?
            ));
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
                "    const {target_row_type}* {function_name}(const {}& Data) const;\n",
                names.game_data_access_interface_name()?
            )),
            CodeRelationCardinality::OptionalOne => content.raw_fmt(format_args!(
                "    const {target_row_type}* {function_name}(const {}& Data) const;\n",
                names.game_data_access_interface_name()?
            )),
            CodeRelationCardinality::Many => content.raw_fmt(format_args!(
                "    TArray<const {target_row_type}*> {function_name}(const {}& Data) const;\n",
                names.game_data_access_interface_name()?
            )),
        }
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw_fmt(format_args!(
            "    void Link(const {}& Data) const;\n",
            names.game_data_access_interface_name()?
        ));
    }
    Ok(())
}

pub(super) fn render_compound_index_key_structs(
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
