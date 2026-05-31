# 10. Build 模块详细设计

本文档描述 `src/build/` 的工程构建编排职责。Build 是顶层业务 orchestrator，负责把 Project / Meta / Data / Validation / Exporter / Codegen 串成完整 target build；其他业务模块不能反向依赖 Build。

## 10.1 设计目标

Build 模块需要解决：

1. 从 CLI options 加载工程配置。
2. 选择并归一化 target。
3. 创建和管理 CPU worker pool。
4. 按 target tags 组织 runtime data export。
5. 在导出前加载、归一化、校验数据。
6. 支持一个 target 内多个 exporter task，并按 exporter 声明的 dependency contract 复用 Meta / Data / Validation 节点。
7. 运行时数据格式选择停留在 exporter / runtime loader 层，不进入 MetaOnly codegen 绑定。
8. 收集所有 exporter artifacts，交给 `artifact` 模块统一检查路径冲突、dry-run 和写出文件。

非目标：

- 不把 Project Config schema 放在 build 内；schema 属于 `project` / `exporter` / `codegen`。
- 不把 runtime data export type 实现放在 build 内；格式属于 `exporter`。
- 不把语言生成器实现放在 build 内；生成器属于 `codegen`。

## 10.2 模块结构

```text
src/build/
  mod.rs              # build_target 主入口、target-local execution view、branch executor、artifact 收集与写入调用
  options.rs          # BuildOptions / ExecutionOptions / BuildOutput / BuildTrace
  target.rs           # TargetConfig normalization、export task naming、dependency validation
  runtime.rs          # MetaAndData export task 编排、data load / validation / export
```

职责：

| 子模块 | 责任 |
| --- | --- |
| `options.rs` | CLI 传入 build 的稳定参数对象；执行资源限制默认值与合法性；BuildTrace / stage metrics 输出模型。 |
| `target.rs` | target tags、export task 默认命名、export task 名称校验、dependency 与禁止 task-level input/data 的字段校验。 |
| `runtime.rs` | MetaAndData export task 执行：消费已准备的 `RawSourceData`、复用 normalized DataSet、收集 validation warning、调用 exporter。 |
| `mod.rs` | `build_target()` 主流程、target-local execution view 构造、MetaAndData / MetaOnly branch executor、artifact 稳定排序与 Artifact Writer 调用。 |

当前 `target.rs` 输出 `NormalizedExportTask`；`mod.rs` 以 exporter dependency contract 构造 target-local execution view，并直接执行 MetaAndData / MetaOnly 两个分支。`BuildDagNodeKey` 当前是测试和未来通用 executor 的 skeleton，不参与缓存或诊断。`BuildTraceRecorder` 在现有阶段式 executor 边界记录 stage name、relative start、duration 和 result state，并通过 `BuildOutput.trace` 暴露给 CLI 的 `--trace-tasks` JSON 输出。`MetaModel::link()` 与 `SourceReadPlan -> RawSourceData` 已下沉到 build input preparation，并在 data-backed target 中并行执行；后续若引入完整 DAG executor，可把这些节点和分支执行统一下沉到 node-level scheduler。

## 10.2a 统一 Export Task 与 DAG

`TargetConfig` 使用统一 `exports` 描述 Lua data+code 合一导出、gettext POT、asset manifest、schema manifest、runtime data 和 codegen 等混合产物。Build 的内部模型为：

```rust
pub(crate) struct NormalizedTargetConfig {
    pub(crate) name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) exports: Vec<NormalizedExportTask>,
}

pub(crate) struct NormalizedExportTask {
    pub(crate) name: String,
    pub(crate) type_id: String,
    pub(crate) output: PathBuf,
    pub(crate) dependency: ExportDependency,
    pub(crate) options: BTreeMap<String, toml::Value>,
}

pub(crate) enum ExportDependency {
    MetaOnly,
    MetaAndData,
}
```

`dependency` 不应由用户手写，而应来自 exporter registry。Build normalization 阶段会从 data exporter registry 与 code generator registry 查询 `type_id` / alias 对应的 dependency contract，并写入 `NormalizedExportTask.dependency`：

- Rust / C++ / TypeScript codegen adapter：`MetaOnly`。
- JSON / MessagePack / bin runtime data exporter：`MetaAndData`。
- gettext POT：`MetaAndData`。
- Lua table：`MetaAndData`。

不提供 `DataOnly` 模式：当前和可预见的 exporter 都需要 Meta 来完成类型解释、target 可见性、校验和 artifact 规划。若未来出现不需要 Meta 的纯文件搬运类能力，应作为独立 build step 重新设计，而不是复用 target export task dependency contract。

当前 execution view skeleton 使用下列节点 key；实际执行仍由 `execute_target_dag()` 内的两个分支完成：

| Node | Key | 产出 | 说明 |
| --- | --- | --- | --- |
| `LinkMeta` | `project_path` | `LinkedMetaModel` | 由 build input preparation 执行；target tags 不改变 link 结构。 |
| `EffectiveTargetTags` | `(target, effective_tags)` | `TargetTags` | 当前由 `build_target()` 在 branch executor 前执行，作为 context 注入；skeleton node key 记录合并后的 effective tags。 |
| `DataLoadPlan` | `(project.input, effective tags)` | `SourceReadPlan` / `RawSourceData` | Source discovery 基于 raw `MetaModel` 执行，必须包含 tags，因为 override source 受 tags 影响；可与 `LinkMeta` 并行。 |
| `NormalizeAndValidate` | `(project.input, effective_tags)` | normalized `DataSet` + warnings | 当前分支 executor 中合并 data load / normalize / validation execution；error 阻断依赖该数据集的 exporter。 |
| `RunExporter` | `(target, export_name)` | `Vec<Artifact>` | 根据 dependency 注入 MetaOnly 或 MetaAndData context。 |
| `WriteArtifacts` | all exporter outputs | `BuildOutput` | 全局路径冲突、dry-run、落盘。 |

当前 branch executor 已满足的执行约束：

1. `MetaOnly` exporter 只依赖 `LinkMeta` 和 `EffectiveTargetTags`，可以与 data load 并行。
2. `MetaAndData` exporter 依赖对应 `(project.input, effective_tags)` 的 validation 成功结果。
3. 同一 target 下的 JSON / MessagePack / bin / gettext / Lua 等 exporter 共享一次 DataLoadPlan、DataSet、normalize 和 validation。
4. data-backed target 在 raw `MetaModel` 和 effective tags 可用后即可构造 `SourceReadPlan` 并读取 `RawSourceData`，不等待 `LinkedMetaModel`；normalize 必须等待 `LinkMeta + RawSourceData` join。
5. Artifact 写出必须在所有 exporter 产物收集后统一执行，确保跨 exporter 路径冲突不会产生部分写入。
6. 输出顺序按 target 中 export task 声明顺序稳定排序；并行执行不得改变 dry-run 和诊断顺序。

当前状态：Project Config 使用 `[[targets.exports]]`；normalize 阶段全部转换为 `NormalizedExportTask`；data exporter registry 与 code generator registry 均提供 dependency contract；branch executor 按 dependency contract 拆成 MetaAndData / MetaOnly 两个分支并行执行。通用 node-level DAG scheduler 不是当前默认执行模型。

## 10.3 Target normalization

Build 会把 project-level `TargetConfig` 归一化为内部结构：

```rust
pub(crate) struct NormalizedTargetConfig {
    pub(crate) name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) exports: Vec<NormalizedExportTask>,
}
```

规则：

- 单个未命名 export task 归一化为 `default`；多个 export task 省略 `name` 时默认使用 `type`。
- export task name 只能使用 ASCII 字母、数字、`_`、`-`，且首尾必须是字母或数字。
- 同一 target 内 export task name 不能重复。
- export task 禁止设置 `input`；source data root 统一来自 `project.input`。`MetaOnly` task 还禁止设置 `data`。
- 无论 target 有 0 个、1 个还是多个 MetaAndData task，MetaOnly codegen task 都只依赖 Meta 和 target tags。
- 生成代码使用逻辑 table name，由运行时 loader / reader 决定实际加载 `.json`、`.msgpack` 或 `.bin` 等具体格式文件。
- BuildOptions 可以携带 CLI extra tags。`TargetConfig::normalize()` 保留 Project Config 中声明的原始 `target.tags`；Build 主流程通过 `NormalizedTargetConfig::effective_target_tags(extra_tags)` 将 `target.tags` 与 `extra_tags` 合并为 effective target tags：去重、保持稳定顺序，并用于 DataLoadPlan、Validation、Exporter 和 Codegen。
- CLI extra tags 只能追加，不能删除 Project Config 中声明的 target tags。

## 10.4 Build 主流程

```text
build_target(options)
  -> create rayon CPU worker pool
  -> ProjectConfig::from_path
  -> select target
  -> target.normalize()
  -> target.effective_target_tags(options.extra_tags)
  -> project.raw_meta_model()
  -> execute_target_dag(...)
      -> build execution view skeleton for tests / future executor
      -> prepare build inputs:
          LinkMeta || SourceReadPlan + RawSourceData when target requires data
          LinkMeta only when target is code-only
      -> split export tasks by dependency contract
      -> MetaAndData branch: RawSourceData / NormalizeTable[*] + deterministic merge / validation / exporter
      -> MetaOnly branch: format-agnostic codegen input / CodeGenerator::generate
      -> run both branches through the rayon pool when both exist
      -> collect export task outputs by original declaration order
  -> ArtifactWriter::write(all artifacts)
  -> BuildOutput { warnings, artifacts, trace }
```

Build 不再强制 “runtime data 阶段整体先于 codegen 阶段”。data-backed target 会先并行准备 `LinkedMetaModel` 与 `RawSourceData`，再进入 normalize / validation；normalize 在 table 粒度通过 CPU pool 并行，并按 raw `DataSet.tables()` 顺序稳定 merge；validation 先全量收集 `ValidationReport`，再按 stable key 排序并通过 error gate 阻断 data-backed export。validation 模块已提供 full `DataSet` table-local validation 的 CPU pool 入口；当前 single-target runtime path 的 source read 仍按 effective target tags 过滤 raw tables，因此 build 主流程继续对 target-scoped normalized data 执行 target-aware report-backed validation。runtime path 调用 `validate_normalized_data_set_for_target_with_report_and_pool()`，并行构建 target `ExportValidationView`、按 visible table 并行执行 target table validation，再稳定合并 report；真正跨 target 共享 full normalized DataSet 的调度仍留给 multi-target build。MetaAndData branch 在 validation gate 通过前先按 export task 并行执行 artifact planning / path conflict check，gate 通过后再按 export task 并行执行 runtime table exporter 或 aggregate exporter；每个 table exporter 内部仍按 table artifact 并行序列化。code-only target 不启动 source read，因此缺失 `project.input` 或 data files 不会影响 MetaOnly codegen。`MetaOnly` codegen export task 只依赖 linked Meta 与 effective target tags，可与 `MetaAndData` 分支中的 validation / export 并行。Artifact 收集仍按 `[[targets.exports]]` 声明顺序排序，因此并行执行不会改变 dry-run 条目顺序或跨 exporter 路径冲突检查语义。

### 10.4a Build tracing / metrics

当前实现不引入通用 DAG executor，但 `build_target()` 会在阶段式 executor 的稳定边界记录轻量 trace：

```rust
pub struct BuildTrace {
    pub target: String,
    pub jobs: usize,
    pub io_concurrency: usize,
    pub network_concurrency: usize,
    pub serial: bool,
    pub artifact_memory_bytes: usize,
    pub artifact_queue_capacity: usize,
    pub total_duration_ms: u128,
    pub stages: Vec<BuildTraceStage>,
}

pub struct BuildTraceStage {
    pub name: String,
    pub started_at_ms: u128,
    pub duration_ms: u128,
    pub status: BuildTraceStageStatus,
    pub detail: Option<String>,
}
```

已记录的阶段包括：`create_cpu_pool`、`load_project`、`normalize_target`、`plan_dependencies`、`build_execution_view`、`prepare_build_inputs`、`run_exporters`、`collect_artifacts`、`write_artifacts`。CLI 传入 `--trace-tasks <FILE>` 时，`src/cli.rs` 会把 `BuildOutput.trace` 以 pretty JSON 写入指定路径；stdout 仍保持原有成功 / dry-run 输出，避免破坏 integration golden。该 trace 只描述阶段级耗时，不承诺 node-level critical path、resource wait、cancellation propagation 或 per-exporter span；这些能力只有在未来确实需要完整 DAG executor 时再引入。

## 10.5 MetaAndData export task 编排

`runtime.rs` 负责 target 内 MetaAndData export task 编排：

- 使用 build input preparation 已读取的 `RawSourceData`；`runtime.rs` 不再负责 source I/O，只负责 normalize / validation / export。
- normalize 使用 `normalize_data_set_with_pool()` 将 `normalize_table()` 拆成 table 粒度 CPU task，并按 `RawSourceData.as_data_set().tables()` 顺序稳定合并。当前 normalize 规则保持 table-local：ObjectTable singleton、struct expanded value、defaults、typed fragment payload 等规则不访问跨表可变状态。
- build input preparation 为当前 target 可见 raw `TableMeta` 构造 `SourceReadPlan`，再通过 `DataLoader::read_sources_with_pool()` 读取 `RawSourceData`。
- 构造 `SourceReadPlan` 时传入 effective target tags，用于过滤 `override_sources[].export_when`。例如 Project target 只有 `client, shipping`，CLI 传入 `--tag s1` 后，`materials_s1.csv` 才会进入 plan 并覆盖 Oak row。
- 同一个 target 内多个 MetaAndData export task 基于 `(project.input, effective target tags)` 复用同一个 `RawSourceData` 和 normalized `DataSet`，避免重复读取和重复 validation。
- 每个 export task 可使用不同 `type` 和 `output`，例如 JSON + MessagePack + Lua。
- runtime table exporter 调用 `exporter::export_runtime_data()` 生成 runtime artifacts，并保留 `RuntimeDataFile` 列表供未来 manifest 或诊断使用；codegen 不绑定该列表。同一 target 内多个 MetaAndData export task 通过 Rayon `par_iter()` 并行执行，结果按 export task 在 `[[targets.exports]]` 中的顺序稳定恢复。
- validation 使用 `validate_data_set_for_target()` 的 report-backed error gate：基础 required/type/enum/unique、registry validator、FK 与 target-aware checks 会先尽量收集为 `ValidationReport`，再统一判断 error / warning。warning 被收集到 `BuildOutput.warnings`，error 以 `ConfigForgeError::ValidationReport` 直接阻断依赖该 validated DataSet 的 data-backed export。

错误顺序策略：`MetaLink` 与 `SourceRead` 并行执行，但 join 后优先返回 MetaLink 错误；如果 MetaLink 成功而 SourceRead 失败，则返回 source 错误。当前本地 CSV `DataLoader` 无 cancellation token，MetaLink failure 时已启动的 source read 会等待完成并丢弃结果；未来远端 provider 可在 `AsyncSourceReadShell` 基础上接入协作取消。

注意：当前 `build_target` 一次只构建一个 target，且同一 target 内所有 export task 共享同一组 effective target tags，所以 runtime task 内部只需加载一次 `project.input`。如果未来同一次 build 支持多个 target 或同一 target 内 task-specific tags，DataSet reuse 的 cache key 必须扩展为 `(project.input, effective target tags)`，不能只按 `project.input` 复用；否则 source-level `override_sources[].export_when` 可能让普通 shipping 构建与 `--tag s1` 构建错误共享不同 materialized DataSet。

路径冲突分层：

- build 在所有 MetaAndData export tasks 维度提前检查 planned runtime output paths，避免第二个 export task 覆盖第一个；该 planning 在 export task 粒度并行执行，并在合并阶段按声明顺序报告第一个稳定冲突。
- exporter 在单个 export task 内检查 table artifact path 冲突。
- 上述早期检查与 Artifact Writer final check 当前都使用 lexical-normalized path key；它们不会解析 symlink，也不会覆盖 macOS / Windows 常见的大小写不敏感 alias。中期平台文件系统增强应统一下沉到 artifact/path key 层，而不是让 build/runtime/exporter 各自实现一套不同规则。

## 10.6 MetaOnly codegen export 编排

`write_generated_code()` 负责 MetaOnly codegen export task：

- 内置生成器由 `codegen::builtin_generators()` 提供。
- codegen export task 不选择数据 export task，也不消费 `RuntimeDataFile`。
- 每个 codegen export task 构造 `CodeExportInput`，包含 linked meta、target tags、project 信息、target language config、空 runtime data file list、generated_at。
- 传给 CodeModel 的 target tags 必须是 effective target tags，而不是仅 Project Config 中的 target tags；否则 CLI `--tag s1` 影响了 runtime data override，却没有同步影响 table/field/index codegen 过滤，会造成 schema 与数据不一致。
- codegen export task 可以并行生成，但交给 Artifact Writer 前必须恢复 task 声明顺序；全局路径冲突由 artifact 模块统一检查。

## 10.7 依赖边界

Build 是顶层业务编排层，因此可以依赖：

- `project`：工程配置和 Meta 抽取；
- `meta`：linked meta 和 target tags；
- `data`：load plan / loader；
- `validation`：normalize 和 validation report；
- `exporter`：runtime data export type 执行；
- `codegen`：生成器和 generated files；
- `runtime_contract`：`ExportDependency`、`RuntimeDataFile` 和 binary schema revision 等共享底层契约；
- `artifact`：统一制品写入、dry-run 和写入结果。

但反向依赖禁止：上述模块不能为了调用方便引用 `build` 中的 normalized target 类型或 runtime export result。若类型需要跨模块共享，应下沉到更合适的模块，或由 build 做转换。

## 10.8 测试边界

Build 模块测试应覆盖：

- execution options 默认值和非法值。
- export task 默认命名、重复命名、非法命名。
- export task 禁止 `input` 字段；MetaOnly export task 禁止 `data` 字段；包含 MetaAndData export task 时必须设置 `project.input`。
- 多 export task 之间输出路径冲突在写文件前失败。
- CLI dry-run 完整执行但不写输出文件。
- MetaAndData export task warning 不阻断 build，但 error 阻断 build。
- CLI extra tags 与 target tags 合并，并驱动 table/field/export_when、override source selection、runtime export 和 codegen。
- 同一 `project.input` 在不同 effective target tags 下不能复用 DataSet；同一 effective target tags 下多个 MetaAndData export task 可以复用 DataSet。
- code-only target 不加载数据。
- JSON + MessagePack + bin + Lua 等多个 MetaAndData export task 不影响同一份格式无关 codegen 输出。
- CLI integration 覆盖完整 build target 流程。
