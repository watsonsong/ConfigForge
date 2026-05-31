// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use super::{
    EnumMeta, ExportCondition, FieldMeta, ForeignKeyDeclarationSite, FragmentMeta, IndexMeta,
    MetaModel, PrimitiveFieldType, StructMeta, TableMeta, TargetTags,
};

#[derive(Debug, Clone, PartialEq)]
pub enum LinkedField<'meta> {
    Primitive(PrimitiveFieldType),
    Enum(&'meta EnumMeta),
    Struct(LinkedStructType<'meta>),
    List(Box<LinkedField<'meta>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkedStructType<'meta> {
    pub meta: &'meta StructMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
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
    pub fragments: Vec<LinkedFragmentMeta<'meta>>,
    pub indexes: Vec<LinkedIndexMeta<'meta>>,
    pub foreign_keys: Vec<LinkedForeignKeyMeta<'meta>>,
    primary_index: Option<usize>,
    field_index: HashMap<&'meta str, usize>,
    index_index: HashMap<&'meta str, usize>,
    fragment_index: HashMap<&'meta str, usize>,
    foreign_key_index: HashMap<&'meta str, usize>,
}

pub struct LinkedFragmentMeta<'meta> {
    pub meta: &'meta FragmentMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    pub payload: LinkedFragmentPayload<'meta>,
    field_index: HashMap<&'meta str, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkedFragmentPayload<'meta> {
    InlineFields,
    Struct(LinkedStructType<'meta>),
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
pub struct LinkedForeignKeyMeta<'meta> {
    pub name: &'meta str,
    pub desc: Option<&'meta str>,
    pub export: bool,
    pub export_when: &'meta ExportCondition,
    pub allow_empty: bool,
    pub declared_at: ForeignKeyDeclarationSite<'meta>,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    pub target_table_name: &'meta str,
    pub target_index_name: &'meta str,
    pub linked_field: Option<&'meta str>,
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
        fragments: Vec<LinkedFragmentMeta<'meta>>,
        indexes: Vec<LinkedIndexMeta<'meta>>,
        foreign_keys: Vec<LinkedForeignKeyMeta<'meta>>,
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
        let fragment_index = fragments
            .iter()
            .enumerate()
            .map(|(index, fragment)| (fragment.meta.name.as_str(), index))
            .collect();
        let foreign_key_index = foreign_keys
            .iter()
            .enumerate()
            .map(|(index, foreign_key)| (foreign_key.name, index))
            .collect();
        Self {
            meta,
            fields,
            fragments,
            indexes,
            foreign_keys,
            primary_index,
            field_index,
            index_index,
            fragment_index,
            foreign_key_index,
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

    pub fn fragment(&self, name: &str) -> Option<&LinkedFragmentMeta<'meta>> {
        self.fragment_index
            .get(name)
            .and_then(|index| self.fragments.get(*index))
    }

    pub fn foreign_key(&self, name: &str) -> Option<&LinkedForeignKeyMeta<'meta>> {
        self.foreign_key_index
            .get(name)
            .and_then(|index| self.foreign_keys.get(*index))
    }
}

impl LinkedForeignKeyMeta<'_> {
    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }

    pub fn is_field_level(&self) -> bool {
        matches!(self.declared_at, ForeignKeyDeclarationSite::Field(_, _))
    }
}

impl<'meta> LinkedFragmentMeta<'meta> {
    pub fn new(
        meta: &'meta FragmentMeta,
        fields: Vec<LinkedFieldMeta<'meta>>,
        payload: LinkedFragmentPayload<'meta>,
    ) -> Self {
        let field_index = fields
            .iter()
            .enumerate()
            .map(|(index, field)| (field.meta.name.as_str(), index))
            .collect();
        Self {
            meta,
            fields,
            payload,
            field_index,
        }
    }

    pub fn struct_type(&self) -> Option<&LinkedStructType<'meta>> {
        match &self.payload {
            LinkedFragmentPayload::Struct(struct_type) => Some(struct_type),
            LinkedFragmentPayload::InlineFields => None,
        }
    }

    pub fn field(&self, name: &str) -> Option<&LinkedFieldMeta<'meta>> {
        self.field_index
            .get(name)
            .and_then(|index| self.fields.get(*index))
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

    pub fn foreign_key_primitive_type(&self) -> Option<PrimitiveFieldType> {
        self.foreign_key_scalar_field()
            .and_then(LinkedField::primitive_type)
    }

    pub fn foreign_key_enum_name(&self) -> Option<&'meta str> {
        self.foreign_key_scalar_field()
            .and_then(LinkedField::enum_name)
    }

    pub fn foreign_key_scalar_field(&self) -> Option<&LinkedField<'meta>> {
        match &self.linked_field {
            LinkedField::Primitive(_) | LinkedField::Enum(_) => Some(&self.linked_field),
            LinkedField::List(element) => match element.as_ref() {
                LinkedField::Primitive(_) | LinkedField::Enum(_) => Some(element.as_ref()),
                _ => None,
            },
            LinkedField::Struct(_) => None,
        }
    }
}

impl<'meta> LinkedField<'meta> {
    pub fn primitive_type(&self) -> Option<PrimitiveFieldType> {
        match self {
            LinkedField::Primitive(primitive) => Some(*primitive),
            _ => None,
        }
    }

    pub fn enum_name(&self) -> Option<&'meta str> {
        match self {
            LinkedField::Enum(enum_meta) => Some(enum_meta.name.as_str()),
            _ => None,
        }
    }
}
