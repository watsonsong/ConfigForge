// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

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
        self.inner
            .open_reader(&path.replace(".msgpack", ".json").replace(".bin", ".json"))
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
        self.inner.open_reader(&path.replace(".bin", ".msgpack"))
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
    assert_eq!(item.stack().expect("stack fragment").max_stack, 999);
    assert!(data.items.get_by_id(404).is_none());

    let class_level = data
        .class_levels
        .get_by_class_level(1, 2)
        .expect("compound class level index should resolve");
    assert_eq!(class_level.attributes.max_health, Some(145));
    assert_eq!(class_level.attributes.attack, Some(16));

    assert_eq!(data.game_rules.max_player_level, 60);
    assert!(data.game_rules.pvp_enabled);
}

#[test]
fn generated_rust_loads_exported_json_and_queries_indexes() {
    let data = GameData::load(&json_fixture_loader_from_env())
        .expect("generated data should load from JSON runtime reader");
    assert_standard_data(&data);
}

#[test]
fn generated_rust_loads_exported_messagepack_and_queries_indexes() {
    let data = GameData::load(&msgpack_fixture_loader_from_env())
        .expect("generated data should load from MessagePack runtime reader");
    assert_standard_data(&data);
}

#[test]
fn generated_rust_loads_exported_binary_and_queries_indexes() {
    let data = GameData::load(&binary_fixture_loader_from_env())
        .expect("generated data should load from binary runtime reader");
    assert_standard_data(&data);
}
