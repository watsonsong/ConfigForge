// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>

#include <msgpack.hpp>

class IRuntimeDataReader {
public:
    virtual ~IRuntimeDataReader() = default;
    virtual bool EnterField(const std::string& name) = 0;
    virtual void EnterIndex(std::size_t index) = 0;
    virtual void Exit() = 0;
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
#include "config_forge/runtime/msgpackc_reader/runtime_loader.hpp"

int main() {
    msgpack::sbuffer buffer;
    msgpack::packer<msgpack::sbuffer> packer(buffer);
    packer.pack_map(1);
    packer.pack(std::string("rows"));
    packer.pack_array(1);
    packer.pack_map(2);
    packer.pack(std::string("enabled"));
    packer.pack(std::string("1"));
    packer.pack(std::string("id"));
    packer.pack(42);

    config_forge_runtime::MessagePackValueReader reader(buffer.data(), buffer.size());
    if (reader.Size() != 1) {
        return 2;
    }
    if (!reader.EnterField("rows")) {
        return 5;
    }
    reader.EnterIndex(0);
    if (!reader.EnterField("enabled") || !reader.AsBool()) {
        return 1;
    }
    reader.Exit();
    if (!reader.EnterField("id") || reader.AsInt64() != 42) {
        return 4;
    }
    reader.Exit();
    reader.Exit();
    reader.Exit();

    try {
        config_forge_runtime::MessagePackFsRuntimeDataLoader loader(".");
        (void)loader.Open("../secret.msgpack");
        return 3;
    } catch (const std::runtime_error&) {
    }
    return 0;
}
