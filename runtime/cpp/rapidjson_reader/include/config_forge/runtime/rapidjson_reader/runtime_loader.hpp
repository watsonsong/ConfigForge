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
#include "config_forge/runtime/reader.hpp"

#define CONFIG_FORGE_RUNTIME_CPP_READER config_forge_runtime::IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER config_forge_runtime::IRuntimeDataLoader
#endif

namespace config_forge_runtime {

#ifndef CONFIG_FORGE_RUNTIME_CPP_ERROR_DEFINED
#define CONFIG_FORGE_RUNTIME_CPP_ERROR_DEFINED
enum class RuntimeDataErrorKind {
    Io,
    Decode,
    SchemaMismatch,
    TypeMismatch,
    NotFound,
    Unsupported,
    InvalidPath,
};

class RuntimeDataError : public std::runtime_error {
public:
    RuntimeDataError(RuntimeDataErrorKind kind, const std::string& message)
        : std::runtime_error(message), kind_(kind) {}
    RuntimeDataErrorKind Kind() const noexcept { return kind_; }
private:
    RuntimeDataErrorKind kind_;
};
#endif

// RapidJSON-backed adapter for generated ConfigForge C++ runtime interfaces.
// Override these macros before including this header when generated code uses a
// different namespace or global-scope runtime interfaces.
#ifndef CONFIG_FORGE_RUNTIME_CPP_SUPPORTED_READER_CONTRACT_DEFINED
#define CONFIG_FORGE_RUNTIME_CPP_SUPPORTED_READER_CONTRACT_DEFINED
inline constexpr const char* SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0";
#endif

#ifndef CONFIG_FORGE_RUNTIME_CPP_DECODE_LIMITS_DEFINED
#define CONFIG_FORGE_RUNTIME_CPP_DECODE_LIMITS_DEFINED
struct RuntimeDecodeLimits {
    std::size_t max_file_bytes = 64u * 1024u * 1024u;
    std::size_t max_rows = 1000000u;
    std::size_t max_array_length = 1000000u;
    std::size_t max_string_bytes = 16u * 1024u * 1024u;
    std::size_t max_nesting_depth = 128u;
};

inline void CheckRuntimeDecodeLimit(const char* label, std::size_t actual, std::size_t limit) {
    if (actual > limit) {
        throw RuntimeDataError(RuntimeDataErrorKind::Decode, std::string(label) + " " + std::to_string(actual) +
                               " exceeds decode limit " + std::to_string(limit));
    }
}
#endif

#ifndef CONFIG_FORGE_RUNTIME_CPP_SAFE_JOIN_PATH_DEFINED
#define CONFIG_FORGE_RUNTIME_CPP_SAFE_JOIN_PATH_DEFINED
inline std::string SafeJoinPath(const std::string& root, const std::string& logical_path) {
    if (logical_path.empty()) {
        throw RuntimeDataError(RuntimeDataErrorKind::InvalidPath, "runtime data path must not be empty");
    }
    if (logical_path.front() == '/' || logical_path.front() == '\\' ||
        (logical_path.size() >= 2 && logical_path[1] == ':')) {
        throw RuntimeDataError(RuntimeDataErrorKind::InvalidPath, "runtime data path must be relative: " + logical_path);
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
                throw RuntimeDataError(RuntimeDataErrorKind::InvalidPath, "runtime data path escapes data root: " + logical_path);
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
    explicit JsonValueReader(
        std::string json,
        RuntimeDecodeLimits limits = RuntimeDecodeLimits())
        : document_(std::make_shared<rapidjson::Document>()), limits_(limits) {
        CheckRuntimeDecodeLimit("runtime file bytes", json.size(), limits_.max_file_bytes);
        document_->Parse(json.c_str(), json.size());
        if (document_->HasParseError()) {
            throw RuntimeDataError(RuntimeDataErrorKind::Decode, "failed to parse JSON runtime data");
        }
        stack_.reserve(8);
        stack_.push_back(document_.get());
    }

    explicit JsonValueReader(
        std::shared_ptr<rapidjson::Document> document,
        RuntimeDecodeLimits limits = RuntimeDecodeLimits())
        : document_(std::move(document)), limits_(limits) {
        stack_.reserve(8);
        stack_.push_back(document_.get());
    }

    void EnterIndex(std::size_t index) override {
        const rapidjson::Value& value = Current();
        if (!value.IsArray() || index >= value.Size()) {
            throw RuntimeDataError(RuntimeDataErrorKind::NotFound, "missing array index " + std::to_string(index));
        }
        CheckRuntimeDecodeLimit("nesting depth", stack_.size(), limits_.max_nesting_depth);
        stack_.push_back(&value[static_cast<rapidjson::SizeType>(index)]);
    }

    void ExitField() override {
        if (stack_.size() <= 1) {
            throw RuntimeDataError(RuntimeDataErrorKind::Decode, "JSON reader cursor stack underflow");
        }
        stack_.pop_back();
    }

    std::size_t ReadRowCount() override {
        const rapidjson::Value& value = Current();
        if (value.IsArray()) {
            const std::size_t rows = value.Size();
            CheckRuntimeDecodeLimit("row count", rows, limits_.max_rows);
            return rows;
        }
        throw RuntimeDataError(RuntimeDataErrorKind::TypeMismatch, "reader value is not an array");
    }

    std::size_t ReadListLength() override {
        const rapidjson::Value& value = Current();
        if (value.IsArray()) {
            const std::size_t length = value.Size();
            CheckRuntimeDecodeLimit("list length", length, limits_.max_array_length);
            return length;
        }
        throw RuntimeDataError(RuntimeDataErrorKind::TypeMismatch, "reader value is not an array");
    }

    std::size_t ReadArrayLength()
#if defined(CONFIG_FORGE_RUNTIME_CPP_READER_HAS_READ_ARRAY_LENGTH)
        override
#endif
    {
        return ReadListLength();
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
        const std::string text = ScalarText();
        std::size_t consumed = 0;
        const std::int64_t parsed = std::stoll(text, &consumed);
        if (consumed != text.size()) {
            throw RuntimeDataError(RuntimeDataErrorKind::Decode, "expected int64-like value, got `" + text + "`");
        }
        return parsed;
    }

    double AsDouble() const override {
        const rapidjson::Value& value = Current();
        if (value.IsNumber()) {
            return value.GetDouble();
        }
        const std::string text = ScalarText();
        std::size_t consumed = 0;
        const double parsed = std::stod(text, &consumed);
        if (consumed != text.size() || !std::isfinite(parsed)) {
            throw RuntimeDataError(RuntimeDataErrorKind::TypeMismatch, "expected finite double-like value, got `" + text + "`");
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
        const std::string text = ScalarText();
        if (text == "true" || text == "1") {
            return true;
        }
        if (text == "false" || text == "0") {
            return false;
        }
        throw RuntimeDataError(RuntimeDataErrorKind::Decode, "expected bool-like value, got `" + text + "`");
    }

    std::string AsString() const override {
        const rapidjson::Value& value = Current();
        if (!value.IsString()) {
            throw RuntimeDataError(RuntimeDataErrorKind::TypeMismatch, "expected string value");
        }
        CheckRuntimeDecodeLimit("string byte length", value.GetStringLength(), limits_.max_string_bytes);
        return std::string(value.GetString(), value.GetStringLength());
    }

    void EnterRequiredField(const std::string& name) override {
        if (!EnterNamedField(name)) {
            throw RuntimeDataError(RuntimeDataErrorKind::NotFound, "missing required field `" + name + "`");
        }
        if (Current().IsNull()) {
            ExitField();
            throw RuntimeDataError(RuntimeDataErrorKind::NotFound, "missing required field `" + name + "`");
        }
    }

    bool EnterOptionalField(const std::string& name) override {
        if (!EnterNamedField(name)) {
            return false;
        }
        if (Current().IsNull()) {
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
        CheckRuntimeDecodeLimit("nesting depth", stack_.size(), limits_.max_nesting_depth);
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
        throw RuntimeDataError(RuntimeDataErrorKind::TypeMismatch, "expected scalar JSON value");
    }

    const rapidjson::Value& Current() const { return *stack_.back(); }

    std::shared_ptr<rapidjson::Document> document_;
    RuntimeDecodeLimits limits_;
    std::vector<const rapidjson::Value*> stack_;
};

class JsonFsRuntimeDataLoader final : public CONFIG_FORGE_RUNTIME_CPP_LOADER {
public:
    explicit JsonFsRuntimeDataLoader(
        std::string data_dir,
        RuntimeDecodeLimits limits = RuntimeDecodeLimits())
        : data_dir_(std::move(data_dir)), limits_(limits) {}

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Open(const std::string& path) override {
        std::ifstream file(SafeJoinPath(data_dir_, WithExtension(path, ".json")), std::ios::binary | std::ios::ate);
        if (!file) {
            throw RuntimeDataError(RuntimeDataErrorKind::Io, "failed to open runtime data: " + path);
        }
        const std::streamoff size = file.tellg();
        if (size < 0) {
            throw RuntimeDataError(RuntimeDataErrorKind::Io, "failed to read runtime data size: " + path);
        }
        CheckRuntimeDecodeLimit("runtime file bytes", static_cast<std::size_t>(size), limits_.max_file_bytes);
        file.seekg(0, std::ios::beg);
        rapidjson::IStreamWrapper stream(file);
        auto document = std::make_shared<rapidjson::Document>();
        document->ParseStream(stream);
        if (document->HasParseError()) {
            throw RuntimeDataError(RuntimeDataErrorKind::Decode, "failed to parse runtime data: " + path);
        }
        return std::make_unique<JsonValueReader>(std::move(document), limits_);
    }

private:
    std::string data_dir_;
    RuntimeDecodeLimits limits_;
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
