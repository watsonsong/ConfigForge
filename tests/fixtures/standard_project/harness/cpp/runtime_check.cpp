#include "generated.hpp"

#include <cctype>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace cfg = reference::game::config;

class FixtureLoader final : public cfg::IRuntimeDataLoader {
public:
    explicit FixtureLoader(std::string data_dir) : data_dir_(std::move(data_dir)) {}

    std::vector<cfg::ItemsRow> LoadItemsRows(const std::string& path) override {
        const std::string content = Read(path);
        std::vector<cfg::ItemsRow> rows;
        for (const std::string& object : ExtractRows(content, path)) {
            cfg::ItemsRow row{};
            row.id = ParseInt(ExtractString(object, "id", path));
            row.name = ExtractString(object, "name", path);
            row.price = ParseInt(ExtractString(object, "price", path));

            std::string nested;
            if (TryExtractObject(object, "stack", &nested)) {
                cfg::ItemsStackFragment stack{};
                stack.max_stack = ParseInt(ExtractString(nested, "max_stack", path));
                row.stack = stack;
            }
            if (TryExtractObject(object, "attributes", &nested)) {
                row.attributes = ParseAttributeSet(nested, path);
            }
            if (TryExtractObject(object, "skill", &nested)) {
                cfg::ItemsSkillFragment skill{};
                skill.skill_asset = ExtractString(nested, "skill_asset", path);
                row.skill = skill;
            }
            rows.push_back(row);
        }
        return rows;
    }

    std::vector<cfg::ClassesRow> LoadClassesRows(const std::string& path) override {
        const std::string content = Read(path);
        std::vector<cfg::ClassesRow> rows;
        for (const std::string& object : ExtractRows(content, path)) {
            cfg::ClassesRow row{};
            row.class_id = ParseInt(ExtractString(object, "class_id", path));
            row.name = ExtractString(object, "name", path);
            row.description = ExtractString(object, "description", path);
            row.icon = ExtractString(object, "icon", path);
            rows.push_back(row);
        }
        return rows;
    }

    std::vector<cfg::ClassLevelsRow> LoadClassLevelsRows(const std::string& path) override {
        const std::string content = Read(path);
        std::vector<cfg::ClassLevelsRow> rows;
        for (const std::string& object : ExtractRows(content, path)) {
            cfg::ClassLevelsRow row{};
            row.class_id = ParseInt(ExtractString(object, "class_id", path));
            row.level = ParseInt(ExtractString(object, "level", path));

            std::string attributes;
            if (!TryExtractObject(object, "attributes", &attributes)) {
                throw std::runtime_error("class level row in `" + path + "` is missing attributes");
            }
            row.attributes = ParseAttributeSet(attributes, path);
            rows.push_back(row);
        }
        return rows;
    }

    cfg::GameRulesRow LoadGameRules(const std::string& path) override {
        const std::string content = Read(path);
        cfg::GameRulesRow rules{};
        rules.max_player_level = ParseInt(ExtractString(content, "max_player_level", path));
        rules.starter_gold = ParseInt(ExtractString(content, "starter_gold", path));
        rules.stamina_cap = ParseInt(ExtractString(content, "stamina_cap", path));
        rules.daily_reset_hour = ParseInt(ExtractString(content, "daily_reset_hour", path));
        rules.pvp_enabled = ParseBool(ExtractString(content, "pvp_enabled", path));
        return rules;
    }

private:
    std::string Read(const std::string& path) const {
        std::ifstream file(data_dir_ + "/" + path);
        if (!file) {
            throw std::runtime_error("failed to open runtime data: " + path);
        }
        std::ostringstream buffer;
        buffer << file.rdbuf();
        return buffer.str();
    }

    static cfg::AttributeSet ParseAttributeSet(const std::string& object, const std::string& path) {
        cfg::AttributeSet attributes{};
        attributes.max_health = ParseInt(ExtractString(object, "max_health", path));
        attributes.max_stamina = ParseInt(ExtractString(object, "max_stamina", path));
        attributes.attack = ParseInt(ExtractString(object, "attack", path));
        attributes.defence = ParseInt(ExtractString(object, "defence", path));
        return attributes;
    }

    static std::vector<std::string> ExtractRows(const std::string& content, const std::string& path) {
        const size_t value_pos = FindFieldValue(content, "rows", path);
        if (value_pos >= content.size() || content[value_pos] != '[') {
            throw std::runtime_error("collection payload `" + path + "` should contain rows");
        }

        std::vector<std::string> rows;
        for (size_t pos = value_pos + 1; pos < content.size();) {
            SkipWhitespace(content, &pos);
            if (pos < content.size() && content[pos] == ']') {
                return rows;
            }
            if (pos < content.size() && content[pos] == '{') {
                std::string row = ExtractBraced(content, pos, path);
                pos += row.size();
                rows.push_back(std::move(row));
                continue;
            }
            ++pos;
        }
        throw std::runtime_error("unterminated rows array in `" + path + "`");
    }

    static bool TryExtractObject(
        const std::string& content,
        const std::string& key,
        std::string* object) {
        const std::string quoted_key = "\"" + key + "\"";
        size_t key_pos = content.find(quoted_key);
        if (key_pos == std::string::npos) {
            return false;
        }
        size_t value_pos = content.find(':', key_pos + quoted_key.size());
        if (value_pos == std::string::npos) {
            return false;
        }
        ++value_pos;
        SkipWhitespace(content, &value_pos);
        if (value_pos >= content.size() || content[value_pos] != '{') {
            return false;
        }
        *object = ExtractBraced(content, value_pos, key);
        return true;
    }

    static std::string ExtractBraced(
        const std::string& content,
        size_t open_pos,
        const std::string& context) {
        int depth = 0;
        bool in_string = false;
        bool escaped = false;
        for (size_t pos = open_pos; pos < content.size(); ++pos) {
            const char ch = content[pos];
            if (in_string) {
                if (escaped) {
                    escaped = false;
                } else if (ch == '\\') {
                    escaped = true;
                } else if (ch == '"') {
                    in_string = false;
                }
                continue;
            }
            if (ch == '"') {
                in_string = true;
            } else if (ch == '{') {
                ++depth;
            } else if (ch == '}') {
                --depth;
                if (depth == 0) {
                    return content.substr(open_pos, pos - open_pos + 1);
                }
            }
        }
        throw std::runtime_error("unterminated object while reading `" + context + "`");
    }

    static std::string ExtractString(
        const std::string& content,
        const std::string& key,
        const std::string& path) {
        size_t pos = FindFieldValue(content, key, path);
        if (pos >= content.size()) {
            throw std::runtime_error("missing value for `" + key + "` in `" + path + "`");
        }
        if (content[pos] == '"') {
            ++pos;
            std::string value;
            bool escaped = false;
            for (; pos < content.size(); ++pos) {
                const char ch = content[pos];
                if (escaped) {
                    value.push_back(ch);
                    escaped = false;
                } else if (ch == '\\') {
                    escaped = true;
                } else if (ch == '"') {
                    return value;
                } else {
                    value.push_back(ch);
                }
            }
            throw std::runtime_error("unterminated string for `" + key + "` in `" + path + "`");
        }

        const size_t end = content.find_first_of(",}\n\r", pos);
        return Trim(content.substr(pos, end == std::string::npos ? std::string::npos : end - pos));
    }

    static size_t FindFieldValue(
        const std::string& content,
        const std::string& key,
        const std::string& path) {
        const std::string quoted_key = "\"" + key + "\"";
        const size_t key_pos = content.find(quoted_key);
        if (key_pos == std::string::npos) {
            throw std::runtime_error("missing key `" + key + "` in runtime data `" + path + "`");
        }
        size_t value_pos = content.find(':', key_pos + quoted_key.size());
        if (value_pos == std::string::npos) {
            throw std::runtime_error("missing ':' after key `" + key + "` in runtime data `" + path + "`");
        }
        ++value_pos;
        SkipWhitespace(content, &value_pos);
        return value_pos;
    }

    static int ParseInt(const std::string& value) {
        return std::stoi(value);
    }

    static bool ParseBool(const std::string& value) {
        if (value == "true") {
            return true;
        }
        if (value == "false") {
            return false;
        }
        throw std::runtime_error("expected bool-like value, got `" + value + "`");
    }

    static void SkipWhitespace(const std::string& content, size_t* pos) {
        while (*pos < content.size() && std::isspace(static_cast<unsigned char>(content[*pos]))) {
            ++(*pos);
        }
    }

    static std::string Trim(std::string value) {
        size_t begin = 0;
        while (begin < value.size() && std::isspace(static_cast<unsigned char>(value[begin]))) {
            ++begin;
        }
        size_t end = value.size();
        while (end > begin && std::isspace(static_cast<unsigned char>(value[end - 1]))) {
            --end;
        }
        return value.substr(begin, end - begin);
    }

    std::string data_dir_;
};

int main() {
    const char* data_dir = std::getenv("DATA_DIR");
    if (data_dir == nullptr) {
        throw std::runtime_error("DATA_DIR should point to exported JSON data");
    }

    FixtureLoader loader(data_dir);
    cfg::GameData data = cfg::GameData::Load(loader);

    const cfg::ItemsRow* item = data.Items().GetById(2001);
    if (item == nullptr || item->name != "Iron Ore" || item->price != 5) {
        throw std::runtime_error("single-field item index query failed");
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
