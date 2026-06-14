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
    Color,
    Vector2,
    Vector3,
    Vector4,
    Vector2Int,
    Vector3Int,
    Vector4Int,
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
            "color" => Some(Self::Color),
            "vector2" => Some(Self::Vector2),
            "vector3" => Some(Self::Vector3),
            "vector4" => Some(Self::Vector4),
            "vector2_int" => Some(Self::Vector2Int),
            "vector3_int" => Some(Self::Vector3Int),
            "vector4_int" => Some(Self::Vector4Int),
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
            Self::Color => "color",
            Self::Vector2 => "vector2",
            Self::Vector3 => "vector3",
            Self::Vector4 => "vector4",
            Self::Vector2Int => "vector2_int",
            Self::Vector3Int => "vector3_int",
            Self::Vector4Int => "vector4_int",
        }
    }

    pub fn is_builtin_value_type(&self) -> bool {
        matches!(
            self,
            Self::Color
                | Self::Vector2
                | Self::Vector3
                | Self::Vector4
                | Self::Vector2Int
                | Self::Vector3Int
                | Self::Vector4Int
        )
    }

    pub fn vector_dimension(&self) -> Option<usize> {
        match self {
            Self::Vector2 | Self::Vector2Int => Some(2),
            Self::Vector3 | Self::Vector3Int => Some(3),
            Self::Vector4 | Self::Vector4Int => Some(4),
            _ => None,
        }
    }

    pub fn is_int_vector(&self) -> bool {
        matches!(self, Self::Vector2Int | Self::Vector3Int | Self::Vector4Int)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueLayout {
    ScalarCell,
    JsonCell,
    ExpandedValue,
}
