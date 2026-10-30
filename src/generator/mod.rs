// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod artifact_generator;
mod asset_manifest;
mod binary;
mod cpp;
mod cpp_unreal;
mod csharp;
mod gettext_pot;
mod input;
mod java;
mod json;
mod lua;
mod messagepack;
mod options;
mod registry;
mod requirements;
mod rust;
mod schema_doc;
mod typescript;
mod validation_report;

pub(crate) use crate::code_artifact::{
    CaseStyle, CodeEnum, CodeExportInput, CodeField, CodeFragmentPayload, CodeGenerator, CodeIndex,
    CodeModel, CodeStruct, CodeTable, CodeTableKind, CodeTypeRef, CodeWriterConfig, CodeWriterLike,
    I18nFunctionReturn, I18nMode, I18nOptions, NamingConfig, ReaderShapePlan, RuntimeInterfaceEmit,
    RuntimeInterfaceMode, build_code_model, ir, write_code_to_sink, write_file_header,
};
pub(crate) use artifact_generator::{ArtifactGenerator, generate_code_artifact_to_sink};
#[cfg(test)]
pub(crate) use binary::BinaryDataExporter;
#[cfg(test)]
pub(crate) use cpp::CppCodeGenerator;
pub use cpp::CppCodegenOptions;
#[cfg(test)]
pub(crate) use cpp_unreal::CppUnrealCodeGenerator;
pub use cpp_unreal::{CppUnrealCodegenOptions, CppUnrealSupportLevel};
#[cfg(test)]
pub(crate) use csharp::CSharpCodeGenerator;
pub use csharp::CSharpCodegenOptions;
pub(crate) use input::{
    GeneratorCommonInput, GeneratorCommonInputContext, GeneratorTaskInput, PreparedGeneratorInputs,
};
pub use java::JavaCodegenOptions;
pub(crate) use options::GeneratorOptionParser;
pub(crate) use registry::GeneratorRegistry;
pub(crate) use requirements::{GeneratorDescriptor, GeneratorRequirements};
#[cfg(test)]
pub(crate) use rust::RustCodeGenerator;
pub use rust::RustCodegenOptions;
pub use schema_doc::{SchemaDoc, SchemaDocExportConfig, SchemaDocFormat, SchemaDocOptions};
#[cfg(test)]
pub(crate) use typescript::TypeScriptCodeGenerator;
pub use typescript::TypeScriptCodegenOptions;

#[cfg(test)]
pub(crate) use crate::code_artifact::{CodeWriter, GeneratedFile};
