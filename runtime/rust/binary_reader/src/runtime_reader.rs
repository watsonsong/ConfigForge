// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::RefCell,
    fs,
    path::{Component, Path, PathBuf},
    rc::Rc,
};

use config_forge_runtime_reader_core::{RuntimeDataLoader, RuntimeDataReader};

#[derive(Clone)]
pub struct BinaryRuntimeDataReader {
    state: Rc<RefCell<BinaryState>>,
}

struct BinaryState {
    bytes: Vec<u8>,
    offset: usize,
    schema_revision: u64,
    bitmap: Vec<u8>,
}

pub struct FsBinaryRuntimeDataLoader {
    root: PathBuf,
}

impl BinaryRuntimeDataReader {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() < 16 {
            return Err("truncated binary runtime data header".to_owned());
        }
        if &bytes[0..4] != b"CFGB" {
            return Err("invalid binary runtime data magic".to_owned());
        }
        let version = u16::from_le_bytes(bytes[4..6].try_into().expect("header slice"));
        if version != 1 {
            return Err(format!(
                "unsupported binary runtime data version `{version}`"
            ));
        }
        let header_size = u16::from_le_bytes(bytes[6..8].try_into().expect("header slice"));
        if header_size != 16 {
            return Err(format!(
                "unsupported binary runtime data header size `{header_size}`"
            ));
        }
        let schema_revision = u64::from_le_bytes(bytes[8..16].try_into().expect("header slice"));
        Ok(Self {
            state: Rc::new(RefCell::new(BinaryState {
                bytes,
                offset: 16,
                schema_revision,
                bitmap: Vec::new(),
            })),
        })
    }
}

impl RuntimeDataReader for BinaryRuntimeDataReader {
    fn field(&self, _name: &str) -> Result<Option<Self>, String> {
        Err("binary reader does not support named fields".to_owned())
    }

    fn index(&self, _index: usize) -> Result<Self, String> {
        Err("binary reader does not support random index access".to_owned())
    }

    fn len(&self) -> Result<usize, String> {
        Err("binary reader does not support named array length".to_owned())
    }

    fn as_i64(&self) -> Result<i64, String> {
        self.read_next_i64()
    }

    fn as_f64(&self) -> Result<f64, String> {
        self.read_next_f64()
    }

    fn as_bool(&self) -> Result<bool, String> {
        self.read_next_bool()
    }

    fn as_string(&self) -> Result<String, String> {
        self.read_next_string()
    }

    fn is_null(&self) -> bool {
        false
    }

    fn schema_revision(&self) -> Option<u64> {
        Some(self.state.borrow().schema_revision)
    }

    fn read_row_count(&self) -> Result<usize, String> {
        let value = read_varuint(&mut self.state.borrow_mut())?;
        usize::try_from(value).map_err(|_| "row count exceeds usize".to_owned())
    }

    fn begin_record(&self, optional_slot_count: usize) -> Result<(), String> {
        let byte_count = optional_slot_count.div_ceil(8);
        let mut state = self.state.borrow_mut();
        let bytes = read_exact(&mut state, byte_count)?.to_vec();
        state.bitmap = bytes;
        Ok(())
    }

    fn read_optional_present(&self, slot_index: usize) -> Result<bool, String> {
        let state = self.state.borrow();
        let Some(byte) = state.bitmap.get(slot_index / 8) else {
            return Err(format!(
                "optional slot {slot_index} is out of current record bitmap"
            ));
        };
        Ok((byte & (1 << (slot_index % 8))) != 0)
    }

    fn read_next_i64(&self) -> Result<i64, String> {
        let encoded = read_varuint(&mut self.state.borrow_mut())?;
        Ok(((encoded >> 1) as i64) ^ (-((encoded & 1) as i64)))
    }

    fn read_next_f64(&self) -> Result<f64, String> {
        let mut state = self.state.borrow_mut();
        let bytes = read_exact(&mut state, 8)?;
        let value = f64::from_le_bytes(bytes.try_into().expect("f64 slice"));
        if value.is_finite() {
            Ok(value)
        } else {
            Err("non-finite float in binary runtime data".to_owned())
        }
    }

    fn read_next_bool(&self) -> Result<bool, String> {
        match read_exact(&mut self.state.borrow_mut(), 1)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(format!("invalid bool byte `{other}`")),
        }
    }

    fn read_next_string(&self) -> Result<String, String> {
        let mut state = self.state.borrow_mut();
        let len = usize::try_from(read_varuint(&mut state)?)
            .map_err(|_| "string length exceeds usize".to_owned())?;
        let bytes = read_exact(&mut state, len)?.to_vec();
        String::from_utf8(bytes).map_err(|source| format!("invalid UTF-8 string: {source}"))
    }

    fn read_enum_ordinal(&self) -> Result<u64, String> {
        read_varuint(&mut self.state.borrow_mut())
    }

    fn ensure_exhausted(&self) -> Result<(), String> {
        let state = self.state.borrow();
        if state.offset == state.bytes.len() {
            Ok(())
        } else {
            Err(format!(
                "trailing bytes in binary runtime data: {} bytes remaining",
                state.bytes.len() - state.offset
            ))
        }
    }
}

impl FsBinaryRuntimeDataLoader {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl RuntimeDataLoader for FsBinaryRuntimeDataLoader {
    type Reader = BinaryRuntimeDataReader;

    fn open(&self, path: &str) -> Result<Self::Reader, String> {
        let full_path = safe_join(&self.root, path)?;
        let bytes = fs::read(&full_path)
            .map_err(|source| format!("failed to read {}: {source}", full_path.display()))?;
        BinaryRuntimeDataReader::from_bytes(bytes)
    }
}

fn read_varuint(state: &mut BinaryState) -> Result<u64, String> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for index in 0..10 {
        let byte = read_exact(state, 1)?[0];
        if index == 9 && (byte & 0x7f) > 1 {
            return Err("varuint exceeds u64".to_owned());
        }
        value |= u64::from(byte & 0x7f) << shift;
        if (byte & 0x80) == 0 {
            return Ok(value);
        }
        shift += 7;
    }
    Err("malformed varuint".to_owned())
}

fn read_exact(state: &mut BinaryState, len: usize) -> Result<&[u8], String> {
    let end = state
        .offset
        .checked_add(len)
        .ok_or_else(|| "binary cursor overflow".to_owned())?;
    if end > state.bytes.len() {
        return Err("truncated binary runtime data".to_owned());
    }
    let start = state.offset;
    state.offset = end;
    Ok(&state.bytes[start..end])
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err(format!("runtime data path `{relative}` must be relative"));
    }
    let mut result = PathBuf::from(root);
    for component in path.components() {
        match component {
            Component::Normal(part) => result.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!("runtime data path `{relative}` escapes data root"));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("runtime data path `{relative}` must be relative"));
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuntimeDataLoader, RuntimeDataReader};
    use std::{env, process};

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
        assert!(reader.read_optional_present(0).unwrap());
        assert_eq!(reader.read_next_i64().unwrap(), -7);
        assert_eq!(reader.read_next_f64().unwrap(), 1.5);
        assert!(reader.read_next_bool().unwrap());
        assert_eq!(reader.read_next_string().unwrap(), "Potion");
        assert_eq!(reader.read_enum_ordinal().unwrap(), 3);
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
        assert!(reader.ensure_exhausted().unwrap_err().contains("trailing"));

        let mut oversized = Vec::new();
        oversized.extend_from_slice(b"CFGB");
        oversized.extend_from_slice(&1u16.to_le_bytes());
        oversized.extend_from_slice(&16u16.to_le_bytes());
        oversized.extend_from_slice(&1u64.to_le_bytes());
        oversized.extend_from_slice(&[0x80; 9]);
        oversized.push(0x02);

        let reader = BinaryRuntimeDataReader::from_bytes(oversized).unwrap();
        assert!(reader.read_row_count().unwrap_err().contains("u64"));
    }

    #[test]
    fn fs_binary_loader_rejects_path_traversal() {
        let loader = FsBinaryRuntimeDataLoader::new("/tmp/config_forge_runtime_root");
        let traversal_error = loader
            .open("../secret.bin")
            .err()
            .expect("path traversal should be rejected");
        assert!(traversal_error.contains("escapes"));
        let absolute_error = loader
            .open("/tmp/secret.bin")
            .err()
            .expect("absolute path should be rejected");
        assert!(absolute_error.contains("relative"));
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
}
