// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Result,
    artifact::ArtifactWriteStream,
    data::{ArrayElementValue, DataResolver, DataTable, DataValue, ResolvedValue, ScalarValue},
    meta::{
        LinkedField, LinkedFieldMeta, LinkedFragmentMeta, LinkedStructType, LinkedTableMeta,
        PrimitiveFieldType, TableKind, TargetTags,
    },
    runtime_contract::binary_schema_revision,
};

use super::DataFormatExporter;

pub const FORMAT: &str = "bin";

const MAGIC: &[u8; 4] = b"CFGB";
const FORMAT_VERSION: u16 = 1;
const HEADER_SIZE: u16 = 16;
const JAVASCRIPT_SAFE_INT_MIN: i64 = -(1_i64 << 53) + 1;
const JAVASCRIPT_SAFE_INT_MAX: i64 = (1_i64 << 53) - 1;

pub struct BinaryDataExporter;

impl DataFormatExporter for BinaryDataExporter {
    fn name(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["binary"]
    }

    fn extension(&self) -> &'static str {
        "bin"
    }

    fn serialize_table(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
    ) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut encoder = BinaryEncoder::new(table_meta, target_tags, &mut bytes);
        encoder.write_table(table)?;
        Ok(bytes)
    }

    fn serialize_table_to_stream(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
        stream: &mut ArtifactWriteStream,
    ) -> Result<()> {
        BinaryEncoder::new(table_meta, target_tags, stream).write_table(table)
    }
}

trait BinaryWriteTarget {
    fn write_binary_bytes(&mut self, bytes: &[u8]) -> Result<()>;
}

impl BinaryWriteTarget for Vec<u8> {
    fn write_binary_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

impl BinaryWriteTarget for ArtifactWriteStream {
    fn write_binary_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.write_all_artifact(bytes)
    }
}

struct BinaryEncoder<'a, 'meta, W: BinaryWriteTarget> {
    table_meta: &'a LinkedTableMeta<'meta>,
    target_tags: &'a TargetTags,
    out: &'a mut W,
}

impl<'a, 'meta, W: BinaryWriteTarget> BinaryEncoder<'a, 'meta, W> {
    fn new(
        table_meta: &'a LinkedTableMeta<'meta>,
        target_tags: &'a TargetTags,
        out: &'a mut W,
    ) -> Self {
        Self {
            table_meta,
            target_tags,
            out,
        }
    }

    fn write_table(&mut self, table: &DataTable) -> Result<()> {
        self.write_header()?;
        match self.table_meta.meta.kind {
            TableKind::Collection => self.write_collection(table),
            TableKind::Object => self.write_object(table),
        }
    }

    fn write_header(&mut self) -> Result<()> {
        self.out.write_binary_bytes(MAGIC)?;
        self.out.write_binary_bytes(&FORMAT_VERSION.to_le_bytes())?;
        self.out.write_binary_bytes(&HEADER_SIZE.to_le_bytes())?;
        self.out.write_binary_bytes(
            &binary_schema_revision(self.table_meta, self.target_tags).to_le_bytes(),
        )
    }

    fn write_collection(&mut self, table: &DataTable) -> Result<()> {
        let visible_row_indices = table
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                self.table_meta
                    .meta
                    .is_row_exported_for(self.target_tags, |field| {
                        row.get(field).map(String::as_str)
                    })
            })
            .map(|(row_index, _)| row_index)
            .collect::<Vec<_>>();
        let resolved_table = DataResolver::resolve_table_rows(
            self.table_meta,
            table,
            Some(self.target_tags),
            visible_row_indices.iter().copied(),
        )?;
        write_varuint(self.out, visible_row_indices.len() as u64)?;
        for (resolved_row_index, row_index) in visible_row_indices.iter().enumerate() {
            let resolved_row = resolved_table.rows.get(resolved_row_index).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "resolved table `{}` is missing row {}",
                    self.table_meta.meta.name,
                    resolved_row_index + 1
                ))
            })?;
            self.write_table_record(
                resolved_row,
                table.row_source(*row_index).map(|source| &source.fragments),
            )?;
        }
        Ok(())
    }

    fn write_object(&mut self, table: &DataTable) -> Result<()> {
        if table.rows.len() != 1 {
            return Err(ConfigForgeError::Validation(format!(
                "object table `{}` must normalize to exactly one logical row, got {}",
                self.table_meta.meta.name,
                table.rows.len()
            )));
        }

        let Some(row) = table.rows.first() else {
            return Ok(());
        };
        if !self
            .table_meta
            .meta
            .is_row_exported_for(self.target_tags, |field| row.get(field).map(String::as_str))
        {
            if self.has_visible_slots() {
                return Err(ConfigForgeError::Validation(format!(
                    "binary export object table `{}` row is filtered out but visible fields/fragments require a singleton record",
                    self.table_meta.meta.name
                )));
            }
            return Ok(());
        }
        let resolved_table =
            DataResolver::resolve_table(self.table_meta, table, Some(self.target_tags))?;
        let resolved_row = resolved_table.rows.first().ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "resolved object table `{}` is missing singleton row",
                self.table_meta.meta.name
            ))
        })?;
        if self.has_visible_slots() {
            self.write_table_record(resolved_row, None)?;
        }
        Ok(())
    }

    fn has_visible_slots(&self) -> bool {
        self.table_meta
            .fields
            .iter()
            .any(|field| field.meta.is_exported_for(self.target_tags))
            || self
                .table_meta
                .fragments
                .iter()
                .any(|fragment| fragment.meta.is_exported_for(self.target_tags))
    }

    fn write_table_record(
        &mut self,
        values: &BTreeMap<String, ResolvedValue>,
        active_fragments: Option<&Vec<String>>,
    ) -> Result<()> {
        let fields = self
            .table_meta
            .fields
            .iter()
            .filter(|field| field.meta.is_exported_for(self.target_tags))
            .collect::<Vec<_>>();
        let fragments = self
            .table_meta
            .fragments
            .iter()
            .filter(|fragment| fragment.meta.is_exported_for(self.target_tags))
            .collect::<Vec<_>>();
        let presence_slots = fields
            .iter()
            .map(|field| PresenceSlot::field(field, values))
            .chain(
                fragments
                    .iter()
                    .map(|fragment| PresenceSlot::fragment(fragment, values, active_fragments)),
            );
        write_optional_bitmap(self.out, presence_slots)?;

        for field in fields {
            match values.get(&field.meta.name) {
                Some(value) => self.write_field_value(field, value)?,
                None if field.meta.required => {
                    return Err(ConfigForgeError::Validation(format!(
                        "binary export missing required field `{}.{}`",
                        self.table_meta.meta.name, field.meta.name
                    )));
                }
                None => {}
            }
        }
        for fragment in fragments {
            if !fragment_present(fragment, values, active_fragments) {
                continue;
            }
            let value = values.get(&fragment.meta.name).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "binary export missing present fragment `{}.{}`",
                    self.table_meta.meta.name, fragment.meta.name
                ))
            })?;
            self.write_fragment_record(fragment, value)?;
        }
        Ok(())
    }

    fn write_fragment_record(
        &mut self,
        fragment: &LinkedFragmentMeta<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let DataValue::Struct(children) = &value.value else {
            return Err(ConfigForgeError::Validation(format!(
                "binary export expected struct value for fragment `{}`",
                fragment.meta.name
            )));
        };
        self.write_record(
            &fragment.fields,
            children,
            &format!("fragment `{}`", fragment.meta.name),
        )
    }

    fn write_record(
        &mut self,
        fields: &[LinkedFieldMeta<'meta>],
        values: &BTreeMap<String, ResolvedValue>,
        context: &str,
    ) -> Result<()> {
        let visible_fields = fields
            .iter()
            .filter(|field| field.meta.is_exported_for(self.target_tags))
            .collect::<Vec<_>>();
        write_optional_bitmap(
            self.out,
            visible_fields
                .iter()
                .map(|field| PresenceSlot::field(field, values)),
        )?;
        for field in visible_fields {
            match values.get(&field.meta.name) {
                Some(value) => self.write_field_value(field, value)?,
                None if field.meta.required => {
                    return Err(ConfigForgeError::Validation(format!(
                        "binary export missing required field `{}` in {}",
                        field.meta.name, context
                    )));
                }
                None => {}
            }
        }
        Ok(())
    }

    fn write_field_value(
        &mut self,
        field: &LinkedFieldMeta<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        match &field.linked_field {
            LinkedField::Primitive(primitive) => self.write_primitive(*primitive, field, value),
            LinkedField::Enum(enum_meta) => self.write_enum(enum_meta, field, value),
            LinkedField::Struct(struct_type) => self.write_struct(struct_type, value),
            LinkedField::List(element_type) => self.write_list(element_type, value),
        }
    }

    fn write_list(
        &mut self,
        element_type: &LinkedField<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let DataValue::Array(elements) = &value.value else {
            return Err(ConfigForgeError::Validation(
                "binary export expected list value".to_owned(),
            ));
        };
        write_varuint(self.out, elements.len() as u64)?;
        for element in elements {
            self.write_array_element_for_type(element_type, element)?;
        }
        Ok(())
    }

    fn write_array_element_for_type(
        &mut self,
        field_type: &LinkedField<'meta>,
        value: &ArrayElementValue,
    ) -> Result<()> {
        match field_type {
            LinkedField::Primitive(primitive) => {
                self.write_primitive_array_element(*primitive, value)
            }
            LinkedField::Enum(enum_meta) => self.write_enum_array_element(enum_meta, value),
            LinkedField::Struct(struct_type) => self.write_struct_array_element(struct_type, value),
            LinkedField::List(element_type) => {
                self.write_nested_list_array_element(element_type, value)
            }
        }
    }

    fn write_primitive(
        &mut self,
        primitive: PrimitiveFieldType,
        field: &LinkedFieldMeta<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let DataValue::Scalar(scalar) = &value.value else {
            return Err(expected_scalar(field));
        };
        self.write_scalar(primitive, scalar, field.meta.name.as_str())
    }

    fn write_primitive_array_element(
        &mut self,
        primitive: PrimitiveFieldType,
        value: &ArrayElementValue,
    ) -> Result<()> {
        let DataValue::Scalar(scalar) = &value.value else {
            return Err(ConfigForgeError::Validation(
                "binary export expected scalar list element".to_owned(),
            ));
        };
        self.write_scalar(primitive, scalar, "<list element>")
    }

    fn write_scalar(
        &mut self,
        primitive: PrimitiveFieldType,
        scalar: &ScalarValue,
        field_name: &str,
    ) -> Result<()> {
        match (primitive, scalar) {
            (PrimitiveFieldType::Int, ScalarValue::Int(value)) => {
                ensure_javascript_safe_int(field_name, *value)?;
                write_varuint(self.out, zigzag(*value))?;
                Ok(())
            }
            (PrimitiveFieldType::Float, ScalarValue::Float(value)) => {
                if !value.is_finite() {
                    return Err(ConfigForgeError::Validation(format!(
                        "binary export field `{}` contains non-finite float `{}`",
                        field_name, value
                    )));
                }
                self.out.write_binary_bytes(&value.to_le_bytes())?;
                Ok(())
            }
            (PrimitiveFieldType::Bool, ScalarValue::Bool(value)) => {
                self.out.write_binary_bytes(&[u8::from(*value)])?;
                Ok(())
            }
            (
                PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
                ScalarValue::String(value),
            ) => {
                write_bytes(self.out, value.as_bytes())?;
                Ok(())
            }
            _ => Err(ConfigForgeError::Validation(format!(
                "binary export field `{}` has scalar incompatible with `{}`",
                field_name,
                primitive.as_str()
            ))),
        }
    }

    fn write_enum(
        &mut self,
        enum_meta: &crate::meta::EnumMeta,
        field: &LinkedFieldMeta<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let DataValue::Scalar(ScalarValue::Enum(value)) = &value.value else {
            return Err(expected_scalar(field));
        };
        self.write_enum_value(enum_meta, value, field.meta.name.as_str())
    }

    fn write_enum_array_element(
        &mut self,
        enum_meta: &crate::meta::EnumMeta,
        value: &ArrayElementValue,
    ) -> Result<()> {
        let DataValue::Scalar(ScalarValue::Enum(value)) = &value.value else {
            return Err(ConfigForgeError::Validation(
                "binary export expected enum list element".to_owned(),
            ));
        };
        self.write_enum_value(enum_meta, value, "<list element>")
    }

    fn write_enum_value(
        &mut self,
        enum_meta: &crate::meta::EnumMeta,
        value: &str,
        field_name: &str,
    ) -> Result<()> {
        let ordinal = enum_meta
            .values
            .iter()
            .position(|enum_value| enum_value.name == value)
            .ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "binary export field `{}` value `{}` is not a member of enum `{}`",
                    field_name, value, enum_meta.name
                ))
            })?;
        write_varuint(self.out, ordinal as u64)?;
        Ok(())
    }

    fn write_struct(
        &mut self,
        struct_type: &LinkedStructType<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let DataValue::Struct(children) = &value.value else {
            return Err(ConfigForgeError::Validation(format!(
                "binary export expected struct `{}` value",
                struct_type.meta.name
            )));
        };
        self.write_record(
            &struct_type.fields,
            children,
            &format!("struct `{}`", struct_type.meta.name),
        )
    }

    fn write_struct_array_element(
        &mut self,
        struct_type: &LinkedStructType<'meta>,
        value: &ArrayElementValue,
    ) -> Result<()> {
        let DataValue::Struct(children) = &value.value else {
            return Err(ConfigForgeError::Validation(format!(
                "binary export expected struct `{}` list element",
                struct_type.meta.name
            )));
        };
        self.write_record(
            &struct_type.fields,
            children,
            &format!("struct `{}` list element", struct_type.meta.name),
        )
    }

    fn write_nested_list_array_element(
        &mut self,
        element_type: &LinkedField<'meta>,
        value: &ArrayElementValue,
    ) -> Result<()> {
        let DataValue::Array(elements) = &value.value else {
            return Err(ConfigForgeError::Validation(
                "binary export expected nested list element".to_owned(),
            ));
        };
        write_varuint(self.out, elements.len() as u64)?;
        for element in elements {
            self.write_array_element_for_type(element_type, element)?;
        }
        Ok(())
    }
}

fn fragment_present(
    fragment: &LinkedFragmentMeta<'_>,
    values: &BTreeMap<String, ResolvedValue>,
    active_fragments: Option<&Vec<String>>,
) -> bool {
    let source_active = active_fragments
        .map(|fragments| fragments.iter().any(|name| name == &fragment.meta.name))
        .unwrap_or(true);
    source_active && values.contains_key(&fragment.meta.name)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PresenceSlot {
    optional: bool,
    present: bool,
}

impl PresenceSlot {
    fn field(field: &LinkedFieldMeta<'_>, values: &BTreeMap<String, ResolvedValue>) -> Self {
        Self {
            optional: !field.meta.required,
            present: values.contains_key(&field.meta.name),
        }
    }

    fn fragment(
        fragment: &LinkedFragmentMeta<'_>,
        values: &BTreeMap<String, ResolvedValue>,
        active_fragments: Option<&Vec<String>>,
    ) -> Self {
        Self {
            optional: true,
            present: fragment_present(fragment, values, active_fragments),
        }
    }
}

fn write_optional_bitmap<W: BinaryWriteTarget>(
    out: &mut W,
    slots: impl IntoIterator<Item = PresenceSlot>,
) -> Result<()> {
    let optional_slots = slots
        .into_iter()
        .filter(|slot| slot.optional)
        .collect::<Vec<_>>();
    let mut bitmap = vec![0u8; optional_slots.len().div_ceil(8)];
    for (optional_index, slot) in optional_slots.iter().enumerate() {
        if slot.present {
            set_bit(&mut bitmap, optional_index);
        }
    }
    out.write_binary_bytes(&bitmap)
}

fn expected_scalar(field: &LinkedFieldMeta<'_>) -> ConfigForgeError {
    ConfigForgeError::Validation(format!(
        "binary export field `{}` expected scalar value",
        field.meta.name
    ))
}

fn ensure_javascript_safe_int(field_name: &str, value: i64) -> Result<()> {
    if (JAVASCRIPT_SAFE_INT_MIN..=JAVASCRIPT_SAFE_INT_MAX).contains(&value) {
        return Ok(());
    }
    Err(ConfigForgeError::Validation(format!(
        "binary export field `{}` int value `{}` exceeds JavaScript safe integer range [{}, {}]",
        field_name, value, JAVASCRIPT_SAFE_INT_MIN, JAVASCRIPT_SAFE_INT_MAX
    )))
}

fn set_bit(bitmap: &mut [u8], index: usize) {
    bitmap[index / 8] |= 1 << (index % 8);
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn write_varuint<W: BinaryWriteTarget>(out: &mut W, mut value: u64) -> Result<()> {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.write_binary_bytes(&[byte])?;
        if value == 0 {
            break Ok(());
        }
    }
}

fn write_bytes<W: BinaryWriteTarget>(out: &mut W, bytes: &[u8]) -> Result<()> {
    write_varuint(out, bytes.len() as u64)?;
    out.write_binary_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        data::DataTable,
        exporter::DataFormatExporter,
        meta::{MetaModel, TargetTags},
    };

    use super::{
        BinaryDataExporter, FORMAT_VERSION, HEADER_SIZE, MAGIC, PresenceSlot, write_optional_bitmap,
    };
    use crate::runtime_contract::binary_schema_revision;

    #[test]
    fn optional_bitmap_skips_required_slots_and_packs_optional_presence() {
        let mut bytes = Vec::new();

        write_optional_bitmap(
            &mut bytes,
            [
                PresenceSlot {
                    optional: false,
                    present: true,
                },
                PresenceSlot {
                    optional: true,
                    present: true,
                },
                PresenceSlot {
                    optional: true,
                    present: false,
                },
                PresenceSlot {
                    optional: true,
                    present: true,
                },
            ],
        )
        .expect("bitmap should write");

        assert_eq!(bytes, vec![0b0000_0101]);
    }

    #[test]
    fn optional_bitmap_extends_to_multiple_bytes() {
        let mut bytes = Vec::new();
        let slots = (0..10).map(|index| PresenceSlot {
            optional: true,
            present: index == 0 || index == 8 || index == 9,
        });

        write_optional_bitmap(&mut bytes, slots).expect("bitmap should write");

        assert_eq!(bytes, vec![0b0000_0001, 0b0000_0011]);
    }

    #[test]
    fn writes_header_and_collection_rows() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "name"
                type = "string"
                required = false
            "#,
        )
        .unwrap();
        let linked = meta.link().unwrap();
        let table_meta = linked.table("items").unwrap();
        let table = DataTable::new(
            "items",
            vec![BTreeMap::from([
                ("id".to_owned(), "7".to_owned()),
                ("name".to_owned(), "Potion".to_owned()),
            ])],
        );

        let bytes = BinaryDataExporter
            .serialize_table(table_meta, &table, &TargetTags::default())
            .unwrap();

        assert_eq!(&bytes[0..4], MAGIC);
        assert_eq!(
            u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
            FORMAT_VERSION
        );
        assert_eq!(
            u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
            HEADER_SIZE
        );
        assert_eq!(
            u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            binary_schema_revision(table_meta, &TargetTags::default())
        );
        assert_eq!(bytes[16], 1); // row_count
        assert_eq!(bytes[17], 0b0000_0001); // id required, name optional present
        assert_eq!(bytes[18], 14); // zigzag(7)
        assert_eq!(bytes[19], 6);
        assert_eq!(&bytes[20..26], b"Potion");
    }
}
