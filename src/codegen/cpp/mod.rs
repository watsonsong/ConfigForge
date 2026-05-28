// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{exporter::BINARY_FORMAT, ConfigForgeError, Result};

use super::{
    build_code_model, ensure_unique_generated_symbols, ir::require_non_empty_index_fields, naming,
    CaseStyle, CodeEnum, CodeExportInput, CodeFragmentPayload, CodeGenerator, CodeIndex, CodeModel,
    CodeStruct, CodeTable, CodeTableKind, CodeTypeRef, CodeWriter, CodeWriterConfig,
    CodegenOptions, GeneratedFile, GeneratedSymbol, NamingConfig,
};

#[cfg(test)]
mod tests;

pub const LANGUAGE: &str = "cpp";

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CppCodegenOptions {
    #[serde(default)]
    pub namespace: Option<String>,
}

pub struct CppCodeGenerator;

impl CodeGenerator for CppCodeGenerator {
    fn language(&self) -> &'static str {
        LANGUAGE
    }

    fn language_aliases(&self) -> &'static [&'static str] {
        &["c++"]
    }

    fn default_naming(&self) -> NamingConfig {
        NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::PascalCase,
            field: CaseStyle::SnakeCase,
            function: CaseStyle::PascalCase,
            constant: CaseStyle::PascalCase,
        }
    }

    fn default_formatting(&self) -> CodeWriterConfig {
        CodeWriterConfig::default().with_indent_width(4)
    }

    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let cpp_options = input
            .target
            .options
            .language_options::<CppCodegenOptions>(LANGUAGE)?;
        let names = CppNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;
        let mut header = CodeWriter::new(input.target.options.formatting);
        self.write_file_header(&mut header, input);
        header.raw("#pragma once\n\n");
        header.raw(concat!(
            "#include <cstdint>\n",
            "#include <cstddef>\n",
            "#include <map>\n",
            "#include <memory>\n",
            "#include <optional>\n",
            "#include <stdexcept>\n",
            "#include <string>\n",
            "#include <tuple>\n",
            "#include <type_traits>\n",
            "#include <unordered_map>\n",
            "#include <utility>\n",
            "#include <vector>\n\n",
        ));
        let namespace = cpp_options
            .namespace
            .as_ref()
            .or(input.target.namespace.as_ref());
        let namespace = namespace.map(String::as_str);
        if let Some(namespace) = namespace {
            header.raw_fmt(format_args!("namespace {namespace} {{\n\n"));
        }
        header.raw(
            "inline constexpr const char* CONFIG_FORGE_READER_CONTRACT_VERSION = \"1.0\";\n\n",
        );
        header.raw(concat!(
            "class IRuntimeDataReader {\n",
            "public:\n",
            "    virtual ~IRuntimeDataReader() = default;\n",
            "    virtual bool EnterField(const std::string& name) = 0;\n",
            "    virtual void EnterIndex(std::size_t index) = 0;\n",
            "    virtual void Exit() = 0;\n",
            "    virtual std::size_t Size() const = 0;\n",
            "    virtual std::int64_t AsInt64() const = 0;\n",
            "    virtual double AsDouble() const = 0;\n",
            "    virtual bool AsBool() const = 0;\n",
            "    virtual std::string AsString() const = 0;\n",
            "    virtual bool IsNull() const = 0;\n",
            "    virtual std::optional<std::uint64_t> SchemaRevision() const { return std::nullopt; }\n",
            "    virtual bool IsPositional() const { return false; }\n",
            "    virtual std::size_t ReadRowCount() { throw std::runtime_error(\"positional runtime reader is not supported\"); }\n",
            "    virtual void BeginRecord(std::size_t) {}\n",
            "    virtual void EndRecord() {}\n",
            "    virtual bool ReadOptionalPresent(std::size_t) {\n",
            "        throw std::runtime_error(\"positional runtime reader is not supported\");\n",
            "    }\n",
            "    virtual std::int64_t ReadNextInt64() { throw std::runtime_error(\"positional runtime reader is not supported\"); }\n",
            "    virtual double ReadNextDouble() { throw std::runtime_error(\"positional runtime reader is not supported\"); }\n",
            "    virtual bool ReadNextBool() { throw std::runtime_error(\"positional runtime reader is not supported\"); }\n",
            "    virtual std::string ReadNextString() { throw std::runtime_error(\"positional runtime reader is not supported\"); }\n",
            "    virtual std::uint64_t ReadEnumOrdinal() { throw std::runtime_error(\"positional runtime reader is not supported\"); }\n",
            "    virtual void EnsureExhausted() { throw std::runtime_error(\"positional runtime reader is not supported\"); }\n",
            "};\n\n",
            "class IRuntimeDataLoader {\n",
            "public:\n",
            "    virtual ~IRuntimeDataLoader() = default;\n",
            "    virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;\n",
            "};\n\n",
        ));

        for code_enum in &model.enums {
            push_doc(&mut header, code_enum.desc.as_deref());
            header.raw_fmt(format_args!(
                "enum class {} {{\n",
                names.type_name(&code_enum.name)?
            ));
            for value in &code_enum.values {
                push_doc_indented(&mut header, value.desc.as_deref(), "    ");
                header.raw_fmt(format_args!(
                    "    {},\n",
                    names.enum_variant_name(&value.name)?
                ));
            }
            header.raw("};\n\n");
        }

        for code_struct in &model.structs {
            push_doc(&mut header, code_struct.desc.as_deref());
            header.raw_fmt(format_args!(
                "struct {} {{\n",
                names.type_name(&code_struct.name)?
            ));
            for field in &code_struct.fields {
                push_doc_indented(&mut header, field.desc.as_deref(), "    ");
                header.raw_fmt(format_args!(
                    "    {} {};\n",
                    names.cpp_field_type(field)?,
                    names.field_name(&field.name)?
                ));
            }
            header.raw("};\n\n");
        }

        render_compound_index_key_structs(&mut header, &model, &names)?;

        for table in &model.tables {
            for fragment in &table.fragments {
                if matches!(fragment.payload, CodeFragmentPayload::Struct { .. }) {
                    continue;
                }
                push_doc(&mut header, fragment.desc.as_deref());
                header.raw_fmt(format_args!(
                    "struct {} {{\n",
                    names.fragment_type_name(&table.name, &fragment.name)?
                ));
                for field in &fragment.fields {
                    push_doc_indented(&mut header, field.desc.as_deref(), "    ");
                    header.raw_fmt(format_args!(
                        "    {} {};\n",
                        names.cpp_field_type(field)?,
                        names.field_name(&field.name)?
                    ));
                }
                header.raw("};\n\n");
            }
        }

        for table in &model.tables {
            push_doc(&mut header, table.desc.as_deref());
            header.raw_fmt(format_args!(
                "struct {} {{\n",
                names.row_type_name(&table.name)?
            ));
            if table.kind == CodeTableKind::Object {
                header.raw_fmt(format_args!(
                    "    static constexpr const char* DataFile = {:?};\n",
                    table.data_file
                ));
                if let Some(revision) = table.schema_revision {
                    header.raw_fmt(format_args!(
                        "    static constexpr std::uint64_t ExpectedSchemaRevision = {revision}ULL;\n"
                    ));
                }
            }
            for field in &table.fields {
                push_doc_indented(&mut header, field.desc.as_deref(), "    ");
                header.raw_fmt(format_args!(
                    "    {} {};\n",
                    names.cpp_field_type(field)?,
                    names.field_name(&field.name)?
                ));
            }
            for fragment in &table.fragments {
                header.raw_fmt(format_args!(
                    "    std::optional<{}> {};\n",
                    names.fragment_payload_type_name(table, fragment)?,
                    names.field_name(&fragment.name)?
                ));
            }
            header.raw("};\n\n");
            if table.kind == CodeTableKind::Collection {
                render_table(&mut header, table, &names)?;
            }
        }

        header.raw("class GameData {\npublic:\n");
        header.raw("    static GameData Load(IRuntimeDataLoader& loader);\n");
        for table in &model.tables {
            header.raw_fmt(format_args!(
                "    const {}& {}() const {{ return {}; }}\n",
                names.table_data_type_name(table)?,
                names.type_name(&table.name)?,
                names.member_name(&table.name)?
            ));
        }
        header.raw("private:\n");
        for table in &model.tables {
            header.raw_fmt(format_args!(
                "    {} {};\n",
                names.table_data_type_name(table)?,
                names.member_name(&table.name)?
            ));
        }
        header.raw("};\n");
        if let Some(namespace) = namespace {
            header.raw_fmt(format_args!("\n}} // namespace {namespace}\n"));
        }

        let source = render_source_file(self, input, &model, namespace, &names)?;

        Ok(vec![
            GeneratedFile {
                path: input.target.out_dir.join("generated.hpp"),
                content: header.finish(),
            },
            GeneratedFile {
                path: input.target.out_dir.join("generated.cpp"),
                content: source,
            },
        ])
    }
}

fn validate_generated_symbols(model: &CodeModel, names: &CppNames<'_>) -> Result<()> {
    let mut type_symbols = vec![
        GeneratedSymbol::new(
            "runtime interface",
            "IRuntimeDataReader",
            "IRuntimeDataReader",
        ),
        GeneratedSymbol::new(
            "runtime interface",
            "IRuntimeDataLoader",
            "IRuntimeDataLoader",
        ),
    ];
    for code_enum in &model.enums {
        type_symbols.push(GeneratedSymbol::new(
            "enum",
            &code_enum.name,
            names.type_name(&code_enum.name)?,
        ));
    }
    for code_struct in &model.structs {
        type_symbols.push(GeneratedSymbol::new(
            "struct",
            &code_struct.name,
            names.type_name(&code_struct.name)?,
        ));
        validate_named_symbols(
            "cpp",
            &format!("struct `{}` fields", code_struct.name),
            code_struct.fields.iter().map(|field| {
                Ok(GeneratedSymbol::new(
                    "field",
                    &field.name,
                    names.field_name(&field.name)?,
                ))
            }),
        )?;
    }
    for table in &model.tables {
        type_symbols.push(GeneratedSymbol::new(
            "table row",
            format!("{}_row", table.name),
            names.row_type_name(&table.name)?,
        ));
        if table.kind == CodeTableKind::Collection {
            type_symbols.push(GeneratedSymbol::new(
                "table repository",
                format!("{}_table", table.name),
                names.repository_type_name(&table.name)?,
            ));
        }
        for index in &table.indexes {
            if index.fields.len() > 1 {
                type_symbols.push(GeneratedSymbol::new(
                    "compound index key",
                    format!("{}_{}_key", table.name, index.name),
                    names.compound_index_key_type_name(table, index)?,
                ));
            }
        }
        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::InlineFields) {
                type_symbols.push(GeneratedSymbol::new(
                    "fragment",
                    format!("{}_{}", table.name, fragment.name),
                    names.fragment_type_name(&table.name, &fragment.name)?,
                ));
                validate_named_symbols(
                    "cpp",
                    &format!("table `{}` fragment `{}` fields", table.name, fragment.name),
                    fragment.fields.iter().map(|field| {
                        Ok(GeneratedSymbol::new(
                            "field",
                            &field.name,
                            names.field_name(&field.name)?,
                        ))
                    }),
                )?;
            }
        }

        validate_named_symbols(
            "cpp",
            &format!("table `{}` row fields", table.name),
            table
                .fields
                .iter()
                .map(|field| {
                    Ok(GeneratedSymbol::new(
                        "field",
                        &field.name,
                        names.field_name(&field.name)?,
                    ))
                })
                .chain(table.fragments.iter().map(|fragment| {
                    Ok(GeneratedSymbol::new(
                        "fragment field",
                        &fragment.name,
                        names.field_name(&fragment.name)?,
                    ))
                })),
        )?;

        if table.kind == CodeTableKind::Object {
            continue;
        }

        let mut member_symbols = vec![GeneratedSymbol::new("field", "rows_", "rows_")];
        for index in &table.indexes {
            member_symbols.push(GeneratedSymbol::new(
                "index storage",
                &index.name,
                names.index_storage_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "cpp",
            &format!("table `{}` private members", table.name),
            member_symbols,
        )?;

        let mut method_symbols = vec![
            GeneratedSymbol::new("method", "Load", "Load"),
            GeneratedSymbol::new("method", "FromRows", "FromRows"),
            GeneratedSymbol::new("method", "Rows", "Rows"),
        ];
        for index in &table.indexes {
            method_symbols.push(GeneratedSymbol::new(
                "index function",
                &index.name,
                names.index_function_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "cpp",
            &format!("table `{}` methods", table.name),
            method_symbols,
        )?;
    }
    type_symbols.push(GeneratedSymbol::new("class", "GameData", "GameData"));
    ensure_unique_generated_symbols("cpp", "top-level types", type_symbols)?;

    validate_named_symbols(
        "cpp",
        "GameData members",
        model.tables.iter().map(|table| {
            Ok(GeneratedSymbol::new(
                "table member",
                &table.name,
                names.member_name(&table.name)?,
            ))
        }),
    )?;

    Ok(())
}

fn validate_named_symbols(
    language: &str,
    scope: &str,
    symbols: impl Iterator<Item = Result<GeneratedSymbol>>,
) -> Result<()> {
    ensure_unique_generated_symbols(language, scope, symbols.collect::<Result<Vec<_>>>()?)
}

fn render_source_file(
    generator: &dyn CodeGenerator,
    input: &CodeExportInput<'_>,
    model: &CodeModel,
    namespace: Option<&str>,
    names: &CppNames<'_>,
) -> Result<String> {
    let mut content = CodeWriter::new(input.target.options.formatting);
    generator.write_file_header(&mut content, input);
    content.raw("#include \"generated.hpp\"\n\n");
    content.raw("#include <stdexcept>\n\n");
    if let Some(namespace) = namespace {
        content.raw_fmt(format_args!("namespace {namespace} {{\n\n"));
    }

    render_reader_helpers_source(&mut content);
    for code_enum in &model.enums {
        render_enum_reader_source(&mut content, code_enum, names)?;
    }
    for code_struct in &model.structs {
        render_struct_reader_source(&mut content, code_struct, names)?;
    }
    for table in &model.tables {
        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::InlineFields) {
                render_fragment_reader_source(&mut content, table, fragment, names)?;
            }
        }
    }
    for table in &model.tables {
        render_row_reader_source(&mut content, table, names)?;
    }
    content.raw("} // namespace\n\n");

    for table in &model.tables {
        if table.kind == CodeTableKind::Collection {
            render_table_source(&mut content, table, names)?;
        }
    }
    render_game_data_load_source(&mut content, model, names)?;

    if let Some(namespace) = namespace {
        content.raw_fmt(format_args!("\n}} // namespace {namespace}\n"));
    }
    Ok(content.finish())
}

fn render_reader_helpers_source(content: &mut CodeWriter) {
    content.raw(concat!(
        "namespace {\n\n",
        "class ReaderScope {\n",
        "public:\n",
        "    explicit ReaderScope(IRuntimeDataReader& reader) : reader_(&reader) {}\n",
        "    ReaderScope(ReaderScope&& other) noexcept : reader_(other.reader_) { other.reader_ = nullptr; }\n",
        "    ReaderScope(const ReaderScope&) = delete;\n",
        "    ReaderScope& operator=(const ReaderScope&) = delete;\n",
        "    ~ReaderScope() noexcept { if (reader_) { try { reader_->Exit(); } catch (...) {} } }\n",
        "private:\n",
        "    IRuntimeDataReader* reader_;\n",
        "};\n\n",
        "ReaderScope EnterRequiredField(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    if (!reader.EnterField(name)) {\n",
        "        throw std::runtime_error(\"missing required field `\" + name + \"`\");\n",
        "    }\n",
        "    ReaderScope scope(reader);\n",
        "    if (reader.IsNull()) {\n",
        "        throw std::runtime_error(\"missing required field `\" + name + \"`\");\n",
        "    }\n",
        "    return scope;\n",
        "}\n\n",
        "template <typename F>\n",
        "auto RequiredField(\n",
        "    IRuntimeDataReader& reader,\n",
        "    const std::string& name,\n",
        "    F convert) -> decltype(convert(std::declval<IRuntimeDataReader&>())) {\n",
        "    auto scope = EnterRequiredField(reader, name);\n",
        "    return convert(reader);\n",
        "}\n\n",
        "template <typename F>\n",
        "auto OptionalField(\n",
        "    IRuntimeDataReader& reader,\n",
        "    const std::string& name,\n",
        "    F convert) -> std::optional<decltype(convert(std::declval<IRuntimeDataReader&>()))> {\n",
        "    if (!reader.EnterField(name)) { return std::nullopt; }\n",
        "    ReaderScope scope(reader);\n",
        "    if (reader.IsNull()) { return std::nullopt; }\n",
        "    return convert(reader);\n",
        "}\n\n",
        "std::int64_t ReadRequiredInt64(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return reader.IsPositional() ? reader.AsInt64() : RequiredField(reader, name, [](IRuntimeDataReader& value) { return value.AsInt64(); });\n",
        "}\n\n",
        "double ReadRequiredDouble(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return reader.IsPositional() ? reader.AsDouble() : RequiredField(reader, name, [](IRuntimeDataReader& value) { return value.AsDouble(); });\n",
        "}\n\n",
        "bool ReadRequiredBool(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return reader.IsPositional() ? reader.AsBool() : RequiredField(reader, name, [](IRuntimeDataReader& value) { return value.AsBool(); });\n",
        "}\n\n",
        "std::string ReadRequiredString(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return reader.IsPositional() ? reader.AsString() : RequiredField(reader, name, [](IRuntimeDataReader& value) { return value.AsString(); });\n",
        "}\n\n",
        "std::optional<std::int64_t> ReadOptionalInt64(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    if (reader.IsPositional()) {\n",
        "        return reader.ReadOptionalPresent(0) ? std::optional<std::int64_t>(reader.AsInt64()) : std::nullopt;\n",
        "    }\n",
        "    return OptionalField(reader, name, [](IRuntimeDataReader& value) { return value.AsInt64(); });\n",
        "}\n\n",
        "std::optional<double> ReadOptionalDouble(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    if (reader.IsPositional()) {\n",
        "        return reader.ReadOptionalPresent(0) ? std::optional<double>(reader.AsDouble()) : std::nullopt;\n",
        "    }\n",
        "    return OptionalField(reader, name, [](IRuntimeDataReader& value) { return value.AsDouble(); });\n",
        "}\n\n",
        "std::optional<bool> ReadOptionalBool(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    if (reader.IsPositional()) {\n",
        "        return reader.ReadOptionalPresent(0) ? std::optional<bool>(reader.AsBool()) : std::nullopt;\n",
        "    }\n",
        "    return OptionalField(reader, name, [](IRuntimeDataReader& value) { return value.AsBool(); });\n",
        "}\n\n",
        "std::optional<std::string> ReadOptionalString(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    if (reader.IsPositional()) {\n",
        "        return reader.ReadOptionalPresent(0) ? std::optional<std::string>(reader.AsString()) : std::nullopt;\n",
        "    }\n",
        "    return OptionalField(reader, name, [](IRuntimeDataReader& value) { return value.AsString(); });\n",
        "}\n\n",
    ));
}

fn render_enum_reader_source(
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
    content.raw_fmt(format_args!(
        "    if (reader.IsPositional()) {{ return {}(reader.ReadEnumOrdinal()); }}\n",
        names.binary_enum_reader_function_name(&type_name)?
    ));
    content.raw("    const std::string value = reader.AsString();\n");
    for value in &code_enum.values {
        content.raw_fmt(format_args!(
            "    if (value == {:?}) {{ return {}::{}; }}\n",
            value.name,
            type_name,
            names.enum_variant_name(&value.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "    throw std::runtime_error(\"unknown {} value `\" + value + \"`\");\n",
        code_enum.name
    ));
    content.raw("}\n\n");
    content.raw_fmt(format_args!(
        "{} {}(std::uint64_t ordinal) {{\n",
        type_name,
        names.binary_enum_reader_function_name(&type_name)?
    ));
    content.raw("    switch (ordinal) {\n");
    for (ordinal, value) in code_enum.values.iter().enumerate() {
        content.raw_fmt(format_args!(
            "    case {ordinal}: return {}::{};\n",
            type_name,
            names.enum_variant_name(&value.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "    default: throw std::runtime_error(\"unknown {} ordinal\");\n",
        code_enum.name
    ));
    content.raw("    }\n");
    content.raw("}\n\n");
    Ok(())
}

fn render_struct_reader_source(
    content: &mut CodeWriter,
    code_struct: &CodeStruct,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_struct.name)?;
    render_reader_function_source(content, &type_name, &code_struct.fields, None, names)
}

fn render_fragment_reader_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    fragment: &super::CodeFragment,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.fragment_payload_type_name(table, fragment)?;
    render_reader_function_source(content, &type_name, &fragment.fields, None, names)
}

fn render_row_reader_source(
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
    fields: &[super::CodeField],
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
                    "    if (reader.IsPositional()) {{\n",
                    "        if (reader.ReadOptionalPresent(0)) {{\n",
                    "            value.{field_name} = {reader_function}(reader);\n",
                    "        }}\n",
                    "    }} else if (reader.EnterField({:?})) {{\n",
                    "        ReaderScope fragment_scope(reader);\n",
                    "        if (!reader.IsNull()) {{\n",
                    "            value.{field_name} = {reader_function}(reader);\n",
                    "        }}\n",
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

fn render_table_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    let repository_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    let row_reader_function = names.reader_function_name(&row_type)?;

    content.raw_fmt(format_args!(
        "{} {}::Load(IRuntimeDataLoader& loader) {{\n",
        repository_type, repository_type
    ));
    content.raw("    auto root = loader.Open(DataFile);\n");
    if table.data_format == BINARY_FORMAT {
        content.raw(concat!(
            "    if (!root->SchemaRevision() || *root->SchemaRevision() != ExpectedSchemaRevision) {\n",
            "        throw std::runtime_error(\"schema revision mismatch for binary runtime data\");\n",
            "    }\n",
        ));
        content.raw("    const std::size_t row_count = root->ReadRowCount();\n");
        content.raw_fmt(format_args!("    std::vector<{}> rows;\n", row_type));
        content.raw("    for (std::size_t row_index = 0; row_index < row_count; ++row_index) {\n");
        content.raw_fmt(format_args!(
            "        rows.push_back({row_reader_function}(*root));\n"
        ));
        content.raw("    }\n");
        content.raw("    root->EnsureExhausted();\n");
        content.raw("    return FromRows(std::move(rows));\n");
        content.raw("}\n\n");
    } else {
        content.raw("    auto rows_scope = EnterRequiredField(*root, \"rows\");\n");
        content.raw_fmt(format_args!("    std::vector<{}> rows;\n", row_type));
        content.raw("    rows.reserve(root->Size());\n");
        content
            .raw("    for (std::size_t row_index = 0; row_index < root->Size(); ++row_index) {\n");
        content.raw("        root->EnterIndex(row_index);\n");
        content.raw("        ReaderScope row_scope(*root);\n");
        content.raw_fmt(format_args!(
            "        rows.push_back({row_reader_function}(*root));\n"
        ));
        content.raw("    }\n");
        content.raw("    return FromRows(std::move(rows));\n");
        content.raw("}\n\n");
    }
    if table.data_format == BINARY_FORMAT {
        // named path has already been emitted in the `else` branch.
    } else {
        return render_from_rows_source(content, table, names, &repository_type, &row_type);
    }
    render_from_rows_source(content, table, names, &repository_type, &row_type)
}

fn render_from_rows_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &CppNames<'_>,
    repository_type: &str,
    row_type: &str,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "{} {}::FromRows(std::vector<{}> rows) {{\n",
        repository_type, repository_type, row_type
    ));
    content.raw_fmt(format_args!("    {} table;\n", repository_type));
    content.raw("    table.rows_ = std::move(rows);\n");
    content.raw(
        "    for (std::size_t row_index = 0; row_index < table.rows_.size(); ++row_index) {\n",
    );
    content.raw("        const auto& row = table.rows_[row_index];\n");
    for index in &table.indexes {
        let storage_name = names.index_storage_name(index)?;
        let key = names.cpp_row_key_expr(table, index, "row")?;
        if index.unique {
            content.raw_fmt(format_args!(
                "        auto inserted_{storage_name} = table.{storage_name}.emplace({key}, row_index);\n"
            ));
            content.raw_fmt(format_args!(
                "        if (!inserted_{storage_name}.second) {{ throw std::runtime_error(\"Duplicate unique index key for {}.{}\"); }}\n",
                repository_type, index.name
            ));
        } else {
            content.raw_fmt(format_args!(
                "        table.{storage_name}[{key}].push_back(row_index);\n"
            ));
        }
    }
    content.raw("    }\n");
    content.raw("    return table;\n");
    content.raw("}\n\n");

    for index in &table.indexes {
        render_index_function_source(content, table, index, names)?;
    }
    Ok(())
}

fn render_index_function_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppNames<'_>,
) -> Result<()> {
    let params = names.cpp_index_params(index)?;
    let repository_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    let function_name = names.index_function_name(index)?;
    let storage_name = names.index_storage_name(index)?;
    let key = names.cpp_lookup_key_expr(table, index)?;

    if index.unique {
        content.raw_fmt(format_args!(
            "const {}* {}::{}({}) const {{\n",
            row_type, repository_type, function_name, params
        ));
        content.raw_fmt(format_args!(
            "    auto iter = {}.find({});\n",
            storage_name, key
        ));
        content.raw_fmt(format_args!(
            "    if (iter == {}.end()) {{ return nullptr; }}\n",
            storage_name
        ));
        content.raw("    return &rows_[iter->second];\n");
        content.raw("}\n\n");
    } else {
        content.raw_fmt(format_args!(
            "std::vector<const {}*> {}::{}({}) const {{\n",
            row_type, repository_type, function_name, params
        ));
        content.raw_fmt(format_args!(
            "    auto iter = {}.find({});\n",
            storage_name, key
        ));
        content.raw_fmt(format_args!(
            "    if (iter == {}.end()) {{ return {{}}; }}\n",
            storage_name
        ));
        content.raw_fmt(format_args!(
            "    std::vector<const {}*> result;\n",
            row_type
        ));
        content.raw("    result.reserve(iter->second.size());\n");
        content.raw("    for (std::size_t row_index : iter->second) {\n");
        content.raw("        result.push_back(&rows_[row_index]);\n");
        content.raw("    }\n");
        content.raw("    return result;\n");
        content.raw("}\n\n");
    }
    Ok(())
}

fn render_game_data_load_source(
    content: &mut CodeWriter,
    model: &CodeModel,
    names: &CppNames<'_>,
) -> Result<()> {
    content.raw("GameData GameData::Load(IRuntimeDataLoader& loader) {\n");
    content.raw("    GameData data;\n");
    for table in &model.tables {
        let member_name = names.member_name(&table.name)?;
        match table.kind {
            CodeTableKind::Collection => content.raw_fmt(format_args!(
                "    data.{member_name} = {}::Load(loader);\n",
                names.repository_type_name(&table.name)?
            )),
            CodeTableKind::Object => {
                if table.data_format == BINARY_FORMAT {
                    let row_type = names.row_type_name(&table.name)?;
                    content.raw_fmt(format_args!(
                        "    auto root_{member_name} = loader.Open({row_type}::DataFile);\n"
                    ));
                    content.raw_fmt(format_args!(
                        concat!(
                            "    if (!root_{member_name}->SchemaRevision() ||\n",
                            "        *root_{member_name}->SchemaRevision() != {row_type}::ExpectedSchemaRevision) {{\n",
                            "        throw std::runtime_error(\"schema revision mismatch for binary runtime data\");\n",
                            "    }}\n",
                        ),
                        member_name = member_name,
                        row_type = row_type,
                    ));
                    content.raw_fmt(format_args!(
                        "    data.{member_name} = {}(*root_{member_name});\n",
                        names.reader_function_name(&row_type)?
                    ));
                    content.raw_fmt(format_args!("    root_{member_name}->EnsureExhausted();\n"));
                } else {
                    content.raw_fmt(format_args!(
                        "    data.{member_name} = {}(*loader.Open({}::DataFile));\n",
                        names.reader_function_name(&names.row_type_name(&table.name)?)?,
                        names.row_type_name(&table.name)?
                    ));
                }
            }
        }
    }
    content.raw("    return data;\n");
    content.raw("}\n");
    Ok(())
}

fn render_compound_index_key_structs(
    content: &mut CodeWriter,
    model: &CodeModel,
    names: &CppNames<'_>,
) -> Result<()> {
    for table in &model.tables {
        for index in &table.indexes {
            if index.fields.len() <= 1 {
                continue;
            }

            content.raw_fmt(format_args!(
                "struct {} {{\n",
                names.compound_index_key_type_name(table, index)?
            ));
            for field in &index.fields {
                content.raw_fmt(format_args!(
                    "    {} {};\n",
                    names.cpp_field_type(field)?,
                    names.field_name(&field.name)?
                ));
            }
            content.newline();
            content.raw_fmt(format_args!(
                "    bool operator<(const {}& other) const {{\n",
                names.compound_index_key_type_name(table, index)?
            ));
            content.raw("        return std::tie(");
            content.raw(
                &index
                    .fields
                    .iter()
                    .map(|field| names.field_name(&field.name))
                    .collect::<Result<Vec<_>>>()?
                    .join(", "),
            );
            content.raw(") < std::tie(");
            content.raw(
                &index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("other.{}", names.field_name(&field.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", "),
            );
            content.raw(");\n");
            content.raw("    }\n");
            content.raw("};\n\n");
        }
    }
    Ok(())
}

fn render_table(content: &mut CodeWriter, table: &CodeTable, names: &CppNames<'_>) -> Result<()> {
    content.raw_fmt(format_args!(
        "class {} {{\npublic:\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    static constexpr const char* DataFile = {:?};\n",
        table.data_file
    ));
    if let Some(revision) = table.schema_revision {
        content.raw_fmt(format_args!(
            "    static constexpr std::uint64_t ExpectedSchemaRevision = {revision}ULL;\n"
        ));
    }
    content.raw_fmt(format_args!(
        "    static {} Load(IRuntimeDataLoader& loader);\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    static {} FromRows(std::vector<{}> rows);\n",
        names.repository_type_name(&table.name)?,
        names.row_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    const std::vector<{}>& Rows() const {{ return rows_; }}\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
    }
    content.raw("private:\n");
    content.raw_fmt(format_args!(
        "    std::vector<{}> rows_;\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        content.raw_fmt(format_args!(
            "    {} {};\n",
            names.cpp_index_storage(table, index)?,
            names.index_storage_name(index)?
        ));
    }
    content.raw("};\n\n");
    Ok(())
}

fn render_index_function(
    content: &mut CodeWriter,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppNames<'_>,
) -> Result<()> {
    let params = names.cpp_index_params(index)?;
    if index.unique {
        content.raw_fmt(format_args!(
            "    const {}* {}({}) const;\n",
            names.row_type_name(&table.name)?,
            names.index_function_name(index)?,
            params
        ));
    } else {
        content.raw_fmt(format_args!(
            "    std::vector<const {}*> {}({}) const;\n",
            names.row_type_name(&table.name)?,
            names.index_function_name(index)?,
            params
        ));
    }
    Ok(())
}

struct CppNames<'a> {
    options: &'a CodegenOptions,
}

impl<'a> CppNames<'a> {
    fn new(options: &'a CodegenOptions) -> Self {
        Self { options }
    }

    fn type_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.type_name)
    }

    fn row_type_name(&self, table_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_row"))
    }

    fn repository_type_name(&self, table_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_table"))
    }

    fn table_data_type_name(&self, table: &CodeTable) -> Result<String> {
        match table.kind {
            CodeTableKind::Object => self.row_type_name(&table.name),
            CodeTableKind::Collection => self.repository_type_name(&table.name),
        }
    }

    fn fragment_type_name(&self, table_name: &str, fragment_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_{fragment_name}_fragment"))
    }

    fn fragment_payload_type_name(
        &self,
        table: &CodeTable,
        fragment: &super::CodeFragment,
    ) -> Result<String> {
        self.fragment_payload_type_name_for_type_name(&table.name, fragment)
    }

    fn fragment_payload_type_name_for_type_name(
        &self,
        table_name: &str,
        fragment: &super::CodeFragment,
    ) -> Result<String> {
        match &fragment.payload {
            CodeFragmentPayload::InlineFields => {
                self.fragment_type_name(table_name, &fragment.name)
            }
            CodeFragmentPayload::Struct { name } => self.type_name(name),
        }
    }

    fn enum_variant_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.enum_value)
    }

    fn field_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.field)
    }

    fn member_name(&self, name: &str) -> Result<String> {
        Ok(format!("{}_", self.field_name(name)?))
    }

    fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        Ok(format!("{}_", self.field_name(&index.name)?))
    }

    fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.options.naming.function)
    }

    fn reader_function_name(&self, type_name: &str) -> Result<String> {
        self.identifier(&format!("read_{type_name}"), self.options.naming.function)
    }

    fn binary_enum_reader_function_name(&self, type_name: &str) -> Result<String> {
        self.identifier(
            &format!("read_{type_name}_binary_ordinal"),
            self.options.naming.function,
        )
    }

    fn cpp_index_params(&self, index: &CodeIndex) -> Result<String> {
        index
            .fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "{} {}",
                    self.cpp_field_type(field)?,
                    self.field_name(&field.name)?
                ))
            })
            .collect::<Result<Vec<_>>>()
            .map(|params| params.join(", "))
    }

    fn cpp_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "std::int64_t".to_owned(),
            CodeTypeRef::Float => "double".to_owned(),
            CodeTypeRef::Bool => "bool".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                "std::string".to_owned()
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
        })
    }

    fn cpp_field_type(&self, field: &super::CodeField) -> Result<String> {
        let base_type = self.cpp_type(&field.code_type)?;
        if field.required {
            Ok(base_type)
        } else {
            Ok(format!("std::optional<{base_type}>"))
        }
    }

    fn cpp_read_field_expr(&self, field: &super::CodeField) -> Result<String> {
        let source_name = format!("{:?}", field.name);
        if field.required {
            return Ok(match &field.code_type {
                CodeTypeRef::Int => format!("ReadRequiredInt64(reader, {source_name})"),
                CodeTypeRef::Float => format!("ReadRequiredDouble(reader, {source_name})"),
                CodeTypeRef::Bool => format!("ReadRequiredBool(reader, {source_name})"),
                CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                    format!("ReadRequiredString(reader, {source_name})")
                }
                CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                    let type_name = self.type_name(name)?;
                    let reader_function = self.reader_function_name(&type_name)?;
                    format!(
                        concat!(
                            "reader.IsPositional()\n",
                            "        ? {reader_function}(reader)\n",
                            "        : RequiredField(reader, {source_name}, [](IRuntimeDataReader& value) {{\n",
                            "            return {reader_function}(value);\n",
                            "        }})",
                        ),
                        reader_function = reader_function,
                        source_name = source_name,
                    )
                }
            });
        }

        Ok(match &field.code_type {
            CodeTypeRef::Int => format!("ReadOptionalInt64(reader, {source_name})"),
            CodeTypeRef::Float => format!("ReadOptionalDouble(reader, {source_name})"),
            CodeTypeRef::Bool => format!("ReadOptionalBool(reader, {source_name})"),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                format!("ReadOptionalString(reader, {source_name})")
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => {
                let type_name = self.type_name(name)?;
                let reader_function = self.reader_function_name(&type_name)?;
                format!(
                    concat!(
                        "reader.IsPositional()\n",
                        "        ? (reader.ReadOptionalPresent(0)\n",
                        "            ? std::optional<{type_name}>({reader_function}(reader))\n",
                        "            : std::nullopt)\n",
                        "        : OptionalField(reader, {source_name}, [](IRuntimeDataReader& value) {{\n",
                        "            return {reader_function}(value);\n",
                        "        }})",
                    ),
                    type_name = type_name,
                    reader_function = reader_function,
                    source_name = source_name,
                )
            }
        })
    }

    fn cpp_index_storage(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        let key = self.cpp_key_type(table, index)?;
        let map_type = if index.fields.len() == 1 {
            "std::unordered_map"
        } else {
            "std::map"
        };
        if index.unique {
            Ok(format!("{map_type}<{key}, std::size_t>"))
        } else {
            Ok(format!("{map_type}<{key}, std::vector<std::size_t>>"))
        }
    }

    fn cpp_key_type(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.cpp_field_type(&index.fields[0])
        } else {
            self.compound_index_key_type_name(table, index)
        }
    }

    fn cpp_row_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
        row_expr: &str,
    ) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(format!(
                "{}.{}",
                row_expr,
                self.field_name(&index.fields[0].name)?
            ))
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("{}.{}", row_expr, self.field_name(&field.name)?))),
            )
        }
    }

    fn cpp_lookup_key_expr(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.field_name(&index.fields[0].name)
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index
                    .fields
                    .iter()
                    .map(|field| self.field_name(&field.name)),
            )
        }
    }

    fn cpp_compound_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
        values: impl Iterator<Item = Result<String>>,
    ) -> Result<String> {
        Ok(format!(
            "{}{{{}}}",
            self.compound_index_key_type_name(table, index)?,
            values.collect::<Result<Vec<_>>>()?.join(", ")
        ))
    }

    fn compound_index_key_type_name(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        self.type_name(&format!("{}_{}_key", table.name, index.name))
    }

    fn identifier(&self, value: &str, style: super::CaseStyle) -> Result<String> {
        naming::make_identifier(
            value,
            style,
            self.options.keyword_conflict,
            naming::is_cpp_keyword,
            |_| None,
        )
        .map_err(|message| ConfigForgeError::Validation(format!("cpp codegen: {message}")))
    }
}

fn push_doc(content: &mut CodeWriter, desc: Option<&str>) {
    push_doc_indented(content, desc, "");
}

fn push_doc_indented(content: &mut CodeWriter, desc: Option<&str>, indent: &str) {
    if let Some(desc) = desc {
        for line in desc.lines() {
            content.raw(indent);
            content.raw("// ");
            content.raw(line);
            content.newline();
        }
    }
}
