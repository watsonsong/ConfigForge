cmake_minimum_required(VERSION 3.16)

project(ConfigForgeRuntimeReaders LANGUAGES CXX)

include(CMakePackageConfigHelpers)
include(GNUInstallDirs)

option(CONFIG_FORGE_RUNTIME_BUILD_TESTS "Build ConfigForge runtime reader tests" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_JSON_BUILTIN "Enable minimal builtin JSON fallback" OFF)
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

if(CONFIG_FORGE_RUNTIME_WITH_JSON_BUILTIN)
    add_library(config_forge_runtime_json_builtin_test_fallback INTERFACE)
    add_library(config_forge_runtime::json_builtin_test_fallback ALIAS config_forge_runtime_json_builtin_test_fallback)
    set_target_properties(config_forge_runtime_json_builtin_test_fallback PROPERTIES EXPORT_NAME json_builtin_test_fallback)
    target_include_directories(config_forge_runtime_json_builtin_test_fallback INTERFACE
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/json_builtin/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
    target_link_libraries(config_forge_runtime_json_builtin_test_fallback INTERFACE config_forge_runtime::core)
    target_compile_definitions(config_forge_runtime_json_builtin_test_fallback INTERFACE
        CONFIG_FORGE_RUNTIME_JSON_BUILTIN_TEST_FALLBACK=1)
endif()

if(CONFIG_FORGE_RUNTIME_WITH_JSON_RAPIDJSON)
    find_package(RapidJSON CONFIG QUIET)
    if(NOT RapidJSON_FOUND)
        if(CONFIG_FORGE_RUNTIME_FETCH_DEPS)
            message(FATAL_ERROR "RapidJSON was not found. CONFIG_FORGE_RUNTIME_FETCH_DEPS is reserved, but built-in FetchContent is not implemented yet; install RapidJSON with vcpkg/Conan/system packages or provide it from the parent project.")
        endif()
        message(FATAL_ERROR "RapidJSON was not found. Install it with vcpkg/Conan/system packages or provide it from the parent project.")
    endif()
    add_library(config_forge_runtime_json_rapidjson INTERFACE)
    add_library(config_forge_runtime::json_rapidjson ALIAS config_forge_runtime_json_rapidjson)
    set_target_properties(config_forge_runtime_json_rapidjson PROPERTIES EXPORT_NAME json_rapidjson)
    target_link_libraries(config_forge_runtime_json_rapidjson INTERFACE config_forge_runtime::core RapidJSON::RapidJSON)
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
if(TARGET config_forge_runtime_json_builtin_test_fallback)
    list(APPEND config_forge_runtime_install_targets config_forge_runtime_json_builtin_test_fallback)
endif()
if(TARGET config_forge_runtime_json_rapidjson)
    list(APPEND config_forge_runtime_install_targets config_forge_runtime_json_rapidjson)
endif()
if(TARGET config_forge_runtime_messagepack_msgpack_cpp)
    list(APPEND config_forge_runtime_install_targets config_forge_runtime_messagepack_msgpack_cpp)
endif()

install(TARGETS ${config_forge_runtime_install_targets}
    EXPORT ConfigForgeRuntimeReadersTargets)
install(DIRECTORY core/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})

if(TARGET config_forge_runtime_json_builtin_test_fallback)
    install(DIRECTORY json_builtin/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
endif()

install(EXPORT ConfigForgeRuntimeReadersTargets
    NAMESPACE config_forge_runtime::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/ConfigForgeRuntimeReaders)

configure_package_config_file(
    ${CMAKE_CURRENT_SOURCE_DIR}/cmake/ConfigForgeRuntimeReadersConfig.cmake.in
    ${CMAKE_CURRENT_BINARY_DIR}/ConfigForgeRuntimeReadersConfig.cmake
    INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/ConfigForgeRuntimeReaders)

write_basic_package_version_file(
    ${CMAKE_CURRENT_BINARY_DIR}/ConfigForgeRuntimeReadersConfigVersion.cmake
    VERSION 0.1.0
    COMPATIBILITY SameMajorVersion)

install(FILES
    ${CMAKE_CURRENT_BINARY_DIR}/ConfigForgeRuntimeReadersConfig.cmake
    ${CMAKE_CURRENT_BINARY_DIR}/ConfigForgeRuntimeReadersConfigVersion.cmake
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/ConfigForgeRuntimeReaders)

if(CONFIG_FORGE_RUNTIME_BUILD_TESTS)
    enable_testing()
    if(CONFIG_FORGE_RUNTIME_WITH_JSON_BUILTIN)
        add_executable(config_forge_runtime_json_builtin_smoke json_builtin/smoke_test.cpp)
        target_link_libraries(config_forge_runtime_json_builtin_smoke PRIVATE config_forge_runtime::json_builtin_test_fallback)
        add_test(NAME config_forge_runtime_json_builtin_smoke COMMAND config_forge_runtime_json_builtin_smoke)
    endif()
endif()
