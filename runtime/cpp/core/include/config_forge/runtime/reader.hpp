// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <cstdint>
#include <cstddef>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

namespace config_forge_runtime {

inline constexpr const char* CONFIG_FORGE_READER_CONTRACT_VERSION = "1.0";

class IRuntimeDataReader {
public:
    virtual ~IRuntimeDataReader() = default;
    virtual void EnterIndex(std::size_t index) = 0;
    virtual std::int64_t AsInt64() const = 0;
    virtual double AsDouble() const = 0;
    virtual bool AsBool() const = 0;
    virtual std::string AsString() const = 0;
    virtual std::optional<std::uint64_t> SchemaRevision() const { return std::nullopt; }
    virtual void EnterRequiredField(const std::string& name) = 0;
    virtual bool EnterOptionalField(const std::string& name) = 0;
    virtual void ExitField() = 0;
    virtual std::size_t ReadEnumIndex(const std::vector<std::string>& variants) {
        const std::string value = AsString();
        for (std::size_t index = 0; index < variants.size(); ++index) {
            if (variants[index] == value) { return index; }
        }
        throw std::runtime_error("unknown enum value `" + value + "`");
    }
    virtual std::size_t ReadRowCount() = 0;
    virtual std::size_t ReadListLength() { throw std::runtime_error("runtime reader does not support list length"); }
    virtual bool ArrayElementsArePositional() const { return false; }
    virtual void BeginRecord(std::size_t) {}
    virtual void EndRecord() {}
    virtual void EnsureExhausted() {}
};

class IRuntimeDataLoader {
public:
    virtual ~IRuntimeDataLoader() = default;
    virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;
};

} // namespace config_forge_runtime
