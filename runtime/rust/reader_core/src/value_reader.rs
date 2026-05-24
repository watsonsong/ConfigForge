// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde_json::Value;

pub const SUPPORTED_CONFIG_FORGE_READER_CONTRACT: &str = "^1.0";

#[derive(Debug, Clone)]
pub struct ValueReader {
    value: Value,
}

impl ValueReader {
    pub fn new(value: Value) -> Self {
        Self { value }
    }

    pub fn field(&self, name: &str) -> Result<Option<Self>, String> {
        Ok(self.value.get(name).cloned().map(Self::new))
    }

    pub fn index(&self, index: usize) -> Result<Self, String> {
        self.value
            .as_array()
            .and_then(|values| values.get(index))
            .cloned()
            .map(Self::new)
            .ok_or_else(|| format!("missing array index {index}"))
    }

    pub fn len(&self) -> Result<usize, String> {
        self.value
            .as_array()
            .map(Vec::len)
            .or_else(|| self.value.as_object().map(|object| object.len()))
            .ok_or_else(|| "reader value is not array/object".to_owned())
    }

    pub fn as_i64(&self) -> Result<i64, String> {
        match &self.value {
            Value::Number(number) => number.as_i64().ok_or_else(|| "number is not i64".to_owned()),
            Value::String(text) => text.parse().map_err(|error| format!("invalid i64: {error}")),
            other => Err(format!("expected i64-like value, got {other:?}")),
        }
    }

    pub fn as_f64(&self) -> Result<f64, String> {
        let parsed = match &self.value {
            Value::Number(number) => number
                .as_f64()
                .ok_or_else(|| "number is not f64".to_owned())?,
            Value::String(text) => text.parse().map_err(|error| format!("invalid f64: {error}"))?,
            other => return Err(format!("expected f64-like value, got {other:?}")),
        };
        if parsed.is_finite() {
            Ok(parsed)
        } else {
            Err(format!("expected finite f64-like value, got {:?}", self.value))
        }
    }

    pub fn as_bool(&self) -> Result<bool, String> {
        match &self.value {
            Value::Bool(value) => Ok(*value),
            Value::String(text) => match text.as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(format!("invalid bool: {text}")),
            },
            other => Err(format!("expected bool-like value, got {other:?}")),
        }
    }

    pub fn as_string(&self) -> Result<String, String> {
        match &self.value {
            Value::String(text) => Ok(text.clone()),
            other => Err(format!("expected string value, got {other:?}")),
        }
    }

    pub fn is_null(&self) -> bool {
        self.value.is_null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_json_values() {
        let reader = ValueReader::new(serde_json::json!({
            "rows": [{ "id": "1", "ratio": "1.5", "enabled": "true", "name": "Potion" }]
        }));

        let rows = reader.field("rows").unwrap().expect("rows field");
        assert_eq!(rows.len().unwrap(), 1);
        let row = rows.index(0).unwrap();
        assert_eq!(row.field("id").unwrap().unwrap().as_i64().unwrap(), 1);
        assert_eq!(row.field("ratio").unwrap().unwrap().as_f64().unwrap(), 1.5);
        assert!(row.field("enabled").unwrap().unwrap().as_bool().unwrap());
        assert_eq!(
            row.field("name").unwrap().unwrap().as_string().unwrap(),
            "Potion"
        );
        assert!(row.field("missing").unwrap().is_none());
    }

    #[test]
    fn rejects_non_finite_number_strings() {
        let reader = ValueReader::new(Value::String("NaN".to_owned()));
        assert!(reader.as_f64().unwrap_err().contains("finite"));
    }

    #[test]
    fn accepts_numeric_bool_strings() {
        assert!(ValueReader::new(Value::String("1".to_owned()))
            .as_bool()
            .unwrap());
        assert!(!ValueReader::new(Value::String("0".to_owned()))
            .as_bool()
            .unwrap());
    }
}
