#pragma once

#include <optional>
#include <string>
#include <unordered_map>

inline const std::unordered_map<std::string, std::string>* g_config_forge_text_catalog = nullptr;

inline void SetGlobalTextCatalog(const std::unordered_map<std::string, std::string>* catalog) {
    g_config_forge_text_catalog = catalog;
}

inline std::optional<std::string> GlobalTranslate(const char* context, const char* msgid) {
    if (g_config_forge_text_catalog == nullptr || context == nullptr || msgid == nullptr) {
        return std::nullopt;
    }
    const std::string key = std::string(context) + '\x04' + msgid;
    const auto iterator = g_config_forge_text_catalog->find(key);
    if (iterator == g_config_forge_text_catalog->end()) {
        return std::nullopt;
    }
    return iterator->second;
}
