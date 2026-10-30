# Test and CI Matrix

本文定义 ConfigForge 本地测试、quick CI、full CI / nightly 的工具链要求、skip / fail-fast 策略，以及 generated-code runtime harness 与 runtime adapter compliance runner 的覆盖矩阵。详细环境安装见 [`development-environment.md`](development-environment.md)。

## Current GitHub Actions workflows

当前仓库配置的 workflow：

- [CI](../../.github/workflows/ci.yml)：在 `main` 的 push / pull request 上运行 Ubuntu 格式检查、Clippy 和 `cargo test --lib`，通过后构建 Linux x64 GNU、Windows x64 和 macOS 15 的 ARM64 / Intel 两种 release binary。
- [Release](../../.github/workflows/release.yml)：在 `v*.*.*` tag push 上构建上述平台，并额外构建 Linux x64 musl；打包后生成 SHA256 校验文件并发布 GitHub Release。默认 token 权限为 `contents: read`，仅发布 job 提升为 `contents: write`。发布 job 在 Bash 中通过 `${GITHUB_REF_NAME#v}` 去除 tag 的 `v` 前缀，再通过 step output 传给 Release 正文。

两个 workflow 的 macOS runner / Rust target 必须保持一致：

| Runner | 架构 | Rust target | Artifact suffix |
| --- | --- | --- | --- |
| `macos-15` | ARM64 | `aarch64-apple-darwin` | `macos-arm64` |
| `macos-15-intel` | Intel x64 | `x86_64-apple-darwin` | `macos-x64` |

当前 CI 的跨平台 job 只构建，不执行测试；Ubuntu quality job 只运行 library unit tests。因此下文的 quick / full / nightly profiles 是测试执行策略，尚未由当前 workflow 完整接入；`tests/` 集成测试、runtime harness 和其他平台专属单元测试仍需单独运行。

## Test profiles

测试 fixture 的路径断言使用 `Path` 语义；嵌入 TOML 的临时路径通过统一 helper 使用正斜杠，不能直接插入 Windows `display()`。从工作区读取后需要按多行块匹配的文本应统一 CRLF / LF。Java harness 显式使用 `javac -encoding UTF-8`，避免系统默认编码影响中文测试数据。绝对路径拒绝测试使用平台实际的绝对路径，不用 Unix-only `/tmp` 假定。

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
- `CONFIG_FORGE_MAX_OPTIONAL_SKIPS=<count>`：覆盖本次运行允许的 optional skip 上限；未设置时，`full` / `nightly` 或设置了 `CONFIG_FORGE_REQUIRE_TOOLS` 的运行默认上限为 `0`，`local` / `quick` 默认不设上限。

当前 ToolProbe 会按 profile、显式 require 和 language skip policy 统一输出 skip / fail-fast 诊断；未设置 profile 时按 `local` 处理。Generated-code runtime harness 与 standalone runtime compliance runner 必须输出 `executed/skipped/failed` summary，并在 summary 后检查 optional skip 上限，避免 CI 中由于可选工具缺失而静默降低覆盖率。

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
| Rust `binary_reader` | Binary | Cargo | 已由 `runtime/rust/binary_reader` crate tests、`tests/runtime_compliance.rs` schema-derived materializer 与 standard_project binary harness 覆盖；包含 malformed corpus、decode limits、optional bitmap padding bits 非 0 拒绝。 | 后续将 operation/case schema 抽为共享数据，减少语言内联 builder。 |
| TypeScript `json_reader` | JSON | Node + TypeScript | 已由 `tests/typescript_runtime_compliance.rs` 覆盖，并接入 profile-aware ToolProbe；默认不联网；workspace package typecheck/build 已验证。 | 后续迁移到 package-level test 并覆盖 node-fs loader。 |
| TypeScript `messagepack_reader` | MessagePack | Node + TypeScript + `@msgpack/msgpack` backend（测试中用本地 shim 离线替代真实包） | 已由 `tests/typescript_runtime_compliance.rs` 覆盖 JSON → MessagePack materialization、默认 backend decode 路径与 MessagePack-only negative cases；workspace package typecheck/build 已验证。 | 后续迁移到 package-level test 并覆盖 node-fs loader。 |
| TypeScript `binary_reader` | Binary | Node + TypeScript | 已由 `tests/typescript_runtime_compliance.rs` 覆盖 shared logical success cases 与 binary malformed case；包含 optional bitmap padding bits 非 0 拒绝；workspace package typecheck/build 已验证。 | 后续纳入完整 shared binary negative corpus。 |
| C++ `rapidjson_reader` | JSON | CMake + C++17 + RapidJSON | 已由 `runtime/cpp/compliance_test.cpp` 的 CTest target 覆盖，并保留 runtime smoke。 | 继续扩展 shared logical cases。 |
| C++ `msgpackc_reader` | MessagePack | CMake + C++17 + msgpack-cxx + RapidJSON fixture materializer | 已由 `runtime/cpp/compliance_test.cpp` 的 CTest target 覆盖 JSON → MessagePack materialization，并保留 runtime smoke。 | 增加更多 MessagePack-only negative cases。 |
| C++ `binary_reader` | Binary | CMake + C++17 | 已由 `runtime/cpp/compliance_test.cpp` 的 CTest target 覆盖 shared logical success cases、CFGB header、schema revision、record boundary、sequential rows、scalar/list reads 和 optional bitmap padding bits 非 0 拒绝；CMake install + `find_package` smoke 已覆盖。 | 增加更多 shared binary negative corpus。 |
| C# JSON / MessagePack / Binary | 多格式 | .NET SDK | 已由 `tests/csharp_java_runtime_compliance.rs` 覆盖标准 reader；Binary 覆盖 shared logical success cases、bad magic、unsupported version、truncated header/body、wrong schema revision、invalid optional bitmap、invalid enum ordinal、huge length prefix、not exhausted 与 record/row boundary；template metadata/version/contract range 由 compliance 断言。standard_project 继续覆盖三格式 smoke。 | 增加更多 shared binary negative corpus。 |
| Java JSON / MessagePack / Binary | 多格式 | JDK | 已由 `tests/csharp_java_runtime_compliance.rs` 覆盖标准 reader；Binary 覆盖 shared logical success cases、bad magic、unsupported version、truncated header/body、wrong schema revision、invalid optional bitmap、invalid enum ordinal、huge length prefix、not exhausted 与 record/row boundary；template metadata/version/contract range 由 compliance 断言。standard_project 继续覆盖三格式 smoke。 | 增加更多 shared binary negative corpus。 |

## Runtime helper release gate

standard runtime helpers 进入 Stable 后，release / full CI 应额外覆盖 package/template metadata 与安装 smoke：

| Gate | 命令 / 入口 | 说明 |
| --- | --- | --- |
| Metadata drift | `cargo test --test runtime_helper_release_metadata` | 断言 generated code contract constants、Rust/TypeScript/C++ package metadata、C# / Java template version、supported contract range 与 distribution mode 一致。 |
| Rust reader core package | `cargo package --manifest-path runtime/rust/reader_core/Cargo.toml --allow-dirty` | 验证可独立打包并从 package tarball 编译。 |
| Rust adapter package file-list | `cargo package --manifest-path runtime/rust/{serde_json_reader,serde_msgpack_reader,binary_reader}/Cargo.toml --list` | 验证 adapter crate 打包文件集合。正式 registry verification 需先发布 `config-forge-runtime-reader-core = 0.1.0`，再按 core → adapters 顺序执行。 |
| TypeScript packages | `npm pack runtime/typescript/<package> --dry-run` | 覆盖 `reader_core`、JSON、MessagePack、binary、node-fs 包的 `exports` / `types` / 文件列表。若 source 改动影响 dist，先运行对应 `tsc --project tsconfig.build.json`。 |
| C++ install / find_package | `cmake -S runtime/cpp -B <build> -DCONFIG_FORGE_RUNTIME_BUILD_TESTS=ON && cmake --build <build> && ctest --test-dir <build> --output-on-failure` | 覆盖 binary reader smoke、compliance 与 downstream `find_package(ConfigForgeRuntime 0.1 REQUIRED CONFIG)` consumer build。 |
| C# / Java templates | `cargo test --test csharp_java_runtime_compliance` | 编译 copyable templates，并断言 template version、supported contract range、distribution mode 和 JSON / MessagePack / binary reader 语义。 |

## Platform filesystem edge matrix

Artifact Writer 的 path containment 与 conflict key 需要在真实平台文件系统上验证；相关语义见 [`platform-filesystem-semantics.md`](platform-filesystem-semantics.md)。这些测试属于 Rust core test suite，不依赖跨语言工具。

| 场景 | 平台 / 文件系统要求 | 覆盖入口 | quick 策略 | full / nightly 目标 | skip / 诊断策略 |
| --- | --- | --- | --- | --- | --- |
| macOS default case-insensitive volume | macOS 默认 APFS / HFS+ 大小写不敏感卷 | `src/artifact/mod.rs` 的 `output_root_detects_case_insensitive_collision_on_default_macos_volume` | 在 macOS runner 上执行；非 macOS 不适用。 | macOS runner 必跑。 | 如果 CI 临时目录所在卷被配置为大小写敏感，测试输出明确 skip 诊断并返回；CI 配置应优先使用默认大小写不敏感卷。 |
| Linux symlink / hardlink alias | Linux ext4 / XFS 等常见大小写敏感文件系统，支持 symlink 与 hardlink | `src/artifact/mod.rs` 的 Unix artifact alias tests（symlink output root alias、existing hardlink alias） | Linux runner 必跑。 | Linux runner 必跑。 | 这些能力在 Linux CI 中视为基础能力；失败应作为真实回归处理，不静默 skip。 |
| Windows symlink / junction alias | Windows NTFS / ReFS；symlink 可能需要 Developer Mode 或权限，junction 通常不需要管理员权限但仍可能受策略限制 | `src/artifact/mod.rs` 的 `output_root_detects_windows_symlink_alias_conflict`、`output_root_detects_windows_junction_alias_conflict` | Windows runner 上执行；创建 symlink / junction 失败时 graceful skip。 | Windows runner 上执行；建议启用 Developer Mode 使 symlink test 真实覆盖，junction 失败需在日志中说明。 | 测试必须打印被 skip 的对象、目标和系统错误；不能把权限不足伪装成 pass。 |
| Windows path prefix / drive letter / UNC lexical normalization | Windows path parser，可覆盖 drive、verbatim drive (`\\?\C:`) 与 UNC (`\\server\share`) | `src/artifact/mod.rs` 的 `windows_lexical_normalization_preserves_drive_verbatim_and_unc_prefixes` | Windows runner 必跑。 | Windows runner 必跑。 | 不访问网络 UNC；只验证 lexical component 处理，因此不应因网络或权限 skip。 |

## Skip, fail-fast and network policy

- 本地 `local` profile 中，非 Rust target 工具缺失时必须输出明确 skip 原因，包括测试名称和缺失工具。
- `full` / `nightly` 中，被矩阵声明为必需或由 `CONFIG_FORGE_REQUIRE_TOOLS` 指定的工具缺失时必须 fail-fast；summary 级别的 optional skip 上限默认为 `0`，因此即使某个 runner 先记录 skip，也会在同一测试入口失败。
- CI 可通过 `CONFIG_FORGE_MAX_OPTIONAL_SKIPS=0` 显式加固 quick / custom profile；如确需临时放宽，必须设置为具体数字并在 CI 配置中说明原因。
- Generated-code runtime harness、TypeScript compliance、C# compliance、Java compliance 都必须打印 `executed/skipped/failed`，方便从 CI 日志中确认真实覆盖数量。
- 不允许静默降级到另一个 reader 代跑；adapter 不支持的 case 必须显式标注 unsupported / skipped，并说明原因。
- 默认不允许联网：`npx --yes -p typescript tsc`、npm install、C++ `FetchContent` 等必须由 `CONFIG_FORGE_ALLOW_NETWORK=1` 或 adapter 专用显式开关启用。
- 标准工程 integration harness 与 runtime compliance suite 的覆盖目标不同，新增边界语义 case 应优先放入 compliance；新增完整 pipeline / generated API case 才放入 standard_project。
