// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, process::Command as StdCommand};

use super::{assert_command_success, require_tool_or_skip};

pub(crate) fn verify_standard_generated_cpp_unreal_runtime(
    _fixture_root: &std::path::Path,
    output_root: &std::path::Path,
) {
    if require_tool_or_skip("standard generated cpp_unreal syntax test", "c++").is_none() {
        return;
    }

    let temp_dir = tempfile::tempdir().expect("cpp_unreal harness temp dir should be created");
    let generated_dir = output_root.join("client_code/cpp_unreal");
    write_cpp_unreal_core_minimal_shim(temp_dir.path());

    let output = StdCommand::new("c++")
        .arg("-std=c++17")
        .arg("-fsyntax-only")
        .arg(generated_dir.join("reference_game_config.cpp"))
        .arg("-I")
        .arg(&generated_dir)
        .arg("-I")
        .arg(temp_dir.path())
        .output()
        .expect("standard generated cpp_unreal syntax test should run");
    assert_command_success(output, "standard generated cpp_unreal syntax test");
}
pub(crate) fn write_cpp_unreal_core_minimal_shim(shim_dir: &std::path::Path) {
    fs::create_dir_all(shim_dir).expect("cpp_unreal shim dir should be created");
    fs::write(
        shim_dir.join("CoreMinimal.h"),
        r#"
            #pragma once
            #include <algorithm>
            #include <cstdint>
            #include <functional>
            #include <memory>
            #include <optional>
            #include <string>
            #include <unordered_map>
            #include <vector>
            class UBlueprintFunctionLibrary {};
            
            using int32 = std::int32_t;
            using int64 = std::int64_t;
            using uint32 = std::uint32_t;
            using uint8 = std::uint8_t;
            using TCHAR = char;
            #define REFERENCE_GAME_CONFIG_API
            #define FORCEINLINE inline
            #define TEXT(value) value
            #define USTRUCT(...)
            #define UPROPERTY(...)
            #define UENUM(...)
            #define UCLASS(...)
            #define UFUNCTION(...)
            #define GENERATED_BODY()
            using FString = std::string;
            struct FLinearColor {
                float R;
                float G;
                float B;
                float A;
                FLinearColor() : R(0), G(0), B(0), A(1) {}
                FLinearColor(float InR, float InG, float InB, float InA) : R(InR), G(InG), B(InB), A(InA) {}
            };
            struct FVector2D {
                float X;
                float Y;
                FVector2D() : X(0), Y(0) {}
                FVector2D(float InX, float InY) : X(InX), Y(InY) {}
            };
            struct FVector {
                float X;
                float Y;
                float Z;
                FVector() : X(0), Y(0), Z(0) {}
                FVector(float InX, float InY, float InZ) : X(InX), Y(InY), Z(InZ) {}
                static const FVector ZeroVector;
            };
            inline const FVector FVector::ZeroVector = FVector();
            struct FVector4 {
                float X;
                float Y;
                float Z;
                float W;
                FVector4() : X(0), Y(0), Z(0), W(0) {}
                FVector4(float InX, float InY, float InZ, float InW) : X(InX), Y(InY), Z(InZ), W(InW) {}
            };
            struct FIntPoint {
                int32 X;
                int32 Y;
                FIntPoint() : X(0), Y(0) {}
                FIntPoint(int32 InX, int32 InY) : X(InX), Y(InY) {}
            };
            struct FIntVector {
                int32 X;
                int32 Y;
                int32 Z;
                FIntVector() : X(0), Y(0), Z(0) {}
                FIntVector(int32 InX, int32 InY, int32 InZ) : X(InX), Y(InY), Z(InZ) {}
            };
            struct FIntVector4 {
                int32 X;
                int32 Y;
                int32 Z;
                int32 W;
                FIntVector4() : X(0), Y(0), Z(0), W(0) {}
                FIntVector4(int32 InX, int32 InY, int32 InZ, int32 InW) : X(InX), Y(InY), Z(InZ), W(InW) {}
            };
            inline uint32 HashCombine(uint32 Left, uint32 Right) { return Left ^ (Right + 0x9e3779b9U + (Left << 6) + (Left >> 2)); }
            template <typename T>
            uint32 GetTypeHash(const T& Value) { return static_cast<uint32>(std::hash<T>{}(Value)); }
            template <typename T>
            struct FUnrealHash {
                std::size_t operator()(const T& Value) const { return GetTypeHash(Value); }
            };
            template <typename T>
            using TUniquePtr = std::unique_ptr<T>;
            template <typename T>
            struct TArray : public std::vector<T> {
                using std::vector<T>::vector;
                void Add(const T& Value) { this->push_back(Value); }
                void Reset() { this->clear(); }
                void Reserve(std::size_t Count) { this->reserve(Count); }
                int32 Num() const { return static_cast<int32>(this->size()); }
                bool IsValidIndex(int32 Index) const { return Index >= 0 && static_cast<std::size_t>(Index) < this->size(); }
                template <typename Predicate>
                void StableSort(Predicate Less) { std::stable_sort(this->begin(), this->end(), Less); }
            };
            template <typename K, typename V>
            struct TPair {
                K Key;
                V Value;
                TPair() = default;
                TPair(const K& InKey, const V& InValue) : Key(InKey), Value(InValue) {}
            };
            template <typename K, typename V>
            struct TMap : public std::unordered_map<K, V, FUnrealHash<K>> {
                bool Contains(const K& Key) const { return this->find(Key) != this->end(); }
                void Add(const K& Key, const V& Value) { (*this)[Key] = Value; }
                V* Find(const K& Key) { auto Iter = this->find(Key); return Iter == this->end() ? nullptr : &Iter->second; }
                const V* Find(const K& Key) const { auto Iter = this->find(Key); return Iter == this->end() ? nullptr : &Iter->second; }
                V& FindOrAdd(const K& Key) { return (*this)[Key]; }
            };
            template <typename Signature>
            using TFunction = std::function<Signature>;
            template <typename T>
            struct TOptional : public std::optional<T> {
                using std::optional<T>::optional;
                TOptional() = default;
                TOptional(const T& Value) : std::optional<T>(Value) {}
                TOptional& operator=(const T& Value) { std::optional<T>::operator=(Value); return *this; }
                bool IsSet() const { return this->has_value(); }
                T& GetValue() { return this->value(); }
                const T& GetValue() const { return this->value(); }
            };
            namespace Algo {
                template <typename Array, typename Value, typename Projection>
                int32 LowerBoundBy(const Array& Values, const Value& Target, Projection Project) {
                    return static_cast<int32>(std::lower_bound(Values.begin(), Values.end(), Target, [&](const auto& Entry, const auto& Needle) { return Project(Entry) < Needle; }) - Values.begin());
                }
                template <typename Array, typename Value, typename Projection>
                int32 UpperBoundBy(const Array& Values, const Value& Target, Projection Project) {
                    return static_cast<int32>(std::upper_bound(Values.begin(), Values.end(), Target, [&](const auto& Needle, const auto& Entry) { return Needle < Project(Entry); }) - Values.begin());
                }
            }
        "#,
    )
    .expect("CoreMinimal shim should be written");
    fs::create_dir_all(shim_dir.join("Kismet")).expect("Kismet shim dir should be created");
    fs::write(
        shim_dir.join("Kismet/BlueprintFunctionLibrary.h"),
        "#pragma once\n",
    )
    .expect("BlueprintFunctionLibrary shim should be written");
}
