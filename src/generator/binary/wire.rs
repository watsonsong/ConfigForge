// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Result,
    artifact::ArtifactWriteStream,
    data::ResolvedValue,
    meta::{LinkedFieldMeta, LinkedFragmentMeta},
};

pub(super) const MAGIC: &[u8; 4] = b"CFGB";
pub(super) const FORMAT_VERSION: u16 = 1;
pub(super) const HEADER_SIZE: u16 = 16;

const JAVASCRIPT_SAFE_INT_MIN: i64 = -(1_i64 << 53) + 1;
const JAVASCRIPT_SAFE_INT_MAX: i64 = (1_i64 << 53) - 1;

pub(super) trait BinaryWriteTarget {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PresenceSlot {
    pub(super) optional: bool,
    pub(super) present: bool,
}

impl PresenceSlot {
    pub(super) fn field(
        field: &LinkedFieldMeta<'_>,
        values: &BTreeMap<String, ResolvedValue>,
    ) -> Self {
        Self {
            optional: !field.meta.required,
            present: values.contains_key(&field.meta.name),
        }
    }

    pub(super) fn fragment(
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

pub(super) fn fragment_present(
    fragment: &LinkedFragmentMeta<'_>,
    values: &BTreeMap<String, ResolvedValue>,
    active_fragments: Option<&Vec<String>>,
) -> bool {
    let source_active = active_fragments
        .map(|fragments| fragments.iter().any(|name| name == &fragment.meta.name))
        .unwrap_or(true);
    source_active && values.contains_key(&fragment.meta.name)
}

pub(super) fn write_optional_bitmap<W: BinaryWriteTarget>(
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

pub(super) fn expected_scalar(field: &LinkedFieldMeta<'_>) -> ConfigForgeError {
    ConfigForgeError::Validation(format!(
        "binary export field `{}` expected scalar value",
        field.meta.name
    ))
}

pub(super) fn ensure_javascript_safe_int(field_name: &str, value: i64) -> Result<()> {
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

pub(super) fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

pub(super) fn write_varuint<W: BinaryWriteTarget>(out: &mut W, mut value: u64) -> Result<()> {
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

pub(super) fn write_bytes<W: BinaryWriteTarget>(out: &mut W, bytes: &[u8]) -> Result<()> {
    write_varuint(out, bytes.len() as u64)?;
    out.write_binary_bytes(bytes)
}
