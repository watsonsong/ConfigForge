// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <iterator>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#if !defined(CONFIG_FORGE_RUNTIME_CPP_READER) || !defined(CONFIG_FORGE_RUNTIME_CPP_LOADER)
#include "generated.hpp"

#define CONFIG_FORGE_RUNTIME_CPP_READER reference::game::config::IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER reference::game::config::IRuntimeDataLoader
#endif

namespace config_forge_runtime {

// Tight binary adapter for generated ConfigForge C++ runtime interfaces.
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

class BinaryRuntimeDataReader final : public CONFIG_FORGE_RUNTIME_CPP_READER {
public:
    explicit BinaryRuntimeDataReader(std::vector<std::uint8_t> bytes)
        : state_(std::make_shared<State>(std::move(bytes))) {
        ParseHeader();
    }

    BinaryRuntimeDataReader(const char* bytes, std::size_t size)
        : state_(std::make_shared<State>(std::vector<std::uint8_t>(
              reinterpret_cast<const std::uint8_t*>(bytes),
              reinterpret_cast<const std::uint8_t*>(bytes) + size))) {
        ParseHeader();
    }

    void EnterIndex(std::size_t index) override {
        if (index != state_->next_row_index) {
            throw std::runtime_error(
                "binary reader rows must be read sequentially: expected index " +
                std::to_string(state_->next_row_index) + ", got " + std::to_string(index));
        }
        ++state_->next_row_index;
    }

    std::int64_t AsInt64() const override { return ReadNextInt64Value(); }

    double AsDouble() const override { return ReadNextDoubleValue(); }

    bool AsBool() const override { return ReadNextBoolValue(); }

    std::string AsString() const override { return ReadNextStringValue(); }

    bool IsNull() const override { return false; }

    std::optional<std::uint64_t> SchemaRevision() const override { return state_->schema_revision; }

    void EnterRequiredField(const std::string&) override {}

    bool EnterOptionalField(const std::string&) override { return ReadOptionalPresent(); }

    void ExitField() override {}

    std::size_t ReadEnumIndex(const std::vector<std::string>& variants) override {
        const std::size_t ordinal = CheckedSize(ReadEnumOrdinal(), "enum ordinal");
        if (ordinal >= variants.size()) {
            throw std::runtime_error("enum ordinal is out of range");
        }
        return ordinal;
    }

    std::size_t ReadRowCount() override {
        const std::size_t row_count = CheckedSize(ReadVaruint(), "row count");
        state_->next_row_index = 0;
        return row_count;
    }

    void BeginRecord(std::size_t optional_slot_count) override {
        const std::size_t byte_count = (optional_slot_count + 7) / 8;
        const std::uint8_t* bytes = ReadExact(byte_count);
        state_->records.push_back(RecordState{std::vector<std::uint8_t>(bytes, bytes + byte_count), 0});
    }

    void EndRecord() override {
        if (state_->records.empty()) {
            throw std::runtime_error("binary reader record stack underflow");
        }
        state_->records.pop_back();
    }

    bool ReadOptionalPresent() override {
        if (state_->records.empty()) {
            throw std::runtime_error("binary reader is not inside a record");
        }
        auto& record = state_->records.back();
        const std::size_t slot_index = record.next_optional_slot++;
        const std::size_t byte_index = slot_index / 8;
        if (byte_index >= record.bitmap.size()) {
            throw std::runtime_error("optional slot is out of current record bitmap");
        }
        return (record.bitmap[byte_index] & (1u << (slot_index % 8))) != 0;
    }

    std::int64_t ReadNextInt64() override { return ReadNextInt64Value(); }

    double ReadNextDouble() override { return ReadNextDoubleValue(); }

    bool ReadNextBool() override { return ReadNextBoolValue(); }

    std::string ReadNextString() override { return ReadNextStringValue(); }

    std::uint64_t ReadEnumOrdinal() override { return ReadVaruint(); }

    void EnsureExhausted() override {
        if (state_->offset != state_->bytes.size()) {
            throw std::runtime_error("trailing bytes in binary runtime data");
        }
    }

private:
    struct RecordState {
        std::vector<std::uint8_t> bitmap;
        std::size_t next_optional_slot = 0;
    };

    struct State {
        explicit State(std::vector<std::uint8_t> input) : bytes(std::move(input)) {}

        std::vector<std::uint8_t> bytes;
        std::size_t offset = 16;
        std::uint64_t schema_revision = 0;
        std::size_t next_row_index = 0;
        std::vector<RecordState> records;
    };

    std::int64_t ReadNextInt64Value() const {
        const std::uint64_t encoded = ReadVaruint();
        return static_cast<std::int64_t>((encoded >> 1) ^ (0u - (encoded & 1u)));
    }

    double ReadNextDoubleValue() const {
        const std::uint64_t raw = ReadLe64(ReadExact(sizeof(std::uint64_t)));
        double value = 0.0;
        std::memcpy(&value, &raw, sizeof(value));
        if (!std::isfinite(value)) {
            throw std::runtime_error("non-finite float in binary runtime data");
        }
        return value;
    }

    bool ReadNextBoolValue() const {
        const std::uint8_t byte = *ReadExact(1);
        if (byte == 0) {
            return false;
        }
        if (byte == 1) {
            return true;
        }
        throw std::runtime_error("invalid bool byte in binary runtime data");
    }

    std::string ReadNextStringValue() const {
        const std::size_t len = CheckedSize(ReadVaruint(), "string length");
        const std::uint8_t* bytes = ReadExact(len);
        if (!IsValidUtf8(bytes, len)) {
            throw std::runtime_error("invalid UTF-8 string in binary runtime data");
        }
        return std::string(reinterpret_cast<const char*>(bytes), len);
    }

    void ParseHeader() {
        if (state_->bytes.size() < 16) {
            throw std::runtime_error("truncated binary runtime data header");
        }
        if (state_->bytes[0] != 'C' || state_->bytes[1] != 'F' || state_->bytes[2] != 'G' || state_->bytes[3] != 'B') {
            throw std::runtime_error("invalid binary runtime data magic");
        }
        const std::uint16_t version = ReadLe16At(4);
        if (version != 1) {
            throw std::runtime_error("unsupported binary runtime data version");
        }
        const std::uint16_t header_size = ReadLe16At(6);
        if (header_size != 16) {
            throw std::runtime_error("unsupported binary runtime data header size");
        }
        state_->schema_revision = ReadLe64(state_->bytes.data() + 8);
    }

    std::uint16_t ReadLe16At(std::size_t offset) const {
        return static_cast<std::uint16_t>(state_->bytes[offset]) |
               (static_cast<std::uint16_t>(state_->bytes[offset + 1]) << 8);
    }

    std::uint64_t ReadLe64(const std::uint8_t* bytes) const {
        std::uint64_t value = 0;
        for (std::size_t index = 0; index < 8; ++index) {
            value |= static_cast<std::uint64_t>(bytes[index]) << (index * 8);
        }
        return value;
    }

    std::uint64_t ReadVaruint() const {
        std::uint64_t value = 0;
        unsigned shift = 0;
        for (int index = 0; index < 10; ++index) {
            const std::uint8_t byte = *ReadExact(1);
            if (index == 9 && (byte & 0x7f) > 1) {
                throw std::runtime_error("varuint exceeds u64");
            }
            value |= static_cast<std::uint64_t>(byte & 0x7f) << shift;
            if ((byte & 0x80) == 0) {
                return value;
            }
            shift += 7;
        }
        throw std::runtime_error("malformed varuint");
    }

    const std::uint8_t* ReadExact(std::size_t len) const {
        const std::size_t end = state_->offset + len;
        if (end < state_->offset || end > state_->bytes.size()) {
            throw std::runtime_error("truncated binary runtime data");
        }
        const std::uint8_t* result = state_->bytes.data() + state_->offset;
        state_->offset = end;
        return result;
    }

    std::size_t CheckedSize(std::uint64_t value, const char* label) const {
        if (value > static_cast<std::uint64_t>(static_cast<std::size_t>(-1))) {
            throw std::runtime_error(std::string(label) + " exceeds size_t");
        }
        return static_cast<std::size_t>(value);
    }

    bool IsValidUtf8(const std::uint8_t* bytes, std::size_t len) const {
        std::size_t index = 0;
        while (index < len) {
            const std::uint8_t first = bytes[index++];
            if (first <= 0x7f) {
                continue;
            }
            std::uint32_t codepoint = 0;
            int continuation_count = 0;
            if ((first & 0xe0) == 0xc0) {
                codepoint = first & 0x1f;
                continuation_count = 1;
                if (codepoint == 0) {
                    return false;
                }
            } else if ((first & 0xf0) == 0xe0) {
                codepoint = first & 0x0f;
                continuation_count = 2;
            } else if ((first & 0xf8) == 0xf0) {
                codepoint = first & 0x07;
                continuation_count = 3;
            } else {
                return false;
            }
            if (index + static_cast<std::size_t>(continuation_count) > len) {
                return false;
            }
            for (int offset = 0; offset < continuation_count; ++offset) {
                const std::uint8_t next = bytes[index++];
                if ((next & 0xc0) != 0x80) {
                    return false;
                }
                codepoint = (codepoint << 6) | (next & 0x3f);
            }
            if ((continuation_count == 1 && codepoint < 0x80) ||
                (continuation_count == 2 && codepoint < 0x800) ||
                (continuation_count == 3 && codepoint < 0x10000) ||
                codepoint > 0x10ffff ||
                (codepoint >= 0xd800 && codepoint <= 0xdfff)) {
                return false;
            }
        }
        return true;
    }

    mutable std::shared_ptr<State> state_;
};

class BinaryFsRuntimeDataLoader final : public CONFIG_FORGE_RUNTIME_CPP_LOADER {
public:
    explicit BinaryFsRuntimeDataLoader(std::string data_dir) : data_dir_(std::move(data_dir)) {}

    std::unique_ptr<CONFIG_FORGE_RUNTIME_CPP_READER> Open(const std::string& path) override {
        std::ifstream file(SafeJoinPath(data_dir_, WithExtension(path, ".bin")), std::ios::binary);
        if (!file) {
            throw std::runtime_error("failed to open runtime data: " + path);
        }
        std::vector<std::uint8_t> bytes((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
        return std::make_unique<BinaryRuntimeDataReader>(std::move(bytes));
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
