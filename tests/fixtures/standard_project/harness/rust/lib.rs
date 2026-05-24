// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

include!("generated.rs");

mod runtime_loader;

use std::env;

use runtime_loader::FsMessagePackRuntimeDataLoader;

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
