// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::diagnostic::DiagnosticSeverity;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct MetaModel {
    #[serde(default)]
    pub enums: Vec<EnumMeta>,
    #[serde(default)]
    pub structs: Vec<StructMeta>,
    #[serde(default)]
    pub tables: Vec<TableMeta>,
}

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
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub export_when: ExportCondition,
    #[serde(default)]
    pub row_export: Option<RowExportPolicy>,
    #[serde(default)]
    pub validators: Vec<ValidatorMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    #[serde(default)]
    pub override_sources: Vec<SourceRef>,
    #[serde(default)]
    pub fields: Vec<FieldMeta>,
    #[serde(default)]
    pub fragments: Vec<FragmentMeta>,
    #[serde(default)]
    pub indexes: Vec<IndexMeta>,
    #[serde(default)]
    pub foreign_keys: Vec<ForeignKeyMeta>,
    #[serde(default)]
    pub primary_index: Option<String>,
    #[serde(default)]
    pub preserve_source_order: bool,
    #[serde(default)]
    pub polymorphic: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FragmentMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub export_when: ExportCondition,
    #[serde(default)]
    pub fields: Vec<FieldMeta>,
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
    pub export_when: ExportCondition,
    #[serde(default)]
    pub validators: Vec<ValidatorMeta>,
    #[serde(default)]
    pub tool_only: Option<bool>,
    #[serde(default)]
    pub filter_field: Option<bool>,
    #[serde(default)]
    pub validator_only: Option<bool>,
    #[serde(default)]
    pub field_number: Option<u32>,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub foreign_key: Option<FieldForeignKeyMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FieldForeignKeyMeta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub export_when: ExportCondition,
    pub target_table: String,
    #[serde(default)]
    pub target_index: Option<String>,
    #[serde(default)]
    pub allow_empty: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ForeignKeyMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub export_when: ExportCondition,
    #[serde(default)]
    pub fields: Vec<String>,
    pub target_table: String,
    #[serde(default)]
    pub target_index: Option<String>,
    #[serde(default)]
    pub allow_empty: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ForeignKeyDeclarationSite<'meta> {
    Table(&'meta ForeignKeyMeta),
    Field(&'meta FieldMeta, &'meta FieldForeignKeyMeta),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedForeignKeyMeta<'meta> {
    pub name: &'meta str,
    pub desc: Option<&'meta str>,
    pub export: bool,
    pub export_when: &'meta ExportCondition,
    pub fields: Vec<&'meta str>,
    pub target_table: &'meta str,
    pub target_index: Option<&'meta str>,
    pub allow_empty: bool,
    pub declared_at: ForeignKeyDeclarationSite<'meta>,
}

fn default_export() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ValidatorMeta {
    pub kind: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub severity: ValidationSeverity,
    #[serde(default)]
    pub export_when: ExportCondition,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(flatten)]
    pub params: BTreeMap<String, toml::Value>,
}

pub type ValidationSeverity = DiagnosticSeverity;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExportCondition {
    #[serde(default)]
    pub any: Vec<String>,
    #[serde(default)]
    pub all: Vec<String>,
    #[serde(default)]
    pub none: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RowExportPolicy {
    #[serde(default)]
    pub required_tags_field: Option<String>,
    #[serde(default)]
    pub excluded_tags_field: Option<String>,
    #[serde(default)]
    #[serde(rename = "match")]
    pub match_mode: TagMatchMode,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TagMatchMode {
    #[default]
    Any,
    All,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetTags {
    tags: BTreeSet<String>,
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
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub export_when: ExportCondition,
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
        let (_, mut diagnostics) = super::link::link_meta_model(self);
        diagnostics.extend(super::validate::validate_meta_model(self));
        diagnostics.sort();
        diagnostics.dedup();
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

    pub fn resolved_foreign_keys(&self) -> Vec<ResolvedForeignKeyMeta<'_>> {
        let mut resolved = Vec::new();
        for foreign_key in &self.foreign_keys {
            let fields = foreign_key
                .fields
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            // Missing allow_empty inherits requiredness so optional references do not need duplicate FK config.
            let allow_empty = foreign_key.allow_empty.unwrap_or_else(|| {
                fields.iter().all(|field_name| {
                    self.field(field_name)
                        .map(|field| !field.required)
                        .unwrap_or(false)
                })
            });
            resolved.push(ResolvedForeignKeyMeta {
                name: foreign_key.name.as_str(),
                desc: foreign_key.desc.as_deref(),
                export: foreign_key.export,
                export_when: &foreign_key.export_when,
                fields,
                target_table: foreign_key.target_table.as_str(),
                target_index: foreign_key.target_index.as_deref(),
                allow_empty,
                declared_at: ForeignKeyDeclarationSite::Table(foreign_key),
            });
        }

        for field in &self.fields {
            let Some(foreign_key) = &field.foreign_key else {
                continue;
            };
            resolved.push(ResolvedForeignKeyMeta {
                name: foreign_key.name.as_deref().unwrap_or(field.name.as_str()),
                desc: foreign_key.desc.as_deref(),
                export: foreign_key.export,
                export_when: &foreign_key.export_when,
                fields: vec![field.name.as_str()],
                target_table: foreign_key.target_table.as_str(),
                target_index: foreign_key.target_index.as_deref(),
                allow_empty: foreign_key.allow_empty.unwrap_or(!field.required),
                declared_at: ForeignKeyDeclarationSite::Field(field, foreign_key),
            });
        }
        resolved
    }

    pub fn source_path(&self) -> Option<&str> {
        self.sources
            .first()
            .map(|source| source.path.as_str())
            .or(self.source.as_deref())
    }

    pub fn is_variant_source_mode(&self) -> bool {
        // Fragment activation depends on per-row source metadata, so only collection tables opt in.
        self.kind == TableKind::Collection
            && self.polymorphic
            && self
                .sources
                .iter()
                .any(|source| !source.fragments.is_empty())
    }

    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }

    pub fn is_row_exported_for<'a>(
        &self,
        target: &TargetTags,
        value_of: impl Fn(&str) -> Option<&'a str>,
    ) -> bool {
        self.row_export
            .as_ref()
            .map(|policy| policy.matches(target, value_of))
            .unwrap_or(true)
    }
}

impl FieldMeta {
    pub fn primitive_type(&self) -> Option<PrimitiveFieldType> {
        PrimitiveFieldType::parse(&self.field_type)
    }

    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }
}

impl FragmentMeta {
    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }
}

impl IndexMeta {
    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }
}

impl ForeignKeyMeta {
    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }
}

impl FieldForeignKeyMeta {
    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }
}

impl ResolvedForeignKeyMeta<'_> {
    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }

    pub fn is_field_level(&self) -> bool {
        matches!(self.declared_at, ForeignKeyDeclarationSite::Field(_, _))
    }
}

impl ExportCondition {
    pub fn matches(&self, target: &TargetTags) -> bool {
        (self.any.is_empty() || self.any.iter().any(|tag| target.contains(tag)))
            && self.all.iter().all(|tag| target.contains(tag))
            && self.none.iter().all(|tag| !target.contains(tag))
    }
}

impl RowExportPolicy {
    pub fn matches<'a>(
        &self,
        target: &TargetTags,
        value_of: impl Fn(&str) -> Option<&'a str>,
    ) -> bool {
        if let Some(field) = &self.excluded_tags_field {
            if parse_tag_list(value_of(field))
                .iter()
                .any(|tag| target.contains(tag))
            {
                return false;
            }
        }

        let required_tags = self
            .required_tags_field
            .as_ref()
            .map(|field| parse_tag_list(value_of(field)))
            .unwrap_or_default();
        if required_tags.is_empty() {
            return true;
        }

        match self.match_mode {
            TagMatchMode::Any => required_tags.iter().any(|tag| target.contains(tag)),
            TagMatchMode::All => required_tags.iter().all(|tag| target.contains(tag)),
        }
    }
}

impl TargetTags {
    pub fn new(tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            tags: tags
                .into_iter()
                .filter_map(|tag| normalize_tag(tag.into()))
                .collect(),
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn contains(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.tags.iter().map(String::as_str)
    }
}

fn parse_tag_list(value: Option<&str>) -> Vec<&str> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect()
}

pub(crate) fn normalize_tag(tag: String) -> Option<String> {
    let tag = tag.trim();
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_owned())
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
}
