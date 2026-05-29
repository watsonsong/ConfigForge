// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <cmath>
#include <cstdint>
#include <fstream>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <rapidjson/document.h>
#include <rapidjson/istreamwrapper.h>

#if !defined(CONFIG_FORGE_RUNTIME_CPP_READER) || !defined(CONFIG_FORGE_RUNTIME_CPP_LOADER)
#include "generated.hpp"

#define CONFIG_FORGE_RUNTIME_CPP_READER reference::game::config::IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER reference::game::config::IRuntimeDataLoader
#endif

namespace config_forge_runtime {

// RapidJSON-backed adapter for generated ConfigForge C++ runtime interfaces.
// Override these macros before including this header when generated code uses a
// different namespace or global-scope runtime interfaces.
#ifndef CONFIG_FORGE_RUNTIME_CPP_SUPPORTED_READER_CONTRACT_DEFINED
#define CONFIG_FORGE_RUNTIME_CPP_SUPPORTED_READER_CONTRACT_DEFINED
inline constexpr const char* SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0";
#endif

#ifndef CONFIG_FORGE_RUNTIME_CPP_SAFE_JOIN_PATH_DEFINED
#define CONFIG_FORGE_RUNTIME_CPP_SAFE_JOIN_PATH_DEFINED
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
#endif

inline std::string WithExtension(const std::string& logical_path, const std::string& extension);

class JsonValueReader final : public CONFIG_FORGE_RUNTIME_CPP_READER {
public:
    explicit JsonValueReader(std::string json) : document_(std::make_shared<rapidjson::Document>()) {
        document_->Parse(json.c_str(), json.size());
        if (document_->HasParseError()) {
            throw std::runtime_error("failed to parse JSON runtime data");
        }
        stack_.reserve(8);
        stack_.push_back(document_.get());
    }

    explicit JsonValueReader(std::shared_ptr<rapidjson::Document> document)
        : document_(std::move(document)) {
        stack_.reserve(8);
        stack_.push_back(document_.get());
    }

    void EnterIndex(std::size_t index) override {
        const rapidjson::Value& value = Current();
        if (!value.IsArray() || index >= value.Size()) {
            throw std::runtime_error("missing array index " + std::to_string(index));
        }
        stack_.push_back(&value[static_cast<rapidjson::SizeType>(index)]);
    }

    void ExitField() override {
        if (stack_.size() <= 1) {
            throw std::runtime_error("JSON reader cursor stack underflow");
        }
        stack_.pop_back();
    }

    std::size_t ReadRowCount() override {
        const rapidjson::Value& value = Current();
        if (value.IsArray()) {
            return value.Size();
        }
        throw std::runtime_error("reader value is not an array");
    }

    std::int64_t AsInt64() const override {
        const rapidjson::Value& value = Current();
        if (value.IsInt64()) {
            return value.GetInt64();
        }
        if (value.IsUint64()) {
            const auto parsed = value.GetUint64();
            if (parsed <= static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max())) {
                return static_cast<std::int64_t>(parsed);
            }
        }
        const std::string value = ScalarText();
        std::size_t consumed = 0;
        const std::int64_t parsed = std::stoll(value, &consumed);
        if (consumed != value.size()) {
            throw std::runtime_error("expected int64-like value, got `" + value + "`");
        }
        return parsed;
    }

    double AsDouble() const override {
        const rapidjson::Value& value = Current();
        if (value.IsNumber()) {
            return value.GetDouble();
        }
        const std::string value = ScalarText();
        std::size_t consumed = 0;
        const double parsed = std::stod(value, &consumed);
        if (consumed != value.size() || !std::isfinite(parsed)) {
            throw std::runtime_error("expected finite double-like value, got `" + value + "`");
        }
        return parsed;
    }

    bool AsBool() const override {
        const rapidjson::Value& value = Current();
        if (value.IsBool()) {
            return value.GetBool();
        }
        if (value.IsInt()) {
            if (value.GetInt() == 1) {
                return true;
            }
            if (value.GetInt() == 0) {
                return false;
            }
        }
        const std::string value = ScalarText();
        if (value == "true" || value == "1") {
            return true;
        }
        if (value == "false" || value == "0") {
            return false;
        }
        throw std::runtime_error("expected bool-like value, got `" + value + "`");
    }

    std::string AsString() const override {
        const rapidjson::Value& value = Current();
        if (!value.IsString()) {
            throw std::runtime_error("expected string value");
        }
        return std::string(value.GetString(), value.GetStringLength());
    }

    bool IsNull() const override { return Current().IsNull(); }

    void EnterRequiredField(const std::string& name) override {
        if (!EnterNamedField(name)) {
            throw std::runtime_error("missing required field `" + name + "`");
        }
        if (IsNull()) {
            ExitField();
            throw std::runtime_error("missing required field `" + name + "`");
        }
    }

    bool EnterOptionalField(const std::string& name) override {
        if (!EnterNamedField(name)) {
            return false;
        }
        if (IsNull()) {
            ExitField();
            return false;
        }
        return true;
    }

private:
    bool EnterNamedField(const std::string& name) {
        const rapidjson::Value& value = Current();
        if (!value.IsObject()) {
            return false;
        }
        const auto member = value.FindMember(name.c_str());
        if (member == value.MemberEnd()) {
            return false;
        }
        stack_.push_back(&member->value);
        return true;
    }

    std::string ScalarText() const {
        const rapidjson::Value& value = Current();
        if (value.IsString()) {
            return std::string(value.GetString(), value.GetStringLength());
        }
        if (value.IsBool()) {
            return value.GetBool() ? "true" : "false";
        }
        if (value.IsInt64()) {
            return std::to_string(value.GetInt64());
        }
        if (value.IsUint64()) {
            return std::to_string(value.GetUint64());
        }
        if (value.IsDouble()) {
            return std::to_string(value.GetDouble());
        }
        if (value.IsNull()) {
            return "null";
        }
        throw std::runtime_error("expected scalar JSON value");
    }

    const rapidjson::Value& Current() const { return *stack_.back(); }

    std::shared_ptr<rapidjson::Document> document_;
    std::vector<const rapidjson::Value*> stack_;
};

class JsonFsRuntimeDataLoader final : public CONFIG_FORGE_RUNTIME_CPP_LOADER {
public:
    explicit JsonFsRuntimeDataLoader(std::string data_dir) : data_dir_(std::move(data_dir)) {}

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Open(const std::string& path) override {
        std::ifstream file(SafeJoinPath(data_dir_, WithExtension(path, ".json")));
        if (!file) {
            throw std::runtime_error("failed to open runtime data: " + path);
        }
        rapidjson::IStreamWrapper stream(file);
        auto document = std::make_shared<rapidjson::Document>();
        document->ParseStream(stream);
        if (document->HasParseError()) {
            throw std::runtime_error("failed to parse runtime data: " + path);
        }
        return std::make_unique<JsonValueReader>(std::move(document));
    }

private:
    std::string data_dir_;
};

inline std::string WithExtension(const std::string& logical_path, const std::string& extension) {
    const std::size_t slash = logical_path.find_last_of("/\\");
    const std::size_t dot = logical_path.find_last_of('.');
    if (dot != std::string::npos && (slash == std::string::npos || dot > slash)) {
        return logical_path;
    }
    return logical_path + extension;
}

} // namespace config_forge_runtime
