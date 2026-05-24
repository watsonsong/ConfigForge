// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <cctype>
#include <cmath>
#include <cstdint>
#include <fstream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#if !defined(CONFIG_FORGE_RUNTIME_CPP_READER) || !defined(CONFIG_FORGE_RUNTIME_CPP_LOADER)
#include "generated.hpp"

#define CONFIG_FORGE_RUNTIME_CPP_READER reference::game::config::IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER reference::game::config::IRuntimeDataLoader
#endif

namespace config_forge_runtime {

// This minimal builtin adapter is intended as a dependency-free test fallback.
// Production projects should prefer a full JSON implementation such as a future
// json_rapidjson adapter. Override these macros before including this header when
// generated code uses a different namespace or global-scope runtime interfaces.
inline constexpr const char* SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0";

#ifndef CONFIG_FORGE_RUNTIME_JSON_BUILTIN_TEST_FALLBACK
#define CONFIG_FORGE_RUNTIME_JSON_BUILTIN_TEST_FALLBACK 1
#endif

inline std::string Trim(std::string value) {
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

inline void SkipWhitespace(const std::string& content, std::size_t* pos) {
    while (*pos < content.size() && std::isspace(static_cast<unsigned char>(content[*pos]))) {
        ++(*pos);
    }
}

inline std::size_t EndOfJsonValue(const std::string& content, std::size_t begin) {
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

inline std::string UnquoteJsonString(const std::string& value) {
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

inline std::string SafeJoinPath(const std::string& root, const std::string& logical_path) {
    if (logical_path.empty()) {
        throw std::runtime_error("runtime data path must not be empty");
    }
    if (logical_path.front() == '/' || logical_path.front() == '\\' ||
        (logical_path.size() >= 2 && logical_path[1] == ':')) {
        throw std::runtime_error("runtime data path must be relative: " + logical_path);
    }

    std::vector<std::string> parts;
    std::size_t begin = 0;
    while (begin <= logical_path.size()) {
        const std::size_t end = logical_path.find_first_of("/\\", begin);
        const std::string part = logical_path.substr(begin, end == std::string::npos ? std::string::npos : end - begin);
        if (part.empty() || part == ".") {
            // Skip redundant separators and current-directory segments.
        } else if (part == "..") {
            if (parts.empty()) {
                throw std::runtime_error("runtime data path escapes data root: " + logical_path);
            }
            parts.pop_back();
        } else {
            parts.push_back(part);
        }
        if (end == std::string::npos) {
            break;
        }
        begin = end + 1;
    }

    std::string result = root;
    while (!result.empty() && (result.back() == '/' || result.back() == '\\')) {
        result.pop_back();
    }
    for (const std::string& part : parts) {
        result += "/";
        result += part;
    }
    return result;
}

class JsonValueReader final : public CONFIG_FORGE_RUNTIME_CPP_READER {
public:
    explicit JsonValueReader(std::string value) : value_(Trim(std::move(value))) {}

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Field(const std::string& name) const override {
        if (value_.empty() || value_.front() != '{') {
            return nullptr;
        }

        std::size_t pos = 1;
        while (pos < value_.size()) {
            SkipWhitespace(value_, &pos);
            if (pos < value_.size() && value_[pos] == '}') {
                return nullptr;
            }
            if (pos >= value_.size() || value_[pos] != '"') {
                throw std::runtime_error("expected object field key");
            }

            const std::size_t key_begin = pos;
            const std::size_t key_end = EndOfJsonValue(value_, key_begin);
            const std::string key = UnquoteJsonString(value_.substr(key_begin, key_end - key_begin));
            pos = key_end;
            SkipWhitespace(value_, &pos);
            if (pos >= value_.size() || value_[pos] != ':') {
                throw std::runtime_error("missing ':' after object field key");
            }
            ++pos;
            SkipWhitespace(value_, &pos);

            const std::size_t value_begin = pos;
            const std::size_t value_end = EndOfJsonValue(value_, value_begin);
            if (key == name) {
                return std::make_unique<JsonValueReader>(value_.substr(value_begin, value_end - value_begin));
            }

            pos = value_end;
            SkipWhitespace(value_, &pos);
            if (pos < value_.size() && value_[pos] == ',') {
                ++pos;
            }
        }
        throw std::runtime_error("unterminated JSON object");
    }

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Index(std::size_t index) const override {
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
                return std::make_unique<JsonValueReader>(value_.substr(value_begin, value_end - value_begin));
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
        if (value_.empty() || (value_.front() != '[' && value_.front() != '{')) {
            throw std::runtime_error("reader value is not an array/object");
        }

        std::size_t pos = 1;
        std::size_t count = 0;
        const char close = value_.front() == '[' ? ']' : '}';
        while (pos < value_.size()) {
            SkipWhitespace(value_, &pos);
            if (pos < value_.size() && value_[pos] == close) {
                return count;
            }
            if (value_.front() == '{') {
                if (pos >= value_.size() || value_[pos] != '"') {
                    throw std::runtime_error("expected object field key");
                }
                pos = EndOfJsonValue(value_, pos);
                SkipWhitespace(value_, &pos);
                if (pos >= value_.size() || value_[pos] != ':') {
                    throw std::runtime_error("missing ':' after object field key");
                }
                ++pos;
                SkipWhitespace(value_, &pos);
            }
            pos = EndOfJsonValue(value_, pos);
            SkipWhitespace(value_, &pos);
            if (pos < value_.size() && value_[pos] == ',') {
                ++pos;
            }
            ++count;
        }
        throw std::runtime_error(value_.front() == '[' ? "unterminated JSON array" : "unterminated JSON object");
    }

    std::int64_t AsInt64() const override {
        const std::string value = ScalarText();
        std::size_t consumed = 0;
        const std::int64_t parsed = std::stoll(value, &consumed);
        if (consumed != value.size()) {
            throw std::runtime_error("expected int64-like value, got `" + value + "`");
        }
        return parsed;
    }

    double AsDouble() const override {
        const std::string value = ScalarText();
        std::size_t consumed = 0;
        const double parsed = std::stod(value, &consumed);
        if (consumed != value.size() || !std::isfinite(parsed)) {
            throw std::runtime_error("expected finite double-like value, got `" + value + "`");
        }
        return parsed;
    }

    bool AsBool() const override {
        const std::string value = ScalarText();
        if (value == "true") {
            return true;
        }
        if (value == "false") {
            return false;
        }
        if (value == "1") {
            return true;
        }
        if (value == "0") {
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

class JsonFsRuntimeDataLoader final : public CONFIG_FORGE_RUNTIME_CPP_LOADER {
public:
    explicit JsonFsRuntimeDataLoader(std::string data_dir) : data_dir_(std::move(data_dir)) {}

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Open(const std::string& path) override {
        std::ifstream file(SafeJoinPath(data_dir_, path));
        if (!file) {
            throw std::runtime_error("failed to open runtime data: " + path);
        }
        std::ostringstream buffer;
        buffer << file.rdbuf();
        return std::make_unique<JsonValueReader>(buffer.str());
    }

private:
    std::string data_dir_;
};

} // namespace config_forge_runtime
