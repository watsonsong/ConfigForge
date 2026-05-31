# 07. Artifact 模块详细设计

本文档描述 `src/artifact/` 模块的实现边界。Artifact 模块负责统一管理 ConfigForge 的所有文件制品：runtime data、generated code、未来 POT / asset manifest 等聚合 data export 输出。Exporter 和 Codegen 只负责生成制品内容与相对目标路径；Build 负责收集制品并调用 Artifact Writer 执行路径冲突检查、dry-run、direct 写入或 temp-commit 写入。

## 7.1 设计目标

Artifact 模块需要解决：

1. 统一 runtime data 与 generated code 的制品模型，避免 exporter / codegen 各自定义写文件流程。
2. 在写入前集中检查输出路径冲突，覆盖跨 export task、runtime data 与 generated code 之间的冲突。
3. 支持 `dry-run`：完整执行配置加载、Meta link、data load、validation、serialization/codegen 和路径规划，但不写入最终文件。
4. 支持 `direct` 与 `temp-commit` 两种写入策略，并支持 `dry-run` 写入计划。
5. 生成稳定的写入结果：路径稳定排序、bytes/文本内容统一表达。
6. 保持模块依赖方向简单：artifact 是底层服务模块，不依赖 exporter、codegen、build、project、data、validation。

非目标：

- 不决定哪些 runtime data / codegen task 应该执行；task 编排属于 `build`。
- 不理解 Meta、DataSet、TargetTags 或运行时格式；语义生成属于 exporter / codegen。
- 不负责 CLI 参数解析；CLI 只把 dry-run / write mode 传给 build options。
- 第一阶段不实现远端上传、增量缓存、分布式 artifact store。

## 7.2 模块结构

建议新增目录模块：

```text
src/artifact/
  mod.rs          # 对外 API 与 re-export
  model.rs        # Artifact / ArtifactContent / ArtifactPath
  writer.rs       # ArtifactWriter / ArtifactWriteMode / dry-run / direct / temp-commit
  tests.rs        # 路径冲突、dry-run、写入策略单元测试
```

第一阶段可以先合并在 `mod.rs` 中实现，待类型膨胀后再拆文件；但公开 API 应按上述边界命名，避免后续迁移影响调用方。

## 7.3 核心类型

### 7.3.1 `Artifact`

```rust
pub struct Artifact {
    pub path: PathBuf,
    pub content: ArtifactContent,
}
```

语义：

- `path` 是最终输出路径，可以是绝对路径，也可以是 build 已按 project root / target output 解析后的路径。
- `content` 统一表达 binary/text 内容；写入层只处理路径、bytes 和写入策略，不理解制品业务来源。

### 7.3.2 `ArtifactContent`

```rust
pub enum ArtifactContent {
    Text(String),
    Binary(Vec<u8>),
}

impl ArtifactContent {
    pub fn into_bytes(self) -> Vec<u8>;
    pub fn len(&self) -> usize;
}
```

Codegen 可以返回 `Text`，runtime data exporter 可以返回 `Binary`。Artifact Writer 不关心文本编码转换之外的业务语义；文本统一以 UTF-8 写入。

### 7.3.3 `ArtifactWriteMode`

```rust
pub enum ArtifactWriteMode {
    DryRun,
    Direct,
    TempCommit,
}
```

语义：

- `DryRun`：检查路径冲突并返回写入计划，不创建目录、不写临时文件、不覆盖最终文件。
- `Direct`：创建父目录并直接写最终路径。
- `TempCommit`：先写入同目录临时文件，全部临时文件写入成功后再按稳定顺序 rename/commit；临时文件写入失败时清理已创建的临时文件，提交阶段失败可能保留已提交的最终文件。

### 7.3.4 `ArtifactWriteResult`

```rust
pub struct ArtifactWriteResult {
    pub mode: ArtifactWriteMode,
    pub entries: Vec<ArtifactWriteEntry>,
}
```

`entries` 必须按规范化后的 path 稳定排序，便于 CLI 输出和测试断言。Artifact Writer 只返回写入计划和写入结果，不生成业务产物。

## 7.4 Artifact Writer 流程

```text
Build collects Vec<Artifact>
  -> ArtifactWriter::new(mode)
  -> normalize and sort artifact paths
  -> ensure no duplicate final path
  -> if DryRun: return ArtifactWriteResult without filesystem writes
  -> if Direct: create parent dirs and write bytes
  -> if TempCommit: write same-directory temp files, then rename to final paths
```

路径冲突检查必须在任何写入前完成，错误信息包含冲突路径，方便用户定位 export task 配置问题。当前 Artifact Writer 的 final authority 使用 lexical-normalized path 作为冲突 key：它能消除 `.` / `..` 造成的字符串差异，但不会解析 symlink，也不会在 macOS / Windows 上做大小写折叠。因此 `Out/items.json` 与 `out/items.json` 在大小写不敏感文件系统上可能实际冲突但当前不会提前报 duplicate path；通过 symlink 指向同一目录的两个输出 root 也可能绕过 lexical check。中期方案是在写入前对已存在的父目录做 canonicalize，并在 macOS / Windows 或探测到大小写不敏感卷时引入可选 case-fold key；不存在的路径继续保留 lexical fallback，避免要求输出目录预先存在。平台细节见 [`../Tech/platform-filesystem-semantics.md`](../Tech/platform-filesystem-semantics.md)。

### 7.4.1 Phase 11：bounded artifact write pipeline

Phase 10 后，runtime table exporter、aggregate exporter 和 codegen 可以并行生成 artifact；如果继续让所有 generator 先返回 `Vec<Artifact>`，再一次性交给 `ArtifactWriter::write(Vec<Artifact>)`，大项目会在写入前同时持有大量 `Vec<u8>` / `String`，内存峰值接近“所有待写 artifact 内容之和 + 正在生成的 artifact 内容”。Phase 11 的目标是把“生成完成但尚未写入”的 artifact bytes 限制在可配置预算内，并让 generator 与 writer 形成 back-pressure。

设计目标：

1. 在任何真实写入前仍完成全局 path planning 与冲突检查；bounded pipeline 只改变内容生成与写入的流动方式，不降低 path conflict 的确定性。
2. 支持 CLI 指定固定内存容量或按当前可用内存百分比指定 artifact pipeline 预算。
3. 避免 CPU worker 在等待写入容量时长期占住 Rayon worker；当可能阻塞时优先把等待放到 dedicated coordinator / I/O writer 线程，或使用 Rayon `spawn_fifo`/异步 channel 让 CPU pool 继续调度其它可运行任务。
4. 保持 `DryRun`、`Direct`、`TempCommit` 语义不变；dry-run 不创建目录、不写临时文件，但仍执行 planning 和 byte-size 统计所需的生成逻辑。
5. 保持 result entries、commit 顺序与错误优先级稳定。

#### CLI / BuildOptions 内存预算

Phase 11 建议新增 artifact pipeline 预算选项：

| CLI 参数 | BuildOptions 字段 | 说明 |
| --- | --- | --- |
| `--artifact-memory-limit <SIZE>` | `artifact_memory_limit: Option<ByteSize>` | 固定容量预算，支持 `B`/`KiB`/`MiB`/`GiB` 与常见简写 `kb`/`mb`/`gb`。示例：`--artifact-memory-limit 512MiB`。 |
| `--artifact-memory-percent <PERCENT>` | `artifact_memory_percent: Option<u8>` | 按进程启动时探测到的“当前可用内存”计算预算，范围 `1..=80`，示例：`--artifact-memory-percent 20`。 |
| 未指定 | 默认 `artifact_memory_percent = 25` | 默认允许 artifact pipeline 使用启动时可用内存的 25%，并应用下方 min/max clamp。 |

参数优先级：

1. 如果同时指定 `--artifact-memory-limit` 与 `--artifact-memory-percent`，应报 CLI validation error，避免用户误以为两者会叠加。
2. 指定固定容量时直接使用该容量，但仍需要 `>= 1MiB` 的最小值校验；过小预算会让单个大 artifact 无法进入队列。
3. 指定百分比或使用默认百分比时，预算 = `available_memory_at_startup * percent / 100`，再 clamp 到 `[64MiB, 2GiB]`。如果平台无法探测可用内存，则 fallback 为 `256MiB`。
4. `--serial` 只影响 CPU / I/O 并发数量，不改变 artifact memory budget；用户仍可用固定容量参数获得可复现的内存上限。

选择“当前可用内存”而不是“总物理内存”的原因：CI、容器和开发机后台任务会让总内存百分比过于乐观；可用内存更接近本次 build 可安全占用的空间。实现上可以先使用跨平台系统信息库读取 available memory；读不到时不要失败，使用 fallback 并输出 warning。

#### Pipeline 模型

Phase 11 当前落地为 `ArtifactWriter::write(Vec<Artifact>)` 的 bounded writer queue：Build 仍先按声明顺序收集 `Vec<Artifact>` 并完成最终 path conflict check，然后 writer 在消费该 Vec 时使用 bounded queue、byte semaphore 和 dedicated writer thread 限制“已提交给 writer 但尚未写出”的 bytes。该实现保持现有 API 与测试兼容，并为后续 streaming generator 入口保留扩展空间。

当前 API：

```rust
pub struct ArtifactWriteLimits {
    pub max_in_flight_bytes: usize,
    pub max_in_flight_artifacts: usize,
}

impl ArtifactWriter {
    pub fn with_limits(self, limits: ArtifactWriteLimits) -> Self;
}
```

后续如果要进一步降低“生成阶段已经收集在 `Vec<Artifact>` 中”的内存峰值，应把当前 API 扩展为 streaming 写入入口，但保留现有 API 作为小项目和测试便利入口：

```rust
pub struct ArtifactWriteLimits {
    pub max_in_flight_bytes: usize,
    pub max_in_flight_artifacts: usize,
    pub writer_threads: usize,
}

pub trait ArtifactSink {
    fn submit(&self, artifact: Artifact) -> Result<()>;
    fn finish(self) -> Result<ArtifactWriteResult>;
}

impl ArtifactWriter {
    pub fn open_bounded(
        self,
        planned_paths: Vec<PlannedArtifactPath>,
        limits: ArtifactWriteLimits,
    ) -> Result<BoundedArtifactSink>;
}
```

当前 bounded writer 流程：

```text
Build / exporters / codegen
  -> generate and collect Vec<Artifact> in declaration order
  -> ArtifactWriter::new(mode).with_limits(limits).write(artifacts)
     -> final path normalization + conflict check before writes
     -> create bounded byte semaphore + sync_channel queue
  -> main thread drains planned artifacts
     -> acquire memory permit for artifact.content.len()
     -> enqueue to dedicated writer thread
  -> writer thread writes Direct temp/final files
     -> release memory permit immediately after bytes are written
  -> TempCommit: stable commit/rename after all temp writes succeed
  -> return stable ArtifactWriteResult
```

后续 streaming generator 流程：

```text
Build / exporters / codegen
  -> PlanArtifacts for all tasks
  -> ArtifactWriter::open_bounded(planned_paths, limits)
     -> final path normalization + conflict check before writes
     -> create bounded byte semaphore + bounded queue
  -> generators produce one artifact at a time
     -> submit(artifact)
        -> acquire memory permit for artifact.content.len()
        -> enqueue to writer queue
  -> writer threads write Direct temp/final files
     -> release memory permit immediately after bytes are written to OS/file
  -> finish()
     -> wait all queued writes
     -> TempCommit: stable commit/rename after all temp writes succeed
     -> return stable ArtifactWriteResult
```

关键约束：

- `planned_paths` 是写入前的完整 path authority；`submit()` 收到的 artifact path 必须命中 planned set，不能出现未规划路径，也不能重复提交同一 planned path。
- `submit()` 返回成功只表示 artifact 已被 writer 接收，不表示最终文件已 commit；最终错误由 `finish()` 统一返回。若 writer 已遇到错误，后续 `submit()` 应尽快返回同一个稳定错误并触发 cancellation。
- 当前 `max_in_flight_bytes` 统计“已经进入 writer queue、尚未被 writer 消费完”的 artifact content bytes；由于当前 Build 仍先收集 `Vec<Artifact>`，它不能限制已经生成并保存在 Vec 中的内容。后续 streaming generator 入口应把统计边界前移为“已经生成、尚未被 writer 消费完”的 artifact content bytes；并限制 generator 并发，避免 `jobs` 个超大 artifact 同时生成造成预算外峰值。
- `max_in_flight_artifacts` 防止大量小文件占满队列元数据；默认可取 `max(16, jobs * 4)`，但 bytes budget 是主限流。

#### 生成端限流与线程行为

仅在 `submit()` 内阻塞 Rayon worker 会产生两个问题：

1. CPU worker 被阻塞等待 writer 释放内存 permit，无法继续执行其它 ready tasks。
2. 如果所有 writer 或 coordinator 工作也依赖同一个 Rayon pool，可能出现 worker starvation。

因此 Phase 11 推荐采用“双层限流”：

1. **生成并发限流**：Build 在调度 artifact-producing tasks 时，为“可能生成大 artifact”的任务使用 `ArtifactGenerationLimiter`。默认并发可取 `min(jobs, max(1, max_in_flight_bytes / 64MiB))`，并至少保留 1 个生成槽。table exporter 已知每张表会产出一个 artifact，可以按 table task 粒度申请生成槽；aggregate exporter 若只能一次性产出多个 artifact，则视为一个大生成任务。
2. **写入队列限流**：`BoundedArtifactSink::submit()` 使用 byte semaphore + bounded queue。如果 queue 满或 byte permit 不足，submitter 不应在 Rayon worker 上长期 busy-wait；实现应优先使用 `std::sync::Condvar` / channel parking，让线程睡眠并释放 CPU 时间片。更进一步，Build 可以把 submit 操作交给 dedicated coordinator thread：Rayon worker 只把生成结果移交给 coordinator 的小容量 channel；当 channel 满时，生成端的 limiter 会阻止继续启动新任务，而不是让所有 CPU worker 堵在提交点。

线程所有权策略：

- **短期实现（无 async runtime）**：使用 dedicated writer thread(s) + `Mutex/Condvar` byte semaphore。等待 permit 的线程会 park，不消耗 CPU；为了避免占满 Rayon worker，生成任务数量由 `ArtifactGenerationLimiter` 控制在较小范围。此方案简单、跨平台、与当前同步文件写入模型一致。
- **中期 DAG executor**：把 `RunGenerator` 与 `WriteArtifact` 拆成不同 resource lane。`RunGenerator` 在 CPU lane；`WriteArtifact` 在 I/O lane；当 byte budget 不足时，node 保持 `PendingCapacity` 状态并交还 worker，等待 writer 释放 permit 后由 readiness queue 重新唤醒。此时不需要阻塞 Rayon worker。
- **不推荐**：在 CPU pool 内无限制 `par_iter()` 生成 artifact，然后在 `submit()` 阻塞等待 byte permit。这只能限制 writer queue，不能限制“正在生成”的内存峰值。

#### Direct / TempCommit / DryRun 细节

- `DryRun`：仍执行 full build、validation、artifact planning 和内容生成，以获得准确 bytes；`submit()` 只统计 entries 并释放 permit，不创建目录、不写文件。若后续需要纯 path-only dry-run，可另加 `--dry-run=plan-only`，不能改变现有 dry-run 语义。
- `Direct`：writer thread 写入最终路径后立即释放该 artifact 的 byte permit；如果写入失败，记录第一个稳定错误、通知 cancellation，并尽量 drain / reject 后续 submit。
- `TempCommit`：writer thread 先写同目录 temp file，写完 temp file 即可释放 content bytes permit；所有 temp writes 成功后，`finish()` 按 planned path 稳定顺序 rename。commit 阶段不再持有 artifact content bytes，因此内存峰值只与 temp write 队列有关。
- `TempCommit` 清理：temp write 阶段失败时清理已创建 temp files；commit 阶段失败时沿用当前语义，可能已有部分最终文件被 rename 成功，但错误路径必须稳定。

#### 超大单文件策略

如果某个 artifact 的 `content.len()` 大于 `max_in_flight_bytes`：

1. 默认允许“单 artifact 例外”：独占 byte budget，等所有其它 in-flight artifact 写完后提交该 artifact，并在 warning 中提示该文件超过预算。
2. 如果用户显式设置 `--artifact-memory-limit` 且该值小于单个 artifact，仍允许单 artifact 例外；否则构建会因为任何单个大文件无法写入而不可用。需要严格失败时可在未来增加 `--artifact-memory-strict`。
3. 长期优化是让 runtime exporter / codegen 支持 streaming encoder，直接写入 `ArtifactByteWriter`，避免单 artifact 也必须完整驻留内存。

## 7.5 与 Exporter 的关系

Exporter 的职责从“序列化并写文件”收敛为“序列化并返回 runtime data artifacts”：

```rust
pub(crate) fn export_runtime_data(...) -> Result<Vec<RuntimeDataArtifact>>;

pub struct RuntimeDataArtifact {
    pub file: RuntimeDataFile,
    pub artifact: Artifact,
}
```

其中：

- `RuntimeDataFile` 是 runtime artifact metadata / manifest contract；当前默认 codegen 不消费真实 artifact path，而是使用逻辑 table name。
- `Artifact.path` 是实际写入路径，由 export task output root + runtime data file path 得到。
- Exporter 不再调用 `fs::create_dir_all` / `fs::write`，也不选择 `ArtifactWriteMode`。

正式 build 流程统一由 build 收集 exporter/codegen artifacts，再按 `BuildOptions.artifact_write_mode`
调用 `ArtifactWriter` 写出；测试若需要落盘断言，应在测试模块内定义辅助函数，不应作为 exporter
正式 API 暴露。

## 7.6 与 Codegen 的关系

Codegen 继续返回生成文件列表，但文件模型应迁移到 artifact 模块或可无损转换为 artifact：

```rust
pub struct GeneratedFile {
    pub path: PathBuf,
    pub content: String,
}
```

后续可直接替换为：

```rust
pub type GeneratedFile = Artifact;
```

第一阶段推荐保留 `GeneratedFile` 作为 codegen 语义类型，Build 在写入前统一转换为 `ArtifactContent::Text`。

## 7.7 与 Build / CLI 的关系

Build 是唯一编排者：

1. CLI 解析 `--dry-run`，写入 `BuildOptions`。
2. Build 正常执行 project/meta/data/validation/exporter/codegen。
3. Build 收集 runtime data artifacts 与 generated code artifacts。
4. Build 调用 `ArtifactWriter`。
5. Build 返回 `ArtifactWriteResult`；dry-run 模式下 CLI 输出 artifact plan，但不写文件。

CLI dry-run 不应跳过 validation 或 serialization/codegen，因为用户需要确认完整构建能成功，只是不提交文件系统变更。

## 7.8 错误边界

第一阶段可复用 `ConfigForgeError`：

| 场景 | 错误 |
| --- | --- |
| 重复 artifact path | `ConfigForgeError::Validation` |
| 创建目录失败 | `ConfigForgeError::WriteFile` |
| 写最终文件失败 | `ConfigForgeError::WriteFile` |
| temp commit rename 失败 | `ConfigForgeError::WriteFile`；已 rename 成功的文件可能保留在最终路径 |

后续如果 artifact 诊断增多，可新增 `ArtifactDiagnostic`，但不应让 exporter/codegen 为写入错误各自包装一套错误类型。

## 7.9 测试计划

Artifact 模块单元测试应覆盖：

1. 相同 path 的 artifact 在写入前报错。
2. dry-run 不创建父目录、不写文件，但返回稳定排序 entries。
3. direct 模式创建父目录并写入 text/binary 内容。
4. write entries 按 path 稳定排序。
5. runtime data artifact 与 generated code artifact 冲突时由统一 writer 报错。
6. temp-commit 模式先写临时文件再提交最终路径，写临时文件失败时清理临时文件。

集成测试应覆盖：

1. CLI `--dry-run` 对标准工程执行成功并输出 artifact plan，但不产生输出文件。
2. 正常 build 仍产生 JSON / MessagePack / generated code。
3. data output 与 code output 指向同一路径时报统一 artifact conflict。
4. CLI `--write-mode temp-commit` 产生与 direct 模式等价的最终 artifact。

## 7.10 分阶段落地

### Phase 1：文档与边界

- 新增本文档。
- 调整 Implementation 目录，把 Artifact 作为模块文档插入 Exporter / Codegen / Build 之前。
- 修正模块依赖图和文档索引。

### Phase 2：最小 Artifact 模块

- 新增 `src/artifact/`。
- 实现 `Artifact`、`ArtifactContent`、`ArtifactWriter`、`ArtifactWriteMode::DryRun/Direct`。
- Build generated code 写入改走 Artifact Writer。

### Phase 3：Runtime data 迁移

- Exporter 返回 runtime data artifacts，不再直接写文件。
- Build 统一收集 runtime data + generated code，再一次性调用 Artifact Writer。
- 删除 build/exporter 中重复路径冲突检查或迁移到 artifact writer。

### Phase 4：CLI dry-run 与 temp-commit

- CLI 增加 `--dry-run`。
- `BuildOptions` 增加 artifact write mode / dry-run 标志。
- 实现 direct、dry-run 与 temp-commit 集成测试。
- temp-commit 当前提供临时文件阶段保护；提交阶段失败按 `ConfigForgeError::WriteFile` 报错，并可能保留已提交的最终文件。
