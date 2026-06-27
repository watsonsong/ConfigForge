// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{
    ConfigForgeError, Result, artifact::ArtifactSink, diagnostic::code::CF_GENERATOR_INVALID_OPTION,
};

use super::{
    ArtifactGenerator, CaseStyle, CheckedStreamingCodeWriter, CodeExportInput, CodeField,
    CodeFragmentPayload, CodeGenerator, CodeModel, CodeTableKind, CodeWriterConfig, CodeWriterLike,
    GeneratorCommonInput, GeneratorRequirements, I18nFunctionReturn, I18nMode, I18nOptions,
    NamingConfig, RuntimeInterfaceEmit, RuntimeInterfaceMode, build_code_model,
    ir::require_non_empty_index_fields,
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
use runtime::{
    render_reader_helpers_source, render_runtime_interface_include, render_runtime_interfaces,
};
use symbols::validate_generated_symbols;
use types::CppNames;

#[cfg(test)]
mod tests;

pub const LANGUAGE: &str = "cpp";

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CppCodegenOptions {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub runtime_interface: RuntimeInterfaceMode,
    #[serde(default)]
    pub runtime_interface_emit: RuntimeInterfaceEmit,
    #[serde(default)]
    pub runtime_core_header: Option<String>,
    #[serde(default)]
    pub runtime_core_namespace: Option<String>,
    #[serde(default)]
    pub type_mappings: CppTypeMappings,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CppTypeMappings {
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub vector2: Option<String>,
    #[serde(default)]
    pub vector3: Option<String>,
    #[serde(default)]
    pub vector4: Option<String>,
    #[serde(default)]
    pub vector2_int: Option<String>,
    #[serde(default)]
    pub vector3_int: Option<String>,
    #[serde(default)]
    pub vector4_int: Option<String>,
}

pub(crate) struct CppCodeGenerator;

impl CodeGenerator for CppCodeGenerator {
    fn language(&self) -> &'static str {
        LANGUAGE
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

    #[cfg(test)]
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<super::GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let cpp_options = input
            .target
            .options
            .language_options::<CppCodegenOptions>(LANGUAGE)?;
        validate_runtime_interface_options(&cpp_options)?;
        let names = CppNames::new(&input.target.options, &cpp_options);
        validate_generated_symbols(&model, &names)?;
        let mut header = super::CodeWriter::new(input.target.options.formatting);
        let namespace = cpp_options
            .namespace
            .as_ref()
            .or(input.target.namespace.as_ref());
        let namespace = namespace.map(String::as_str);
        let header_file_name = input.target.single_file_name("generated", "hpp");
        let source_file_name = format!("{}.cpp", input.target.file_stem("generated"));
        render_header_file(
            self,
            &mut header,
            input,
            &model,
            namespace,
            &names,
            &cpp_options,
        )?;
        let mut source = super::CodeWriter::new(input.target.options.formatting);
        render_source_file(
            self,
            &mut source,
            input,
            &model,
            namespace,
            &names,
            &header_file_name,
        )?;

        let mut files = vec![
            super::GeneratedFile {
                path: input.target.out_dir.join(header_file_name),
                content: header.finish(),
            },
            super::GeneratedFile {
                path: input.target.out_dir.join(source_file_name),
                content: source.finish(),
            },
        ];
        if matches!(
            cpp_options.runtime_interface_emit,
            RuntimeInterfaceEmit::Reference
        ) {
            files.push(super::GeneratedFile {
                path: runtime_interface_manifest_path(input),
                content: cpp_runtime_interface_manifest(&cpp_options),
            });
        }
        Ok(files)
    }

    fn generate_to_sink(&self, input: &CodeExportInput<'_>, sink: &ArtifactSink) -> Result<()> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let cpp_options = input
            .target
            .options
            .language_options::<CppCodegenOptions>(LANGUAGE)?;
        validate_runtime_interface_options(&cpp_options)?;
        let names = CppNames::new(&input.target.options, &cpp_options);
        validate_generated_symbols(&model, &names)?;
        let namespace = cpp_options
            .namespace
            .as_ref()
            .or(input.target.namespace.as_ref());
        let namespace = namespace.map(String::as_str);
        let header_file_name = input.target.single_file_name("generated", "hpp");
        let source_file_name = format!("{}.cpp", input.target.file_stem("generated"));
        let header_path = input.target.out_dir.join(&header_file_name);
        let source_path = input.target.out_dir.join(source_file_name);

        let header_stream = sink.create(header_path.clone())?;
        let mut header =
            CheckedStreamingCodeWriter::new(header_stream, input.target.options.formatting);
        render_header_file(
            self,
            &mut header,
            input,
            &model,
            namespace,
            &names,
            &cpp_options,
        )?;
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
        render_source_file(
            self,
            &mut source,
            input,
            &model,
            namespace,
            &names,
            &header_file_name,
        )?;
        let source_stream = source
            .finish()
            .map_err(|source| ConfigForgeError::WriteFile {
                path: source_path,
                source,
            })?;
        source_stream.finish()?;

        if matches!(
            cpp_options.runtime_interface_emit,
            RuntimeInterfaceEmit::Reference
        ) {
            let manifest_path = runtime_interface_manifest_path(input);
            let manifest_stream = sink.create(manifest_path.clone())?;
            let mut manifest = CheckedStreamingCodeWriter::new(
                manifest_stream,
                CodeWriterConfig::default().with_indent_width(2),
            );
            manifest.raw(cpp_runtime_interface_manifest(&cpp_options));
            let manifest_stream =
                manifest
                    .finish()
                    .map_err(|source| ConfigForgeError::WriteFile {
                        path: manifest_path,
                        source,
                    })?;
            manifest_stream.finish()?;
        }

        Ok(())
    }
}

impl ArtifactGenerator for CppCodeGenerator {
    fn type_id(&self) -> &'static str {
        LANGUAGE
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["c++"]
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_only()
    }

    fn generate_to_sink(
        &self,
        input: &GeneratorCommonInput<'_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        super::generate_code_artifact_to_sink(self, input, sink)
    }
}

fn validate_runtime_interface_options(options: &CppCodegenOptions) -> Result<()> {
    if matches!(options.runtime_interface, RuntimeInterfaceMode::External) {
        validate_optional_single_line("runtime_core_header", &options.runtime_core_header)?;
        validate_optional_single_line("runtime_core_namespace", &options.runtime_core_namespace)?;
    }
    if matches!(
        options.runtime_interface_emit,
        RuntimeInterfaceEmit::Reference
    ) && !matches!(options.runtime_interface, RuntimeInterfaceMode::External)
    {
        return Err(crate::code_artifact::generator_option_error(
            CF_GENERATOR_INVALID_OPTION,
            "generator.cpp.options",
            "cpp runtime_interface_emit=reference requires runtime_interface=external".to_owned(),
            "runtime_interface_emit=reference",
            "runtime_interface=external",
            "Set cpp.runtime_interface to external when referencing a packaged runtime reader contract.",
        ));
    }
    validate_type_mappings(&options.type_mappings)?;
    Ok(())
}

fn runtime_interface_manifest_path(input: &CodeExportInput<'_>) -> std::path::PathBuf {
    input
        .target
        .out_dir
        .join("config_forge_runtime_interface.json")
}

fn cpp_runtime_interface_manifest(options: &CppCodegenOptions) -> String {
    let runtime_core_header = options
        .runtime_core_header
        .as_deref()
        .unwrap_or("config_forge/runtime/reader.hpp");
    let runtime_core_namespace = options
        .runtime_core_namespace
        .as_deref()
        .unwrap_or("config_forge_runtime");
    format!(
        concat!(
            "{{\n",
            "  \"contract\": \"config_forge_runtime_reader\",\n",
            "  \"contract_version\": \"1.0\",\n",
            "  \"required_contract\": \"^1.0\",\n",
            "  \"language\": \"cpp\",\n",
            "  \"emit\": \"reference\",\n",
            "  \"runtime_interface\": \"external\",\n",
            "  \"runtime_core_header\": {:?},\n",
            "  \"runtime_core_namespace\": {:?}\n",
            "}}\n"
        ),
        runtime_core_header, runtime_core_namespace
    )
}

fn validate_type_mappings(mappings: &CppTypeMappings) -> Result<()> {
    for (key, value) in [
        ("type_mappings.color", &mappings.color),
        ("type_mappings.vector2", &mappings.vector2),
        ("type_mappings.vector3", &mappings.vector3),
        ("type_mappings.vector4", &mappings.vector4),
        ("type_mappings.vector2_int", &mappings.vector2_int),
        ("type_mappings.vector3_int", &mappings.vector3_int),
        ("type_mappings.vector4_int", &mappings.vector4_int),
    ] {
        if let Some(value) = value {
            validate_cpp_type_reference(key, value)?;
        }
    }
    Ok(())
}

fn validate_cpp_type_reference(option: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.contains(';')
        || trimmed.contains('(')
        || trimmed.contains(')')
        || trimmed.contains('#')
        || trimmed.chars().any(char::is_whitespace)
    {
        return Err(validation_error(format!(
            "invalid {option} `{value}`; expected a C++ type reference"
        )));
    }
    Ok(())
}

fn validation_error(message: String) -> ConfigForgeError {
    crate::code_artifact::generator_option_error(
        CF_GENERATOR_INVALID_OPTION,
        "generator.cpp.options",
        message,
        "cpp",
        "valid C++ generator option",
        "Check the cpp generator language options.",
    )
}

fn validate_optional_single_line(kind: &str, value: &Option<String>) -> Result<()> {
    if let Some(value) = value {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            let message =
                format!("invalid C++ {kind} `{value}`; value must be non-empty and single-line");
            return Err(crate::code_artifact::generator_option_error(
                CF_GENERATOR_INVALID_OPTION,
                "generator.cpp.options",
                message.clone(),
                value,
                format!("non-empty single-line C++ {kind}"),
                "Check the cpp external runtime interface options.",
            ));
        }
    }
    Ok(())
}

fn render_header_file(
    generator: &dyn CodeGenerator,
    mut header: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
    model: &CodeModel,
    namespace: Option<&str>,
    names: &CppNames<'_>,
    options: &CppCodegenOptions,
) -> Result<()> {
    write_file_header(generator, header, input);
    header.raw("#pragma once\n\n");
    header.raw(concat!(
        "#include <algorithm>\n",
        "#include <cstdint>\n",
        "#include <cstddef>\n",
        "#include <memory>\n",
        "#include <optional>\n",
        "#include <stdexcept>\n",
        "#include <functional>\n",
        "#include <iterator>\n",
        "#include <string>\n",
        "#include <tuple>\n",
        "#include <type_traits>\n",
        "#include <unordered_map>\n",
        "#include <utility>\n",
        "#include <vector>\n\n",
    ));
    render_runtime_interface_include(&mut header, options);
    render_cpp_i18n_include(&mut header, &input.target.options.i18n);
    if let Some(namespace) = namespace {
        header.raw_fmt(format_args!("namespace {namespace} {{\n\n"));
    }
    render_runtime_interfaces(&mut header, options);
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
            names,
            &input.target.options.i18n,
        )?;
        header.raw("};\n\n");
    }

    render_compound_index_key_structs(&mut header, model, names)?;

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
                names,
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
            names,
            &input.target.options.i18n,
        )?;
        render_row_relation_declarations(&mut header, model, table, names)?;
        render_row_relation_cache_members(&mut header, table, names)?;
        header.raw("};\n\n");
        if table.kind == CodeTableKind::Collection {
            render_table(&mut header, table, names)?;
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
    Ok(())
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

fn render_cpp_text_helper_declarations(
    header: &mut impl CodeWriterLike,
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

fn render_cpp_i18n_include(header: &mut impl CodeWriterLike, i18n: &I18nOptions) {
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

fn render_cpp_i18n_prelude(header: &mut impl CodeWriterLike, i18n: &I18nOptions) {
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
    mut content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
    model: &CodeModel,
    namespace: Option<&str>,
    names: &CppNames<'_>,
    header_file_name: &str,
) -> Result<()> {
    write_file_header(generator, content, input);
    content.raw_fmt(format_args!("#include {:?}\n\n", header_file_name));
    content.raw("#include <stdexcept>\n\n");
    if let Some(namespace) = namespace {
        content.raw_fmt(format_args!("namespace {namespace} {{\n\n"));
    }

    render_reader_helpers_source(&mut content, names.cpp_options());
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
    Ok(())
}

fn push_doc(content: &mut impl CodeWriterLike, desc: Option<&str>) {
    push_doc_indented(content, desc, "");
}

fn push_doc_indented(content: &mut impl CodeWriterLike, desc: Option<&str>, indent: &str) {
    if let Some(desc) = desc {
        for line in desc.lines() {
            content.raw(indent);
            content.raw("// ");
            content.raw(line);
            content.newline();
        }
    }
}
