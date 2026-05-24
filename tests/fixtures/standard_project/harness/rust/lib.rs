include!("generated.rs");

use std::{env, fs, path::PathBuf};

use serde_json::Value;

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

    fn read_value(&self, path: &str) -> Value {
        let bytes = fs::read(self.data_dir.join(path)).expect("runtime data should be readable");
        rmp_serde::from_slice(&bytes).expect("runtime data should decode as MessagePack")
    }

    fn rows(&self, path: &str) -> Vec<Value> {
        self.read_value(path)["rows"]
            .as_array()
            .expect("collection table payload should contain rows")
            .clone()
    }
}

impl RuntimeDataLoader for FixtureLoader {
    fn load_items_rows(&self, path: &str) -> Result<Vec<ItemsRow>, String> {
        if path != ItemsTable::DATA_FILE {
            return Err(format!("unexpected items path: {path}"));
        }
        Ok(self
            .rows(path)
            .iter()
            .map(|row| ItemsRow {
                id: required_i64(row, "id"),
                price: required_i64(row, "price"),
                stack: row.get("stack").and_then(|stack| {
                    stack.as_object().map(|_| ItemsStackFragment {
                        max_stack: required_i64(stack, "max_stack"),
                    })
                }),
                attributes: row.get("attributes").and_then(optional_attribute_set),
            })
            .collect())
    }

    fn load_classes_rows(&self, path: &str) -> Result<Vec<ClassesRow>, String> {
        if path != ClassesTable::DATA_FILE {
            return Err(format!("unexpected classes path: {path}"));
        }
        Ok(self
            .rows(path)
            .iter()
            .map(|row| ClassesRow {
                class_id: required_i64(row, "class_id"),
            })
            .collect())
    }

    fn load_class_levels_rows(&self, path: &str) -> Result<Vec<ClassLevelsRow>, String> {
        if path != ClassLevelsTable::DATA_FILE {
            return Err(format!("unexpected class levels path: {path}"));
        }
        Ok(self
            .rows(path)
            .iter()
            .map(|row| ClassLevelsRow {
                class_id: required_i64(row, "class_id"),
                level: required_i64(row, "level"),
                attributes: optional_attribute_set(&row["attributes"])
                    .expect("class level attributes should exist"),
            })
            .collect())
    }

    fn load_game_rules(&self, path: &str) -> Result<GameRulesRow, String> {
        if path != GameRulesRow::DATA_FILE {
            return Err(format!("unexpected game rules path: {path}"));
        }
        let value = self.read_value(path);
        Ok(GameRulesRow {
            max_player_level: required_i64(&value, "max_player_level"),
            starter_gold: required_i64(&value, "starter_gold"),
            stamina_cap: required_i64(&value, "stamina_cap"),
            daily_reset_hour: required_i64(&value, "daily_reset_hour"),
            pvp_enabled: required_bool(&value, "pvp_enabled"),
        })
    }
}

fn optional_attribute_set(value: &Value) -> Option<AttributeSet> {
    value.as_object().map(|_| AttributeSet {
        max_health: required_i64(value, "max_health"),
        max_stamina: required_i64(value, "max_stamina"),
        attack: required_i64(value, "attack"),
        defence: required_i64(value, "defence"),
    })
}

fn required_i64(value: &Value, field: &str) -> i64 {
    match &value[field] {
        Value::Number(number) => number.as_i64().expect("integer number should fit i64"),
        Value::String(text) => text.parse().expect("integer string should parse"),
        other => panic!("field `{field}` should be integer-like, got {other:?}"),
    }
}

fn required_bool(value: &Value, field: &str) -> bool {
    match &value[field] {
        Value::Bool(value) => *value,
        Value::String(text) => text.parse().expect("bool string should parse"),
        other => panic!("field `{field}` should be bool-like, got {other:?}"),
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
