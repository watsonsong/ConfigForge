#include "generated.hpp"

#include <cctype>
#include <cstdlib>
#include <fstream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>

namespace cfg = reference::game::config;

namespace {

std::string Trim(std::string value) {
    std::size_t begin = 0;
    while (begin < value.size() && std::isspace(static_cast<unsigned char>(value[begin]))) {
        ++begin;
    }
    std::size_t end = value.size();
    while (end > begin && std::isspace(static_cast<unsigned char>(value[end - 1]))) {
        --end;
    }
    return value.substr(begin, end - begin);
}

void SkipWhitespace(const std::string& content, std::size_t* pos) {
    while (*pos < content.size() && std::isspace(static_cast<unsigned char>(content[*pos]))) {
        ++(*pos);
    }
}

std::size_t EndOfJsonValue(const std::string& content, std::size_t begin) {
    SkipWhitespace(content, &begin);
    if (begin >= content.size()) {
        throw std::runtime_error("missing JSON value");
    }

    bool in_string = false;
    bool escaped = false;
    int object_depth = 0;
    int array_depth = 0;
    for (std::size_t pos = begin; pos < content.size(); ++pos) {
        const char ch = content[pos];
        if (in_string) {
            if (escaped) {
                escaped = false;
            } else if (ch == '\\') {
                escaped = true;
            } else if (ch == '"') {
                in_string = false;
                if (object_depth == 0 && array_depth == 0) {
                    return pos + 1;
                }
            }
            continue;
        }

        if (ch == '"') {
            in_string = true;
        } else if (ch == '{') {
            ++object_depth;
        } else if (ch == '}') {
            if (object_depth == 0) {
                return pos;
            }
            --object_depth;
            if (object_depth == 0 && array_depth == 0) {
                return pos + 1;
            }
        } else if (ch == '[') {
            ++array_depth;
        } else if (ch == ']') {
            if (array_depth == 0) {
                return pos;
            }
            --array_depth;
            if (object_depth == 0 && array_depth == 0) {
                return pos + 1;
            }
        } else if ((ch == ',' || std::isspace(static_cast<unsigned char>(ch))) && object_depth == 0 &&
                   array_depth == 0) {
            return pos;
        }
    }
    return content.size();
}

std::string UnquoteJsonString(const std::string& value) {
    const std::string trimmed = Trim(value);
    if (trimmed.size() < 2 || trimmed.front() != '"' || trimmed.back() != '"') {
        throw std::runtime_error("expected JSON string, got `" + trimmed + "`");
    }

    std::string result;
    bool escaped = false;
    for (std::size_t pos = 1; pos + 1 < trimmed.size(); ++pos) {
        const char ch = trimmed[pos];
        if (escaped) {
            result.push_back(ch);
            escaped = false;
        } else if (ch == '\\') {
            escaped = true;
        } else {
            result.push_back(ch);
        }
    }
    return result;
}

class ValueReader final : public cfg::IRuntimeDataReader {
public:
    explicit ValueReader(std::string value) : value_(Trim(std::move(value))) {}

    std::unique_ptr<cfg::IRuntimeDataReader> Field(const std::string& name) const override {
        if (value_.empty() || value_.front() != '{') {
            return nullptr;
        }

        const std::string quoted_key = "\"" + name + "\"";
        const std::size_t key_pos = value_.find(quoted_key);
        if (key_pos == std::string::npos) {
            return nullptr;
        }
        std::size_t value_pos = value_.find(':', key_pos + quoted_key.size());
        if (value_pos == std::string::npos) {
            return nullptr;
        }
        ++value_pos;
        SkipWhitespace(value_, &value_pos);
        const std::size_t value_end = EndOfJsonValue(value_, value_pos);
        return std::make_unique<ValueReader>(value_.substr(value_pos, value_end - value_pos));
    }

    std::unique_ptr<cfg::IRuntimeDataReader> Index(std::size_t index) const override {
        if (value_.empty() || value_.front() != '[') {
            throw std::runtime_error("reader value is not an array");
        }

        std::size_t pos = 1;
        std::size_t current = 0;
        while (pos < value_.size()) {
            SkipWhitespace(value_, &pos);
            if (pos < value_.size() && value_[pos] == ']') {
                break;
            }
            const std::size_t value_begin = pos;
            const std::size_t value_end = EndOfJsonValue(value_, value_begin);
            if (current == index) {
                return std::make_unique<ValueReader>(value_.substr(value_begin, value_end - value_begin));
            }
            pos = value_end;
            SkipWhitespace(value_, &pos);
            if (pos < value_.size() && value_[pos] == ',') {
                ++pos;
            }
            ++current;
        }
        throw std::runtime_error("missing array index " + std::to_string(index));
    }

    std::size_t Size() const override {
        if (value_.empty() || value_.front() != '[') {
            throw std::runtime_error("reader value is not an array");
        }

        std::size_t pos = 1;
        std::size_t count = 0;
        while (pos < value_.size()) {
            SkipWhitespace(value_, &pos);
            if (pos < value_.size() && value_[pos] == ']') {
                return count;
            }
            pos = EndOfJsonValue(value_, pos);
            SkipWhitespace(value_, &pos);
            if (pos < value_.size() && value_[pos] == ',') {
                ++pos;
            }
            ++count;
        }
        throw std::runtime_error("unterminated JSON array");
    }

    std::int64_t AsInt64() const override { return std::stoll(ScalarText()); }

    double AsDouble() const override { return std::stod(ScalarText()); }

    bool AsBool() const override {
        const std::string value = ScalarText();
        if (value == "true") {
            return true;
        }
        if (value == "false") {
            return false;
        }
        throw std::runtime_error("expected bool-like value, got `" + value + "`");
    }

    std::string AsString() const override { return UnquoteJsonString(value_); }

    bool IsNull() const override { return value_ == "null"; }

private:
    std::string ScalarText() const {
        const std::string trimmed = Trim(value_);
        if (!trimmed.empty() && trimmed.front() == '"') {
            return UnquoteJsonString(trimmed);
        }
        return trimmed;
    }

    std::string value_;
};

class FixtureLoader final : public cfg::IRuntimeDataLoader {
public:
    explicit FixtureLoader(std::string data_dir) : data_dir_(std::move(data_dir)) {}

    std::unique_ptr<cfg::IRuntimeDataReader> Open(const std::string& path) override {
        std::ifstream file(data_dir_ + "/" + path);
        if (!file) {
            throw std::runtime_error("failed to open runtime data: " + path);
        }
        std::ostringstream buffer;
        buffer << file.rdbuf();
        return std::make_unique<ValueReader>(buffer.str());
    }

private:
    std::string data_dir_;
};

} // namespace

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
