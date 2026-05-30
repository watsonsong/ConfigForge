// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#include <cstdint>
#include <cstddef>
#include <memory>
#include <stdexcept>
#include <string>

class IRuntimeDataReader {
public:
    virtual ~IRuntimeDataReader() = default;
    virtual void EnterIndex(std::size_t index) = 0;
    virtual std::int64_t AsInt64() const = 0;
    virtual double AsDouble() const = 0;
    virtual bool AsBool() const = 0;
    virtual std::string AsString() const = 0;
    virtual void EnterRequiredField(const std::string& name) = 0;
    virtual bool EnterOptionalField(const std::string& name) = 0;
    virtual void ExitField() = 0;
    virtual std::size_t ReadRowCount() = 0;
    virtual std::size_t ReadListLength() { throw std::runtime_error("runtime reader does not support list length"); }
    virtual std::size_t ReadArrayLength() { return ReadListLength(); }
};

class IRuntimeDataLoader {
public:
    virtual ~IRuntimeDataLoader() = default;
    virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;
};

#define CONFIG_FORGE_RUNTIME_CPP_READER_HAS_READ_LIST_LENGTH 1
#define CONFIG_FORGE_RUNTIME_CPP_READER_HAS_READ_ARRAY_LENGTH 1
#define CONFIG_FORGE_RUNTIME_CPP_READER IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER IRuntimeDataLoader
#include "config_forge/runtime/rapidjson_reader/runtime_loader.hpp"

int main() {
    config_forge_runtime::JsonValueReader reader("{\"rows\":[{\"enabled\":\"1\"}]}");
    reader.EnterRequiredField("rows");
    if (reader.ReadRowCount() != 1) {
        return 2;
    }
    reader.EnterIndex(0);
    if (!reader.EnterOptionalField("enabled") || !reader.AsBool()) {
        return 1;
    }
    reader.ExitField();
    reader.ExitField();
    reader.ExitField();

    try {
        config_forge_runtime::JsonFsRuntimeDataLoader loader(".");
        (void)loader.Open("../secret.json");
        return 4;
    } catch (const std::runtime_error&) {
    }
    try {
        config_forge_runtime::RuntimeDecodeLimits limits;
        limits.max_rows = 1;
        config_forge_runtime::JsonValueReader limited("[{},{}]", limits);
        (void)limited.ReadRowCount();
        return 5;
    } catch (const std::runtime_error&) {
    }
    try {
        config_forge_runtime::RuntimeDecodeLimits limits;
        limits.max_array_length = 1;
        config_forge_runtime::JsonValueReader limited("[1,2]", limits);
        (void)limited.ReadListLength();
        return 6;
    } catch (const std::runtime_error&) {
    }
    try {
        config_forge_runtime::RuntimeDecodeLimits limits;
        limits.max_string_bytes = 3;
        config_forge_runtime::JsonValueReader limited("\"test\"", limits);
        (void)limited.AsString();
        return 7;
    } catch (const std::runtime_error&) {
    }
    try {
        config_forge_runtime::RuntimeDecodeLimits limits;
        limits.max_file_bytes = 1;
        config_forge_runtime::JsonValueReader limited("[]", limits);
        return 8;
    } catch (const std::runtime_error&) {
    }
    return 0;
}
