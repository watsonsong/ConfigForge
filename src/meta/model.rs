// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod conditions;
mod linked;
mod raw;
mod types;

pub(crate) use conditions::normalize_tag;
pub use conditions::{ExportCondition, RowExportPolicy, TagMatchMode, TargetTags};
pub use linked::{
    LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta, LinkedFragmentMeta, LinkedFragmentPayload,
    LinkedIndexMeta, LinkedMetaModel, LinkedStructMeta, LinkedStructType, LinkedTableMeta,
};
pub use raw::{
    EnumMeta, EnumValueMeta, FieldForeignKeyMeta, FieldMeta, ForeignKeyDeclarationSite,
    ForeignKeyMeta, FragmentMeta, IndexMeta, MetaModel, ResolvedForeignKeyMeta, SourceRef,
    StructMeta, TableMeta, ValidatorMeta,
};
pub use types::{PrimitiveFieldType, TableKind, ValidationSeverity, ValueLayout};
