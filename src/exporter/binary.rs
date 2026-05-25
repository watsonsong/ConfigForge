// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Result,
    data::{DataResolver, DataTable, DataValue, ResolvedValue, ScalarValue},
    meta::{
        LinkedField, LinkedFieldMeta, LinkedFragmentMeta, LinkedStructType, LinkedTableMeta,
        PrimitiveFieldType, TableKind, TargetTags,
    },
};

use super::DataFormatExporter;

pub const FORMAT: &str = "bin";

const MAGIC: &[u8; 4] = b"CFGB";
const FORMAT_VERSION: u16 = 1;
const HEADER_SIZE: u16 = 16;
const CONTRACT_TOKEN: &str = "config-forge-bin-v1";

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
        let mut encoder = BinaryEncoder::new(table_meta, target_tags);
        encoder.write_header();
        match table_meta.meta.kind {
            TableKind::Collection => encoder.write_collection(table)?,
            TableKind::Object => encoder.write_object(table)?,
        }
        Ok(encoder.finish())
    }
}

pub(crate) fn schema_revision(table_meta: &LinkedTableMeta<'_>, target_tags: &TargetTags) -> u64 {
    let mut signature = Vec::new();
    write_token(&mut signature, 0x01, |payload| {
        write_string(payload, CONTRACT_TOKEN)
    });
    write_token(&mut signature, 0x02, |payload| {
        write_string(payload, &table_meta.meta.name);
        payload.push(match table_meta.meta.kind {
            TableKind::Object => 0,
            TableKind::Collection => 1,
        });
    });
    for field in table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
    {
        write_field_signature(&mut signature, field, target_tags);
    }
    for fragment in table_meta
        .fragments
        .iter()
        .filter(|fragment| fragment.meta.is_exported_for(target_tags))
    {
        write_fragment_signature(&mut signature, fragment, target_tags);
    }
    fnv1a64(&signature)
}

struct BinaryEncoder<'a, 'meta> {
    table_meta: &'a LinkedTableMeta<'meta>,
    target_tags: &'a TargetTags,
    bytes: Vec<u8>,
}

impl<'a, 'meta> BinaryEncoder<'a, 'meta> {
    fn new(table_meta: &'a LinkedTableMeta<'meta>, target_tags: &'a TargetTags) -> Self {
        Self {
            table_meta,
            target_tags,
            bytes: Vec::new(),
        }
    }

    fn write_header(&mut self) {
        self.bytes.extend_from_slice(MAGIC);
        self.bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        self.bytes.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        self.bytes
            .extend_from_slice(&schema_revision(self.table_meta, self.target_tags).to_le_bytes());
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
        write_varuint(&mut self.bytes, visible_row_indices.len() as u64);
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
        let optional_slot_count =
            fields.iter().filter(|field| !field.meta.required).count() + fragments.len();
        let mut bitmap = vec![0u8; optional_slot_count.div_ceil(8)];
        let mut optional_index = 0usize;
        for field in &fields {
            if !field.meta.required {
                if values.contains_key(&field.meta.name) {
                    set_bit(&mut bitmap, optional_index);
                }
                optional_index += 1;
            }
        }
        for fragment in &fragments {
            if fragment_present(fragment, values, active_fragments) {
                set_bit(&mut bitmap, optional_index);
            }
            optional_index += 1;
        }
        self.bytes.extend_from_slice(&bitmap);

        for field in fields {
            match values.get(&field.meta.name) {
                Some(value) => self.write_field_value(field, value)?,
                None if field.meta.required => {
                    return Err(ConfigForgeError::Validation(format!(
                        "binary export missing required field `{}.{}`",
                        self.table_meta.meta.name, field.meta.name
                    )));
                }
                None => {
                    return Err(ConfigForgeError::Validation(format!(
                        "binary export missing optional field `{}.{}`; optional scalar/object fields are not supported by current generated binary readers",
                        self.table_meta.meta.name, field.meta.name
                    )));
                }
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
        let (DataValue::Struct(children) | DataValue::Map(children)) = &value.value else {
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
        let optional_slot_count = visible_fields
            .iter()
            .filter(|field| !field.meta.required)
            .count();
        let mut bitmap = vec![0u8; optional_slot_count.div_ceil(8)];
        let mut optional_index = 0usize;
        for field in &visible_fields {
            if !field.meta.required {
                if values.contains_key(&field.meta.name) {
                    set_bit(&mut bitmap, optional_index);
                }
                optional_index += 1;
            }
        }
        self.bytes.extend_from_slice(&bitmap);
        for field in visible_fields {
            match values.get(&field.meta.name) {
                Some(value) => self.write_field_value(field, value)?,
                None if field.meta.required => {
                    return Err(ConfigForgeError::Validation(format!(
                        "binary export missing required field `{}` in {}",
                        field.meta.name, context
                    )));
                }
                None => {
                    return Err(ConfigForgeError::Validation(format!(
                        "binary export missing optional field `{}` in {}; optional scalar/object fields are not supported by current generated binary readers",
                        field.meta.name, context
                    )));
                }
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
        match (primitive, scalar) {
            (PrimitiveFieldType::Int, ScalarValue::Int(value)) => {
                write_varuint(&mut self.bytes, zigzag(*value));
                Ok(())
            }
            (PrimitiveFieldType::Float, ScalarValue::Float(value)) => {
                if !value.is_finite() {
                    return Err(ConfigForgeError::Validation(format!(
                        "binary export field `{}` contains non-finite float `{}`",
                        field.meta.name, value
                    )));
                }
                self.bytes.extend_from_slice(&value.to_le_bytes());
                Ok(())
            }
            (PrimitiveFieldType::Bool, ScalarValue::Bool(value)) => {
                self.bytes.push(u8::from(*value));
                Ok(())
            }
            (
                PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
                ScalarValue::String(value),
            ) => {
                write_bytes(&mut self.bytes, value.as_bytes());
                Ok(())
            }
            _ => Err(ConfigForgeError::Validation(format!(
                "binary export field `{}` has scalar incompatible with `{}`",
                field.meta.name,
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
        let ordinal = enum_meta
            .values
            .iter()
            .position(|enum_value| enum_value.name == *value)
            .ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "binary export field `{}` value `{}` is not a member of enum `{}`",
                    field.meta.name, value, enum_meta.name
                ))
            })?;
        write_varuint(&mut self.bytes, ordinal as u64);
        Ok(())
    }

    fn write_struct(
        &mut self,
        struct_type: &LinkedStructType<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let (DataValue::Struct(children) | DataValue::Map(children)) = &value.value else {
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

    fn finish(self) -> Vec<u8> {
        self.bytes
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

fn expected_scalar(field: &LinkedFieldMeta<'_>) -> ConfigForgeError {
    ConfigForgeError::Validation(format!(
        "binary export field `{}` expected scalar value",
        field.meta.name
    ))
}

fn set_bit(bitmap: &mut [u8], index: usize) {
    bitmap[index / 8] |= 1 << (index % 8);
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn write_varuint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    write_varuint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    write_bytes(out, value.as_bytes());
}

fn write_token(out: &mut Vec<u8>, token_kind: u8, write_payload: impl FnOnce(&mut Vec<u8>)) {
    let mut payload = Vec::new();
    write_payload(&mut payload);
    out.push(token_kind);
    write_varuint(out, payload.len() as u64);
    out.extend_from_slice(&payload);
}

fn write_field_signature(out: &mut Vec<u8>, field: &LinkedFieldMeta<'_>, target_tags: &TargetTags) {
    write_token(out, 0x10, |payload| {
        write_string(payload, &field.meta.name);
        payload.push(u8::from(field.meta.required));
        write_type_signature(payload, &field.linked_field, target_tags);
    });
}

fn write_fragment_signature(
    out: &mut Vec<u8>,
    fragment: &LinkedFragmentMeta<'_>,
    target_tags: &TargetTags,
) {
    write_token(out, 0x11, |payload| {
        write_string(payload, &fragment.meta.name);
        payload.push(0); // fragments are represented as optional table-record slots in bin v1.
        match &fragment.payload {
            crate::meta::LinkedFragmentPayload::InlineFields => {
                payload.push(0);
                write_varuint(
                    payload,
                    visible_field_count(&fragment.fields, target_tags) as u64,
                );
                for field in fragment
                    .fields
                    .iter()
                    .filter(|field| field.meta.is_exported_for(target_tags))
                {
                    write_field_signature(payload, field, target_tags);
                }
            }
            crate::meta::LinkedFragmentPayload::Struct(struct_type) => {
                payload.push(1);
                write_struct_signature(payload, struct_type, target_tags);
            }
        }
    });
}

fn write_type_signature(out: &mut Vec<u8>, field: &LinkedField<'_>, target_tags: &TargetTags) {
    match field {
        LinkedField::Primitive(primitive) => {
            out.push(0x01);
            out.push(match primitive {
                PrimitiveFieldType::Int => 0,
                PrimitiveFieldType::Float => 1,
                PrimitiveFieldType::Bool => 2,
                PrimitiveFieldType::String => 3,
                PrimitiveFieldType::Text => 4,
                PrimitiveFieldType::Asset => 5,
            });
        }
        LinkedField::Enum(enum_meta) => {
            out.push(0x02);
            write_string(out, &enum_meta.name);
            write_varuint(out, enum_meta.values.len() as u64);
            for value in &enum_meta.values {
                write_string(out, &value.name);
            }
        }
        LinkedField::Struct(struct_type) => {
            out.push(0x03);
            write_struct_signature(out, struct_type, target_tags);
        }
    }
}

fn write_struct_signature(
    out: &mut Vec<u8>,
    struct_type: &LinkedStructType<'_>,
    target_tags: &TargetTags,
) {
    write_string(out, &struct_type.meta.name);
    write_varuint(
        out,
        visible_field_count(&struct_type.fields, target_tags) as u64,
    );
    for field in struct_type
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
    {
        write_field_signature(out, field, target_tags);
    }
}

fn visible_field_count(fields: &[LinkedFieldMeta<'_>], target_tags: &TargetTags) -> usize {
    fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .count()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 14_695_981_039_346_656_037u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        data::DataTable,
        exporter::DataFormatExporter,
        meta::{MetaModel, TargetTags},
    };

    use super::{BinaryDataExporter, FORMAT_VERSION, HEADER_SIZE, MAGIC, schema_revision};

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
            schema_revision(table_meta, &TargetTags::default())
        );
        assert_eq!(bytes[16], 1); // row_count
        assert_eq!(bytes[17], 0b0000_0001); // id required, name optional present
        assert_eq!(bytes[18], 14); // zigzag(7)
        assert_eq!(bytes[19], 6);
        assert_eq!(&bytes[20..26], b"Potion");
    }
}
