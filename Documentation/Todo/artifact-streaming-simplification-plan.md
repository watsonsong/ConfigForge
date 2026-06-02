# Artifact Streaming 简化重构 TODO

> 状态：短期执行计划。本文档记录本轮讨论后的分步 TODO，不作为长期 canonical 设计；每个阶段完成后，应把最终状态合并回 `Documentation/Implementation/07-artifact-module-detail-design.md`、`Documentation/Implementation/08-exporter-module-detail-design.md`、`Documentation/Implementation/09-codegen-module-detail-design.md` 和 `Documentation/Implementation/10-build-module-detail-design.md`。

## 1. 背景总结

当前 Artifact / Build / Exporter / Codegen 已从“收集全量 `Vec<Artifact>` 后统一写出”迁移到 shared `ArtifactSink`：

- Build 在 DAG 执行前创建 shared sink，并在成功后调用 `sink.finish()`。
- Runtime table exporter 已通过 `ArtifactProducer` / `ArtifactWriteStream` 写入 sink。
- JSON / MessagePack 已有 writer-style serializer。
- Codegen 仍通过 `generate_code_task()` materialize `Vec<GeneratedFile>`，再写入 sink；入口是 `src/build/mod.rs:621` 附近的 `generate_and_write_code_task()`。
- `CodeGenerator::generate()` 当前仍返回 `Vec<GeneratedFile>`，见 `src/codegen/mod.rs:411`。
- `CodeWriter` 当前仍是 `String` backed，见 `src/codegen/writer.rs:64`。

本轮讨论后的结论：前一轮“ArtifactSink 同时承担 byte-level memory semaphore、generation memory budget、未来 async / seekable / external path adapter”等设计偏复杂。接下来应把 Artifact 模块收敛为更简单、更稳定的边界：**ArtifactSink 是标准文件流生命周期管理器，不是通用内存调度器。**

## 2. 最终决策

### 2.1 放弃 memory limit 支持

最终目标是删除对 artifact memory limit 的用户可见支持，包括：

- `--artifact-memory-limit`
- `--artifact-memory-percent`
- `ArtifactMemoryOptions`
- `ArtifactWriteLimits`
- `ArtifactGenerationLimiter`
- sink 内部 byte semaphore / artifact semaphore
- build trace 中 artifact memory 预算字段

原因：

1. 真正的大内存来源是生成端一次性构建完整 `String` / `Vec<u8>`，而不是 ArtifactSink 内部写入。
2. 如果 ArtifactSink 直接返回标准文件流，sink 本身不持有完整 artifact 内容，byte-level semaphore 的收益有限。
3. 资源控制应优先通过“避免 materialization”和“生成任务自然受 Rayon jobs 限制”解决，而不是维护一套看似 hard limit、实际不能覆盖总 RSS 的复杂预算。

### 2.2 ArtifactSink 只负责标准流和生命周期

目标语义：

```text
ArtifactSink::create(path)
  -> DryRun: 返回只统计 bytes 的伪输出流
  -> Direct: 返回真实 final file 输出流
  -> TempCommit: 返回同目录 temp file 输出流
```

所有最终 artifact 都应通过 ArtifactSink 打开和完成：

```rust
let mut stream = sink.create(path)?;
third_party_or_own_writer.write_to(&mut stream)?;
stream.finish()?;
```

ArtifactSink 必须继续负责：

- output path lexical normalization；
- path reservation / duplicate conflict；
- DryRun counting；
- Direct final file creation；
- TempCommit temp file creation、commit、cleanup；
- sink drop 时清理已记录 temp files；
- stable `ArtifactWriteResult.entries`。

### 2.3 标准 `std::io::Write` 是当前唯一一等兼容接口

短期只承诺兼容标准同步文件流：

- JSON / MessagePack / bin / Lua / POT / manifest / codegen 都优先写入 `std::io::Write`。
- 支持 `std::io::Write` 的第三方库直接写 `ArtifactWriteStream`。
- 暂不实现 async writer、seekable writer、path-only third-party adapter、directory artifact。

如未来确实遇到真实格式需求，再按需扩展，不提前设计到主路径中。

### 2.4 内存优化方向

内存优化不再靠 `max_in_flight_bytes`，而靠：

1. Runtime data exporter 避免 `to_vec`，直接使用 writer API。
2. Aggregate exporter 避免返回 `Vec<Artifact>`，直接写 sink。
3. Codegen 避免 `String` backed `CodeWriter`，逐步迁移为 streaming text writer。
4. Build 不收集完整 generated contents，只调度生成任务并让生成器写入 sink。

## 3. 分步 TODO

每一步都应独立可测试、可提交 git；不要把 API 删除、writer 迁移、CLI 删除混在一个大提交中。Step 0 已基本由现有测试与 `ArtifactSinkInner::Drop` 兜底清理补齐，后续可以从 Step 1 开始推进。

### Step 0：锁定当前行为回归测试 `[x]`

目标：在删减复杂逻辑前，先确保核心 ArtifactSink 行为有测试保护。

开发内容：

- 补齐 / 确认 DryRun 不创建目录和文件，只统计 bytes。
- 补齐 / 确认 Direct 写真实 final file。
- 补齐 / 确认 TempCommit 在 `sink.finish()` 前不暴露 final file。
- 补齐 / 确认 TempCommit `sink.finish()` 成功后 rename 到 final path。
- 补齐 / 确认 TempCommit stream drop without finish 会失败并清理 temp file。
- 补齐 / 确认 TempCommit sink drop without finish 会清理已 finish stream 的 temp file。
- 补齐 / 确认 duplicate normalized path 会失败，并使最终 `finish()` 失败。

建议测试位置：`src/artifact/mod.rs` 单元测试模块。

验收命令：

```bash
cargo test artifact::tests::
```

建议提交：

```text
test(artifact): lock sink stream lifecycle behavior
```

### Step 1：简化 ArtifactSink 内部流实现，移除 sink 级 semaphore `[x]`

目标：ArtifactSink 内部只保留 path registry、mode dispatch 和 file/count writer，不再做 byte / artifact in-flight 控制。

开发内容：

- 从 `ArtifactSinkInner` 移除 `byte_semaphore`、`artifact_semaphore`。
- 删除 `ByteSemaphore`、`ArtifactSemaphore` 及其状态结构。
- `ArtifactSink::create()` 不再 acquire artifact permit。
- `ArtifactWriteStream::write()` 直接转发到底层 target：DryRun / Count 累计 `buf.len()`；Direct / TempCommit 调用 `File::write()`。
- 删除 `ArtifactWriteStream::artifact_permit_held` 和相关 release 逻辑。
- 保留 `ArtifactWriteStream::Drop` 中对未 finish stream 的失败标记和 temp file 删除。
- 保留 `ArtifactSinkInner::Drop` 中的 temp file 兜底清理。

不在本步骤删除 public `ArtifactWriteLimits` / CLI 参数，避免一次改动过大。若 API 暂时仍接收 limits，应明确它在 sink 层不再参与 byte / artifact semaphore。

单元测试：

- 现有 ArtifactSink 模式测试应全部通过。
- 删除或重写依赖 “small queue / bounded write” 的测试。
- 改为断言大文件可正常通过标准 stream 写入。

验收命令：

```bash
cargo test artifact::tests::
cargo check
```

建议提交：

```text
refactor(artifact): simplify sink streams to standard writers
```

### Step 2：删除 ArtifactWriteLimits 与 ArtifactGenerationLimiter `[x]`

目标：彻底移除 artifact memory limit 的核心类型和内部调用。

开发内容：

- 删除 `ArtifactWriteLimits`。
- 删除 `ArtifactGenerationLimiter` / `ArtifactGenerationPermit`。
- 删除 `ArtifactPlan.estimated_generation_bytes` 字段，或将 `ArtifactPlan` 简化为仅包含 `path`。
- 调整 `ArtifactProducer`：保留 producer 便利接口，但只表达 path + produce，不再携带 generation byte estimate。
- 调整 `ArtifactWriter`：删除 `limits` 字段、删除 `with_limits()`，`open_sink()` 只接收 `ArtifactWriteMode`。
- 调整 runtime exporter / build 中所有 `generation_limiter` 参数和调用。
- `generate_and_write_code_task()` 直接调用 generator / sink，不再包 `run_unknown()`。

单元测试：

- 删除 `artifact_write_limits_reject_zero_values` 等 limits 测试。
- 删除 `generation_limiter_blocks_until_generation_permit_is_released` 测试。
- Runtime table exporter 测试改为只传 sink，不传 limiter。
- Build runtime 并发测试仍可保留，但只验证任务并行，不验证 generation limiter。

验收命令：

```bash
cargo test artifact::tests:: exporter::tests:: build::runtime::tests::
cargo check
```

建议提交：

```text
refactor(artifact): remove memory limit primitives
```

### Step 3：删除 CLI / BuildOptions 中的 memory limit 支持 `[ ]`

目标：删除用户可见 memory limit 参数，避免保留误导性配置。

开发内容：

- 删除 CLI 参数：`--artifact-memory-limit`、`--artifact-memory-percent`。
- 删除 `ArtifactMemoryOptions`。
- 删除 `BuildOptions.artifact_write_limits`。
- 删除 `BuildTrace.artifact_memory_bytes` 或类似字段。
- 更新 trace JSON fixture / integration assertions。
- 删除或改写 memory options parse / validation tests。
- 删除或改写 conflicting memory options integration test。
- 删除或改写 artifact memory limit flag integration test。
- 更新 CLI help / docs 中所有 memory limit 描述。

注意：如果 trace 输出是用户可见格式，删除字段可能是 breaking change，需要在提交说明中明确。`--serial` / `--jobs` 仍保留，用于控制 CPU 并行度。

验收命令：

```bash
cargo test build::options::tests::
CARGO_NET_OFFLINE=true cargo test --test cli_integration
cargo check
```

建议提交：

```text
refactor(cli): remove artifact memory limit options
```

### Step 4：将 DataFormatExporter 的 stream API 设为主路径 `[x]`

目标：runtime data table exporter 不再依赖 `serialize_table() -> Vec<u8>` fallback 作为主设计。

开发内容：

- 保留 `serialize_table_to_stream()` 作为主接口。
- 将 `serialize_table() -> Vec<u8>` 标记为 legacy fallback，或在内置 exporter 全迁移后删除。
- 确认 JSON 使用 `serde_json::to_writer_pretty()`。
- 确认 MessagePack 使用 `rmp_serde::encode::write_named()`。
- Binary exporter 优先写 `ArtifactWriteStream`，避免先构建完整 `Vec<u8>`；如果当前 binary 仍有内部小 buffer，应在本步骤评估是否能流式写。
- `export_runtime_data_to_sink()` 直接 plan path、`sink.create(path)`、`exporter.serialize_table_to_stream(..., &mut stream)`、`stream.finish()`。

单元测试：

- JSON / MessagePack / bin exporter 原有内容测试全部通过。
- 新增一个 custom exporter 测试，验证只实现 stream API 也可工作。
- DryRun bytes 与 Direct bytes 保持一致。

验收命令：

```bash
cargo test exporter::tests:: exporter::json::tests:: exporter::messagepack::tests:: exporter::binary::tests::
```

建议提交：

```text
refactor(exporter): prefer streaming table serialization
```

### Step 5：将 AggregateDataExporter 改为 sink-first `[ ]`

目标：gettext POT、Lua、manifest 等 aggregate exporter 不再返回 `Vec<Artifact>` 作为主路径。

开发内容：

- `AggregateDataExporter::export_to_sink()` 成为 required 方法。
- `export() -> Vec<Artifact>` 改为 legacy helper 或删除。
- gettext POT exporter 直接创建 stream 写入。
- Lua table exporter 直接创建多个 stream 写入。
- 若某些 aggregate exporter 仍必须生成 String，先局部保留，但不要通过 `Vec<Artifact>` 回到 build。

单元测试：

- gettext POT 内容测试。
- Lua table 多文件 artifact plan / content 测试。
- aggregate exporter duplicate path 测试仍通过。
- DryRun entries 与 Direct 输出一致。

验收命令：

```bash
cargo test exporter::gettext_pot::tests:: exporter::lua_table::tests:: build::runtime::tests::
```

建议提交：

```text
refactor(exporter): make aggregate exports sink-first
```

### Step 6：为 Codegen 增加 sink-first API，但先使用 fallback `[ ]`

目标：让 build 层不再直接依赖 `Vec<GeneratedFile>`，为后续 streaming codegen 铺路。

开发内容：

- 在 `CodeGenerator` trait 上新增 `generate_to_sink(&self, input, sink)`。
- 默认实现仍调用 `generate(input)?`，然后 `sink.write_text(file.path, &file.content)`。
- Build 层 `generate_and_write_code_task()` 改为调用 `generator.generate_to_sink(&input, context.sink)`。
- 暂时保留 `generate()` 和 `GeneratedFile`，确保行为不变。
- 抽出 `build_code_export_input()` helper，减少 build code 重复。

单元测试：

- 所有 codegen snapshot / behavior tests 不变。
- CLI codegen integration tests 不变。
- 可新增 fake generator 测试，确认 build 使用 `generate_to_sink()` 覆盖实现而不是强制走 fallback。

验收命令：

```bash
cargo test codegen::tests:: build::tests::
CARGO_NET_OFFLINE=true cargo test --test cli_integration codegen
```

建议提交：

```text
refactor(codegen): add sink-first generation API
```

### Step 7：新增 streaming text writer `[ ]`

目标：为 codegen 消除 full `String` materialization 提供文本写入工具。

开发内容：

- 新增 `StreamingCodeWriter<W: std::io::Write>`。
- 保持与现有 `CodeWriter` 接近的 API：`raw()`、`raw_fmt()`、`write()`、`line()`、`line_fmt()`、`blank_line()`、`indent()` / `dedent()` / `indented()`、`finish()`。
- 方法可以返回 `Result<()>`，将 `io::Error` 转换为 `ConfigForgeError::WriteFile` 的边界由调用方 stream 处理。
- 明确 trailing newline 策略：第一版可以只支持追加 trailing newline / blank line；如果需要完全兼容 `trailing_newline = false`，实现小型 tail newline buffer，不持有完整文件内容。
- 保留现有 `CodeWriter` 供 tests / legacy `generate()` 使用。

单元测试：

- `StreamingCodeWriter<Vec<u8>>` 与 `CodeWriter` 在常见 formatting 场景输出一致。
- indent / dedent / blank line / trailing newline 测试。
- 可用 custom failing writer 测试错误传播。

验收命令：

```bash
cargo test codegen::writer::tests::
```

建议提交：

```text
feat(codegen): add streaming code writer
```

### Step 8：逐个迁移内置 Codegen backend 到 streaming `[ ]`

目标：Rust / TypeScript / C++ generator 覆盖 `generate_to_sink()`，不再在 production build 中构建完整 generated file `String`。

推荐顺序：

1. Rust backend：单文件，最适合作为样板。
2. TypeScript backend：单文件或较少文件，跟随 Rust 模式。
3. C++ backend：多文件，需要 header/source 分别打开 stream。

每个 backend 的开发内容：

- 保持原 `generate()` 不变，确保现有测试可继续使用 String-backed 输出。
- 将 render helper 从只接受 `&mut CodeWriter` 逐步泛化，或新增 streaming render path。
- `generate_to_sink()` 先 build `CodeModel` / names / validate symbols，计算 output paths，再 `sink.create(path)`，用 `StreamingCodeWriter` render，最后 `writer.finish()` 与 `stream.finish()`。
- 确保所有可预知错误在打开 stream 前完成，以减少 Direct 模式 partial file 风险。

单元测试：

- 每个 backend 增加 equivalence 测试：legacy `generate()` 输出与 `generate_to_sink()` 写入内容一致。
- C++ 额外测试 header/source 都写出。
- DryRun codegen entries bytes 与 Direct 内容长度一致。

验收命令：

```bash
cargo test codegen::rust::tests::
cargo test codegen::typescript::tests::
cargo test codegen::cpp::tests::
CARGO_NET_OFFLINE=true cargo test --test cli_integration
```

建议拆成 3 个提交：

```text
refactor(codegen): stream rust generation to sink
refactor(codegen): stream typescript generation to sink
refactor(codegen): stream cpp generation to sink
```

### Step 9：删除 legacy generated content path `[ ]`

目标：内置生成器全部 sink-first 后，删除 production path 中的 full String / Vec materialization。

开发内容：

- 删除或降级 `GeneratedFile`；如果测试仍需要，可放到 `#[cfg(test)]` helper。
- `CodeGenerator::generate()` 删除或仅保留 test-only extension helper。
- Build 层删除 `generate_code_task() -> Vec<GeneratedFile>`。
- 所有 codegen artifact 写入统一通过 `generate_to_sink()`。

单元测试：

- 所有 codegen backend tests 改用 sink-backed helper 或 test-only collector。
- CLI integration tests 全部通过。

验收命令：

```bash
cargo test codegen::tests:: codegen::rust::tests:: codegen::typescript::tests:: codegen::cpp::tests::
CARGO_NET_OFFLINE=true cargo test
```

建议提交：

```text
refactor(codegen): remove generated file materialization path
```

### Step 10：整理文档与最终状态 `[ ]`

目标：把 TODO 执行结果合并进 canonical 文档，并删除过时描述。

开发内容：

- 更新 `Documentation/Implementation/07-artifact-module-detail-design.md`：ArtifactSink 是标准流生命周期管理器，不再描述 memory limit / byte semaphore。
- 更新 `Documentation/Implementation/08-exporter-module-detail-design.md`：table exporter stream-first，aggregate exporter sink-first。
- 更新 `Documentation/Implementation/09-codegen-module-detail-design.md` 和 `09h-codegen-generator-api-and-tests.md`：codegen sink-first / streaming writer。
- 更新 `Documentation/Implementation/10-build-module-detail-design.md`：build 不再传 artifact memory limits / generation limiter；build 只创建 shared sink 并传入 exporters/codegen。
- 更新 `Documentation/Tech/task-system-design.md`：删除 artifact memory budget 相关规划；保留 `jobs` / Rayon 并发策略。
- 更新 `Documentation/README.md`：若 Todo 文档执行完毕，应删除或归档本文档，避免 Todo 成为长期 canonical area。

验收命令：

```bash
git grep -n "artifact-memory\|ArtifactWriteLimits\|ArtifactGenerationLimiter\|GeneratedFile" Documentation src
cargo test
```

建议提交：

```text
docs: align artifact streaming simplification design
```

## 4. 全局验收标准

最终重构完成后，应满足：

- `git grep -n "ArtifactWriteLimits\|ArtifactGenerationLimiter\|artifact-memory" src tests` 无生产路径残留。
- Build 不再提供 artifact memory limit CLI 参数。
- ArtifactSink 内部不再包含 byte semaphore / artifact semaphore。
- 所有最终输出仍通过 ArtifactSink。
- DryRun 不创建最终输出，并能统计写入 bytes。
- TempCommit 成功才提交 final file，失败 / drop 清理 temp files。
- Runtime data generation 不通过完整 `Vec<u8>` 主路径。
- Aggregate exporter 不通过 `Vec<Artifact>` 主路径。
- Codegen production path 不构造完整 generated `String`。
- 所有内置格式仍通过现有 CLI integration tests。

## 5. 每步提交前检查清单

每个阶段提交前至少运行：

```bash
cargo fmt
cargo check
git diff --check
```

根据影响范围运行对应测试；涉及 exporter / build / CLI 时，优先运行：

```bash
CARGO_NET_OFFLINE=true cargo test
```

提交粒度保持“一步一个目标”：测试锁定、ArtifactSink 简化、memory limit 删除、exporter streaming、aggregate sink-first、codegen sink-first、backend streaming、legacy 删除、文档收口分别提交。
