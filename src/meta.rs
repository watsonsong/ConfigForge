use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{ConfigForgeError, Result};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MetaSet {
    pub tables: Vec<TableMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TableMeta {
    pub name: String,
    pub source: String,
    pub fields: Vec<FieldMeta>,
    #[serde(default)]
    pub indexes: Vec<IndexMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FieldMeta {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub unique: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    I64,
    F64,
    Bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct IndexMeta {
    pub name: String,
    pub fields: Vec<String>,
    #[serde(default)]
    pub unique: bool,
}

impl MetaSet {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|source| ConfigForgeError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;

        toml::from_str(&content).map_err(|source| ConfigForgeError::ParseMeta {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_table_meta() {
        let meta: MetaSet = toml::from_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "i64"
                required = true
                unique = true
            "#,
        )
        .expect("meta should parse");

        assert_eq!(meta.tables[0].name, "items");
        assert_eq!(meta.tables[0].fields[0].field_type, FieldType::I64);
    }
}
