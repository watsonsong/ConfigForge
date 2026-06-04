// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{ConfigForgeError, Result, artifact::ArtifactSink};

use super::{
    CaseStyle, CheckedStreamingCodeWriter, CodeExportInput, CodeGenerator, CodeModel,
    CodeWriterConfig, CodeWriterLike, NamingConfig, build_code_model,
    ir::require_non_empty_index_fields,
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
        let names = CppUnrealNames;
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
        let names = CppUnrealNames;
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
    _model: &CodeModel,
    options: &CppUnrealCodegenOptions,
) -> Result<()> {
    write_file_header(generator, content, input);
    content.raw("#pragma once\n\n");
    content.raw("#include \"CoreMinimal.h\"\n\n");
    render_namespace_open(content, input, options);
    content.raw("struct FConfigForgeUnrealGameData {\n");
    content.raw("    static constexpr const TCHAR* Generator = TEXT(\"ConfigForge\");\n");
    content.raw("};\n");
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
    content.raw("// cpp_unreal source skeleton.\n");
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
