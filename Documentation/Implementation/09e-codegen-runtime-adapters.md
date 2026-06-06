# 09e. Codegen Runtime Adapters

本文从 Codegen 模块详细设计中拆出，专门描述顶层 `runtime/` 标准 adapters、reader contract compliance、C++ CMake、TypeScript packages、Rust templates 与测试矩阵。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 总览、IR、命名、冲突检测和测试总览。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、runtime loading 和 generated API 语义。
- [`13-runtime-binary-format-detail-design.md`](13-runtime-binary-format-detail-design.md) — `bin` runtime data schema revision 与 positional reader contract。

## 当前状态

`runtime/` 下标准 adapters 是 generated code 的辅助接入层，不是每次 codegen 的必然输出。核心 generated code 只依赖 loader/reader contract；标准 adapters 提供可复用的 JSON / MessagePack / `bin` reader 和常用 transport loader。当前 C++ runtime helper 已有可通过 CMake 引入的 interface targets；Rust 与 TypeScript 仍以 copyable template / workspace package template 为主，尚不是发布稳定 crate / npm package。C++ 三方依赖与 CMake 策略以 [`../Tech/cpp-runtime-dependencies.md`](../Tech/cpp-runtime-dependencies.md) 为准。

## 标准 runtime helper 工程结构

生成代码只声明 reader / loader contract，这解决了 Meta-specific 逻辑与业务资源系统耦合的问题，但实际游戏项目仍需要可运行的 JSON / MessagePack 等格式 reader。项目应提供标准 RuntimeDataReader adapter 源码/模板，作为 generated code 之外的 runtime support 层。

### 分层边界

标准 adapter 分三层：

1. **Contract glue**：把某语言的 concrete value/cursor 类型适配成 generated code 期望的 `RuntimeDataReader` / `IRuntimeDataReader` / `RuntimeDataReader` 形状。
2. **Format reader**：把 JSON、MessagePack 等具体格式解码为 value/cursor，并实现 field/index/len/scalar/null 语义。
3. **Transport loader**：从文件系统、HTTP、bundle、引擎资源系统等位置读取 bytes/text，再交给 format reader。Transport 可选，业务项目可以只复用 format reader 并接入自己的资源系统。

核心 codegen 不 import adapter，也不把 adapter 的第三方依赖写进 generated code。adapter 是可选 runtime support：可以被业务项目直接依赖，也可以由 codegen option 拷贝到目标目录，还可以作为测试 harness 的基础实现。

### Reader contract 可执行规范

标准 adapter 不能只对齐接口形状，还必须对齐 reader contract 版本、value model、missing/null、scalar conversion 和错误上下文。当前 contract 版本定义为 `1.0`；后续所有 generated code 与 adapter 都应声明支持的 contract version。

建议 generated code 输出常量：

```rust
pub const CONFIG_FORGE_READER_CONTRACT_VERSION: &str = "1.0";
```

```ts
export const CONFIG_FORGE_READER_CONTRACT_VERSION = "1.0" as const;
```

```cpp
inline constexpr const char* CONFIG_FORGE_READER_CONTRACT_VERSION = "1.0";
```

adapter 也声明支持范围，例如 `SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0"`。当 codegen `emit = "copy"` 时，应把 adapter 支持范围写入复制文件头或 adapter manifest；当 `emit = "reference"` 时，应在生成 manifest / 诊断中说明所需 adapter 包或 CMake target 的 contract range。

Contract version bump 规则：

| 变化类型 | 版本级别 | 示例 |
| --- | --- | --- |
| reader / loader method signature 变更 | major | `field` 返回类型变化、loader open 参数变化。 |
| scalar conversion 或 missing/null 破坏性语义变更 | major | bool 不再接受 `"1"` / `"0"`。 |
| 新增可选方法或非破坏性能力 | minor | 新增 path context helper，但旧 adapter 仍可工作。 |
| 错误信息更清晰、内部性能优化 | patch | 增加 source path 到错误 message。 |

Contract 层定义的是 **ConfigForge Runtime Value**，不是 JSON Value。当前 Rust implementation 可以短期复用 `serde_json::Value` 降低实现成本，但 contract 应按下列 value model 描述：

| Runtime Value | JSON | MessagePack | Contract 语义 |
| --- | --- | --- | --- |
| object/map | object | map with string keys | `field(name)` 按字段名查找；非 string map key 暂不支持，应报错。 |
| array | array | array | `index(i)` 按 0-based 下标读取；越界报错。 |
| int / uint | number 或 numeric string | signed/unsigned integer 或 numeric string | generated parser 根据字段类型读取并检查范围；超出目标类型报错。 |
| float | finite number 或 numeric string | finite float / integer 或 numeric string | 非 finite 值必须报错。 |
| bool | bool 或 string | bool 或 string | string 仅接受大小写敏感的 `"true"` / `"false"` / `"1"` / `"0"`。 |
| string | string | string | 不做 number-to-string / bool-to-string 自动转换。 |
| null | null | nil | explicit null 在字段入口处按 missing 处理；scalar conversion 不接受 null。 |
| binary/ext | 不适用 | bin/ext | 当前 contract 不支持，adapter 必须明确报错，不能隐式转 string。 |

统一 scalar conversion policy：

| 读取路径 | 接受输入 | 禁止输入 | 额外要求 |
| --- | --- | --- | --- |
| int | integer value、可解析为整数的 string | float 小数、bool、object/array/null、溢出值 | 错误包含 path、expected `int`、actual type/value。 |
| float | finite number、可解析为 finite number 的 string | NaN、Infinity、bool、object/array/null | 错误包含 path、expected `float`、actual type/value。 |
| bool | bool、`"true"`、`"false"`、`"1"`、`"0"` | 大小写变体、number 1/0、其他 string、object/array/null | 保持与 ConfigForge validation/export 当前 bool string 语义一致。 |
| string | string | number/bool/object/array/null | 不自动 stringify。 |

missing/null 语义：

- `field(name)` 找不到字段或字段值为 null/nil 表示 missing：Rust/TS 返回 `None` / `undefined`，C++ 返回 `nullptr`。
- explicit null 不再依赖独立 reader null-check API；由 `EnterOptionalField` / `enter_optional_field` / `enterOptionalField` 的 bool 返回值统一表达 optional presence。
- `index(i)` 越界必须报错，不返回 null reader。
- `length()` / `len()` 对 payload array 返回元素数；对 object/map 返回字段数；对 scalar/null 报错。C++ adapter 不再暴露通用 `Size()`，generated collection 读取使用 `ReadRowCount()`；generated list 读取使用 `ReadListLength()` / `readListLength()` / `read_list_len()`。schema-aware binary reader 额外提供 list length / positional list element 读取能力，使生成代码能在 `bin` 中读取 length prefix 后顺序读取元素；named JSON / MessagePack reader 可在 list length API 内部复用 `length + index(i)`。

错误上下文要求：adapter 抛出的错误应包含 logical data path、reader path（field/index 链）、expected type、actual type/value。transport loader 还应包含 resolved resource path 或 URL。

### 工程化目标与当前缺口

`runtime/` 的目标不是只服务 standard_project harness，而是成为游戏项目可直接引用的 runtime support 层。当前仓库已经有首批源码：

| 当前目录 | 定位 | 当前状态 |
| --- | --- | --- |
| `runtime/rust/serde_json_reader` | Rust JSON reader module | 已有基于 `serde_json` 的 `JsonValueReader` 与 fs loader。 |
| `runtime/rust/serde_msgpack_reader` | Rust MessagePack reader module | 已有基于 `rmp-serde` 的 `MessagePackValueReader` 与 fs loader，并被 Rust integration harness 使用。 |
| `runtime/typescript/json_reader` | TypeScript JSON reader + Node fs loader | 已有 JSON reader，使用结构化类型，不依赖 generated.ts。 |
| `runtime/typescript/messagepack_reader` | TypeScript MessagePack value reader | 已有基于 `@msgpack/msgpack` peer dependency 的 reader，复用 shared TypeScript core。 |
| `runtime/cpp/rapidjson_reader` | C++ RapidJSON reader adapter | 已有 header-only RapidJSON adapter，可用于测试与生产接入。 |
| `runtime/cpp/msgpackc_reader` | C++ msgpack-cxx reader adapter | 已有 header-only msgpack-cxx adapter，可用于 MessagePack runtime data 接入。 |
| C# JSON harness reader | C# JSON reader smoke harness | 当前位于 standard_project 集成测试中，临时 `.csproj` 使用 `System.Text.Json` 实现 generated `IConfigForgeRuntimeDataReader` / `IConfigForgeRuntimeDataLoader`；尚未沉淀为 `runtime/csharp/` 标准 adapter。 |
| Java JSON harness reader | Java JSON reader smoke harness | 当前位于 standard_project 集成测试中，临时 `RuntimeCheck.java` 使用 JDK-only 简化 JSON reader 实现 generated `ConfigForgeRuntimeDataReader` / `ConfigForgeRuntimeDataLoader`；尚未沉淀为 `runtime/java/` 标准 adapter。 |

主要缺口：

1. 各语言 adapter 缺少统一 compatibility / test matrix 描述，无法明确哪些 adapter 是生产推荐、哪些只是 custom/example。
2. C++ stable interface header 仍待补齐，当前 C++ adapter 仍通过宏绑定 generated header 中的 `IRuntimeDataReader` / `IRuntimeDataLoader`。
3. C# / Java 当前只有集成测试 harness reader，尚未提供可复用 `runtime/csharp/*` / `runtime/java/*` 标准 adapter。

因此 runtime loader 工程化按“两层稳定 + 多 adapter 可选”的方向推进：

- **稳定契约层**：定义 reader/loader contract 版本、payload shape、错误语义、scalar 兼容策略。生成代码只依赖该契约，不依赖具体 adapter。
- **可选 adapter 层**：每个语言/格式组合是独立工程单元，明确三方依赖、构建方式、测试入口和生产推荐等级。

### 目录布局

推荐在仓库顶层维护 `runtime/` 作为标准 adapter 源码/模板目录：

```text
runtime/
  compliance/
    payloads/             # 跨语言共享 payload cases
    expected/             # scalar / error case expectations
  rust/
    serde_json_reader/
      src/runtime_reader.rs # JsonValueReader、JSON fs loader、generated trait glue
      Cargo.toml          # features: fs
    serde_msgpack_reader/
      src/runtime_reader.rs # MessagePackValueReader、MessagePack fs loader、generated trait glue
      Cargo.toml          # features: fs
  typescript/
    reader_core/
      src/                # shared reader helpers、error helpers
      package.json
    json_reader/
      src/                # JSON.parse value reader
      package.json
    messagepack_reader/
      src/                # @msgpack/msgpack based value reader
      package.json
    loaders/
      node-fs/            # Node fs transport loader
      fetch/              # browser/fetch transport loader
  cpp/
    rapidjson_reader/
      include/config_forge/runtime/rapidjson_reader/ # RapidJSON-backed header-only reader adapter
      CMakeLists.txt
    msgpackc_reader/
      include/config_forge/runtime/msgpackc_reader/ # msgpack-cxx-backed header-only reader adapter
      CMakeLists.txt
    loaders/
      filesystem/         # filesystem transport loader example
      CMakeLists.txt
    cmake/
      ConfigForgeRuntimeConfig.cmake.in
    CMakeLists.txt        # options、find_package、adapter targets、tests
  csharp/
    json_reader/          # 未来：System.Text.Json reader + fs loader
    messagepack_reader/   # 未来：MessagePack reader adapter
  java/
    json_reader/          # 未来：JDK/Jackson/Gson 可选 JSON reader + fs loader
    messagepack_reader/   # 未来：MessagePack reader adapter
```

`runtime/` 不是每次 export 的输出目录；它是 ConfigForge 源码仓库中维护的标准 runtime adapter 集合。未来如增加 `yaml`、`sqlite`、自定义二进制等 export type，应按同样结构补充对应语言子目录。

### 官方支持矩阵、推荐等级与依赖策略

对 stable runtime data export type，应覆盖官方 target language；对 experimental type 可以先只支持部分语言。Transport loader 不进入语言 × 格式矩阵，而是与 format reader 组合。每个 adapter 单元必须明确：

- 支持的 generated reader contract 版本；
- 支持的 data payload shape（当前至少 `{ meta, rows }` collection envelope 与 object table root object）；
- 依赖的三方库与启用方式；
- 是否提供 transport loader；
- 是否用于 CI / integration harness；
- 支持等级：`stable`、`preview`、`custom/example`；
- 形态：`standalone_package`、`copyable_template`、`generated_header_bound`。

首批矩阵：

| 语言 | 格式 | adapter 单元 | 支持等级 | 形态 | 依赖与启用方式 | 备注 |
| --- | --- | --- | --- | --- | --- | --- |
| Rust | JSON | `runtime/rust/serde_json_reader` | `stable` | `copyable_template` | `serde_json`；Cargo feature `json` / `fs` | 当前 trait 由 generated crate 声明，短期是生产可用模块；`JsonValueReader` 留在 JSON reader 内，stable runtime crate 落地后再升级为 standalone package。 |
| Rust | MessagePack | `runtime/rust/serde_msgpack_reader` | `stable` | `copyable_template` | `rmp-serde` + `serde_json::Value` 作为中间 value；Cargo feature `messagepack` / `fs` | `MessagePackValueReader` 留在 MessagePack reader 内，避免测试 harness 自定义 reader；中期可收敛到真正的 ConfigForge Runtime Value。 |
| TypeScript | JSON | `runtime/typescript/json_reader` | `stable template` | `copyable_package_template` | 原生 `JSON.parse`；workspace/package 模板 | 不依赖 generated.ts，Node fs/fetch loader 分离；尚未作为稳定 npm 包发布。 |
| TypeScript | MessagePack | `runtime/typescript/messagepack_reader` | `preview template` | `copyable_package_template` | `@msgpack/msgpack` 作为 peer dependency；workspace/package 模板 | 解码后的 unknown value 复用 core reader 语义；尚未作为稳定 npm 包发布。 |
| C++ | JSON | `runtime/cpp/rapidjson_reader` | `stable` | `generated_header_bound`，中期 `external_interface` | CMake option + `find_package(RapidJSON CONFIG)` / package manager / optional `FetchContent` | 用 RapidJSON 完整处理转义、Unicode、数字和 parse error；可通过 CMake target 引入。 |
| C++ | MessagePack | `runtime/cpp/msgpackc_reader` | `stable` | `generated_header_bound`，中期 `external_interface` | CMake option + `find_package(msgpack-cxx CONFIG)` / package manager / `FetchContent` 固定 `cpp-6.1.1` | 不手写二进制 parser；解码到 msgpack object/value 后适配 reader，可通过 CMake target 引入。 |
| C# | JSON | standard_project 临时 harness | `custom/example` | `test_harness` | .NET BCL `System.Text.Json` | 已用于 `dotnet run` 回归验证 generated C#；生产可复用 adapter 仍待抽出到 `runtime/csharp/`。 |
| Java | JSON | standard_project 临时 harness | `custom/example` | `test_harness` | JDK-only 简化 JSON reader | 已用于 `javac` / `java` 回归验证 generated Java；生产可复用 adapter 仍待抽出到 `runtime/java/`，可选择 JDK-only、Jackson、Gson 或 Android 友好实现。 |

依赖策略：

- Rust 使用 Cargo feature 控制，如 `json`、`messagepack`、`fs`，默认 feature 尽量为空或只包含 core。
- TypeScript 使用拆包和 peer dependency：`@config-forge/runtime-reader-json` 不依赖 MessagePack；`@config-forge/runtime-reader-messagepack` peer-depend on `@msgpack/msgpack`，避免只用 JSON 的项目被迫安装 MessagePack 包。
- C++ 使用 CMake options / package manager `find_package` 控制第三方库启用，不 vendored 到 generated code。允许在 examples/CI 中用 `FetchContent` 或包管理器（vcpkg、Conan、系统包）下载三方库，但必须是显式 opt-in。
- adapter 版本应与 generated reader contract 版本兼容；当 reader interface 发生 breaking change，adapter 需要同步升级并在包名、feature、manifest 或文档中标明兼容范围。

### C++ CMake 工程化方案

C++ runtime helpers 采用 CMake target 管理，目标是让游戏项目能通过 `add_subdirectory(runtime/cpp)` 或安装后的 `find_package(ConfigForgeRuntime CONFIG)` 引入 adapter。

#### CMake options

```cmake
option(CONFIG_FORGE_RUNTIME_BUILD_TESTS "Build ConfigForge runtime reader tests" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER "Enable RapidJSON reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER "Enable msgpack-cxx MessagePack reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_FETCH_DEPS "Allow FetchContent for missing third-party deps" OFF)
```

普通业务构建默认不启用 `rapidjson_reader`，避免最容易链接的 target 被误用到生产。测试构建可以在 `CONFIG_FORGE_RUNTIME_BUILD_TESTS=ON` 时显式打开 `CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER=ON`，用于 RapidJSON reader smoke test。生产项目推荐显式启用 RapidJSON 或 MessagePack adapter：

```cmake
add_subdirectory(path/to/ConfigForge/runtime/cpp)

target_link_libraries(game_config_runtime PRIVATE
  config_forge_runtime::rapidjson_reader
  config_forge_runtime::filesystem_loader
)

target_include_directories(game_config_runtime PRIVATE
  ${CMAKE_CURRENT_BINARY_DIR}/Generated/Cpp
)
```

#### CMake targets

| target | 类型 | 说明 |
| --- | --- | --- |
| `config_forge_runtime::binary_reader` | interface | `bin` schema-aware reader 基础 target。 |
| `config_forge_runtime::rapidjson_reader` | interface | RapidJSON based reader，生产推荐 JSON adapter；启用 `CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER` 后提供。 |
| `config_forge_runtime::msgpackc_reader` | interface | msgpack-cxx based reader，生产推荐 MessagePack adapter；启用 `CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER` 后提供。 |

当前 C++ adapter 不提供单独的 `config_forge_runtime::core` target。短期以 generated header 中的 `IRuntimeDataReader` / `IRuntimeDataLoader` 作为唯一 C++ contract，adapter 通过宏绑定到该 generated interface，避免同时存在 generated interface 与 runtime core interface 两套 nominal virtual 类型。后续如拆出稳定 `config_forge_runtime.hpp`，再重新引入独立 stable interface target。

#### 三方依赖解析

- `rapidjson_reader` 优先 `find_package(RapidJSON CONFIG QUIET)`；找不到时，如果 `CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON`，可用 `FetchContent` 拉取 RapidJSON；否则给出明确 CMake error，提示通过 vcpkg/Conan/system package 安装。
- `msgpackc_reader` 优先 `find_package(msgpack-cxx CONFIG QUIET)` 或项目约定的 `find_package(msgpack CONFIG QUIET)`；找不到时，如果 `CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON`，可用 `FetchContent` 拉取 `msgpack/msgpack-c` 的 `cpp-6.1.1` tag；否则给出明确 CMake error。
- 三方库只链接到 adapter target，不出现在 generated code target 的 public API 中。业务可替换为引擎内置 JSON / MessagePack 实现，只要实现同一 `IRuntimeDataReader` contract。

#### generated header / stable interface 绑定

短期保持当前模式：C++ adapter include `generated.hpp`，并通过宏指定 generated interface 类型：

```cpp
#define CONFIG_FORGE_RUNTIME_CPP_READER my::game::config::IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER my::game::config::IRuntimeDataLoader
#include "config_forge/runtime/rapidjson_reader/runtime_loader.hpp"
```

Phase 1.5 优先评估拆出稳定 `config_forge_runtime.hpp`：generated C++ 提供 `interface = "inline" | "external"` 选项。默认 `inline` 保持当前 generated header 自带 interface；production adapter 推荐 `external`，由 generated code 和 adapter 同时 include 稳定 header。这样 adapter library 不再依赖某次 generated header，CMake target 可以更像普通第三方库被复用。

#### C++ reader 语义要求

生产级 C++ adapter 必须覆盖：

- JSON string escape / Unicode / number parse error 的完整库语义，不手写简化 parser；
- MessagePack map / array / integer / float / bool / string / nil；
- `field` 将 missing 与 explicit null 都映射为字段不存在，由 `EnterOptionalField` 的 bool 返回值表达 optional presence；
- reader cursor 必须 owning 或共享持有 root document/value，不能返回指向临时 DOM 节点的悬垂指针；RapidJSON cursor 通常应通过 `std::shared_ptr<Document>` 或等价 owner 维持 root 生命周期；
- field / index cursor 都必须满足同样 ownership 规则；
- `Index` 越界应抛出 `std::runtime_error` 或 ConfigForge runtime 定义的派生异常；C++ `ReadRowCount()` 对 array 返回元素数，对非 array 抛错；
- bool string 兼容 `"true"` / `"false"` / `"1"` / `"0"`，与 ConfigForge validation/export 语义保持一致；
- float 必须有限，非 finite 值报错；
- 错误信息包含 path / field / index 上下文，便于定位运行时数据问题。

#### filesystem loader 安全边界

标准 filesystem loader 只处理 logical data file path 到本地文件的安全映射：

- 禁止 absolute path；
- normalize / lexical clean 后路径必须仍在 data root 内，禁止 `..` path traversal；
- 错误信息同时包含 logical path 和 resolved path；
- 不对大小写敏感文件系统差异做隐式兼容；
- 不负责格式选择，格式 decode 由注入的 JSON / MessagePack decoder 或 adapter target 决定。

### TypeScript MessagePack 工程化方案

TypeScript adapter 使用结构化类型，不需要 import generated module。包结构推荐：

```text
runtime/typescript/
  core/                 # RuntimeDataReaderLike、ValueReader、shared scalar conversion
  json/                 # JsonValueReader、decodeJsonValue
  messagepack/          # MessagePackValueReader、decodeMessagePackValue
  loaders/node-fs/      # FsRuntimeDataLoader，可注入 decoder
  loaders/fetch/        # FetchRuntimeDataLoader，可注入 decoder
```

推荐 npm 包边界：

| package | 依赖 | 导出 |
| --- | --- | --- |
| `@config-forge/runtime-reader-core` | 无 | `RuntimeDataReaderLike`、shared `ValueReader`、scalar conversion helpers。 |
| `@config-forge/runtime-reader-json` | `core` | `JsonValueReader`、`decodeJson`。 |
| `@config-forge/runtime-reader-messagepack` | `core` + peer `@msgpack/msgpack` | `MessagePackValueReader`、`decodeMessagePack`。 |
| `@config-forge/runtime-loader-node-fs` | `core` | Node fs loader；decoder 由 JSON/MessagePack 包注入。 |

Loader / decoder 边界允许同步或异步，保持与 generated `RuntimeDataLoader.open` 的 `Promise<RuntimeDataReader> | RuntimeDataReader` 契约一致：

```ts
export type RuntimeReaderDecoder =
  (bytes: Uint8Array, path: string) => RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
```

`FetchRuntimeDataLoader` 必然 async；Node fs 推荐提供 async `FsRuntimeDataLoader`，同时可以提供测试用 `SyncFsRuntimeDataLoader`，但两者都应通过同一个 decoder 类型组合 JSON / MessagePack reader。

示例：

```ts
import { decode } from "@msgpack/msgpack";
import { MessagePackValueReader } from "@config-forge/runtime-reader-messagepack";
import { FsRuntimeDataLoader } from "@config-forge/runtime-loader-node-fs";
import { GameData } from "./generated";

const loader = new FsRuntimeDataLoader(dataDir, bytes => new MessagePackValueReader(decode(bytes)));
const data = await GameData.load(loader);
```

MessagePack adapter 的 value 语义应与 JSON adapter 对齐：object/map 支持 `field(name)`，array 支持 `index/length`，scalar 支持 string-to-number / string-to-bool 兼容策略，`null/undefined` 在字段入口处按 missing 处理。

### 各语言 contract 约束

不同语言的 reader interface 绑定方式不同，adapter 形态也不同：

- **Rust**：当前 trait 由 generated crate 声明，外部 crate 无法不依赖用户 generated crate 就直接实现该 trait。短期方案是提供可复制 reader module，例如 `runtime/rust/serde_json_reader/src/runtime_reader.rs`，由 codegen option 或用户复制进 generated crate/业务 crate 后实现本地 trait；中期方案是拆出稳定 `config_forge_runtime` support crate，让 generated code 可配置为 `use config_forge_runtime::{RuntimeDataReader, RuntimeDataLoader}`，adapter crate 再直接实现这些 trait。
- **TypeScript**：接口是结构化类型，adapter package 只要导出同形状 class 即可直接传给 `GameData.load`。因此 TypeScript adapter 可以最早做成标准 npm workspace 包。
- **C++**：`IRuntimeDataReader` 是 nominal virtual interface，adapter 需要看到 generated header 或稳定 runtime interface header。短期方案是 adapter header/source 通过 include path 引用 generated header；中期方案是拆出稳定 `config_forge_runtime.hpp`，generated code 和 adapter 都依赖该 header，降低每次生成代码对 adapter 编译的耦合。

### 与 codegen / 测试集成

Codegen 后续可增加 runtime adapter 相关选项，但默认仍不强制输出 adapter：

```toml
[targets.exports.runtime_reader]
adapter = "rust/serde_msgpack_reader"   # 或 "typescript/messagepack_reader", "cpp/rapidjson_reader", "cpp/msgpackc_reader"
emit = "copy"                      # copy / reference / none
loader = "fs"                      # fs / fetch / none / custom
```

配置校验规则：

- `adapter` 的 language 必须与 code target language 匹配，例如 TypeScript target 不能选择 `cpp/rapidjson_reader`。
- `adapter` 的 format 不与 codegen export task 绑定；同一份生成代码可以配合不同格式 adapter 使用。若未来显式配置 adapter，校验只需确认该 adapter 支持目标语言和 reader contract。
- `loader` 必须被所选语言和 adapter 支持，例如 `fetch` 只对提供 fetch transport 的 TypeScript/browser adapter 合法。
- `emit = "reference"` 时，必须能解析到对应 package/crate/CMake target；无法解析时应在 config 阶段报错或生成明确 diagnostic，不应延迟到编译/运行时。
- `emit = "copy"` 时，应记录 adapter source version 和 supported contract range；默认不覆盖用户已修改文件，除非显式配置 `overwrite = true` 或输出到全新 generated adapter 目录。

`emit` 语义：

| emit | 行为 | 适用场景 |
| --- | --- | --- |
| `none` | 只生成核心 generated code；不输出 adapter 文件、不输出依赖接入提示。 | 用户完全自研 loader，或引擎已有资源系统。 |
| `copy` | 复制 adapter source/template 到目标输出目录；同时复制必要 package/Cargo/CMake 片段或 manifest。复制内容必须带 adapter version / contract range 注释。 | 快速接入、示例工程、没有包管理体系的游戏项目。 |
| `reference` | 不复制源码；生成依赖接入提示或 manifest，例如 Rust path/crate dependency、TypeScript npm package、C++ CMake target。 | 正式项目通过包管理或 submodule 引入 runtime support。 |

`copy` 模式应避免覆盖用户手改：adapter 输出目录建议独立于 generated core code，例如 `Generated/Runtime/<adapter>/`；若目标文件已存在且 adapter version 不一致，默认报 diagnostic，提示用户清理或显式允许覆盖。

建议推进顺序：

1. 将 C++ JSON adapter 收敛为 `rapidjson_reader`，移除无依赖 builtin fallback，测试与生产都通过 RapidJSON 路径验证。
2. 将 Rust / TypeScript templates 整理为清晰的 copyable crate/package template，并补齐版本与 contract range 标注。
3. 评估 Phase 1.5 C++ stable interface header，让 C++ adapter 从 generated header bound 迁移到 external interface。
4. 集成测试 harness 改为引用或复制这些 adapter，避免 harness 内长期维护独立 reader 实现。
5. 增加 adapter 自测：每个 adapter 至少用同一份 canonical payload 验证 field/index/len/scalar/null、missing vs null、string-to-number/bool、finite float、escaped string。
6. 再考虑是否提供 codegen option 自动复制 adapter 到目标工程输出目录。

### Adapter compliance suite 与 CI / 测试矩阵

canonical payload 应升级为正式 compliance suite，而不是只停留在矩阵描述。推荐目录：

```text
runtime/compliance/
  payloads/
    collection_envelope.json
    object_table.json
    scalar_compat.json
    missing_vs_null.json
    nested_struct.json
    fragment.json
    escaped_string.json
  expected/
    scalar_results.toml
    error_cases.toml
```

Rust / TypeScript / C++ adapter 都必须跑同一套 compliance cases。覆盖范围包括 missing vs null、bool `true/false/1/0`、numeric string、NaN/Infinity、int overflow、escaped string、array 越界、MessagePack 非 string map key、binary/ext unsupported 等场景。MessagePack cases 可以由同一份 logical fixture 生成 `.msgpack` payload，避免 JSON 与 MessagePack fixture 漂移。

标准 runtime helpers 的 CI 至少覆盖：

| 层级 | Rust | TypeScript | C++ |
| --- | --- | --- | --- |
| adapter unit | `cargo test --manifest-path runtime/rust/<adapter>/Cargo.toml` | `npm test` / `tsc -p` per package | `cmake -S runtime/cpp -B build -DCONFIG_FORGE_RUNTIME_BUILD_TESTS=ON ... && cmake --build build && ctest` |
| format matrix | JSON + MessagePack | JSON + MessagePack | RapidJSON JSON smoke、RapidJSON JSON、msgpack-c MessagePack |
| canonical payload | collection envelope、object table、nested struct、fragment object、missing/null、bool `1/0`、finite float、escaped string | 同左 | 同左 |
| integration harness | standard_project Rust MessagePack + JSON compile check | standard_project TypeScript JSON，后续增加 MessagePack fixture | standard_project C++ RapidJSON JSON smoke；后续增加 RapidJSON JSON 与 MessagePack compile/run |

CI 中三方依赖策略：

- 默认快速 CI 只跑无三方或已缓存依赖：Rust、TS JSON、C++ builtin。
- 完整 CI / nightly 跑 production adapter：TS MessagePack、C++ RapidJSON、C++ msgpack-c。
- 如果三方包不可用，测试应明确 skip 或 fail-fast 给出安装提示，不能静默退回 builtin parser 造成覆盖误判。

## Reader loader 落地状态

迁移已完成：

1. Rust / C++ / TypeScript / C# 均生成 `RuntimeDataReader` trait/interface 和通用 `RuntimeDataLoader.open` / `IRuntimeDataLoader::Open` / `IConfigForgeRuntimeDataLoader.Open`。
2. table row、object table、named struct、fragment 均由生成代码输出 reader parser helper。
3. table/object `load` 已从 `loader.load_<table>_rows(DATA_FILE)` 切换为 `loader.open(DATA_FILE)`，由生成代码读取 `rows` 或 object payload。
4. codegen 单测和 standard_project harness 已改为通用 reader loader，不再按表手写 typed rows / objects。

当前验收标准：生成代码不再出现按表的 loader 方法；新增表、字段、fragment 或 object table 字段时，用户侧 loader / reader 接口和 harness loader 实现不需要改动，只需要重新生成代码并更新业务断言。
