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

class JsonValueReader final : public CONFIG_FORGE_RUNTIME_CPP_READER {
public:
    explicit JsonValueReader(std::string json) : document_(std::make_shared<rapidjson::Document>()) {
        document_->Parse(json.c_str(), json.size());
        if (document_->HasParseError()) {
            throw std::runtime_error("failed to parse JSON runtime data");
        }
        value_ = document_.get();
    }

    explicit JsonValueReader(std::shared_ptr<rapidjson::Document> document)
        : document_(std::move(document)), value_(document_.get()) {}

    JsonValueReader(std::shared_ptr<rapidjson::Document> document, const rapidjson::Value* value)
        : document_(std::move(document)), value_(value) {
        if (value_ == nullptr) {
            throw std::runtime_error("JSON reader cannot wrap a null value pointer");
        }
    }

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Field(const std::string& name) const override {
        if (!value_->IsObject()) {
            return nullptr;
        }
        const auto member = value_->FindMember(name.c_str());
        if (member == value_->MemberEnd()) {
            return nullptr;
        }
        return std::make_unique<JsonValueReader>(document_, &member->value);
    }

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Index(std::size_t index) const override {
        if (!value_->IsArray() || index >= value_->Size()) {
            throw std::runtime_error("missing array index " + std::to_string(index));
        }
        return std::make_unique<JsonValueReader>(document_, &(*value_)[static_cast<rapidjson::SizeType>(index)]);
    }

    std::size_t Size() const override {
        if (value_->IsArray()) {
            return value_->Size();
        }
        if (value_->IsObject()) {
            return value_->MemberCount();
        }
        throw std::runtime_error("reader value is not an array/object");
    }

    std::int64_t AsInt64() const override {
        if (value_->IsInt64()) {
            return value_->GetInt64();
        }
        if (value_->IsUint64()) {
            const auto parsed = value_->GetUint64();
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
        if (value_->IsNumber()) {
            return value_->GetDouble();
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
        if (value_->IsBool()) {
            return value_->GetBool();
        }
        if (value_->IsInt()) {
            if (value_->GetInt() == 1) {
                return true;
            }
            if (value_->GetInt() == 0) {
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
        if (!value_->IsString()) {
            throw std::runtime_error("expected string value");
        }
        return std::string(value_->GetString(), value_->GetStringLength());
    }

    bool IsNull() const override { return value_->IsNull(); }

private:
    std::string ScalarText() const {
        if (value_->IsString()) {
            return std::string(value_->GetString(), value_->GetStringLength());
        }
        if (value_->IsBool()) {
            return value_->GetBool() ? "true" : "false";
        }
        if (value_->IsInt64()) {
            return std::to_string(value_->GetInt64());
        }
        if (value_->IsUint64()) {
            return std::to_string(value_->GetUint64());
        }
        if (value_->IsDouble()) {
            return std::to_string(value_->GetDouble());
        }
        if (value_->IsNull()) {
            return "null";
        }
        throw std::runtime_error("expected scalar JSON value");
    }

    std::shared_ptr<rapidjson::Document> document_;
    const rapidjson::Value* value_ = nullptr;
};

class JsonFsRuntimeDataLoader final : public CONFIG_FORGE_RUNTIME_CPP_LOADER {
public:
    explicit JsonFsRuntimeDataLoader(std::string data_dir) : data_dir_(std::move(data_dir)) {}

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Open(const std::string& path) override {
        std::ifstream file(SafeJoinPath(data_dir_, path));
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

} // namespace config_forge_runtime
