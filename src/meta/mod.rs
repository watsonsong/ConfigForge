mod diagnostic;
mod link;
mod loader;
mod model;
mod validate;

pub use diagnostic::{MetaDiagnostic, MetaDiagnosticCode};
pub use link::link_meta_model;
pub use loader::load_meta_path;
pub use model::{
    EnumMeta, EnumValueMeta, ExportCondition, FieldForeignKeyMeta, FieldMeta,
    ForeignKeyDeclarationSite, ForeignKeyMeta, FragmentMeta, IndexMeta, LinkedField,
    LinkedFieldMeta, LinkedForeignKeyMeta, LinkedFragmentMeta, LinkedFragmentPayload,
    LinkedIndexMeta, LinkedMetaModel, LinkedStructMeta, LinkedStructType, LinkedTableMeta,
    MetaModel, MetaSet, PrimitiveFieldType, ResolvedForeignKeyMeta, RowExportPolicy, SourceRef,
    StructMeta, TableKind, TableMeta, TagMatchMode, TargetTags, ValidationSeverity, ValidatorMeta,
    ValueLayout,
};
pub use validate::validate_meta_model;

#[cfg(test)]
mod tests;
