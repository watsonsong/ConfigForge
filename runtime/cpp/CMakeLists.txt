# Copyright (c) 2026 ConfigForge contributors
# SPDX-License-Identifier: Apache-2.0

cmake_minimum_required(VERSION 3.16)

project(ConfigForgeRuntime LANGUAGES CXX)

include(CMakePackageConfigHelpers)
include(GNUInstallDirs)

option(CONFIG_FORGE_RUNTIME_BUILD_TESTS "Build ConfigForge runtime helper tests" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_JSON_RAPIDJSON "Enable RapidJSON adapter" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_MESSAGEPACK "Enable msgpack-c adapter" OFF)
option(CONFIG_FORGE_RUNTIME_FETCH_DEPS "Allow FetchContent for missing third-party deps" OFF)

add_library(config_forge_runtime_core INTERFACE)
add_library(config_forge_runtime::core ALIAS config_forge_runtime_core)
set_target_properties(config_forge_runtime_core PROPERTIES EXPORT_NAME core)
target_include_directories(config_forge_runtime_core INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/core/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
target_compile_features(config_forge_runtime_core INTERFACE cxx_std_17)

if(CONFIG_FORGE_RUNTIME_WITH_JSON_RAPIDJSON)
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
            FetchContent_MakeAvailable(rapidjson)
            set(RAPIDJSON_INCLUDE_DIR "${rapidjson_SOURCE_DIR}/include")
        else()
            message(FATAL_ERROR "RapidJSON was not found. Install it with vcpkg/Conan/system packages, set RAPIDJSON_INCLUDE_DIR, or enable CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON.")
        endif()
    endif()
    add_library(config_forge_runtime_json_rapidjson INTERFACE)
    add_library(config_forge_runtime::json_rapidjson ALIAS config_forge_runtime_json_rapidjson)
    set_target_properties(config_forge_runtime_json_rapidjson PROPERTIES EXPORT_NAME json_rapidjson)
    target_include_directories(config_forge_runtime_json_rapidjson INTERFACE
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/json_rapidjson/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
    target_link_libraries(config_forge_runtime_json_rapidjson INTERFACE config_forge_runtime::core)
    if(TARGET RapidJSON::RapidJSON)
        target_link_libraries(config_forge_runtime_json_rapidjson INTERFACE RapidJSON::RapidJSON)
    elseif(RAPIDJSON_INCLUDE_DIR)
        target_include_directories(config_forge_runtime_json_rapidjson INTERFACE ${RAPIDJSON_INCLUDE_DIR})
    endif()
endif()

if(CONFIG_FORGE_RUNTIME_WITH_MESSAGEPACK)
    find_package(msgpack-cxx CONFIG QUIET)
    if(NOT msgpack-cxx_FOUND)
        find_package(msgpack CONFIG QUIET)
    endif()
    if(NOT msgpack-cxx_FOUND AND NOT msgpack_FOUND)
        if(CONFIG_FORGE_RUNTIME_FETCH_DEPS)
            message(FATAL_ERROR "msgpack-cxx/msgpack was not found. CONFIG_FORGE_RUNTIME_FETCH_DEPS is reserved, but built-in FetchContent is not implemented yet; install msgpack with vcpkg/Conan/system packages or provide it from the parent project.")
        endif()
        message(FATAL_ERROR "msgpack-cxx/msgpack was not found. Install it with vcpkg/Conan/system packages or provide it from the parent project.")
    endif()
    add_library(config_forge_runtime_messagepack_msgpack_cpp INTERFACE)
    add_library(config_forge_runtime::messagepack_msgpack_cpp ALIAS config_forge_runtime_messagepack_msgpack_cpp)
    set_target_properties(config_forge_runtime_messagepack_msgpack_cpp PROPERTIES EXPORT_NAME messagepack_msgpack_cpp)
    target_link_libraries(config_forge_runtime_messagepack_msgpack_cpp INTERFACE config_forge_runtime::core)
    if(msgpack-cxx_FOUND)
        target_link_libraries(config_forge_runtime_messagepack_msgpack_cpp INTERFACE msgpack-cxx)
    elseif(TARGET msgpackc-cxx)
        target_link_libraries(config_forge_runtime_messagepack_msgpack_cpp INTERFACE msgpackc-cxx)
    endif()
endif()

set(config_forge_runtime_install_targets config_forge_runtime_core)
if(TARGET config_forge_runtime_json_rapidjson)
    list(APPEND config_forge_runtime_install_targets config_forge_runtime_json_rapidjson)
endif()
if(TARGET config_forge_runtime_messagepack_msgpack_cpp)
    list(APPEND config_forge_runtime_install_targets config_forge_runtime_messagepack_msgpack_cpp)
endif()

install(TARGETS ${config_forge_runtime_install_targets}
    EXPORT ConfigForgeRuntimeTargets)
install(DIRECTORY core/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})

if(TARGET config_forge_runtime_json_rapidjson)
    install(DIRECTORY json_rapidjson/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
endif()

install(EXPORT ConfigForgeRuntimeTargets
    NAMESPACE config_forge_runtime::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/ConfigForgeRuntime)

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
    if(CONFIG_FORGE_RUNTIME_WITH_JSON_RAPIDJSON)
        add_executable(config_forge_runtime_json_rapidjson_smoke json_rapidjson/smoke_test.cpp)
        target_link_libraries(config_forge_runtime_json_rapidjson_smoke PRIVATE config_forge_runtime::json_rapidjson)
        add_test(NAME config_forge_runtime_json_rapidjson_smoke COMMAND config_forge_runtime_json_rapidjson_smoke)
    endif()
endif()
