// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

include!("generated.rs");

mod runtime_reader_core;
mod runtime_loader;

use std::env;

use runtime_loader::FsMessagePackRuntimeDataLoader;
use runtime_reader_core::ValueReader;

impl RuntimeDataReader for ValueReader {
    fn field(&self, name: &str) -> Result<Option<Self>, String> {
        ValueReader::field(self, name)
    }

    fn index(&self, index: usize) -> Result<Self, String> {
        ValueReader::index(self, index)
    }

    fn len(&self) -> Result<usize, String> {
        ValueReader::len(self)
    }

    fn as_i64(&self) -> Result<i64, String> {
        ValueReader::as_i64(self)
    }

    fn as_f64(&self) -> Result<f64, String> {
        ValueReader::as_f64(self)
    }

    fn as_bool(&self) -> Result<bool, String> {
        ValueReader::as_bool(self)
    }

    fn as_string(&self) -> Result<String, String> {
        ValueReader::as_string(self)
    }

    fn is_null(&self) -> bool {
        ValueReader::is_null(self)
    }
}

impl RuntimeDataLoader for FsMessagePackRuntimeDataLoader {
    type Reader = ValueReader;

    fn open(&self, path: &str) -> Result<Self::Reader, String> {
        self.open_reader(path)
    }
}

fn fixture_loader_from_env() -> FsMessagePackRuntimeDataLoader {
    FsMessagePackRuntimeDataLoader::new(
        env::var("DATA_DIR").expect("DATA_DIR should point to exported data"),
    )
}

#[test]
fn generated_rust_loads_exported_messagepack_and_queries_indexes() {
    let data = GameData::load(&fixture_loader_from_env()).expect("generated data should load");

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
