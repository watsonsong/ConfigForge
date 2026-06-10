// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod ir;
pub(crate) mod naming;
mod writer;

#[cfg(test)]
pub use self::ir::GeneratedFile;
pub(crate) use self::ir::{
    CodeEnum, CodeField, CodeFragment, CodeFragmentPayload, CodeIndex, CodeModel, CodeStruct,
    CodeTable, CodeTableKind, CodeTypeRef, build_code_model,
};
#[cfg(test)]
pub(crate) use writer::CodeWriter;
pub use writer::{
    CheckedStreamingCodeWriter, CodeWriterConfig, CodeWriterLike, IndentStyle,
    PartialCodeWriterConfig, StreamingCodeWriter,
};

use std::{collections::BTreeMap, collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    ArtifactProjectInfo, ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity,
    Result,
    artifact::ArtifactSink,
    diagnostic::code::{
        CF_GENERATOR_INVALID_OPTION, CF_GENERATOR_UNKNOWN_TYPE, CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
    },
    meta::{LinkedMetaModel, TargetTags},
    runtime_contract::RuntimeDataFile,
};

#[derive(Debug, Clone, PartialEq)]
pub struct CodeExportTarget {
    pub language: String,
    pub out_dir: PathBuf,
    pub file_name: Option<String>,
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

impl From<&ArtifactProjectInfo> for CodeExportProject {
    fn from(project: &ArtifactProjectInfo) -> Self {
        Self {
            name: project.name.clone(),
            desc: project.desc.clone(),
            copyright: project.copyright.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeArtifactOptions {
    pub file_header: Option<String>,
    pub keyword_conflict: KeywordConflictPolicy,
    pub i18n: I18nOptions,
    pub naming: NamingConfig,
    pub formatting: CodeWriterConfig,
    pub language_options: HashMap<String, toml::Value>,
}

pub type CodegenOptions = CodeArtifactOptions;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct I18nOptions {
    #[serde(default)]
    pub mode: I18nMode,
    #[serde(default)]
    pub function: Option<String>,
    #[serde(default)]
    pub returns: I18nFunctionReturn,
    #[serde(default)]
    pub cpp_include: Option<String>,
    #[serde(default)]
    pub csharp_using: Option<String>,
    #[serde(default)]
    pub typescript_import: Option<String>,
    #[serde(default)]
    pub java_import: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum I18nMode {
    #[default]
    Generic,
    GlobalFunction,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum I18nFunctionReturn {
    #[default]
    String,
    Optional,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInterfaceMode {
    #[default]
    Inline,
    External,
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
    pub file_name: Option<String>,
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
    pub i18n: I18nOptions,
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
            file_name: None,
            data: None,
            module_name: None,
            namespace: None,
            file_header: None,
            keyword_conflict: None,
            i18n: I18nOptions::default(),
            naming: PartialNamingConfig::default(),
            formatting: PartialCodeWriterConfig::default(),
            language_options: HashMap::new(),
        }
    }

    pub(crate) fn from_export_task_parts(
        language: String,
        output: PathBuf,
        mut options: BTreeMap<String, toml::Value>,
    ) -> Result<Self> {
        Ok(Self {
            language,
            output,
            file_name: take_string(&mut options, "file_name")?,
            data: take_string(&mut options, "data")?,
            module_name: take_string(&mut options, "module_name")?,
            namespace: take_string(&mut options, "namespace")?,
            file_header: take_string(&mut options, "file_header")?,
            keyword_conflict: take_optional_value(&mut options, "keyword_conflict")?,
            i18n: take_value(&mut options, "i18n")?.unwrap_or_default(),
            naming: take_value(&mut options, "naming")?.unwrap_or_default(),
            formatting: take_value(&mut options, "formatting")?.unwrap_or_default(),
            language_options: options.into_iter().collect(),
        })
    }
}

fn take_string(options: &mut BTreeMap<String, toml::Value>, key: &str) -> Result<Option<String>> {
    take_optional_value(options, key)
}

fn take_optional_value<T>(
    options: &mut BTreeMap<String, toml::Value>,
    key: &str,
) -> Result<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    take_value(options, key)
}

fn take_value<T>(options: &mut BTreeMap<String, toml::Value>, key: &str) -> Result<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    options
        .remove(key)
        .map(|value| {
            let current_value = format!("{key}={value}");
            value.try_into().map_err(|source| {
                generator_option_error(
                    CF_GENERATOR_INVALID_OPTION,
                    "generator.codegen",
                    format!("invalid export task option `{key}`: {source}"),
                    current_value,
                    format!("valid `{key}` option value"),
                    "Check the export task option type and supported values.",
                )
            })
        })
        .transpose()
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
    pub(crate) fn from_task(
        task: CodegenTaskConfig,
        generator: &dyn CodeGenerator,
    ) -> Result<Self> {
        let language = generator.language().to_owned();
        let file_name = match task.file_name {
            Some(file_name) => Some(validate_generated_file_name(&file_name)?),
            None => None,
        };
        Ok(Self {
            language,
            out_dir: task.output,
            file_name,
            module_name: task
                .module_name
                .unwrap_or_else(|| generator.default_module_name().to_owned()),
            namespace: task.namespace,
            options: CodegenOptions {
                file_header: task.file_header,
                keyword_conflict: task
                    .keyword_conflict
                    .unwrap_or(KeywordConflictPolicy::SuffixUnderscore),
                i18n: validate_i18n_options(task.i18n)?,
                naming: generator.default_naming().merge(task.naming),
                formatting: generator.default_formatting().merge(task.formatting),
                language_options: task.language_options,
            },
        })
    }
}

fn validate_i18n_options(options: I18nOptions) -> Result<I18nOptions> {
    fn validate_optional_code_snippet(kind: &str, value: &Option<String>) -> Result<()> {
        if let Some(value) = value {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(generator_option_error(
                    CF_GENERATOR_INVALID_OPTION,
                    "generator.codegen",
                    format!(
                        "invalid i18n {kind} `{value}`; value must be non-empty and single-line"
                    ),
                    value,
                    "non-empty single-line code snippet",
                    "Remove control characters and provide a valid i18n code snippet.",
                ));
            }
        }
        Ok(())
    }

    match options.mode {
        I18nMode::Generic => Ok(options),
        I18nMode::GlobalFunction => {
            validate_optional_code_snippet("function", &options.function)?;
            validate_optional_code_snippet("cpp_include", &options.cpp_include)?;
            validate_optional_code_snippet("csharp_using", &options.csharp_using)?;
            validate_optional_code_snippet("typescript_import", &options.typescript_import)?;
            validate_optional_code_snippet("java_import", &options.java_import)?;
            if options.function.is_none() {
                return Err(generator_option_error(
                    CF_GENERATOR_INVALID_OPTION,
                    "generator.codegen",
                    "i18n mode `global_function` requires `function`".to_owned(),
                    "missing i18n.function",
                    "global function name",
                    "Set `i18n.function` when using global_function mode.",
                ));
            }
            Ok(options)
        }
    }
}

impl CodeExportTarget {
    pub(crate) fn single_file_name(&self, default_stem: &str, extension: &str) -> String {
        match &self.file_name {
            Some(file_name) if std::path::Path::new(file_name).extension().is_some() => {
                file_name.clone()
            }
            Some(file_name) => format!("{file_name}.{extension}"),
            None => format!("{default_stem}.{extension}"),
        }
    }

    pub(crate) fn file_stem(&self, default_stem: &str) -> String {
        match &self.file_name {
            Some(file_name) => std::path::Path::new(file_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(file_name)
                .to_owned(),
            None => default_stem.to_owned(),
        }
    }
}

fn validate_generated_file_name(file_name: &str) -> Result<String> {
    let is_valid = !file_name.is_empty()
        && file_name != "."
        && file_name != ".."
        && !file_name.contains('/')
        && !file_name.contains('\\')
        && std::path::Path::new(file_name)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(file_name);
    if is_valid {
        Ok(file_name.to_owned())
    } else {
        Err(generator_option_error(
            CF_GENERATOR_INVALID_OPTION,
            "generator.codegen",
            format!(
                "invalid generated code file name `{file_name}`; file_name must be a single file name without path separators"
            ),
            file_name,
            "single file name without path separators",
            "Use a file name such as `generated.rs`, not a nested or absolute path.",
        ))
    }
}

impl CodeArtifactOptions {
    pub fn for_language(language: &str) -> Result<Self> {
        let (canonical, naming, formatting) = language_defaults(language).ok_or_else(|| {
            generator_option_error(
                CF_GENERATOR_UNKNOWN_TYPE,
                "generator.codegen",
                format!("no code generator registered for `{language}`"),
                language,
                "registered code generator language",
                "Use a supported code generator language such as rust, cpp, cpp_unreal, csharp, java, or typescript.",
            )
        })?;
        Ok(Self {
            file_header: None,
            keyword_conflict: KeywordConflictPolicy::SuffixUnderscore,
            i18n: I18nOptions::default(),
            naming,
            formatting,
            language_options: HashMap::from([(
                canonical.to_owned(),
                toml::Value::Table(Default::default()),
            )]),
        })
    }

    pub fn language_options<T>(&self, language: &str) -> Result<T>
    where
        T: DeserializeOwned + Default,
    {
        match self.language_options.get(language) {
            Some(options) => options.clone().try_into().map_err(|source| {
                generator_option_error(
                    CF_GENERATOR_INVALID_OPTION,
                    format!("generator.{language}"),
                    format!("failed to parse {language} codegen options: {source}"),
                    options.to_string(),
                    format!("valid {language} codegen options"),
                    "Check the language-specific codegen option names, types, and values.",
                )
            }),
            None => Ok(T::default()),
        }
    }
}

fn generator_option_error(
    code: &'static str,
    source: impl Into<String>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            code,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(source)
        .with_domain("generator")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
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

impl Default for CodeArtifactOptions {
    fn default() -> Self {
        Self {
            file_header: None,
            keyword_conflict: KeywordConflictPolicy::SuffixUnderscore,
            i18n: I18nOptions::default(),
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

impl Default for I18nOptions {
    fn default() -> Self {
        Self {
            mode: I18nMode::Generic,
            function: None,
            returns: I18nFunctionReturn::String,
            cpp_include: None,
            csharp_using: None,
            typescript_import: None,
            java_import: None,
        }
    }
}

pub fn default_module_name(language: &str) -> Option<&'static str> {
    language_defaults(language).map(|(canonical, _, _)| match canonical {
        "typescript" => "configForgeGenerated",
        "csharp" => "ConfigForge.Generated",
        "java" => "configforge.generated",
        _ => "config_forge_generated",
    })
}

fn language_defaults(language: &str) -> Option<(&'static str, NamingConfig, CodeWriterConfig)> {
    let canonical = match language {
        "rust" => "rust",
        "cpp" | "c++" => "cpp",
        "cpp_unreal" => "cpp_unreal",
        "typescript" | "ts" => "typescript",
        "csharp" | "cs" => "csharp",
        "java" | "jvm" => "java",
        _ => return None,
    };
    let naming = match canonical {
        "rust" => NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::PascalCase,
            field: CaseStyle::SnakeCase,
            function: CaseStyle::SnakeCase,
            constant: CaseStyle::ScreamingSnakeCase,
        },
        "cpp" => NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::PascalCase,
            field: CaseStyle::SnakeCase,
            function: CaseStyle::PascalCase,
            constant: CaseStyle::PascalCase,
        },
        "cpp_unreal" | "csharp" => NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::PascalCase,
            field: CaseStyle::PascalCase,
            function: CaseStyle::PascalCase,
            constant: CaseStyle::PascalCase,
        },
        "typescript" => NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::Preserve,
            field: CaseStyle::CamelCase,
            function: CaseStyle::CamelCase,
            constant: CaseStyle::CamelCase,
        },
        "java" => NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::ScreamingSnakeCase,
            field: CaseStyle::CamelCase,
            function: CaseStyle::CamelCase,
            constant: CaseStyle::ScreamingSnakeCase,
        },
        _ => unreachable!(),
    };
    let formatting = CodeWriterConfig::default().with_indent_width(match canonical {
        "typescript" => 2,
        _ => 4,
    });
    Some((canonical, naming, formatting))
}

pub struct CodeExportInput<'meta> {
    pub meta: &'meta LinkedMetaModel<'meta>,
    pub target_tags: TargetTags,
    pub project: CodeExportProject,
    pub project_root: PathBuf,
    pub target_name: String,
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
pub(crate) trait CodeGenerator: Send + Sync {
    fn language(&self) -> &'static str;
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
    #[cfg(test)]
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
    #[cfg(test)]
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>>;

    fn generate_to_sink(&self, input: &CodeExportInput<'_>, sink: &ArtifactSink) -> Result<()>;
}

#[cfg(test)]
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
            return Err(generator_option_error(
                CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
                format!("generator.{language}.naming"),
                format!(
                    "{language} codegen: duplicate generated identifier `{}` in {scope}; {} `{}` and {} `{}` both map to it",
                    generated, previous.kind, previous.original, symbol.kind, symbol.original
                ),
                generated,
                format!("unique generated identifier in {scope}"),
                "Adjust source names or naming options so generated identifiers are unique.",
            ));
        }
        seen.insert(generated, symbol);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
