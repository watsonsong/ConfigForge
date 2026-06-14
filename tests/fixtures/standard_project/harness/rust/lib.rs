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

use config_forge_runtime_binary_reader::{BinaryRuntimeDataReader, FsBinaryRuntimeDataLoader};
use json_runtime_reader::{FsJsonRuntimeDataLoader, JsonValueReader};
use msgpack_runtime_reader::{FsMessagePackRuntimeDataLoader, MessagePackValueReader};

fn runtime_error_from_core(
    error: config_forge_runtime_reader_core::RuntimeDataError,
) -> RuntimeDataError {
    let kind = match error.kind {
        config_forge_runtime_reader_core::RuntimeDataErrorKind::Io => RuntimeDataErrorKind::Io,
        config_forge_runtime_reader_core::RuntimeDataErrorKind::Decode => {
            RuntimeDataErrorKind::Decode
        }
        config_forge_runtime_reader_core::RuntimeDataErrorKind::SchemaMismatch => {
            RuntimeDataErrorKind::SchemaMismatch
        }
        config_forge_runtime_reader_core::RuntimeDataErrorKind::TypeMismatch => {
            RuntimeDataErrorKind::TypeMismatch
        }
        config_forge_runtime_reader_core::RuntimeDataErrorKind::NotFound => {
            RuntimeDataErrorKind::NotFound
        }
        config_forge_runtime_reader_core::RuntimeDataErrorKind::Unsupported => {
            RuntimeDataErrorKind::Unsupported
        }
        config_forge_runtime_reader_core::RuntimeDataErrorKind::InvalidPath => {
            RuntimeDataErrorKind::InvalidPath
        }
    };
    RuntimeDataError {
        kind,
        message: error.message,
        path: error.path,
        source: error.source,
    }
}

macro_rules! impl_generated_runtime_reader {
    ($reader:ty) => {
        impl RuntimeDataReader for $reader {
            fn enter_required_field(&self, name: &'static str) -> RuntimeDataResult<()> {
                config_forge_runtime_reader_core::RuntimeDataReader::enter_required_field(self, name)
                    .map_err(runtime_error_from_core)
            }

            fn enter_optional_field(&self, name: &'static str) -> RuntimeDataResult<bool> {
                config_forge_runtime_reader_core::RuntimeDataReader::enter_optional_field(self, name)
                    .map_err(runtime_error_from_core)
            }

            fn exit_field(&self) -> RuntimeDataResult<()> {
                config_forge_runtime_reader_core::RuntimeDataReader::exit_field(self)
                    .map_err(runtime_error_from_core)
            }

            fn enter_index(&self, index: usize) -> RuntimeDataResult<()> {
                config_forge_runtime_reader_core::RuntimeDataReader::enter_index(self, index)
                    .map_err(runtime_error_from_core)
            }

            fn exit(&self) -> RuntimeDataResult<()> {
                config_forge_runtime_reader_core::RuntimeDataReader::exit(self)
                    .map_err(runtime_error_from_core)
            }

            fn read_row_count(&self) -> RuntimeDataResult<usize> {
                config_forge_runtime_reader_core::RuntimeDataReader::read_row_count(self)
                    .map_err(runtime_error_from_core)
            }

            fn read_list_len(&self) -> RuntimeDataResult<usize> {
                config_forge_runtime_reader_core::RuntimeDataReader::read_list_len(self)
                    .map_err(runtime_error_from_core)
            }

            fn array_elements_are_positional(&self) -> bool {
                config_forge_runtime_reader_core::RuntimeDataReader::array_elements_are_positional(self)
            }

            fn begin_record(&self, optional_slot_count: usize) -> RuntimeDataResult<()> {
                config_forge_runtime_reader_core::RuntimeDataReader::begin_record(
                    self,
                    optional_slot_count,
                )
                .map_err(runtime_error_from_core)
            }

            fn end_record(&self) -> RuntimeDataResult<()> {
                config_forge_runtime_reader_core::RuntimeDataReader::end_record(self)
                    .map_err(runtime_error_from_core)
            }

            fn read_enum_index(&self, variants: &[&str]) -> RuntimeDataResult<usize> {
                config_forge_runtime_reader_core::RuntimeDataReader::read_enum_index(self, variants)
                    .map_err(runtime_error_from_core)
            }

            fn as_i64(&self) -> RuntimeDataResult<i64> {
                config_forge_runtime_reader_core::RuntimeDataReader::as_i64(self)
                    .map_err(runtime_error_from_core)
            }

            fn as_f64(&self) -> RuntimeDataResult<f64> {
                config_forge_runtime_reader_core::RuntimeDataReader::as_f64(self)
                    .map_err(runtime_error_from_core)
            }

            fn as_bool(&self) -> RuntimeDataResult<bool> {
                config_forge_runtime_reader_core::RuntimeDataReader::as_bool(self)
                    .map_err(runtime_error_from_core)
            }

            fn as_string(&self) -> RuntimeDataResult<String> {
                config_forge_runtime_reader_core::RuntimeDataReader::as_string(self)
                    .map_err(runtime_error_from_core)
            }

            fn schema_revision(&self) -> Option<u64> {
                config_forge_runtime_reader_core::RuntimeDataReader::schema_revision(self)
            }

            fn ensure_exhausted(&self) -> RuntimeDataResult<()> {
                config_forge_runtime_reader_core::RuntimeDataReader::ensure_exhausted(self)
                    .map_err(runtime_error_from_core)
            }
        }
    };
}

impl_generated_runtime_reader!(JsonValueReader);
impl_generated_runtime_reader!(MessagePackValueReader);
impl_generated_runtime_reader!(BinaryRuntimeDataReader);

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
        self.inner.open_reader(path).map_err(runtime_error_from_core)
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
    type Reader = MessagePackValueReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader> {
        self.inner.open_reader(path).map_err(runtime_error_from_core)
    }
}

impl RuntimeDataLoader for FsBinaryRuntimeDataLoader {
    type Reader = BinaryRuntimeDataReader;

    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader> {
        config_forge_runtime_reader_core::RuntimeDataLoader::open(self, path)
            .map_err(runtime_error_from_core)
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
    assert_eq!(data.game_rules.starter_reward.gold, 500);
    assert_eq!(data.game_rules.starter_reward.item_ids, vec![2001, 2002]);
    assert_eq!(data.game_rules.ambient_tint.r, 51.0);
    assert_eq!(data.game_rules.ambient_tint.g, 102.0);
    assert_eq!(data.game_rules.ambient_tint.b, 153.0);
    assert_eq!(data.game_rules.ambient_tint.a, 255.0);
    assert_eq!(data.game_rules.spawn_position.x, 1.5);
    assert_eq!(data.game_rules.spawn_position.y, 2.5);
    assert_eq!(data.game_rules.spawn_position.z, 3.5);
    assert_eq!(data.game_rules.grid_size.x, 16);
    assert_eq!(data.game_rules.grid_size.y, 9);
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
    assert_eq!(game_rules.starter_reward.gold, 500);
    assert_eq!(game_rules.starter_reward.item_ids, vec![2001, 2002]);
    assert_eq!(game_rules.ambient_tint.r, 51.0);
    assert_eq!(game_rules.ambient_tint.g, 102.0);
    assert_eq!(game_rules.ambient_tint.b, 153.0);
    assert_eq!(game_rules.ambient_tint.a, 255.0);
    assert_eq!(game_rules.spawn_position.x, 1.5);
    assert_eq!(game_rules.spawn_position.y, 2.5);
    assert_eq!(game_rules.spawn_position.z, 3.5);
    assert_eq!(game_rules.grid_size.x, 16);
    assert_eq!(game_rules.grid_size.y, 9);
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

#[test]
fn rust_binary_reader_rejects_optional_bitmap_padding_bits() {
    let mut bytes = Vec::from(*b"CFGB");
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(&7u64.to_le_bytes());
    write_varuint_for_binary_test(&mut bytes, 1);
    bytes.push(0x80);

    let reader = BinaryRuntimeDataReader::from_bytes(bytes).expect("binary header should parse");
    assert_eq!(reader.read_row_count().expect("row count"), 1);
    assert!(
        reader
            .begin_record(1)
            .expect_err("bitmap padding bits should be rejected")
            .message
            .contains("bitmap")
    );
}

fn write_varuint_for_binary_test(bytes: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
}
