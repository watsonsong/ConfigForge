// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

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
    virtual std::optional<std::uint64_t> SchemaRevision() const { return std::nullopt; }
    virtual void EnterRequiredField(const std::string& name) {
        if (!EnterField(name)) { throw std::runtime_error("missing required field"); }
        if (IsNull()) { Exit(); throw std::runtime_error("missing required field"); }
    }
    virtual bool EnterOptionalField(const std::string& name) {
        if (!EnterField(name)) { return false; }
        if (IsNull()) { Exit(); return false; }
        return true;
    }
    virtual void ExitField() { Exit(); }
    virtual std::size_t ReadEnumIndex(const std::vector<std::string>& variants) {
        const std::string value = AsString();
        for (std::size_t index = 0; index < variants.size(); ++index) {
            if (variants[index] == value) { return index; }
        }
        throw std::runtime_error("unknown enum value");
    }
    virtual std::size_t ReadRowCount() { return 0; }
    virtual void BeginRecord(std::size_t) {}
    virtual void EndRecord() {}
    virtual bool ReadOptionalPresent() { return false; }
    virtual std::int64_t ReadNextInt64() { return 0; }
    virtual double ReadNextDouble() { return 0.0; }
    virtual bool ReadNextBool() { return false; }
    virtual std::string ReadNextString() { return {}; }
    virtual std::uint64_t ReadEnumOrdinal() { return 0; }
    virtual void EnsureExhausted() {}
};

class IRuntimeDataLoader {
public:
    virtual ~IRuntimeDataLoader() = default;
    virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;
};

#define CONFIG_FORGE_RUNTIME_CPP_READER IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER IRuntimeDataLoader
#include "config_forge/runtime/binary_reader/runtime_loader.hpp"

namespace {
void WriteVaruint(std::vector<std::uint8_t>& bytes, std::uint64_t value) {
    do {
        std::uint8_t byte = static_cast<std::uint8_t>(value & 0x7f);
        value >>= 7;
        if (value != 0) {
            byte |= 0x80;
        }
        bytes.push_back(byte);
    } while (value != 0);
}

void WriteLe64(std::vector<std::uint8_t>& bytes, std::uint64_t value) {
    for (int index = 0; index < 8; ++index) {
        bytes.push_back(static_cast<std::uint8_t>((value >> (index * 8)) & 0xff));
    }
}
} // namespace

int main() {
    std::vector<std::uint8_t> bytes{'C', 'F', 'G', 'B', 1, 0, 16, 0};
    WriteLe64(bytes, 7);
    WriteVaruint(bytes, 1);
    bytes.push_back(1);
    WriteVaruint(bytes, 14);
    bytes.push_back(1);
    WriteVaruint(bytes, 3);

    config_forge_runtime::BinaryRuntimeDataReader reader(std::move(bytes));
    if (!reader.SchemaRevision() || *reader.SchemaRevision() != 7 || reader.ReadRowCount() != 1) {
        return 1;
    }
    reader.BeginRecord(1);
    if (!reader.ReadOptionalPresent() || reader.ReadNextInt64() != 7 || !reader.ReadNextBool() ||
        reader.ReadEnumIndex({"zero", "one", "two", "three"}) != 3) {
        return 2;
    }
    reader.EnsureExhausted();

    try {
        config_forge_runtime::BinaryFsRuntimeDataLoader loader(".");
        (void)loader.Open("../secret.bin");
        return 3;
    } catch (const std::runtime_error&) {
    }
    return 0;
}
