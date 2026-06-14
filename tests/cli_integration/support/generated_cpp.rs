// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, process::Command as StdCommand};

use super::{assert_command_success, require_tool_or_skip};

pub(crate) fn verify_generated_cpp_runtime(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("compile_check.cpp"),
        r#"
            #include "generated.hpp"

            #include <memory>
            #include <stdexcept>
            #include <vector>

            class MockReader final : public IRuntimeDataReader {
            public:
                explicit MockReader(std::string path) : path_(std::move(path)) {}

                void EnterIndex(std::size_t index) override {
                    if (path_ == "items" && CurrentField() == "rows" && index == 0) {
                        fields_.push_back("");
                        return;
                    }
                    if (path_ == "game_rules" && CurrentField() == "starter_item_ids" && index < 2) {
                        fields_.push_back("");
                        return;
                    }
                    if (path_ == "game_rules" && CurrentField() == "item_ids" && index < 2) {
                        fields_.push_back(index == 0 ? "starter_reward.item_ids[0]" : "starter_reward.item_ids[1]");
                        return;
                    }
                    throw std::runtime_error("unexpected index");
                }

                void ExitField() override {
                    if (fields_.empty()) { throw std::runtime_error("unexpected exit"); }
                    fields_.pop_back();
                }

                std::size_t ReadRowCount() override {
                    if (path_ == "items" && CurrentField() == "rows") {
                        return 1;
                    }
                    if ((path_ == "classes" || path_ == "class_levels" || path_ == "character_surnames" || path_ == "character_male_given_names" || path_ == "character_female_given_names" || path_ == "quests" || path_ == "loot_pools") && CurrentField() == "rows") {
                        return 0;
                    }
                    if (path_ == "game_rules" && (CurrentField() == "starter_item_ids" || CurrentField() == "item_ids")) {
                        return 2;
                    }
                    throw std::runtime_error("unexpected row count");
                }

                std::size_t ReadListLength() override {
                    if (path_ == "game_rules" && (CurrentField() == "starter_item_ids" || CurrentField() == "item_ids" || CurrentField().empty())) {
                        return 2;
                    }
                    throw std::runtime_error("unexpected list length");
                }

                std::int64_t AsInt64() const override {
                    if (path_ == "items" && CurrentField() == "id") { return 1; }
                    if (path_ == "items" && CurrentField() == "price") { return 10; }
                    if (path_ == "game_rules" && CurrentField() == "max_player_level") { return 60; }
                    if (path_ == "game_rules" && CurrentField() == "starter_gold") { return 500; }
                    if (path_ == "game_rules" && CurrentField() == "default_class_id") { return 1; }
                    if (path_ == "game_rules" && CurrentField() == "") { return 2001; }
                    if (path_ == "game_rules" && CurrentField() == "xp") { return 0; }
                    if (path_ == "game_rules" && CurrentField() == "gold") { return 500; }
                    if (path_ == "game_rules" && CurrentField() == "starter_reward.item_ids[0]") { return 2001; }
                    if (path_ == "game_rules" && CurrentField() == "starter_reward.item_ids[1]") { return 2002; }
                    if (path_ == "game_rules" && CurrentField() == "stamina_cap") { return 120; }
                    if (path_ == "game_rules" && CurrentField() == "daily_reset_hour") { return 5; }
                    throw std::runtime_error("unexpected int64");
                }

                double AsDouble() const override {
                    if (path_ == "game_rules" && ParentField() == "ambient_tint" && CurrentField() == "r") { return 51.0; }
                    if (path_ == "game_rules" && ParentField() == "ambient_tint" && CurrentField() == "g") { return 102.0; }
                    if (path_ == "game_rules" && ParentField() == "ambient_tint" && CurrentField() == "b") { return 153.0; }
                    if (path_ == "game_rules" && ParentField() == "ambient_tint" && CurrentField() == "a") { return 255.0; }
                    if (path_ == "game_rules" && ParentField() == "spawn_position" && CurrentField() == "x") { return 1.5; }
                    if (path_ == "game_rules" && ParentField() == "spawn_position" && CurrentField() == "y") { return 2.5; }
                    if (path_ == "game_rules" && ParentField() == "spawn_position" && CurrentField() == "z") { return 3.5; }
                    if (path_ == "game_rules" && ParentField() == "grid_size" && CurrentField() == "x") { return 16.0; }
                    if (path_ == "game_rules" && ParentField() == "grid_size" && CurrentField() == "y") { return 9.0; }
                    return static_cast<double>(AsInt64());
                }

                bool AsBool() const override {
                    if (path_ == "game_rules" && CurrentField() == "pvp_enabled") { return true; }
                    throw std::runtime_error("unexpected bool");
                }

                std::string AsString() const override {
                    if (path_ == "items" && CurrentField() == "name") { return "Potion"; }
                    throw std::runtime_error("unexpected string");
                }

                void EnterRequiredField(const std::string& name) override {
                    if (!EnterNamedField(name)) { throw std::runtime_error("missing required field"); }
                }

                bool EnterOptionalField(const std::string& name) override {
                    return EnterNamedField(name);
                }
            private:
                bool EnterNamedField(const std::string& name) {
                    if (name == "stack" || name == "attributes" || name == "skill") {
                        return false;
                    }
                    fields_.push_back(name);
                    return true;
                }

                const std::string& CurrentField() const {
                    static const std::string root;
                    return fields_.empty() ? root : fields_.back();
                }

                const std::string& ParentField() const {
                    static const std::string root;
                    return fields_.size() < 2 ? root : fields_[fields_.size() - 2];
                }

                std::string path_;
                std::vector<std::string> fields_;
            };

            class MockLoader final : public IRuntimeDataLoader {
            public:
                std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) override {
                    if (path == "items" || path == "classes" || path == "class_levels" || path == "character_surnames" || path == "character_male_given_names" || path == "character_female_given_names" || path == "quests" || path == "loot_pools" || path == "game_rules") {
                        return std::make_unique<MockReader>(path);
                    }
                    throw std::runtime_error("unexpected path");
                }
            };

            int main() {
                ItemsRow row{};
                row.id = 1;
                row.name = "Potion";
                row.price = 10;
                TextTranslator translator = [](const char* context, const std::string& msgid) -> std::optional<std::string> {
                    if (std::string(context) == "items.name" && msgid == "Potion") {
                        return std::string("Potion CN");
                    }
                    return std::nullopt;
                };
                if (row.NameText(translator) != "Potion CN") {
                    return 6;
                }
                (void)row;
                GameRulesRow rules{};
                rules.max_player_level = 60;
                rules.starter_gold = 500;
                rules.default_class_id = 1;
                rules.starter_item_ids = {2001, 2002};
                rules.starter_reward.gold = 500;
                rules.starter_reward.item_ids = {2001, 2002};
                rules.ambient_tint = ConfigForgeColor{51.0, 102.0, 153.0, 255.0};
                rules.spawn_position = ConfigForgeVector3{1.5, 2.5, 3.5};
                rules.grid_size = ConfigForgeVector2Int{16, 9};
                rules.stamina_cap = 120;
                rules.daily_reset_hour = 5;
                rules.pvp_enabled = true;
                if (std::string(GameRulesRow::DataFile) != "game_rules") {
                    return 1;
                }
                (void)rules;
                MockLoader loader;
                GameData data = GameData::Load(loader);
                const ItemsRow* item = data.Items().GetById(1);
                if (item == nullptr || item->name != "Potion") {
                    return 2;
                }
                if (data.Items().GetById(404) != nullptr) {
                    return 3;
                }
                if (data.GameRules().max_player_level != 60) {
                    return 4;
                }
                if (data.GameRules().default_class_id != 1 || data.GameRules().starter_item_ids.size() != 2) {
                    return 5;
                }
                if (data.GameRules().starter_reward.gold != 500 || data.GameRules().starter_reward.item_ids.size() != 2) {
                    return 7;
                }
                if (data.GameRules().ambient_tint.r != 51.0 || data.GameRules().ambient_tint.g != 102.0 || data.GameRules().ambient_tint.b != 153.0 || data.GameRules().ambient_tint.a != 255.0) {
                    return 8;
                }
                if (data.GameRules().spawn_position.x != 1.5 || data.GameRules().spawn_position.y != 2.5 || data.GameRules().spawn_position.z != 3.5) {
                    return 9;
                }
                if (data.GameRules().grid_size.x != 16 || data.GameRules().grid_size.y != 9) {
                    return 10;
                }
                return 0;
            }
        "#,
    )
    .expect("generated C++ compile check should be written");

    if require_tool_or_skip("generated C++ runtime check", "c++").is_none() {
        return;
    }

    let output = StdCommand::new("c++")
        .args([
            "-std=c++17",
            generated_dir
                .join("compile_check.cpp")
                .to_str()
                .expect("utf8 path"),
            generated_dir
                .join("generated.cpp")
                .to_str()
                .expect("utf8 path"),
            "-o",
            generated_dir
                .join("compile_check")
                .to_str()
                .expect("utf8 path"),
        ])
        .output()
        .expect("generated C++ compile check should compile");
    assert_command_success(output, "generated C++ header compile");

    let output = StdCommand::new(generated_dir.join("compile_check"))
        .output()
        .expect("generated C++ compile check should run");
    assert_command_success(output, "generated C++ runtime check");
}
pub(crate) fn verify_generated_cpp_global_i18n(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("i18n.hpp"),
        r#"
            #pragma once
            #include <string>

            inline std::string GlobalTranslate(const char* context, const char* msgid) {
                if (std::string(context) == "items.name" && std::string(msgid) == "Potion") {
                    return "Potion CN";
                }
                return msgid;
            }
        "#,
    )
    .expect("generated C++ i18n header should be written");
    fs::write(
        generated_dir.join("global_i18n_check.cpp"),
        r#"
            #include "generated.hpp"

            int main() {
                ItemsRow row{};
                row.id = 1;
                row.name = "Potion";
                row.price = 10;
                return row.NameText() == "Potion CN" ? 0 : 1;
            }
        "#,
    )
    .expect("generated C++ global i18n check should be written");

    if require_tool_or_skip("generated C++ global i18n check", "c++").is_none() {
        return;
    }

    let output = StdCommand::new("c++")
        .args(["-std=c++17"])
        .arg(generated_dir.join("global_i18n_check.cpp"))
        .arg(generated_dir.join("generated.cpp"))
        .arg("-o")
        .arg(generated_dir.join("global_i18n_check"))
        .output()
        .expect("generated C++ global i18n check should compile");
    assert_command_success(output, "generated C++ global i18n compile");

    let output = StdCommand::new(generated_dir.join("global_i18n_check"))
        .output()
        .expect("generated C++ global i18n check should run");
    assert_command_success(output, "generated C++ global i18n runtime check");
}
