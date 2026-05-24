// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#include <cstdint>
#include <cstddef>
#include <memory>
#include <string>

class IRuntimeDataReader {
public:
    virtual ~IRuntimeDataReader() = default;
    virtual std::unique_ptr<IRuntimeDataReader> Field(const std::string& name) const = 0;
    virtual std::unique_ptr<IRuntimeDataReader> Index(std::size_t index) const = 0;
    virtual std::size_t Size() const = 0;
    virtual std::int64_t AsInt64() const = 0;
    virtual double AsDouble() const = 0;
    virtual bool AsBool() const = 0;
    virtual std::string AsString() const = 0;
    virtual bool IsNull() const = 0;
};

class IRuntimeDataLoader {
public:
    virtual ~IRuntimeDataLoader() = default;
    virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;
};

#define CONFIG_FORGE_RUNTIME_CPP_READER IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER IRuntimeDataLoader
#include "config_forge/runtime/rapidjson_reader/runtime_loader.hpp"

int main() {
    config_forge_runtime::JsonValueReader reader("{\"rows\":[{\"enabled\":\"1\"}]}");
    if (reader.Size() != 1) {
        return 2;
    }
    auto rows = reader.Field("rows");
    auto row = rows->Index(0);
    auto enabled = row->Field("enabled");
    if (!enabled->AsBool()) {
        return 1;
    }

    try {
        config_forge_runtime::JsonFsRuntimeDataLoader loader(".");
        (void)loader.Open("../secret.json");
        return 3;
    } catch (const std::runtime_error&) {
    }
    return 0;
}
