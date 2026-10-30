// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use config_forge_runtime_reader_core::{
    RuntimeDataError, RuntimeDataLoader, RuntimeDataReader, RuntimeDataResult, RuntimeDecodeLimits,
    read_runtime_file,
};

pub struct BinaryRuntimeDataReader {
    state: Rc<RefCell<BinaryState>>,
    limits: RuntimeDecodeLimits,
}

struct BinaryState {
    bytes: Vec<u8>,
    offset: usize,
    schema_revision: u64,
    next_row_index: usize,
    records: Vec<RecordState>,
}

struct RecordState {
    bitmap: Vec<u8>,
    next_optional_slot: usize,
}

pub struct FsBinaryRuntimeDataLoader {
    root: PathBuf,
    limits: RuntimeDecodeLimits,
}

impl BinaryRuntimeDataReader {
    pub fn from_bytes(bytes: Vec<u8>) -> RuntimeDataResult<Self> {
        Self::from_bytes_with_limits(bytes, RuntimeDecodeLimits::default())
    }

    pub fn from_bytes_with_limits(
        bytes: Vec<u8>,
        limits: RuntimeDecodeLimits,
    ) -> RuntimeDataResult<Self> {
        limits.check_file_bytes(bytes.len())?;
        if bytes.len() < 16 {
            return Err(RuntimeDataError::decode(
                "truncated binary runtime data header",
            ));
        }
        if &bytes[0..4] != b"CFGB" {
            return Err(RuntimeDataError::decode(
                "invalid binary runtime data magic",
            ));
        }
        let version = u16::from_le_bytes(bytes[4..6].try_into().expect("header slice"));
        if version != 1 {
            return Err(RuntimeDataError::decode(format!(
                "unsupported binary runtime data version `{version}`"
            )));
        }
        let header_size = u16::from_le_bytes(bytes[6..8].try_into().expect("header slice"));
        if header_size != 16 {
            return Err(RuntimeDataError::decode(format!(
                "unsupported binary runtime data header size `{header_size}`"
            )));
        }
        let schema_revision = u64::from_le_bytes(bytes[8..16].try_into().expect("header slice"));
        Ok(Self {
            state: Rc::new(RefCell::new(BinaryState {
                bytes,
                offset: 16,
                schema_revision,
                next_row_index: 0,
                records: Vec::new(),
            })),
            limits,
        })
    }
}

impl RuntimeDataReader for BinaryRuntimeDataReader {
    fn enter_required_field(&self, _name: &'static str) -> RuntimeDataResult<()> {
        Ok(())
    }

    fn enter_optional_field(&self, _name: &'static str) -> RuntimeDataResult<bool> {
        self.read_optional_present()
    }

    fn exit_field(&self) -> RuntimeDataResult<()> {
        Ok(())
    }

    fn enter_index(&self, index: usize) -> RuntimeDataResult<()> {
        let mut state = self.state.borrow_mut();
        if index != state.next_row_index {
            return Err(RuntimeDataError::decode(format!(
                "binary reader rows must be read sequentially: expected index {}, got {index}",
                state.next_row_index
            )));
        }
        state.next_row_index += 1;
        Ok(())
    }

    fn exit(&self) -> RuntimeDataResult<()> {
        Err(RuntimeDataError::unsupported(
            "binary reader does not support nested named readers",
        ))
    }

    fn read_row_count(&self) -> RuntimeDataResult<usize> {
        let value = read_varuint(&mut self.state.borrow_mut())?;
        let row_count = usize::try_from(value)
            .map_err(|_| RuntimeDataError::decode("row count exceeds usize"))?;
        self.limits.check_rows(row_count)?;
        self.state.borrow_mut().next_row_index = 0;
        Ok(row_count)
    }

    fn read_list_len(&self) -> RuntimeDataResult<usize> {
        let length = usize::try_from(read_varuint(&mut self.state.borrow_mut())?)
            .map_err(|_| RuntimeDataError::decode("list length exceeds usize"))?;
        self.limits.check_list_length(length)?;
        Ok(length)
    }

    fn array_elements_are_positional(&self) -> bool {
        true
    }

    fn begin_record(&self, optional_slot_count: usize) -> RuntimeDataResult<()> {
        let byte_count = optional_slot_count.div_ceil(8);
        let mut state = self.state.borrow_mut();
        self.limits.check_nesting_depth(state.records.len() + 1)?;
        let bytes = read_exact(&mut state, byte_count)?.to_vec();
        validate_optional_bitmap(&bytes, optional_slot_count)?;
        state.records.push(RecordState {
            bitmap: bytes,
            next_optional_slot: 0,
        });
        Ok(())
    }

    fn end_record(&self) -> RuntimeDataResult<()> {
        let mut state = self.state.borrow_mut();
        let Some(record) = state.records.pop() else {
            return Err(RuntimeDataError::decode(
                "binary reader record stack underflow",
            ));
        };
        if record.next_optional_slot > record.bitmap.len() * 8 {
            return Err(RuntimeDataError::decode(
                "binary reader consumed more optional slots than current record bitmap",
            ));
        }
        Ok(())
    }

    fn read_enum_index(&self, variants: &[&str]) -> RuntimeDataResult<usize> {
        let ordinal = usize::try_from(self.read_enum_ordinal()?)
            .map_err(|_| RuntimeDataError::decode("enum ordinal exceeds usize"))?;
        if ordinal < variants.len() {
            Ok(ordinal)
        } else {
            Err(RuntimeDataError::decode(format!(
                "enum ordinal `{ordinal}` is out of range"
            )))
        }
    }

    fn as_i64(&self) -> RuntimeDataResult<i64> {
        self.read_next_i64()
    }

    fn as_f64(&self) -> RuntimeDataResult<f64> {
        self.read_next_f64()
    }

    fn as_bool(&self) -> RuntimeDataResult<bool> {
        self.read_next_bool()
    }

    fn as_string(&self) -> RuntimeDataResult<String> {
        self.read_next_string()
    }

    fn schema_revision(&self) -> Option<u64> {
        Some(self.state.borrow().schema_revision)
    }

    fn ensure_exhausted(&self) -> RuntimeDataResult<()> {
        let state = self.state.borrow();
        if state.offset == state.bytes.len() {
            Ok(())
        } else {
            Err(RuntimeDataError::decode(format!(
                "trailing bytes in binary runtime data: {} bytes remaining",
                state.bytes.len() - state.offset
            )))
        }
    }
}

impl BinaryRuntimeDataReader {
    pub fn read_optional_present(&self) -> RuntimeDataResult<bool> {
        let mut state = self.state.borrow_mut();
        let Some(record) = state.records.last_mut() else {
            return Err(RuntimeDataError::decode(
                "binary reader is not inside a record",
            ));
        };
        let slot_index = record.next_optional_slot;
        record.next_optional_slot += 1;
        let Some(byte) = record.bitmap.get(slot_index / 8) else {
            return Err(RuntimeDataError::decode(format!(
                "optional slot {slot_index} is out of current record bitmap"
            )));
        };
        Ok((byte & (1 << (slot_index % 8))) != 0)
    }

    pub fn read_next_i64(&self) -> RuntimeDataResult<i64> {
        let encoded = read_varuint(&mut self.state.borrow_mut())?;
        Ok(((encoded >> 1) as i64) ^ (-((encoded & 1) as i64)))
    }

    pub fn read_next_f64(&self) -> RuntimeDataResult<f64> {
        let mut state = self.state.borrow_mut();
        let bytes = read_exact(&mut state, 8)?;
        let value = f64::from_le_bytes(bytes.try_into().expect("f64 slice"));
        if value.is_finite() {
            Ok(value)
        } else {
            Err(RuntimeDataError::decode(
                "non-finite float in binary runtime data",
            ))
        }
    }

    pub fn read_next_bool(&self) -> RuntimeDataResult<bool> {
        match read_exact(&mut self.state.borrow_mut(), 1)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(RuntimeDataError::decode(format!(
                "invalid bool byte `{other}`"
            ))),
        }
    }

    pub fn read_next_string(&self) -> RuntimeDataResult<String> {
        let mut state = self.state.borrow_mut();
        let len = usize::try_from(read_varuint(&mut state)?)
            .map_err(|_| RuntimeDataError::decode("string length exceeds usize"))?;
        self.limits.check_string_bytes(len)?;
        let bytes = read_exact(&mut state, len)?.to_vec();
        String::from_utf8(bytes)
            .map_err(|source| RuntimeDataError::decode(format!("invalid UTF-8 string: {source}")))
    }

    pub fn read_enum_ordinal(&self) -> RuntimeDataResult<u64> {
        read_varuint(&mut self.state.borrow_mut())
    }
}

impl FsBinaryRuntimeDataLoader {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            limits: RuntimeDecodeLimits::default(),
        }
    }

    pub fn with_limits(root: impl Into<PathBuf>, limits: RuntimeDecodeLimits) -> Self {
        Self {
            root: root.into(),
            limits,
        }
    }
}

impl RuntimeDataLoader for FsBinaryRuntimeDataLoader {
    type Reader = BinaryRuntimeDataReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader> {
        let bytes = read_runtime_file(&self.root, path, "bin", self.limits)?;
        BinaryRuntimeDataReader::from_bytes_with_limits(bytes, self.limits)
    }
}

fn read_varuint(state: &mut BinaryState) -> RuntimeDataResult<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for index in 0..10 {
        let byte = read_exact(state, 1)?[0];
        if index == 9 && (byte & 0x7f) > 1 {
            return Err(RuntimeDataError::decode("varuint exceeds u64"));
        }
        value |= u64::from(byte & 0x7f) << shift;
        if (byte & 0x80) == 0 {
            return Ok(value);
        }
        shift += 7;
    }
    Err(RuntimeDataError::decode("malformed varuint"))
}

fn read_exact(state: &mut BinaryState, len: usize) -> RuntimeDataResult<&[u8]> {
    let end = state
        .offset
        .checked_add(len)
        .ok_or_else(|| RuntimeDataError::decode("binary cursor overflow"))?;
    if end > state.bytes.len() {
        return Err(RuntimeDataError::decode("truncated binary runtime data"));
    }
    let start = state.offset;
    state.offset = end;
    Ok(&state.bytes[start..end])
}

fn validate_optional_bitmap(bitmap: &[u8], optional_slot_count: usize) -> RuntimeDataResult<()> {
    let used_bits_in_last_byte = optional_slot_count % 8;
    if bitmap.is_empty() || used_bits_in_last_byte == 0 {
        return Ok(());
    }
    let unused_mask = !((1u8 << used_bits_in_last_byte) - 1);
    if bitmap[bitmap.len() - 1] & unused_mask != 0 {
        return Err(RuntimeDataError::decode(
            "invalid optional bitmap padding bits",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuntimeDataLoader, RuntimeDataReader};
    use std::{env, fs, process};

    #[test]
    fn binary_reader_reads_header_records_and_scalars() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"CFGB");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(&42u64.to_le_bytes());
        write_varuint_for_test(&mut bytes, 1);
        bytes.push(0b0000_0001);
        write_zigzag_for_test(&mut bytes, -7);
        bytes.extend_from_slice(&1.5f64.to_le_bytes());
        bytes.push(1);
        write_string_for_test(&mut bytes, "Potion");
        write_varuint_for_test(&mut bytes, 3);

        let reader = BinaryRuntimeDataReader::from_bytes(bytes).unwrap();
        assert_eq!(reader.schema_revision(), Some(42));
        assert_eq!(reader.read_row_count().unwrap(), 1);
        reader.begin_record(1).unwrap();
        assert!(reader.read_optional_present().unwrap());
        assert_eq!(reader.read_next_i64().unwrap(), -7);
        assert_eq!(reader.read_next_f64().unwrap(), 1.5);
        assert!(reader.read_next_bool().unwrap());
        assert_eq!(reader.read_next_string().unwrap(), "Potion");
        assert_eq!(
            reader
                .read_enum_index(&["zero", "one", "two", "three"])
                .unwrap(),
            3
        );
        reader.ensure_exhausted().unwrap();
    }

    #[test]
    fn binary_reader_rejects_trailing_bytes_and_oversized_varuint() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"CFGB");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        write_varuint_for_test(&mut bytes, 0);
        bytes.push(0xff);

        let reader = BinaryRuntimeDataReader::from_bytes(bytes).unwrap();
        assert_eq!(reader.read_row_count().unwrap(), 0);
        assert!(
            reader
                .ensure_exhausted()
                .unwrap_err()
                .message
                .contains("trailing")
        );

        let mut oversized = Vec::new();
        oversized.extend_from_slice(b"CFGB");
        oversized.extend_from_slice(&1u16.to_le_bytes());
        oversized.extend_from_slice(&16u16.to_le_bytes());
        oversized.extend_from_slice(&1u64.to_le_bytes());
        oversized.extend_from_slice(&[0x80; 9]);
        oversized.push(0x02);

        let reader = BinaryRuntimeDataReader::from_bytes(oversized).unwrap();
        assert!(reader.read_row_count().unwrap_err().message.contains("u64"));
    }

    #[test]
    fn binary_reader_rejects_optional_bitmap_padding_bits() {
        let mut bytes = binary_bytes_with_row_count(1);
        bytes.push(0b1000_0000);

        let reader = BinaryRuntimeDataReader::from_bytes(bytes).unwrap();
        assert_eq!(reader.read_row_count().unwrap(), 1);
        assert!(
            reader
                .begin_record(1)
                .unwrap_err()
                .message
                .contains("bitmap")
        );
    }

    #[test]
    fn fs_binary_loader_rejects_path_traversal() {
        let loader = FsBinaryRuntimeDataLoader::new("/tmp/config_forge_runtime_root");
        let traversal_error = loader
            .open("../secret.bin")
            .err()
            .expect("path traversal should be rejected");
        assert!(traversal_error.message.contains("escapes"));
        let absolute_error = loader
            .open(std::env::temp_dir().join("secret.bin").to_str().unwrap())
            .err()
            .expect("absolute path should be rejected");
        assert!(absolute_error.message.contains("relative"));
    }

    #[test]
    fn fs_binary_loader_opens_payload() {
        let dir =
            env::temp_dir().join(format!("config_forge_binary_loader_test_{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp runtime data dir should be created");

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"CFGB");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(&9u64.to_le_bytes());
        write_varuint_for_test(&mut bytes, 0);
        fs::write(dir.join("items.bin"), bytes).unwrap();

        let reader = FsBinaryRuntimeDataLoader::new(&dir)
            .open("items.bin")
            .unwrap();
        assert_eq!(reader.schema_revision(), Some(9));
        assert_eq!(reader.read_row_count().unwrap(), 0);

        let reader = FsBinaryRuntimeDataLoader::new(&dir)
            .open("nested/../items")
            .unwrap();
        assert_eq!(reader.schema_revision(), Some(9));

        fs::remove_dir_all(&dir).expect("temp runtime data dir should be removed");
    }

    #[test]
    fn binary_reader_enforces_decode_limits() {
        let limits = RuntimeDecodeLimits {
            max_file_bytes: 16,
            ..RuntimeDecodeLimits::default()
        };
        assert!(
            BinaryRuntimeDataReader::from_bytes_with_limits(binary_bytes_with_row_count(0), limits)
                .err()
                .expect("oversized bytes should be rejected")
                .message
                .contains("runtime file bytes")
        );

        let limits = RuntimeDecodeLimits {
            max_rows: 1,
            ..RuntimeDecodeLimits::default()
        };
        let reader =
            BinaryRuntimeDataReader::from_bytes_with_limits(binary_bytes_with_row_count(2), limits)
                .unwrap();
        assert!(
            reader
                .read_row_count()
                .unwrap_err()
                .message
                .contains("row count")
        );

        let limits = RuntimeDecodeLimits {
            max_array_length: 1,
            ..RuntimeDecodeLimits::default()
        };
        let reader =
            BinaryRuntimeDataReader::from_bytes_with_limits(binary_bytes_with_row_count(0), limits)
                .unwrap();
        let _ = reader.read_row_count().unwrap();
        write_varuint_for_test(&mut reader.state.borrow_mut().bytes, 2);
        assert!(
            reader
                .read_list_len()
                .unwrap_err()
                .message
                .contains("list length")
        );

        let limits = RuntimeDecodeLimits {
            max_string_bytes: 3,
            ..RuntimeDecodeLimits::default()
        };
        let mut bytes = binary_bytes_with_row_count(0);
        write_string_for_test(&mut bytes, "abcd");
        let reader = BinaryRuntimeDataReader::from_bytes_with_limits(bytes, limits).unwrap();
        let _ = reader.read_row_count().unwrap();
        assert!(
            reader
                .read_next_string()
                .unwrap_err()
                .message
                .contains("string byte length")
        );

        let limits = RuntimeDecodeLimits {
            max_nesting_depth: 1,
            ..RuntimeDecodeLimits::default()
        };
        let reader =
            BinaryRuntimeDataReader::from_bytes_with_limits(binary_bytes_with_row_count(0), limits)
                .unwrap();
        reader.begin_record(0).unwrap();
        assert!(
            reader
                .begin_record(0)
                .unwrap_err()
                .message
                .contains("nesting depth")
        );
    }

    #[test]
    fn fs_binary_loader_rejects_oversized_file_before_reading_payload() {
        let dir = env::temp_dir().join(format!(
            "config_forge_binary_limit_loader_test_{}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp runtime data dir should be created");

        fs::write(dir.join("items.bin"), binary_bytes_with_row_count(0)).unwrap();
        let loader = FsBinaryRuntimeDataLoader::with_limits(
            &dir,
            RuntimeDecodeLimits {
                max_file_bytes: 16,
                ..RuntimeDecodeLimits::default()
            },
        );
        assert!(
            loader
                .open("items.bin")
                .err()
                .expect("oversized file should be rejected")
                .message
                .contains("runtime file bytes")
        );

        fs::remove_dir_all(&dir).expect("temp runtime data dir should be removed");
    }

    fn write_string_for_test(bytes: &mut Vec<u8>, value: &str) {
        write_varuint_for_test(bytes, value.len() as u64);
        bytes.extend_from_slice(value.as_bytes());
    }

    fn write_zigzag_for_test(bytes: &mut Vec<u8>, value: i64) {
        write_varuint_for_test(bytes, ((value << 1) ^ (value >> 63)) as u64);
    }

    fn write_varuint_for_test(bytes: &mut Vec<u8>, mut value: u64) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    fn binary_bytes_with_row_count(row_count: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"CFGB");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        write_varuint_for_test(&mut bytes, row_count);
        bytes
    }
}
