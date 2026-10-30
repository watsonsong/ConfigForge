// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::write_file_header;
use serde::{Deserialize, Serialize};

use crate::diagnostic::code::CF_GENERATOR_INVALID_OPTION;
use crate::{ConfigForgeError, Result, artifact::ArtifactSink};

use super::{
    ArtifactGenerator, CaseStyle, CodeEnum, CodeExportInput, CodeField, CodeGenerator, CodeIndex,
    CodeModel, CodeStruct, CodeTable, CodeTypeRef, CodeWriterConfig, CodeWriterLike,
    GeneratorCommonInput, GeneratorRequirements, NamingConfig, build_code_model,
    ir::{CodeForeignKey, CodeRelationCardinality, require_non_empty_index_fields},
};

mod blueprint;
mod reader;
mod repository;
mod runtime;
mod symbols;
mod types;

use blueprint::{render_blueprint_facade_declaration, render_blueprint_facade_source};
use reader::{render_struct_reader_source, render_table_reader_source};
use repository::{render_row_relation_source, render_table_load_source};
use runtime::{
    render_game_data_access_interface, render_game_data_load_source,
    render_lazy_game_data_declaration, render_lazy_game_data_source, render_runtime_declarations,
};
use symbols::{CppUnrealNames, validate_generated_symbols};
use types::render_compound_index_key_structs;
use types::{
    api_qualified_type_name, render_enum_declaration, render_struct_declaration,
    render_struct_prefix, render_table_declaration,
};

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
    #[serde(default)]
    pub type_name_prefix: Option<String>,
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
        let names = CppUnrealNames::new(&input.target.options, &options);
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
        let names = CppUnrealNames::new(&input.target.options, &options);
        validate_generated_symbols(&model, &names)?;

        let header_file_name = input.target.single_file_name("generated", "h");
        let source_file_name = format!("{}.cpp", input.target.file_stem("generated"));
        let header_path = input.target.out_dir.join(&header_file_name);
        let source_path = input.target.out_dir.join(&source_file_name);

        super::write_code_to_sink(
            sink,
            header_path,
            input.target.options.formatting,
            |header| {
                render_header_file(self, header, input, &model, &options)?;
                Ok(())
            },
        )?;

        super::write_code_to_sink(
            sink,
            source_path,
            input.target.options.formatting,
            |source| {
                render_source_file(self, source, input, &header_file_name, &options)?;
                Ok(())
            },
        )?;
        Ok(())
    }
}

impl ArtifactGenerator for CppUnrealCodeGenerator {
    fn type_id(&self) -> &'static str {
        LANGUAGE
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_only()
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<std::path::PathBuf>> {
        let input = input.code_export_input(self)?;
        cpp_unreal_options(&input)?;
        Ok(vec![
            input
                .target
                .out_dir
                .join(input.target.single_file_name("generated", "h")),
            input
                .target
                .out_dir
                .join(format!("{}.cpp", input.target.file_stem("generated"))),
        ])
    }

    fn generate(
        &self,
        input: &crate::generator::GeneratorTaskInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let input = &input.common;
        super::generate_code_artifact_to_sink(self, input, sink)
    }
}

fn cpp_unreal_options(input: &CodeExportInput<'_>) -> Result<CppUnrealCodegenOptions> {
    let options = input
        .target
        .options
        .language_options::<CppUnrealCodegenOptions>(LANGUAGE)?;
    validate_api_macro(options.api_macro.as_deref())?;
    validate_type_name_prefix(options.type_name_prefix.as_deref())?;
    if matches!(options.support, CppUnrealSupportLevel::Blueprint) && options.api_macro.is_none() {
        return Err(cpp_unreal_option_error(
            format!(
                "failed to parse {LANGUAGE} codegen options: api_macro is required when support = \"blueprint\""
            ),
            "missing cpp_unreal.api_macro",
            "uppercase C++ macro identifier for blueprint support",
        ));
    }
    if matches!(options.support, CppUnrealSupportLevel::Blueprint)
        && (options.namespace.is_some() || input.target.namespace.is_some())
    {
        return Err(cpp_unreal_option_error(
            format!(
                "failed to parse {LANGUAGE} codegen options: namespace is not supported when support = \"blueprint\"; use cpp_unreal.type_name_prefix to avoid global reflected type collisions"
            ),
            "namespace with blueprint support",
            "no namespace for blueprint support",
        ));
    }
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
            return Err(cpp_unreal_option_error(
                format!(
                    "failed to parse {LANGUAGE} codegen options: api_macro `{api_macro}` must be an uppercase C++ macro identifier"
                ),
                api_macro,
                "uppercase C++ macro identifier",
            ));
        }
    }
    Ok(())
}

fn validate_type_name_prefix(type_name_prefix: Option<&str>) -> Result<()> {
    if let Some(type_name_prefix) = type_name_prefix {
        let is_valid = !type_name_prefix.is_empty()
            && type_name_prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            && type_name_prefix
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_');
        if !is_valid {
            return Err(cpp_unreal_option_error(
                format!(
                    "failed to parse {LANGUAGE} codegen options: type_name_prefix `{type_name_prefix}` must be a C++ identifier prefix"
                ),
                type_name_prefix,
                "C++ identifier prefix",
            ));
        }
    }
    Ok(())
}

fn cpp_unreal_option_error(
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    crate::code_artifact::generator_option_error(
        CF_GENERATOR_INVALID_OPTION,
        "generator.cpp_unreal.options",
        message,
        current,
        expected,
        "Check the cpp_unreal export task options.",
    )
}

fn render_header_file(
    generator: &dyn CodeGenerator,
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
    model: &CodeModel,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    let names = CppUnrealNames::new(&input.target.options, options);
    write_file_header(generator, content, input);
    content.raw("#pragma once\n\n");
    content.raw("#include \"CoreMinimal.h\"\n");
    if matches!(options.support, CppUnrealSupportLevel::Blueprint) {
        content.raw("#include \"Kismet/BlueprintFunctionLibrary.h\"\n");
        content.raw_fmt(format_args!(
            "#include {:?}\n",
            format!("{}.generated.h", input.target.file_stem("generated"))
        ));
    }
    content.raw("\n");
    render_runtime_declarations(content);
    render_namespace_open(content, input, options);
    content.raw_fmt(format_args!(
        "class {};\n",
        names.game_data_access_interface_name()?
    ));
    content.raw_fmt(format_args!(
        "struct {};\n\n",
        names.struct_type_name("game_data")?
    ));
    for code_enum in &model.enums {
        render_enum_declaration(content, code_enum, &names, options)?;
    }
    for code_struct in &model.structs {
        render_struct_declaration(content, code_struct, &names, options)?;
    }
    render_compound_index_key_structs(content, model, &names)?;
    for table in &model.tables {
        render_table_declaration(content, table, &names, options)?;
    }
    render_game_data_access_interface(content, model, &names)?;
    render_struct_prefix(
        content,
        &names.struct_type_name("game_data")?,
        options,
        false,
    );
    content.raw_fmt(format_args!(
        "struct {} : public {} {{\n",
        api_qualified_type_name(&names.struct_type_name("game_data")?, options),
        names.game_data_access_interface_name()?
    ));
    content.raw("    static constexpr const TCHAR* Generator = TEXT(\"ConfigForge\");\n");
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "    {} {};\n",
            names.table_data_type_name(table)?,
            names.member_name(&table.name)?
        ));
    }
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "    TConfigForgeResult<const {}*> {}() const override;\n",
            names.table_data_type_name(table)?,
            names.table_accessor_name(&table.name)?
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
    render_blueprint_facade_declaration(content, model, &names, options)?;
    render_namespace_close(content, input, options);
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
    let names = CppUnrealNames::new(&input.target.options, options);
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
    render_blueprint_facade_source(content, &model, &names, options)?;
    render_namespace_close(content, input, options);
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
