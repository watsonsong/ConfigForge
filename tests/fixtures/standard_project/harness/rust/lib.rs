// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

fn global_translate(context: Option<&str>, msgid: &str) -> Option<String> {
    match (context, msgid) {
        (Some("items.name"), "Iron Ore") => Some("铁矿石".to_owned()),
        (Some("classes.name"), "Warrior") => Some("战士".to_owned()),
        (Some("classes.description"), "Durable melee fighter.") => Some("耐久的近战战士。".to_owned()),
        _ => None,
    }
}

include!("generated.rs");

mod json_runtime_reader;
mod msgpack_runtime_reader;

use std::env;

use config_forge_runtime_binary_reader::FsBinaryRuntimeDataLoader;
use json_runtime_reader::{FsJsonRuntimeDataLoader, JsonValueReader};
use msgpack_runtime_reader::FsMessagePackRuntimeDataLoader;

#[derive(Debug, Clone)]
struct JsonFixtureRuntimeDataLoader {
    inner: FsJsonRuntimeDataLoader,
}

impl JsonFixtureRuntimeDataLoader {
    fn new(data_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            inner: FsJsonRuntimeDataLoader::new(data_dir),
        }
    }
}

impl RuntimeDataLoader for JsonFixtureRuntimeDataLoader {
    type Reader = JsonValueReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader> {
        self.inner.open_reader(path)
    }
}

#[derive(Debug, Clone)]
struct MessagePackFixtureRuntimeDataLoader {
    inner: FsMessagePackRuntimeDataLoader,
}

impl MessagePackFixtureRuntimeDataLoader {
    fn new(data_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            inner: FsMessagePackRuntimeDataLoader::new(data_dir),
        }
    }
}

impl RuntimeDataLoader for MessagePackFixtureRuntimeDataLoader {
    type Reader = msgpack_runtime_reader::MessagePackValueReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader> {
        self.inner.open_reader(path)
    }
}

fn json_fixture_loader_from_env() -> JsonFixtureRuntimeDataLoader {
    JsonFixtureRuntimeDataLoader::new(
        env::var("JSON_DATA_DIR").expect("JSON_DATA_DIR should point to exported JSON data"),
    )
}

fn msgpack_fixture_loader_from_env() -> MessagePackFixtureRuntimeDataLoader {
    MessagePackFixtureRuntimeDataLoader::new(
        env::var("MSGPACK_DATA_DIR")
            .expect("MSGPACK_DATA_DIR should point to exported MessagePack data"),
    )
}

fn binary_fixture_loader_from_env() -> FsBinaryRuntimeDataLoader {
    FsBinaryRuntimeDataLoader::new(
        env::var("BIN_DATA_DIR").expect("BIN_DATA_DIR should point to exported binary data"),
    )
}

fn assert_standard_data(data: &GameData) {
    let item = data.items.get_by_id(2001).expect("item id should be indexed");
    assert_eq!(item.price, 5);
    assert_eq!(item.name_text(), "铁矿石");
    assert_eq!(item.stack().expect("stack fragment").max_stack, 999);
    assert!(data.items.get_by_id(404).is_none());

    let class_level = data
        .class_levels
        .get_by_class_level(1, 2)
        .expect("compound class level index should resolve");
    assert_eq!(class_level.attributes.max_health, Some(145));
    assert_eq!(class_level.attributes.attack, Some(16));

    assert_eq!(data.game_rules.max_player_level, 60);
    assert_eq!(data.game_rules.default_class_id, 1);
    assert_eq!(data.game_rules.starter_item_ids, vec![2001, 2002]);
    let warrior = data.classes.get_by_id(1).expect("class id should be indexed");
    assert_eq!(warrior.name_text(), "战士");
    assert_eq!(warrior.description_text(), "耐久的近战战士。");
    let default_class = data
        .game_rules
        .default_class(data)
        .expect("default class relation should resolve");
    assert_eq!(default_class.class_id, 1);
    let starter_items = data
        .game_rules
        .starter_items(data)
        .expect("starter item relation should resolve");
    assert_eq!(starter_items.len(), 2);
    assert_eq!(starter_items[0].id, 2001);
    assert_eq!(starter_items[1].id, 2002);
    data.link_all().expect("link_all should pre-resolve relations");
    assert_eq!(data.game_rules.default_class(data).unwrap().class_id, 1);
    assert!(data.game_rules.pvp_enabled);
}

fn assert_standard_lazy_data(data: &impl GameDataAccess) {
    let items = data.items().expect("items table should load lazily");
    let item = items.get_by_id(2001).expect("item id should be indexed");
    assert_eq!(item.price, 5);
    assert_eq!(item.name_text(), "铁矿石");
    assert_eq!(item.stack().expect("stack fragment").max_stack, 999);
    assert!(items.get_by_id(404).is_none());

    let class_level = data
        .class_levels()
        .expect("class levels should load lazily")
        .get_by_class_level(1, 2)
        .expect("compound class level index should resolve");
    assert_eq!(class_level.attributes.max_health, Some(145));
    assert_eq!(class_level.attributes.attack, Some(16));

    let game_rules = data.game_rules().expect("game rules should load lazily");
    assert_eq!(game_rules.max_player_level, 60);
    assert_eq!(game_rules.default_class_id, 1);
    assert_eq!(game_rules.starter_item_ids, vec![2001, 2002]);
    let default_class = game_rules
        .default_class(data)
        .expect("default class relation should resolve lazily");
    assert_eq!(default_class.class_id, 1);
    assert_eq!(default_class.name_text(), "战士");
    assert_eq!(default_class.description_text(), "耐久的近战战士。");
    let starter_items = game_rules
        .starter_items(data)
        .expect("starter item relation should resolve lazily");
    assert_eq!(starter_items.len(), 2);
    assert_eq!(starter_items[0].id, 2001);
    assert_eq!(starter_items[1].id, 2002);
    assert!(game_rules.pvp_enabled);
}

#[test]
fn generated_rust_loads_exported_json_and_queries_indexes() {
    let data = GameData::load(&json_fixture_loader_from_env())
        .expect("generated data should load from JSON runtime reader");
    assert_standard_data(&data);

    let lazy_data = LazyGameData::new(json_fixture_loader_from_env());
    assert_standard_lazy_data(&lazy_data);
    lazy_data
        .link_all()
        .expect("lazy link_all should pre-resolve relations");
    assert_eq!(lazy_data.game_rules().unwrap().default_class(&lazy_data).unwrap().class_id, 1);
}

#[test]
fn generated_rust_loads_exported_messagepack_and_queries_indexes() {
    let data = GameData::load(&msgpack_fixture_loader_from_env())
        .expect("generated data should load from MessagePack runtime reader");
    assert_standard_data(&data);

    let lazy_data = LazyGameData::new(msgpack_fixture_loader_from_env());
    assert_standard_lazy_data(&lazy_data);
    lazy_data
        .preload_all()
        .expect("lazy preload_all should load all tables");
}

#[test]
fn generated_rust_loads_exported_binary_and_queries_indexes() {
    let data = GameData::load(&binary_fixture_loader_from_env())
        .expect("generated data should load from binary runtime reader");
    assert_standard_data(&data);

    let lazy_data = LazyGameData::new(binary_fixture_loader_from_env());
    assert_standard_lazy_data(&lazy_data);
}
