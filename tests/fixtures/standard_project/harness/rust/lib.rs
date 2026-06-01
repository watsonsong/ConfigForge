// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::OnceLock;

static TEXT_CATALOG: OnceLock<std::collections::HashMap<String, String>> = OnceLock::new();

fn global_translate(context: Option<&str>, msgid: &str) -> Option<String> {
    let key = context
        .map(|context| format!("{context}\u{4}{msgid}"))
        .unwrap_or_else(|| msgid.to_owned());
    text_catalog().get(&key).cloned()
}

fn text_catalog() -> &'static std::collections::HashMap<String, String> {
    TEXT_CATALOG.get_or_init(|| {
        let mo_path = std::env::var("I18N_MO")
            .expect("I18N_MO should point to the compiled gettext MO catalog");
        load_mo_catalog(&mo_path)
    })
}

fn load_mo_catalog(path: &str) -> std::collections::HashMap<String, String> {
    let data = std::fs::read(path).expect("MO catalog should be readable");
    if data.len() < 28 || read_le32(&data, 0) != 0x9504_12de {
        panic!("unsupported MO catalog");
    }
    let count = read_le32(&data, 8) as usize;
    let original_table = read_le32(&data, 12) as usize;
    let translated_table = read_le32(&data, 16) as usize;
    let mut catalog = std::collections::HashMap::new();
    for index in 0..count {
        let original_length = read_le32(&data, original_table + index * 8) as usize;
        let original_offset = read_le32(&data, original_table + index * 8 + 4) as usize;
        let translated_length = read_le32(&data, translated_table + index * 8) as usize;
        let translated_offset = read_le32(&data, translated_table + index * 8 + 4) as usize;
        let original = std::str::from_utf8(&data[original_offset..original_offset + original_length])
            .expect("MO original should be UTF-8")
            .to_owned();
        let translated =
            std::str::from_utf8(&data[translated_offset..translated_offset + translated_length])
                .expect("MO translation should be UTF-8")
                .to_owned();
        catalog.insert(original, translated);
    }
    catalog
}

fn read_le32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .expect("MO u32 should be in bounds"),
    )
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

    assert_eq!(data.character_surnames.len(), 4);
    assert_eq!(data.character_surnames.row_at(0).unwrap().value, "Stone");
    assert_eq!(
        data.character_surnames
            .get_by_value("Frost".to_owned())
            .unwrap()
            .value,
        "Frost"
    );
    assert_eq!(data.character_male_given_names.rows()[1].value, "Borin");
    assert_eq!(
        data.character_female_given_names.rows()[2].value,
        "Celeste"
    );

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
    assert_eq!(
        data.character_surnames()
            .expect("surnames should load lazily")
            .row_at(1)
            .unwrap()
            .value,
        "River"
    );
    assert_eq!(
        data.character_male_given_names()
            .expect("male given names should load lazily")
            .rows()[2]
            .value,
        "Cedric"
    );
    assert_eq!(
        data.character_female_given_names()
            .expect("female given names should load lazily")
            .rows()[3]
            .value,
        "Daphne"
    );
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
