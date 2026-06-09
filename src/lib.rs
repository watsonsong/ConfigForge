// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod artifact;
mod artifact_info;
mod asset;
mod build;
mod cli;
mod code_artifact;
mod data;
mod data_artifact;
mod diagnostic;
mod generator;
mod meta;
mod project;
mod runtime_contract;
mod validation;

pub mod error {
    pub use crate::diagnostic::{ConfigForgeError, Result};
}

pub use artifact::{
    Artifact, ArtifactContent, ArtifactWriteEntry, ArtifactWriteMode, ArtifactWriteResult,
    ArtifactWriter,
};
pub use artifact_info::ArtifactProjectInfo;
pub use asset::{AssetResolver, EffectiveAssetKind, ResolvedAssetRef};
pub use build::{
    BuildOptions, BuildOutput, BuildTrace, BuildTraceStage, BuildTraceStageStatus,
    ExecutionOptions, build_target,
};
pub use code_artifact::{
    CaseStyle, CheckedStreamingCodeWriter, CodeArtifactOptions, CodeExportInput, CodeExportProject,
    CodeExportTarget, CodeWriterConfig, CodeWriterLike, CodegenOptions, CodegenTaskConfig,
    I18nFunctionReturn, I18nMode, I18nOptions, IndentStyle, KeywordConflictPolicy, NamingConfig,
    PartialCodeWriterConfig, PartialNamingConfig, StreamingCodeWriter, default_module_name,
};
pub use data::{
    AsyncSourceReadShell, CsvDataReader, DataLoadPlan, DataLoader, DataProvider, DataRow, DataSet,
    DataSourceLayout, DataSourcePlan, DataTable, LocalCsvSourceProvider, RawSourceData,
    RemoteProviderPolicy, RowSourceInfo, SourceProvider, SourceProviderConcurrency,
    SourceProviderResource, SourceReadPlan,
};
pub use data_artifact::{
    ExportedTable, ExportedTablePayload, ExportedValue, RuntimeArtifactPlan, RuntimeTableFormat,
    TargetDataConfig,
};
pub use diagnostic::{
    ConfigForgeError, Diagnostic, DiagnosticEntry, DiagnosticLocation, DiagnosticReport,
    DiagnosticSeverity, Result,
};
pub use generator::{
    BinaryDataExporter, CSharpCodegenOptions, CppCodegenOptions, CppUnrealCodegenOptions,
    CppUnrealSupportLevel, JavaCodegenOptions, JsonDataExporter, LuaTableExporter,
    MessagePackDataExporter, RustCodegenOptions, SchemaDoc, SchemaDocExportConfig, SchemaDocFormat,
    SchemaDocOptions, TypeScriptCodegenOptions, render_schema_doc_index_markdown,
    render_schema_table_markdown,
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
pub use project::{
    AssetSearchPathConfig, AssetsConfig, ProjectConfig, ProjectConfigMeta, TargetConfig,
};
pub use runtime_contract::RuntimeDataFile;
pub use validation::{
    CompiledValidationPlan, CompiledValidator, ExportTableView, ExportValidationView,
    ValidationContext, ValidationIssue, ValidationIssueLocation, ValidationReport,
    ValidatorFactory, ValidatorRegistry, VisibleRow, apply_field_defaults, normalize_data_set,
    normalize_data_set_with_pool, normalize_table, validate_data_set, validate_data_set_for_target,
    validate_data_set_for_target_with_report, validate_data_set_with_report,
    validate_normalized_data_set, validate_normalized_data_set_for_target,
    validate_normalized_data_set_for_target_with_assets_and_report,
    validate_normalized_data_set_for_target_with_assets_report_and_pool,
    validate_normalized_data_set_for_target_with_report,
    validate_normalized_data_set_for_target_with_report_and_pool,
    validate_normalized_data_set_with_assets_and_report,
    validate_normalized_data_set_with_report_and_pool, validate_normalized_table,
    validate_normalized_table_for_target, validate_table, validate_table_for_target,
};

pub fn run_cli() -> anyhow::Result<()> {
    cli::run()
}
