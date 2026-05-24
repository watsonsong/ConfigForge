# C++ Runtime Reader 三方依赖 CMake 集成指南

## 目标

`runtime/cpp/` 是 ConfigForge 为 C++ 项目提供 runtime helper 的入口。未来每个 runtime data format 都可能有自己的 reader adapter，例如：

- `rapidjson_reader`：依赖 RapidJSON（header-only）；
- `msgpackc_reader`：依赖 msgpack-cxx（header-only C++ API，来自 msgpack/msgpack-c 的 cpp release）；
- `yaml_yaml_cpp`：可能依赖 yaml-cpp（通常需要编译和链接库）；
- `protobuf_*` / `flatbuffers_*`：通常依赖官方 C++ runtime（可能需要生成代码、编译库和链接库）。

这些 reader 不应把三方库 vendored 到 generated code，也不应让 generated code 直接感知具体三方库。正确边界是：

1. generated C++ code 只依赖 `IRuntimeDataReader` / `IRuntimeDataLoader` contract；
2. 每个 `runtime/cpp/<format_reader>/` adapter 自己处理三方库；
3. `runtime/cpp/CMakeLists.txt` 负责按 option 启用 adapter，并统一处理 `find_package`、本地 include/library path、FetchContent 自动下载、target 暴露和安装导出。

本文给未来继续开发 C++ reader 的 AI / 开发者一套可复用的 CMake 技术指导。

## 总原则

### 默认不联网

CMake configure 默认不应产生网络访问。自动下载三方库必须由用户显式开启：

```cmake
option(CONFIG_FORGE_RUNTIME_FETCH_DEPS "Allow FetchContent for missing third-party deps" OFF)
```

当依赖缺失且该 option 为 `OFF` 时，应给出明确错误，提示用户选择：

- 安装系统包 / vcpkg / Conan 包；
- 设置显式 include/library 路径；
- 或开启 `CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON`。

### generated code 不链接三方库

三方库只应该出现在 runtime adapter target 的 `INTERFACE` / `PUBLIC` / `PRIVATE` 依赖中。例如业务项目链接：

```cmake
target_link_libraries(game_config PRIVATE
    config_forge_runtime::rapidjson_reader)
```

而不是让 generated code target 直接链接 `RapidJSON::RapidJSON`、`yaml-cpp::yaml-cpp` 或 `msgpack-cxx`。

### 每个 adapter 都有独立 option

新增 reader 时应新增独立开关，命名模式：

```cmake
option(CONFIG_FORGE_RUNTIME_WITH_<FORMAT>_<BACKEND> "Enable <Backend> <Format> adapter" OFF)
```

示例：

```cmake
option(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER "Enable RapidJSON reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER "Enable msgpack-cxx MessagePack reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_YAML_YAML_CPP "Enable yaml-cpp YAML adapter" OFF)
```

### 依赖版本必须固定

使用 `FetchContent` 自动下载时必须固定版本：

- Git：优先固定 commit hash；短期可用稳定 tag；不要跟随 `master` / `main`。
- URL 压缩包：必须配置 `URL_HASH`。

### 支持上层项目覆盖

上层项目应该能用以下方式覆盖依赖来源：

- `find_package()`：使用 vcpkg / Conan / 系统安装的 CMake package；
- `*_INCLUDE_DIR` / `*_LIBRARY`：手动指定本地路径；
- `FETCHCONTENT_SOURCE_DIR_<UPPER_NAME>`：FetchContent 使用本地源码，不下载。

## 推荐依赖解析顺序

每个三方依赖都建议按以下顺序解析：

1. `find_package(<Dep> CONFIG QUIET)`；
2. 如果没有 CMake package target，尝试读取 package 设置的 include/library 变量；
3. `find_path()` / `find_library()` 搜索系统路径；
4. 如果仍找不到且 `CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON`，使用 `FetchContent` 下载；
5. 如果仍不可用，`message(FATAL_ERROR ...)` 给出修复方式。

## 依赖类型一：header-only 库

### 适用场景

适用于 RapidJSON、部分 single-header/single-include 依赖。这类库通常不需要编译目标，只需要把 include 目录添加到 adapter target。

关键点：

- 可以优先使用 package manager 提供的 CMake target；
- 如果没有 target，则使用 include directory；
- FetchContent 下载后，通常只需要 `${dep_SOURCE_DIR}/include`；
- 不要把下载到 build tree 的第三方 headers 当作会随 ConfigForge install 自动安装的内容。

### 模板

```cmake
find_package(RapidJSON CONFIG QUIET)

if(RapidJSON_FOUND AND NOT TARGET RapidJSON::RapidJSON)
    foreach(candidate
            RapidJSON_INCLUDE_DIRS
            RapidJSON_INCLUDE_DIR
            rapidjson_INCLUDE_DIRS
            rapidjson_INCLUDE_DIR)
        if(DEFINED ${candidate} AND NOT RAPIDJSON_INCLUDE_DIR)
            set(RAPIDJSON_INCLUDE_DIR "${${candidate}}")
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
        message(FATAL_ERROR
            "RapidJSON was not found. Install it, set RAPIDJSON_INCLUDE_DIR, "
            "or enable CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON.")
    endif()
endif()

add_library(config_forge_runtime_rapidjson_reader INTERFACE)
add_library(config_forge_runtime::rapidjson_reader ALIAS config_forge_runtime_rapidjson_reader)
target_include_directories(config_forge_runtime_rapidjson_reader INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/rapidjson_reader/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
target_compile_features(config_forge_runtime_rapidjson_reader INTERFACE cxx_std_17)

if(TARGET RapidJSON::RapidJSON)
    target_link_libraries(config_forge_runtime_rapidjson_reader INTERFACE RapidJSON::RapidJSON)
elseif(RAPIDJSON_INCLUDE_DIR)
    target_include_directories(config_forge_runtime_rapidjson_reader INTERFACE ${RAPIDJSON_INCLUDE_DIR})
endif()
```

## 依赖类型二：需要编译和链接的库

### 适用场景

适用于 yaml-cpp、protobuf、部分 msgpack 发行包、压缩库、数据库客户端等。这类库通常有 headers 和 library 两部分，可能还会引入 transitive dependencies。

关键点：

- 优先链接 package manager 提供的 imported target；
- 没有 imported target 时，才 fallback 到 `find_path()` + `find_library()`；
- FetchContent 下载源码后，通常需要 `FetchContent_MakeAvailable()` 把依赖加入当前构建，让它创建自己的 library target；
- 使用 FetchContent 前应关闭依赖的测试、示例、工具构建；
- adapter target 应链接依赖 target 或 library path，而不是只加 include path。

### 模板

```cmake
# 依赖项目的 option 要在 FetchContent_MakeAvailable 前设置。
# 具体变量名必须查该项目文档，以下是示例。
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(YAML_CPP_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(YAML_CPP_BUILD_TOOLS OFF CACHE BOOL "" FORCE)
set(YAML_CPP_BUILD_CONTRIB OFF CACHE BOOL "" FORCE)

find_package(yaml-cpp CONFIG QUIET)

if(NOT TARGET yaml-cpp::yaml-cpp AND NOT TARGET yaml-cpp)
    find_path(YAML_CPP_INCLUDE_DIR yaml-cpp/yaml.h)
    find_library(YAML_CPP_LIBRARY NAMES yaml-cpp libyaml-cpp)
endif()

if(NOT TARGET yaml-cpp::yaml-cpp
   AND NOT TARGET yaml-cpp
   AND (NOT YAML_CPP_INCLUDE_DIR OR NOT YAML_CPP_LIBRARY))
    if(CONFIG_FORGE_RUNTIME_FETCH_DEPS)
        include(FetchContent)
        FetchContent_Declare(
            yaml_cpp
            GIT_REPOSITORY https://github.com/jbeder/yaml-cpp.git
            GIT_TAG        <固定版本或 commit>
            GIT_SHALLOW    TRUE)
        FetchContent_MakeAvailable(yaml_cpp)
    else()
        message(FATAL_ERROR
            "yaml-cpp was not found. Install it, set YAML_CPP_INCLUDE_DIR/YAML_CPP_LIBRARY, "
            "or enable CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON.")
    endif()
endif()

add_library(config_forge_runtime_yaml_yaml_cpp INTERFACE)
add_library(config_forge_runtime::yaml_yaml_cpp ALIAS config_forge_runtime_yaml_yaml_cpp)
target_compile_features(config_forge_runtime_yaml_yaml_cpp INTERFACE cxx_std_17)

if(TARGET yaml-cpp::yaml-cpp)
    target_link_libraries(config_forge_runtime_yaml_yaml_cpp INTERFACE yaml-cpp::yaml-cpp)
elseif(TARGET yaml-cpp)
    target_link_libraries(config_forge_runtime_yaml_yaml_cpp INTERFACE yaml-cpp)
else()
    target_include_directories(config_forge_runtime_yaml_yaml_cpp INTERFACE ${YAML_CPP_INCLUDE_DIR})
    target_link_libraries(config_forge_runtime_yaml_yaml_cpp INTERFACE ${YAML_CPP_LIBRARY})
endif()
```

### 链接库依赖的额外规则

如果依赖需要编译和链接，新增 reader 时必须额外确认：

1. **target 名称**：不同包管理器可能导出不同 target，例如 `yaml-cpp::yaml-cpp` vs `yaml-cpp`。
2. **静态/动态库行为**：确认是否需要设置 `BUILD_SHARED_LIBS`，以及运行时是否需要动态库搜索路径。
3. **传递依赖**：优先链接 imported target，让依赖自己的 transitive link/interface include 自动传播。
4. **编译选项污染**：第三方 target 不应把全局 warning、标准版本、异常/RTTI 设置污染到整个项目。
5. **安装导出**：如果 ConfigForge 的 install package 暴露该 adapter target，安装后的 package config 必须能重新找到依赖。

## msgpack-cxx 指导

MessagePack C++ adapter 选择 `msgpack-cxx`，即 `msgpack/msgpack-c` 仓库的 C++ API。它提供 `<msgpack.hpp>`、`msgpack::unpack` 和 `msgpack::object_handle`，适合实现 header-only reader；自动下载时固定使用 `cpp-6.1.1` tag，不跟随主分支。不同包管理器导出的 CMake target 名称可能不同，`runtime/cpp/msgpackc_reader` 应同时兼容：

```cmake
find_package(msgpack-cxx CONFIG QUIET)
if(NOT msgpack-cxx_FOUND)
    find_package(msgpack CONFIG QUIET)
endif()

if(TARGET msgpack-cxx)
    target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE msgpack-cxx)
elseif(TARGET msgpackc-cxx)
    target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE msgpackc-cxx)
elseif(TARGET msgpack::msgpack)
    target_link_libraries(config_forge_runtime_msgpackc_reader INTERFACE msgpack::msgpack)
else()
    # fallback to find_path(MSGPACK_CXX_INCLUDE_DIR msgpack.hpp) or FetchContent.
endif()
```

如果使用 FetchContent，应在 `FetchContent_MakeAvailable(msgpack_cxx)` 前关闭不需要的上游选项：`MSGPACK_USE_BOOST=OFF`、`MSGPACK_BUILD_DOCS=OFF`、`MSGPACK_BUILD_EXAMPLES=OFF`、`MSGPACK_BUILD_TESTS=OFF`，并设置 `MSGPACK_CXX17=ON`。优先把 `${msgpack_cxx_SOURCE_DIR}/include` 作为 include path；如果所选 package 创建了 imported target，则优先链接 target，让传递依赖由 package 自己传播。

## FetchContent 使用规则

推荐：

```cmake
include(FetchContent)

FetchContent_Declare(
    dep_name
    GIT_REPOSITORY https://example.com/upstream/dep.git
    GIT_TAG        <固定 commit 或稳定 tag>
    GIT_SHALLOW    TRUE)

FetchContent_MakeAvailable(dep_name)
```

要点：

- `FetchContent` 在 configure 阶段获取依赖，适合当前 runtime adapter 立即拿 include path / target。
- 依赖名第一次声明生效，上层项目可以更早声明同名依赖以控制版本。
- 用户可通过 `FETCHCONTENT_SOURCE_DIR_<UPPER_NAME>` 指定本地源码目录。
- 不要在 `FIND_PACKAGE_ARGS` 后加 `REQUIRED`，否则找不到系统包时无法 fallback 到下载。
- 如果依赖没有合适的 CMakeLists 或不希望加入构建，可以只把源码目录作为 include/source 输入；必要时使用 `FetchContent_Populate` 或 CMake 新版本的 `SOURCE_SUBDIR`，但要注意项目最低 CMake 版本。

## ExternalProject 使用边界

`ExternalProject_Add` 通常在 build 阶段下载/构建外部项目。它适合“完整外部工程构建”，但不适合 ConfigForge runtime adapter 的默认依赖解析，原因：

- configure 阶段不一定能拿到真实源码文件；
- 当前 target 编译前必须额外建立下载依赖顺序；
- 对 header-only 或小型 C++ runtime library 来说过于复杂。

只有当依赖必须独立 configure/build/install，且不能通过 FetchContent 纳入当前构建时，才考虑 `ExternalProject_Add`。

## 安装与导出要求

新增 C++ reader adapter 时，除了 build tree 可用，还要考虑 install tree。

### 安装 adapter 自身 headers

```cmake
install(DIRECTORY rapidjson_reader/include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
```

### adapter target 加入 export

```cmake
list(APPEND config_forge_runtime_install_targets config_forge_runtime_rapidjson_reader)
install(TARGETS ${config_forge_runtime_install_targets}
    EXPORT ConfigForgeRuntimeTargets)
```

### 安装后的依赖查找

如果依赖是外部 package，安装后的 `ConfigForgeRuntimeConfig.cmake` 可能需要 `find_dependency()`。例如 compiled dependency：

```cmake
include(CMakeFindDependencyMacro)
find_dependency(yaml-cpp CONFIG)
```

Header-only 且通过 include path 使用的依赖，如果没有随 ConfigForge 安装，则需要在 package config 中提示用户继续提供该依赖。不要假设 FetchContent 下载的依赖会被自动安装。

对需要编译和链接的依赖，如果 install package 暴露 adapter target，必须二选一：

1. 要么依赖也由用户环境提供，并在 config 中 `find_dependency()`；
2. 要么明确声明该 adapter 不支持 install package 自动重定位，只支持 build-tree / add_subdirectory 使用。

## 新增 reader 的 checklist

1. 目录命名：优先使用 `runtime/cpp/<third_party_library>_reader/`，例如 `rapidjson_reader`、`msgpackc_reader`；如果三方库名称本身缺少明确文件格式，再使用 `<third_party_library>_<format>_reader`。不要使用单纯 `<format>_reader`，因为同一种语言下同一格式可能有多个可选三方库，目标用户也可能已经在项目中使用另一套 JSON / MessagePack 库。
2. 提供 headers / sources，并只实现 generated reader contract，不侵入 generated code。
3. 在 `runtime/cpp/CMakeLists.txt` 增加独立 option。
4. 按“find_package → find_path/find_library → FetchContent → fatal error”顺序解析依赖。
5. 判断依赖类型：header-only 只传播 include path；compiled library 必须链接 target/library。
6. 固定 FetchContent 版本，不跟随主分支。
7. 创建与目录一致的 `config_forge_runtime::<reader_name>` alias target。
8. 将三方依赖只挂到 adapter target，不挂到 generated code。
9. 补 smoke test / integration test；需要三方库时用 CMake configure 路径触发自动下载。
10. 更新 `Documentation/Design/06-export-cli.md` 和 `Documentation/Implementation/09-codegen-module-detail-design.md` 的支持矩阵。
11. 如涉及安装，更新 `ConfigForgeRuntimeConfig.cmake.in` 的 `find_dependency()` 或依赖说明。

## 当前项目的落地示例

当前 `runtime/cpp/CMakeLists.txt` 中的 `CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER` 是本指南的第一个落地例子。它展示了：

- adapter 独立 option；
- 优先使用系统/包管理器提供的 RapidJSON；
- RapidJSON 是 header-only，因此缺失时在 `CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON` 下用 FetchContent 下载并传播 include path；
- 对外只暴露 `config_forge_runtime::rapidjson_reader`。

未来新增 C++ reader 时，应复制这套结构，而不是把三方库下载逻辑写进测试脚本或 generated code。若新依赖需要编译和链接库，应使用本文的 compiled library 模板，并额外处理 target 名称、传递依赖和安装后的 `find_dependency()`。

`CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER` 是第二个落地例子。它选择 msgpack-cxx 是因为：

- C++ API 可直接把 MessagePack bytes 解码为 `msgpack::object_handle`，reader cursor 可以共享 owner，满足 generated reader contract 的生命周期要求；
- 通过 `<msgpack.hpp>` 暴露 map / array / string / integer / float / bool / nil 类型，能完整覆盖当前 ConfigForge payload shape；
- 常见包管理器支持 `find_package(msgpack-cxx CONFIG)` 或 `find_package(msgpack CONFIG)`，缺失时可显式 FetchContent 固定 `cpp-6.1.1`。
