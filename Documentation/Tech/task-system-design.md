# 任务系统与并发执行技术设计

本文档描述 ConfigForge 后续任务系统的技术实现方案。目标是在不改变现有导出语义、诊断顺序和产物稳定性的前提下，合理利用 CPU 多核与操作系统异步 I/O 能力，支持用户通过 CLI 限制资源消耗，避免默认占满整台机器。

当前实现已引入局部 Rayon CPU worker pool：`build_target` 仍按 Project Config 读取、Meta Link、Data Load、Normalize、Validation、Runtime Data Export、Codegen 与写文件的阶段顺序执行，但 DataLoader、runtime data serialization 和 codegen task 已在受 `jobs` 限制的 worker pool 中并行。远端 provider、async I/O、完整任务 DAG 和独立 artifact writer 仍属于后续演进范围。本文档作为后续重构的工程设计依据。

## 1. 设计目标

### 1.1 功能目标

1. 将一次导出拆成显式任务图，表达阶段依赖和可并行粒度。
2. 支持以下粒度的并行：
   - 按文件读取 Project Config / Meta imports；
   - 按 source / table 读取数据；
   - 按 table 做 normalize、validation、runtime payload 构建；
   - 按 target 和 code task 做 codegen；
   - 按 artifact 写出文件。
3. 区分 I/O 等待任务和 CPU 密集任务：
   - 网络 source、远端表格 API、后续 bounded artifact write 优先使用 async I/O；
   - 本地 CSV 第一阶段可以继续使用同步 reader，并作为受 `jobs` 限制的 blocking/CPU source task 执行，避免为了 async 化而一次性读完整文件增加内存峰值；
   - CSV parse、Meta link、validation、codegen、serialization 使用受限 CPU worker pool。
4. CLI 可控制 CPU 并行度和 I/O 并发度，避免抢占所有机器资源。
5. 诊断、manifest、产物路径检查保持稳定排序，不受任务完成顺序影响。

### 1.2 非目标

第一阶段不实现：

- 分布式执行；
- 跨进程任务缓存；
- daemon / server 模式；
- 复杂增量构建；
- 对所有 provider 强制 async 化。

第一阶段目标是为本进程内并发执行建立可靠基础。

## 2. 任务分类

ConfigForge 的任务可按资源类型分为三类。

| 类型 | 示例 | 执行方式 | 资源限制 |
| --- | --- | --- | --- |
| Async I/O task | 远端 Sheet/API 请求、后续 async artifact write、可选 async file read | async runtime | `io_concurrency` / provider semaphore |
| CPU/blocking task | 本地 CSV reader、TOML/CSV parse、Meta link、normalize、validation、codegen、JSON/MessagePack serialization | CPU worker pool / blocking source pool | `jobs` |
| Coordination task | 建 DAG、排序诊断、合并结果、路径冲突检查 | 当前 async task 或主线程 | 通常串行 |

原则：

- 不为每个文件/表直接创建 OS thread。
- 不在 async runtime worker 上执行长时间 CPU 任务；CPU 任务必须进入 CPU pool。
- 不在 CPU pool 中等待网络或文件 I/O；I/O 等待应回到 async runtime。
- 大任务必须以稳定 key 命名，便于诊断和 tracing。

## 3. 推荐技术选型

### 3.1 Async Runtime

长期推荐使用 `tokio` 承载远端 provider、网络请求和后续 async artifact writer。但第一阶段不强制引入 Tokio：如果只并发化本地 CSV、多 table normalize/codegen，本地同步 reader + 局部 Rayon pool 的收益更直接、复杂度更低。

当远端 provider 或 async artifact writer 落地时，再引入 `tokio`：

- 本地文件可按 provider 策略选择同步 reader 或 `tokio::fs`，不要求统一 async 化；
- 网络 source provider 可复用 async HTTP client；
- `Semaphore` 可限制同时打开文件、HTTP 请求、远端 API 请求数；
- `JoinSet` / `FuturesUnordered` 可收集同类 I/O 任务。

建议配置：

```rust
tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()?;
```

对于 CLI batch 工具，第一阶段可以使用 current-thread Tokio runtime，避免 Tokio worker 与 Rayon worker 叠加造成过度订阅。如果后续远端 provider 并发度很高，可再引入 `async_worker_threads = 1..2` 的 multi-thread runtime。`async_worker_threads` 不等同于 CPU 并行度；CPU 密集工作仍交给 CPU pool。

### 3.2 CPU Worker Pool

推荐使用固定大小 CPU pool。可以选择：

1. `rayon::ThreadPool`：适合数据并行、表级并行、join/scope。
2. 自建 `std::thread` + channel：控制更强，但实现成本高。
3. `tokio::task::spawn_blocking`：简单，但默认 blocking pool 可能扩张，不利于严格控制 CPU 占用。

建议第一阶段采用 `rayon` 并配置局部 pool：

```rust
rayon::ThreadPoolBuilder::new()
    .num_threads(options.jobs)
    .thread_name(|index| format!("config-forge-cpu-{index}"))
    .build()?;
```

不要使用 Rayon global pool：global pool 只能初始化一次，不能按每次 CLI 的 `--jobs` 重建。`ExecutionContext` 应持有局部 `rayon::ThreadPool`，让 `--jobs` 对每次 build 生效。

如果未来需要 async task 调用 CPU pool，可以封装 owned async bridge API：

```rust
pub async fn spawn_cpu<T>(
    ctx: &ExecutionContext,
    key: TaskKey,
    job: impl FnOnce(CancellationToken) -> Result<T> + Send + 'static,
) -> Result<T>
where
    T: Send + 'static;
```

底层可以通过 CPU pool + oneshot channel 返回结果，确保不会绕过 `jobs` 限制。该 API 必须明确：

- 捕获并转译 panic；
- caller cancelled 时 receiver 可以 drop，但已经开始的 CPU job 只能通过 `CancellationToken` 协作退出；
- 不允许在 CPU pool worker 内同步等待另一个 `spawn_cpu`，避免嵌套调度死锁或饥饿。

借用 `LinkedMetaModel<'meta>` 的 CPU 并发优先使用 scoped API，而不是 `'static` async bridge：

```rust
pub fn cpu_scope<'scope, R>(
    ctx: &ExecutionContext,
    f: impl FnOnce(&rayon::Scope<'scope>) -> R,
) -> R;
```

第一阶段也可以不抽象 `spawn_cpu`，直接在 source loading / artifact generation 阶段局部使用 `ThreadPool::install`、`par_iter` 或 `rayon::scope`。

### 3.3 为什么不只用 Tokio

`tokio` 很适合 I/O 并发，但不适合作为唯一 CPU 并行机制：

- async worker 被 CPU 任务占住会影响 timer、I/O wake、网络请求调度；
- `spawn_blocking` 的线程数策略偏“避免阻塞 async runtime”，不是“严格按用户指定核心数执行”；
- ConfigForge 的 CPU 任务更接近 batch 计算，固定 CPU pool 更可控。

因此推荐：Tokio 管 I/O 和任务编排，Rayon/固定 CPU pool 管计算。

## 4. CLI 资源控制

### 4.1 新增参数

建议新增全局参数：

```text
--jobs <N>
    CPU worker 数量。用于 parse、normalize、validation、codegen、serialization 等 CPU 任务。

--io-concurrency <N>
    同时进行的本地/远程 I/O 任务上限。

--network-concurrency <N>
    远程 source provider 的网络请求上限。默认不超过 io-concurrency。

--serial
    等价于 --jobs 1 --io-concurrency 1 --network-concurrency 1，便于复现问题。
```

### 4.2 默认值

默认策略应保守，避免占满开发机或 CI 机器：

```text
jobs = min(available_parallelism, 4)
io_concurrency = min(max(jobs * 2, 4), 16)
network_concurrency = min(io_concurrency, 8)
async_worker_threads = 0/current-thread，或远端 provider 阶段使用 1..2
```

其中 `available_parallelism` 来自 `std::thread::available_parallelism()`。

如果用户显式指定 `--jobs 0`，应报错；不建议把 `0` 解释为 unlimited。

### 4.3 配置结构

```rust
pub struct ExecutionOptions {
    pub jobs: usize,
    pub io_concurrency: usize,
    pub network_concurrency: usize,
    pub serial: bool,
}

pub struct ExecutionContext {
    pub options: ExecutionOptions,
    pub cpu_pool: rayon::ThreadPool,
    pub io_semaphore: Arc<tokio::sync::Semaphore>,
    pub network_semaphore: Arc<tokio::sync::Semaphore>,
}
```

`ExecutionContext` 不应持有用于最终用户输出的共享 `DiagnosticCollector`。并发任务应返回本任务 diagnostics，由协调层统一合并、排序和渲染；共享 collector 只适合 tracing/event，不适合最终诊断。

`BuildOptions` 后续应持有 `ExecutionOptions`，CLI parse 后传入 exporter 编排层。

```rust
pub struct TaskOutput<T> {
    pub key: TaskKey,
    pub value: Option<T>,
    pub diagnostics: Vec<Diagnostic>,
}
```

协调层处理：

```text
collect task outputs
  -> flatten diagnostics
  -> sort by stable key
  -> render
```

## 5. 任务图模型

### 5.1 核心类型

```rust
pub type TaskId = u64;

pub enum TaskKind {
    ConfigRead,
    MetaExtract,
    MetaLink,
    SourceRead,
    NormalizeTable,
    ValidateTable,
    ValidateDataSet,
    GenerateRuntimeData,
    GenerateCode,
    WriteArtifact,
}

pub struct TaskKey {
    pub kind: TaskKind,
    pub target: Option<String>,
    pub table: Option<String>,
    pub source_path: Option<PathBuf>,
    pub artifact_path: Option<PathBuf>,
}

pub struct TaskSpec {
    pub id: TaskId,
    pub key: TaskKey,
    pub deps: Vec<TaskId>,
    pub resource: TaskResource,
}

pub enum TaskResource {
    AsyncIo,
    Cpu,
    Coordination,
}
```

第一阶段不一定要暴露通用 DAG executor；可以先在 exporter 流程中按阶段使用 `join_all` / CPU pool parallel iter。但类型设计应向 DAG 收敛，避免后续重构推倒重来。

### 5.2 稳定性要求

任务可以乱序完成，但结果合并必须稳定：

- Project Config imports 按 canonical path 或 import 声明顺序稳定合并；
- tables 按 MetaModel 中顺序输出；
- code tasks 按 Project Config 中顺序输出；
- diagnostics 使用 `(severity, code, path, table, row, field, message)` 或现有稳定 key 排序；
- artifact 写入前必须先汇总所有计划路径并做冲突检查。

### 5.3 所有权与生命周期约束

当前 ConfigForge 大量使用 borrowed linked view，例如 `LinkedMetaModel<'meta>` 从 `&'meta MetaModel` 派生。该设计非常适合串行和 scoped 并行，但不能直接放入 `tokio::spawn` 这类要求 `'static` future 的任务中。

短期约束：

- 不把 `&LinkedMetaModel<'_>`、`&LinkedTableMeta<'_>` 捕获进 `'static` async task；
- async task 只处理 owned 数据，例如 `PathBuf`、bytes、远端响应、RawSourceData；
- 需要 Meta 借用视图的 CPU 并发使用 `rayon::scope` / `ThreadPool::install`，让任务生命周期受当前 build stack 约束；
- 不构造 `Arc<LinkedMetaModel<'static>>`，避免 self-referential 结构问题。

中长期如果要实现真正通用 DAG executor，应重新设计 BuildState：

```rust
pub struct BuildState {
    pub meta_model: Arc<MetaModel>,
    // 不长期保存 self-borrowing LinkedMetaModel。
}
```

每个阶段在受控 scope 内创建/借用 linked view，或者将 linked view 改造成 owned/index-based 结构，而不是长期保存对 `MetaModel` 内部对象的引用。

### 5.4 任务粒度与调度开销

任务拆分不能无限细。只有当任务自身开销明显大于调度、排队、同步和结果合并开销时，拆分才有意义。过细任务会带来：

- CPU worker 上下文切换和 work stealing 开销；
- 更多 allocation、channel message、锁竞争；
- 更复杂的诊断合并和稳定排序；
- 更高的 peak memory（尤其 artifact bytes）。

推荐原则：

| 场景 | 推荐粒度 | 不推荐粒度 |
| --- | --- | --- |
| Source loading | 一个 source / 一个 table source | 每一行、每个 cell |
| CSV parse | 一个文件或按大 chunk（未来） | 每行一个 job |
| Normalize | 一个 table；小表合并成 chunk | 每个 field、每行 |
| Validation | 一个 table 或一个 target-table；小表合并成 chunk | 每条规则、每个 row |
| Runtime data export | 一个 table artifact；小表合并成 chunk；大表可 streaming/chunk | 每行序列化一个 job |
| Codegen | 一个 code task / language | 每个字段、每个小片段 |
| Artifact write | 一个 artifact；小文件可批量顺序写 | 每个 buffer slice |

经验阈值：

- CPU job 预计耗时低于 1ms 时，通常不值得单独调度；应合并到同 table / same phase chunk。
- CPU job 预计耗时在 1~10ms 时，只有在数量足够多且合并成本低时才并行。
- CPU job 预计耗时超过 10ms，或涉及大文件/大表/大 artifact，适合作为独立任务。
- I/O job 应受 semaphore 控制；大量小文件写入可以按目录或 artifact batch 合并写出计划，但仍需保持错误定位到单文件。

实现上可以给阶段调度器增加 chunking：

```text
if table.row_count < small_table_threshold:
  group multiple tables into one CpuBatch task
else:
  one large table = one task
```

第一阶段不需要精确估算成本，只需避免显然过细的 row/cell/field 级任务。后续 tracing 有真实 duration 后，再根据耗时自动调整 chunk size。

## 6. 阶段级并发设计

### 6.1 Project Config 与 imports

当前 `ProjectConfig::from_path` 递归同步读取配置片段。后续可拆为：

```text
read root config
  -> parse root config
  -> discover imports
  -> read import configs concurrently
  -> parse concurrently
  -> merge by current DFS post-order semantics
```

注意：

- import 图解析仍需要检测循环；
- 同一个 canonical path 只读一次；
- merge 不能按任务完成顺序执行；
- parse TOML 属于 CPU task，read file 属于 async I/O task。

并发实现必须复现当前 DFS post-order 语义，除非另行设计 migration：

```text
for each config fragment:
  merged_order = imports 按声明顺序展开后的 post-order + 当前 fragment
  duplicate canonical path 只保留第一次展开位置
```

也就是说：主文件字段仍在其 imports 之后合并；同一个 import 被多处引用时，只按第一次发现的位置参与合并；读取可以并发，最终 fragment list 不能按任务完成顺序决定。

### 6.2 Meta extract / link / validate

Meta extract 是从 ProjectConfig clone/移动 `enums`、`structs`、`tables` 到 `MetaModel`。

Meta link 当前依赖完整 `MetaModel`，适合保持单阶段执行。后续如果表很多，可以并行预构建局部索引或校验表内字段，但最终 `LinkedMetaModel<'meta>` 的引用构建必须保证生命周期清晰。

建议第一阶段：

- Link 保持串行；
- Meta validate 中不依赖全局可变状态的 table/field 规则可并行收集 diagnostics；
- diagnostics 统一排序后返回。

### 6.3 Source loading

Source loading 是最适合优先并发化的阶段。

任务粒度：

```text
one DataSourcePlan -> one SourceRead task
```

本地 CSV：

- 第一阶段不强制 `tokio::fs`；可以保留现有同步 `CsvDataReader::read_table`，作为受 `jobs` 限制的 blocking source task 并行执行；
- 避免为了 async 化而一次性 read full file 到 bytes/string，防止大 CSV 内存峰值升高；
- 后续如果需要 chunk parse 或 memory-mapped parse，再单独设计；
- 输出 `DataTable`。

远端 source：

- 使用 `network_semaphore` 限制请求；
- provider 内部可根据 API 限流再细分；
- retry/backoff 必须记录在 provider 诊断中，不能无限重试。

结果合并：

- 按 `DataLoadPlan.sources()` 顺序组装 `DataSet`；
- duplicate table name 仍稳定报错；
- 不按完成顺序改变表顺序。

### 6.4 Normalize

Normalize 可按 table 并行：

```text
for each table in DataSet:
  NormalizeTable(table_meta, table)
```

注意：

- default 应用只依赖当前 table；
- 输出仍按 DataSet 原 table 顺序收集；
- 若未来支持跨表 default 或表达式，需要显式依赖声明。

### 6.5 Validation

Validation 分为 table-local 和 dataset-level：

| 规则 | 并行粒度 |
| --- | --- |
| required/type/enum/field unique/index unique | table-local，可按 table 并行 |
| field-level range validator | table-local，可按 table 或 validator 并行 |
| row_export 后 target view 校验 | target + table 粒度并行 |
| foreign key / cross-table consistency | dataset-level，需等 table-local 通过或收集完 |

建议执行顺序：

```text
build ExportValidationView for target
  -> validate table-local rules in CPU pool
  -> validate dataset-level rules
  -> merge ValidationIssue stable sort
```

当前 validator 是 fail-fast 的 `Result<()>` 与 `ValidationReport` 混合模型。并行化前应先明确诊断收集策略：

- 如果继续 fail-fast：并行收益有限，且首个错误会受完成顺序影响；
- 建议中期迁移到收集所有 table-local issues，再按稳定 key 输出。

### 6.6 Runtime data export

Runtime data export 可拆成三段：

1. Coordination task：确定目标 artifact path，并在构建期汇总 artifact。
2. CPU task：按 table 构建 payload 并序列化为 bytes。
3. I/O task：写出文件。

```text
PlanRuntimeData(table)
  -> Artifact { path, content }
conflict check by path
bounded GenerateRuntimeData(table)
  -> ArtifactContent::Binary(bytes)
bounded WriteArtifact(path, bytes)
```

路径冲突检查必须在写入前完成：

```text
collect all Artifact
  -> sort by path
  -> reject duplicate path
  -> bounded generate/write pipeline
```

不要先把所有大表都序列化成 `Vec<u8>` 后再统一写出。并发序列化大表会显著增加内存峰值。短期建议：

```text
max_in_flight_artifacts = min(jobs, 2)
```

后续可以按 bytes 估算做更精确的 bounded channel；再往后才考虑 streaming writer。

JSON pretty 输出可读但体积大、CPU/IO 成本高。后续可增加 `compact_json` 选项，但这属于产物格式策略，应独立评审。

### 6.7 Code generation

Codegen 可按 `targets.code` task 并行。每个语言生成器内部也可以按 table 渲染局部片段，但第一阶段建议只做到 code task 粒度：

```text
for each CodegenTaskConfig:
  GenerateCode(language, target_tags, linked_meta, runtime_data_files)
```

原因：

- codegen 通常不是最大瓶颈；
- 单文件输出需要最终字符串拼接，内部并行会增加复杂度；
- 语言生成器需要保持声明顺序稳定。

若后续表数量很大，可让生成器内部先并行渲染 table section，再按 table order 拼接。

跨线程执行 codegen 前，需要补充 trait 线程安全契约：

```rust
pub trait CodeGenerator: Send + Sync { ... }
```

同理，若并行 runtime data export 共享 `DataFormatExporter` trait object，也需要 `Send + Sync`；或者每个任务重新构造 stateless exporter。未来外部插件也必须声明线程安全边界。

### 6.8 Artifact write

Artifact write 是 async I/O task。

建议语义：

```text
Artifact[]
  -> detect conflicts
  -> ensure parent dirs
  -> write temp files concurrently
  -> commit/rename deterministically
```

当前实现支持 dry-run、direct write 和 temp-commit。direct write 不保证失败时工作区不变；它只保证：

- path conflict 在任何写入前完成；
- 单文件写入失败有明确诊断；
- 不并发写同一路径；

`temp-commit` 写入模式先写同目录临时文件，再按稳定顺序 rename 到最终路径；写临时文件失败时清理已创建临时文件，避免提交未完成的最终产物。提交阶段失败不是全局事务回滚，已经 rename 成功的最终文件可能保留。

## 7. 调度策略

### 7.1 阶段式调度优先于全局 DAG

推荐先采用阶段式调度：

```text
config read/parse phase
meta link/validate phase
source load phase
normalize phase
validation phase
artifact generation phase
artifact write phase
```

优点：

- 容易保持现有语义；
- 容易定位错误阶段；
- 不需要一次性实现完整 DAG executor；
- 与当前 `build_target` 重构路径匹配。

当 source/provider/target 组合变复杂后，再把阶段式调度抽象成 DAG。

### 7.2 背压与限流

必须避免“上游无限产生结果，下游写不动”导致内存膨胀。

策略：

- I/O task 由 semaphore 控制并发；
- CPU task 由固定 pool 控制并发；
- 大 artifact 生成后应尽快写出或落入有界队列；
- 对远端 source 增加 provider-level 限流，避免触发 API rate limit。

### 7.3 取消与失败策略

建议支持 fail-fast cancellation，但必须区分错误类型：

- Config/Meta link 失败：取消后续全部任务。
- 某个 source 失败：可以继续等待已启动的 source 任务结束，以收集更多诊断；是否继续 normalize 取决于是否缺少必要表。
- Validation error：可以继续完成其他 table validation，输出更多错误；默认不进入 artifact write。
- Artifact write 失败：取消未开始的写任务，等待已开始写任务完成或记录失败。

技术实现：

- async task 使用 `CancellationToken`；
- CPU task 在任务边界检查 cancellation，不强行杀线程；
- task output 记录 cancellation reason，协调层统一合并为最终 diagnostics。

## 8. 诊断与可观测性

### 8.1 稳定诊断

并发执行后，诊断不能按任务完成顺序输出。每个诊断应带稳定位置：

```rust
pub struct DiagnosticLocation {
    pub phase: Phase,
    pub path: Option<PathBuf>,
    pub target: Option<String>,
    pub table: Option<String>,
    pub row: Option<usize>,
    pub field: Option<String>,
}
```

输出前统一排序。

### 8.2 Tracing

建议引入 `tracing`：

- 每个 task 创建 span：`phase`, `target`, `table`, `path`；
- debug 模式输出 task start/end/duration；
- 后续可导出 Chrome trace 或 JSON trace。

可新增 CLI：

```text
--trace-tasks <path>
```

第一阶段可只在 debug log 中输出 duration，不必立即实现 trace 文件。

## 9. API 重构建议

### 9.1 Exporter 入口

当前：

```rust
pub fn build_target(options: BuildOptions<'_>) -> Result<BuildOutput>;
```

建议演进：

```rust
pub async fn build_target_async(options: BuildOptions) -> Result<BuildOutput>;

pub struct BuildOptions {
    pub project_path: PathBuf,
    pub target_name: String,
    pub execution: ExecutionOptions,
}
```

async 入口建议使用 owned options，避免跨 `await`、task queue 或 tracing 保存时被 borrowed lifetime 限制。CLI 层可以从 `&Path` / `&str` 转换为 owned，并负责创建 runtime 或调用 async main。

### 9.2 Provider trait

当前 DataLoader 直接按 CSV 同步读取。第一阶段可以先保留同步 provider，并由 Rayon local pool 并行调度多个 source：

```rust
pub trait SourceProvider: Send + Sync {
    fn load_source(&self, source: &DataSourcePlan) -> Result<DataTable>;
}
```

远端 provider 落地后，再引入 async provider trait：

```rust
#[async_trait]
pub trait SourceProvider: Send + Sync {
    async fn load_source(
        &self,
        ctx: &ExecutionContext,
        source: &DataSourcePlan,
    ) -> Result<RawSourceData>;
}
```

本地 CSV provider 的实现：

1. 第一阶段：继续使用现有 streaming-ish 同步 CSV reader，受 `jobs` 限制并行多个 source；
2. 远端 provider 阶段：async request / backoff / network semaphore；
3. 未来可返回 `DataTable` 或更底层的 `RawSheet`。

如果远端 provider 阶段不想引入 `async_trait`，可以先使用 enum dispatch + async fn wrapper。

### 9.3 Artifact model

```rust
pub struct Artifact {
    pub path: PathBuf,
    pub content: ArtifactContent,
}

pub enum ArtifactContent {
    Text(String),
    Binary(Vec<u8>),
}
```

runtime data 和 generated code 都应进入同一写出管线。Artifact writer 只关心最终路径和文本/二进制内容，不承载 runtime data、generated code 或 producer 等业务语义。写出前必须统一完成路径归一化、稳定排序和 path conflict 检查；后续并发阶段应通过 bounded pipeline 生成和写出，避免所有 artifact content 同时驻留内存。

## 10. 迁移计划

### Phase 0：参数和上下文

- 新增 CLI `--jobs`、`--io-concurrency`、`--network-concurrency`、`--serial`。
- 新增 `ExecutionOptions` / `ExecutionContext`。
- 保持内部仍串行执行，只验证参数解析、默认值和传递链路。

### Phase 1：Source loading 并发

- 将 DataLoadPlan 的多个 source 并行加载。
- 本地 CSV 保持现有同步 reader，作为受 `jobs` 限制的 blocking/CPU source task 执行；远端 provider 落地后再引入 async I/O。
- 结果按 plan 顺序合并。
- 补充多 source 并发但稳定排序测试。

### Phase 2：Artifact generation / write 并发

- runtime data 按 table 并行构建 payload。
- codegen 按 code task 并行。
- 所有 artifact 先 plan，再统一冲突检查，再并发写。

### Phase 3：Validation 并发与诊断收集

- table-local validation 改为收集 issues。
- 按 table/target 并行运行。
- 统一排序后输出，避免完成顺序影响诊断。

### Phase 4：完整任务图与 tracing

- 抽象 TaskSpec / TaskExecutor。
- 增加 cancellation token。
- 增加 task tracing 和耗时统计。

## 11. 测试计划

### 11.1 单元测试

- `ExecutionOptions` 默认值、非法值、`--serial` 覆盖。
- TaskKey 排序稳定性。
- `TaskOutput` diagnostics 合并后按稳定 key 排序。
- Artifact path conflict 在并发生成后仍稳定报错。

### 11.2 集成测试

- `--jobs 1` 与默认并发输出完全一致。
- 多 table / 多 code task 项目在 `--jobs 2` 下输出稳定。
- source 加载失败时诊断顺序稳定。
- artifact 写出失败时不会产生重复/乱序诊断。

### 11.3 性能测试

增加 ignored large test 或 benchmark：

```text
100 tables x 10k rows
compare --jobs 1 / --jobs 2 / --jobs 4
measure source load, validation, runtime serialization, total duration
```

性能测试不应作为普通 CI 必跑项，避免 CI 抖动；可以作为 nightly 或手动命令。

## 12. 风险与约束

| 风险 | 缓解 |
| --- | --- |
| 并发改变诊断顺序 | 所有结果按稳定 key 汇总排序。 |
| 线程过多导致机器抢占 | 默认 `jobs <= 4`，所有 CPU 任务走固定 pool。 |
| async runtime 被 CPU 任务阻塞 | CPU 任务统一 `spawn_cpu`。 |
| 远程 API 限流 | `network_concurrency` + provider-level throttle/backoff。 |
| 大 artifact 同时驻留内存 | 有界队列、分阶段写出、未来 streaming。 |
| 生命周期复杂化 | `LinkedMetaModel<'meta>` 继续只读共享；并发任务只借用不可变引用或使用 `Arc` 包装 owned 数据。 |
| 输出半更新 | 后续引入 temp file + deterministic commit。 |

## 13. 第一阶段推荐落地点

结合当前代码，最适合优先落地：

1. CLI 增加 `--jobs` / `--io-concurrency` / `--serial`，先只传递不并发。
2. Data source 按 `DataLoadPlan.sources()` 并行加载，并保持结果顺序。
3. Runtime data export 按 table 并行序列化，artifact write 前统一检查路径。
4. Codegen 按 `targets.code` task 并行。

暂缓：

- Meta link 并行化；
- validation 全量并行化；
- streaming JSON；
- 完整 DAG executor。

这些暂缓项需要更多诊断模型、artifact 模型和基准测试支撑。
