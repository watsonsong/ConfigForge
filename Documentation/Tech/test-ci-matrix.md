# Test and CI Matrix

本文定义 ConfigForge 本地测试、quick CI、full CI / nightly 的工具链要求、skip / fail-fast 策略，以及 generated-code runtime harness 与 runtime adapter compliance runner 的覆盖矩阵。详细环境安装见 [`development-environment.md`](development-environment.md)。

## Test profiles

| Profile | 目标 | 必需工具 | 可选工具缺失策略 | 默认联网 |
| --- | --- | --- | --- | --- |
| `local` | 普通 Rust 开发的默认路径，快速发现核心回归。 | Rust 1.85+、Cargo、rustfmt。 | 目标语言工具缺失时明确 skip。 | 关闭。 |
| `quick` | PR 基础门禁，覆盖 Rust core、关键 integration 与不需要额外三方安装的 runner。 | Rust toolchain；CI 镜像可额外安装 Node / C++ / .NET / JDK。 | 未声明必需的工具仍明确 skip。 | 关闭。 |
| `full` | 跨语言 generated-code harness 与 runtime adapter 主要回归。 | Rust、C++17/CMake、Node/TypeScript、.NET SDK、JDK、Lua；按启用项安装 RapidJSON / msgpack-cxx。 | 声明必需的工具缺失时 fail-fast。 | 关闭，除非显式允许。 |
| `nightly` | 最完整矩阵、慢速和依赖丰富的 adapter 验证。 | full profile 的工具集合，加上可选三方 adapter 依赖。 | 声明必需的工具缺失时 fail-fast。 | 关闭，除非显式允许。 |

Profile 控制变量：

- `CONFIG_FORGE_TEST_PROFILE=local|quick|full|nightly`：选择测试 profile；未设置时等价 `local`。
- `CONFIG_FORGE_REQUIRE_TOOLS=cmake,node,tsc,dotnet,javac,java`：在当前运行中把指定工具提升为必需。
- `CONFIG_FORGE_SKIP_LANGUAGES=java,csharp,typescript`：显式禁用某些可选语言 harness。
- `CONFIG_FORGE_ALLOW_NETWORK=0|1`：是否允许 `npx`、包安装、FetchContent 等潜在联网 fallback；默认 `0`。

当前 ToolProbe 会按 profile、显式 require 和 language skip policy 统一输出 skip / fail-fast 诊断；未设置 profile 时按 `local` 处理。

## Generated-code runtime harness matrix

`standard_project` integration harness 验证真实 export + generated code + runtime reader glue 能否端到端工作。它不替代 runtime compliance suite 的边界 case 覆盖。

| 语言 | Harness 输入 | 工具 | 当前状态 | local 缺工具策略 | full / nightly 目标 |
| --- | --- | --- | --- | --- | --- |
| Rust | `out/server_code/rust/generated.rs` + server runtime data | Cargo | 必跑；覆盖 inline runtime interface、JSON / MessagePack / Binary reader glue，以及 external smoke。 | 不可 skip。 | fail-fast。 |
| C++ | `out/client_code/cpp/generated.hpp/.cpp` + client runtime data | CMake、C++17 compiler、RapidJSON / msgpack-cxx / binary reader target | 已有 standard_project smoke，覆盖 RapidJSON / binary 代表路径与 external interface smoke。 | 缺 CMake / compiler 时明确 skip。 | fail-fast；三方依赖必须显式安装或显式允许 FetchContent。 |
| Unreal C++ | `out/client_code/cpp_unreal/generated.h/.cpp` | C++17 compiler + lightweight Unreal shim | 当前是语法 smoke，不代表真实 UHT / Editor。 | 缺 compiler 时明确 skip。 | fail-fast 或由引擎侧 CI 补真实验证。 |
| TypeScript | `out/client_code/typescript/generated.ts` + JSON runtime data | Node、`tsc` 或受控 `npx` fallback | 已有 typecheck + Node runtime smoke；MessagePack package 主要由 adapter compliance 覆盖。 | 缺 Node / TypeScript 时明确 skip。 | fail-fast；`npx` 仅在 `CONFIG_FORGE_ALLOW_NETWORK=1` 时允许。 |
| C# | `out/client_code/csharp/reference_game_config.cs` + JSON / MessagePack / Binary runtime data | .NET SDK | 已复制 `runtime/csharp/json_reader`、`messagepack_reader`、`binary_reader`，覆盖手写 adapter 与 generated adapter smoke。 | 缺 `dotnet` 时明确 skip。 | fail-fast。 |
| Java | `out/client_code/java/ReferenceGameConfig.java` + JSON / MessagePack / Binary runtime data | JDK (`javac` + `java`) | 已复制 JDK-only JSON / MessagePack / Binary reader，覆盖手写 adapter 与 generated adapter smoke。 | 缺 JDK 时明确 skip。 | fail-fast。 |
| Lua | generated Lua output | `lua5.4` / `lua5.3` / `lua` / `luajit` 任一 | 已有 interpreter smoke。 | 缺 interpreter 时明确 skip。 | fail-fast 或按发布目标显式声明 optional。 |

## Runtime adapter compliance matrix

Runtime compliance runner 验证具体 reader adapter 对通用 contract 的解释，包括 missing/null、scalar conversion、path/index、decode limits 和格式特有错误。它不验证 generated API、索引、关系 helper 或完整 export pipeline。

| 语言 / adapter | 格式 | 工具 | 当前状态 | 下一步 |
| --- | --- | --- | --- | --- |
| Rust `serde_json_reader` | JSON | Cargo | 已由 `tests/runtime_compliance.rs` 覆盖。 | 继续作为 local / quick 基线。 |
| Rust `serde_msgpack_reader` | MessagePack | Cargo | 已由 `tests/runtime_compliance.rs` 通过 JSON → MessagePack materialization 覆盖。 | 增加格式特有 negative case 时同步扩展。 |
| TypeScript `json_reader` | JSON | Node + TypeScript | 已由 `tests/typescript_runtime_compliance.rs` 覆盖，并接入 profile-aware ToolProbe；默认不联网。 | 后续迁移到 package-level test 并覆盖 node-fs loader。 |
| TypeScript `messagepack_reader` | MessagePack | Node + TypeScript + 测试注入 decoder；生产可使用 `@msgpack/msgpack` peer | 已由 `tests/typescript_runtime_compliance.rs` 覆盖 JSON → MessagePack materialization 与 MessagePack-only negative cases。 | 后续迁移到 package-level test 并覆盖 node-fs loader。 |
| C++ `rapidjson_reader` | JSON | CMake + C++17 + RapidJSON | 已由 `runtime/cpp/compliance_test.cpp` 的 CTest target 覆盖，并保留 runtime smoke。 | 继续扩展 shared logical cases。 |
| C++ `msgpackc_reader` | MessagePack | CMake + C++17 + msgpack-cxx + RapidJSON fixture materializer | 已由 `runtime/cpp/compliance_test.cpp` 的 CTest target 覆盖 JSON → MessagePack materialization，并保留 runtime smoke。 | 增加更多 MessagePack-only negative cases。 |
| C++ `binary_reader` | Binary | CMake + C++17 | 已由 `runtime/cpp/compliance_test.cpp` 的 CTest target 覆盖 CFGB header、schema revision、record boundary、sequential rows 和 scalar/list reads。 | 增加 schema-derived binary materializer。 |
| C# JSON / MessagePack / Binary | 多格式 | .NET SDK | 已由 `tests/csharp_java_runtime_compliance.rs` 覆盖标准 reader；standard_project 继续覆盖三格式 smoke。 | 增加更多格式特有 negative cases。 |
| Java JSON / MessagePack / Binary | 多格式 | JDK | 已由 `tests/csharp_java_runtime_compliance.rs` 覆盖标准 reader；standard_project 继续覆盖三格式 smoke。 | 增加更多格式特有 negative cases。 |

## Skip, fail-fast and network policy

- 本地 `local` profile 中，非 Rust target 工具缺失时必须输出明确 skip 原因，包括测试名称和缺失工具。
- `full` / `nightly` 中，被矩阵声明为必需或由 `CONFIG_FORGE_REQUIRE_TOOLS` 指定的工具缺失时必须 fail-fast。
- 不允许静默降级到另一个 reader 代跑；adapter 不支持的 case 必须显式标注 unsupported / skipped，并说明原因。
- 默认不允许联网：`npx --yes -p typescript tsc`、npm install、C++ `FetchContent` 等必须由 `CONFIG_FORGE_ALLOW_NETWORK=1` 或 adapter 专用显式开关启用。
- 标准工程 integration harness 与 runtime compliance suite 的覆盖目标不同，新增边界语义 case 应优先放入 compliance；新增完整 pipeline / generated API case 才放入 standard_project。
