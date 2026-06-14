// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ExportedTable {
    pub(super) meta: ExportedTableMeta,
    pub(super) rows: Vec<BTreeMap<String, ExportedValue>>,
}

impl ExportedTable {
    pub fn rows(&self) -> &[BTreeMap<String, ExportedValue>] {
        &self.rows
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ExportedTablePayload {
    Collection(ExportedTable),
    Object(BTreeMap<String, ExportedValue>),
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportedTableMeta {
    pub(super) name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum ExportedValue {
    Scalar(String),
    #[serde(serialize_with = "serialize_f64_array")]
    NumberArray(Vec<f64>),
    Struct(BTreeMap<String, ExportedValue>),
    Array(Vec<ExportedValue>),
}

fn serialize_f64_array<S>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;

    let mut seq = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        seq.serialize_element(value)?;
    }
    seq.end()
}
