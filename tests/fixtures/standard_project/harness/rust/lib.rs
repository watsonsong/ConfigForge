include!("generated.rs");

use std::{env, fs, path::PathBuf};

use serde_json::Value;

#[derive(Clone)]
struct ValueReader {
    value: Value,
}

impl RuntimeDataReader for ValueReader {
    fn field(&self, name: &str) -> Result<Option<Self>, String> {
        Ok(self
            .value
            .get(name)
            .cloned()
            .map(|value| ValueReader { value }))
    }

    fn index(&self, index: usize) -> Result<Self, String> {
        self.value
            .as_array()
            .and_then(|values| values.get(index))
            .cloned()
            .map(|value| ValueReader { value })
            .ok_or_else(|| format!("missing array index {index}"))
    }

    fn len(&self) -> Result<usize, String> {
        self.value
            .as_array()
            .map(Vec::len)
            .or_else(|| self.value.as_object().map(|object| object.len()))
            .ok_or_else(|| "reader value is not array/object".to_owned())
    }

    fn as_i64(&self) -> Result<i64, String> {
        match &self.value {
            Value::Number(number) => number.as_i64().ok_or_else(|| "number is not i64".to_owned()),
            Value::String(text) => text.parse().map_err(|error| format!("invalid i64: {error}")),
            other => Err(format!("expected i64-like value, got {other:?}")),
        }
    }

    fn as_f64(&self) -> Result<f64, String> {
        match &self.value {
            Value::Number(number) => number.as_f64().ok_or_else(|| "number is not f64".to_owned()),
            Value::String(text) => text.parse().map_err(|error| format!("invalid f64: {error}")),
            other => Err(format!("expected f64-like value, got {other:?}")),
        }
    }

    fn as_bool(&self) -> Result<bool, String> {
        match &self.value {
            Value::Bool(value) => Ok(*value),
            Value::String(text) => text.parse().map_err(|error| format!("invalid bool: {error}")),
            other => Err(format!("expected bool-like value, got {other:?}")),
        }
    }

    fn as_string(&self) -> Result<String, String> {
        match &self.value {
            Value::String(text) => Ok(text.clone()),
            other => Err(format!("expected string value, got {other:?}")),
        }
    }

    fn is_null(&self) -> bool {
        self.value.is_null()
    }
}

struct FixtureLoader {
    data_dir: PathBuf,
}

impl FixtureLoader {
    fn from_env() -> Self {
        let data_dir = env::var("DATA_DIR").expect("DATA_DIR should point to exported data");
        Self {
            data_dir: PathBuf::from(data_dir),
        }
    }
}

impl RuntimeDataLoader for FixtureLoader {
    type Reader = ValueReader;

    fn open(&self, path: &str) -> Result<Self::Reader, String> {
        let bytes = fs::read(self.data_dir.join(path)).map_err(|error| error.to_string())?;
        let value = rmp_serde::from_slice(&bytes).map_err(|error| error.to_string())?;
        Ok(ValueReader { value })
    }
}

#[test]
fn generated_rust_loads_exported_messagepack_and_queries_indexes() {
    let data = GameData::load(&FixtureLoader::from_env()).expect("generated data should load");

    let item = data.items.get_by_id(2001).expect("item id should be indexed");
    assert_eq!(item.price, 5);
    assert_eq!(item.stack().expect("stack fragment").max_stack, 999);
    assert!(data.items.get_by_id(404).is_none());

    let class_level = data
        .class_levels
        .get_by_class_level(1, 2)
        .expect("compound class level index should resolve");
    assert_eq!(class_level.attributes.max_health, 145);
    assert_eq!(class_level.attributes.attack, 16);

    assert_eq!(data.game_rules.max_player_level, 60);
    assert!(data.game_rules.pvp_enabled);
}
