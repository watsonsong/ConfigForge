// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <iterator>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <msgpack.hpp>

#if !defined(CONFIG_FORGE_RUNTIME_CPP_READER) || !defined(CONFIG_FORGE_RUNTIME_CPP_LOADER)
#include "generated.hpp"

#define CONFIG_FORGE_RUNTIME_CPP_READER reference::game::config::IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER reference::game::config::IRuntimeDataLoader
#endif

namespace config_forge_runtime {

// msgpack-cxx-backed adapter for generated ConfigForge C++ runtime interfaces.
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

class MessagePackValueReader final : public CONFIG_FORGE_RUNTIME_CPP_READER {
public:
    explicit MessagePackValueReader(const std::string& bytes)
        : handle_(std::make_shared<msgpack::object_handle>(msgpack::unpack(bytes.data(), bytes.size()))) {
        stack_.reserve(8);
        stack_.push_back(&handle_->get());
    }

    MessagePackValueReader(const char* bytes, std::size_t size)
        : handle_(std::make_shared<msgpack::object_handle>(msgpack::unpack(bytes, size))) {
        stack_.reserve(8);
        stack_.push_back(&handle_->get());
    }

    explicit MessagePackValueReader(std::shared_ptr<msgpack::object_handle> handle)
        : handle_(std::move(handle)) {
        stack_.reserve(8);
        stack_.push_back(&handle_->get());
    }

    void EnterIndex(std::size_t index) override {
        const msgpack::object& value = Current();
        if (value.type != msgpack::type::ARRAY || index >= value.via.array.size) {
            throw std::runtime_error("missing array index " + std::to_string(index));
        }
        stack_.push_back(&value.via.array.ptr[index]);
    }

    void ExitField() override {
        if (stack_.size() <= 1) {
            throw std::runtime_error("MessagePack reader cursor stack underflow");
        }
        stack_.pop_back();
    }

    std::size_t ReadRowCount() override {
        const msgpack::object& value = Current();
        if (value.type == msgpack::type::ARRAY) {
            return value.via.array.size;
        }
        throw std::runtime_error("reader value is not an array");
    }

    std::int64_t AsInt64() const override {
        const msgpack::object& value = Current();
        if (value.type == msgpack::type::NEGATIVE_INTEGER) {
            return value.via.i64;
        }
        if (value.type == msgpack::type::POSITIVE_INTEGER) {
            if (value.via.u64 <= static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max())) {
                return static_cast<std::int64_t>(value.via.u64);
            }
            throw std::runtime_error("MessagePack positive integer exceeds int64 range");
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
        const msgpack::object& value = Current();
        if (value.type == msgpack::type::FLOAT32 || value.type == msgpack::type::FLOAT64) {
            if (!std::isfinite(value.via.f64)) {
                throw std::runtime_error("expected finite double-like value");
            }
            return value.via.f64;
        }
        if (value.type == msgpack::type::NEGATIVE_INTEGER) {
            return static_cast<double>(value.via.i64);
        }
        if (value.type == msgpack::type::POSITIVE_INTEGER) {
            return static_cast<double>(value.via.u64);
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
        const msgpack::object& value = Current();
        if (value.type == msgpack::type::BOOLEAN) {
            return value.via.boolean;
        }
        if (value.type == msgpack::type::POSITIVE_INTEGER) {
            if (value.via.u64 == 1) {
                return true;
            }
            if (value.via.u64 == 0) {
                return false;
            }
        }
        if (value.type == msgpack::type::NEGATIVE_INTEGER && value.via.i64 == 0) {
            return false;
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
        const msgpack::object& value = Current();
        if (value.type != msgpack::type::STR) {
            throw std::runtime_error("expected string value");
        }
        return std::string(value.via.str.ptr, value.via.str.size);
    }

    void EnterRequiredField(const std::string& name) override {
        if (!EnterNamedField(name)) {
            throw std::runtime_error("missing required field `" + name + "`");
        }
        if (Current().type == msgpack::type::NIL) {
            ExitField();
            throw std::runtime_error("missing required field `" + name + "`");
        }
    }

    bool EnterOptionalField(const std::string& name) override {
        if (!EnterNamedField(name)) {
            return false;
        }
        if (Current().type == msgpack::type::NIL) {
            ExitField();
            return false;
        }
        return true;
    }

private:
    bool EnterNamedField(const std::string& name) {
        const msgpack::object& value = Current();
        if (value.type != msgpack::type::MAP) {
            return false;
        }
        const msgpack::object_map& map = value.via.map;
        for (std::uint32_t index = 0; index < map.size; ++index) {
            const msgpack::object& key = map.ptr[index].key;
            if (key.type != msgpack::type::STR) {
                throw std::runtime_error("MessagePack map key is not a string");
            }
            if (key.via.str.size == name.size() &&
                std::memcmp(key.via.str.ptr, name.data(), name.size()) == 0) {
                stack_.push_back(&map.ptr[index].val);
                return true;
            }
        }
        return false;
    }

    std::string ScalarText() const {
        const msgpack::object& value = Current();
        switch (value.type) {
            case msgpack::type::STR:
                return std::string(value.via.str.ptr, value.via.str.size);
            case msgpack::type::BOOLEAN:
                return value.via.boolean ? "true" : "false";
            case msgpack::type::NEGATIVE_INTEGER:
                return std::to_string(value.via.i64);
            case msgpack::type::POSITIVE_INTEGER:
                return std::to_string(value.via.u64);
            case msgpack::type::FLOAT32:
            case msgpack::type::FLOAT64:
                return std::to_string(value.via.f64);
            case msgpack::type::NIL:
                return "null";
            default:
                throw std::runtime_error("expected scalar MessagePack value");
        }
    }

    const msgpack::object& Current() const { return *stack_.back(); }

    std::shared_ptr<msgpack::object_handle> handle_;
    std::vector<const msgpack::object*> stack_;
};

class MessagePackFsRuntimeDataLoader final : public CONFIG_FORGE_RUNTIME_CPP_LOADER {
public:
    explicit MessagePackFsRuntimeDataLoader(std::string data_dir) : data_dir_(std::move(data_dir)) {}

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Open(const std::string& path) override {
        std::ifstream file(SafeJoinPath(data_dir_, WithExtension(path, ".msgpack")), std::ios::binary);
        if (!file) {
            throw std::runtime_error("failed to open runtime data: " + path);
        }
        const std::string bytes((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
        return std::make_unique<MessagePackValueReader>(bytes);
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
