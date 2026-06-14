// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::meta::{
    LinkedField, LinkedFieldMeta, LinkedFragmentMeta, LinkedStructType, LinkedTableMeta,
    PrimitiveFieldType, TableKind, TargetTags,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDataFile {
    pub table_name: String,
    pub path: String,
    pub format: String,
}

pub(crate) const DEFAULT_RUNTIME_DATA_FORMAT: &str = "json";

const BINARY_CONTRACT_TOKEN: &str = "config-forge-bin-v1";

pub(crate) fn binary_schema_revision(
    table_meta: &LinkedTableMeta<'_>,
    target_tags: &TargetTags,
) -> u64 {
    let mut signature = Vec::new();
    write_token(&mut signature, 0x01, |payload| {
        write_string(payload, BINARY_CONTRACT_TOKEN)
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
                PrimitiveFieldType::Color => 6,
                PrimitiveFieldType::Vector2 => 7,
                PrimitiveFieldType::Vector3 => 8,
                PrimitiveFieldType::Vector4 => 9,
                PrimitiveFieldType::Vector2Int => 10,
                PrimitiveFieldType::Vector3Int => 11,
                PrimitiveFieldType::Vector4Int => 12,
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
        LinkedField::List(element_type) => {
            out.push(0x04);
            write_type_signature(out, element_type, target_tags);
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
