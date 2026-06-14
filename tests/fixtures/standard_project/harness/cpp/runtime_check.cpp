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
#include <cstdint>
#include <fstream>
#include <iterator>
#include <memory>
#include <stdexcept>
#include <unordered_map>
#include <optional>
#include <string>

namespace cfg = reference::game::config;

namespace {

std::uint32_t ReadLe32(const std::string& data, std::size_t offset) {
    return static_cast<std::uint32_t>(static_cast<unsigned char>(data[offset])) |
           (static_cast<std::uint32_t>(static_cast<unsigned char>(data[offset + 1])) << 8U) |
           (static_cast<std::uint32_t>(static_cast<unsigned char>(data[offset + 2])) << 16U) |
           (static_cast<std::uint32_t>(static_cast<unsigned char>(data[offset + 3])) << 24U);
}

std::unordered_map<std::string, std::string> LoadMoCatalog(const char* path) {
    std::ifstream input(path, std::ios::binary);
    if (!input) {
        throw std::runtime_error("failed to open MO catalog");
    }
    const std::string data((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
    if (data.size() < 28 || ReadLe32(data, 0) != 0x950412deU) {
        throw std::runtime_error("unsupported MO catalog");
    }
    const std::uint32_t count = ReadLe32(data, 8);
    const std::uint32_t original_table = ReadLe32(data, 12);
    const std::uint32_t translated_table = ReadLe32(data, 16);
    std::unordered_map<std::string, std::string> catalog;
    for (std::uint32_t index = 0; index < count; ++index) {
        const std::uint32_t original_length = ReadLe32(data, original_table + index * 8);
        const std::uint32_t original_offset = ReadLe32(data, original_table + index * 8 + 4);
        const std::uint32_t translated_length = ReadLe32(data, translated_table + index * 8);
        const std::uint32_t translated_offset = ReadLe32(data, translated_table + index * 8 + 4);
        catalog.emplace(data.substr(original_offset, original_length),
                        data.substr(translated_offset, translated_length));
    }
    return catalog;
}

void AssertTranslatedText(const cfg::GameData& data) {
    const cfg::ItemsRow* item = data.Items().GetById(2001);
    if (item == nullptr || item->NameText() != "铁矿石") {
        throw std::runtime_error("item text translation failed");
    }
    const cfg::ClassesRow* warrior = data.Classes().GetById(1);
    if (warrior == nullptr || warrior->NameText() != "战士") {
        throw std::runtime_error("class name translation failed");
    }
    if (warrior->DescriptionText() != "耐久的近战战士。") {
        throw std::runtime_error("class description translation failed");
    }
}

} // namespace

int main() {
    const char* data_dir = std::getenv("DATA_DIR");
    if (data_dir == nullptr) {
        throw std::runtime_error("DATA_DIR should point to exported runtime data");
    }
    const char* i18n_mo = std::getenv("I18N_MO");
#if defined(CONFIG_FORGE_USE_LIBINTL)
    const char* i18n_locale_dir = std::getenv("I18N_LOCALE_DIR");
    if (i18n_locale_dir == nullptr) {
        throw std::runtime_error("I18N_LOCALE_DIR should point to the gettext locale directory");
    }
    InitializeGlobalTextDomain(i18n_locale_dir);
#else
    if (i18n_mo == nullptr) {
        throw std::runtime_error("I18N_MO should point to the compiled gettext MO catalog");
    }
    const auto catalog = LoadMoCatalog(i18n_mo);
    SetGlobalTextCatalog(&catalog);
#endif

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
    if (item == nullptr || item->name != "Iron Ore" || item->price != 5 || item->rarity_id != 2) {
        throw std::runtime_error("single-field item index query failed");
    }
    if (!item->stack.has_value() || item->stack->max_stack != 999) {
        throw std::runtime_error("fragment payload load failed");
    }
    const cfg::RarityLevelsRow& item_rarity = item->GetRarity(data);
    if (item_rarity.rarity_id != 2 || item_rarity.NameText() != "普通" ||
        item_rarity.standard_color.r != 255.0 || item_rarity.standard_color.g != 255.0 ||
        item_rarity.standard_color.b != 255.0 || item_rarity.background_icon != "assets/icons/rarity/common.png") {
        throw std::runtime_error("item rarity relation failed");
    }
    const cfg::RarityLevelsRow* epic = data.RarityLevels().GetById(5);
    if (epic == nullptr || epic->rarity_id != 5 || epic->standard_color.r != 163.0) {
        throw std::runtime_error("rarity id index failed");
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
    if (data.GameRules().starter_reward.gold != 500 || data.GameRules().starter_reward.item_ids.size() != 2 ||
        data.GameRules().starter_reward.item_ids[0] != 2001 || data.GameRules().starter_reward.item_ids[1] != 2002) {
        throw std::runtime_error("object table fixed reward field load failed");
    }
    if (data.GameRules().spawn_position.x != 1.5 || data.GameRules().spawn_position.y != 2.5 ||
        data.GameRules().spawn_position.z != 3.5) {
        throw std::runtime_error("object table vector field load failed");
    }
    if (data.GameRules().grid_size.x != 16 || data.GameRules().grid_size.y != 9) {
        throw std::runtime_error("object table int vector field load failed");
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
    if (data.CharacterSurnames().Size() != 4 || data.CharacterSurnames().RowAt(0)->value != "Stone") {
        throw std::runtime_error("source-order surname table failed");
    }
    const cfg::CharacterSurnamesRow* surname = data.CharacterSurnames().GetByValue("Frost");
    if (surname == nullptr || surname->value != "Frost") {
        throw std::runtime_error("indexed source-order surname table failed");
    }
    if (data.CharacterMaleGivenNames().Rows()[1].value != "Borin" || data.CharacterFemaleGivenNames().Rows()[2].value != "Celeste") {
        throw std::runtime_error("source-order given name table failed");
    }
    AssertTranslatedText(data);

#if defined(CONFIG_FORGE_USE_BINARY_RUNTIME_READER)
    auto lazy_loader = std::make_shared<config_forge_runtime::BinaryFsRuntimeDataLoader>(data_dir);
#else
    auto lazy_loader = std::make_shared<config_forge_runtime::JsonFsRuntimeDataLoader>(data_dir);
#endif
    cfg::LazyGameData lazy_data(lazy_loader);
    const cfg::ItemsRow* lazy_item = lazy_data.Items().GetById(2001);
    if (lazy_item == nullptr || lazy_item->id != 2001 || lazy_item->GetRarity(lazy_data).rarity_id != 2) {
        throw std::runtime_error("lazy item table load failed");
    }
    if (lazy_data.GameRules().default_class_id != 1) {
        throw std::runtime_error("lazy object table load failed");
    }
    lazy_data.LinkAll();
    if (lazy_data.GameRules().GetDefaultClass(lazy_data).class_id != 1) {
        throw std::runtime_error("lazy link all relation cache failed");
    }
    return 0;
}
