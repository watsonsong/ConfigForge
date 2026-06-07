# Copyright (c) 2026 ConfigForge contributors
# SPDX-License-Identifier: Apache-2.0

cmake_minimum_required(VERSION 3.16)

project(ConfigForgeRuntime LANGUAGES CXX)

include(CMakePackageConfigHelpers)
include(GNUInstallDirs)

option(CONFIG_FORGE_RUNTIME_BUILD_TESTS "Build ConfigForge runtime helper tests" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER "Enable RapidJSON reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER "Enable msgpack-cxx MessagePack reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_FETCH_DEPS "Allow FetchContent for missing third-party deps" OFF)

if(CONFIG_FORGE_RUNTIME_FETCH_DEPS AND NOT DEFINED CMAKE_POLICY_VERSION_MINIMUM)
    # Some pinned header-only dependencies still declare very old CMake minimum
    # versions. Newer CMake releases require an explicit compatibility floor for
    # those subprojects when they are fetched as part of the runtime smoke tests.
    set(CMAKE_POLICY_VERSION_MINIMUM 3.5 CACHE STRING "Minimum CMake policy version for fetched runtime dependencies")
endif()

add_library(config_forge_runtime_core INTERFACE)
add_library(config_forge_runtime::core ALIAS config_forge_runtime_core)
set_target_properties(config_forge_runtime_core PROPERTIES EXPORT_NAME core)
target_include_directories(config_forge_runtime_core INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/core/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
target_compile_features(config_forge_runtime_core INTERFACE cxx_std_17)

add_library(config_forge_runtime_binary_reader INTERFACE)
add_library(config_forge_runtime::binary_reader ALIAS config_forge_runtime_binary_reader)
set_target_properties(config_forge_runtime_binary_reader PROPERTIES EXPORT_NAME binary_reader)
target_include_directories(config_forge_runtime_binary_reader INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/binary_reader/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
target_compile_features(config_forge_runtime_binary_reader INTERFACE cxx_std_17)
target_link_libraries(config_forge_runtime_binary_reader INTERFACE config_forge_runtime::core)

if(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER)
    find_package(RapidJSON CONFIG QUIET)
    if(RapidJSON_FOUND AND NOT TARGET RapidJSON::RapidJSON)
        foreach(config_forge_runtime_rapidjson_var
                RapidJSON_INCLUDE_DIRS
                RapidJSON_INCLUDE_DIR
                rapidjson_INCLUDE_DIRS
                rapidjson_INCLUDE_DIR)
            if(DEFINED ${config_forge_runtime_rapidjson_var} AND NOT RAPIDJSON_INCLUDE_DIR)
                set(RAPIDJSON_INCLUDE_DIR "${${config_forge_runtime_rapidjson_var}}")
            endif()
        endforeach()
    endif()
    if(NOT RapidJSON_FOUND AND NOT TARGET RapidJSON::RapidJSON)
        find_path(RAPIDJSON_INCLUDE_DIR rapidjson/document.h)
    endif()
    if(NOT RapidJSON_FOUND AND NOT TARGET RapidJSON::RapidJSON AND NOT RAPIDJSON_INCLUDE_DIR)
        if(CONFIG_FORGE_RUNTIME_FETCH_DEPS)
            include(FetchContent)
            FetchContent_Declare(
                rapidjson
                GIT_REPOSITORY https://github.com/Tencent/rapidjson.git
                GIT_TAG        v1.1.0
                GIT_SHALLOW    TRUE)
            FetchContent_GetProperties(rapidjson)
            if(NOT rapidjson_POPULATED)
                if(POLICY CMP0169)
                    cmake_policy(PUSH)
                    cmake_policy(SET CMP0169 OLD)
                endif()
                FetchContent_Populate(rapidjson)
                if(POLICY CMP0169)
                    cmake_policy(POP)
                endif()
            endif()
            set(RAPIDJSON_INCLUDE_DIR "${rapidjson_SOURCE_DIR}/include")
        else()
            message(FATAL_ERROR "RapidJSON was not found. Install it with vcpkg/Conan/system packages, set RAPIDJSON_INCLUDE_DIR, or enable CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON.")
        endif()
    endif()
    add_library(config_forge_runtime_rapidjson_reader INTERFACE)
    add_library(config_forge_runtime::rapidjson_reader ALIAS config_forge_runtime_rapidjson_reader)
    set_target_properties(config_forge_runtime_rapidjson_reader PROPERTIES EXPORT_NAME rapidjson_reader)
    target_include_directories(config_forge_runtime_rapidjson_reader INTERFACE
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/rapidjson_reader/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
    target_compile_features(config_forge_runtime_rapidjson_reader INTERFACE cxx_std_17)
    target_link_libraries(config_forge_runtime_rapidjson_reader INTERFACE config_forge_runtime::core)
    if(TARGET RapidJSON::RapidJSON)
        target_link_libraries(config_forge_runtime_rapidjson_reader INTERFACE RapidJSON::RapidJSON)
    elseif(RAPIDJSON_INCLUDE_DIR)
        target_include_directories(config_forge_runtime_rapidjson_reader INTERFACE
            $<BUILD_INTERFACE:${RAPIDJSON_INCLUDE_DIR}>)
    endif()
endif()

if(CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER)
    find_package(msgpack-cxx CONFIG QUIET)
    if(NOT msgpack-cxx_FOUND)
        find_package(msgpack CONFIG QUIET)
    endif()
    if(NOT msgpack-cxx_FOUND AND NOT msgpack_FOUND)
        find_path(MSGPACK_CXX_INCLUDE_DIR msgpack.hpp)
    endif()
    if(NOT msgpack-cxx_FOUND AND NOT msgpack_FOUND AND NOT MSGPACK_CXX_INCLUDE_DIR)
        if(CONFIG_FORGE_RUNTIME_FETCH_DEPS)
            include(FetchContent)
            set(MSGPACK_USE_BOOST OFF CACHE BOOL "" FORCE)
            set(MSGPACK_BUILD_DOCS OFF CACHE BOOL "" FORCE)
            set(MSGPACK_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
            set(MSGPACK_BUILD_TESTS OFF CACHE BOOL "" FORCE)
            set(MSGPACK_CXX17 ON CACHE BOOL "" FORCE)
            FetchContent_Declare(
                msgpack_cxx
                GIT_REPOSITORY https://github.com/msgpack/msgpack-c.git
                GIT_TAG        cpp-6.1.1
                GIT_SHALLOW    TRUE)
            FetchContent_GetProperties(msgpack_cxx)
            if(NOT msgpack_cxx_POPULATED)
                if(POLICY CMP0169)
                    cmake_policy(PUSH)
                    cmake_policy(SET CMP0169 OLD)
                endif()
                FetchContent_Populate(msgpack_cxx)
                if(POLICY CMP0169)
                    cmake_policy(POP)
                endif()
            endif()
            set(MSGPACK_CXX_INCLUDE_DIR "${msgpack_cxx_SOURCE_DIR}/include")
        else()
            message(FATAL_ERROR "msgpack-cxx/msgpack was not found. Install it with vcpkg/Conan/system packages, set MSGPACK_CXX_INCLUDE_DIR, or enable CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON.")
        endif()
    endif()
    add_library(config_forge_runtime_msgpackc_reader INTERFACE)
    add_library(config_forge_runtime::msgpackc_reader ALIAS config_forge_runtime_msgpackc_reader)
    set_target_properties(config_forge_runtime_msgpackc_reader PROPERTIES EXPORT_NAME msgpackc_reader)
    target_include_directories(config_forge_runtime_msgpackc_reader INTERFACE
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/msgpackc_reader/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
    target_compile_features(config_forge_runtime_msgpackc_reader INTERFACE cxx_std_17)
    target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE config_forge_runtime::core)
    if(TARGET msgpack-cxx)
        target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE msgpack-cxx)
    elseif(TARGET msgpackc-cxx)
        target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE msgpackc-cxx)
    elseif(TARGET msgpack::msgpack)
        target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE msgpack::msgpack)
    elseif(MSGPACK_CXX_INCLUDE_DIR)
        target_include_directories(config_forge_runtime_msgpackc_reader INTERFACE
            $<BUILD_INTERFACE:${MSGPACK_CXX_INCLUDE_DIR}>)
        target_compile_definitions(config_forge_runtime_msgpackc_reader INTERFACE MSGPACK_NO_BOOST)
    endif()
endif()

set(config_forge_runtime_install_targets config_forge_runtime_core config_forge_runtime_binary_reader)
if(TARGET config_forge_runtime_rapidjson_reader)
    list(APPEND config_forge_runtime_install_targets config_forge_runtime_rapidjson_reader)
endif()
if(TARGET config_forge_runtime_msgpackc_reader)
    list(APPEND config_forge_runtime_install_targets config_forge_runtime_msgpackc_reader)
endif()

install(TARGETS ${config_forge_runtime_install_targets}
    EXPORT ConfigForgeRuntimeTargets)

install(EXPORT ConfigForgeRuntimeTargets
    NAMESPACE config_forge_runtime::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/ConfigForgeRuntime)

install(DIRECTORY binary_reader/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
install(DIRECTORY core/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
if(TARGET config_forge_runtime_rapidjson_reader)
    install(DIRECTORY rapidjson_reader/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
endif()
if(TARGET config_forge_runtime_msgpackc_reader)
    install(DIRECTORY msgpackc_reader/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
endif()

configure_package_config_file(
    ${CMAKE_CURRENT_SOURCE_DIR}/cmake/ConfigForgeRuntimeConfig.cmake.in
    ${CMAKE_CURRENT_BINARY_DIR}/ConfigForgeRuntimeConfig.cmake
    INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/ConfigForgeRuntime)

write_basic_package_version_file(
    ${CMAKE_CURRENT_BINARY_DIR}/ConfigForgeRuntimeConfigVersion.cmake
    VERSION 0.1.0
    COMPATIBILITY SameMajorVersion)

install(FILES
    ${CMAKE_CURRENT_BINARY_DIR}/ConfigForgeRuntimeConfig.cmake
    ${CMAKE_CURRENT_BINARY_DIR}/ConfigForgeRuntimeConfigVersion.cmake
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/ConfigForgeRuntime)

if(CONFIG_FORGE_RUNTIME_BUILD_TESTS)
    enable_testing()
    add_executable(config_forge_runtime_binary_reader_smoke binary_reader/smoke_test.cpp)
    target_link_libraries(config_forge_runtime_binary_reader_smoke PRIVATE config_forge_runtime::binary_reader)
    add_test(NAME config_forge_runtime_binary_reader_smoke COMMAND config_forge_runtime_binary_reader_smoke)
    if(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER)
        add_executable(config_forge_runtime_rapidjson_reader_smoke rapidjson_reader/smoke_test.cpp)
        target_link_libraries(config_forge_runtime_rapidjson_reader_smoke PRIVATE config_forge_runtime::rapidjson_reader)
        add_test(NAME config_forge_runtime_rapidjson_reader_smoke COMMAND config_forge_runtime_rapidjson_reader_smoke)
    endif()
    if(CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER)
        add_executable(config_forge_runtime_msgpackc_reader_smoke msgpackc_reader/smoke_test.cpp)
        target_link_libraries(config_forge_runtime_msgpackc_reader_smoke PRIVATE config_forge_runtime::msgpackc_reader)
        add_test(NAME config_forge_runtime_msgpackc_reader_smoke COMMAND config_forge_runtime_msgpackc_reader_smoke)
    endif()
endif()
