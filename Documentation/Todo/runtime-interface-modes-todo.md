# Runtime Interface Modes TODO

本文规划 `runtime_interface = "inline" | "external"` 的分步落地。目标是统一 Rust / C++ / C# / Java 等 nominal 类型语言的 runtime reader 接入策略：默认生成代码保持自包含 inline contract，稳定 runtime core/package 成熟后可选择 external contract；TypeScript 因结构化类型可继续保持 package-friendly，但也应在文档中明确其等价策略。

## 当前判断

这项工作不建议一次性完成。原因：

1. Rust 当前实现已经硬绑定 `config_forge_runtime_reader_core`，而文档目标写的是 inline default + external opt-in；修正会影响 Rust codegen、standard_project harness、adapter compliance 和生成 fixture。
2. C++ 已有 generated-header-bound adapter 与计划中的 stable interface header，external 模式涉及 CMake target、宏兼容和 install / find_package 策略。
3. C# / Java 当前是 inline nominal interface，标准 reader 通过薄 adapter 接入；external 模式需要先明确 NuGet / jar 包边界，且 binary reader 还依赖 record boundary / schema revision / enum ordinal 等 contract 扩展。
4. 这会触及 codegen option schema、语言生成器、runtime adapter、standard_project、compliance runner 和文档矩阵；一次性改动过大，回归面也过宽。

推荐策略：先把 Rust 从 external-only 调整为 inline default + external opt-in，再逐步推广到 C++ / C# / Java。

## 全局设计

新增语言级 codegen option：

```toml
[targets.exports.rust]
runtime_interface = "inline"      # inline / external，默认 inline
runtime_core_path = "config_forge_runtime_reader_core" # external 时使用
```

对应语言可按生态调整命名：

- Rust：`runtime_core_path` 表示 crate/module path，例如 `config_forge_runtime_reader_core`、`crate::runtime`、`my_game_runtime::config_reader`。
- C++：`runtime_core_header` / `runtime_core_namespace` 或 CMake target 由 stable header 阶段定义。
- C#：`runtime_namespace` 表示 external interface 所在 namespace。
- Java：`runtime_package` 表示 external interface 所在 package。

默认策略：

| 语言 | 默认 | external 前置条件 |
| --- | --- | --- |
| Rust | inline | `runtime/rust/reader_core` contract 通过 compliance，standard_project 覆盖 inline + external smoke。 |
| C++ | inline | stable `runtime/cpp/core` header + CMake target 可安装 / 可引用。 |
| C# | inline | stable interface assembly / NuGet 包边界明确。 |
| Java | inline | stable interface jar / Maven package 边界明确。 |
| TypeScript | structural | core package 可继续使用结构化类型；如需 external import option，单独规划 package import path。 |

## [x] Step 0：确认现状并修正文档表述

目标：把当前状态描述准确，避免“Rust 默认 inline”的文档与实现冲突。

主要修改：

- 在 `Documentation/Implementation/09e-codegen-runtime-adapters.md` 中标明 Rust 当前实际为 external-core 绑定，目标是调整为 inline default + external opt-in。
- 在 Rust backend 文档中说明当前生成代码依赖 `config_forge_runtime_reader_core`，后续会补 inline 模式。
- 保留 C# / Java inline nominal + thin adapter 的当前说明。

当前实现：已在 `09e-codegen-runtime-adapters.md` 与 `09b-codegen-rust-backend.md` 区分 Rust 当前 external-core 绑定和目标 inline default + external opt-in，避免把未落地的目标态描述成现状。

验证：

- `git diff --check`
- 人工检查文档中“当前实现”和“目标态”不再混淆。

建议提交：

```text
docs(runtime): clarify runtime interface mode roadmap
```

## [ ] Step 1：抽象 shared runtime interface option schema

目标：为各语言 codegen option 提供统一的 `RuntimeInterfaceMode` 解析和校验，先不改变生成输出。

主要修改：

- 在 codegen option 层新增 `RuntimeInterfaceMode { Inline, External }` 或语言私有但同名的 serde enum。
- Rust / C++ / C# / Java options 先接收 `runtime_interface` 字段，默认 `inline`。
- 对 external 相关 path / namespace / package 做基础校验：非空、合法标识符 / path 片段。
- 为非法值增加 focused unit tests。

验证：

- `cargo fmt`
- `cargo test` 中 codegen option 相关测试。

建议提交：

```text
feat(codegen): parse runtime interface mode options
```

## [ ] Step 2：Rust 支持 inline default

目标：Rust generated code 默认自带最小 reader / loader trait、error 和 result，不再强制依赖 `config_forge_runtime_reader_core`。

主要修改：

- 拆分 `src/codegen/rust/runtime.rs`：
  - inline 模式渲染本地 `RuntimeDataReader` / `RuntimeDataLoader` / `RuntimeDataError` / `RuntimeDataResult` / contract version。
  - external 模式继续 `pub use <runtime_core_path>::{...}`。
- `RustCodegenOptions` 新增：
  - `runtime_interface: RuntimeInterfaceMode`，默认 inline。
  - `runtime_core_path: Option<String>`，external 默认 `config_forge_runtime_reader_core`。
- 更新 Rust codegen tests：默认不再包含 `pub use config_forge_runtime_reader_core`；external 测试覆盖自定义 path。
- 更新 standard_project Rust harness：默认 inline 路径需要薄 adapter 或让 runtime reader template 同时实现 inline trait；external 路径保留 core crate smoke。

验证：

- `cargo fmt`
- `cargo test --test cli_integration standard_project_generated_code_loads_exported_runtime_data`
- Rust codegen focused tests。
- Rust compliance runner 继续通过。

建议提交：

```text
feat(codegen): make Rust runtime interface inline by default
```

## [ ] Step 3：Rust external opt-in 与自定义 path 完整验证

目标：保留 Rust external core 能力，并允许用户自定义 core path / re-export path。

主要修改：

- 在 standard_project 或新增 focused fixture 中加入 external Rust target：
  - `runtime_interface = "external"`
  - 覆盖默认 `config_forge_runtime_reader_core`。
- 增加自定义 path 生成测试，例如 `runtime_core_path = "crate::runtime"`。
- 明确 external 模式生成 manifest / 诊断中提示所需 core crate / contract range。

验证：

- `cargo fmt`
- Rust codegen tests。
- standard_project Rust inline + external smoke。

建议提交：

```text
test(codegen): cover Rust external runtime interface mode
```

## [ ] Step 4：C++ stable interface header 与 external 模式

目标：让 C++ adapter 不再只能宏绑定某次 generated header，同时保留 inline 兼容。

主要修改：

- 新增 `runtime/cpp/core/include/config_forge/runtime/reader.hpp`。
- CMake 新增 `config_forge_runtime::core` interface target。
- C++ codegen 支持 `runtime_interface = "inline" | "external"`。
- RapidJSON / msgpack-cxx / binary reader 兼容 external core header，同时保留当前 generated-header-bound 迁移路径。

验证：

- `cmake -S runtime/cpp -B <build> -DCONFIG_FORGE_RUNTIME_BUILD_TESTS=ON`
- `cmake --build <build>`
- `ctest --test-dir <build>`
- standard_project C++ smoke。

建议提交：

```text
feat(runtime): add C++ external reader core header
```

## [ ] Step 5：C# / Java external 模式设计与 adapter 生成

目标：在不强制 NuGet / Maven 的前提下，降低 inline nominal interface 下的用户 glue 成本，并为未来 external package 铺路。

主要修改：

- 先增加 codegen emit thin adapter 选项，而不是直接发布 package：
  - C# 可生成 standard reader 到 generated interface 的 adapter class。
  - Java 可生成 standard reader 到 generated interface 的 adapter class。
- 设计 external 模式字段但默认不启用：
  - C#：`runtime_namespace`。
  - Java：`runtime_package`。
- 文档明确 C# / Java external 模式需要稳定 assembly / jar 后才推荐生产使用。

验证：

- `cargo fmt`
- C# / Java codegen tests。
- standard_project C# / Java JSON + MessagePack harness 继续通过。

建议提交：

```text
feat(codegen): prepare C# and Java runtime adapter glue
```

## [ ] Step 6：C# / Java binary reader contract 扩展

目标：为 C# / Java binary reader 补齐 positional layout 必需 API。

主要修改：

- C# / Java generated reader interface 增加：
  - schema revision API。
  - record boundary API。
  - enum ordinal / `ReadEnumIndex` API。
  - exhausted check。
- 更新 generated parser：row / struct / fragment 读取包裹 record boundary；enum 走 index；table load 校验 schema revision。
- 然后补 `runtime/csharp/binary_reader` 与 `runtime/java/binary_reader`。

验证：

- C# / Java codegen tests。
- standard_project C# / Java binary smoke。
- 后续 compliance runner 覆盖 binary negative cases。

建议提交：

```text
feat(codegen): support C# and Java positional binary readers
```

## [ ] Step 7：最终文档收敛

目标：把 runtime interface mode 的设计从 TODO 收敛到 canonical 文档，并删除或归档本文。

主要修改：

- 更新 runtime adapter 文档、各 backend 文档、测试矩阵文档。
- 明确各语言 inline / external 的默认值、稳定程度和推荐接入方式。
- 删除已完成的临时 TODO 或移入 DevLogs。

验证：

- `git diff --check`
- 人工检查文档中无过时 “Rust external-only” 或 “C# / Java only JSON” 表述。

建议提交：

```text
docs(runtime): finalize runtime interface mode guidance
```
