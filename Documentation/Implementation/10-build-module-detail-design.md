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
8. 收集所有 exporter artifacts，交给 `artifact` 模块的 `ArtifactWriter` / `ArtifactSink` 兼容入口统一协调路径占用、dry-run、direct/temp-commit 和写出文件。后续再把 sink 或受限 wrapper 直接传给 exporter / codegen。

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
| `mod.rs` | `build_target()` 主流程、target-local execution view 构造、MetaAndData / MetaOnly branch executor、artifact 稳定收集与 Artifact Writer 调用。 |

当前 `target.rs` 输出 `NormalizedExportTask`；`mod.rs` 以 exporter dependency contract 构造 target-local execution view，并直接执行 MetaAndData / MetaOnly 两类分支。`BuildDagNodeKey` 当前是测试和未来通用 executor 的 skeleton，不参与缓存或诊断。`BuildTraceRecorder` 在现有阶段式 executor 边界记录 stage name、relative start、duration 和 result state，并通过 `BuildOutput.trace` 暴露给 CLI 的 `--trace-tasks` JSON 输出。`SourceReadPlan` 构造保留在 build input preparation；`MetaModel::link()`、`SourceReadPlan -> RawSourceData` 与 MetaOnly codegen 在 `run_exporters` 阶段按依赖 pipeline 执行：source read 与 link 并行，link 完成后立即运行 codegen，normalize / validation 等待 link + raw sources join。后续若引入完整 DAG executor，可把这些节点和分支执行统一下沉到 node-level scheduler。

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
| `RunExporter` | `(target, export_name)` | warnings / sink entries | 根据 dependency 注入 MetaOnly 或 MetaAndData context；Build 传入共享 `ArtifactSink`，runtime table exporter、aggregate exporter 与 codegen 分支直接写 sink。 |
| `FinishArtifacts` | shared `ArtifactSink` | `BuildOutput` | 当前通过 `ArtifactSink::finish()` 完成 dry-run / direct / temp-commit；不再收集全局 `Vec<Artifact>` 后调用 `ArtifactWriter::write()`。 |

当前 branch executor 已满足的执行约束：

1. `MetaOnly` exporter 只依赖 `LinkMeta` 和 `EffectiveTargetTags`；mixed target 中 `LinkMeta` 完成后会立即启动 MetaOnly codegen，不等待 source read / normalize / validation。
2. `MetaAndData` exporter 依赖对应 `(project.input, effective_tags)` 的 validation 成功结果。
3. 同一 target 下的 JSON / MessagePack / bin / gettext / Lua 等 exporter 共享一次 DataLoadPlan、DataSet、normalize 和 validation。
4. data-backed target 在 raw `MetaModel` 和 effective tags 可用后即可构造 `SourceReadPlan` 并读取 `RawSourceData`，不等待 `LinkedMetaModel`；normalize 必须等待 `LinkMeta + RawSourceData` join。
5. 当前 Build 在 `build_target()` 创建共享 `ArtifactSink` 并传入 exporter/codegen 分支；runtime table exporter 通过 stream API 写 sink，aggregate exporter 通过 required `export_to_sink()` 写 sink，codegen 短期仍生成 `String` 后立即 `sink.write_text()`。
6. dry-run artifact plan 由 `ArtifactSink::finish()` 返回的 entries 按 normalized path 稳定排序；诊断仍按稳定阶段边界合并。

当前状态：Project Config 使用 `[[targets.exports]]`；normalize 阶段全部转换为 `NormalizedExportTask`；data exporter registry 与 code generator registry 均提供 dependency contract；branch executor 按 dependency contract 拆成 MetaAndData / MetaOnly 两类分支，并在阶段式执行器中提前启动只依赖 Meta 的 codegen。通用 node-level DAG scheduler 不是当前默认执行模型。

阶段 A 的实施步骤：

1. 将 `prepare_build_inputs` 从“link + source read join”收窄为 dependency plan / `SourceReadPlan` 准备，保持 code-only target 不要求 `project.input`。
2. 在 `run_exporters` 中使用 scoped Rayon join：左支执行 `LinkMeta -> MetaOnly codegen`，右支执行可选 `SourceRead`。
3. 在左支与右支 join 后，使用 `LinkedMetaModel + RawSourceData` 执行 normalize / validation / MetaAndData exporter；artifact 收集仍按 export task 声明顺序排序。
4. 保持错误优先级稳定：`LinkMeta` 错误优先于 source read；若 link 成功但 codegen 和 source read 同时失败，则左支 codegen 错误优先，因为它是当前 target 中已满足依赖的独立 requested export task。
5. 补充测试覆盖：code-only 不启动 source read；mixed target 中 codegen 可在 source read 完成前开始；stage trace 仍保留 `prepare_build_inputs` / `run_exporters` 边界。

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
  -> ArtifactWriter::new(mode).open_sink()
  -> execute_target_dag(...)
      -> build execution view skeleton for tests / future executor
      -> prepare build inputs:
          build optional SourceReadPlan when target requires data
          no SourceReadPlan when target is code-only
      -> split export tasks by dependency contract
      -> run exporters:
          (LinkMeta -> MetaOnly codegen) || optional SourceReadPlan + RawSourceData
          after join: MetaAndData branch NormalizeTable[*] + deterministic merge / validation / exporter
      -> collect export diagnostics
  -> sink.finish()
  -> BuildOutput { warnings, artifacts, trace }
```

Build 不再强制 “runtime data 阶段整体先于 codegen 阶段”。data-backed target 会先构造 `SourceReadPlan`，然后在 `run_exporters` 中并行执行 `MetaModel::link()` 与 raw source read；`LinkMeta` 完成后立即启动 `MetaOnly` codegen，因此 mixed target 的代码生成可以覆盖 source I/O 等待时间。normalize / validation 仍等待 `LinkedMetaModel + RawSourceData` join 后执行；normalize 在 table 粒度通过 CPU pool 并行，并按 raw `DataSet.tables()` 顺序稳定 merge；validation 先全量收集 `ValidationReport`，再按 stable key 排序并通过 error gate 阻断 data-backed export。validation 模块已提供 full `DataSet` table-local validation 的 CPU pool 入口；当前 single-target runtime path 的 source read 仍按 effective target tags 过滤 raw tables，因此 build 主流程继续对 target-scoped normalized data 执行 target-aware report-backed validation。runtime path 调用 `validate_normalized_data_set_for_target_with_report_and_pool()`，并行构建 target `ExportValidationView`、按 visible table 并行执行 target table validation，再稳定合并 report；真正跨 target 共享 full normalized DataSet 的调度仍留给 multi-target build。MetaAndData branch 在 validation gate 通过后按 export task 并行执行 runtime table exporter 或 aggregate exporter；runtime table exporter 直接写入共享 `ArtifactSink` 并只返回 `RuntimeDataFile` metadata，aggregate exporter 也通过 required `export_to_sink()` 直接写入同一个 sink。code-only target 不启动 source read，因此缺失 `project.input` 或 data files 不会影响 MetaOnly codegen。Artifact path 冲突由 shared sink 在 `create()` / `write_text()` / `write_binary()` 时动态发现，dry-run entries 由 `finish()` 按 path 稳定输出。

### 10.4a Build tracing / metrics

当前实现不引入通用 DAG executor，但 `build_target_with_trace()` 会在阶段式 executor 的稳定边界记录轻量 trace；`build_target()` 仍作为兼容 wrapper 返回原有错误类型：

```rust
pub struct BuildTrace {
    pub target: String,
    pub jobs: usize,
    pub io_concurrency: usize,
    pub network_concurrency: usize,
    pub serial: bool,
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

已记录的阶段包括：`create_cpu_pool`、`load_project`、`normalize_target`、`plan_dependencies`、`build_execution_view`、`prepare_build_inputs`、`run_exporters`、`collect_export_diagnostics`、`finish_artifacts`。其中 `prepare_build_inputs` 当前记录 dependency plan / 可选 `SourceReadPlan` 准备，`run_exporters` 记录 `LinkMeta -> MetaOnly codegen` 与 source read 的 pipeline 以及后续 MetaAndData exporter，并且 exporter/codegen 在该阶段写入共享 `ArtifactSink`；`collect_export_diagnostics` 合并 warning / metadata side effects；`finish_artifacts` 记录 `ArtifactSink::finish()`。CLI 传入 `--trace-tasks <FILE>` 时，`src/cli.rs` 会把成功路径的 `BuildOutput.trace` 或失败路径的 `BuildErrorWithTrace.trace` 以 pretty JSON 写入指定路径；stdout 仍保持原有成功 / dry-run 输出，避免破坏 integration golden。失败阶段记录为 `failed`，未开始的后续阶段不会出现在 trace 中。该 trace 只描述阶段级耗时，不承诺 node-level critical path、resource wait、cancellation propagation 或 per-exporter span；这些能力只有在未来确实需要完整 DAG executor 时再引入。

## 10.5 MetaAndData export task 编排

`runtime.rs` 负责 target 内 MetaAndData export task 编排：

- 使用 build input preparation 已读取的 `RawSourceData`；`runtime.rs` 不再负责 source I/O，只负责 normalize / validation / export。
- normalize 使用 `normalize_data_set_with_pool()` 将 `normalize_table()` 拆成 table 粒度 CPU task，并按 `RawSourceData.as_data_set().tables()` 顺序稳定合并。当前 normalize 规则保持 table-local：ObjectTable singleton、struct expanded value、defaults、typed fragment payload 等规则不访问跨表可变状态。
- build input preparation 为当前 target 可见 raw `TableMeta` 构造 `SourceReadPlan`，再通过 `DataLoader::read_sources_with_pool()` 读取 `RawSourceData`。
- 构造 `SourceReadPlan` 时传入 effective target tags，用于过滤 `override_sources[].export_when`。例如 Project target 只有 `client, shipping`，CLI 传入 `--tag s1` 后，`materials_s1.csv` 才会进入 plan 并覆盖 Oak row。
- 同一个 target 内多个 MetaAndData export task 基于 `(project.input, effective target tags)` 复用同一个 `RawSourceData` 和 normalized `DataSet`，避免重复读取和重复 validation。
- 每个 export task 可使用不同 `type` 和 `output`，例如 JSON + MessagePack + Lua。
- runtime table exporter 调用 `exporter::export_runtime_data_to_sink()` 生成 runtime artifacts，直接通过 `sink.create(path)` 打开 shared sink stream，调用 `DataFormatExporter::serialize_table_to_stream()` 后 `stream.finish()`，并保留 `RuntimeDataFile` 列表供未来 manifest 或诊断使用；codegen 不绑定该列表。同一 target 内多个 MetaAndData export task 通过 Rayon `par_iter()` 并行执行；JSON / MessagePack 已使用 writer-style serializer，binary serializer 也直接写 stream，不再为最终 artifact 先构建完整 `Vec<u8>`。aggregate exporter 通过 required `export_to_sink()` 直接写入 shared sink；POT 与 Lua 当前仍可在 exporter 内部局部生成 `String`，但不再通过 `Vec<Artifact>` 返回 build。
- validation 使用 `validate_data_set_for_target()` 的 report-backed error gate：基础 required/type/enum/unique、registry validator、FK 与 target-aware checks 会先尽量收集为 `ValidationReport`，再统一判断 error / warning。warning 被收集到 `BuildOutput.warnings`，error 以 `ConfigForgeError::ValidationReport` 直接阻断依赖该 validated DataSet 的 data-backed export。

错误顺序策略：`MetaLink` 与 `SourceRead` 并行执行，但 join 后优先返回 MetaLink 错误；如果 MetaLink 成功而 SourceRead 失败，则返回 source 错误。当前本地 CSV `DataLoader` 无 cancellation token，MetaLink failure 时已启动的 source read 会等待完成并丢弃结果；未来远端 provider 可在 `AsyncSourceReadShell` 基础上接入协作取消。

注意：当前 `build_target` 一次只构建一个 target，且同一 target 内所有 export task 共享同一组 effective target tags，所以 runtime task 内部只需加载一次 `project.input`。如果未来同一次 build 支持多个 target 或同一 target 内 task-specific tags，DataSet reuse 的 cache key 必须扩展为 `(project.input, effective target tags)`，不能只按 `project.input` 复用；否则 source-level `override_sources[].export_when` 可能让普通 shipping 构建与 `--tag s1` 构建错误共享不同 materialized DataSet。

路径冲突分层：

- 当前 build 在所有 MetaAndData export tasks 维度提前检查 planned runtime output paths，避免第二个 export task 覆盖第一个；该 planning 在 export task 粒度并行执行，并在合并阶段按声明顺序报告第一个稳定冲突。
- exporter 在单个 export task 内检查 table artifact path 冲突。
- Artifact Writer final check / sink path reservation 仍是最后防线；当前使用 lexical-normalized path key，不解析 symlink，也不会覆盖 macOS / Windows 常见的大小写不敏感 alias。中期平台文件系统增强应统一下沉到 artifact/path key 层，而不是让 build/runtime/exporter 各自实现一套不同规则。

## 10.6 MetaOnly codegen export 编排

`write_generated_code()` 负责 MetaOnly codegen export task：

- 内置生成器由 `codegen::builtin_generators()` 提供。
- codegen export task 不选择数据 export task，也不消费 `RuntimeDataFile`。
- 每个 codegen export task 构造 `CodeExportInput`，包含 linked meta、target tags、project 信息、target language config、空 runtime data file list、generated_at。
- 传给 CodeModel 的 target tags 必须是 effective target tags，而不是仅 Project Config 中的 target tags；否则 CLI `--tag s1` 影响了 runtime data override，却没有同步影响 table/field/index codegen 过滤，会造成 schema 与数据不一致。
- codegen export task 可以并行生成，但交给 Artifact Writer 前必须恢复 task 声明顺序；全局路径冲突由 artifact 模块统一检查。

## 10.6a 稳定性与回归基线

并行执行、阶段式 pipeline 和不同 artifact write mode 都不能改变用户可见产物语义。Build 稳定性基线按三层覆盖：

1. **执行资源稳定性**：同一 Project / target / effective tags 下，`--serial` 或 `--jobs 1` 与 `--jobs N` 必须生成相同 artifact manifest 和相同文件内容。若代码生成默认文件头包含 `{{datetime}}` 或当前时间，则测试应使用固定 `file_header` 消除时间因素，避免把有意的时间戳差异误判为并发非确定性。
2. **写入模式稳定性**：`direct` 与 `temp-commit` 应写出相同相对路径、字节数和文件内容；`dry-run` 应返回同一 artifact plan（路径 + bytes），但所有 entry 的 `written` 必须为 `false` 且不得创建输出文件。
3. **声明顺序稳定性**：不论 exporter/codegen 完成顺序如何，Artifact Writer 前的输出仍按 `[[targets.exports]]` 声明顺序汇总，再由 artifact 模块做统一路径规范化和冲突检查；并行完成顺序不能影响 dry-run stdout、manifest 或第一个稳定冲突。

回归测试建议组合：

- CLI integration 使用混合 target（至少包含一个 MetaAndData exporter 和一个 MetaOnly codegen exporter）比较 `--serial` 与 `--jobs N` 的实际输出树。
- 使用同一 mixed target 分别运行 `--write-mode direct`、`--write-mode temp-commit` 和 `--dry-run`：direct/temp-commit 比较文件内容；dry-run 比较 plan 中的相对路径与 byte count，并断言未写出文件。
- 单元测试继续覆盖内部 branch 时序，例如 `LinkMeta -> codegen` 可在 `SourceRead` 完成前开始；CLI 测试覆盖端到端产物稳定性。

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
- `--jobs 1` / `--serial` 与 `--jobs N` 的 artifact manifest 和文件内容一致；并行调度不能改变产物。
- `dry-run` / `direct` / `temp-commit` 的 artifact plan 一致；direct 与 temp-commit 写出内容一致，dry-run 不写文件。
- CLI integration 覆盖完整 build target 流程。
