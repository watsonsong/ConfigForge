// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::code_artifact::{CodeWriterLike, RuntimeInterfaceMode};

use super::CppCodegenOptions;

const DEFAULT_RUNTIME_CORE_HEADER: &str = "config_forge/runtime/reader.hpp";
const DEFAULT_RUNTIME_CORE_NAMESPACE: &str = "config_forge_runtime";

pub(super) fn render_runtime_interface_include(
    content: &mut impl CodeWriterLike,
    options: &CppCodegenOptions,
) {
    if !matches!(options.runtime_interface, RuntimeInterfaceMode::External) {
        return;
    }
    let header = options
        .runtime_core_header
        .as_deref()
        .unwrap_or(DEFAULT_RUNTIME_CORE_HEADER);
    content.raw_fmt(format_args!("#include {:?}\n\n", header));
}

pub(super) fn render_runtime_interfaces(
    content: &mut impl CodeWriterLike,
    options: &CppCodegenOptions,
) {
    if matches!(options.runtime_interface, RuntimeInterfaceMode::External) {
        render_external_runtime_interfaces(content, options);
        render_builtin_value_types(content);
        return;
    }

    content.raw("inline constexpr const char* CONFIG_FORGE_READER_CONTRACT_VERSION = \"1.0\";\n\n");
    content.raw(concat!(
        "class IRuntimeDataReader {\n",
        "public:\n",
        "    virtual ~IRuntimeDataReader() = default;\n",
        "    virtual void EnterIndex(std::size_t index) = 0;\n",
        "    virtual std::int64_t AsInt64() const = 0;\n",
        "    virtual double AsDouble() const = 0;\n",
        "    virtual bool AsBool() const = 0;\n",
        "    virtual std::string AsString() const = 0;\n",
        "    virtual std::optional<std::uint64_t> SchemaRevision() const { return std::nullopt; }\n",
        "    virtual void EnterRequiredField(const std::string& name) = 0;\n",
        "    virtual bool EnterOptionalField(const std::string& name) = 0;\n",
        "    virtual void ExitField() = 0;\n",
        "    virtual std::size_t ReadEnumIndex(const std::vector<std::string>& variants) {\n",
        "        const std::string value = AsString();\n",
        "        for (std::size_t index = 0; index < variants.size(); ++index) {\n",
        "            if (variants[index] == value) { return index; }\n",
        "        }\n",
        "        throw std::runtime_error(\"unknown enum value `\" + value + \"`\");\n",
        "    }\n",
        "    virtual std::size_t ReadRowCount() = 0;\n",
        "    virtual std::size_t ReadListLength() { throw std::runtime_error(\"runtime reader does not support list length\"); }\n",
        "    virtual bool ArrayElementsArePositional() const { return false; }\n",
        "    virtual void BeginRecord(std::size_t) {}\n",
        "    virtual void EndRecord() {}\n",
        "    virtual void EnsureExhausted() {}\n",
        "};\n\n",
        "class IRuntimeDataLoader {\n",
        "public:\n",
        "    virtual ~IRuntimeDataLoader() = default;\n",
        "    virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;\n",
        "};\n\n",
    ));
    render_builtin_value_types(content);
}

fn render_builtin_value_types(content: &mut impl CodeWriterLike) {
    content.raw(concat!(
        "struct ConfigForgeColor {\n",
        "    double r = 0.0;\n",
        "    double g = 0.0;\n",
        "    double b = 0.0;\n",
        "    double a = 1.0;\n",
        "};\n\n",
        "struct ConfigForgeVector2 { double x = 0.0; double y = 0.0; };\n",
        "struct ConfigForgeVector3 { double x = 0.0; double y = 0.0; double z = 0.0; };\n",
        "struct ConfigForgeVector4 { double x = 0.0; double y = 0.0; double z = 0.0; double w = 0.0; };\n",
        "struct ConfigForgeVector2Int { std::int64_t x = 0; std::int64_t y = 0; };\n",
        "struct ConfigForgeVector3Int { std::int64_t x = 0; std::int64_t y = 0; std::int64_t z = 0; };\n",
        "struct ConfigForgeVector4Int { std::int64_t x = 0; std::int64_t y = 0; std::int64_t z = 0; std::int64_t w = 0; };\n\n",
    ));
}

fn render_external_runtime_interfaces(
    content: &mut impl CodeWriterLike,
    options: &CppCodegenOptions,
) {
    let namespace = options
        .runtime_core_namespace
        .as_deref()
        .unwrap_or(DEFAULT_RUNTIME_CORE_NAMESPACE);
    content.raw_fmt(format_args!(
        "using {namespace}::CONFIG_FORGE_READER_CONTRACT_VERSION;\n"
    ));
    content.raw_fmt(format_args!("using {namespace}::IRuntimeDataLoader;\n"));
    content.raw_fmt(format_args!("using {namespace}::IRuntimeDataReader;\n\n"));
}

pub(super) fn render_reader_helpers_source(content: &mut impl CodeWriterLike) {
    content.raw(concat!(
        "namespace {\n\n",
        "class ReaderScope {\n",
        "public:\n",
        "    explicit ReaderScope(IRuntimeDataReader& reader) : reader_(&reader) {}\n",
        "    ReaderScope(ReaderScope&& other) noexcept : reader_(other.reader_) { other.reader_ = nullptr; }\n",
        "    ReaderScope(const ReaderScope&) = delete;\n",
        "    ReaderScope& operator=(const ReaderScope&) = delete;\n",
        "    ~ReaderScope() noexcept { if (reader_) { try { reader_->ExitField(); } catch (...) {} } }\n",
        "private:\n",
        "    IRuntimeDataReader* reader_;\n",
        "};\n\n",
        "ReaderScope EnterRequiredField(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    reader.EnterRequiredField(name);\n",
        "    return ReaderScope(reader);\n",
        "}\n\n",
        "template <typename F>\n",
        "auto RequiredField(\n",
        "    IRuntimeDataReader& reader,\n",
        "    const std::string& name,\n",
        "    F convert) -> decltype(convert(std::declval<IRuntimeDataReader&>())) {\n",
        "    auto scope = EnterRequiredField(reader, name);\n",
        "    return convert(reader);\n",
        "}\n\n",
        "template <typename F>\n",
        "auto OptionalField(\n",
        "    IRuntimeDataReader& reader,\n",
        "    const std::string& name,\n",
        "    F convert) -> std::optional<decltype(convert(std::declval<IRuntimeDataReader&>()))> {\n",
        "    if (!reader.EnterOptionalField(name)) { return std::nullopt; }\n",
        "    ReaderScope scope(reader);\n",
        "    return convert(reader);\n",
        "}\n\n",
        "std::int64_t ReadRequiredInt64(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return RequiredField(reader, name, [](IRuntimeDataReader& value) { return value.AsInt64(); });\n",
        "}\n\n",
        "double ReadRequiredDouble(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return RequiredField(reader, name, [](IRuntimeDataReader& value) { return value.AsDouble(); });\n",
        "}\n\n",
        "bool ReadRequiredBool(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return RequiredField(reader, name, [](IRuntimeDataReader& value) { return value.AsBool(); });\n",
        "}\n\n",
        "std::string ReadRequiredString(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return RequiredField(reader, name, [](IRuntimeDataReader& value) { return value.AsString(); });\n",
        "}\n\n",
        "std::optional<std::int64_t> ReadOptionalInt64(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return OptionalField(reader, name, [](IRuntimeDataReader& value) { return value.AsInt64(); });\n",
        "}\n\n",
        "std::optional<double> ReadOptionalDouble(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return OptionalField(reader, name, [](IRuntimeDataReader& value) { return value.AsDouble(); });\n",
        "}\n\n",
        "std::optional<bool> ReadOptionalBool(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return OptionalField(reader, name, [](IRuntimeDataReader& value) { return value.AsBool(); });\n",
        "}\n\n",
        "std::optional<std::string> ReadOptionalString(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return OptionalField(reader, name, [](IRuntimeDataReader& value) { return value.AsString(); });\n",
        "}\n\n",
        "ConfigForgeColor ReadColor(IRuntimeDataReader& reader) {\n",
        "    reader.BeginRecord(0);\n",
        "    try {\n",
        "        auto value = ConfigForgeColor{\n",
        "            ReadRequiredDouble(reader, \"r\"),\n",
        "            ReadRequiredDouble(reader, \"g\"),\n",
        "            ReadRequiredDouble(reader, \"b\"),\n",
        "            ReadRequiredDouble(reader, \"a\")\n",
        "        };\n",
        "        reader.EndRecord();\n",
        "        return value;\n",
        "    } catch (...) {\n",
        "        reader.EndRecord();\n",
        "        throw;\n",
        "    }\n",
        "}\n\n",
        "ConfigForgeColor ReadRequiredColor(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return RequiredField(reader, name, [](IRuntimeDataReader& value) { return ReadColor(value); });\n",
        "}\n\n",
        "std::optional<ConfigForgeColor> ReadOptionalColor(IRuntimeDataReader& reader, const std::string& name) {\n",
        "    return OptionalField(reader, name, [](IRuntimeDataReader& value) { return ReadColor(value); });\n",
        "}\n\n",
        "ConfigForgeVector2 ReadConfigForgeVector2(IRuntimeDataReader& reader) {\n",
        "    reader.BeginRecord(0);\n",
        "    try { auto value = ConfigForgeVector2{ReadRequiredDouble(reader, \"x\"), ReadRequiredDouble(reader, \"y\")}; reader.EndRecord(); return value; }\n",
        "    catch (...) { reader.EndRecord(); throw; }\n",
        "}\n\n",
        "ConfigForgeVector3 ReadConfigForgeVector3(IRuntimeDataReader& reader) {\n",
        "    reader.BeginRecord(0);\n",
        "    try { auto value = ConfigForgeVector3{ReadRequiredDouble(reader, \"x\"), ReadRequiredDouble(reader, \"y\"), ReadRequiredDouble(reader, \"z\")}; reader.EndRecord(); return value; }\n",
        "    catch (...) { reader.EndRecord(); throw; }\n",
        "}\n\n",
        "ConfigForgeVector4 ReadConfigForgeVector4(IRuntimeDataReader& reader) {\n",
        "    reader.BeginRecord(0);\n",
        "    try { auto value = ConfigForgeVector4{ReadRequiredDouble(reader, \"x\"), ReadRequiredDouble(reader, \"y\"), ReadRequiredDouble(reader, \"z\"), ReadRequiredDouble(reader, \"w\")}; reader.EndRecord(); return value; }\n",
        "    catch (...) { reader.EndRecord(); throw; }\n",
        "}\n\n",
        "ConfigForgeVector2Int ReadConfigForgeVector2Int(IRuntimeDataReader& reader) {\n",
        "    reader.BeginRecord(0);\n",
        "    try { auto value = ConfigForgeVector2Int{static_cast<std::int64_t>(ReadRequiredDouble(reader, \"x\")), static_cast<std::int64_t>(ReadRequiredDouble(reader, \"y\"))}; reader.EndRecord(); return value; }\n",
        "    catch (...) { reader.EndRecord(); throw; }\n",
        "}\n\n",
        "ConfigForgeVector3Int ReadConfigForgeVector3Int(IRuntimeDataReader& reader) {\n",
        "    reader.BeginRecord(0);\n",
        "    try { auto value = ConfigForgeVector3Int{static_cast<std::int64_t>(ReadRequiredDouble(reader, \"x\")), static_cast<std::int64_t>(ReadRequiredDouble(reader, \"y\")), static_cast<std::int64_t>(ReadRequiredDouble(reader, \"z\"))}; reader.EndRecord(); return value; }\n",
        "    catch (...) { reader.EndRecord(); throw; }\n",
        "}\n\n",
        "ConfigForgeVector4Int ReadConfigForgeVector4Int(IRuntimeDataReader& reader) {\n",
        "    reader.BeginRecord(0);\n",
        "    try { auto value = ConfigForgeVector4Int{static_cast<std::int64_t>(ReadRequiredDouble(reader, \"x\")), static_cast<std::int64_t>(ReadRequiredDouble(reader, \"y\")), static_cast<std::int64_t>(ReadRequiredDouble(reader, \"z\")), static_cast<std::int64_t>(ReadRequiredDouble(reader, \"w\"))}; reader.EndRecord(); return value; }\n",
        "    catch (...) { reader.EndRecord(); throw; }\n",
        "}\n\n",
        "ConfigForgeVector2 ReadRequiredConfigForgeVector2(IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector2(value); }); }\n",
        "ConfigForgeVector3 ReadRequiredConfigForgeVector3(IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector3(value); }); }\n",
        "ConfigForgeVector4 ReadRequiredConfigForgeVector4(IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector4(value); }); }\n",
        "ConfigForgeVector2Int ReadRequiredConfigForgeVector2Int(IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector2Int(value); }); }\n",
        "ConfigForgeVector3Int ReadRequiredConfigForgeVector3Int(IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector3Int(value); }); }\n",
        "ConfigForgeVector4Int ReadRequiredConfigForgeVector4Int(IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector4Int(value); }); }\n\n",
        "std::optional<ConfigForgeVector2> ReadOptionalConfigForgeVector2(IRuntimeDataReader& reader, const std::string& name) { return OptionalField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector2(value); }); }\n",
        "std::optional<ConfigForgeVector3> ReadOptionalConfigForgeVector3(IRuntimeDataReader& reader, const std::string& name) { return OptionalField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector3(value); }); }\n",
        "std::optional<ConfigForgeVector4> ReadOptionalConfigForgeVector4(IRuntimeDataReader& reader, const std::string& name) { return OptionalField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector4(value); }); }\n",
        "std::optional<ConfigForgeVector2Int> ReadOptionalConfigForgeVector2Int(IRuntimeDataReader& reader, const std::string& name) { return OptionalField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector2Int(value); }); }\n",
        "std::optional<ConfigForgeVector3Int> ReadOptionalConfigForgeVector3Int(IRuntimeDataReader& reader, const std::string& name) { return OptionalField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector3Int(value); }); }\n",
        "std::optional<ConfigForgeVector4Int> ReadOptionalConfigForgeVector4Int(IRuntimeDataReader& reader, const std::string& name) { return OptionalField(reader, name, [](IRuntimeDataReader& value) { return ReadConfigForgeVector4Int(value); }); }\n\n",
        "template <typename F>\n",
        "auto ReadList(IRuntimeDataReader& reader, F convert) -> std::vector<decltype(convert(std::declval<IRuntimeDataReader&>()))> {\n",
        "    using Value = decltype(convert(std::declval<IRuntimeDataReader&>()));\n",
        "    const std::size_t length = reader.ReadListLength();\n",
        "    const bool positional = reader.ArrayElementsArePositional();\n",
        "    std::vector<Value> values;\n",
        "    for (std::size_t index = 0; index < length; ++index) {\n",
        "        if (positional) {\n",
        "            values.push_back(convert(reader));\n",
        "            continue;\n",
        "        }\n",
        "        reader.EnterIndex(index);\n",
        "        ReaderScope scope(reader);\n",
        "        values.push_back(convert(reader));\n",
        "    }\n",
        "    return values;\n",
        "}\n\n",
        "template <typename F>\n",
        "auto ReadRequiredList(IRuntimeDataReader& reader, const std::string& name, F convert)\n",
        "    -> decltype(ReadList(reader, convert)) {\n",
        "    return RequiredField(reader, name, [&](IRuntimeDataReader& value) { return ReadList(value, convert); });\n",
        "}\n\n",
        "template <typename F>\n",
        "auto ReadOptionalList(IRuntimeDataReader& reader, const std::string& name, F convert)\n",
        "    -> std::optional<decltype(ReadList(reader, convert))> {\n",
        "    return OptionalField(reader, name, [&](IRuntimeDataReader& value) { return ReadList(value, convert); });\n",
        "}\n\n",
    ));
}
