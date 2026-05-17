mod cpp;
mod ir;
mod naming;
mod rust;
mod typescript;

pub use self::cpp::CppCodeGenerator;
pub use self::ir::{
    build_code_model, CodeEnum, CodeEnumValue, CodeField, CodeIndex, CodeModel, CodeStruct,
    CodeTable, CodeTypeRef, GeneratedFile,
};
pub use self::rust::RustCodeGenerator;
pub use self::typescript::TypeScriptCodeGenerator;

use std::path::PathBuf;

use serde::Deserialize;

use crate::{meta::LinkedMetaModel, ConfigForgeError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeExportLanguage {
    Rust,
    Cpp,
    TypeScript,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeExportTarget {
    pub language: CodeExportLanguage,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenOptions {
    pub file_header: Option<String>,
    pub keyword_conflict: KeywordConflictPolicy,
    pub naming: NamingConfig,
    pub language: LanguageCodegenOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanguageCodegenOptions {
    Rust(RustCodegenOptions),
    Cpp(CppCodegenOptions),
    TypeScript(TypeScriptCodegenOptions),
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct RustCodegenOptions {
    #[serde(default)]
    pub derive_serde: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct CppCodegenOptions {
    #[serde(default)]
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct TypeScriptCodegenOptions {
    #[serde(default)]
    pub readonly_fields: bool,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProjectCodegenConfig {
    pub targets: Vec<ProjectTargetConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProjectTargetConfig {
    pub name: String,
    #[serde(default)]
    pub code: Vec<CodegenTaskConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodegenTaskConfig {
    pub language: CodeExportLanguage,
    pub output: PathBuf,
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
    pub rust: RustCodegenOptions,
    #[serde(default)]
    pub cpp: CppCodegenOptions,
    #[serde(default)]
    pub typescript: TypeScriptCodegenOptions,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeDataFormat {
    Json,
    MessagePack,
}

impl RuntimeDataFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::MessagePack => "msgpack",
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::MessagePack => "messagepack",
        }
    }

    pub fn rust_variant(self) -> &'static str {
        match self {
            Self::Json => "Json",
            Self::MessagePack => "MessagePack",
        }
    }
}

impl<'de> Deserialize<'de> for CodeExportLanguage {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| {
            serde::de::Error::custom(format!("unsupported codegen language `{value}`"))
        })
    }
}

impl CodeExportLanguage {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rust" => Some(Self::Rust),
            "cpp" | "c++" => Some(Self::Cpp),
            "typescript" | "ts" => Some(Self::TypeScript),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Cpp => "cpp",
            Self::TypeScript => "typescript",
        }
    }
}

impl CodeExportTarget {
    pub fn from_task(task: CodegenTaskConfig) -> Self {
        let language = task.language;
        Self {
            language,
            out_dir: task.output,
            module_name: task
                .module_name
                .unwrap_or_else(|| default_module_name(language).to_owned()),
            namespace: task.namespace,
            options: CodegenOptions {
                file_header: task.file_header,
                keyword_conflict: task
                    .keyword_conflict
                    .unwrap_or(KeywordConflictPolicy::SuffixUnderscore),
                naming: NamingConfig::for_language(language).merge(task.naming),
                language: LanguageCodegenOptions::from_task(
                    language,
                    task.rust,
                    task.cpp,
                    task.typescript,
                ),
            },
        }
    }
}

impl LanguageCodegenOptions {
    fn from_task(
        language: CodeExportLanguage,
        rust: RustCodegenOptions,
        cpp: CppCodegenOptions,
        typescript: TypeScriptCodegenOptions,
    ) -> Self {
        match language {
            CodeExportLanguage::Rust => Self::Rust(rust),
            CodeExportLanguage::Cpp => Self::Cpp(cpp),
            CodeExportLanguage::TypeScript => Self::TypeScript(typescript),
        }
    }
}

impl CodegenOptions {
    pub fn for_language(language: CodeExportLanguage) -> Self {
        Self {
            file_header: None,
            keyword_conflict: KeywordConflictPolicy::SuffixUnderscore,
            naming: NamingConfig::for_language(language),
            language: match language {
                CodeExportLanguage::Rust => {
                    LanguageCodegenOptions::Rust(RustCodegenOptions::default())
                }
                CodeExportLanguage::Cpp => {
                    LanguageCodegenOptions::Cpp(CppCodegenOptions::default())
                }
                CodeExportLanguage::TypeScript => {
                    LanguageCodegenOptions::TypeScript(TypeScriptCodegenOptions::default())
                }
            },
        }
    }

    pub fn rust(&self) -> Result<&RustCodegenOptions> {
        match &self.language {
            LanguageCodegenOptions::Rust(options) => Ok(options),
            _ => Err(ConfigForgeError::Validation(
                "rust codegen received non-rust language options".to_owned(),
            )),
        }
    }

    pub fn cpp(&self) -> Result<&CppCodegenOptions> {
        match &self.language {
            LanguageCodegenOptions::Cpp(options) => Ok(options),
            _ => Err(ConfigForgeError::Validation(
                "cpp codegen received non-cpp language options".to_owned(),
            )),
        }
    }

    pub fn typescript(&self) -> Result<&TypeScriptCodegenOptions> {
        match &self.language {
            LanguageCodegenOptions::TypeScript(options) => Ok(options),
            _ => Err(ConfigForgeError::Validation(
                "typescript codegen received non-typescript language options".to_owned(),
            )),
        }
    }
}

impl NamingConfig {
    pub fn for_language(language: CodeExportLanguage) -> Self {
        match language {
            CodeExportLanguage::Rust => Self {
                type_name: CaseStyle::PascalCase,
                enum_value: CaseStyle::PascalCase,
                field: CaseStyle::SnakeCase,
                function: CaseStyle::SnakeCase,
                constant: CaseStyle::ScreamingSnakeCase,
            },
            CodeExportLanguage::Cpp => Self {
                type_name: CaseStyle::PascalCase,
                enum_value: CaseStyle::PascalCase,
                field: CaseStyle::SnakeCase,
                function: CaseStyle::PascalCase,
                constant: CaseStyle::PascalCase,
            },
            CodeExportLanguage::TypeScript => Self {
                type_name: CaseStyle::PascalCase,
                enum_value: CaseStyle::Preserve,
                field: CaseStyle::CamelCase,
                function: CaseStyle::CamelCase,
                constant: CaseStyle::CamelCase,
            },
        }
    }

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
        Self::for_language(CodeExportLanguage::Rust)
    }
}

pub fn default_module_name(language: CodeExportLanguage) -> &'static str {
    match language {
        CodeExportLanguage::Rust => "config_forge_generated",
        CodeExportLanguage::Cpp => "config_forge_generated",
        CodeExportLanguage::TypeScript => "dataExportGenerated",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDataFile {
    pub table_name: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    GeneratedCode,
}

pub struct CodeExportInput<'meta> {
    pub meta: &'meta LinkedMetaModel<'meta>,
    pub project: CodeExportProject,
    pub target: CodeExportTarget,
    pub runtime_data_files: Vec<RuntimeDataFile>,
    pub runtime_data_format: RuntimeDataFormat,
    pub generated_at: String,
}

pub trait CodeGenerator {
    fn language(&self) -> CodeExportLanguage;
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>>;
}

pub fn builtin_generators() -> Vec<Box<dyn CodeGenerator>> {
    vec![
        Box::new(RustCodeGenerator),
        Box::new(CppCodeGenerator),
        Box::new(TypeScriptCodeGenerator),
    ]
}

pub fn render_file_header(input: &CodeExportInput<'_>) -> String {
    let Some(template) = &input.target.options.file_header else {
        return String::from("// Generated by config_forge. Do not edit.\n");
    };
    let project = &input.project;
    let mut output = template.clone();
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
    output
}

#[cfg(test)]
mod tests;
