// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod artifact;
mod build;
mod cli;
mod codegen;
mod data;
mod diagnostic;
mod exporter;
mod meta;
mod project;
mod validation;

pub mod error {
    pub use crate::diagnostic::{ConfigForgeError, Result};
}

pub use artifact::{
    Artifact, ArtifactContent, ArtifactWriteEntry, ArtifactWriteMode, ArtifactWriteResult,
    ArtifactWriter,
};
pub use build::{build_target, BuildOptions, BuildOutput, ExecutionOptions};
pub use codegen::{
    builtin_generators, default_module_name, find_generator, CaseStyle, CodeExportInput,
    CodeExportProject, CodeExportTarget, CodeGenerator, CodeWriterConfig, CodegenOptions,
    CodegenTaskConfig, CppCodeGenerator, CppCodegenOptions, GeneratedFile, IndentStyle,
    KeywordConflictPolicy, NamingConfig, PartialCodeWriterConfig, PartialNamingConfig,
    RustCodeGenerator, RustCodegenOptions, TypeScriptCodeGenerator, TypeScriptCodegenOptions,
};
pub use data::{
    CsvDataReader, DataLoadPlan, DataLoader, DataProvider, DataRow, DataSet, DataSourceLayout,
    DataSourcePlan, DataTable, RowSourceInfo,
};
pub use diagnostic::{
    ConfigForgeError, Diagnostic, DiagnosticEntry, DiagnosticLocation, DiagnosticReport,
    DiagnosticSeverity, Result,
};
pub use exporter::{
    builtin_exporters, BinaryDataExporter, DataFormatExporter, DataFormatRegistry, ExportedTable,
    ExportedTablePayload, ExportedValue, JsonDataExporter, MessagePackDataExporter,
    RuntimeArtifactPlan, RuntimeDataFile, TargetDataConfig,
};
pub use meta::{
    EnumMeta, EnumValueMeta, ExportCondition, FieldForeignKeyMeta, FieldMeta,
    ForeignKeyDeclarationSite, ForeignKeyMeta, FragmentMeta, IndexMeta, LinkedField,
    LinkedFieldMeta, LinkedForeignKeyMeta, LinkedFragmentMeta, LinkedFragmentPayload,
    LinkedIndexMeta, LinkedMetaModel, LinkedStructMeta, LinkedStructType, LinkedTableMeta,
    MetaDiagnostic, MetaDiagnosticCode, MetaModel, PrimitiveFieldType, ResolvedForeignKeyMeta,
    RowExportPolicy, SourceRef, StructMeta, TableKind, TableMeta, TagMatchMode, TargetTags,
    ValidationSeverity, ValidatorMeta, ValueLayout,
};
pub use project::{ProjectConfig, ProjectConfigMeta, TargetConfig};
pub use validation::{
    apply_field_defaults, normalize_data_set, normalize_table, validate_data_set,
    validate_data_set_for_target, validate_data_set_for_target_with_report,
    validate_data_set_with_report, validate_normalized_data_set,
    validate_normalized_data_set_for_target, validate_normalized_data_set_for_target_with_report,
    validate_normalized_table, validate_normalized_table_for_target, validate_table,
    validate_table_for_target, CompiledValidationPlan, CompiledValidator, ExportTableView,
    ExportValidationView, ValidationContext, ValidationIssue, ValidationIssueLocation,
    ValidationReport, ValidatorFactory, ValidatorRegistry, VisibleRow,
};

pub fn run_cli() -> anyhow::Result<()> {
    cli::run()
}
