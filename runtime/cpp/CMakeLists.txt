# Copyright (c) 2026 ConfigForge contributors
# SPDX-License-Identifier: Apache-2.0

cmake_minimum_required(VERSION 3.16)

project(ConfigForgeRuntime LANGUAGES CXX)

include(CMakePackageConfigHelpers)
include(GNUInstallDirs)

option(CONFIG_FORGE_RUNTIME_BUILD_TESTS "Build ConfigForge runtime helper tests" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER "Enable RapidJSON reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_MESSAGEPACK "Enable msgpack-c adapter" OFF)
option(CONFIG_FORGE_RUNTIME_FETCH_DEPS "Allow FetchContent for missing third-party deps" OFF)

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
            FetchContent_MakeAvailable(rapidjson)
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
    if(TARGET RapidJSON::RapidJSON)
        target_link_libraries(config_forge_runtime_rapidjson_reader INTERFACE RapidJSON::RapidJSON)
    elseif(RAPIDJSON_INCLUDE_DIR)
        target_include_directories(config_forge_runtime_rapidjson_reader INTERFACE ${RAPIDJSON_INCLUDE_DIR})
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
    target_compile_features(config_forge_runtime_messagepack_msgpack_cpp INTERFACE cxx_std_17)
    if(msgpack-cxx_FOUND)
        target_link_libraries(config_forge_runtime_messagepack_msgpack_cpp INTERFACE msgpack-cxx)
    elseif(TARGET msgpackc-cxx)
        target_link_libraries(config_forge_runtime_messagepack_msgpack_cpp INTERFACE msgpackc-cxx)
    endif()
endif()

set(config_forge_runtime_install_targets)
if(TARGET config_forge_runtime_rapidjson_reader)
    list(APPEND config_forge_runtime_install_targets config_forge_runtime_rapidjson_reader)
endif()
if(TARGET config_forge_runtime_messagepack_msgpack_cpp)
    list(APPEND config_forge_runtime_install_targets config_forge_runtime_messagepack_msgpack_cpp)
endif()

if(config_forge_runtime_install_targets)
    install(TARGETS ${config_forge_runtime_install_targets}
        EXPORT ConfigForgeRuntimeTargets)

    install(EXPORT ConfigForgeRuntimeTargets
        NAMESPACE config_forge_runtime::
        DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/ConfigForgeRuntime)
endif()

if(TARGET config_forge_runtime_rapidjson_reader)
    install(DIRECTORY rapidjson_reader/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
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
    if(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER)
        add_executable(config_forge_runtime_rapidjson_reader_smoke rapidjson_reader/smoke_test.cpp)
        target_link_libraries(config_forge_runtime_rapidjson_reader_smoke PRIVATE config_forge_runtime::rapidjson_reader)
        add_test(NAME config_forge_runtime_rapidjson_reader_smoke COMMAND config_forge_runtime_rapidjson_reader_smoke)
    endif()
endif()
