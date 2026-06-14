// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, process::Command as StdCommand};

use super::assert_command_success;

pub(crate) fn verify_generated_rust_crate(generated_dir: &std::path::Path, derive_serde: bool) {
    let reader_core_dependency = format!(
        "config-forge-runtime-reader-core = {{ path = {:?} }}",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/rust/reader_core")
    );
    for edition in ["2021", "2024"] {
        fs::write(
            generated_dir.join("Cargo.toml"),
            format!(
                r#"
                [package]
                name = "generated_config_check"
                version = "0.1.0"
                edition = "{edition}"

                [lib]
                path = "lib.rs"

                [dependencies]
                {reader_core_dependency}
                {dependencies}
            "#,
                edition = edition,
                reader_core_dependency = reader_core_dependency,
                dependencies = if derive_serde {
                    "serde = { version = \"1.0\", features = [\"derive\"] }\nserde_json = \"1.0\""
                } else {
                    ""
                }
            ),
        )
        .expect("generated rust Cargo.toml should be written");
        fs::write(
        generated_dir.join("lib.rs"),
        format!(
            r#"
                include!("generated.rs");

                struct MockReader {{
                    path: &'static str,
                    fields: std::cell::RefCell<Vec<&'static str>>,
                }}

                impl MockReader {{
                    fn root(path: &'static str) -> Self {{
                        Self {{ path, fields: std::cell::RefCell::new(Vec::new()) }}
                    }}

                    fn current_field(&self) -> Option<&'static str> {{
                        self.fields.borrow().last().copied()
                    }}

                    fn parent_field(&self) -> Option<&'static str> {{
                        let fields = self.fields.borrow();
                        fields.len().checked_sub(2).and_then(|index| fields.get(index).copied())
                    }}

                    fn enter_field(&self, name: &'static str) -> RuntimeDataResult<bool> {{
                        let field = match name {{
                            "rows" => "rows",
                            "id" => "id",
                            "rarity_id" => "rarity_id",
                            "name" => "name",
                            "standard_color" => "standard_color",
                            "background_icon" => "background_icon",
                            "price" => "price",
                            "max_player_level" => "max_player_level",
                            "starter_gold" => "starter_gold",
                            "default_class_id" => "default_class_id",
                            "starter_item_ids" => "starter_item_ids",
                            "starter_reward" => "starter_reward",
                            "xp" => "xp",
                            "gold" => "gold",
                            "item_ids" => "item_ids",
                            "spawn_position" => "spawn_position",
                            "grid_size" => "grid_size",
                            "r" => "r",
                            "g" => "g",
                            "b" => "b",
                            "a" => "a",
                            "x" => "x",
                            "y" => "y",
                            "z" => "z",
                            "stamina_cap" => "stamina_cap",
                            "daily_reset_hour" => "daily_reset_hour",
                            "pvp_enabled" => "pvp_enabled",
                            "stack" | "attributes" | "skill" => return Ok(false),
                            other => return Err(RuntimeDataError::not_found(format!("unexpected field: {{other}}"))),
                        }};
                        self.fields.borrow_mut().push(field);
                        Ok(true)
                    }}

                    fn len(&self) -> RuntimeDataResult<usize> {{
                        if self.path == "items" && self.current_field() == Some("rows") {{ Ok(1) }} else if self.path == "rarity_levels" && self.current_field() == Some("rows") {{ Ok(1) }} else if matches!(self.path, "classes" | "class_levels" | "character_surnames" | "character_male_given_names" | "character_female_given_names" | "quests" | "loot_pools") && self.current_field() == Some("rows") {{ Ok(0) }} else if self.path == "game_rules" && matches!(self.current_field(), Some("starter_item_ids" | "item_ids")) {{ Ok(2) }} else {{ Err(RuntimeDataError::type_mismatch("unexpected len")) }}
                    }}
                }}

                impl RuntimeDataReader for MockReader {{
                    fn enter_required_field(&self, name: &'static str) -> RuntimeDataResult<()> {{
                        if !self.enter_field(name)? {{
                            return Err(RuntimeDataError::not_found(format!("missing required field: {{name}}")));
                        }}
                        Ok(())
                    }}

                    fn enter_optional_field(&self, name: &'static str) -> RuntimeDataResult<bool> {{
                        self.enter_field(name)
                    }}

                    fn exit_field(&self) -> RuntimeDataResult<()> {{ self.exit() }}

                    fn enter_index(&self, index: usize) -> RuntimeDataResult<()> {{
                        if self.path == "items" && self.current_field() == Some("rows") && index == 0 {{
                            self.fields.borrow_mut().push("");
                            Ok(())
                        }} else if self.path == "rarity_levels" && self.current_field() == Some("rows") && index == 0 {{
                            self.fields.borrow_mut().push("");
                            Ok(())
                        }} else if self.path == "game_rules" && self.current_field() == Some("starter_item_ids") && index < 2 {{
                            self.fields.borrow_mut().push(if index == 0 {{ "starter_item_ids[0]" }} else {{ "starter_item_ids[1]" }});
                            Ok(())
                        }} else if self.path == "game_rules" && self.current_field() == Some("item_ids") && index < 2 {{
                            self.fields.borrow_mut().push(if index == 0 {{ "starter_reward.item_ids[0]" }} else {{ "starter_reward.item_ids[1]" }});
                            Ok(())
                        }} else {{
                            Err(RuntimeDataError::not_found(format!("unexpected index: {{index}}")))
                        }}
                    }}

                    fn exit(&self) -> RuntimeDataResult<()> {{
                        self.fields
                            .borrow_mut()
                            .pop()
                            .map(|_| ())
                            .ok_or_else(|| RuntimeDataError::decode("unexpected exit"))
                    }}

                    fn read_row_count(&self) -> RuntimeDataResult<usize> {{ self.len() }}

                    fn read_list_len(&self) -> RuntimeDataResult<usize> {{ self.len() }}

                    fn array_elements_are_positional(&self) -> bool {{ false }}

                    fn begin_record(&self, _optional_slot_count: usize) -> RuntimeDataResult<()> {{ Ok(()) }}

                    fn end_record(&self) -> RuntimeDataResult<()> {{ Ok(()) }}

                    fn read_enum_index(&self, variants: &[&str]) -> RuntimeDataResult<usize> {{
                        let value = self.as_string()?;
                        variants.iter().position(|variant| *variant == value.as_str()).ok_or_else(|| RuntimeDataError::decode(format!("unknown enum value `{{value}}`")))
                    }}

                    fn as_i64(&self) -> RuntimeDataResult<i64> {{
                        match (self.path, self.current_field()) {{
                            ("items", Some("id")) => Ok(1),
                            ("items", Some("rarity_id")) => Ok(2),
                            ("items", Some("price")) => Ok(10),
                            ("rarity_levels", Some("rarity_id")) => Ok(2),
                            ("game_rules", Some("max_player_level")) => Ok(60),
                            ("game_rules", Some("starter_gold")) => Ok(500),
                            ("game_rules", Some("default_class_id")) => Ok(1),
                            ("game_rules", Some("starter_item_ids[0]")) => Ok(2001),
                            ("game_rules", Some("starter_item_ids[1]")) => Ok(2002),
                            ("game_rules", Some("xp")) => Ok(0),
                            ("game_rules", Some("gold")) => Ok(500),
                            ("game_rules", Some("starter_reward.item_ids[0]")) => Ok(2001),
                            ("game_rules", Some("starter_reward.item_ids[1]")) => Ok(2002),
                            ("game_rules", Some("stamina_cap")) => Ok(120),
                            ("game_rules", Some("daily_reset_hour")) => Ok(5),
                            _ => Err(RuntimeDataError::type_mismatch("unexpected i64")),
                        }}
                    }}

                    fn as_f64(&self) -> RuntimeDataResult<f64> {{
                        match (self.path, self.parent_field(), self.current_field()) {{
                            ("rarity_levels", Some("standard_color"), Some("r")) => Ok(255.0),
                            ("rarity_levels", Some("standard_color"), Some("g")) => Ok(255.0),
                            ("rarity_levels", Some("standard_color"), Some("b")) => Ok(255.0),
                            ("rarity_levels", Some("standard_color"), Some("a")) => Ok(255.0),
                            ("game_rules", Some("spawn_position"), Some("x")) => Ok(1.5),
                            ("game_rules", Some("spawn_position"), Some("y")) => Ok(2.5),
                            ("game_rules", Some("spawn_position"), Some("z")) => Ok(3.5),
                            ("game_rules", Some("grid_size"), Some("x")) => Ok(16.0),
                            ("game_rules", Some("grid_size"), Some("y")) => Ok(9.0),
                            _ => Ok(self.as_i64()? as f64),
                        }}
                    }}

                    fn as_bool(&self) -> RuntimeDataResult<bool> {{
                        if self.path == "game_rules" && self.current_field() == Some("pvp_enabled") {{ Ok(true) }} else {{ Err(RuntimeDataError::type_mismatch("unexpected bool")) }}
                    }}

                    fn as_string(&self) -> RuntimeDataResult<String> {{
                        match (self.path, self.current_field()) {{
                            ("items", Some("name")) => Ok("Potion".to_owned()),
                            ("rarity_levels", Some("name")) => Ok("Common".to_owned()),
                            ("rarity_levels", Some("background_icon")) => Ok("assets/icons/rarity/common.png".to_owned()),
                            _ => Err(RuntimeDataError::type_mismatch("unexpected string")),
                        }}
                    }}

                    fn schema_revision(&self) -> Option<u64> {{ None }}

                    fn ensure_exhausted(&self) -> RuntimeDataResult<()> {{ Ok(()) }}

                }}

                struct MockLoader;

                impl RuntimeDataLoader for MockLoader {{
                    type Reader = MockReader;

                    fn open(&self, path: &str) -> RuntimeDataResult<Self::Reader> {{
                        match path {{
                            "items" => Ok(MockReader::root("items")),
                            "rarity_levels" => Ok(MockReader::root("rarity_levels")),
                            "classes" => Ok(MockReader::root("classes")),
                            "class_levels" => Ok(MockReader::root("class_levels")),
                            "character_surnames" => Ok(MockReader::root("character_surnames")),
                            "character_male_given_names" => Ok(MockReader::root("character_male_given_names")),
                            "character_female_given_names" => Ok(MockReader::root("character_female_given_names")),
                            "quests" => Ok(MockReader::root("quests")),
                            "loot_pools" => Ok(MockReader::root("loot_pools")),
                            "game_rules" => Ok(MockReader::root("game_rules")),
                            other => Err(RuntimeDataError::not_found(format!("unexpected path: {{other}}"))),
                        }}
                    }}
                }}

                struct MockTranslator;

                impl TextTranslator for MockTranslator {{
                    fn translate(&self, msgctxt: Option<&str>, msgid: &str) -> Option<String> {{
                        match (msgctxt, msgid) {{
                            (Some("items.name"), "Potion") => Some("药水".to_owned()),
                            _ => None,
                        }}
                    }}
                }}

                #[test]
                fn generated_code_loads_and_queries_rows() {{
                    let data = GameData::load(&MockLoader).expect("generated data should load");
                    let item = data.items.get_by_id(1).expect("item should be indexed");
                    assert_eq!(item.name, "Potion");
                    assert_eq!(item.rarity_id, 2);
                    assert_eq!(item.name_text(&MockTranslator), "药水");
                    let rarity = data.rarity_levels.get_by_id(2).expect("rarity should be indexed");
                    assert_eq!(rarity.rarity_id, 2);
                    assert_eq!(rarity.standard_color.r, 255.0);
                    assert_eq!(rarity.background_icon, "assets/icons/rarity/common.png");
                    assert_eq!(data.game_rules.max_player_level, 60);
                    assert!(data.items.get_by_id(404).is_none());
                    assert_eq!(data.game_rules.default_class_id, 1);
                    assert_eq!(data.game_rules.starter_item_ids, vec![2001, 2002]);
                    assert_eq!(data.game_rules.starter_reward.gold, 500);
                    assert_eq!(data.game_rules.starter_reward.item_ids, vec![2001, 2002]);
                    assert_eq!(data.game_rules.spawn_position.x, 1.5);
                    assert_eq!(data.game_rules.spawn_position.y, 2.5);
                    assert_eq!(data.game_rules.spawn_position.z, 3.5);
                    assert_eq!(data.game_rules.grid_size.x, 16);
                    assert_eq!(data.game_rules.grid_size.y, 9);
                    assert!(data.game_rules.pvp_enabled);
                }}

                {serde_assertion}
            "#,
            serde_assertion = if derive_serde {
                "#[test]\nfn generated_rows_support_serde() {\n    let _ = serde_json::to_string(&ItemsRow { id: 1, name: \"Potion\".to_owned(), rarity_id: 2, price: 10, stack: None, attributes: None, skill: None, rarity_link: RefCell::new(LinkCache::default()) }).unwrap();\n    let _ = serde_json::to_string(&RarityLevelsRow { rarity_id: 2, name: \"Common\".to_owned(), standard_color: ConfigForgeColor { r: 255.0, g: 255.0, b: 255.0, a: 255.0 }, background_icon: \"assets/icons/rarity/common.png\".to_owned() }).unwrap();\n    let _ = serde_json::to_string(&GameRulesRow { max_player_level: 60, starter_gold: 500, default_class_id: 1, starter_item_ids: vec![2001], starter_reward: FixedReward { xp: 0, gold: 500, item_ids: vec![2001] }, spawn_position: ConfigForgeVector3 { x: 1.5, y: 2.5, z: 3.5 }, grid_size: ConfigForgeVector2Int { x: 16, y: 9 }, stamina_cap: 120, daily_reset_hour: 5, pvp_enabled: true, default_class_link: RefCell::new(LinkCache::default()), starter_items_link: RefCell::new(LinkListCache::default()) }).unwrap();\n}"
            } else {
                ""
            }
        ),
    )
    .expect("generated rust lib.rs should be written");

        let output = StdCommand::new("cargo")
            .args(["test", "--quiet", "--manifest-path"])
            .arg(generated_dir.join("Cargo.toml"))
            .output()
            .expect("generated Rust cargo test should run");
        assert_command_success(output, &format!("generated Rust cargo test ({edition})"));
    }
}
pub(crate) fn verify_generated_rust_global_i18n(generated_dir: &std::path::Path) {
    let reader_core_dependency = format!(
        "config-forge-runtime-reader-core = {{ path = {:?} }}",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/rust/reader_core")
    );
    fs::write(
        generated_dir.join("Cargo.toml"),
        format!(
            r#"
                [package]
                name = "generated_global_i18n_check"
                version = "0.1.0"
                edition = "2024"

                [lib]
                path = "lib.rs"

                [dependencies]
                {reader_core_dependency}
            "#
        ),
    )
    .expect("generated global i18n Rust Cargo.toml should be written");
    fs::write(
        generated_dir.join("lib.rs"),
        r#"
            fn global_translate(context: Option<&str>, msgid: &str) -> String {
                if context == Some("items.name") && msgid == "Potion" {
                    "Potion CN".to_owned()
                } else {
                    msgid.to_owned()
                }
            }

            include!("generated.rs");

            #[test]
            fn global_i18n_text_helper_does_not_require_translator_arg() {
                let row = ItemsRow {
                    id: 1,
                    name: "Potion".to_owned(),
                    rarity_id: 2,
                    price: 10,
                    stack: None,
                    attributes: None,
                    skill: None,
                    rarity_link: std::cell::Cell::new(LinkCache::default()),
                };
                assert_eq!(row.name_text(), "Potion CN");
            }
        "#,
    )
    .expect("generated global i18n Rust lib.rs should be written");

    let output = StdCommand::new("cargo")
        .args(["test", "--quiet", "--manifest-path"])
        .arg(generated_dir.join("Cargo.toml"))
        .output()
        .expect("generated global i18n Rust cargo test should run");
    assert_command_success(output, "generated Rust global i18n cargo test");
}
