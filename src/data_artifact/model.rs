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
    Number(f64),
    Struct(BTreeMap<String, ExportedValue>),
    Array(Vec<ExportedValue>),
}
