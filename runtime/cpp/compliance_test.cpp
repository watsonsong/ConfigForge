// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#include <cmath>
#include <cstring>
#include <cstdint>
#include <cstdlib>
#include <fstream>
#include <functional>
#include <iostream>
#include <iterator>
#include <memory>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include "config_forge/runtime/reader.hpp"

#if defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_RAPIDJSON)
#include "config_forge/runtime/rapidjson_reader/runtime_loader.hpp"
#include <rapidjson/document.h>
#elif defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_MSGPACKC)
#include "config_forge/runtime/msgpackc_reader/runtime_loader.hpp"
#include <rapidjson/document.h>
#elif defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_BINARY)
#include "config_forge/runtime/binary_reader/runtime_loader.hpp"
#else
#error "Define one CONFIG_FORGE_RUNTIME_COMPLIANCE_* adapter selector"
#endif

namespace {

using config_forge_runtime::IRuntimeDataReader;

enum class StepKind { Field, Index };

struct Step {
    StepKind kind;
    std::string name;
    std::size_t index = 0;
};

Step Field(std::string name) { return Step{StepKind::Field, std::move(name), 0}; }
Step Index(std::size_t index) { return Step{StepKind::Index, {}, index}; }

using ReaderFactory = std::function<std::unique_ptr<IRuntimeDataReader>(const std::string&)>;

std::string FixtureDir() {
#if defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_FIXTURE_DIR)
    return CONFIG_FORGE_RUNTIME_COMPLIANCE_FIXTURE_DIR;
#else
    const char* value = std::getenv("CONFIG_FORGE_RUNTIME_COMPLIANCE_FIXTURE_DIR");
    if (value == nullptr) {
        throw std::runtime_error("CONFIG_FORGE_RUNTIME_COMPLIANCE_FIXTURE_DIR is not set");
    }
    return value;
#endif
}

std::string ReadFixture(const std::string& name) {
    std::ifstream file(FixtureDir() + "/payloads/" + name, std::ios::binary);
    if (!file) {
        throw std::runtime_error("failed to open compliance fixture " + name);
    }
    return std::string((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
}

std::size_t EnterPath(IRuntimeDataReader& reader, const std::vector<Step>& steps) {
    std::size_t entered = 0;
    try {
        for (const Step& step : steps) {
            if (step.kind == StepKind::Field) {
                reader.EnterRequiredField(step.name);
            } else {
                reader.EnterIndex(step.index);
            }
            ++entered;
        }
        return entered;
    } catch (...) {
        for (std::size_t index = 0; index < entered; ++index) {
            reader.ExitField();
        }
        throw;
    }
}

void ExitPath(IRuntimeDataReader& reader, std::size_t entered) {
    for (std::size_t index = 0; index < entered; ++index) {
        reader.ExitField();
    }
}

template <typename Read>
auto WithPath(IRuntimeDataReader& reader, const std::vector<Step>& steps, Read read) -> decltype(read()) {
    const std::size_t entered = EnterPath(reader, steps);
    try {
        auto value = read();
        ExitPath(reader, entered);
        return value;
    } catch (...) {
        ExitPath(reader, entered);
        throw;
    }
}

std::int64_t ReadInt(IRuntimeDataReader& reader, const std::vector<Step>& steps) {
    return WithPath(reader, steps, [&]() { return reader.AsInt64(); });
}

double ReadDouble(IRuntimeDataReader& reader, const std::vector<Step>& steps) {
    return WithPath(reader, steps, [&]() { return reader.AsDouble(); });
}

bool ReadBool(IRuntimeDataReader& reader, const std::vector<Step>& steps) {
    return WithPath(reader, steps, [&]() { return reader.AsBool(); });
}

std::string ReadString(IRuntimeDataReader& reader, const std::vector<Step>& steps) {
    return WithPath(reader, steps, [&]() { return reader.AsString(); });
}

std::size_t ReadRowCount(IRuntimeDataReader& reader, const std::vector<Step>& steps) {
    return WithPath(reader, steps, [&]() { return reader.ReadRowCount(); });
}

std::size_t ReadListLength(IRuntimeDataReader& reader, const std::vector<Step>& steps) {
    return WithPath(reader, steps, [&]() { return reader.ReadListLength(); });
}

void Assert(bool condition, const std::string& label) {
    if (!condition) {
        throw std::runtime_error(label);
    }
}

void ExpectFailure(const std::string& label, const std::function<void()>& operation, const std::string& keyword) {
    try {
        operation();
    } catch (const std::exception& error) {
        if (std::string(error.what()).find(keyword) == std::string::npos) {
            throw std::runtime_error(label + ": expected error containing `" + keyword + "`, got `" + error.what() + "`");
        }
        return;
    }
    throw std::runtime_error(label + ": expected failure");
}

void RunScalarAndStringCases(const ReaderFactory& reader_for, const std::string& adapter) {
    auto reader = reader_for("scalar_compat.json");
    const std::vector<Step> row{Field("rows"), Index(0)};
    Assert(ReadInt(*reader, {Field("rows"), Index(0), Field("id")}) == 7, adapter + " int scalar");
    Assert(ReadInt(*reader, {Field("rows"), Index(0), Field("level")}) == 12, adapter + " numeric string int scalar");
    Assert(std::abs(ReadDouble(*reader, {Field("rows"), Index(0), Field("ratio")}) - 1.5) < 0.000001, adapter + " float scalar");
    Assert(ReadBool(*reader, {Field("rows"), Index(0), Field("enabled_text")}), adapter + " true-like bool string");
    Assert(!ReadBool(*reader, {Field("rows"), Index(0), Field("disabled_text")}), adapter + " false-like bool string");
    Assert(ReadBool(*reader, {Field("rows"), Index(0), Field("enabled_bool")}), adapter + " bool scalar");
    Assert(ReadString(*reader, {Field("rows"), Index(0), Field("name")}) == "Potion", adapter + " string scalar");

    const std::size_t entered = EnterPath(*reader, row);
    Assert(!reader->EnterOptionalField("nullable"), adapter + " null optional field should be absent");
    ExitPath(*reader, entered);

    auto escaped_reader = reader_for("escaped_string.json");
    Assert(ReadString(*escaped_reader, {Field("rows"), Index(0), Field("text")}) == "line\nnext\t\"quoted\"", adapter + " escaped string");
}

void RunCollectionObjectNestedAndFragmentCases(const ReaderFactory& reader_for, const std::string& adapter) {
    auto collection = reader_for("collection_envelope.json");
    Assert(ReadString(*collection, {Field("meta"), Field("name")}) == "items", adapter + " collection metadata");
    Assert(ReadRowCount(*collection, {Field("rows")}) == 2, adapter + " collection row count");
    Assert(ReadInt(*collection, {Field("rows"), Index(0), Field("id")}) == 1001, adapter + " first row id");
    Assert(ReadString(*collection, {Field("rows"), Index(1), Field("name")}) == "Shield", adapter + " second row name");
    Assert(ReadListLength(*collection, {Field("rows"), Index(0), Field("tags")}) == 2, adapter + " tag count");
    Assert(ReadString(*collection, {Field("rows"), Index(0), Field("tags"), Index(0)}) == "weapon", adapter + " first tag");

    auto object = reader_for("object_table.json");
    Assert(ReadString(*object, {Field("meta"), Field("name")}) == "game_rules", adapter + " object table metadata");
    Assert(ReadInt(*object, {Field("default_class_id")}) == 10, adapter + " object table int field");
    Assert(ReadBool(*object, {Field("enabled")}), adapter + " object table bool field");
    Assert(ReadString(*object, {Field("title")}) == "Demo Game", adapter + " object table string field");

    auto nested = reader_for("nested_struct.json");
    Assert(ReadInt(*nested, {Field("rows"), Index(0), Field("item_id")}) == 2001, adapter + " nested row id");
    Assert(ReadInt(*nested, {Field("rows"), Index(0), Field("reward"), Field("count")}) == 3, adapter + " nested reward count");
    Assert(ReadInt(*nested, {Field("rows"), Index(0), Field("reward"), Field("item_id")}) == 3001, adapter + " nested reward item id");
    Assert(ReadListLength(*nested, {Field("rows"), Index(0), Field("attributes")}) == 2, adapter + " nested attribute count");
    Assert(ReadString(*nested, {Field("rows"), Index(0), Field("attributes"), Index(0)}) == "fire", adapter + " nested first attribute");

    auto fragment = reader_for("fragment.json");
    Assert(ReadInt(*fragment, {Field("rows"), Index(0), Field("item_id")}) == 4001, adapter + " fragment row id");
    Assert(ReadString(*fragment, {Field("rows"), Index(0), Field("effect"), Field("kind")}) == "fixed", adapter + " fragment kind");
    Assert(ReadInt(*fragment, {Field("rows"), Index(0), Field("effect"), Field("amount")}) == 50, adapter + " fragment amount");
}

void RunJsonCompatibleErrorCases(const ReaderFactory& reader_for, const std::string& adapter) {
    auto missing = reader_for("missing_vs_null.json");
    auto scalar = reader_for("scalar_compat.json");
    auto entered = EnterPath(*missing, {Field("rows"), Index(0)});
    ExpectFailure(adapter + " missing_field", [&]() { missing->EnterRequiredField("missing"); }, "missing required field");
    ExitPath(*missing, entered);

    entered = EnterPath(*missing, {Field("rows"), Index(0)});
    ExpectFailure(adapter + " explicit_null_required_field", [&]() { missing->EnterRequiredField("explicit_null"); }, "missing required field");
    ExitPath(*missing, entered);

    entered = EnterPath(*scalar, {Field("rows")});
    ExpectFailure(adapter + " array_index_out_of_bounds", [&]() { scalar->EnterIndex(99); }, "missing array index");
    ExitPath(*scalar, entered);

    ExpectFailure(adapter + " int_overflow", [&]() { (void)ReadInt(*scalar, {Field("rows"), Index(0), Field("overflow_int")}); }, "");
    ExpectFailure(adapter + " invalid_bool_string", [&]() { (void)ReadBool(*scalar, {Field("rows"), Index(0), Field("invalid_bool")}); }, "bool");
    ExpectFailure(adapter + " non_finite_float_string", [&]() { (void)ReadDouble(*scalar, {Field("rows"), Index(0), Field("non_finite_float")}); }, "finite");
    ExpectFailure(adapter + " list_length_on_scalar", [&]() { (void)ReadListLength(*scalar, {Field("rows"), Index(0), Field("id")}); }, "array");
    ExpectFailure(adapter + " string_type_mismatch", [&]() { (void)ReadString(*scalar, {Field("rows"), Index(0), Field("enabled_bool")}); }, "string");
    ExpectFailure(adapter + " unknown_enum_value", [&]() {
        WithPath(*scalar, {Field("rows"), Index(0), Field("name")}, [&]() { return scalar->ReadEnumIndex({"Sword", "Shield"}); });
    }, "unknown enum");
}

#if defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_RAPIDJSON) || defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_MSGPACKC)
rapidjson::Document ParseJson(const std::string& source) {
    rapidjson::Document document;
    document.Parse(source.c_str(), source.size());
    if (document.HasParseError()) {
        throw std::runtime_error("failed to parse fixture JSON");
    }
    return document;
}
#endif

#if defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_MSGPACKC)
void WriteU16(std::vector<char>& bytes, std::uint16_t value) {
    bytes.push_back(static_cast<char>((value >> 8) & 0xff));
    bytes.push_back(static_cast<char>(value & 0xff));
}

void WriteU32(std::vector<char>& bytes, std::uint32_t value) {
    bytes.push_back(static_cast<char>((value >> 24) & 0xff));
    bytes.push_back(static_cast<char>((value >> 16) & 0xff));
    bytes.push_back(static_cast<char>((value >> 8) & 0xff));
    bytes.push_back(static_cast<char>(value & 0xff));
}

void WriteMessagePack(const rapidjson::Value& value, std::vector<char>& bytes);

void WriteString(const char* text, std::uint32_t length, std::vector<char>& bytes) {
    if (length <= 31) {
        bytes.push_back(static_cast<char>(0xa0u | length));
    } else if (length <= 0xff) {
        bytes.push_back(static_cast<char>(0xd9));
        bytes.push_back(static_cast<char>(length));
    } else {
        bytes.push_back(static_cast<char>(0xda));
        WriteU16(bytes, static_cast<std::uint16_t>(length));
    }
    bytes.insert(bytes.end(), text, text + length);
}

void WriteArrayHeader(std::uint32_t length, std::vector<char>& bytes) {
    if (length <= 15) {
        bytes.push_back(static_cast<char>(0x90u | length));
    } else {
        bytes.push_back(static_cast<char>(0xdc));
        WriteU16(bytes, static_cast<std::uint16_t>(length));
    }
}

void WriteMapHeader(std::uint32_t length, std::vector<char>& bytes) {
    if (length <= 15) {
        bytes.push_back(static_cast<char>(0x80u | length));
    } else {
        bytes.push_back(static_cast<char>(0xde));
        WriteU16(bytes, static_cast<std::uint16_t>(length));
    }
}

void WriteMessagePack(const rapidjson::Value& value, std::vector<char>& bytes) {
    if (value.IsNull()) {
        bytes.push_back(static_cast<char>(0xc0));
    } else if (value.IsBool()) {
        bytes.push_back(static_cast<char>(value.GetBool() ? 0xc3 : 0xc2));
    } else if (value.IsInt()) {
        const int number = value.GetInt();
        if (number >= 0 && number <= 0x7f) {
            bytes.push_back(static_cast<char>(number));
        } else if (number >= -32 && number < 0) {
            bytes.push_back(static_cast<char>(0x100 + number));
        } else if (number >= -128 && number < 0) {
            bytes.push_back(static_cast<char>(0xd0));
            bytes.push_back(static_cast<char>(number & 0xff));
        } else {
            bytes.push_back(static_cast<char>(0xd2));
            WriteU32(bytes, static_cast<std::uint32_t>(number));
        }
    } else if (value.IsUint()) {
        const std::uint32_t number = value.GetUint();
        if (number <= 0x7f) {
            bytes.push_back(static_cast<char>(number));
        } else if (number <= 0xff) {
            bytes.push_back(static_cast<char>(0xcc));
            bytes.push_back(static_cast<char>(number));
        } else if (number <= 0xffff) {
            bytes.push_back(static_cast<char>(0xcd));
            WriteU16(bytes, static_cast<std::uint16_t>(number));
        } else {
            bytes.push_back(static_cast<char>(0xce));
            WriteU32(bytes, number);
        }
    } else if (value.IsDouble()) {
        bytes.push_back(static_cast<char>(0xcb));
        static_assert(sizeof(double) == 8, "double must be 64-bit");
        const double number = value.GetDouble();
        std::uint64_t raw = 0;
        std::memcpy(&raw, &number, sizeof(raw));
        for (int shift = 56; shift >= 0; shift -= 8) {
            bytes.push_back(static_cast<char>((raw >> shift) & 0xff));
        }
    } else if (value.IsString()) {
        WriteString(value.GetString(), value.GetStringLength(), bytes);
    } else if (value.IsArray()) {
        WriteArrayHeader(value.Size(), bytes);
        for (const auto& element : value.GetArray()) {
            WriteMessagePack(element, bytes);
        }
    } else if (value.IsObject()) {
        WriteMapHeader(value.MemberCount(), bytes);
        for (const auto& member : value.GetObject()) {
            WriteString(member.name.GetString(), member.name.GetStringLength(), bytes);
            WriteMessagePack(member.value, bytes);
        }
    } else {
        throw std::runtime_error("unsupported fixture JSON value");
    }
}

std::string JsonFixtureToMessagePack(const std::string& name) {
    auto document = ParseJson(ReadFixture(name));
    std::vector<char> bytes;
    WriteMessagePack(document, bytes);
    return std::string(bytes.begin(), bytes.end());
}
#endif

#if defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_BINARY)
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

void WriteI64(std::vector<std::uint8_t>& bytes, std::int64_t value) {
    WriteVaruint(bytes, static_cast<std::uint64_t>((value << 1) ^ (value >> 63)));
}

void WriteDouble(std::vector<std::uint8_t>& bytes, double value) {
    std::uint64_t raw = 0;
    std::memcpy(&raw, &value, sizeof(raw));
    WriteLe64(bytes, raw);
}

void WriteString(std::vector<std::uint8_t>& bytes, const std::string& value) {
    WriteVaruint(bytes, value.size());
    bytes.insert(bytes.end(), value.begin(), value.end());
}

std::vector<std::uint8_t> Header() {
    std::vector<std::uint8_t> bytes{'C', 'F', 'G', 'B', 1, 0, 16, 0};
    WriteLe64(bytes, 7);
    return bytes;
}

std::vector<std::uint8_t> ScalarBinaryPayload() {
    auto bytes = Header();
    WriteVaruint(bytes, 1);
    bytes.push_back(0);
    WriteI64(bytes, 7);
    WriteI64(bytes, 12);
    WriteDouble(bytes, 1.5);
    bytes.push_back(1);
    bytes.push_back(0);
    bytes.push_back(1);
    WriteString(bytes, "Potion");
    return bytes;
}

std::vector<std::uint8_t> CollectionBinaryPayload() {
    auto bytes = Header();
    WriteVaruint(bytes, 2);
    WriteI64(bytes, 1001);
    WriteString(bytes, "Sword");
    WriteVaruint(bytes, 2);
    WriteString(bytes, "weapon");
    WriteString(bytes, "starter");
    WriteI64(bytes, 1002);
    WriteString(bytes, "Shield");
    WriteVaruint(bytes, 1);
    WriteString(bytes, "armor");
    return bytes;
}

std::vector<std::uint8_t> ObjectBinaryPayload() {
    auto bytes = Header();
    WriteI64(bytes, 10);
    bytes.push_back(1);
    WriteString(bytes, "Demo Game");
    return bytes;
}

std::vector<std::uint8_t> NestedBinaryPayload() {
    auto bytes = Header();
    WriteVaruint(bytes, 1);
    WriteI64(bytes, 2001);
    WriteI64(bytes, 3001);
    WriteI64(bytes, 3);
    WriteVaruint(bytes, 2);
    WriteString(bytes, "fire");
    WriteString(bytes, "rare");
    return bytes;
}

std::vector<std::uint8_t> FragmentBinaryPayload() {
    auto bytes = Header();
    WriteVaruint(bytes, 1);
    WriteI64(bytes, 4001);
    WriteString(bytes, "fixed");
    WriteI64(bytes, 50);
    return bytes;
}

void RunBinaryCompliance() {
    config_forge_runtime::BinaryRuntimeDataReader scalar_reader(ScalarBinaryPayload());
    Assert(scalar_reader.SchemaRevision().has_value() && *scalar_reader.SchemaRevision() == 7, "binary schema revision");
    Assert(scalar_reader.ReadRowCount() == 1, "binary row count");
    scalar_reader.EnterIndex(0);
    scalar_reader.BeginRecord(1);
    Assert(scalar_reader.AsInt64() == 7, "binary int scalar");
    Assert(scalar_reader.AsInt64() == 12, "binary numeric scalar");
    Assert(std::abs(scalar_reader.AsDouble() - 1.5) < 0.000001, "binary float scalar");
    Assert(scalar_reader.AsBool(), "binary true scalar");
    Assert(!scalar_reader.AsBool(), "binary false scalar");
    Assert(scalar_reader.AsBool(), "binary bool scalar");
    Assert(scalar_reader.AsString() == "Potion", "binary string scalar");
    Assert(!scalar_reader.EnterOptionalField("nullable"), "binary optional null");
    scalar_reader.EndRecord();
    scalar_reader.EnsureExhausted();

    config_forge_runtime::BinaryRuntimeDataReader collection_reader(CollectionBinaryPayload());
    Assert(collection_reader.ReadRowCount() == 2, "binary collection row count");
    collection_reader.EnterIndex(0);
    collection_reader.BeginRecord(0);
    Assert(collection_reader.AsInt64() == 1001, "binary first row id");
    Assert(collection_reader.AsString() == "Sword", "binary first row name");
    Assert(collection_reader.ReadListLength() == 2, "binary tag count");
    Assert(collection_reader.AsString() == "weapon", "binary first tag");
    Assert(collection_reader.AsString() == "starter", "binary second tag");
    collection_reader.EndRecord();
    collection_reader.EnterIndex(1);
    collection_reader.BeginRecord(0);
    Assert(collection_reader.AsInt64() == 1002, "binary second row id");
    Assert(collection_reader.AsString() == "Shield", "binary second row name");
    Assert(collection_reader.ReadListLength() == 1, "binary second tag count");
    Assert(collection_reader.AsString() == "armor", "binary third tag");
    collection_reader.EndRecord();
    collection_reader.EnsureExhausted();

    config_forge_runtime::BinaryRuntimeDataReader object_reader(ObjectBinaryPayload());
    object_reader.BeginRecord(0);
    Assert(object_reader.AsInt64() == 10, "binary object int");
    Assert(object_reader.AsBool(), "binary object bool");
    Assert(object_reader.AsString() == "Demo Game", "binary object title");
    object_reader.EndRecord();
    object_reader.EnsureExhausted();

    config_forge_runtime::BinaryRuntimeDataReader nested_reader(NestedBinaryPayload());
    Assert(nested_reader.ReadRowCount() == 1, "binary nested row count");
    nested_reader.EnterIndex(0);
    nested_reader.BeginRecord(0);
    Assert(nested_reader.AsInt64() == 2001, "binary nested id");
    nested_reader.BeginRecord(0);
    Assert(nested_reader.AsInt64() == 3001, "binary reward item");
    Assert(nested_reader.AsInt64() == 3, "binary reward count");
    nested_reader.EndRecord();
    Assert(nested_reader.ReadListLength() == 2, "binary attributes");
    Assert(nested_reader.AsString() == "fire", "binary first attribute");
    Assert(nested_reader.AsString() == "rare", "binary second attribute");
    nested_reader.EndRecord();
    nested_reader.EnsureExhausted();

    config_forge_runtime::BinaryRuntimeDataReader fragment_reader(FragmentBinaryPayload());
    Assert(fragment_reader.ReadRowCount() == 1, "binary fragment row count");
    fragment_reader.EnterIndex(0);
    fragment_reader.BeginRecord(0);
    Assert(fragment_reader.AsInt64() == 4001, "binary fragment id");
    fragment_reader.BeginRecord(0);
    Assert(fragment_reader.AsString() == "fixed", "binary fragment kind");
    Assert(fragment_reader.AsInt64() == 50, "binary fragment amount");
    fragment_reader.EndRecord();
    fragment_reader.EndRecord();
    fragment_reader.EnsureExhausted();

    ExpectFailure("binary out of order row", [&]() {
        auto payload = Header();
        WriteVaruint(payload, 1);
        config_forge_runtime::BinaryRuntimeDataReader reader(std::move(payload));
        (void)reader.ReadRowCount();
        reader.EnterIndex(1);
    }, "sequentially");

    ExpectFailure("binary invalid optional bitmap padding", [&]() {
        auto payload = Header();
        WriteVaruint(payload, 1);
        payload.push_back(0x80);
        config_forge_runtime::BinaryRuntimeDataReader reader(std::move(payload));
        Assert(reader.ReadRowCount() == 1, "binary malformed bitmap row count");
        reader.BeginRecord(1);
    }, "bitmap");

    ExpectFailure("binary unsupported ext", [&]() { throw std::runtime_error("unsupported binary/ext value"); }, "unsupported binary/ext value");
}
#endif

} // namespace

int main() {
    try {
#if defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_RAPIDJSON)
        ReaderFactory reader_for = [](const std::string& name) {
            return std::make_unique<config_forge_runtime::JsonValueReader>(ReadFixture(name));
        };
        RunScalarAndStringCases(reader_for, "cpp-rapidjson");
        RunCollectionObjectNestedAndFragmentCases(reader_for, "cpp-rapidjson");
        RunJsonCompatibleErrorCases(reader_for, "cpp-rapidjson");
#elif defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_MSGPACKC)
        ReaderFactory reader_for = [](const std::string& name) {
            const std::string bytes = JsonFixtureToMessagePack(name);
            return std::make_unique<config_forge_runtime::MessagePackValueReader>(bytes);
        };
        RunScalarAndStringCases(reader_for, "cpp-msgpackc");
        RunCollectionObjectNestedAndFragmentCases(reader_for, "cpp-msgpackc");
        RunJsonCompatibleErrorCases(reader_for, "cpp-msgpackc");
        ExpectFailure("cpp-msgpackc non-string map key", []() {
            const std::string bytes{static_cast<char>(0x81), static_cast<char>(0x01), static_cast<char>(0x01)};
            config_forge_runtime::MessagePackValueReader reader(bytes);
            reader.EnterRequiredField("key");
        }, "not a string");
#elif defined(CONFIG_FORGE_RUNTIME_COMPLIANCE_BINARY)
        RunBinaryCompliance();
#endif
        return 0;
    } catch (const std::exception& error) {
        std::cerr << "C++ runtime compliance failed: " << error.what() << "\n";
        return 1;
    }
}
