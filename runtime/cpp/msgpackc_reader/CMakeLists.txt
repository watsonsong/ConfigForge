# Copyright (c) 2026 ConfigForge contributors
# SPDX-License-Identifier: Apache-2.0

find_package(msgpack-cxx CONFIG QUIET)
if(NOT msgpack-cxx_FOUND)
    find_package(msgpack CONFIG QUIET)
endif()
if(NOT msgpack-cxx_FOUND AND NOT msgpack_FOUND)
    find_path(MSGPACK_CXX_INCLUDE_DIR msgpack.hpp)
endif()
if(NOT msgpack-cxx_FOUND AND NOT msgpack_FOUND AND NOT MSGPACK_CXX_INCLUDE_DIR)
    include(FetchContent)
    set(MSGPACK_USE_BOOST OFF CACHE BOOL "" FORCE)
    set(MSGPACK_BUILD_DOCS OFF CACHE BOOL "" FORCE)
    set(MSGPACK_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
    set(MSGPACK_BUILD_TESTS OFF CACHE BOOL "" FORCE)
    set(MSGPACK_CXX17 ON CACHE BOOL "" FORCE)
    FetchContent_Declare(
        msgpack_cxx
        GIT_REPOSITORY https://github.com/msgpack/msgpack-c.git
        GIT_TAG        cpp-9.0.0
        GIT_SHALLOW    TRUE)
    FetchContent_MakeAvailable(msgpack_cxx)
    set(MSGPACK_CXX_INCLUDE_DIR "${msgpack_cxx_SOURCE_DIR}/include")
endif()

add_library(config_forge_runtime_msgpackc_reader INTERFACE)
add_library(config_forge_runtime::msgpackc_reader ALIAS config_forge_runtime_msgpackc_reader)
target_include_directories(config_forge_runtime_msgpackc_reader INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_LIST_DIR}/include>)
target_compile_features(config_forge_runtime_msgpackc_reader INTERFACE cxx_std_17)
if(TARGET msgpack-cxx)
    target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE msgpack-cxx)
elseif(TARGET msgpackc-cxx)
    target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE msgpackc-cxx)
elseif(TARGET msgpack::msgpack)
    target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE msgpack::msgpack)
elseif(MSGPACK_CXX_INCLUDE_DIR)
    target_include_directories(config_forge_runtime_msgpackc_reader INTERFACE ${MSGPACK_CXX_INCLUDE_DIR})
endif()
