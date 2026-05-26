// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

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
    return 0;
}
