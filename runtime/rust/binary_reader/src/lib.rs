// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod runtime_reader;

pub use runtime_reader::{
    BinaryRuntimeDataReader, FsBinaryRuntimeDataLoader, SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
};

pub trait RuntimeDataReader: Clone {
    fn field(&self, name: &str) -> Result<Option<Self>, String>;
    fn index(&self, index: usize) -> Result<Self, String>;
    fn len(&self) -> Result<usize, String>;
    fn as_i64(&self) -> Result<i64, String>;
    fn as_f64(&self) -> Result<f64, String>;
    fn as_bool(&self) -> Result<bool, String>;
    fn as_string(&self) -> Result<String, String>;
    fn is_null(&self) -> bool;
    fn schema_revision(&self) -> Option<u64> {
        None
    }
    fn read_row_count(&self) -> Result<usize, String> {
        Err("positional runtime reader is not supported".to_owned())
    }
    fn begin_record(&self, _optional_slot_count: usize) -> Result<(), String> {
        Err("positional runtime reader is not supported".to_owned())
    }
    fn read_optional_present(&self, _slot_index: usize) -> Result<bool, String> {
        Err("positional runtime reader is not supported".to_owned())
    }
    fn read_next_i64(&self) -> Result<i64, String> {
        Err("positional runtime reader is not supported".to_owned())
    }
    fn read_next_f64(&self) -> Result<f64, String> {
        Err("positional runtime reader is not supported".to_owned())
    }
    fn read_next_bool(&self) -> Result<bool, String> {
        Err("positional runtime reader is not supported".to_owned())
    }
    fn read_next_string(&self) -> Result<String, String> {
        Err("positional runtime reader is not supported".to_owned())
    }
    fn read_enum_ordinal(&self) -> Result<u64, String> {
        Err("positional runtime reader is not supported".to_owned())
    }
    fn ensure_exhausted(&self) -> Result<(), String> {
        Err("positional runtime reader is not supported".to_owned())
    }
}

pub trait RuntimeDataLoader {
    type Reader: RuntimeDataReader;

    fn open(&self, path: &str) -> Result<Self::Reader, String>;
}
