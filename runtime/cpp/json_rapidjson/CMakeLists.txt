# Copyright (c) 2026 ConfigForge contributors
# SPDX-License-Identifier: Apache-2.0

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
    include(FetchContent)
    FetchContent_Declare(
        rapidjson
        GIT_REPOSITORY https://github.com/Tencent/rapidjson.git
        GIT_TAG        v1.1.0
        GIT_SHALLOW    TRUE)
    FetchContent_MakeAvailable(rapidjson)
    set(RAPIDJSON_INCLUDE_DIR "${rapidjson_SOURCE_DIR}/include")
endif()

add_library(config_forge_runtime_json_rapidjson INTERFACE)
add_library(config_forge_runtime::json_rapidjson ALIAS config_forge_runtime_json_rapidjson)
target_include_directories(config_forge_runtime_json_rapidjson INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_LIST_DIR}/include>)
target_compile_features(config_forge_runtime_json_rapidjson INTERFACE cxx_std_17)
if(TARGET RapidJSON::RapidJSON)
    target_link_libraries(config_forge_runtime_json_rapidjson INTERFACE RapidJSON::RapidJSON)
elseif(RAPIDJSON_INCLUDE_DIR)
    target_include_directories(config_forge_runtime_json_rapidjson INTERFACE ${RAPIDJSON_INCLUDE_DIR})
endif()
