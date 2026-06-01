# 07. Artifact 模块详细设计

本文档描述 `src/artifact/` 模块的实现边界。Artifact 模块负责统一管理 ConfigForge 的所有文件制品：runtime data、generated code、未来 POT / asset manifest 等聚合 data export 输出。Exporter 和 Codegen 只负责生成制品内容与相对目标路径；Build 负责创建统一的 Artifact 写入协调器，由协调器打开每个 artifact 的写入流，并执行路径冲突检查、dry-run、direct 写入或 temp-commit 写入。

## 7.1 设计目标

Artifact 模块需要解决：

1. 统一 runtime data 与 generated code 的制品模型，避免 exporter / codegen 各自定义写文件流程。
2. 通过中心协调器集中占用输出路径并检查冲突，覆盖跨 export task、runtime data 与 generated code 之间的冲突。
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
  writer.rs       # ArtifactWriter / ArtifactSink / ArtifactWriteMode / dry-run / direct / temp-commit
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

### 7.3.5 `ArtifactSink`

streaming 化的主入口是中心协调器 `ArtifactSink`，而不是让 exporter / codegen 自行持有文件句柄或绕过 artifact 模块写文件：

```rust
pub struct ArtifactSink { /* private */ }

impl ArtifactSink {
    pub fn create(&self, path: impl Into<PathBuf>) -> Result<ArtifactWriteStream>;
    pub fn write_text(&self, path: impl Into<PathBuf>, content: impl AsRef<str>) -> Result<()>;
    pub fn write_binary(&self, path: impl Into<PathBuf>, content: impl AsRef<[u8]>) -> Result<()>;
    pub fn finish(self) -> Result<ArtifactWriteResult>;
}

pub struct ArtifactWriteStream { /* private */ }

impl std::io::Write for ArtifactWriteStream { /* ... */ }

impl ArtifactWriteStream {
    pub fn finish(self) -> Result<ArtifactWriteEntry>;
}
```

语义：

- `create(path)` 是唯一打开 artifact 写入流的入口。它会规范化 final path，在协调器的全局 registry 中原子占用该 path，并根据 `ArtifactWriteMode` 返回 direct / temp / counting writer。
- `write_text()` / `write_binary()` 是 convenience API，内部等价于 `create(path) -> write_all(...) -> finish()`；它们不是另一套写入机制。
- `ArtifactWriteStream` 实现 `std::io::Write`，使 JSON / MessagePack / bin / Lua / codegen 都可以使用同一套 `write_all()` / serializer-to-writer 接口。
- `finish()` 关闭当前 stream，记录 bytes 与临时文件路径；`ArtifactSink::finish()` 等待所有 stream 完成，并在 `TempCommit` 模式执行统一 commit。
- `ArtifactWriter::write(Vec<Artifact>)` 保留为兼容入口，但实现上应转调 `ArtifactSink`，避免长期维护两套写入路径。

中心协调器内部维护的状态建议为：

```rust
struct ArtifactSinkState {
    paths: BTreeMap<PathBuf, ArtifactPathState>,
    entries: BTreeMap<PathBuf, ArtifactWriteEntry>,
    temp_files: BTreeMap<PathBuf, PathBuf>,
    first_error: Option<ConfigForgeError>,
}

enum ArtifactPathState {
    Open,
    Finished,
    Failed,
}
```

`paths` 的 key 必须使用 artifact 模块统一的 path normalization 结果；后续 filesystem-aware key 也应只在 artifact 模块中实现，不能让 build / exporter / codegen 各自维护不同规则。

## 7.4 Artifact Writer 流程

```text
Build creates ArtifactSink
  -> exporter/codegen asks sink.create(path)
  -> sink normalizes and reserves final path atomically
  -> task writes bytes through ArtifactWriteStream::write_all / serializer.write_to
  -> stream.finish records bytes and stream status
  -> sink.finish returns stable ArtifactWriteResult
     -> if DryRun: no directories/files are created
     -> if Direct: files have been written to final paths
     -> if TempCommit: all temp files are renamed to final paths in stable order
```

路径冲突检查不要求 build 一开始预判所有 artifact path；但所有写入都必须经过同一个 `ArtifactSink`。`create(path)` 在写入流打开前完成 path normalization 与 registry 占用：如果 normalized final path 已经处于 `Open` / `Finished` / `Failed` 任一状态，立即返回 duplicate artifact path 错误。这样可以在生成过程中动态发现跨 task 冲突，同时避免多个任务绕过协调器直接写同一 final path。

当前 Artifact Writer 的 final authority 使用 lexical-normalized path 作为冲突 key：它能消除 `.` / `..` 造成的字符串差异，但不会解析 symlink，也不会在 macOS / Windows 上做大小写折叠。因此 `Out/items.json` 与 `out/items.json` 在大小写不敏感文件系统上可能实际冲突但当前不会提前报 duplicate path；通过 symlink 指向同一目录的两个输出 root 也可能绕过 lexical check。中期方案是在 `ArtifactSink::create()` 对已存在的父目录做 canonicalize，并在 macOS / Windows 或探测到大小写不敏感卷时引入可选 case-fold key；不存在的路径继续保留 lexical fallback，避免要求输出目录预先存在。平台细节见 [`../Tech/platform-filesystem-semantics.md`](../Tech/platform-filesystem-semantics.md)。

### 7.4.1 中心协调式 ArtifactSink pipeline

runtime table exporter、aggregate exporter 和 codegen 可以并行生成 artifact。旧模型要求所有 generator 先返回 `Vec<Artifact>`，再一次性交给 `ArtifactWriter::write(Vec<Artifact>)`；大项目会在写入前同时持有大量 `Vec<u8>` / `String`，内存峰值接近“所有待写 artifact 内容之和 + 正在生成的 artifact 内容”。`ArtifactSink` pipeline 的目标是把写入边界前移：generator 不再把完整内容交给 build 收集，而是向中心协调器申请一个 artifact 写入流，然后直接通过 `Write` 接口序列化/写入。

设计目标：

1. 用一个中心协调器统一 path reservation、dry-run、direct、temp-commit、byte accounting 和 result entry 生成，避免 `Vec<Artifact>` writer 与 streaming writer 长期并存。
2. 不强制 build 一开始预判所有 artifact path；path 冲突可以在 `ArtifactSink::create(path)` 时动态发现，但发现冲突后必须阻断当前 stream，并使最终 `finish()` 返回稳定错误。
3. 支持 CLI 指定固定内存容量或按当前可用内存百分比指定 artifact pipeline 预算；预算限制“已经写入 sink 但尚未被底层文件系统/计数器消费”的 bytes。
4. 避免 CPU worker 在等待写入容量时 busy-wait；短期允许同步阻塞/parking，线程数量可略高于 CPU 核心数以覆盖本地文件 I/O 等待；中长期在 DAG executor 中把 writer lane 与 CPU lane 分离。
5. 保持 `DryRun`、`Direct`、`TempCommit` 语义稳定；dry-run 不创建目录、不写临时文件，但仍执行内容生成并通过 counting writer 获得准确 bytes。
6. 保持 result entries、commit 顺序与错误优先级稳定。

#### CLI / BuildOptions 内存预算

当前 artifact pipeline 预算选项：

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

当前 artifact 模块已经以 `ArtifactSink` 作为唯一写入协调器；`ArtifactWriter::write(Vec<Artifact>)` 是兼容入口，负责遍历旧 artifact 列表，对每个 artifact 调用 `sink.write_text()` / `sink.write_binary()`，最后调用 `sink.finish()`。旧的 dedicated bounded writer queue 已移除；byte / artifact 限流现在由 `ArtifactWriteStream` 与 sink 内部 semaphore 承担。Build 主流程已创建共享 `ArtifactSink` 并传入 runtime/codegen 分支，同时创建 `ArtifactGenerationLimiter` 限制 generation 阶段 materialized payload 的并发预算。runtime table exporter 写入 sink，JSON / MessagePack 已覆盖 writer-style serializer；codegen 短期仍生成 `String`，随后在 generation limiter permit 内调用 `sink.write_text()`。aggregate exporter trait 仍保留 `Vec<Artifact>` 兼容边界，由 build runtime wrapper 在 generation limiter permit 内立即写入 sink。

已实现 API：

```rust
pub struct ArtifactWriteLimits {
    pub max_in_flight_bytes: usize,
    pub max_in_flight_artifacts: usize,
}

impl ArtifactWriter {
    pub fn with_limits(self, limits: ArtifactWriteLimits) -> Self;
    pub fn open_sink(self) -> ArtifactSink;
    pub fn write(self, artifacts: Vec<Artifact>) -> Result<ArtifactWriteResult>; // compatibility wrapper
}

impl ArtifactSink {
    pub fn create(&self, path: impl Into<PathBuf>) -> Result<ArtifactWriteStream>;
    pub fn write_producer<P: ArtifactProducer>(&self, producer: P) -> Result<ArtifactWriteEntry>;
    pub fn finish(self) -> Result<ArtifactWriteResult>;
}

pub struct ArtifactPlan {
    pub path: PathBuf,
    pub estimated_generation_bytes: usize,
}

pub trait ArtifactProducer {
    fn plan(&self) -> &ArtifactPlan;
    fn produce(self, stream: &mut ArtifactWriteStream) -> Result<()>;
}

pub struct ArtifactGenerationLimiter;
```

兼容入口流程：

```text
Build / exporters / codegen
  -> generate and collect Vec<Artifact> in declaration order
  -> ArtifactWriter::new(mode).with_limits(limits).write(artifacts)
     -> create ArtifactSink
     -> for artifact in artifacts: sink.write_text/write_binary(...)
     -> sink.finish()
```

Build 主流程：

```text
build_target()
  -> ArtifactWriter::new(mode).with_limits(limits).open_sink()
  -> ArtifactGenerationLimiter::from_limits(limits)
  -> run exporter/codegen branches with shared sink
  -> sink.finish()
```

streaming generator 流程：

```text
Build / exporters / codegen
  -> ArtifactWriter::new(mode).with_limits(limits).open_sink()
  -> tasks run in parallel
     -> stream = sink.create(path)
        -> normalize final path and reserve it in registry atomically
        -> create CountingWriter / final file / temp file stream
     -> serializer.write_to(&mut stream) or stream.write_all(bytes)
        -> stream acquires/releases byte permits around buffered chunks
     -> stream.finish()
        -> record bytes, mark path Finished, store temp path if needed
  -> sink.finish()
     -> if any stream failed: return first stable error and cleanup temp files
     -> TempCommit: stable commit/rename after all temp writes succeed
     -> return stable ArtifactWriteResult sorted by normalized path
```

关键约束：

- `ArtifactSink::create(path)` 是 path authority；它必须在返回 stream 前完成 normalized path 占用。重复 path 即使发生在另一个 stream 失败之后，也应报 duplicate path，避免失败任务留下的路径状态被第二个任务复用。
- `ArtifactWriteStream::finish()` 成功只表示该 artifact 的内容已经被当前写入策略消费完；`TempCommit` 的最终可见性仍由 `ArtifactSink::finish()` 统一决定。
- 若任一 stream 写入失败，sink 记录第一个稳定错误并进入 cancellation 状态；后续 `create()` 应尽快失败，已打开 stream 的 `write()` 可以继续 drain 到错误或在下一次 write/finish 返回 cancellation 错误。
- `max_in_flight_bytes` 在 streaming 模型中统计“stream 内部已经接收、尚未写入底层 writer 或 counting writer 的 bytes”。如果第一阶段直接对 `File` 做同步 `write_all()`，可以用小块 buffer + byte semaphore 实现；后续 dedicated writer thread / async writer 可以把同一套 permit 边界迁移到 channel buffer。
- `max_in_flight_artifacts` 防止大量打开但未 finish 的 stream 占满文件句柄、临时文件和 registry 状态；默认可取 `max(16, jobs * 4)`，但 bytes budget 是主限流。

#### 生成端限流与线程行为

仅在 `ArtifactWriteStream::write()` / `finish()` 内阻塞 Rayon worker 会产生两个问题：

1. CPU worker 被阻塞等待 writer 释放内存 permit，无法继续执行其它 ready tasks。
2. 如果所有 writer 或 coordinator 工作也依赖同一个 Rayon pool，可能出现 worker starvation。

因此未来 streaming generator 推荐采用“双层限流”：

1. **生成并发限流**：Build 在调度 artifact-producing tasks 时，为“可能生成大 artifact”的任务使用 `ArtifactGenerationLimiter`。默认并发可取 `min(jobs, max(1, max_in_flight_bytes / 64MiB))`，并至少保留 1 个生成槽。table exporter 已知每张表会产出一个 artifact，可以按 table task 粒度申请生成槽；aggregate exporter 若只能一次性产出多个 artifact，则视为一个大生成任务。
2. **写入流限流**：`ArtifactWriteStream::write()` 使用 byte semaphore、小块 buffer 或 dedicated writer queue。如果 stream buffer 满或 byte permit 不足，写入方不应在 Rayon worker 上 busy-wait；短期实现应使用 `std::sync::Condvar` / channel parking，让线程睡眠并释放 CPU 时间片。更进一步，Build 可以把实际文件写入交给 dedicated writer thread：Rayon worker 只把 serializer 产生的 chunk 移交给 writer lane；当 writer lane 容量不足时，生成端 limiter 会阻止继续启动新任务，而不是让所有 CPU worker 堵在写入点。

线程所有权策略：

- **短期实现（无 async runtime）**：使用 dedicated writer thread(s) + `Mutex/Condvar` byte semaphore。等待 permit 的线程会 park，不消耗 CPU；为了避免占满 Rayon worker，生成任务数量由 `ArtifactGenerationLimiter` 控制在较小范围。此方案简单、跨平台、与当前同步文件写入模型一致。
- **中期 DAG executor**：把 `RunGenerator` 与 `WriteArtifact` 拆成不同 resource lane。`RunGenerator` 在 CPU lane；`WriteArtifact` 在 I/O lane；当 byte budget 不足时，node 保持 `PendingCapacity` 状态并交还 worker，等待 writer 释放 permit 后由 readiness queue 重新唤醒。此时不需要阻塞 Rayon worker。
- **不推荐**：在 CPU pool 内无限制 `par_iter()` 生成 artifact，然后在 `write()` 上长期阻塞等待 byte permit。这只能限制 writer buffer，不能限制“正在生成”的内存峰值。

#### Direct / TempCommit / DryRun 细节

- `DryRun`：仍执行 full build、validation 和内容生成，以获得准确 bytes；`ArtifactSink::create()` 返回 `CountingWriter`，`write()` 只累计 bytes，不创建目录、不写文件。若后续需要纯 path-only dry-run，可另加 `--dry-run=plan-only`，不能改变现有 dry-run 语义。
- `Direct`：`ArtifactSink::create()` 创建父目录并打开最终路径文件；`write()` 同步写入最终文件并按 chunk 参与 byte budget。Direct 模式失败时可能已经留下部分最终文件，这是该模式的既有弱语义；需要“全部成功后再可见”时应使用 `TempCommit`。
- `TempCommit`：`ArtifactSink::create()` 创建同目录 temp file，stream 写入 temp file；所有 stream 都 `finish()` 且无错误后，`ArtifactSink::finish()` 按 normalized final path 稳定顺序 rename。commit 阶段不再持有 artifact content bytes，因此内存峰值只与打开 stream 的 buffer / temp file writer 有关。
- `TempCommit` 清理：任一 stream 写 temp 阶段失败时，`ArtifactSink::finish()` 清理已创建 temp files；commit 阶段失败时沿用当前语义，可能已有部分最终文件被 rename 成功，但错误路径必须稳定。
- `create()` 失败持久化：duplicate path、final/temp file open 失败都会记录 `first_error`；即使调用方忽略 `create()` 返回的错误，后续 `ArtifactSink::finish()` 仍必须失败。若存在 `Failed` path 但没有 `first_error`，`finish()` 会返回 fallback validation error。

#### Stream 生命周期与并发安全

`ArtifactWriteStream` 的生命周期必须显式结束：

```text
create(path) -> Open
  -> write(...)*
  -> finish() -> Finished
```

约束：

- `ArtifactWriteStream` 应是 move-only handle；drop 未 finish 的 stream 时，sink 必须把该 path 标记为 `Failed`，并让最终 `ArtifactSink::finish()` 返回错误。这样可以避免 exporter 因 early return 丢弃 writer 后构建仍然成功。
- `ArtifactSink` 可以是 `Arc<ArtifactSinkInner>` clone handle，便于多个 Rayon task 同时调用 `create()`；最终 `finish()` 需要消费 owner handle 或通过显式 `close()` 阻止新 stream 创建。
- `create()` 的 path reservation 必须在 `Mutex` 内完成；文件打开可以在 reservation 完成后执行，但如果打开失败，需要把 path 状态标记为 `Failed`，不能删除 reservation 后允许另一个任务重试同一路径。
- `entries` 的排序不依赖任务完成顺序；sink 在 `finish()` 中按 normalized path 输出 `ArtifactWriteResult.entries`。
- 对同一 path 的重复写入错误应尽量包含 normalized path；未来可在 registry 中保存 first owner task key，用于诊断“哪个 export task 首先占用了该路径”。

#### 超大单文件策略

如果单个 stream 写入的 artifact 总大小大于 `max_in_flight_bytes`：

1. streaming path 不应要求一次性持有完整 artifact bytes；`write()` 按 chunk 申请 byte permit，写入底层 writer 后释放 permit。因此单文件大小可以超过总 byte budget，只要单个 chunk / internal buffer 不超过预算。
2. 兼容入口 `ArtifactWriter::write(Vec<Artifact>)` 仍可能遇到单个 materialized artifact 大于 `max_in_flight_bytes`；默认允许“单 artifact 例外”：独占 byte budget，等所有其它 in-flight artifact 写完后提交该 artifact，并在 warning 中提示该文件超过预算。
3. 如果用户显式设置 `--artifact-memory-limit` 且该值小于单个 materialized artifact，仍允许单 artifact 例外；否则构建会因为任何单个大文件无法写入而不可用。需要严格失败时可在未来增加 `--artifact-memory-strict`。

## 7.5 与 Exporter 的关系

Exporter 的职责从“序列化并写文件”收敛为“向 Build 提供 artifact 写入逻辑”。目标模型中 exporter 不再返回完整 `Vec<u8>` / `String` 内容，而是接收 `ArtifactSink` 或 `ArtifactWriteContext`，为每个输出 path 打开 stream 并直接写入：

```rust
pub(crate) fn export_runtime_data_to_sink(
    ...,
    sink: &ArtifactSink,
    generation_limiter: &ArtifactGenerationLimiter,
) -> Result<Vec<RuntimeDataFile>>;
```

其中：

- `RuntimeDataFile` 是 runtime artifact metadata / manifest contract；当前默认 codegen 不消费真实 artifact path，而是使用逻辑 table name。
- 实际写入路径由 export task output root + runtime data file path 得到，并传给 `sink.create(path)`。
- Exporter 不再调用 `fs::create_dir_all` / `fs::write`，也不选择 `ArtifactWriteMode`。
- Serializer 应优先支持 `std::io::Write` 输出，避免先构造完整 `Vec<u8>`；暂时只能产出内存 buffer 的格式可以通过 `sink.write_binary()` 兼容。

旧 `RuntimeDataArtifact` / `Vec<Artifact>` table exporter 兼容入口已移除；测试若需要落盘断言，应使用 `ArtifactSink` / `ArtifactWriter` 辅助函数，不应让 exporter 自己暴露 filesystem API。aggregate exporter trait 暂时仍保留 `Vec<Artifact>` fallback，但 Build adapter 会在 limiter permit 内立即写入 shared sink。

## 7.6 与 Codegen 的关系

Codegen 短期继续返回生成文件列表，但目标模型也应迁移到 `ArtifactSink`，避免 codegen 与 exporter 走两套写入路径：

```rust
pub struct GeneratedFile {
    pub path: PathBuf,
    pub content: String,
}
```

兼容期 Build 可把 `GeneratedFile` 写入 sink：

```rust
let mut stream = sink.create(output.join(generated.path))?;
stream.write_all(generated.content.as_bytes())?;
stream.finish()?;
```

长期可以把 codegen trait 改为接收 `&ArtifactSink` 或更窄的 `GeneratedCodeSink` wrapper；小型源码文件仍可先构造 `String` 再调用 `sink.write_text()`，大型聚合代码生成器可以直接向 stream 写模板片段。

## 7.7 与 Build / CLI 的关系

Build 是唯一编排者：

1. CLI 解析 `--dry-run`，写入 `BuildOptions`。
2. Build 正常执行 project/meta/data/validation/exporter/codegen。
3. Build 创建 `ArtifactSink`，并把 sink 或受限 wrapper 传给 exporter/codegen。
4. exporter/codegen 通过 sink 打开 artifact stream 并写入内容。
5. Build 调用 `ArtifactSink::finish()` 获得 `ArtifactWriteResult`；dry-run 模式下 CLI 输出 artifact plan，但不写文件。

CLI dry-run 不应跳过 validation 或 serialization/codegen，因为用户需要确认完整构建能成功，只是不提交文件系统变更。

过渡期为了降低一次性改造风险，`ArtifactWriter::write(Vec<Artifact>) -> ArtifactSink` 兼容层仍保留给旧 API、测试和外部调用使用；Build 主流程不再收集全局 `Vec<Artifact>`，而是在 `build_target()` 创建共享 sink，exporter/codegen 分支直接或通过兼容 wrapper 写入同一个 sink。不能让旧 writer 重新保留独立的 path conflict / temp commit 实现。

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

1. 相同 path 的 artifact 在第二次 `ArtifactSink::create()` 时立即报错。
2. dry-run 不创建父目录、不写文件，但返回稳定排序 entries。
3. direct 模式通过 `ArtifactWriteStream` 创建父目录并写入 text/binary 内容。
4. write entries 按 path 稳定排序。
5. runtime data artifact 与 generated code artifact 冲突时由统一 writer 报错。
6. temp-commit 模式先写临时文件再提交最终路径，写临时文件失败时清理临时文件。
7. stream drop without finish 会让 sink finish 失败，并清理 temp file。
8. `ArtifactWriter::write(Vec<Artifact>)` 与手动 sink 写入产生相同 `ArtifactWriteResult` 和文件内容。
9. `create()` duplicate / open file 失败即使被调用方忽略，`finish()` 仍失败。

集成测试应覆盖：

1. CLI `--dry-run` 对标准工程执行成功并输出 artifact plan，但不产生输出文件。
2. 正常 build 仍产生 JSON / MessagePack / generated code。
3. data output 与 code output 指向同一路径时报统一 artifact conflict。
4. CLI `--write-mode temp-commit` 产生与 direct 模式等价的最终 artifact。

## 7.10 当前状态与未来规划

当前状态：

- `src/artifact/` 已提供 `Artifact`、`ArtifactContent`、`ArtifactWriter`、`ArtifactSink`、`ArtifactWriteStream`、`ArtifactWriteMode`、`ArtifactWriteLimits` 和 `ArtifactWriteResult`。
- Artifact 模块内的写入 authority 已收敛到 `ArtifactSink`；`ArtifactWriter::write(Vec<Artifact>)` 是兼容 wrapper，并通过 sink 写入。
- 旧 dedicated bounded queue 已被 sink wrapper 替代；当前 byte budget 通过 `ArtifactWriteStream` 写入时的 byte semaphore 生效，artifact 数量预算通过 sink 的 artifact semaphore 生效。
- runtime table exporter 与 generated code 在 Build 主流程中已接入共享 `ArtifactSink`；runtime table exporter 通过 `ArtifactProducer` / `ArtifactPlan` 写入 sink，JSON / MessagePack 使用 writer-style serializer，binary 暂时仍复用 `Vec<u8>` fallback。codegen 短期仍先生成 `String`，再在 generation limiter permit 内立即 `sink.write_text()`。aggregate exporter trait 仍返回 `Vec<Artifact>`，由 Build runtime wrapper 在 generation limiter permit 内立刻写入 sink，后续可继续改造为显式 writer context / stream API。
- CLI 支持 `--dry-run`、`--write-mode direct|temp-commit`、`--artifact-memory-limit`、`--artifact-memory-percent`。
- temp-commit 当前提供临时文件阶段保护；提交阶段失败按 `ConfigForgeError::WriteFile` 报错，并可能保留已提交的最终文件。

下一阶段规划：

- aggregate exporter trait、codegen trait 与 remaining binary serializer 继续从“返回 materialized artifact / String / Vec<u8>”迁移到“接收 sink 或受限 writer context 并向 stream 写入”。runtime table exporter 已先接入 shared sink 和 producer API；generation limiter 已限制这些 fallback materialized payload 的并发数量，后续可继续降低单文件内存峰值。
- 短期使用同步 file writer、`Mutex/Condvar`、bounded stream buffer 和略高于 CPU 核心数的 I/O/worker 配置承载本地文件写入等待；远端 source provider 落地时再引入 async runtime。
- filesystem-aware path key 可在跨平台大小写不敏感冲突或 symlink alias 成为实际问题时接入，当前 lexical-normalized key 继续作为默认 authority。
- summary manifest、远端上传、增量缓存或分布式 artifact store 应作为独立 writer / exporter 扩展，不改变当前 Artifact Writer 的基础语义。
