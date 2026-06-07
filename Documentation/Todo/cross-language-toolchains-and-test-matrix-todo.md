# Cross-language Toolchains and Test Matrix TODO

本文是跨语言开发环境、generated-code integration harness、runtime adapter compliance suite 的分步推进计划。目标是让 ConfigForge 的 Rust 核心开发不强制依赖所有目标语言环境，同时让 C++ / Unreal C++ / TypeScript / C# / Java / Lua 等可选工具链在本地和 CI 中被明确探测、明确 skip 或 fail-fast，并逐步把 `tests/fixtures/runtime_reader_compliance/` 升级为可执行规范。

本文放在 `Documentation/Todo/`，只用于短期推进。稳定设计完成后，应把长期规则收敛到 `Documentation/Tech/`、`Documentation/Implementation/09e-codegen-runtime-adapters.md`、`Documentation/Implementation/09h-codegen-generator-api-and-tests.md` 等 canonical 文档，并删除或归档本文。

## 全局目标

- 提供开发环境安装指导，区分核心 Rust 必需工具链和各语言 backend / runtime adapter 的可选工具链。
- 提供本地、quick CI、full CI / nightly 的测试矩阵说明。
- 将现有分散的 `command_exists` 风格探测收敛为统一 ToolProbe / skip / fail-fast 机制。
- 保证可选语言环境缺失时本地测试明确 skip，不静默降低覆盖；full CI 中声明必需的工具缺失时 fail-fast。
- 让 `standard_project` generated-code runtime harness 使用统一工具要求、统一 skip 输出和可汇总结果。
- 将 `tests/fixtures/runtime_reader_compliance/` 的 canonical payload 与 expected 结果接入自动 runner；当前已覆盖 Rust JSON / MessagePack 与 TypeScript JSON，后续补齐 TypeScript MessagePack、C++ 与 C# / Java 标准 adapters。
- 避免 TypeScript `npx` fallback 在 CI 中隐式联网；所有联网行为必须由显式配置启用。
- 保持每个步骤可以独立开发、独立验证、独立提交。

## 当前基线

- `tests/cli_integration/support/commands.rs` 已有 `command_exists(command)`，但只返回 bool，缺少版本、路径、stderr 和测试 profile 诊断。
- `tests/cli_integration/standard_project.rs` 已按语言调用 generated-code runtime harness，覆盖 Rust / C++ / Unreal C++ / C# / Java / TypeScript / Lua。
- 多数 harness 已能在缺工具时 `eprintln!` skip，但 skip 逻辑分散，格式不统一；TypeScript 仍存在 `npx` fallback，后续需要受显式 network policy 控制。
- `tests/fixtures/runtime_reader_compliance/` 已有首批 canonical JSON payload 与 expected TOML，并已形成自动 runner：`tests/runtime_compliance.rs` 覆盖 Rust JSON / MessagePack adapters，`tests/typescript_runtime_compliance.rs` 覆盖 TypeScript JSON reader。TypeScript MessagePack、C++、C#、Java runner 仍待接入。
- C# / Java JSON / MessagePack / Binary reader 已抽出到 `runtime/csharp/` 与 `runtime/java/`，generated contract 已支持 schema revision、record boundary、enum index 与 exhausted check；standard_project harness 通过薄 adapter 与 generated adapter smoke 覆盖三种格式，但尚未接入 runtime compliance runner。

## 全局验收规则

每个步骤完成后都要执行并通过：

1. `cargo fmt`。
2. 与本步骤相关的单元测试或集成测试，例如 `cargo test --test cli_integration standard_project_generated_code_loads_exported_runtime_data`。
3. 文档步骤至少执行 `git diff --check`。
4. 修改 Rust 测试工具、harness 或 runner 后执行相关 `cargo test`；具备本地 clippy wrapper 时运行 `cargo lint`。
5. 涉及可选语言工具链的步骤必须同时验证“工具存在时执行”和“工具缺失或被配置跳过时明确 skip / fail-fast”的行为；无法模拟缺失工具时至少通过单元测试覆盖 policy 分支。
6. 涉及 TypeScript `npx` / package install / C++ FetchContent 等可能联网行为时，必须验证默认不联网，只有显式配置允许联网。
7. 每个步骤完成后单独 git commit，避免把文档、工具探测、runner 和 adapter 抽取混在一个提交中。

## [x] Step 0：TODO 计划落地

目标：先固化跨语言工具链与测试矩阵的分步计划，确保后续实现有独立提交边界。

主要修改：

- 新增 `Documentation/Todo/cross-language-toolchains-and-test-matrix-todo.md`。
- 确认每个 step 标题包含 Markdown checkbox。
- 确认每个 step 都能独立开发、独立验证、独立提交。

验证：

- `git diff --check`
- 人工检查本文覆盖开发环境文档、测试矩阵、工具探测、profile、harness、compliance runner、C# / Java adapter 抽取和最终文档收敛。

建议提交：

```text
docs(testing): plan cross-language toolchain test matrix
```

## [x] Step 1：新增开发环境安装指导

目标：让开发者知道哪些工具是 ConfigForge Rust 核心开发必需，哪些工具只在负责特定语言 backend 或 runtime adapter 时安装。

主要修改：

- 新增 `Documentation/Tech/development-environment.md`。
- 覆盖核心必需工具：Rust 1.85+、Cargo、rustfmt、Clippy、可选 coverage 组件。
- 覆盖按语言可选工具：CMake / C++17 compiler、Unreal Engine 与仓库内 shim 的边界、Node.js / npm / TypeScript、.NET SDK、JDK、Lua interpreter。
- 覆盖 runtime adapter 三方依赖：RapidJSON、msgpack-cxx、TypeScript MessagePack peer dependency，以及默认不联网策略。
- 给出不同开发角色的推荐安装组合：Rust core、codegen backend、runtime adapter、full CI maintainer、Unreal 接入者。
- 更新 `Documentation/README.md` 和 `AGENTS.md` 的 Tech 文档索引。
- 在 `Documentation/code-style.md` 的 Testing Rules 中仅增加到新文档的链接，避免重复完整矩阵。

验证：

- `git diff --check`
- 人工检查文档是否明确说明“不要求所有开发者安装所有语言工具链”。
- 人工检查新增文档已被 `Documentation/README.md` 和 `AGENTS.md` 索引。

建议提交：

```text
docs(testing): add development environment guide
```

## [ ] Step 2：新增测试与 CI 矩阵文档

目标：明确本地、quick CI、full CI / nightly 的测试分层，以及各语言 generated-code harness 和 runtime compliance 的工具要求。

主要修改：

- 新增 `Documentation/Tech/test-ci-matrix.md`。
- 定义测试 profile：`local`、`quick`、`full`、`nightly`。
- 定义 generated-code runtime harness 矩阵：Rust、C++、Unreal C++ shim、TypeScript、C#、Java、Lua。
- 定义 runtime adapter compliance 矩阵：Rust JSON / MessagePack、TypeScript JSON / MessagePack、C++ RapidJSON / msgpack-cxx / binary，以及 C# / Java JSON / MessagePack / Binary 后续状态。
- 明确 skip vs fail-fast 策略：本地可选工具缺失显式 skip；full CI 声明必需工具缺失 fail-fast。
- 明确 TypeScript `npx` fallback、C++ FetchContent、npm install 等联网行为必须显式开启。
- 更新 `Documentation/README.md`、`AGENTS.md` 和 `Documentation/code-style.md` 链接。
- 在 `Documentation/Implementation/09e-codegen-runtime-adapters.md` 的 compliance 章节中链接到测试矩阵，避免重复维护命令清单。
- 在 `Documentation/Implementation/09h-codegen-generator-api-and-tests.md` 中链接到测试矩阵，保留 generated-code harness 的实现设计说明。

验证：

- `git diff --check`
- 人工检查矩阵中每个语言的工具要求、默认 profile 策略和 full CI 策略都明确。
- 人工检查没有把测试矩阵重复散落到多个 canonical 文档中。

建议提交：

```text
docs(testing): document test and CI matrix
```

## [ ] Step 3：统一 ToolProbe 与 skip 输出

目标：把现有分散的 bool 型 `command_exists` 探测升级为带诊断信息的统一工具探测层，但暂不改变测试 profile 语义。

主要修改：

- 扩展 `tests/cli_integration/support/commands.rs`。
- 新增 `ToolProbe` / `ToolStatus` 等结构，记录 command、version args、available、stdout、stderr、exit status。
- 保留兼容的 `command_exists` 或逐步替换调用点，避免一次性大改。
- 新增 helper：`probe_tool`、`probe_any_tool`、`skip_optional_test`、`require_tool_or_skip`、`require_any_tool_or_skip`。
- 统一 skip 输出格式，至少包含测试名称、缺失工具、可选安装提示。
- 清理现有 harness 中的重复手写 skip 文案。
- 修复所有静默 skip：如果某测试因缺少可选工具未执行，必须有明确输出。

验证：

- `cargo fmt`
- `cargo test --test cli_integration standard_project_generated_code_loads_exported_runtime_data`
- 使用本机已有工具验证 harness 正常执行。
- 通过单元测试或可控 fake command 覆盖 tool missing / version command failed 的分支。

建议提交：

```text
test(integration): centralize optional tool probing
```

## [ ] Step 4：引入 profile-aware 工具要求

目标：让同一套 harness 在本地默认 optional skip，在 full CI 中对声明必需工具 fail-fast。

主要修改：

- 新增测试 profile 解析，例如 `CONFIG_FORGE_TEST_PROFILE=local|quick|full|nightly`。
- 新增可选控制变量，例如：
  - `CONFIG_FORGE_REQUIRE_TOOLS=cmake,node,tsc,dotnet,javac,java`
  - `CONFIG_FORGE_SKIP_LANGUAGES=java,csharp,typescript`
  - `CONFIG_FORGE_ALLOW_NETWORK=0|1`
- 为工具要求定义 policy：`Required`、`OptionalLocalRequiredFull`、`Optional`、`Disabled`。
- 在 full / nightly profile 中，矩阵声明必需但缺失的工具应 fail-fast。
- 本地 profile 中，可选工具缺失仍明确 skip。
- 输出当前 profile 和工具探测摘要，便于 CI 诊断。

验证：

- `cargo fmt`
- `cargo test --test cli_integration`
- `CONFIG_FORGE_TEST_PROFILE=local cargo test --test cli_integration standard_project_generated_code_loads_exported_runtime_data`
- 用 fake / impossible tool 名称覆盖 local skip 与 full fail-fast 分支。
- 确认未设置环境变量时保持本地友好，不强制安装所有语言环境。

建议提交：

```text
test(integration): support profile-aware tool requirements
```

## [ ] Step 5：标准工程 runtime harness 注册表化

目标：将 `standard_project` 中逐个手写调用的多语言 harness 收敛成可声明工具要求、profile policy 和执行函数的注册表。

主要修改：

- 在 `tests/cli_integration/support/standard_runtime.rs` 或新 support 子模块中定义 `GeneratedRuntimeHarness`。
- 每个 harness 声明：language、test name、required tools、network policy、profile policy、runner function。
- `standard_project` 按注册表顺序执行，保留当前 Rust / C++ / Unreal C++ / C# / Java / TypeScript / Lua 覆盖。
- 汇总执行结果：executed、skipped、failed。
- 保持 Rust generated-code runtime test 不可 skip。
- 明确 Unreal C++ 当前是 shim syntax smoke，不代表真实 UE / UHT / Editor integration。

验证：

- `cargo fmt`
- `cargo test --test cli_integration standard_project_generated_code_loads_exported_runtime_data`
- 人工检查输出中可区分 executed 与 skipped harness。
- 确认各语言 harness 的实际断言没有因为注册表化而减少。

建议提交：

```text
test(integration): register generated runtime harnesses
```

## [ ] Step 6：控制 TypeScript 与三方依赖联网行为

目标：避免本地或 CI 中因为 `npx` / FetchContent / package install 隐式联网造成不可重复测试。

主要修改：

- TypeScript harness 中 `npx --yes -p typescript tsc` fallback 必须受 `CONFIG_FORGE_ALLOW_NETWORK=1` 或等价配置控制。
- 若 `tsc` 不存在且不允许联网，本地 profile 应明确 skip TypeScript typecheck/runtime harness，full profile 应 fail-fast 并提示安装 TypeScript。
- C++ runtime adapter 测试中明确 `CONFIG_FORGE_RUNTIME_FETCH_DEPS` 默认关闭；如果测试需要 FetchContent，必须由 full / nightly profile 显式开启。
- 文档同步说明默认不联网原则。

验证：

- `cargo fmt`
- `cargo test --test cli_integration standard_project_generated_code_loads_exported_runtime_data`
- 覆盖 `CONFIG_FORGE_ALLOW_NETWORK=0` 且无 `tsc` 时不调用 `npx` 的分支。
- 覆盖允许联网时仍可使用 `npx` fallback 的分支，若本机无网络则记录 skip / fail 原因但不能静默通过。

建议提交：

```text
test(integration): gate network-dependent tool fallbacks
```

## [x] Step 7：补齐 runtime compliance fixture schema

目标：先把 `tests/fixtures/runtime_reader_compliance/` 的 case 范围补齐到可执行规范需要的最小集合，为 runner 做准备。

主要修改：

- 补齐或调整 canonical payload：
  - `collection_envelope.json`
  - `object_table.json`
  - `nested_struct.json`
  - `fragment.json`
  - 现有 `scalar_compat.json`、`missing_vs_null.json`、`escaped_string.json`
- 补齐 expected TOML，覆盖正常结果和错误结果。
- 明确 contract version，例如 `1.0`。
- 覆盖 missing vs null、bool `true/false/1/0`、numeric string、finite float、escaped string、array 越界、int overflow、MessagePack 非 string map key、binary/ext unsupported 等 case。
- 更新 `Documentation/Implementation/09e-codegen-runtime-adapters.md`，说明哪些 fixture 已存在，哪些格式 runner 尚未接入。

验证：

- `git diff --check`
- 如果新增 TOML/JSON parser 单元测试，则运行对应 `cargo test`。
- 人工检查 fixture 与 expected 命名一致，case id 稳定。

建议提交：

```text
test(runtime): expand compliance fixtures
```

## [x] Step 8：实现 Rust runtime compliance runner MVP

目标：让 `tests/fixtures/runtime_reader_compliance/` 首先被 Rust JSON / MessagePack runtime reader 消费，证明 fixture schema 可执行。

主要修改：

- 新增 Rust compliance runner 测试入口，可放在 runtime/rust adapter crate tests 或 ConfigForge 集成测试中。
- 读取 `tests/fixtures/runtime_reader_compliance/payloads` 和 `tests/fixtures/runtime_reader_compliance/expected`。
- 复用现有 Rust JSON / MessagePack runtime reader templates。
- 覆盖 scalar、missing/null、escaped string、collection envelope 等首批 cases。
- 对错误 case 断言错误类型或错误消息的稳定关键字，不要求不同语言错误文本完全一致。
- 输出 case name、format、adapter、结果。

当前实现：`tests/runtime_compliance.rs` 作为 ConfigForge 集成测试入口，直接复用 `runtime/rust/serde_json_reader` 与 `runtime/rust/serde_msgpack_reader` 的真实 adapter 源码；同一套 canonical JSON payload 会分别经 JSON decode 与 JSON → MessagePack materialization → MessagePack decode 后执行相同 reader contract case。`reader_core` 内部的 test-only `ComplianceJsonReader` 仅保留 contract / fixture smoke，不再作为真实 adapter 合规性的唯一证明。

验证：

- `cargo fmt`
- `cargo test` 中新增 runner 相关测试。
- 如 runtime/rust adapter 是独立 crate，则运行对应 `cargo test --manifest-path runtime/rust/<adapter>/Cargo.toml`。
- 确认缺 fixture / expected mismatch 时测试失败且信息可定位 case。

建议提交：

```text
test(runtime): add Rust compliance runner
```

## [ ] Step 9：补齐 TypeScript MessagePack 与 C++ runtime compliance runner

目标：在已接入 TypeScript JSON reader 的基础上，将同一套 compliance fixture 扩展到 TypeScript MessagePack 与 C++ adapters，验证跨语言 contract 一致性。

主要修改：

- TypeScript：扩展现有 Rust 驱动的临时 compliance runner 或迁移到 package-level runner，覆盖 MessagePack reader，并保留 JSON reader 覆盖。
- C++：新增 CMake / CTest compliance runner，覆盖 RapidJSON JSON reader、msgpack-cxx reader 和 binary reader。
- runner 接入 ToolProbe / profile policy：缺 Node / npm / tsc / cmake / C++ compiler / 三方库时明确 skip 或 fail-fast。
- 更新 `Documentation/Tech/test-ci-matrix.md` 和 `Documentation/Implementation/09e-codegen-runtime-adapters.md` 的接入状态。

当前进展：TypeScript JSON reader 已通过 `tests/typescript_runtime_compliance.rs` 接入 Rust 驱动的临时测试工程。Rust 测试负责读取 TOML / JSON fixture 并生成临时 TypeScript case 数据，避免为 JSON reader package 引入 TOML parser 或测试专用依赖；本地缺 `node` 或 `tsc` / `npx` 时明确 skip。TypeScript MessagePack 与 C++ RapidJSON / msgpack-cxx / binary runner 仍待接入。

验证：

- `cargo fmt`
- `npm run typecheck` 或对应 TypeScript package test（本机有 Node / npm 时）。
- `cmake -S runtime/cpp -B <build> -DCONFIG_FORGE_RUNTIME_BUILD_TESTS=ON && cmake --build <build> && ctest --test-dir <build>`（本机有 CMake / compiler 时）。
- 缺工具时本地 profile 明确 skip，full profile fail-fast。

建议提交：

```text
test(runtime): add TypeScript and C++ compliance runners
```

## [x] Step 10：抽出 C# / Java 标准 runtime adapters

目标：减少 standard_project harness 内嵌 reader 与未来生产 adapter 漂移，为 C# / Java compliance 接入做准备。

主要修改：

- 新增 `runtime/csharp/` JSON / MessagePack / Binary reader 与 filesystem loader 示例或 copyable adapter。
- 新增 `runtime/java/` JSON / MessagePack / Binary reader 与 filesystem loader 示例或 copyable adapter。
- standard_project C# harness 改为引用或复制 `runtime/csharp/` adapter，而不是长期维护内嵌 reader。
- standard_project Java harness 改为引用或复制 `runtime/java/` adapter，而不是长期维护内嵌 reader。
- C# / Java adapter 保持 generated core 无第三方依赖；当前标准模板覆盖 BCL / JDK-only JSON、dependency-free MessagePack subset 与 Binary reader，生态 adapter 可另行明确 Jackson / Gson / System.Text.Json 依赖边界。
- 更新开发环境和测试矩阵文档。

验证：

- `cargo fmt`
- `cargo test --test cli_integration standard_project_generated_code_loads_exported_runtime_data`
- 本机有 `dotnet` 时 C# harness 运行通过；缺失时明确 skip。
- 本机有 `javac` / `java` 时 Java harness 运行通过；缺失时明确 skip。
- 人工检查 generated C# / Java core 仍不直接依赖具体 JSON 库。

建议提交：

```text
feat(runtime): add C# and Java standard runtime adapters
```

## [ ] Step 11：接入 C# / Java runtime compliance runner

目标：让 C# / Java 标准 adapter 也消费 `tests/fixtures/runtime_reader_compliance/`，覆盖与 Rust / TypeScript / C++ 一致的 reader contract。

主要修改：

- C#：新增 compliance runner project 或测试入口，读取 canonical fixtures，覆盖 JSON / MessagePack / Binary reader。
- Java：新增 compliance runner harness，读取 canonical fixtures，覆盖 JSON / MessagePack / Binary reader。
- 接入 ToolProbe / profile policy：缺 `dotnet`、`javac`、`java` 时按 profile skip 或 fail-fast。
- 更新 runtime adapter 文档中的支持矩阵。

验证：

- `cargo fmt`
- `cargo test --test cli_integration` 中相关 runner 入口。
- 本机有 `dotnet` 时 C# compliance runner 通过。
- 本机有 JDK 时 Java compliance runner 通过。
- 缺工具时本地 profile 明确 skip，full profile fail-fast。

建议提交：

```text
test(runtime): add C# and Java compliance runners
```

## [ ] Step 12：最终文档收敛和回归

目标：把已落地的工具链、测试 profile、harness 注册表和 compliance runner 状态同步到 canonical 文档，并移除或归档本文。

主要修改：

- 更新 `Documentation/Tech/development-environment.md`，确保安装指导与最终实现一致。
- 更新 `Documentation/Tech/test-ci-matrix.md`，确保 profile、工具、skip / fail-fast 和命令矩阵与代码一致。
- 更新 `Documentation/Implementation/09e-codegen-runtime-adapters.md`，记录 compliance runner 当前覆盖范围和剩余缺口。
- 更新 `Documentation/Implementation/09h-codegen-generator-api-and-tests.md`，记录 generated-code runtime harness 的注册表和 profile 行为。
- 更新 `Documentation/code-style.md`、`Documentation/README.md`、`AGENTS.md` 的索引链接。
- 完成后删除本文，或改名为非 TODO 的历史实现记录。

验证：

- `cargo fmt`
- `cargo test`
- `cargo test --test cli_integration`
- `git diff --check`
- 本机可用工具链对应的 C++ / TypeScript / C# / Java / Lua harness 和 compliance runner 均通过。
- 缺失工具链的 skip / fail-fast 行为符合当前 profile 文档。

建议提交：

```text
docs(testing): converge toolchain and compliance documentation
```

## 不能做的事

- 不能要求普通 Rust 开发者为了运行基础测试安装所有目标语言工具链。
- 不能让 optional harness 因缺少工具静默通过；必须明确 skip 或 fail-fast。
- 不能在 full CI / nightly 中因为缺少声明必需的工具而悄悄降低覆盖。
- 不能让 TypeScript `npx`、C++ FetchContent、npm install 等联网行为默认发生。
- 不能让 generated core 绑定具体 JSON / MessagePack / Android / Spring / Unreal / Jackson / Gson / System.Text.Json 等 runtime adapter 依赖。
- 不能用 standard_project integration harness 替代 runtime compliance suite；二者覆盖目标不同。
- 不能把 C# / Java harness 内嵌 reader 当作长期标准 adapter；reader 逻辑应保留在 `runtime/`，harness 只保留适配 generated nominal interface 的薄包装。
- 不能把长期测试矩阵只写在 TODO 中；实现稳定后必须收敛到 `Documentation/Tech/` 和相关 Implementation 文档。
