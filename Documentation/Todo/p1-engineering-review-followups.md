# P1 Engineering Review Follow-ups TODO

## 背景

本 TODO 汇总 2026-06-28 工程结构 Review 中判定为 P1 的问题。当前工程整体模块划分健康，没有发现阻断发布的 P0；但进入 Stable 后，需要优先收敛以下高价值风险：

1. crate public API 暴露过宽，内部 generator/build 协议可能被外部 API 兼容性冻结；
2. architecture guard 没有完整覆盖 `01-module-outline.md` 中的依赖 DAG；
3. structured runtime error 的跨语言 `code` 暴露不一致；
4. Java runtime error 可见性、C# / Java loader IO 错误包装与文档承诺不完全一致。

完成本 TODO 后，应删除已完成条目；若全部完成，删除本文件。

## P1-1：收敛 crate public API 暴露

### 问题定位

- `src/lib.rs:34` 起从 `code_artifact` re-export 多个内部类型。
- `src/lib.rs:35` re-export `CodeExportInput`、`CodeExportProject`、`CodeExportTarget`。
- `src/lib.rs:36` re-export `CodegenTaskConfig`、`CodegenOptions`、`CodeWriterLike` 等内部配置/实现细节。
- `src/code_artifact/mod.rs:573` 定义 `CodeExportInput<'meta>`，包含 generator 执行内部字段。
- `src/code_artifact/mod.rs:590` 的 `CodeGenerator` trait 是 `pub(crate)`，外部并不能完整消费这些 public input 类型。

### 目标

- crate root 只公开稳定用户 facade、配置 schema、诊断/报告、必要产物类型。
- generator/build 内部协议不作为 crate public API 暴露。
- 不破坏现有 CLI/build 功能与内部测试。

### TODO

- [ ] 审计 `src/lib.rs` 所有 `pub use code_artifact::{...}` 条目，区分稳定 public API 与内部实现细节。
- [ ] 将 `CodeExportInput`、`CodeExportProject`、`CodeExportTarget`、`CodegenTaskConfig` 从 crate root re-export 移除；如无外部稳定需求，进一步将其可见性收窄为 `pub(crate)`。
- [ ] 审计 `CodeWriterLike`、`CheckedStreamingCodeWriter`、`StreamingCodeWriter` 是否确实需要作为 public API；若只供 generator 内部使用，移出 crate root re-export。
- [ ] 保留必要的用户配置类型，例如 `CodeArtifactOptions`、`NamingConfig`、语言 options、`I18nOptions` 等。
- [ ] 补充或调整单元测试，确保内部 generator 仍可正常访问收窄后的类型。
- [ ] 若 public API 收敛影响文档，更新 `Documentation/Implementation/09-codegen-module-detail-design.md` 与 `Documentation/Implementation/01-module-outline.md` 中的 public API 说明。

### 验收

- [ ] `cargo test` 通过。
- [ ] crate root 不再公开内部 generator execution input。
- [ ] 文档中的“最小公开 API”原则与实现一致。

## P1-2：补强 architecture module boundary guard

### 问题定位

- `tests/architecture.rs:10` 只覆盖 `diagnostic` 部分 forbidden modules。
- `tests/architecture.rs:14` 只覆盖 `validation` 部分 forbidden modules。
- `tests/architecture.rs:18` 只覆盖 `code_artifact`。
- `tests/architecture.rs:22` 只覆盖 `data_artifact`。
- `tests/architecture.rs:26` 只覆盖 `generator/schema_doc`。
- `Documentation/Implementation/01-module-outline.md:82` 之后的完整依赖约束未完全测试化。

### 目标

- 把 `01-module-outline.md` 中核心单向依赖规则转化为自动化 guard。
- 防止后续新增功能引入隐性反向依赖或循环依赖。

### TODO

- [x] 扩展 `tests/architecture.rs` 的 forbidden dependency 表，覆盖 `meta`、`data`、`asset`、`artifact`、`artifact_info`、`runtime_contract`、`project`、`build`、`cli`。
- [x] 增加 `runtime_contract` guard：禁止依赖 `data`、`validation`、`data_artifact`、`code_artifact`、`generator`、`project`、`build`。
- [x] 增加 `artifact` guard：禁止依赖 `meta`、`data`、`validation`、`data_artifact`、`code_artifact`、`generator`、`project`、`build`。
- [x] 增加 `asset` guard：禁止依赖 `data`、`validation`、`data_artifact`、`code_artifact`、`generator`、`build`。
- [x] 增加 `project` guard：禁止依赖 `build`，并避免依赖 toolkit 内部 task config。
- [x] 增加 `cli` guard：只能依赖 `build` 与诊断/错误展示相关底层类型。
- [x] 评估当前 string-based `crate::<module>` 检查是否足够；如误报/漏报明显，改进 grouped import / absolute path 检测。
- [x] 若发现现有代码违反约束，先判断是实现需要修正还是文档依赖图需要更新。

### 验收

- [x] `cargo test --test architecture` 通过。
- [x] `tests/architecture.rs` 覆盖 `01-module-outline.md` 中所有关键依赖方向。
- [x] 生产代码新增反向依赖会被测试捕捉。

## P1-5：修正 structured runtime error 的跨语言 code 暴露不一致

### 问题定位

- 文档在 `Documentation/Implementation/09a-codegen-runtime-reader-contract.md:74`、`:82` 描述跨语言 structured `kind/code/path`。
- Rust `runtime/rust/reader_core/src/lib.rs:90` 附近的 `RuntimeDataError` 没有显式 `code()`。
- TypeScript `runtime/typescript/reader_core/src/runtime_reader.ts:23` 的 `RuntimeDataError` 没有显式 `code`。
- C++ `runtime/cpp/core/include/config_forge/runtime/reader.hpp:35` 附近的 `RuntimeDataError` 没有 `Code()`。
- C# `runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs:35` 已有 `Code`。
- Java binary reader 已有 `code()`，但部分 Java error 类型可见性不足，见 P1-6。

### 目标

- 所有 Stable runtime helpers 对业务暴露统一的 `CF_RT_*` 机器码。
- 用户不需要解析 message，也不需要自行约定 kind-to-code 映射。

### TODO

- [x] Rust `RuntimeDataError` 增加 `code()` 或等价 accessor，返回 `CF_RT_*`。
- [x] TypeScript `RuntimeDataError` 增加 `readonly code` 或 `code()`，返回 `CF_RT_*`。
- [x] C++ `RuntimeDataError` 增加 `Code()`，返回稳定 `const char*` 或 `std::string`。
- [x] 同步 generated inline runtime contract 中的 Rust/TS/C++ error 类型，避免 inline/external 漂移。
- [x] C# / Java 保持已有 `Code` / `code()`，并确认所有 adapter/template 一致。
- [x] 扩展 compliance tests，断言至少 `missing field`、`type mismatch`、`invalid path`、`decode` 的 `code`。
- [x] 更新 `Documentation/Implementation/09a-codegen-runtime-reader-contract.md` 与 `09e-codegen-runtime-adapters.md`，明确 `kind` 与 `code` 的映射表。

### 验收

- [x] `cargo test --test runtime_compliance` 通过，并覆盖 Rust code 断言。
- [x] `cargo test --test typescript_runtime_compliance` 通过，并覆盖 TS code 断言。
- [x] C++ CTest compliance 通过，并覆盖 C++ code 断言。
- [x] `cargo test --test csharp_java_runtime_compliance` 通过，并覆盖 C# / Java code 断言。

## P1-6：修正 Java runtime error API 可见性与 C# / Java loader IO structured error

### 问题定位

- Java JSON reader：`runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataReader.java:12` 的 error kind 是 package-private。
- Java JSON reader：`runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataReader.java:22` 的 exception 是 package-private。
- Java JSON reader：`runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataReader.java:38`、`:39`、`:40` 的 `kind()` / `code()` / `path()` 也是 package-private。
- Java JSON loader：`runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataLoader.java:28` 捕获 IO。
- Java JSON loader：`runtime/java/json_reader/src/configforge/runtime/json/ConfigForgeJsonRuntimeDataLoader.java:29` 直接 `throw new RuntimeException(error)`。
- C# binary loader：`runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs:51` 是 loader 入口。
- C# binary loader：`runtime/csharp/binary_reader/BinaryRuntimeDataReader.cs:54` 直接 `File.ReadAllBytes(...)`，未包装 IO。

### 目标

- Java runtime structured error 对包外业务代码可见。
- C# / Java 所有标准 loader 的 IO/path 错误都按 `Io` kind 和 `CF_RT_IO` 机器码暴露。

### TODO

- [x] Java JSON / MessagePack / Binary reader 中的 `ConfigForgeRuntimeErrorKind`、`ConfigForgeRuntimeException` 及 `kind()` / `code()` / `path()` 改为 public 可见；如受 Java 单 public top-level class 限制，考虑拆文件或嵌套 public static 类型。
- [x] Java JSON / MessagePack / Binary loader 捕获 `IOException` 并包装为 `ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Io, ..., cause)`。
- [x] Java loader 的 logical path / resource path 尽量写入 exception path/context。
- [x] C# JSON / MessagePack / Binary loader 捕获 `IOException`、`UnauthorizedAccessException`、path 相关异常，并包装为 `ConfigForgeRuntimeException(RuntimeDataErrorKind.Io, ..., innerException)`。
- [x] 补 C# / Java compliance：在包外测试代码中捕获 structured exception 并断言 `Kind` / `Code` / `Path`。
- [x] 补 missing file / path traversal / unreadable file 的 loader-level negative cases。

### 验收

- [x] `cargo test --test csharp_java_runtime_compliance` 通过。
- [x] C# / Java 包外业务代码可读取 runtime error kind/code/path。
- [x] loader IO 错误不再落到普通 `RuntimeException` / `IOException` / `FileNotFoundException`。

## 推荐执行顺序

1. **P1-5 + P1-6**：统一 structured runtime error 的 code/API/loader 行为。
2. **P1-2**：补强 architecture guard，防止后续重构引入反向依赖。
3. **P1-1**：最后收敛 public API；这一步可能影响外部 API，需要结合 release note 或迁移说明处理。

## 总体验收命令

完成全部 P1 后至少执行：

```bash
cargo fmt
cargo test
cargo test --test architecture
cargo test --test runtime_helper_release_metadata
cargo test --test runtime_compliance
cargo test --test typescript_runtime_compliance
cargo test --test csharp_java_runtime_compliance
```

如果本机具备 C++ / Node 工具链，还应执行：

```bash
cmake -S runtime/cpp -B /tmp/configforge-runtime-cpp-review-build -DCONFIG_FORGE_RUNTIME_BUILD_TESTS=ON
cmake --build /tmp/configforge-runtime-cpp-review-build
ctest --test-dir /tmp/configforge-runtime-cpp-review-build --output-on-failure
npm pack runtime/typescript/reader_core --dry-run
npm pack runtime/typescript/json_reader --dry-run
npm pack runtime/typescript/messagepack_reader --dry-run
npm pack runtime/typescript/binary_reader --dry-run
npm pack runtime/typescript/loaders/node-fs --dry-run
```
