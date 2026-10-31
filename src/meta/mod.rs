// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod diagnostic;
mod link;
mod loader;
mod model;
mod validate;

pub(crate) use diagnostic::meta_diagnostics_to_report;
pub use diagnostic::{MetaDiagnostic, MetaDiagnosticCode};
#[allow(unused_imports)]
pub use model::{
    AssetExtensionPolicy, AssetInputKind, AssetKindMeta, AssetPathStyle, AssetRegexRuleMeta,
    AssetRegexTarget, AssetSearchPathConfig, AssetsConfig, ColorAlphaPolicy, ColorChannel,
    ColorFormat, ColorSpace, EnumMeta, EnumValueMeta, ExportCondition, FieldForeignKeyMeta,
    FieldMeta, ForeignKeyDeclarationSite, ForeignKeyMeta, FragmentMeta, IndexMeta, LinkedField,
    LinkedFieldMeta, LinkedForeignKeyMeta, LinkedFragmentMeta, LinkedFragmentPayload,
    LinkedIndexMeta, LinkedMetaModel, LinkedStructMeta, LinkedStructType, LinkedTableMeta,
    MetaModel, PrimitiveFieldType, ResolvedForeignKeyMeta, RowExportPolicy, SourceRef, StructMeta,
    TableKind, TableMeta, TagMatchMode, TargetTags, ValidationSeverity, ValidatorMeta, ValueLayout,
    VectorFormat, list_element_type_name,
};

mod provenance;
#[cfg(test)]
mod tests;
pub use provenance::DeclarationSources;
pub(crate) use provenance::{declaration_sources, enrich_diagnostics};
