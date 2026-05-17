mod diagnostic;
mod link;
mod loader;
mod model;
mod validate;

pub use diagnostic::{MetaDiagnostic, MetaDiagnosticCode};
pub use link::link_meta_model;
pub use loader::load_meta_path;
pub use model::{
    EnumMeta, EnumValueMeta, FieldMeta, IndexMeta, LinkedField, LinkedFieldMeta, LinkedIndexMeta,
    LinkedMetaModel, LinkedStructMeta, LinkedTableMeta, MetaModel, MetaSet, PrimitiveFieldType,
    SourceRef, StructMeta, TableKind, TableMeta, ValueLayout,
};
pub use validate::validate_meta_model;

#[cfg(test)]
mod tests;
