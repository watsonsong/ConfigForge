#pragma once

#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>

namespace config_forge_runtime {

inline constexpr const char* SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0";

class RuntimeDataReader {
public:
    virtual ~RuntimeDataReader() = default;
    virtual std::unique_ptr<RuntimeDataReader> Field(const std::string& name) const = 0;
    virtual std::unique_ptr<RuntimeDataReader> Index(std::size_t index) const = 0;
    virtual std::size_t Size() const = 0;
    virtual std::int64_t AsInt64() const = 0;
    virtual double AsDouble() const = 0;
    virtual bool AsBool() const = 0;
    virtual std::string AsString() const = 0;
    virtual bool IsNull() const = 0;
};

class RuntimeDataLoader {
public:
    virtual ~RuntimeDataLoader() = default;
    virtual std::unique_ptr<RuntimeDataReader> Open(const std::string& path) = 0;
};

} // namespace config_forge_runtime
