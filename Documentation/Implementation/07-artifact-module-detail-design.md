# 07. Artifact 模块详细设计

本文档描述 `src/artifact/` 模块的实现边界。Artifact 模块负责统一管理 ConfigForge 的所有文件制品：runtime data、generated code、POT / asset manifest 等聚合 data artifact、schema / validation report 等辅助输出。Artifact generator 和对应 toolkit 只负责生成制品内容与相对目标路径；Build 负责创建统一的 Artifact 写入协调器，由协调器打开每个 artifact 的写入流，并执行路径冲突检查、dry-run、direct 写入或 temp-commit 写入。

## 7.1 设计目标

Artifact 模块需要解决：

1. 统一 runtime data、generated code、report/doc/manifest 等制品模型，避免 generator / toolkit 各自定义写文件流程。
2. 通过中心协调器集中占用输出路径并检查冲突，覆盖跨 export task、runtime data 与 generated code 之间的冲突。
3. 支持 `dry-run`：完整执行配置加载、Meta link、data load、validation、serialization/codegen 和路径规划，但不写入最终文件。
4. 支持 `direct` 与 `temp-commit` 两种写入策略，并支持 `dry-run` 写入计划。
5. 生成稳定的写入结果：路径稳定排序、bytes/文本内容统一表达。
6. 保持模块依赖方向简单：artifact 是底层服务模块，不依赖 generator、data_artifact、code_artifact、build、project、data、validation。

非目标：

- 不决定哪些 runtime data / code-like / report / doc task 应该执行；task 编排属于 `build`。
- 不理解 Meta、DataSet、TargetTags 或运行时格式；语义生成属于 artifact generator 及其 data_artifact / code_artifact toolkit。
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

Code-like generator 可以返回 `Text`，runtime data generator 可以返回 `Binary`。Artifact Writer 不关心文本编码转换之外的业务语义；文本统一以 UTF-8 写入。

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
- `TempCommit`：先写入同目录临时文件，全部临时文件写入成功后再按稳定顺序 replace/commit 到最终路径；它与 `Direct` 一样允许覆盖已有最终文件。临时文件写入失败时清理已创建的临时文件，提交阶段失败可能保留已提交的最终文件。

### 7.3.4 `ArtifactWriteResult`

```rust
pub struct ArtifactWriteResult {
    pub mode: ArtifactWriteMode,
    pub entries: Vec<ArtifactWriteEntry>,
}
```

`entries` 必须按规范化后的 path 稳定排序，便于 CLI 输出和测试断言。Artifact Writer 只返回写入计划和写入结果，不生成业务产物。

### 7.3.5 `ArtifactSink`

streaming 化的主入口是中心协调器 `ArtifactSink`，而不是让 generator / toolkit 自行持有文件句柄或绕过 artifact 模块写文件：

```rust
pub struct ArtifactSink { /* private */ }

impl ArtifactSink {
    pub(crate) fn reserve_paths(&self, paths: impl IntoIterator<Item = PathBuf>) -> Result<()>;
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

- Build 在任何 generator 执行前调用 `reserve_paths(paths)`，一次性检查并预留整个 target 的输出路径；只读取文件系统 metadata，不创建目录或文件。预留成功后 sink 进入严格计划模式。
- `create(path)` 是唯一打开 artifact 写入流的入口。严格计划模式下只允许打开 `Reserved` 路径一次，并再次检查 filesystem conflict key；未声明路径或重复打开都会失败。独立调用 `open_sink()` 的低层调用方仍可使用动态路径模式。
- `write_text()` / `write_binary()` 是 convenience API，内部等价于 `create(path) -> write_all(...) -> finish()`；它们不是另一套写入机制。
- `ArtifactWriteStream` 实现 `std::io::Write`，使 JSON / MessagePack / bin / Lua / codegen 都可以使用同一套 `write_all()` / serializer-to-writer 接口。
- `finish()` 关闭当前 stream，记录 bytes 与临时文件路径；`ArtifactSink::finish()` 检查所有预留路径均已生成且所有 stream 已完成，并在 `TempCommit` 模式执行统一 commit。
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
    Reserved,
    Open,
    Finished,
    Failed,
}
```

`paths` 的 key 必须使用 artifact 模块统一的 path normalization 结果；filesystem-aware conflict key 也只在 artifact 模块中实现，不能让 build / artifact generator 各自维护不同规则。

## 7.4 Artifact Writer 流程

```text
Build creates ArtifactSink
  -> all generators plan paths after Meta link
  -> sink.reserve_paths checks and reserves all target paths
  -> artifact generator asks sink.create(path)
  -> sink claims a Reserved path atomically
  -> task writes bytes through ArtifactWriteStream::write_all / serializer.write_to
  -> stream.finish records bytes and stream status
  -> sink.finish returns stable ArtifactWriteResult
     -> if DryRun: no directories/files are created
     -> if Direct: files have been written to final paths
     -> if TempCommit: all temp files replace final paths in stable order
```

Build 必须在生成开始前收集所有 export task 的完整输出清单，包括 MetaOnly、MetaAndData、report/doc 与附属 manifest。`ArtifactGenerator::plan_artifacts()` 是必需方法，没有默认空清单；具体 generator 负责根据 Meta、target tags 与 options 计算路径，不依赖 Data 内容或执行 serialization。Build 按 export task 声明顺序把所有路径交给同一个 `ArtifactSink::reserve_paths()`，存在冲突时任何 generator 都不得写文件，Direct 模式下已有文件内容也必须保留。

`reserve_paths()` 与 `create()` 共用 artifact 模块的 path normalization、output-root containment 与 filesystem-aware conflict key：解析已存在 ancestor 的 symlink / junction，macOS / Windows 使用 ASCII case-fold，Unix 对已存在文件使用 inode identity 检测 hardlink alias。不存在的 suffix 保留 lexical fallback，避免要求输出目录预先存在。`create()` 重新计算 conflict key，保留预留时的 key，并检查路径是否已被其他制品占用；路径从 `Reserved` 转为 `Open` 后不能再次打开。平台细节见 [`../Tech/platform-filesystem-semantics.md`](../Tech/platform-filesystem-semantics.md)。

### 7.4.1 中心协调式 ArtifactSink pipeline

runtime table exporter、aggregate exporter 和 codegen 可以并行生成 artifact。旧模型要求所有 generator 先返回 `Vec<Artifact>`，再一次性交给 `ArtifactWriter::write(Vec<Artifact>)`；大项目会在写入前同时持有大量 `Vec<u8>` / `String`，内存峰值接近“所有待写 artifact 内容之和 + 正在生成的 artifact 内容”。`ArtifactSink` pipeline 的目标是把写入边界前移：generator 不再把完整内容交给 build 收集，而是向中心协调器申请一个 artifact 写入流，然后直接通过 `Write` 接口序列化/写入。

设计目标：

1. 用一个中心协调器统一 path reservation、dry-run、direct、temp-commit、byte accounting 和 result entry 生成，避免 `Vec<Artifact>` writer 与 streaming writer 长期并存。
2. Build 在生成前统一规划并预留整个 target 的 artifact path；`create(path)` 校验计划契约并防止重复写入。路径规划只收集 metadata，不 materialize artifact 内容。
3. ArtifactSink 只负责返回标准同步 `std::io::Write` 流和管理文件生命周期，不承担全局资源调度职责。
4. 保持 `DryRun`、`Direct`、`TempCommit` 语义稳定；dry-run 不创建目录、不写临时文件，但仍执行内容生成并通过 counting writer 获得准确 bytes。
5. 保持 result entries、commit 顺序与错误优先级稳定。

#### CLI / BuildOptions 边界

当前 CLI 只暴露 artifact 写入模式。`BuildOptions` 只接收 `artifact_write_mode`；`--serial` / `--jobs` 仍控制 CPU 执行并行度。原因是 ArtifactSink 返回标准文件流后，sink 自身不持有完整 artifact 内容；当前设计接受 parsed Meta / DataSet 全量驻留内存，把复杂度控制在 artifact 输出边界：生成端应保持 streaming 写出，避免构建完整 `String` / `Vec<u8>`，而不是在 sink 内维护无法覆盖总 RSS 的预算，也不为了大表输出引入 out-of-core DataSet。

#### Pipeline 模型

当前 artifact 模块已经以 `ArtifactSink` 作为唯一写入协调器；`ArtifactWriter::write(Vec<Artifact>)` 是兼容入口，负责遍历旧 artifact 列表，对每个 artifact 调用 `sink.write_text()` / `sink.write_binary()`，最后调用 `sink.finish()`。Build 主流程已创建共享 `ArtifactSink` 并传入 generator 分支。runtime table、aggregate、language code-like generator 均作为 direct `ArtifactGenerator` 写入 sink，JSON / MessagePack / binary 已覆盖 writer-style serializer；[`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) 中的内置 code-like generator production path 均通过 `generate_to_sink()` 打开 sink stream 并使用 streaming code writer 写出。

已实现 API：

```rust
impl ArtifactWriter {
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
}

pub trait ArtifactProducer {
    fn plan(&self) -> &ArtifactPlan;
    fn produce(self, stream: &mut ArtifactWriteStream) -> Result<()>;
}

```

兼容入口流程：

```text
Build / generators
  -> generate and collect Vec<Artifact> in declaration order
  -> ArtifactWriter::new(mode).write(artifacts)
     -> create ArtifactSink
     -> sink.reserve_paths(all artifact paths)
     -> for artifact in artifacts: sink.write_text/write_binary(...)
     -> sink.finish()
```

Build 主流程：

```text
build_target()
  -> ArtifactWriter::new(mode).open_sink()
  -> Meta link, then plan all task artifacts and reserve_paths
  -> run generator branches with shared sink
  -> sink.finish()
```

streaming generator 流程：

```text
Build / generators
  -> ArtifactWriter::new(mode).open_sink()
  -> plan all artifact paths and sink.reserve_paths(paths)
  -> tasks run in parallel
     -> stream = sink.create(path)
        -> normalize final path and claim its reservation atomically
        -> create CountingWriter / final file / temp file stream
     -> serializer.write_to(&mut stream) or stream.write_all(bytes)
        -> stream writes directly to counting writer / final file / temp file
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
- 若任一 stream 写入失败，sink 记录第一个稳定错误；后续 `create()` 应尽快失败，最终 `finish()` 必须返回该错误并清理 temp files。
- ArtifactSink 不限制 in-flight bytes 或打开 stream 数量；资源控制依赖生成端 streaming 化与 Rayon jobs 的自然并行度。

#### Direct / TempCommit / DryRun 细节

- `DryRun`：仍执行 full build、validation 和内容生成，以获得准确 bytes；`ArtifactSink::create()` 返回 `CountingWriter`，`write()` 只累计 bytes，不创建目录、不写文件。若后续需要纯 path-only dry-run，可另加 `--dry-run=plan-only`，不能改变现有 dry-run 语义。
- `Direct`：`ArtifactSink::create()` 创建父目录并打开最终路径文件；`write()` 同步写入最终文件并统计 bytes。Direct 模式失败时可能已经留下部分最终文件，这是该模式的既有弱语义；需要“全部成功后再可见”时应使用 `TempCommit`。
- `TempCommit`：`ArtifactSink::create()` 创建同目录 temp file，stream 写入 temp file；所有 stream 都 `finish()` 且无错误后，`ArtifactSink::finish()` 按 normalized final path 稳定顺序 replace 到最终路径。commit 阶段不再持有 artifact content bytes，因此内存峰值只与打开 stream 的 buffer / temp file writer 有关。replace 语义必须跨平台保持一致：Unix/macOS 可以直接 `rename` 覆盖已有最终文件；Windows 下因 `fs::rename` 不能替换已有文件，需要先删除已有最终文件再 rename 临时文件。
- `TempCommit` 清理：任一 stream 写 temp 阶段失败时，`ArtifactSink::finish()` 清理已创建 temp files；如果 build / 调用方在 `finish()` 前因其它错误提前返回，sink drop 也会兜底删除已记录的 temp files；成功 commit 或失败清理后必须清空 sink 内部 temp registry，避免 `ArtifactSink` clone 延迟 drop 时误删后续同进程同路径的新 temp file；commit 阶段失败时沿用当前语义，可能已有部分最终文件被 rename 成功，但错误路径必须稳定。
- 失败持久化：duplicate path、final/temp file open 失败、stream write/flush 失败、stream drop 未 finish 都会记录 `first_error`；即使调用方忽略即时返回的错误，后续 `ArtifactSink::finish()` 仍必须失败，并优先保留最早的真实根因。若存在 `Failed` path 但没有 `first_error`，`finish()` 会返回 fallback validation error。

#### Stream 生命周期与并发安全

`ArtifactWriteStream` 的生命周期必须显式结束：

```text
reserve_paths(paths) -> Reserved
  -> create(path) -> Open
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

streaming path 不要求一次性持有完整 artifact bytes；`write()` 直接转发到底层 counting writer / file writer，因此单文件大小不受 sink 内部预算限制。兼容入口 `ArtifactWriter::write(Vec<Artifact>)` 仍会先持有调用方传入的 materialized content；新 generator 路径应优先通过 `ArtifactSink::create()` 直接写入，避免重新引入完整 `Vec<u8>` / `String` materialization。

## 7.5 与 data-backed generator / data artifact toolkit 的关系

当前 exporter 的职责从“序列化并写文件”收敛为“向 generator 提供 data artifact 写入 helper”。目标模型中具体 JSON / MessagePack / `bin` / POT / Lua / Asset Manifest 都是 `src/generator/<type_id>/` 下的 data-backed generator；data artifact toolkit 不再返回完整 `Vec<u8>` / `String` 内容，而是接收 `ArtifactSink` 或受限写入 context，为每个输出 path 打开 stream 并直接写入：

```rust
pub(crate) fn export_runtime_data_to_sink(
    ...,
    sink: &ArtifactSink,
) -> Result<Vec<RuntimeDataFile>>;
```

其中：

- `RuntimeDataFile` 是 runtime artifact metadata / manifest contract；当前默认 codegen 不消费真实 artifact path，而是使用逻辑 table name。
- 实际写入路径由 export task output root + runtime data file path 得到，并传给 `sink.create(path)`。
- Data artifact helper / generator 不再调用 `fs::create_dir_all` / `fs::write`，也不选择 `ArtifactWriteMode`。
- Serializer 应优先支持 `std::io::Write` 输出，避免先构造完整 `Vec<u8>`；暂时只能产出内存 buffer 的格式可以通过 `sink.write_binary()` 兼容。

旧 `RuntimeDataArtifact` / `Vec<Artifact>` table exporter 兼容入口已移除；测试若需要落盘断言，应使用 `ArtifactSink` / `ArtifactWriter` 辅助函数，不应让 data artifact helper 自己暴露 filesystem API。aggregate exporter 主路径已改为 required `export_to_sink()`；`export() -> Vec<Artifact>` 仅作为 legacy fallback。

## 7.6 与 code-like generator / code artifact toolkit 的关系

Code-like generator production path 已迁移到 `ArtifactSink`，避免 code artifact toolkit 与 data artifact toolkit 走两套写入路径。code artifact renderer facet API 是：

```rust
pub(crate) trait CodeGenerator {
    fn generate_to_sink(&self, input: &CodeExportInput<'_>, sink: &ArtifactSink) -> Result<()>;
}
```

`CodeGenerator` 当前是 crate 内部 renderer facet，不作为稳定外部 generator plugin API 暴露；因此它可以直接依赖内部 `ArtifactSink` 类型，而不要求 crate root 同步 re-export sink/stream 细节。外部用户应通过 Project Config 的内置 `type` 选择 artifact generator，而不是自行实现该 trait。language generator 直接实现 `ArtifactGenerator`，内部复用 code artifact toolkit。

典型 code-like generator 写出流程：

```rust
let mut stream = sink.create(output_path)?;
let mut writer = CheckedStreamingCodeWriter::new(stream, formatting);
render_language_file(&mut writer, input)?;
let stream = writer.finish()?;
stream.finish()?;
```

`GeneratedFile` / `CodeGenerator::generate()` 只在 `#[cfg(test)]` 下保留，用于 String-backed snapshot 和 streaming equivalence 测试；Build 不再持有或排序 generated content。小型表达式片段仍可局部返回 `String`，但文件级 / 语句级写出应走 streaming writer。

## 7.7 与 Build / CLI 的关系

Build 是唯一编排者：

1. CLI 解析 `--dry-run`，写入 `BuildOptions`。
2. Build 正常执行 project/meta/data/validation/generator。
3. Build 创建 `ArtifactSink`，并把 sink 或受限 wrapper 传给 generator。
4. generator 通过 sink 打开 artifact stream 并写入内容。
5. Build 调用 `ArtifactSink::finish()` 获得 `ArtifactWriteResult`；dry-run 模式下 CLI 输出 artifact plan，但不写文件。

CLI dry-run 不应跳过 validation 或 serialization/codegen，因为用户需要确认完整构建能成功，只是不提交文件系统变更。

`ArtifactWriter::write(Vec<Artifact>)` 兼容层仍保留给测试、外部调用和少量旧 API 使用；Build 主流程不再收集全局 `Vec<Artifact>`，而是在 `build_target()` 创建共享 sink，generator 分支直接写入同一个 sink。不能让旧 writer 重新保留独立的 path conflict / temp commit 实现。

## 7.8 错误边界

第一阶段可复用 `ConfigForgeError`：

| 场景 | 错误 |
| --- | --- |
| 重复 artifact path | `ConfigForgeError::Validation` |
| 创建目录失败 | `ConfigForgeError::WriteFile` |
| 写最终文件失败 | `ConfigForgeError::WriteFile` |
| temp commit rename 失败 | `ConfigForgeError::WriteFile`；已 rename 成功的文件可能保留在最终路径 |

后续如果 artifact 诊断增多，可新增 `ArtifactDiagnostic`，但不应让各类 artifact generator 为写入错误各自包装一套错误类型。

## 7.9 测试计划

Artifact 模块单元测试应覆盖：

1. 跨 MetaOnly / MetaAndData task 的路径冲突在 `reserve_paths()` 时失败，Direct 模式下已有文件保持原样；计划内路径重复 `create()` 也会失败。
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

- `src/artifact/` 已提供 `Artifact`、`ArtifactContent`、`ArtifactWriter`、`ArtifactSink`、`ArtifactWriteStream`、`ArtifactWriteMode` 和 `ArtifactWriteResult`。
- Artifact 模块内的写入 authority 已收敛到 `ArtifactSink`；`ArtifactWriter::write(Vec<Artifact>)` 是兼容 wrapper，并通过 sink 写入。
- 旧 dedicated bounded queue 和 sink 级资源限流实现已移除；ArtifactSink 只负责标准写入流与生命周期。
- runtime table generator、aggregate data-backed generator 与 generated code 在 Build 主流程中已通过 direct `ArtifactGenerator` 接入共享 `ArtifactSink`；runtime table generator 通过 stream API 写入 sink，JSON / MessagePack / binary 使用 writer-style serializer；aggregate data-backed generator 通过 required `export_to_sink()` 写入 sink；[`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) 中的内置 code-like generator 通过 `generate_to_sink()` 与 streaming writer 写入 sink。
- CLI 支持 `--dry-run`、`--write-mode direct|temp-commit`；artifact 写入资源控制不再暴露独立参数。
- temp-commit 当前提供临时文件阶段保护；提交阶段失败按 `ConfigForgeError::WriteFile` 报错，并可能保留已提交的最终文件。

下一阶段规划：

- 继续压缩兼容 API 的使用面：`ArtifactWriter::write(Vec<Artifact>)`、table exporter `serialize_table()` 和 aggregate exporter `export()` 只应作为测试 / 兼容 helper，不应回到 Build 主路径。
- 短期使用同步 file writer 与标准 `std::io::Write` 接口；远端 source provider 落地时再评估是否引入 async runtime。
- filesystem-aware path key 已用于全局规划与写入；后续可改进卷级大小写策略与 canonicalize 失败诊断。
- summary manifest、远端上传、增量缓存或分布式 artifact store 应作为独立 writer / exporter 扩展，不改变当前 Artifact Writer 的基础语义。
