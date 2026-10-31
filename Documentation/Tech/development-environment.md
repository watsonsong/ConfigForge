# Development Environment Guide

本文说明 ConfigForge 开发所需的本地工具链。核心原则是：**Rust 核心开发只要求 Rust 工具链；跨语言 generated-code harness 和 runtime adapter 测试按需启用，不要求每个开发者安装所有目标语言环境**。

相关文档：

- [`../code-style.md`](../code-style.md) — Rust、测试和提交规则。
- [`unit-test-coverage.md`](unit-test-coverage.md) — Rust 覆盖率工具安装和使用。
- [`cpp-runtime-dependencies.md`](cpp-runtime-dependencies.md) — C++ runtime reader 三方依赖、CMake option 和默认不联网策略。
- [`../Implementation/09h-codegen-generator-api-and-tests.md`](../Implementation/09h-codegen-generator-api-and-tests.md) — generated-code runtime harness 设计。
- [`../Implementation/09e-codegen-runtime-adapters.md`](../Implementation/09e-codegen-runtime-adapters.md) — runtime adapter contract、compliance suite 和支持矩阵。

## 必需：Rust 核心开发环境

所有开发者都需要安装：

| 工具 | 要求 | 用途 |
| --- | --- | --- |
| Rust / Cargo | Rust 1.99.1+ | ConfigForge crate 使用 Rust 2024 edition，`Cargo.toml` 声明 `rust-version = "1.99.1"`。 |
| rustfmt | 随 Rust toolchain 安装，或通过 rustup component 安装 | 运行 `cargo fmt` / `cargo fmt --check`。 |
| Clippy | `rustup component add clippy` | 运行 `.cargo/config.toml` 中的 `cargo lint` alias。 |

常用命令：

```bash
cargo fmt --check
cargo lint
cargo test
```

如果本机没有 Clippy，可先安装：

```bash
rustup component add clippy
```

`cargo lint` 是项目默认 lint profile，等价于对 workspace 的 all targets / all features 运行 Clippy，并禁止 `unsafe_code`。更严格的 `cargo lint-strict` 只用于评估 API / design cleanup 候选，不要求每次提交都无条件应用 pedantic 建议。

## Rust 工具链与依赖升级策略

- `rust-toolchain.toml` 将本地开发固定为已验证的 Rust 1.99.1（当前稳定版），包含 rustfmt / Clippy；CI 和 Release 的 RUST_VERSION 与之同步。工具 crate 的 rust-version 同为 1.99.1，升级工具链时应一起维护三处声明。
- 独立 runtime/rust crates 继续声明 Rust 1.85；它们的编译器要求与工具分开管理。生成代码默认保持 Rust 2021 兼容，不把工具内部的 let chains 等 Rust 2024 新语法写进默认产物。
- Cargo.toml 使用当前直接依赖的稳定版本作为兼容范围下限，Cargo.lock 固定已验证的完整依赖图。工具 crate 的 CI 检查、测试与 release build 均使用 --locked，避免构建期间悄悄解析到新版本。
- Rust 1.88 起稳定的 let chains 可用于合并共享作用域的嵌套条件，保留原有求值顺序和短路语义；不为采用新 API 改变诊断、排序、锁生命周期或错误处理。syn 3 的 AST visitor 继续负责架构 guard，回归覆盖 let-chain 中的模块引用。
- 独立 runtime crates 不属于根 workspace；CI 额外逐包执行 fmt、严格 Clippy 和单元测试，不能用根 cargo test 代替。
- 库/API 升级先运行编译和对应 contract 回归，再运行 cargo fmt、cargo test --locked、cargo lint 和严格 Clippy；依赖升级本身不代表 runtime 格式或 public API 版本必须改变。

## XLSX 读取库

使用 [calamine](https://github.com/tafia/calamine) 0.37.0（MIT），以 `default-features = false` 引入。它是纯 Rust 读取库，无需 Excel / COM / Python；不启用 chrono、图片或公式计算引擎。工具的 Rust 1.99.1 基线满足该依赖要求；完整传递版本由 Cargo.lock 固定，更新时必须检查 ZIP/XML 依赖和跨平台回归。库支持其他格式不代表 ConfigForge 开放 xls/xlsb/ods。

通过 `Xlsx::worksheet_cells_reader` 和 `next_cell_with_formula_metadata` 同时读取缓存值、公式元数据与绝对坐标。使用稀疏行列存储，不根据格式化后的巨大 worksheet dimension 分配稠密矩阵，也不展开共享公式字符串。缺少缓存（Empty）和有效空字符串（String）由 API 区分，真实 XLSX fixture 验证这两个分支。

ConfigForge 负责 raw string 归一化和业务诊断；Meta/resolver 继续解释业务类型。日期/时长要求文本，错误值拒绝，公式只读取保存后的缓存。详细行为见 [XLSX 输入](../Design/03-configuration.md#341-本地-xlsx-输入)。

工作簿缓存限定在一次 source read 内，按规范化物理路径复用共享字符串/样式和 ZIP 句柄；缓存条目受读取计划文件数量约束，不跨 build 保留。同文件读取互斥、不同文件并行，文件打开不占用全局 map 锁。选用 calamine 而非 umya-spreadsheet 是因为当前无需编辑或回写；rust_xlsxwriter 是写入库，不能承担 source 解析。

测试 fixture 可通过 `node tests/fixtures/xlsx/generate.mjs` 确定性重建；生成脚本仅用于维护测试文件，不是工具或 cargo test 的运行依赖。新增边界时同时维护 fixtures、读取单测、CLI 混合 source 与诊断/report 回归。

## 可选：覆盖率工具

功能开发或测试补强后，应按 [`unit-test-coverage.md`](unit-test-coverage.md) 评估覆盖率。首次使用需要：

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
```

常用命令：

```bash
cargo coverage
cargo coverage-html
cargo coverage-lcov
```

覆盖率工具不是运行普通 `cargo test` 的前置条件；它用于判断新增或修改逻辑的测试完整性。

## 按需：跨语言 generated-code harness 工具链

`tests/fixtures/standard_project/` 会生成多语言 codegen，并在对应工具存在时运行 generated-code runtime harness。缺少某个可选语言工具链时，本地测试应显式 skip 对应 harness，而不是要求所有开发者安装完整矩阵。

| 负责范围 | 建议安装 | 用途 | 本地缺失时 |
| --- | --- | --- | --- |
| Rust generated code | Rust / Cargo | Rust generated-code JSON / MessagePack / bin runtime harness | 不应 skip。Rust 是核心开发必需环境。 |
| C++ backend / runtime adapter | C++17 compiler、CMake 3.16+ | C++ generated-code compile/run、`runtime/cpp` CMake smoke | 显式 skip C++ harness。 |
| Unreal C++ backend | C++17 compiler；真实 Unreal Engine 仅用于项目侧完整验证 | 仓库内使用 lightweight shim 做 `c++ -fsyntax-only` syntax smoke | 显式 skip Unreal C++ syntax smoke。 |
| TypeScript backend / runtime | Node.js、npm、TypeScript compiler (`tsc`) | TypeScript generated-code typecheck、Node runtime harness、runtime package build | 显式 skip TypeScript harness；full / nightly 或显式 require 时 fail-fast。 |
| C# backend | .NET 8 SDK / runtime（generated-code 与 compliance harness 使用 `net8.0`） | C# generated-code compile/run | 显式 skip C# harness。 |
| Java backend | JDK，包含 `javac` 和 `java` | Java generated-code compile/run | 显式 skip Java harness。 |
| Lua data+code export | `lua5.4` / `lua5.3` / `lua` / `luajit` 之一 | Lua runtime smoke | 显式 skip Lua harness。 |

当前本地集成测试使用统一 ToolProbe 和 profile-aware policy 决定是否运行可选 harness。`CONFIG_FORGE_TEST_PROFILE`、`CONFIG_FORGE_REQUIRE_TOOLS`、`CONFIG_FORGE_SKIP_LANGUAGES` 和 `CONFIG_FORGE_ALLOW_NETWORK` 的语义见 [`test-ci-matrix.md`](test-ci-matrix.md)。

## 按需：runtime adapter 三方依赖

Generated core 不应直接依赖具体 JSON / MessagePack / engine / framework 库。第三方依赖只属于 runtime adapter 或测试 harness。

| Adapter 范围 | 依赖 | 说明 |
| --- | --- | --- |
| Rust JSON / MessagePack reader packages | Cargo dependencies | 独立 Cargo packages，使用共享 reader_core；由 package tests/compliance 验证。 |
| TypeScript JSON reader | Node.js / TypeScript | JSON reader 不需要 MessagePack peer dependency。 |
| TypeScript MessagePack reader | `@msgpack/msgpack` peer dependency | 只在 MessagePack reader package / full matrix 中需要。 |
| C++ RapidJSON reader | RapidJSON | 详见 [`cpp-runtime-dependencies.md`](cpp-runtime-dependencies.md)。 |
| C++ msgpack-cxx reader | msgpack-cxx | 详见 [`cpp-runtime-dependencies.md`](cpp-runtime-dependencies.md)。 |
| C# / Java standard adapters | .NET BCL `System.Text.Json` / dependency-free MessagePack subset parser / JDK-only JSON parser / dependency-free binary reader | 当前已提供 `runtime/csharp/json_reader`、`runtime/csharp/messagepack_reader`、`runtime/csharp/binary_reader`、`runtime/java/json_reader`、`runtime/java/messagepack_reader` 和 `runtime/java/binary_reader`，standard_project harness 通过薄 adapter 接入 generated loader / reader interface。 |

### 默认不联网

本仓库的测试和 CMake 配置默认不应隐式联网：

- C++ `runtime/cpp` 的 `CONFIG_FORGE_RUNTIME_FETCH_DEPS` 默认应保持 `OFF`；自动下载三方库必须显式开启。
- TypeScript harness 对 `npx --yes -p typescript tsc` 这类 fallback 受 `CONFIG_FORGE_ALLOW_NETWORK=1` 控制，避免 CI 隐式联网。
- npm install、FetchContent、远程 package download 都应只出现在开发者主动安装或 full / nightly profile 显式允许的流程中。

## 推荐安装组合

### Rust core developer

安装：Rust 1.99.1+、rustfmt、Clippy；可选安装 `cargo-llvm-cov`。

常用验证：

```bash
cargo fmt --check
cargo lint
cargo test
```

### Codegen backend developer

在 Rust core 环境基础上，安装正在开发的目标语言工具链：

- C++ backend：C++17 compiler + CMake。
- Unreal C++ backend：C++17 compiler；真实 UE 验证由项目侧或 dedicated CI 负责。
- TypeScript backend：Node.js + npm + TypeScript compiler。
- C# backend：.NET SDK。
- Java backend：JDK。

常用验证：

```bash
cargo test code_artifact::<language>
cargo test --test cli_integration standard_project_generated_code_loads_exported_runtime_data
```

### Runtime adapter developer

在 Rust core 环境基础上，安装对应 adapter 的三方依赖和语言工具：

- Rust adapter：Cargo 依赖即可。
- TypeScript adapter：Node.js / npm / TypeScript；MessagePack reader 额外需要 `@msgpack/msgpack`。
- C++ adapter：CMake、C++17 compiler、RapidJSON 或 msgpack-cxx。
- C# adapter：.NET SDK；当前 JSON / MessagePack / Binary 标准 reader 已接入 compliance runner。
- Java adapter：JDK (`javac` + `java`)；当前 JSON / MessagePack / Binary 标准 reader 已接入 compliance runner。

常用验证示例：

```bash
cargo test
cmake -S runtime/cpp -B target/runtime-cpp -DCONFIG_FORGE_RUNTIME_BUILD_TESTS=ON
cmake --build target/runtime-cpp
ctest --test-dir target/runtime-cpp
```

### Full CI / release maintainer

安装完整矩阵：Rust、C++17 compiler、CMake、Node.js、npm、TypeScript、.NET SDK、JDK、Lua interpreter，以及完整 runtime adapter 所需三方依赖。full / nightly profile 中，声明必需的工具缺失应视为环境错误，而不是 silent skip。

### Unreal project integrator

ConfigForge 仓库内的 Unreal C++ 测试只验证 generated code 可以在 lightweight shim 下通过 C++ syntax smoke。真实项目还需要在 Unreal module 中提供：

- Unreal headers 和 build rules；
- `api_macro`，例如 `GAMECONFIG_API`；
- 真实 `IConfigForgeRuntimeDataLoader` / `IConfigForgeRuntimeDataReader` adapter；
- UHT / Editor / Cook / packaged build 验证。

## 常见问题

### 为什么本地缺少 JDK / dotnet / Node 时测试仍能通过？

因为这些工具只服务于对应语言的 generated-code harness 或 runtime adapter compliance runner。普通 Rust 开发不应被要求安装全部目标语言环境。测试必须明确打印 skip 原因；full / nightly profile 或 `CONFIG_FORGE_REQUIRE_TOOLS` 可以把这些工具配置为必需并 fail-fast。

### 为什么不能默认使用 `npx` 或 CMake FetchContent 自动下载依赖？

默认联网会让本地和 CI 测试不可重复，也会掩盖环境依赖。所有联网 fallback 都必须显式开启，并在测试输出中可诊断。

### runtime compliance 和 standard_project integration 有什么区别？

`standard_project` integration harness 验证真实 fixture 的导出数据、generated code、runtime glue 和代表性 API 查询。`tests/fixtures/runtime_reader_compliance/` 目标是用同一批 canonical logical cases 验证不同语言 / 不同格式 runtime reader adapter 对 reader contract 的解释一致。core 模块本身应尽量薄，只定义接口形状、错误和 limits；compliance 主要验证具体 adapter，而不是要求每种语言都有厚 core。

二者需要同时保留，但不能维护两套重复语义用例：`standard_project` 只放端到端 smoke 和真实项目组合风险，reader contract 的 scalar、missing/null、index、错误 kind/keyword、格式非法 payload 等边界条件放入 `tests/fixtures/runtime_reader_compliance/`。新增测试时如果只是验证 reader 语义，应优先进入 compliance；如果验证 generated API、target filtering、索引/关系 helper 或完整 build/export/codegen 组合，才进入 `standard_project` integration。

### 新增一种 reader 格式时，compliance 是否只能测 JSON？

不能。JSON 只是 canonical fixture 和本地 MVP 起点。长期 compliance runner 应按“共享 logical case + 格式 materializer + reader factory”接入新格式：case driver 负责执行同一批 path/operation/assertion，格式 adapter 负责把 canonical Runtime Value materialize 成自己的 payload，并返回对应 reader。

例如新增 BSON reader 时，不应复制一套 BSON 专用 expected 文件；应复用现有 scalar、container、missing/null logical cases，把它们 materialize 为 BSON document 后跑同一批 assertion。只有 BSON 特有的非法值或不可表达语义（如 BSON Binary、ObjectId、DateTime 是否 unsupported，document key 限制等）才新增 BSON-only negative cases，并在 capability / skip policy 中显式声明。
