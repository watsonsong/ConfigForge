// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use super::{
    ExportCondition, LinkedField, LinkedStructType, PrimitiveFieldType, RowExportPolicy, TableKind,
    TargetTags, ValidationSeverity, ValueLayout,
};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct MetaModel {
    #[serde(default)]
    pub enums: Vec<EnumMeta>,
    #[serde(default)]
    pub structs: Vec<StructMeta>,
    #[serde(default)]
    pub tables: Vec<TableMeta>,
    #[serde(default)]
    pub asset_kinds: Vec<AssetKindMeta>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct AssetsConfig {
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    #[serde(default)]
    pub require_existing: Option<bool>,
    #[serde(default)]
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub search_paths: Vec<AssetSearchPathConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AssetSearchPathConfig {
    pub name: String,
    pub root: PathBuf,
    #[serde(default = "default_asset_search_path_kind")]
    pub kind: String,
    #[serde(default)]
    pub mount: Option<String>,
}

fn default_asset_search_path_kind() -> String {
    "filesystem".to_owned()
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AssetKindMeta {
    pub name: String,
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub search_paths: Vec<String>,
    #[serde(default)]
    pub base_dirs: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub input: Option<AssetInputKind>,
    #[serde(default)]
    pub path_style: Option<AssetPathStyle>,
    #[serde(default)]
    pub extension_policy: Option<AssetExtensionPolicy>,
    #[serde(default)]
    pub strip_wrappers: Vec<String>,
    #[serde(default)]
    pub object_separator: Option<String>,
    #[serde(default)]
    pub object_name: Option<String>,
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    #[serde(default)]
    pub require_existing: Option<bool>,
    #[serde(default)]
    pub require_sidecars: Vec<String>,
    #[serde(default)]
    pub allow_path_prefixes: Vec<String>,
    #[serde(default)]
    pub deny_path_prefixes: Vec<String>,
    #[serde(default)]
    pub deny_when_tags: Vec<String>,
    #[serde(default)]
    pub name_prefixes: Vec<String>,
    #[serde(default)]
    pub name_suffixes: Vec<String>,
    #[serde(default)]
    pub allow_patterns: Vec<String>,
    #[serde(default)]
    pub deny_patterns: Vec<String>,
    #[serde(default)]
    pub regex_rules: Vec<AssetRegexRuleMeta>,
    #[serde(default)]
    pub allow_auto_complete: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetInputKind {
    Path,
    String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetPathStyle {
    Relative,
    Mounted,
    MountedObject,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetExtensionPolicy {
    Required,
    Forbidden,
    Optional,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AssetRegexRuleMeta {
    pub name: String,
    #[serde(default)]
    pub target: Option<AssetRegexTarget>,
    pub pattern: String,
    #[serde(default)]
    pub severity: ValidationSeverity,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub when_tags: Vec<String>,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub case_sensitive: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetRegexTarget {
    RawInput,
    LogicalRef,
    SearchPathRelative,
    FileName,
    Stem,
    ObjectName,
    ExportRef,
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
    #[serde(
        default,
        deserialize_with = "deserialize_optional_source_ref",
        skip_serializing_if = "Option::is_none"
    )]
    pub source: Option<SourceRef>,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SourceRef {
    #[serde(default = "default_source_provider")]
    pub provider: String,
    pub path: String,
    #[serde(default = "default_source_header_row")]
    pub header_row: usize,
    #[serde(default = "default_source_data_start_row")]
    pub data_start_row: usize,
    #[serde(default)]
    pub fragments: Vec<String>,
    #[serde(default)]
    pub export_when: ExportCondition,
}

impl SourceRef {
    pub fn csv(path: impl Into<String>) -> Self {
        Self {
            provider: default_source_provider(),
            path: path.into(),
            header_row: default_source_header_row(),
            data_start_row: default_source_data_start_row(),
            fragments: Vec::new(),
            export_when: ExportCondition::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SourceRefInput {
    Path(String),
    Ref(SourceRef),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FieldMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub asset_kind: Option<String>,
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
    #[serde(default, rename = "color_format")]
    pub color_format: Option<ColorFormat>,
    #[serde(default, rename = "color_channel")]
    pub color_channel: Option<ColorChannel>,
    #[serde(default, rename = "color_space")]
    pub color_space: Option<ColorSpace>,
    #[serde(default, rename = "alpha")]
    pub color_alpha: Option<ColorAlphaPolicy>,
    #[serde(default, rename = "vector_format")]
    pub vector_format: Option<VectorFormat>,
    #[serde(default, rename = "vector_components")]
    pub vector_components: Option<Vec<String>>,
    #[serde(default)]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorFormat {
    HexRgb,
    #[default]
    HexRgba,
    RgbTuple,
    RgbaTuple,
    Object,
}

impl ColorFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HexRgb => "hex_rgb",
            Self::HexRgba => "hex_rgba",
            Self::RgbTuple => "rgb_tuple",
            Self::RgbaTuple => "rgba_tuple",
            Self::Object => "object",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorChannel {
    Byte,
    Normalized,
}

impl ColorChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Byte => "byte",
            Self::Normalized => "normalized",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorSpace {
    #[default]
    Srgb,
    Linear,
}

impl ColorSpace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::Linear => "linear",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorAlphaPolicy {
    #[default]
    Required,
    DefaultOpaque,
}

impl ColorAlphaPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::DefaultOpaque => "default_opaque",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VectorFormat {
    #[default]
    Tuple,
    Array,
    Object,
    Expanded,
}

impl VectorFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tuple => "tuple",
            Self::Array => "array",
            Self::Object => "object",
            Self::Expanded => "expanded",
        }
    }
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
    #[serde(default)]
    pub linked_field: Option<String>,
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
    #[serde(default)]
    pub linked_field: Option<String>,
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
    pub linked_field: Option<&'meta str>,
    pub declared_at: ForeignKeyDeclarationSite<'meta>,
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
    #[serde(default)]
    pub ordered: bool,
}

fn default_export() -> bool {
    true
}

fn default_source_provider() -> String {
    "csv".to_owned()
}

fn default_source_header_row() -> usize {
    1
}

fn default_source_data_start_row() -> usize {
    3
}

fn deserialize_optional_source_ref<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<SourceRef>, D::Error>
where
    D: Deserializer<'de>,
{
    let input = Option::<SourceRefInput>::deserialize(deserializer)?;
    Ok(input.map(|input| match input {
        SourceRefInput::Path(path) => SourceRef::csv(path),
        SourceRefInput::Ref(source) => source,
    }))
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

    pub fn asset_kind(&self, name: &str) -> Option<&AssetKindMeta> {
        self.asset_kinds
            .iter()
            .find(|asset_kind| asset_kind.name == name)
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
        let (_, mut diagnostics) = super::super::link::link_meta_model(self);
        diagnostics.extend(super::super::validate::validate_meta_model(self));
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
                linked_field: foreign_key.linked_field.as_deref(),
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
                linked_field: foreign_key.linked_field.as_deref(),
                declared_at: ForeignKeyDeclarationSite::Field(field, foreign_key),
            });
        }
        resolved
    }

    /// Returns the primary source path for legacy callers that only handle one source.
    ///
    /// Multi-source tables should use [`TableMeta::source_paths`] or, in production data
    /// loading, go through `DataLoadPlan` so fragment activation and provider checks are
    /// preserved. When `sources` is present this returns the first declared source path.
    pub fn source_path(&self) -> Option<&str> {
        self.sources
            .first()
            .map(|source| source.path.as_str())
            .or(self.source.as_ref().map(|source| source.path.as_str()))
    }

    pub fn source_paths(&self) -> impl Iterator<Item = &str> {
        self.source
            .iter()
            .map(|source| source.path.as_str())
            .chain(self.sources.iter().map(|source| source.path.as_str()))
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

    pub fn is_asset_scalar(&self) -> bool {
        self.primitive_type() == Some(PrimitiveFieldType::Asset)
    }

    pub fn is_color_scalar(&self) -> bool {
        self.primitive_type() == Some(PrimitiveFieldType::Color)
    }

    pub fn vector_dimension(&self) -> Option<usize> {
        self.primitive_type()?.vector_dimension()
    }

    pub fn vector_component_names(&self) -> Vec<String> {
        self.vector_components
            .clone()
            .unwrap_or_else(|| default_vector_component_names(self.vector_dimension().unwrap_or(4)))
    }

    pub fn has_color_options(&self) -> bool {
        self.color_format.is_some()
            || self.color_channel.is_some()
            || self.color_space.is_some()
            || self.color_alpha.is_some()
    }

    pub fn has_vector_options(&self) -> bool {
        self.vector_format.is_some() || self.vector_components.is_some() || self.unit.is_some()
    }

    pub fn foreign_key_scalar_type_name(&self) -> &str {
        scalar_field_type_name(&self.field_type)
    }

    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }
}

pub fn default_vector_component_names(dimension: usize) -> Vec<String> {
    ["x", "y", "z", "w"]
        .into_iter()
        .take(dimension)
        .map(str::to_owned)
        .collect()
}

pub fn list_element_type_name(field_type: &str) -> Option<&str> {
    field_type
        .strip_suffix("[]")
        .or_else(|| field_type.strip_prefix("list<")?.strip_suffix('>'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn scalar_field_type_name(field_type: &str) -> &str {
    list_element_type_name(field_type).unwrap_or(field_type)
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
