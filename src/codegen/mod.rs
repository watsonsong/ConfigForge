// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod cpp;
mod ir;
mod naming;
mod rust;
mod typescript;
mod writer;

pub use self::ir::GeneratedFile;
pub(crate) use self::ir::{
    CodeEnum, CodeField, CodeFragment, CodeFragmentPayload, CodeIndex, CodeModel, CodeStruct,
    CodeTable, CodeTableKind, CodeTypeRef, build_code_model,
};
pub use cpp::{CppCodeGenerator, CppCodegenOptions};
pub use rust::{RustCodeGenerator, RustCodegenOptions};
pub use typescript::{TypeScriptCodeGenerator, TypeScriptCodegenOptions};
pub use writer::{CodeWriter, CodeWriterConfig, IndentStyle, PartialCodeWriterConfig};

use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, de::DeserializeOwned};

use crate::{
    ConfigForgeError, Result,
    exporter::RuntimeDataFile,
    meta::{LinkedMetaModel, TargetTags},
};

#[derive(Debug, Clone, PartialEq)]
pub struct CodeExportTarget {
    pub language: String,
    pub out_dir: PathBuf,
    pub module_name: String,
    pub namespace: Option<String>,
    pub options: CodegenOptions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeExportProject {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub copyright: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodegenOptions {
    pub file_header: Option<String>,
    pub keyword_conflict: KeywordConflictPolicy,
    pub naming: NamingConfig,
    pub formatting: CodeWriterConfig,
    pub language_options: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeywordConflictPolicy {
    SuffixUnderscore,
    RawIdentifier,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingConfig {
    pub type_name: CaseStyle,
    pub enum_value: CaseStyle,
    pub field: CaseStyle,
    pub function: CaseStyle,
    pub constant: CaseStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStyle {
    Preserve,
    PascalCase,
    CamelCase,
    SnakeCase,
    ScreamingSnakeCase,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CodegenTaskConfig {
    pub language: String,
    pub output: PathBuf,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub module_name: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub file_header: Option<String>,
    #[serde(default)]
    pub keyword_conflict: Option<KeywordConflictPolicy>,
    #[serde(default)]
    pub naming: PartialNamingConfig,
    #[serde(default)]
    pub formatting: PartialCodeWriterConfig,
    #[serde(flatten)]
    pub language_options: HashMap<String, toml::Value>,
}

impl CodegenTaskConfig {
    pub fn for_language(language: &str) -> Self {
        Self {
            language: language.to_owned(),
            output: PathBuf::new(),
            data: None,
            module_name: None,
            namespace: None,
            file_header: None,
            keyword_conflict: None,
            naming: PartialNamingConfig::default(),
            formatting: PartialCodeWriterConfig::default(),
            language_options: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct PartialNamingConfig {
    #[serde(rename = "type", default)]
    pub type_name: Option<CaseStyle>,
    #[serde(default)]
    pub enum_value: Option<CaseStyle>,
    #[serde(default)]
    pub field: Option<CaseStyle>,
    #[serde(default)]
    pub function: Option<CaseStyle>,
    #[serde(default)]
    pub constant: Option<CaseStyle>,
}

impl CodeExportTarget {
    pub fn from_task(task: CodegenTaskConfig, generator: &dyn CodeGenerator) -> Result<Self> {
        let language = generator.language().to_owned();
        Ok(Self {
            language,
            out_dir: task.output,
            module_name: task
                .module_name
                .unwrap_or_else(|| generator.default_module_name().to_owned()),
            namespace: task.namespace,
            options: CodegenOptions {
                file_header: task.file_header,
                keyword_conflict: task
                    .keyword_conflict
                    .unwrap_or(KeywordConflictPolicy::SuffixUnderscore),
                naming: generator.default_naming().merge(task.naming),
                formatting: generator.default_formatting().merge(task.formatting),
                language_options: task.language_options,
            },
        })
    }
}

impl CodegenOptions {
    pub fn for_generator(generator: &dyn CodeGenerator) -> Result<Self> {
        let task = CodegenTaskConfig::for_language(generator.language());
        Ok(Self {
            file_header: None,
            keyword_conflict: KeywordConflictPolicy::SuffixUnderscore,
            naming: generator.default_naming(),
            formatting: generator.default_formatting(),
            language_options: task.language_options,
        })
    }

    pub fn for_language(language: &str) -> Result<Self> {
        let generators = builtin_generators();
        let generator = find_generator(&generators, language).ok_or_else(|| {
            ConfigForgeError::Validation(format!("no code generator registered for `{language}`"))
        })?;
        Self::for_generator(generator)
    }

    pub fn language_options<T>(&self, language: &str) -> Result<T>
    where
        T: DeserializeOwned + Default,
    {
        match self.language_options.get(language) {
            Some(options) => options.clone().try_into().map_err(|source| {
                ConfigForgeError::Validation(format!(
                    "failed to parse {language} codegen options: {source}"
                ))
            }),
            None => Ok(T::default()),
        }
    }
}

impl NamingConfig {
    fn merge(self, partial: PartialNamingConfig) -> Self {
        Self {
            type_name: partial.type_name.unwrap_or(self.type_name),
            enum_value: partial.enum_value.unwrap_or(self.enum_value),
            field: partial.field.unwrap_or(self.field),
            function: partial.function.unwrap_or(self.function),
            constant: partial.constant.unwrap_or(self.constant),
        }
    }
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            file_header: None,
            keyword_conflict: KeywordConflictPolicy::SuffixUnderscore,
            naming: NamingConfig {
                type_name: CaseStyle::Preserve,
                enum_value: CaseStyle::Preserve,
                field: CaseStyle::Preserve,
                function: CaseStyle::Preserve,
                constant: CaseStyle::Preserve,
            },
            formatting: CodeWriterConfig::default(),
            language_options: HashMap::new(),
        }
    }
}

pub fn default_module_name(language: &str) -> Option<&'static str> {
    let generators = builtin_generators();
    find_generator(&generators, language).map(|generator| generator.default_module_name())
}

pub struct CodeExportInput<'meta> {
    pub meta: &'meta LinkedMetaModel<'meta>,
    pub target_tags: TargetTags,
    pub project: CodeExportProject,
    pub target: CodeExportTarget,
    pub runtime_data_files: Vec<RuntimeDataFile>,
    pub generated_at: String,
}

/// Target-language code generator.
///
/// Generators may be shared across worker threads and `generate` may be called
/// concurrently for different codegen tasks. Implementations must therefore be
/// thread-safe and reentrant; plugins with mutable caches must synchronize them
/// internally or use a future factory-style registry to create per-task instances.
pub trait CodeGenerator: Send + Sync {
    fn language(&self) -> &'static str;
    fn language_aliases(&self) -> &'static [&'static str] {
        &[]
    }
    fn supports_language(&self, language: &str) -> bool {
        self.language() == language || self.language_aliases().contains(&language)
    }
    fn default_module_name(&self) -> &'static str {
        "config_forge_generated"
    }
    fn default_naming(&self) -> NamingConfig;
    fn default_formatting(&self) -> CodeWriterConfig {
        CodeWriterConfig::default()
    }
    fn line_comment_prefix(&self) -> &'static str {
        "//"
    }
    fn write_file_header(&self, writer: &mut CodeWriter, input: &CodeExportInput<'_>) {
        if let Some(template) = &input.target.options.file_header {
            let mut output = render_file_header_template(input, template);
            if !output.ends_with('\n') {
                output.push('\n');
            }
            writer.raw(output);
            return;
        }

        let prefix = self.line_comment_prefix();
        writer.line(format!(
            "{prefix} Copyright (c) 2026 ConfigForge contributors"
        ));
        writer.line(format!("{prefix} SPDX-License-Identifier: Apache-2.0"));
        writer.blank_line();
        writer.line(format!("{prefix} Generated by config-forge. Do not edit."));
    }
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>>;
}

fn render_file_header_template(input: &CodeExportInput<'_>, template: &str) -> String {
    let project = &input.project;
    let mut output = template.to_owned();
    output = output.replace("{{project.name}}", project.name.as_deref().unwrap_or(""));
    output = output.replace("{{project.desc}}", project.desc.as_deref().unwrap_or(""));
    output = output.replace(
        "{{project.copyright}}",
        project.copyright.as_deref().unwrap_or(""),
    );
    output.replace("{{datetime}}", &input.generated_at)
}

pub(crate) struct GeneratedSymbol {
    pub kind: &'static str,
    pub original: String,
    pub generated: String,
}

impl GeneratedSymbol {
    pub(crate) fn new(
        kind: &'static str,
        original: impl Into<String>,
        generated: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            original: original.into(),
            generated: generated.into(),
        }
    }
}

pub(crate) fn ensure_unique_generated_symbols(
    language: &str,
    scope: &str,
    symbols: Vec<GeneratedSymbol>,
) -> Result<()> {
    let mut seen: HashMap<String, GeneratedSymbol> = HashMap::new();
    for symbol in symbols {
        let generated = symbol.generated.clone();
        if let Some(previous) = seen.get(&generated) {
            return Err(ConfigForgeError::Validation(format!(
                "{language} codegen: duplicate generated identifier `{}` in {scope}; {} `{}` and {} `{}` both map to it",
                generated, previous.kind, previous.original, symbol.kind, symbol.original
            )));
        }
        seen.insert(generated, symbol);
    }
    Ok(())
}

pub fn builtin_generators() -> Vec<Box<dyn CodeGenerator>> {
    vec![
        Box::new(RustCodeGenerator),
        Box::new(CppCodeGenerator),
        Box::new(TypeScriptCodeGenerator),
    ]
}

pub fn find_generator<'a>(
    generators: &'a [Box<dyn CodeGenerator>],
    language: &str,
) -> Option<&'a dyn CodeGenerator> {
    generators
        .iter()
        .map(Box::as_ref)
        .find(|generator| generator.supports_language(language))
}

#[cfg(test)]
pub(in crate::codegen) mod tests;
