// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    artifact::ArtifactWriteStream,
    data::ResolvedValue,
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
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
    binary_value_error(
        Some(field.meta.name.as_str()),
        format!(
            "binary export field `{}` expected scalar value",
            field.meta.name
        ),
        "non-scalar runtime value",
        "scalar runtime value",
        "Ensure resolved data field values match the linked meta field type before binary export.",
    )
}

pub(super) fn ensure_javascript_safe_int(field_name: &str, value: i64) -> Result<()> {
    if (JAVASCRIPT_SAFE_INT_MIN..=JAVASCRIPT_SAFE_INT_MAX).contains(&value) {
        return Ok(());
    }
    Err(binary_value_error(
        Some(field_name),
        format!(
            "binary export field `{}` int value `{}` exceeds JavaScript safe integer range [{}, {}]",
            field_name, value, JAVASCRIPT_SAFE_INT_MIN, JAVASCRIPT_SAFE_INT_MAX
        ),
        value.to_string(),
        format!(
            "integer in JavaScript safe range [{}, {}]",
            JAVASCRIPT_SAFE_INT_MIN, JAVASCRIPT_SAFE_INT_MAX
        ),
        "Use an int value that can be represented exactly by generated JavaScript-compatible runtimes.",
    ))
}

pub(super) fn binary_value_error(
    field: Option<impl Into<String>>,
    message: String,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation {
                field: field.map(Into::into),
                ..Default::default()
            },
        )
        .with_source("generator.binary.encoder")
        .with_domain("generator")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint(hint),
    )
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
