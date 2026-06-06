// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{ConfigForgeError, Result, artifact::ArtifactSink};

use super::{
    CaseStyle, CheckedStreamingCodeWriter, CodeExportInput, CodeGenerator, CodeWriterConfig,
    CodeWriterLike, NamingConfig,
};

mod symbols;
mod types;

use types::JavaNames;

#[cfg(test)]
mod tests;

pub const LANGUAGE: &str = "java";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct JavaCodegenOptions {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub root_class_name: Option<String>,
    #[serde(default)]
    pub game_data_class_name: Option<String>,
    #[serde(default)]
    pub lazy_class_name: Option<String>,
    #[serde(default = "default_access_modifier")]
    pub access_modifier: String,
    #[serde(default = "default_emit_final")]
    pub emit_final: bool,
}

impl Default for JavaCodegenOptions {
    fn default() -> Self {
        Self {
            package: None,
            root_class_name: None,
            game_data_class_name: None,
            lazy_class_name: None,
            access_modifier: default_access_modifier(),
            emit_final: default_emit_final(),
        }
    }
}

fn default_access_modifier() -> String {
    "public".to_owned()
}

fn default_emit_final() -> bool {
    true
}

pub(crate) struct JavaCodeGenerator;

impl CodeGenerator for JavaCodeGenerator {
    fn language(&self) -> &'static str {
        LANGUAGE
    }

    fn language_aliases(&self) -> &'static [&'static str] {
        &["jvm"]
    }

    fn default_module_name(&self) -> &'static str {
        "configforge.generated"
    }

    fn default_naming(&self) -> NamingConfig {
        NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::ScreamingSnakeCase,
            field: CaseStyle::CamelCase,
            function: CaseStyle::CamelCase,
            constant: CaseStyle::ScreamingSnakeCase,
        }
    }

    fn default_formatting(&self) -> CodeWriterConfig {
        CodeWriterConfig::default().with_indent_width(4)
    }

    #[cfg(test)]
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<super::GeneratedFile>> {
        let options = input
            .target
            .options
            .language_options::<JavaCodegenOptions>(LANGUAGE)?;
        let output_path = java_output_path(input, &options)?;
        let mut content = super::CodeWriter::new(input.target.options.formatting);
        render_java_code(self, &mut content, input, &options)?;
        Ok(vec![super::GeneratedFile {
            path: output_path,
            content: content.finish(),
        }])
    }

    fn generate_to_sink(&self, input: &CodeExportInput<'_>, sink: &ArtifactSink) -> Result<()> {
        let options = input
            .target
            .options
            .language_options::<JavaCodegenOptions>(LANGUAGE)?;
        let output_path = java_output_path(input, &options)?;
        let stream = sink.create(output_path.clone())?;
        let mut content = CheckedStreamingCodeWriter::new(stream, input.target.options.formatting);
        render_java_code(self, &mut content, input, &options)?;
        let stream = content
            .finish()
            .map_err(|source| ConfigForgeError::WriteFile {
                path: output_path,
                source,
            })?;
        stream.finish().map(|_| ())
    }
}

fn java_output_path(
    input: &CodeExportInput<'_>,
    options: &JavaCodegenOptions,
) -> Result<std::path::PathBuf> {
    let root_class_name = JavaNames::resolve_root_class_name(input, options)?;
    let file_name = match &input.target.file_name {
        Some(file_name) if std::path::Path::new(file_name).extension().is_some() => {
            let stem = std::path::Path::new(file_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(file_name);
            if stem != root_class_name {
                return Err(validation_error(format!(
                    "java codegen: file_name stem `{stem}` must match root_class_name `{root_class_name}`"
                )));
            }
            file_name.clone()
        }
        Some(file_name) => {
            if file_name != &root_class_name {
                return Err(validation_error(format!(
                    "java codegen: file_name `{file_name}` must match root_class_name `{root_class_name}`"
                )));
            }
            format!("{file_name}.java")
        }
        None => format!("{root_class_name}.java"),
    };
    Ok(input.target.out_dir.join(file_name))
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

fn render_java_code(
    generator: &JavaCodeGenerator,
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
    options: &JavaCodegenOptions,
) -> Result<()> {
    let names = JavaNames::new(input, options)?;

    write_file_header(generator, content, input);
    content.newline();
    if !names.package().is_empty() {
        content.raw_fmt(format_args!("package {};\n\n", names.package()));
    }
    content.raw("import java.util.*;\n\n");
    let root_modifier = names.root_access_modifier();
    let final_modifier = if options.emit_final { " final" } else { "" };
    let prefix = if root_modifier.is_empty() {
        format!("{final_modifier}")
    } else {
        format!("{root_modifier}{final_modifier}")
    };
    content.raw_fmt(format_args!(
        "{} class {} {{\n",
        prefix.trim_start(),
        names.root_class_name()
    ));
    content.raw("    private ");
    if options.emit_final {
        content.raw("final ");
    }
    content.raw_fmt(format_args!("{}() {{}}\n\n", names.root_class_name()));
    content.raw(
        "    public static final class ConfigForgeRuntimeException extends RuntimeException {\n",
    );
    content.raw("        public ConfigForgeRuntimeException(String message) { super(message); }\n");
    content.raw("        public ConfigForgeRuntimeException(String message, Throwable cause) { super(message, cause); }\n");
    content.raw("    }\n\n");
    content.raw("}\n");
    Ok(())
}

pub(super) fn validation_error(message: String) -> ConfigForgeError {
    ConfigForgeError::Validation(message)
}
