#pragma once

#include <optional>
#include <string>

#if defined(CONFIG_FORGE_USE_LIBINTL)

#include <clocale>
#include <libintl.h>

inline void InitializeGlobalTextDomain(const char* locale_dir) {
    setlocale(LC_ALL, "");
    bindtextdomain("reference_game_config", locale_dir);
    bind_textdomain_codeset("reference_game_config", "UTF-8");
    textdomain("reference_game_config");
}

inline std::optional<std::string> GlobalTranslate(const char* context, const char* msgid) {
    if (context == nullptr || msgid == nullptr) {
        return std::nullopt;
    }
    const std::string key = std::string(context) + '\x04' + msgid;
    const char* translated = dgettext("reference_game_config", key.c_str());
    if (translated == key) {
        return std::nullopt;
    }
    return translated;
}

#else

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

#endif
