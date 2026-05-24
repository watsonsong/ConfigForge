// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod diagnostic;
mod link;
mod loader;
mod model;
mod validate;

pub use diagnostic::{MetaDiagnostic, MetaDiagnosticCode};
pub use model::{
    EnumMeta, EnumValueMeta, ExportCondition, FieldForeignKeyMeta, FieldMeta,
    ForeignKeyDeclarationSite, ForeignKeyMeta, FragmentMeta, IndexMeta, LinkedField,
    LinkedFieldMeta, LinkedForeignKeyMeta, LinkedFragmentMeta, LinkedFragmentPayload,
    LinkedIndexMeta, LinkedMetaModel, LinkedStructMeta, LinkedStructType, LinkedTableMeta,
    MetaModel, PrimitiveFieldType, ResolvedForeignKeyMeta, RowExportPolicy, SourceRef, StructMeta,
    TableKind, TableMeta, TagMatchMode, TargetTags, ValidationSeverity, ValidatorMeta, ValueLayout,
};

#[cfg(test)]
mod tests;
