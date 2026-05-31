// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::Result;

use super::{
    CaseStyle, CodeExportInput, CodeField, CodeFragmentPayload, CodeGenerator, CodeModel,
    CodeTableKind, CodeWriter, CodeWriterConfig, GeneratedFile, I18nFunctionReturn, I18nMode,
    I18nOptions, NamingConfig, build_code_model, ir::require_non_empty_index_fields,
};

mod reader;
mod repository;
mod runtime;
mod symbols;
mod types;

use reader::{
    render_enum_reader_source, render_fragment_reader_source, render_row_reader_source,
    render_struct_reader_source,
};
use repository::{
    render_compound_index_key_structs, render_game_data_load_source, render_lazy_game_data_source,
    render_row_relation_cache_members, render_row_relation_declarations,
    render_row_relation_methods, render_table, render_table_source,
};
use runtime::{render_reader_helpers_source, render_runtime_interfaces};
use symbols::validate_generated_symbols;
use types::CppNames;

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
            "#include <functional>\n",
            "#include <string>\n",
            "#include <tuple>\n",
            "#include <type_traits>\n",
            "#include <unordered_map>\n",
            "#include <utility>\n",
            "#include <vector>\n\n",
        ));
        render_cpp_i18n_include(&mut header, &input.target.options.i18n);
        let namespace = cpp_options
            .namespace
            .as_ref()
            .or(input.target.namespace.as_ref());
        let namespace = namespace.map(String::as_str);
        if let Some(namespace) = namespace {
            header.raw_fmt(format_args!("namespace {namespace} {{\n\n"));
        }
        render_runtime_interfaces(&mut header);
        render_cpp_i18n_prelude(&mut header, &input.target.options.i18n);
        header.raw("class GameDataAccess;\nclass GameData;\n\n");
        if model.tables.iter().any(|table| {
            table
                .foreign_keys
                .iter()
                .any(|foreign_key| foreign_key.linked_field.is_some())
        }) {
            header.raw(concat!(
                "struct LinkCache {\n",
                "    enum class State { Unresolved, Empty, RowIndex };\n",
                "    State state = State::Unresolved;\n",
                "    std::size_t row_index = 0;\n",
                "};\n\n",
                "struct LinkListCache {\n",
                "    bool resolved = false;\n",
                "    std::vector<std::size_t> row_indexes;\n",
                "};\n\n",
            ));
        }

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
            render_cpp_text_helper_declarations(
                &mut header,
                &code_struct.name,
                &code_struct.fields,
                &names,
                &input.target.options.i18n,
            )?;
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
                render_cpp_text_helper_declarations(
                    &mut header,
                    &format!("{}.{}", table.name, fragment.name),
                    &fragment.fields,
                    &names,
                    &input.target.options.i18n,
                )?;
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
            render_cpp_text_helper_declarations(
                &mut header,
                &table.name,
                &table.fields,
                &names,
                &input.target.options.i18n,
            )?;
            render_row_relation_declarations(&mut header, &model, table, &names)?;
            render_row_relation_cache_members(&mut header, table, &names)?;
            header.raw("};\n\n");
            if table.kind == CodeTableKind::Collection {
                render_table(&mut header, table, &names)?;
            }
        }

        header.raw("class GameDataAccess {\npublic:\n");
        header.raw("    virtual ~GameDataAccess() = default;\n");
        for table in &model.tables {
            header.raw_fmt(format_args!(
                "    virtual const {}& {}() const = 0;\n",
                names.table_data_type_name(table)?,
                names.type_name(&table.name)?
            ));
        }
        header.raw("};\n\n");

        header.raw("class GameData : public GameDataAccess {\npublic:\n");
        header.raw("    static GameData Load(IRuntimeDataLoader& loader);\n");
        header.raw("    void LinkAll() const;\n");
        for table in &model.tables {
            header.raw_fmt(format_args!(
                "    const {}& {}() const override {{ return {}; }}\n",
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

        header.raw("\nclass LazyGameData : public GameDataAccess {\npublic:\n");
        header.raw("    explicit LazyGameData(std::shared_ptr<IRuntimeDataLoader> loader);\n");
        header.raw("    void PreloadAll() const;\n");
        header.raw("    void LinkAll() const;\n");
        for table in &model.tables {
            header.raw_fmt(format_args!(
                "    const {}& {}() const override;\n",
                names.table_data_type_name(table)?,
                names.type_name(&table.name)?
            ));
        }
        header.raw("private:\n");
        header.raw("    std::shared_ptr<IRuntimeDataLoader> loader_;\n");
        for table in &model.tables {
            header.raw_fmt(format_args!(
                "    mutable std::optional<{}> {};\n",
                names.table_data_type_name(table)?,
                names.member_name(&table.name)?
            ));
        }
        header.raw("};\n");
        if let Some(namespace) = namespace {
            header.raw_fmt(format_args!("\n}} // namespace {namespace}\n"));
        }

        let header_file_name = input.target.single_file_name("generated", "hpp");
        let source_file_name = format!("{}.cpp", input.target.file_stem("generated"));
        let source = render_source_file(self, input, &model, namespace, &names, &header_file_name)?;

        Ok(vec![
            GeneratedFile {
                path: input.target.out_dir.join(header_file_name),
                content: header.finish(),
            },
            GeneratedFile {
                path: input.target.out_dir.join(source_file_name),
                content: source,
            },
        ])
    }
}

fn render_cpp_text_helper_declarations(
    header: &mut CodeWriter,
    context_prefix: &str,
    fields: &[CodeField],
    names: &CppNames<'_>,
    i18n: &I18nOptions,
) -> Result<()> {
    for field in fields
        .iter()
        .filter(|field| matches!(field.code_type, super::CodeTypeRef::Text))
    {
        let function_name = names.text_function_name(&field.name)?;
        let field_name = names.field_name(&field.name)?;
        let context = format!("{context_prefix}.{}", field.name);
        match i18n.mode {
            I18nMode::Generic if field.required => {
                header.raw_fmt(format_args!(
                    concat!(
                        "    std::string {function_name}(const TextTranslator& translator) const {{\n",
                        "        auto translated = translator({context:?}, {field_name});\n",
                        "        return translated.has_value() ? *translated : {field_name};\n",
                        "    }}\n",
                    ),
                    function_name = function_name,
                    context = context,
                    field_name = field_name,
                ));
            }
            I18nMode::Generic => {
                header.raw_fmt(format_args!(
                    concat!(
                        "    std::optional<std::string> {function_name}(const TextTranslator& translator) const {{\n",
                        "        if (!{field_name}.has_value()) {{ return std::nullopt; }}\n",
                        "        auto translated = translator({context:?}, *{field_name});\n",
                        "        return translated.has_value() ? translated : {field_name};\n",
                        "    }}\n",
                    ),
                    function_name = function_name,
                    context = context,
                    field_name = field_name,
                ));
            }
            I18nMode::GlobalFunction if field.required => {
                header.raw_fmt(format_args!(
                    concat!(
                        "    std::string {function_name}() const {{\n",
                        "        {statement}\n",
                        "    }}\n",
                    ),
                    function_name = function_name,
                    statement = cpp_global_i18n_statement(
                        i18n,
                        &context,
                        &format!("{field_name}.c_str()"),
                        &field_name,
                        false
                    ),
                ));
            }
            I18nMode::GlobalFunction => {
                header.raw_fmt(format_args!(
                    concat!(
                        "    std::optional<std::string> {function_name}() const {{\n",
                        "        if (!{field_name}.has_value()) {{ return std::nullopt; }}\n",
                        "        {statement}\n",
                        "    }}\n",
                    ),
                    function_name = function_name,
                    field_name = field_name,
                    statement = cpp_global_i18n_statement(
                        i18n,
                        &context,
                        &format!("{field_name}->c_str()"),
                        &field_name,
                        true
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn render_cpp_i18n_include(header: &mut CodeWriter, i18n: &I18nOptions) {
    if !matches!(i18n.mode, I18nMode::GlobalFunction) {
        return;
    }
    if let Some(include) = i18n.cpp_include.as_deref() {
        if include.trim_start().starts_with("#include") {
            header.raw(include);
            header.newline();
        } else {
            header.raw_fmt(format_args!("#include {include}\n"));
        }
        header.newline();
    }
}

fn render_cpp_i18n_prelude(header: &mut CodeWriter, i18n: &I18nOptions) {
    if matches!(i18n.mode, I18nMode::Generic) {
        header.raw("using TextTranslator = std::function<std::optional<std::string>(const char*, const std::string&)>;\n\n");
    }
}

fn cpp_global_i18n_statement(
    i18n: &I18nOptions,
    context: &str,
    msgid_expr: &str,
    fallback_expr: &str,
    returns_optional_text: bool,
) -> String {
    let function = i18n.function.as_deref().expect("global i18n function");
    match i18n.returns {
        I18nFunctionReturn::String => {
            format!("return {function}({context:?}, {msgid_expr});")
        }
        I18nFunctionReturn::Optional => {
            if returns_optional_text {
                format!(
                    "auto translated = {function}({context:?}, {msgid_expr}); return translated.has_value() ? translated : {fallback_expr};"
                )
            } else {
                format!(
                    "auto translated = {function}({context:?}, {msgid_expr}); return translated.has_value() ? *translated : {fallback_expr};"
                )
            }
        }
    }
}

fn render_source_file(
    generator: &dyn CodeGenerator,
    input: &CodeExportInput<'_>,
    model: &CodeModel,
    namespace: Option<&str>,
    names: &CppNames<'_>,
    header_file_name: &str,
) -> Result<String> {
    let mut content = CodeWriter::new(input.target.options.formatting);
    generator.write_file_header(&mut content, input);
    content.raw_fmt(format_args!("#include {:?}\n\n", header_file_name));
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
        render_row_relation_methods(&mut content, model, table, names)?;
    }

    for table in &model.tables {
        if table.kind == CodeTableKind::Collection {
            render_table_source(&mut content, table, names)?;
        }
    }
    render_game_data_load_source(&mut content, model, names)?;
    render_lazy_game_data_source(&mut content, model, names)?;

    if let Some(namespace) = namespace {
        content.raw_fmt(format_args!("\n}} // namespace {namespace}\n"));
    }
    Ok(content.finish())
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
