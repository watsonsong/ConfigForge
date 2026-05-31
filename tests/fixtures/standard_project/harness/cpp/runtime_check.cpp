// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#include "reference_game_config.hpp"

#define CONFIG_FORGE_RUNTIME_CPP_READER reference::game::config::IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER reference::game::config::IRuntimeDataLoader

#if defined(CONFIG_FORGE_USE_BINARY_RUNTIME_READER)
#include "config_forge/runtime/binary_reader/runtime_loader.hpp"
#else
#include "config_forge/runtime/rapidjson_reader/runtime_loader.hpp"
#endif

#include <cstdlib>
#include <stdexcept>

namespace cfg = reference::game::config;

int main() {
    const char* data_dir = std::getenv("DATA_DIR");
    if (data_dir == nullptr) {
        throw std::runtime_error("DATA_DIR should point to exported runtime data");
    }

#if defined(CONFIG_FORGE_USE_BINARY_RUNTIME_READER)
    config_forge_runtime::BinaryFsRuntimeDataLoader loader(data_dir);
#else
    config_forge_runtime::JsonFsRuntimeDataLoader loader(data_dir);
    if (!config_forge_runtime::JsonValueReader("\"1\"").AsBool() ||
        config_forge_runtime::JsonValueReader("\"0\"").AsBool()) {
        throw std::runtime_error("numeric bool string parsing failed");
    }
#endif

    cfg::GameData data = cfg::GameData::Load(loader);

    const cfg::ItemsRow* item = data.Items().GetById(2001);
    if (item == nullptr || item->name != "Iron Ore" || item->price != 5) {
        throw std::runtime_error("single-field item index query failed");
    }
    if (!item->stack.has_value() || item->stack->max_stack != 999) {
        throw std::runtime_error("fragment payload load failed");
    }
    if (data.Items().GetById(404) != nullptr) {
        throw std::runtime_error("missing item index query should return null");
    }

    const cfg::ClassLevelsRow* class_level = data.ClassLevels().GetByClassLevel(1, 2);
    if (class_level == nullptr || class_level->attributes.max_health != 145) {
        throw std::runtime_error("compound class level index query failed");
    }

    if (data.GameRules().max_player_level != 60 || !data.GameRules().pvp_enabled) {
        throw std::runtime_error("object table load failed");
    }
    if (data.GameRules().default_class_id != 1 || data.GameRules().starter_item_ids.size() != 2 ||
        data.GameRules().starter_item_ids[0] != 2001 || data.GameRules().starter_item_ids[1] != 2002) {
        throw std::runtime_error("object table array field load failed");
    }
    const cfg::ClassesRow& default_class = data.GameRules().GetDefaultClass(data);
    if (default_class.class_id != 1) {
        throw std::runtime_error("object table single relation failed");
    }
    const std::vector<const cfg::ItemsRow*> starter_items = data.GameRules().GetStarterItems(data);
    if (starter_items.size() != 2 || starter_items[0] == nullptr || starter_items[0]->id != 2001 ||
        starter_items[1] == nullptr || starter_items[1]->id != 2002) {
        throw std::runtime_error("object table list relation failed");
    }
    data.LinkAll();
    if (data.GameRules().GetDefaultClass(data).class_id != 1) {
        throw std::runtime_error("link all relation cache failed");
    }
    return 0;
}
