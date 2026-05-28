// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::diagnostic::DiagnosticSeverity;

pub type ValidationSeverity = DiagnosticSeverity;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    Object,
    #[default]
    Collection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveFieldType {
    Int,
    Float,
    Bool,
    String,
    Text,
    Asset,
}

impl PrimitiveFieldType {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "int" | "i64" => Some(Self::Int),
            "float" | "f64" => Some(Self::Float),
            "bool" => Some(Self::Bool),
            "string" => Some(Self::String),
            "text" => Some(Self::Text),
            "asset" => Some(Self::Asset),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::String => "string",
            Self::Text => "text",
            Self::Asset => "asset",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueLayout {
    ScalarCell,
    JsonCell,
    ExpandedValue,
}
