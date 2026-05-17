use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct MetaModel {
    #[serde(default)]
    pub enums: Vec<EnumMeta>,
    #[serde(default)]
    pub structs: Vec<StructMeta>,
    #[serde(default)]
    pub tables: Vec<TableMeta>,
}

pub type MetaSet = MetaModel;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EnumMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub values: Vec<EnumValueMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EnumValueMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct StructMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub fields: Vec<FieldMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct TableMeta {
    pub name: String,
    #[serde(default)]
    pub kind: TableKind,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    #[serde(default)]
    pub override_sources: Vec<SourceRef>,
    #[serde(default)]
    pub fields: Vec<FieldMeta>,
    #[serde(default)]
    pub indexes: Vec<IndexMeta>,
    #[serde(default)]
    pub primary_index: Option<String>,
    #[serde(default)]
    pub preserve_source_order: bool,
    #[serde(default)]
    pub polymorphic: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    Object,
    #[default]
    Collection,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SourceRef {
    pub provider: String,
    pub path: String,
    #[serde(default)]
    pub fragments: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FieldMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<toml::Value>,
    #[serde(default)]
    pub layout: Option<ValueLayout>,
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub tool_only: bool,
    #[serde(default)]
    pub filter_field: bool,
    #[serde(default)]
    pub validator_only: bool,
    #[serde(default)]
    pub field_number: Option<u32>,
    #[serde(default)]
    pub unique: bool,
}

fn default_export() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkedField<'meta> {
    Primitive(PrimitiveFieldType),
    Enum(&'meta EnumMeta),
    Struct(LinkedStructType<'meta>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkedStructType<'meta> {
    pub meta: &'meta StructMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveFieldType {
    Int,
    Float,
    Bool,
    String,
    Text,
    Asset,
}

impl PrimitiveFieldType {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "int" | "i64" => Some(Self::Int),
            "float" | "f64" => Some(Self::Float),
            "bool" => Some(Self::Bool),
            "string" => Some(Self::String),
            "text" => Some(Self::Text),
            "asset" => Some(Self::Asset),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::String => "string",
            Self::Text => "text",
            Self::Asset => "asset",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueLayout {
    ScalarCell,
    JsonCell,
    ExpandedValue,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct IndexMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub primary: bool,
}

impl MetaModel {
    pub fn table(&self, name: &str) -> Option<&TableMeta> {
        self.tables.iter().find(|table| table.name == name)
    }

    pub fn enum_meta(&self, name: &str) -> Option<&EnumMeta> {
        self.enums.iter().find(|enum_meta| enum_meta.name == name)
    }

    pub fn struct_meta(&self, name: &str) -> Option<&StructMeta> {
        self.structs
            .iter()
            .find(|struct_meta| struct_meta.name == name)
    }

    pub fn linked_enum<'meta>(
        &'meta self,
        linked_field: &LinkedField<'meta>,
    ) -> Option<&'meta EnumMeta> {
        match linked_field {
            LinkedField::Enum(enum_meta) => Some(enum_meta),
            _ => None,
        }
    }

    pub fn linked_struct<'meta>(
        &'meta self,
        linked_field: &'meta LinkedField<'meta>,
    ) -> Option<&'meta LinkedStructType<'meta>> {
        match linked_field {
            LinkedField::Struct(struct_type) => Some(struct_type),
            _ => None,
        }
    }

    pub fn collection_tables(&self) -> impl Iterator<Item = &TableMeta> {
        self.tables
            .iter()
            .filter(|table| table.kind == TableKind::Collection)
    }

    pub fn validate(&self) -> std::result::Result<(), Vec<crate::meta::MetaDiagnostic>> {
        let diagnostics = crate::meta::validate_meta_model(self);
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(diagnostics)
        }
    }
}

impl TableMeta {
    pub fn field(&self, name: &str) -> Option<&FieldMeta> {
        self.fields.iter().find(|field| field.name == name)
    }

    pub fn index(&self, name: &str) -> Option<&IndexMeta> {
        self.indexes.iter().find(|index| index.name == name)
    }

    pub fn primary_index(&self) -> Option<&IndexMeta> {
        if let Some(primary_index) = &self.primary_index {
            return self.index(primary_index);
        }
        self.indexes.iter().find(|index| index.primary)
    }

    pub fn primary_index_meta(&self) -> Option<&IndexMeta> {
        self.primary_index()
    }

    pub fn source_path(&self) -> Option<&str> {
        self.sources
            .first()
            .map(|source| source.path.as_str())
            .or(self.source.as_deref())
    }
}

impl FieldMeta {
    pub fn primitive_type(&self) -> Option<PrimitiveFieldType> {
        PrimitiveFieldType::parse(&self.field_type)
    }
}

pub struct LinkedMetaModel<'meta> {
    pub model: &'meta MetaModel,
    pub tables: Vec<LinkedTableMeta<'meta>>,
    pub structs: Vec<LinkedStructMeta<'meta>>,
    enum_index: HashMap<&'meta str, &'meta EnumMeta>,
    struct_meta_index: HashMap<&'meta str, &'meta StructMeta>,
    table_index: HashMap<&'meta str, usize>,
    struct_index: HashMap<&'meta str, usize>,
}

pub struct LinkedTableMeta<'meta> {
    pub meta: &'meta TableMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    pub indexes: Vec<LinkedIndexMeta<'meta>>,
    primary_index: Option<usize>,
    field_index: HashMap<&'meta str, usize>,
    index_index: HashMap<&'meta str, usize>,
}

pub struct LinkedStructMeta<'meta> {
    pub meta: &'meta StructMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    field_index: HashMap<&'meta str, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkedIndexMeta<'meta> {
    pub meta: &'meta IndexMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkedFieldMeta<'meta> {
    pub meta: &'meta FieldMeta,
    pub linked_field: LinkedField<'meta>,
}

impl<'meta> LinkedMetaModel<'meta> {
    pub fn new(
        model: &'meta MetaModel,
        tables: Vec<LinkedTableMeta<'meta>>,
        structs: Vec<LinkedStructMeta<'meta>>,
    ) -> Self {
        let enum_index = model
            .enums
            .iter()
            .map(|enum_meta| (enum_meta.name.as_str(), enum_meta))
            .collect();
        let struct_meta_index = model
            .structs
            .iter()
            .map(|struct_meta| (struct_meta.name.as_str(), struct_meta))
            .collect();
        let table_index = tables
            .iter()
            .enumerate()
            .map(|(index, table)| (table.meta.name.as_str(), index))
            .collect();
        let struct_index = structs
            .iter()
            .enumerate()
            .map(|(index, struct_meta)| (struct_meta.meta.name.as_str(), index))
            .collect();
        Self {
            model,
            tables,
            structs,
            enum_index,
            struct_meta_index,
            table_index,
            struct_index,
        }
    }

    pub fn enum_meta(&self, name: &str) -> Option<&'meta EnumMeta> {
        self.enum_index.get(name).copied()
    }

    pub fn raw_struct_meta(&self, name: &str) -> Option<&'meta StructMeta> {
        self.struct_meta_index.get(name).copied()
    }

    pub fn table(&self, name: &str) -> Option<&LinkedTableMeta<'meta>> {
        self.table_index
            .get(name)
            .and_then(|index| self.tables.get(*index))
    }

    pub fn struct_meta(&self, name: &str) -> Option<&LinkedStructMeta<'meta>> {
        self.struct_index
            .get(name)
            .and_then(|index| self.structs.get(*index))
    }
}

impl<'meta> LinkedTableMeta<'meta> {
    pub fn new(
        meta: &'meta TableMeta,
        fields: Vec<LinkedFieldMeta<'meta>>,
        indexes: Vec<LinkedIndexMeta<'meta>>,
    ) -> Self {
        let field_index = fields
            .iter()
            .enumerate()
            .map(|(index, field)| (field.meta.name.as_str(), index))
            .collect();
        let index_index = indexes
            .iter()
            .enumerate()
            .map(|(index, index_meta)| (index_meta.meta.name.as_str(), index))
            .collect::<HashMap<_, _>>();
        let primary_index = meta
            .primary_index()
            .and_then(|index| index_index.get(index.name.as_str()).copied());
        Self {
            meta,
            fields,
            indexes,
            primary_index,
            field_index,
            index_index,
        }
    }

    pub fn field(&self, name: &str) -> Option<&LinkedFieldMeta<'meta>> {
        self.field_index
            .get(name)
            .and_then(|index| self.fields.get(*index))
    }

    pub fn index(&self, name: &str) -> Option<&LinkedIndexMeta<'meta>> {
        self.index_index
            .get(name)
            .and_then(|index| self.indexes.get(*index))
    }

    pub fn primary_index(&self) -> Option<&LinkedIndexMeta<'meta>> {
        self.primary_index.and_then(|index| self.indexes.get(index))
    }
}

impl<'meta> LinkedStructMeta<'meta> {
    pub fn new(meta: &'meta StructMeta, fields: Vec<LinkedFieldMeta<'meta>>) -> Self {
        let field_index = fields
            .iter()
            .enumerate()
            .map(|(index, field)| (field.meta.name.as_str(), index))
            .collect();
        Self {
            meta,
            fields,
            field_index,
        }
    }

    pub fn field(&self, name: &str) -> Option<&LinkedFieldMeta<'meta>> {
        self.field_index
            .get(name)
            .and_then(|index| self.fields.get(*index))
    }
}

impl<'meta> LinkedFieldMeta<'meta> {
    pub fn primitive_type(&self) -> Option<PrimitiveFieldType> {
        match &self.linked_field {
            LinkedField::Primitive(primitive) => Some(*primitive),
            _ => None,
        }
    }
}
