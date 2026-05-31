# 任务系统与并发执行技术设计

本文档描述 ConfigForge 后续任务系统的技术实现方案。目标是在不改变现有导出语义、诊断顺序和产物稳定性的前提下，合理利用 CPU 多核与操作系统异步 I/O 能力，支持用户通过 CLI 限制资源消耗，避免默认占满整台机器。

当前实现已引入局部 Rayon CPU worker pool：`build_target` 仍按 Project Config 读取、选择 target、Meta Link、Data Load、Normalize、Validation、Runtime Data Export、Codegen 与写文件的阶段顺序执行；`execute_target_dag()` 只构造 execution-view skeleton，并按 `MetaAndData` / `MetaOnly` 两个分支执行，不是完整通用 DAG executor。DataLoader、runtime data serialization 和 codegen task 已在受 `jobs` 限制的 worker pool 中并行。远端 provider、async I/O、从目标产物倒推依赖的完整任务 DAG、full dataset validation / target view validation 分层并行、以及 bounded artifact pipeline 仍属于后续演进范围。本文档作为后续重构的工程设计依据。

## 1. 设计目标

### 1.1 功能目标

1. 将一次导出拆成显式任务图，表达阶段依赖和可并行粒度。
2. 从“用户请求的 target / export tasks”倒推需要的前置依赖：
   - code-only target 只需要 Project Config、Meta extract/link 和 MetaOnly codegen，不应读取 Data；
   - 含 MetaAndData export task 的 target 才需要 DataLoadPlan、SourceRead、Normalize、Validation 和 data-backed exporter；
   - 多 target 构建时，依赖 key 必须包含 target/effective tags，避免错误复用不同 target view。
3. 支持以下粒度的并行：
   - 按文件读取 Project Config / Meta imports；
   - 在 Project Config 与原始 MetaModel 已加载后，让 Meta link / validate 与 Data source 读取尽可能并行；
   - 按 source / table 读取数据，本地 CSV、Google Sheet、飞书文档等 provider 使用统一 source task 语义并按资源类型调度；
   - 按 table 做 normalize、validation、runtime payload 构建；
   - 在 full normalized DataSet 上并行运行 full-data validations；
   - 在每个 target 的 target-filtered DataSetView / ExportValidationView 上并行运行 target-aware validations；
   - 按 target、export task 和 table artifact 并行执行 data generation / aggregate exporter / MetaOnly codegen；
   - 按 artifact 写出文件。
4. 区分 I/O 等待任务和 CPU 密集任务：
   - 网络 source、远端表格 API、后续 bounded artifact write 优先使用 async I/O；
   - 本地 CSV 第一阶段可以继续使用同步 reader，并作为受 `jobs` 限制的 blocking/CPU source task 执行，避免为了 async 化而一次性读完整文件增加内存峰值；
   - CSV parse、Meta link、validation、codegen、serialization 使用受限 CPU worker pool。
5. CLI 可控制 CPU 并行度和 I/O 并发度，避免抢占所有机器资源。
6. 诊断、manifest、产物路径检查保持稳定排序，不受任务完成顺序影响。

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
    ProjectNormalize,
    TargetSelect,
    DependencyPlan,
    MetaExtract,
    MetaLink,
    MetaValidate,
    DataLoadPlan,
    SourceRead,
    MergeDataSet,
    NormalizeTable,
    MergeNormalizedDataSet,
    ValidateFullDataSet,
    BuildTargetView,
    ValidateTargetView,
    PlanArtifacts,
    GenerateDataArtifact,
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

### 5.1a 从目标产物倒推依赖

完整任务系统不应固定执行 “Meta -> Data -> Validation -> Export” 全流水线，而应先根据用户请求的 target 和 export tasks 构造 dependency plan。依赖倒推的基本规则：

| 请求 / Export task | 必要依赖 | 不应触发 |
| --- | --- | --- |
| 只包含 `MetaOnly` codegen（Rust / C++ / TypeScript） | Project Config、Target normalize、effective target tags、Meta extract、Meta link / meta validate、GenerateCode、Artifact plan/write | DataLoadPlan、SourceRead、NormalizeDataSet、Data validation、MetaAndData exporter |
| 包含 `MetaAndData` runtime / aggregate / embedded data exporter（JSON / MessagePack / bin / POT / Lua） | Project Config、Target normalize、effective target tags、Meta extract、Meta link / meta validate、DataLoadPlan、SourceRead、NormalizeDataSet、FullDataSet validations、TargetView validations、data artifact generation、Artifact plan/write | 与该 target 无关的其他 target export tasks |
| 多 target 构建（未来） | 每个 target 独立 effective tags / target view；共享 Project Config、raw MetaModel、Meta link；可按 `(project.input, effective_tags)` 复用 DataSet / validation 结果 | 只按 `project.input` 复用 DataSet；把一个 target 的 ExportValidationView 复用到另一个 target |

依赖 plan 的输出至少需要表达：

```rust
pub struct TargetBuildPlan {
    pub target_name: String,
    pub effective_tags: TargetTags,
    pub requires_data: bool,
    pub meta_only_exports: Vec<ExportTaskId>,
    pub meta_and_data_exports: Vec<ExportTaskId>,
}
```

当前文档和实现不足之处：

- `execute_target_dag()` 的 node key 只是 execution view skeleton，不是调度权威；文档若只写 “DAG” 容易误导为已有完整 DAG executor。
- 当前 build 仍先 link Meta，再进入 target-local executor；目标态应在 Project Config / raw MetaModel 加载后，通过 dependency plan 判断哪些 target 需要 Data，避免 code-only target 读取 Data。
- 当前 `NormalizeAndValidate` 把 source load、normalize、full validation 和 target validation 合并成一个分支步骤；目标态需要拆开，才能表达 source I/O 与 Meta link 并行、full dataset validation 与 target-view validation 分层并行。
- 当前只支持一次构建一个 target；目标态 multi-target 需要显式 cache key：`LinkedMeta` 可共享，Data / normalized data / validation result 至少按 `(project.input, effective_tags)` 或更细粒度 key 区分。

### 5.1b 目标态 DAG 形态

在完成基础工程加载后，目标态 DAG 应接近下列结构：

```text
ConfigRead(root/imports)
  -> ProjectNormalize
  -> TargetSelect / NormalizeTargets
  -> DependencyPlan(target exports)

ProjectNormalize
  -> MetaExtract
  -> MetaLink / MetaValidate

DependencyPlan(target requires data?)
  -> if any MetaAndData target:
       DataLoadPlan(project.input, effective_tags)
         -> SourceRead(source A)       # local CSV = blocking/CPU or file I/O
         -> SourceRead(source B)       # Google Sheet = async network I/O
         -> SourceRead(source C)       # Feishu/Lark doc/sheet = async network I/O
       SourceRead[*] -> MergeDataSet(plan order)

MetaLink + MergeDataSet
  -> NormalizeTable(table*) in parallel
  -> MergeNormalizedDataSet
  -> ValidateFullDataSet(rule/table*) in parallel

ValidateFullDataSet + TargetSelect(target N)
  -> BuildTargetView(target N, effective_tags)
  -> ValidateTargetView(target N, rule/table*) in parallel

MetaLink + TargetView/Validation success
  -> PlanArtifacts(all requested exports)
  -> GenerateDataArtifact(export/table*) in parallel
  -> GenerateCode(target MetaOnly export*) in parallel
  -> final path conflict check
  -> WriteArtifact(*) bounded / deterministic
```

关键并行点：

1. `MetaLink / MetaValidate` 与 `SourceRead[*]` 可以并行：source provider 读取原始表格不应依赖 linked Meta；需要 Meta 的 typed normalize / validation 在二者之后执行。
2. 多个 `SourceRead` 按 provider 资源类型调度：本地 CSV 可以作为 blocking/CPU source task，Google Sheet / 飞书文档属于 async network I/O task，并受 `network_concurrency` 和 provider-level throttle 限制。
3. `ValidateFullDataSet` 在 normalized full DataSet 上运行，与具体 target 无关；适合按 table / validator group 并行并稳定合并 diagnostics。
4. 每个 target 的 `BuildTargetView` / `ValidateTargetView` 依赖 full validation 成功或至少 full validation diagnostics 收集完成；target-aware rules 在 target-filtered DataSetView / ExportValidationView 上运行，不应直接遍历未经 target 过滤的 full DataSet。
5. 所有生成任务都必须在 artifact path planning 后或生成前可推导 path 时参与统一 conflict check；生成任务之间可并行，但结果按 target / export declaration order 稳定收集。

### 5.1c Validation 分层依赖

为了支撑 full DataSet 与 target view 并行验证，Validation 需要显式拆分规则层级：

| 层级 | 输入 | 示例规则 | 并行粒度 | 依赖 |
| --- | --- | --- | --- | --- |
| Normalize | Raw `DataSet` + linked Meta | default、typed struct/list 解析、ObjectTable logical row | table | `MetaLink + MergeDataSet` |
| FullDataSet validation | normalized full `DataSet` + linked Meta | required/type/enum、struct boundary、full FK、full unique、全局 validator | table / validator group；跨表规则按 relation group | Normalize 完成 |
| TargetView build | normalized full `DataSet` + target tags | table/field/row visibility、ExportValidationView | target | FullDataSet validation 完成或 no-error gate |
| TargetView validation | target-filtered view | target-aware required、target-visible unique/FK、validator `export_when` | target + table / validator group | TargetView build |

需要避免的设计：

- 在 target-aware validation 中重新 normalize raw data；normalize 应只做一次，target view 应从 normalized full DataSet 派生。
- 在 full validation 未完成时启动 data exporter；否则导出可能使用未经过全局一致性验证的数据。
- 用共享 mutable `ValidationReport` 从多个 worker 直接 push；应让每个 validation task 返回局部 report，再由 coordination 层稳定排序合并。

### 5.2 稳定性要求

任务可以乱序完成，但结果合并必须稳定：

- Project Config imports 按 canonical path 或 import 声明顺序稳定合并；
- tables 按 MetaModel 中顺序输出；
- export tasks 按 Project Config 中顺序输出；
- diagnostics 使用当前 validation stable key `(severity, target, table, row_index, source_row_number, field, validator, code, message)` 排序；
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
| Codegen | 一个 MetaOnly export task / language | 每个字段、每个小片段 |
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

### 6.2 Meta extract / link / validate 与 Data source read 的并行边界

Meta extract 是从 ProjectConfig clone/移动 `enums`、`structs`、`tables` 到 `MetaModel`。

Meta link 当前依赖完整 `MetaModel`，适合保持单阶段执行。后续如果表很多，可以并行预构建局部索引或校验表内字段，但最终 `LinkedMetaModel<'meta>` 的引用构建必须保证生命周期清晰。

建议第一阶段：

- Link 保持串行；
- Meta validate 中不依赖全局可变状态的 table/field 规则可并行收集 diagnostics；
- diagnostics 统一排序后返回。

目标态 DAG 中，`MetaLink / MetaValidate` 不应阻塞所有 source read：

```text
ProjectNormalize
  -> MetaExtract -> MetaLink / MetaValidate
DependencyPlan(requires_data)
  -> DataLoadPlanSourceList -> SourceRead[*]
MetaLink + SourceRead[*] -> NormalizeTable[*]
```

这要求 DataLoadPlan 拆成两个层级：

1. **Source discovery / raw source list**：只需要 Project Config 中的 `project.input`、Meta raw source declarations 和 target effective tags，用于决定需要读取哪些 source；该层应尽量不依赖 linked Meta。
2. **Typed load / normalize plan**：需要 linked Meta，负责把 raw source rows 解释成 typed normalized DataSet。

如果某些 source discovery 规则确实依赖 linked Meta（例如未来按 field type 决定 provider projection），必须在 DAG 中显式声明 `MetaLink -> DataLoadPlan` 依赖；不能隐藏在 SourceRead 内部，否则会破坏 code-only target 不读 Data、以及 source I/O 与 Meta link 并行的目标。

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
- Google Sheet、飞书文档 / 表格等 provider 必须作为 async I/O task，不应占用 CPU worker 等待网络；下载后的 CSV/Sheet cell parse 可再转入 CPU task。

结果合并：

- 按 `DataLoadPlan.sources()` 顺序组装 `DataSet`；
- duplicate table name 仍稳定报错；
- 不按完成顺序改变表顺序。

如果一个 target 只有 MetaOnly exports，DependencyPlan 必须跳过本节所有任务，并通过测试证明 `DataLoader` / provider registry 没有被调用。

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

Validation 分为 full DataSet 层与 target view 层，而不是单一 “validation phase”：

| 规则 | 并行粒度 |
| --- | --- |
| required/type/enum/field unique/index unique | table-local，可按 table 并行 |
| field-level range validator | table-local，可按 table 或 validator 并行 |
| full DataSet foreign key / cross-table consistency | dataset-level，可按 FK relation group 并行；需等 table-local 通过或收集完 |
| row_export / table export / field export 后 target view 校验 | target + table 粒度并行 |
| target-visible foreign key / unique / custom validator | target + table / relation group 粒度并行 |

建议执行顺序：

```text
NormalizeTable[*]
  -> MergeNormalizedDataSet
  -> ValidateFullDataSet(table-local rules in CPU pool)
  -> ValidateFullDataSet(dataset-level relation groups)
  -> for each target:
       BuildTargetView(target, effective_tags)
       -> ValidateTargetView(target table-local rules in CPU pool)
       -> ValidateTargetView(target relation groups)
  -> merge ValidationIssue stable sort
```

当前 validator 主流程已迁移为 report-backed 模型：`*_with_report()` 先收集 `ValidationReport`，兼容 `Result<()>` 入口再通过 `into_result()` 做 error gate。Phase 8/9 并行化时必须保持这一模型：每个并行 task 返回局部 report，由 coordination 层 `append()` / `extend()` 后按 stable key 排序，不能从 worker 直接写共享 mutable report，也不能回退为“完成最快的错误先返回”。

Full validation 与 target validation 的失败策略应分层：

- FullDataSet error 默认阻断所有依赖该 DataSet 的 target data export，但仍可继续收集其他 full validation diagnostics。
- 某个 target 的 TargetView validation error 只阻断该 target 的 data-backed exports；不应影响另一个 target 的 validation 和 MetaOnly codegen。
- Warning 级 validation issue 汇总到对应 target / build output，不能因为并行执行而丢失归属。

### 6.6 Runtime data / aggregate / embedded data export

Data-backed export 可拆成三段：

1. Coordination task：确定目标 artifact path，并在构建期汇总 artifact。
2. CPU task：按 export task / table / aggregate group 构建 payload 并序列化为 bytes。
3. I/O task：写出文件。

```text
PlanRuntimeData(table)
  -> Artifact { path, content }
conflict check by path
bounded GenerateDataArtifact(export, table_or_aggregate)
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

当前 duplicate path key 是 lexical-normalized path；它不解析 symlink，也不覆盖大小写不敏感文件系统上的 case alias。后续并发写出管线如果引入更强的 filesystem-aware path key，必须保持“写入前完成冲突检查”的边界，并遵循 [`platform-filesystem-semantics.md`](platform-filesystem-semantics.md) 中的 canonicalize + lexical fallback 策略。

不要先把所有大表都序列化成 `Vec<u8>` 后再统一写出。并发序列化大表会显著增加内存峰值。短期建议：

```text
max_in_flight_artifacts = min(jobs, 2)
```

后续可以按 bytes 估算做更精确的 bounded channel；再往后才考虑 streaming writer。

JSON pretty 输出可读但体积大、CPU/IO 成本高。后续可增加 `compact_json` 选项，但这属于产物格式策略，应独立评审。

生成任务之间可以并行，但必须满足：

- JSON / MessagePack / bin table artifact 可按 table 并行。
- gettext POT、Lua table 这类 aggregate / embedded data exporter 可以按 exporter 内部安全粒度并行；若需要全局去重或 root facade 汇总，必须先并行收集局部结果，再在 coordination 层稳定合并。
- 一个 target 的 data-backed exporter 必须等待该 target 的 TargetView validation 成功；MetaOnly codegen 不等待 Data 或 TargetView validation。

### 6.7 Code generation

Codegen 可按 `targets.exports` 中的 MetaOnly export task 并行。每个语言生成器内部也可以按 table 渲染局部片段，但第一阶段建议只做到 export task 粒度：

```text
for each MetaOnly NormalizedExportTask:
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

### 7.1 从 branch executor 过渡到依赖驱动 DAG

当前代码的 `execute_target_dag()` 是 branch executor：它按 export dependency contract 拆成 `MetaOnly` / `MetaAndData` 两条分支，并构造 node key 供测试和未来 tracing 使用。它尚不具备：

- 从请求的 target/export task 反向裁剪依赖图；
- node-level readiness scheduling；
- Meta link 与 SourceRead 的交错并行；
- full DataSet validation 与 target view validation 的分层调度；
- multi-target cache key / result reuse；
- cancellation、tracing、duration 统计等统一 task runtime 能力。

迁移策略不应直接跳到“大而全 DAG executor”，而应采用 **dependency-plan first, executor later**：

1. 先让 build normalization 产出明确的 `TargetBuildPlan` / `BuildDependencyPlan`，准确表达 code-only target 不需要 Data。
2. 再把当前 `NormalizeAndValidate` 大步骤拆成 source read、merge、normalize、full validation、target view validation 等可测试函数。
3. 继续用阶段式/局部并行执行这些函数，但每个阶段的输入输出和 cache key 必须已接近 DAG node。
4. 最后引入通用 DAG executor，替换手写阶段编排。

推荐过渡形态：

```text
BuildDependencyPlan
  -> execute_meta_side()
  -> if requires_data: execute_data_side_until_raw_sources()
  -> join(MetaLink, RawSources) for normalize
  -> execute_full_validation()
  -> for each target: execute_target_view_validation()
  -> execute_generators()
  -> ArtifactWriter
```

这样每一步都能独立落地、独立测试，并保持现有 CLI 输出与 artifact 稳定性。

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
- FullDataSet validation error：可以继续完成其他 full validation task，输出更多错误；阻断依赖该 DataSet 的 data-backed export。
- TargetView validation error：只阻断对应 target 的 data-backed export；其他 target 和 MetaOnly codegen 不受影响。
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

## 10. 渐进式改造 TODO list

每一步都必须保持 `--jobs 1` 与当前串行语义一致，并提供单元或集成测试证明该步骤可独立工作。不要把 async provider、validation 重构、DAG executor 和 artifact streaming 混在一个大改动里。

### Phase 0：现状标定与测试护栏

目标：在改调度前锁定当前行为，避免后续并发化改变语义。

- DONE（6e91753）：补充 code-only target 测试，断言只有 MetaOnly export task 时不会调用 DataLoader / provider。
- DONE（6e91753）：补充多 export task declaration order 测试，覆盖 data-backed export 与 codegen 混排后的 artifact order。
- DONE（6e91753）：补充 validation diagnostics 稳定排序基线测试，覆盖多个 range validator issue 的稳定顺序；FK / target-aware 场景在后续 validation 并行阶段继续扩展。
- DONE（6e91753）：补充 `--serial` / `--jobs 1` 与默认执行输出一致的 CLI integration。
- 验证：`cargo test`；如引入测试辅助 provider，必须能精确记录调用次数和调用顺序。

### Phase 1：DependencyPlan / TargetBuildPlan

目标：先实现“从 target/export tasks 倒推依赖”，但执行仍可沿用当前 branch executor。

- DONE：新增内部 `BuildDependencyPlan` / `TargetBuildPlan`，由 normalized target exports 生成 `requires_meta`、`requires_data`、`meta_only_exports`、`meta_and_data_exports`。
- DONE：让 code-only target 显式 `requires_data = false`，并在 build 主流程中跳过 DataLoadPlan / DataLoader / normalize / validation。
- DONE：记录 cache key 约定：`LinkedMeta` 可按 project/raw meta 共享；Data / normalized data / validation result 至少按 `(project.input, effective_tags)` 区分，当前内部表达为 `DataDependencyKey { input, effective_tags }`。
- DONE：将当前 `BuildDagNodeKey` skeleton 与 `TargetBuildPlan` 对齐，避免 node key 表达伪依赖。
- 验证：build 单元测试覆盖 `rust only`、`json only`、`json + rust`、`json + lua + rust`；CLI integration 覆盖 code-only target 不要求 `project.input`。

Phase 1 实现复核：后续 Phase 2-12 的阶段顺序仍然成立；本阶段仅把“target/export task 需要哪些依赖”收敛为内部 plan，尚未改变 DataLoad / normalize / validation 的执行边界。后续开发应复用 `TargetBuildPlan.data_dependency` / `DataDependencyKey` 作为 data-backed 分支的最小启动条件，避免再次从 exporter 类型反推是否需要 Data。

### Phase 2：拆分 DataLoadPlan 为 source discovery 与 typed normalize

目标：让 source read 可以与 Meta link 并行，为远端 provider 做准备。

- DONE：把“需要读取哪些 source”的 raw source discovery 从 typed normalization 中拆出来，明确它只依赖 Project Config / raw Meta source declarations / effective tags。
- DONE：定义 `SourceReadPlan` / `RawSourceData`，本地 CSV 仍可直接返回现有 `DataTable`，但接口层不要要求 linked Meta。
- DONE：保留现有 DataLoader 行为作为 compatibility path，先由 `DataLoadPlan` 包装 `SourceReadPlan`，`load()` / `load_with_pool()` 继续返回兼容的 `DataSet`。
- DONE：当 source discovery 未来需要 linked Meta 时，必须在 plan 中显式声明依赖，不能在 provider 内隐式访问 MetaLink 结果；当前实现保持 `SourceReadPlan::from_tables(raw TableMeta, effective tags)`，不借用 `LinkedMetaModel`。
- 验证：多 source / override source / ObjectTable source fixture 与现有输出一致；source discovery 单元测试覆盖 target tags 对 override source 的影响。

Phase 2 实现复核：后续 Phase 3-12 的阶段顺序总体仍然成立，但 Phase 3 的基础并发读取入口已提前收敛到 `DataLoader::read_sources_with_pool(&SourceReadPlan, pool)`。Phase 3 后续重点应从“创建并发入口”调整为：验证 `SourceReadPlan` 粒度的错误稳定性、jobs/serial 资源限制语义，以及为未来 fake delayed provider 补充完成顺序不影响 merge 顺序的测试。

### Phase 3：SourceRead 并发（本地同步 provider）

目标：先并发读取本地 CSV，但不引入 Tokio 和远端 provider。

- DONE：按 `SourceReadPlan` 粒度在局部 Rayon pool 中并行读取 CSV；Phase 2 已提供 `read_sources_with_pool()`，Phase 3 补齐并发稳定性测试入口。
- DONE：结果按 plan order merge，错误按 source stable key 排序，不按完成顺序输出。
- DONE：限制并发数由 `jobs` 控制；`--serial` 必须退化为串行。当前 build 通过 `ExecutionOptions.jobs` 创建 CPU pool，serial 已归一化为 `jobs = 1`。
- DONE：保证 duplicate table / missing source 等错误与现有语义一致。
- 验证：DataLoader 单元测试覆盖多 source 并发稳定合并；CLI integration 比较 `--jobs 1` / `--jobs 2` 输出；可增加人工延迟测试 provider 验证顺序稳定。

Phase 3 实现复核：后续 Phase 4-12 的阶段顺序仍然成立。Phase 4 引入 async provider shell 时应复用 `SourceReadPlan` 作为 provider-independent 输入，并把当前测试用 reader seam 演进为正式 provider trait / resource lane；不要在 DataLoader 内隐式访问 MetaLink 结果。

### Phase 4：引入 async provider shell（Google Sheet / 飞书准备）

目标：为远端 source 提供 async I/O 调度能力，但不改变本地 CSV 主路径。

- DONE：引入 async provider shell 雏形。当前不引入 Tokio，先用 owned `SourceReadPlan` + scoped worker shell 表达可取消 / 可限流的 provider-independent 调度边界，避免借用跨未来 `await`。
- DONE：定义 provider resource 类型：`BlockingLocal`、`AsyncIo`、`Network`；Google Sheet / 飞书文档 provider 后续应声明为 `Network` 并走 `network` concurrency lane。
- DONE：为远端 provider 预留 `RemoteProviderPolicy`，记录 retry/backoff、rate-limit 入口；认证错误和 source location diagnostics 仍在正式远端 provider 落地时细化。
- DONE：明确下载 bytes/cells 后再切回 CPU pool 做 parse/normalize，不在 async runtime worker 上做重 CPU 工作；当前 shell 只负责 source read，normalize 仍在后续阶段。
- 验证：使用 fake async provider 的单元测试覆盖 semaphore 限流、失败诊断稳定排序、取消行为；本地 CSV integration 不变。

Phase 4 实现复核：后续 Phase 5-12 的阶段顺序仍然成立。Phase 5 可以基于 `AsyncSourceReadShell` 的 stop-scheduling 语义扩展为 MetaLink failure 触发 cancellation token；正式 Tokio / network provider 可在 Google Sheet / 飞书 provider 进入实现阶段再引入，避免当前本地 CSV 主路径承担 async runtime 复杂度。

### Phase 5：MetaLink 与 SourceRead 并行

目标：在 Project Config / raw MetaModel 完成后，让 Meta link / validate 与 source I/O 同时启动。

- DONE：重排 build 主流程：`MetaExtract -> MetaLink` 与 `DependencyPlan.requires_data -> SourceRead[*]` 可并行，二者在 Normalize 前 join。Source discovery 现在基于 raw `MetaModel` + effective target tags 构造 `SourceReadPlan`，不等待 `LinkedMetaModel`。
- DONE：Meta link failure 与 source failure 并发时稳定优先返回 MetaLink 错误；当前本地 CSV source read 无 cancellation token，已启动任务会等待完成并丢弃结果，未启动远端任务的协作取消保留到正式 async provider runtime。
- DONE：确保 code-only target 不启动 source tasks；data-backed target 等待 MetaLink + RawSources 后再 normalize。
- 验证：fake slow MetaLink / fake slow SourceRead 的调度测试，证明可重叠执行；错误场景覆盖 Meta error、source error、双错误稳定输出。

Phase 5 实现复核：后续 Phase 6-12 的阶段顺序仍然成立。Phase 6 可直接消费 Phase 5 join 后的 `LinkedMetaModel + RawSourceData`，把 `normalize_data_set` 拆为 table 粒度任务；完整 cancellation token 仍建议随远端 provider / 通用 DAG executor 阶段落地，而不是在本地 CSV loader 中引入额外复杂度。

### Phase 6：NormalizeTable 并行与 normalized DataSet merge

目标：把 raw source merge 后的 normalize 拆成 table 粒度 CPU task。

- DONE：`normalize_data_set` 拆分为 `normalize_table` task + deterministic merge；build 主流程通过 `normalize_data_set_with_pool()` 使用 CPU pool 并按输入 table 顺序稳定收集。
- DONE：ObjectTable singleton、struct expanded value、defaults、typed fragment payload 等规则保持 table-local；当前 normalize task 只读取当前 `LinkedTableMeta` 与当前 `DataTable`。
- DONE：明确如果发现跨表 normalize 需求，必须显式新增 DAG 依赖，不能在 table task 中访问可变全局状态。
- 验证：现有 validation/data resolver tests 全部通过；新增多 table normalize 顺序稳定测试；大表/小表 chunking 可先不实现。

Phase 6 实现复核：后续 Phase 7-12 的阶段顺序仍然成立。Phase 7 仍应先把 validation 诊断改为全量可合并模型，再并行 validation；Phase 8/9 可复用 Phase 6 的稳定 merge 策略，但需要自己的 validation issue stable sort key，不能依赖 Rayon 完成顺序。

### Phase 7：Validation report 全量收集模型

目标：先改诊断模型，再并行 validation，避免 fail-fast 与并发冲突。

- DONE：将基础 required/type/enum/unique/FK 与 registry validators 统一到可合并的 `ValidationReport` 输出；兼容入口保留 `Result<()>`，但 error gate 返回 `ConfigForgeError::ValidationReport`。
- DONE：定义 validation issue stable sort key，包含 severity、target、table、row_index、source_row_number、field、validator、code、message。
- DONE：明确 warning / error gate：warning 不阻断 export，error 阻断对应依赖。
- 验证：validation 单元测试不只断言失败，还断言多个 issue 的稳定顺序；现有 CLI 错误信息可接受兼容变化时需同步文档。

Phase 7 实现复核：后续 Phase 8-12 的阶段顺序仍然成立。Phase 8/9 可以直接复用 `ValidationReport::append()` / `extend()` / `sort_by_stable_key()` 做并行结果合并；需要补充的是 table-local validation 与 FK/relation-group validation 的任务拆分、局部 report 生命周期，以及 `--jobs 1` / `--jobs N` 下 report 内容和排序完全一致的测试。Phase 10-12 不需要因 report 模型调整顺序，但 data-backed artifact generation 必须继续等待对应 validation report 无 error。

### Phase 8：FullDataSet validation 并行

目标：在 normalized full DataSet 上并行运行与 target 无关的验证。

- DONE：把 table-local full validation 按 table 并行；`validate_normalized_data_set_with_report_and_pool()` 在 Rayon pool 中为每个 table 生成局部 report，再按 Meta table order 稳定 append。
- TODO：把 FK / cross-table validation 按 relation group 或 target table index cache 并行，同时避免重复构建昂贵索引。当前 Phase 8 仍在 table-local report 合并后串行执行 full FK 与 registry validators。
- TODO：FullDataSet error 阻断所有 data-backed target exports，但继续收集其他 full validation diagnostics。当前 single-target build 仍读取 target-scoped raw tables，完整 full DataSet gate 需要 Phase 9 / multi-target DAG 先明确 full normalized DataSet 的 cache key 与 target view 派生关系。
- 验证：full validation 单元测试覆盖多表多错误稳定排序、FK cache 并发安全、`--jobs 1` / `--jobs 4` 结果一致。

Phase 8 实现复核：后续 Phase 9-12 的阶段顺序仍然成立。Phase 9 需要在目标态 DAG 中区分“full normalized DataSet”与“target-scoped SourceRead/ExportValidationView”：如果继续按 effective tags 读取 source，则不能把该 DataSet 当成跨 target full DataSet 复用；若要启用真正 FullDataSet gate，必须先定义 full source read / target override source 的 cache key 和 TargetView 派生规则。Phase 10-12 不需要调整顺序，但 artifact generation 仍必须等待对应 validation gate 无 error。

### Phase 9：TargetView build 与 TargetView validation 并行

目标：每个 target 在 target-filtered DataSetView / ExportValidationView 上独立验证。

- TODO：显式构建 `TargetDataSetView` / `ExportValidationView` task，key 为 `(target, effective_tags, normalized_data_set_id)`。
- TODO：target-aware validation 按 target + table / relation group 并行。
- TODO：某 target validation error 只阻断该 target 的 data-backed exports；不影响其他 target 或 MetaOnly codegen。
- TODO：多 target 时复用 full normalized DataSet，但不能复用 target view。
- 验证：集成测试覆盖两个 target 使用不同 tags、row_export / field export / override source 结果不同；一个 target validation 失败不影响另一个 target codegen dry-run plan。

### Phase 10：Artifact planning 与 generator 并行

目标：在 validation gate 之后并行生成 data artifacts 和 codegen artifacts，同时保持 path conflict 与输出顺序稳定。

- TODO：拆出 `PlanArtifacts`，在重 CPU 生成前尽可能计算所有输出路径，并做 lexical / future filesystem-aware conflict check。
- TODO：JSON / MessagePack / bin 按 table artifact 并行；POT / Lua 等 aggregate exporter 按内部安全粒度并行并稳定合并。
- TODO：MetaOnly codegen 按 export task 并行，不等待 Data 或 TargetView validation。
- TODO：生成结果按 `(target declaration order, export declaration order, artifact path order)` 稳定收集。
- 验证：多 exporter / 多 codegen 混合项目 `--jobs 1` 与 `--jobs 4` artifact plan 完全一致；路径冲突在任何写入前报错。

### Phase 11：Bounded artifact write pipeline

目标：避免所有大 artifact bytes 同时驻留内存，并为 async write 做准备。

- TODO：Artifact Writer 支持 bounded queue / max in-flight artifacts。
- TODO：Direct / temp-commit / dry-run 保持当前语义；temp-commit commit 顺序稳定。
- TODO：接入 `platform-filesystem-semantics.md` 中的 path key 策略，保证 write 前完成冲突检查。
- 验证：dry-run 不创建目录；写入失败测试覆盖 Direct 和 TempCommit；大 artifact benchmark 观察 peak memory。

### Phase 12：通用 DAG executor 与 tracing

目标：在各阶段 node 输入输出稳定后，用统一 executor 替换手写阶段编排。

- TODO：实现 `TaskSpec` / `TaskGraph` / readiness queue / resource-aware scheduler。
- TODO：支持 `AsyncIo`、`Network`、`Cpu`、`Coordination` resource lane 和 cancellation token。
- TODO：为每个 task 输出 tracing span、duration、result state；可选 `--trace-tasks <path>`。
- TODO：保留阶段式 executor 的 golden integration 测试，切换 DAG executor 后输出必须一致。
- 验证：task graph 单元测试覆盖依赖拓扑、取消传播、资源限流、稳定 result merge；全量 `cargo test`。

## 11. 测试计划

### 11.1 单元测试

- `ExecutionOptions` 默认值、非法值、`--serial` 覆盖。
- `BuildDependencyPlan` 对 code-only / data-backed / mixed export tasks 的依赖裁剪。
- TaskKey 排序稳定性。
- `TaskOutput` diagnostics 合并后按稳定 key 排序。
- SourceRead fake provider 并发完成顺序不影响 DataSet merge 顺序。
- FullDataSet validation 与 TargetView validation issue 稳定排序。
- Artifact path conflict 在并发生成后仍稳定报错。

### 11.2 集成测试

- `--jobs 1` 与默认并发输出完全一致。
- code-only target 不读取 Data，即使 project 配置了 `input` 或 source 文件缺失也不触发 data source error。
- 多 table / 多 codegen export task 项目在 `--jobs 2` 下输出稳定。
- 多 target / 多 effective tags 构建不错误复用 DataSetView；target validation failure 不影响无依赖 target 的 MetaOnly codegen。
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
| DAG dependency key 过粗导致错误复用 | Data / validation / target view cache key 必须包含 `project.input`、effective tags、target、source provider 参数等必要维度。 |
| code-only target 意外读取 Data | DependencyPlan 必须先于 DataLoadPlan，并有测试 provider 断言未调用。 |
| 生命周期复杂化 | `LinkedMetaModel<'meta>` 继续只读共享；并发任务只借用不可变引用或使用 `Arc` 包装 owned 数据。 |
| 输出半更新 | 后续引入 temp file + deterministic commit。 |

## 13. 当前文档缺口总结

在本轮补充前，设计文档中不足以支撑目标态 DAG 的点主要是：

1. 对 “DAG” 的描述偏阶段式，容易让人误以为当前 `BuildDagNodeKey` 已参与调度；实际只是 execution-view skeleton。
2. 缺少从 target/export task 倒推依赖的规则，尤其是 code-only target 不读 Data 的硬约束。
3. `NormalizeAndValidate` 被描述为单个节点，无法表达 SourceRead、Normalize、FullDataSet validation、TargetView validation 的并行与 gate。
4. 没有明确 Meta link / validate 与 source read 可以并行，也没有说明 source discovery 应尽量不依赖 linked Meta。
5. Validation 只粗分 table-local / dataset-level，缺少 full DataSet 与 target-filtered DataSetView 的分层依赖。
6. 对 Google Sheet / 飞书文档这类 async network source 的 resource lane、限流和 CPU parse 边界描述不足。
7. 迁移计划过粗，无法保证每一步独立可工作、可验证、可回滚。

后续实现应优先按 Phase 0～Phase 3 建立依赖裁剪与 source 并发基础，再进入 async provider 和 validation 并行；完整 DAG executor 应放在各阶段输入输出稳定之后。
