// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

mod adapter;
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
mod lua_table;
mod messagepack;
mod registry;
mod requirements;
mod rust;
mod schema_doc;
mod typescript;
mod validation_report;

pub(crate) use artifact_generator::ArtifactGenerator;
pub use binary::BinaryDataExporter;
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
    GeneratorCommonInput, GeneratorDataInput, GeneratorTaskInput, PreparedGeneratorInputs,
};
pub use java::JavaCodegenOptions;
pub use json::JsonDataExporter;
pub use lua_table::LuaTableExporter;
pub use messagepack::MessagePackDataExporter;
pub(crate) use registry::GeneratorRegistry;
pub(crate) use requirements::{GeneratorDescriptor, GeneratorRequirements};
#[cfg(test)]
pub(crate) use rust::RustCodeGenerator;
pub use rust::RustCodegenOptions;
pub use schema_doc::{
    SchemaDoc, SchemaDocExportConfig, SchemaDocFormat, SchemaDocOptions,
    render_schema_doc_index_markdown, render_schema_table_markdown,
};
#[cfg(test)]
pub(crate) use typescript::TypeScriptCodeGenerator;
pub use typescript::TypeScriptCodegenOptions;

pub(crate) use crate::codegen::{
    CaseStyle, CheckedStreamingCodeWriter, CodeEnum, CodeExportInput, CodeField,
    CodeFragmentPayload, CodeGenerator, CodeIndex, CodeModel, CodeStruct, CodeTable, CodeTableKind,
    CodeTypeRef, CodeWriterConfig, CodeWriterLike, I18nFunctionReturn, I18nMode, I18nOptions,
    NamingConfig, RuntimeInterfaceMode, build_code_model, ir,
};

#[cfg(test)]
pub(crate) use crate::codegen::{CodeWriter, GeneratedFile};
