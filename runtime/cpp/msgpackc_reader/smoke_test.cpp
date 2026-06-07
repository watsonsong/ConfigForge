// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#include <cstddef>
#include <cstdint>
#include <memory>
#include <stdexcept>
#include <string>

#include <msgpack.hpp>

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
    reader.EnterRequiredField("rows");
    if (reader.ReadRowCount() != 1) {
        return 2;
    }
    reader.EnterIndex(0);
    if (!reader.EnterOptionalField("enabled") || !reader.AsBool()) {
        return 1;
    }
    reader.ExitField();
    if (!reader.EnterOptionalField("id") || reader.AsInt64() != 42) {
        return 4;
    }
    reader.ExitField();
    reader.ExitField();
    reader.ExitField();

    try {
        config_forge_runtime::MessagePackFsRuntimeDataLoader loader(".");
        (void)loader.Open("../secret.msgpack");
        return 3;
    } catch (const std::runtime_error&) {
    }

    msgpack::sbuffer limit_buffer;
    msgpack::packer<msgpack::sbuffer> limit_packer(limit_buffer);
    limit_packer.pack_array(2);
    limit_packer.pack_map(0);
    limit_packer.pack_map(0);

    try {
        config_forge_runtime::RuntimeDecodeLimits limits;
        limits.max_rows = 1;
        config_forge_runtime::MessagePackValueReader limited(limit_buffer.data(), limit_buffer.size(), limits);
        (void)limited.ReadRowCount();
        return 5;
    } catch (const std::runtime_error&) {
    }
    try {
        config_forge_runtime::RuntimeDecodeLimits limits;
        limits.max_array_length = 1;
        config_forge_runtime::MessagePackValueReader limited(limit_buffer.data(), limit_buffer.size(), limits);
        (void)limited.ReadListLength();
        return 6;
    } catch (const std::runtime_error&) {
    }

    msgpack::sbuffer string_buffer;
    msgpack::packer<msgpack::sbuffer> string_packer(string_buffer);
    string_packer.pack(std::string("test"));
    try {
        config_forge_runtime::RuntimeDecodeLimits limits;
        limits.max_string_bytes = 3;
        config_forge_runtime::MessagePackValueReader limited(string_buffer.data(), string_buffer.size(), limits);
        (void)limited.AsString();
        return 7;
    } catch (const std::runtime_error&) {
    }

    try {
        config_forge_runtime::RuntimeDecodeLimits limits;
        limits.max_file_bytes = 1;
        config_forge_runtime::MessagePackValueReader limited(limit_buffer.data(), limit_buffer.size(), limits);
        return 8;
    } catch (const std::runtime_error&) {
    }
    return 0;
}
