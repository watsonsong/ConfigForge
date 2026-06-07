# 10. Build 模块详细设计

本文档描述 `src/build/` 的工程构建编排职责。Build 是顶层业务 orchestrator，负责把 Project / Meta / Data / Validation / Generator / Artifact 串成完整 target build；其他业务模块不能反向依赖 Build。历史上的 `exporter` / `codegen` 不再作为 Build 直接编排的产物分类，而应逐步收敛为 data artifact toolkit / code artifact toolkit，由具体 generator 按需依赖。

## 10.1 设计目标

Build 模块需要解决：

1. 从 CLI options 加载工程配置。
2. 选择并归一化 target。
3. 创建和管理 CPU worker pool。
4. 按 target tags 和 generator requirements 组织 artifact generator 执行。
5. 在导出前加载、归一化、校验数据。
6. 支持一个 target 内多个 export task，并按 generator 声明的 requirements 复用 Meta / Data / Validation 节点。
7. 运行时数据格式选择停留在具体 data-backed generator / runtime loader 层，不进入 MetaOnly code-like generator 绑定。
8. 创建共享 `ArtifactSink` 并传给 generator，由 artifact 模块统一协调路径占用、dry-run、direct/temp-commit 和写出文件。

非目标：

- 不把 Project Config schema 放在 build 内；schema 属于 `project`，export task 的具体 option 解析属于对应 generator 或迁移期 helper。
- 不把 runtime data export type 实现放在 build 内；具体 JSON / MessagePack / `bin` 等格式属于 `generator/<type_id>`，共享 payload / serialization 能力属于 data artifact toolkit。
- 不把语言生成器实现放在 build 内；具体 Rust / C++ / TypeScript 等语言产物属于 `generator/<language>`，共享 CodeModel / writer 能力属于 code artifact toolkit。

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
| `target.rs` | target tags、export task 默认命名、export task 名称校验、requirements 与禁止 task-level input/data 的字段校验。 |
| `runtime.rs` | MetaAndData export task 执行：消费已准备的 `RawSourceData`、复用 normalized DataSet、收集 validation warning、调用 data-backed generator。 |
| `mod.rs` | `build_target()` 主流程、target-local execution summary、MetaAndData / MetaOnly branch pipeline、共享 `ArtifactSink` 创建与 finish。 |

当前 `target.rs` 输出 `NormalizedExportTask`；`mod.rs` 直接使用 generator requirements 中的 `needs_data` 拆分 MetaAndData / MetaOnly 两类分支，并直接执行阶段式 pipeline。`build_execution_view` 是 trace/debug summary，只记录 target、是否需要 data、两类 export 数量和声明顺序，不再维护或测试 node-level DAG key。`BuildTraceRecorder` 在现有阶段式 executor 边界记录 stage name、relative start、duration 和 result state，并通过 `BuildOutput.trace` 暴露给 CLI 的 `--trace-tasks` JSON 输出。`SourceReadPlan` 构造保留在 build input preparation；`MetaModel::link()`、`SourceReadPlan -> RawSourceData` 与 MetaOnly generators 在 `run_exporters` 阶段按依赖 pipeline 执行：source read 与 link 并行，link 完成后立即运行 MetaOnly generators，normalize / validation 等待 link + raw sources join。当前不维护通用 DAG executor 的迁移 skeleton；只有未来出现 multi-target cache、远程执行或更细粒度调度需求时，才重新设计 node-level model。

## 10.2a 统一 Export Task 与依赖分支

`TargetConfig` 使用统一 `exports` 描述 Lua data+code 合一导出、gettext POT、asset manifest、schema manifest、runtime data 和 language code 等混合产物。Build 的内部模型为：

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
    pub(crate) requirements: GeneratorRequirements,
    pub(crate) options: BTreeMap<String, toml::Value>,
}
```

`requirements` 不应由用户手写，而应来自 artifact generator registry。Build normalization 阶段会按 `type_id` / alias 查询已注册 generator 的 input requirements，并写入 `NormalizedExportTask.requirements`。当前实现通过统一 `GeneratorRegistry` facade 查询 build-facing `ArtifactGenerator`；language codegen、data-backed exporter 和 direct artifact generator 都先由 adapter / 直接实现收敛为同一种调度接口。目标态是所有具体 export type 都位于 `src/generator/<type_id>/` 并直接或通过轻量 adapter 注册，`codegen` / `exporter` 仅作为 toolkit：

- [`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) 中的内置 code-like generator / 迁移期 codegen backend：`MetaOnly`。
- JSON / MessagePack / bin runtime data exporter：`MetaAndData`。
- gettext POT：`MetaAndData`。
- Lua table：`MetaAndData`。
- schema_doc：`MetaOnly`。

`MetaOnly` / `MetaAndData` 是调度属性，不是 trait 继承体系。一个 generator 可以组合多种能力 facet：Rust / C++ / TypeScript 是 requirements 只含 Meta 的 code artifact generator；`schema_doc` 是 requirements 只含 Meta 的 doc artifact generator；`lua_table` 是 requirements 包含 Meta + Data、同时复用 code artifact 选项和 aggregate artifact 写出能力的 mixed generator。输入依赖不应决定 generator 的领域身份，避免 Lua 这类 data-backed code artifact 被迫在 “CodeGenerator vs DataExporter” 之间二选一。

不提供 `DataOnly` 模式：当前和可预见的 generator 都需要 Meta 来完成类型解释、target 可见性、校验和 artifact 规划。若未来出现不需要 Meta 的纯文件搬运类能力，应作为独立 build step 重新设计，而不是复用 target export task 的 requirements branch。

当前实现不维护 target-local DAG node key。Build 基于 requirements 中的 `needs_data` 得到一个 target-local execution summary，并按两类输入需求执行分支：

| 分支 / 阶段 | 输入 | 产出 | 说明 |
| --- | --- | --- | --- |
| effective target tags | target config + CLI extra tags | `TargetTags` | 在 `build_target()` 中计算，作为后续 context 注入。 |
| dependency plan | normalized target + effective tags | `TargetBuildPlan` | 记录 MetaOnly / MetaAndData export indices、是否需要 data、共享 data dependency。 |
| build execution view | `TargetBuildPlan` | trace detail string | 仅用于 `--trace-tasks` debug，不作为调度或缓存权威。 |
| MetaOnly branch | linked Meta + target tags | codegen / doc / manifest artifacts / warnings | `LinkMeta` 完成后立即运行不需要 Data 的 generator，不等待 source read / normalize / validation。 |
| MetaAndData branch | linked Meta + raw sources + target tags | runtime / aggregate / mixed artifacts / warnings | 共享一次 source read、normalize 和 validation；validation 成功后执行 requirements 包含 Data 的 generator。 |
| finish artifacts | shared `ArtifactSink` | `BuildOutput` | 由 `ArtifactSink::finish()` 完成 dry-run / direct / temp-commit。 |

当前 branch executor 已满足的执行约束：

1. `MetaOnly` generator 只依赖 `LinkMeta` 和 `EffectiveTargetTags`；mixed target 中 `LinkMeta` 完成后会立即启动 MetaOnly codegen / schema doc 等任务，不等待 source read / normalize / validation。
2. `MetaAndData` generator 依赖对应 `(project.input, effective_tags)` 的 validation 成功结果。
3. 同一 target 下的 JSON / MessagePack / bin / gettext / Lua 等 data-backed generator 共享一次 DataLoadPlan、DataSet、normalize 和 validation。
4. data-backed target 在 raw `MetaModel` 和 effective tags 可用后即可构造 `SourceReadPlan` 并读取 `RawSourceData`，不等待 `LinkedMetaModel`；normalize 必须等待 `LinkMeta + RawSourceData` join。
5. 当前 Build 在 `build_target()` 创建共享 `ArtifactSink` 并传入 generator 分支；分支执行只按 export task 的 `type_id` 调用 `GeneratorRegistry::require_generator()`，再根据 requirements 传入 common input 或 data input。runtime table exporter 与 aggregate exporter 通过迁移期 data-backed adapter 写 sink，language codegen 通过迁移期 codegen adapter 写 sink，`schema_doc` 作为 direct `ArtifactGenerator` 写 sink。目标态这些都应是 `src/generator/<type_id>/` 下的平级 concrete generator，内部继续复用 table serialization、code writer、doc renderer 等 toolkit 能力。
6. dry-run artifact plan 由 `ArtifactSink::finish()` 返回的 entries 按 normalized path 稳定排序；诊断仍按稳定阶段边界合并。

当前状态：Project Config 使用 `[[targets.exports]]`；normalize 阶段全部转换为 `NormalizedExportTask`；`GeneratorRegistry` 统一提供 type / alias 到 `GeneratorRequirements` 的查询；branch pipeline 按 `requirements.needs_data` 拆成 MetaAndData / MetaOnly 两类分支，并在阶段式执行器中提前启动只依赖 Meta 的 generator。通用 node-level DAG scheduler 不是当前默认执行模型，也不保留容易误导为“已 DAG 化”的 skeleton。

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
- 无论 target 有 0 个、1 个还是多个 MetaAndData task，MetaOnly generator task 都只依赖 Meta 和 target tags。
- 生成代码使用逻辑 table name，由运行时 loader / reader 决定实际加载 `.json`、`.msgpack` 或 `.bin` 等具体格式文件。
- BuildOptions 可以携带 CLI extra tags。`TargetConfig::normalize()` 保留 Project Config 中声明的原始 `target.tags`；Build 主流程通过 `NormalizedTargetConfig::effective_target_tags(extra_tags)` 将 `target.tags` 与 `extra_tags` 合并为 effective target tags：去重、保持稳定顺序，并用于 DataLoadPlan、Validation 和 generator 输入。
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
  -> execute_target_pipeline(...)
      -> record build execution view summary for trace/debug
      -> prepare build inputs:
          build optional SourceReadPlan when target requires data
          no SourceReadPlan when target is code-only
      -> split export tasks by requirements.needs_data
      -> run generators:
          (LinkMeta -> MetaOnly generators) || optional SourceReadPlan + RawSourceData
          after join: MetaAndData branch NormalizeTable[*] + deterministic merge / validation / generators
      -> collect export diagnostics
  -> sink.finish()
  -> BuildOutput { warnings, artifacts, trace }
```

Build 不再强制 “runtime data 阶段整体先于 codegen 阶段”。data-backed target 会先构造 `SourceReadPlan`，然后在 `run_exporters`（命名保留为历史实现细节，语义上是 run generators）中并行执行 `MetaModel::link()` 与 raw source read；`LinkMeta` 完成后立即启动 `MetaOnly` generator，因此 mixed target 的代码生成 / schema docs 可以覆盖 source I/O 等待时间。normalize / validation 仍等待 `LinkedMetaModel + RawSourceData` join 后执行；normalize 在 table 粒度通过 CPU pool 并行，并按 raw `DataSet.tables()` 顺序稳定 merge；validation 先全量收集 `ValidationReport`，再按 stable key 排序并通过 error gate 阻断 data-backed export。validation 模块已提供 full `DataSet` table-local validation 的 CPU pool 入口；当前 single-target runtime path 的 source read 仍按 effective target tags 过滤 raw tables，因此 build 主流程继续对 target-scoped normalized data 执行 target-aware report-backed validation。runtime path 调用 `validate_normalized_data_set_for_target_with_assets_report_and_pool()`，并行构建 target `ExportValidationView`、按 visible table 并行执行 target table validation，再稳定合并 report；真正跨 target 共享 full normalized DataSet 的调度仍留给 multi-target build。MetaAndData branch 在 validation gate 通过后按 export task 并行执行 requirements 包含 Data 的 generator；`validation_report` 是特殊 data-backed 附加产物，它消费同一次 validation report，并在 validation error 阻塞 runtime / code-like generator 时仍尽量通过共享 `ArtifactSink` 写出 HTML/Markdown/JSON/CSV report artifact。runtime table exporter 和 aggregate exporter 当前均通过 `GeneratorRegistry` adapter 写入共享 `ArtifactSink`。MetaOnly branch 对 direct artifact generator 构造 `GeneratorCommonInput`，对迁移期 language codegen 构造 `CodeExportInput` 后调用 `generate_to_sink()`，因此 build 层不再持有或排序 `GeneratedFile` 内容。artifact path 冲突由 shared sink 在 `create()` / `write_text()` / `write_binary()` 时动态发现。code-only target 不启动 source read，因此缺失 `project.input` 或 data files 不会影响 MetaOnly code-like generator / docs。dry-run entries 由 `finish()` 按 path 稳定输出。

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

已记录的阶段包括：`create_cpu_pool`、`load_project`、`normalize_target`、`plan_dependencies`、`build_execution_view`、`prepare_build_inputs`、`run_exporters`、`collect_export_diagnostics`、`finish_artifacts`。其中 `build_execution_view` 只记录 summary detail，不暴露或承诺 DAG node 形状；`prepare_build_inputs` 记录 dependency plan / 可选 `SourceReadPlan` 准备；`run_exporters` 记录 `LinkMeta -> MetaOnly generators` 与 source read 的 pipeline 以及后续 MetaAndData generators，并且 generator 在该阶段写入共享 `ArtifactSink`；`collect_export_diagnostics` 合并 warning / metadata side effects；`finish_artifacts` 记录 `ArtifactSink::finish()`。CLI 传入 `--trace-tasks <FILE>` 时，`src/cli.rs` 会把成功路径的 `BuildOutput.trace` 或失败路径的 `BuildErrorWithTrace.trace` 以 pretty JSON 写入指定路径；stdout 仍保持原有成功 / dry-run 输出，避免破坏 integration golden。失败阶段记录为 `failed`，未开始的后续阶段不会出现在 trace 中。该 trace 只描述阶段级耗时，不承诺 node-level critical path、resource wait、cancellation propagation 或 per-generator span；这些能力只有在未来确实需要完整 DAG executor 时再引入。

## 10.5 MetaAndData export task 编排

`runtime.rs` 负责 target 内 MetaAndData export task 编排：

- 使用 build input preparation 已读取的 `RawSourceData`；`runtime.rs` 不再负责 source I/O，只负责 normalize / validation / export。
- normalize 使用 `normalize_data_set_with_pool()` 将 `normalize_table()` 拆成 table 粒度 CPU task，并按 `RawSourceData.as_data_set().tables()` 顺序稳定合并。当前 normalize 规则保持 table-local：ObjectTable singleton、struct expanded value、defaults、typed fragment payload 等规则不访问跨表可变状态。
- build input preparation 为当前 target 可见 raw `TableMeta` 构造 `SourceReadPlan`，再通过 `DataLoader::read_sources_with_pool()` 读取 `RawSourceData`。
- 构造 `SourceReadPlan` 时传入 effective target tags，用于过滤 `override_sources[].export_when`。例如 Project target 只有 `client, shipping`，CLI 传入 `--tag s1` 后，`materials_s1.csv` 才会进入 plan 并覆盖 Oak row。
- 同一个 target 内多个 MetaAndData export task 基于 `(project.input, effective target tags)` 复用同一个 `RawSourceData` 和 normalized `DataSet`，避免重复读取和重复 validation。
- 每个 export task 可使用不同 `type` 和 `output`，例如 JSON + MessagePack + Lua。
- runtime table data generator 当前由 `GeneratorRegistry` data-backed adapter 调用 `exporter::export_runtime_data_to_sink()` 生成 runtime artifacts，直接通过 `sink.create(path)` 打开 shared sink stream，调用 `DataFormatExporter::serialize_table_to_stream()` 后 `stream.finish()`，并保留 `RuntimeDataFile` 列表供未来 manifest 或诊断使用；code-like generator 不绑定该列表。同一 target 内多个 MetaAndData export task 通过 Rayon `par_iter()` 并行执行；JSON / MessagePack 已使用 writer-style serializer，binary serializer 也直接写 stream，不再为最终 artifact 先构建完整 `Vec<u8>`。aggregate data generator 当前也由 data-backed adapter 通过 required `export_to_sink()` 直接写入 shared sink；POT 与 Lua 当前仍可在 exporter 内部局部生成 `String`，但不再通过 `Vec<Artifact>` 返回 build。目标态 JSON / MessagePack / `bin` / POT / Lua / Asset Manifest 均应作为 `src/generator/<type_id>/` 的 concrete generator 实现，复用 data artifact toolkit helper。
- validation 使用 `validate_data_set_for_target()` 的 report-backed error gate：基础 required/type/enum/unique、registry validator、FK 与 target-aware checks 会先尽量收集为 `ValidationReport`，再统一判断 error / warning。warning 被收集到 `BuildOutput.warnings`，error 以 `ConfigForgeError::ValidationReport` 直接阻断依赖该 validated DataSet 的 data-backed export。

错误顺序策略：`MetaLink` 与 `SourceRead` 并行执行，但 join 后优先返回 MetaLink 错误；如果 MetaLink 成功而 SourceRead 失败，则返回 source 错误。当前本地 CSV `DataLoader` 无 cancellation token，MetaLink failure 时已启动的 source read 会等待完成并丢弃结果；未来远端 provider 可在 `AsyncSourceReadShell` 基础上接入协作取消。

注意：当前 `build_target` 一次只构建一个 target，且同一 target 内所有 export task 共享同一组 effective target tags，所以 runtime task 内部只需加载一次 `project.input`。如果未来同一次 build 支持多个 target 或同一 target 内 task-specific tags，DataSet reuse 的 cache key 必须扩展为 `(project.input, effective target tags)`，不能只按 `project.input` 复用；否则 source-level `override_sources[].export_when` 可能让普通 shipping 构建与 `--tag s1` 构建错误共享不同 materialized DataSet。

路径冲突分层：

- 当前 build 在所有 MetaAndData export tasks 维度提前检查 planned runtime output paths，避免第二个 export task 覆盖第一个；该 planning 在 export task 粒度并行执行，并在合并阶段按声明顺序报告第一个稳定冲突。
- data-backed generator / data artifact toolkit helper 在单个 export task 内检查 table artifact path 冲突。
- Artifact Writer final check / sink path reservation 仍是最后防线；当前使用 lexical-normalized path key，不解析 symlink，也不会覆盖 macOS / Windows 常见的大小写不敏感 alias。中期平台文件系统增强应统一下沉到 artifact/path key 层，而不是让 build/runtime/exporter 各自实现一套不同规则。

## 10.6 MetaOnly generator export 编排

`execute_meta_only_generator_nodes()` 负责所有 MetaOnly generator export task，包括 language code-like generator 和 direct `schema_doc` artifact generator。Build 调用统一 artifact generator facade，generator 自身声明 requirements，并按需组合 code / doc / manifest 等 capability facet。

当前行为：`schema_doc` 与语言 codegen 一样读取 linked Meta、project 信息和 effective target tags，然后通过 `ArtifactSink` 输出制品；它不属于 `DataFormatRegistry`，因为不消费 source data / resolved data：

- 内置 language codegen 当前由 `codegen::builtin_generators()` 提供，但 Build 不直接查询 `CodeGenerator`；`GeneratorRegistry` 用 codegen adapter 将其暴露为 build-facing `ArtifactGenerator`。目标态 language generator 应迁入 `src/generator/<language>/` 并直接或通过轻量 adapter 注册，`codegen` 只保留 code artifact toolkit。`schema_doc` 直接实现 doc artifact generator 并由 `GeneratorRegistry` 注册，而不是实现 code-specific `CodeGenerator`。
- code-like / schema_doc export task 不选择数据 export task，也不消费 `RuntimeDataFile`。
- Build 从 `ProjectConfigMeta` 构造共享 `ArtifactProjectInfo`，并为每个 export task 构造统一 `GeneratorCommonInput`；如果 requirements 包含 Data，则在 validation 成功后补充 `GeneratorDataInput`。
- 迁移期 codegen adapter 从 `GeneratorCommonInput` 派生 `CodeExportInput`，包含 linked meta、target tags、project 信息、project root、target name、target generator config、空 runtime data file list、generated_at；这个转换属于 generator / code artifact toolkit 细节，不属于 Build 调度分支逻辑。
- 传给 CodeModel 的 target tags 必须是 effective target tags，而不是仅 Project Config 中的 target tags；否则 CLI `--tag s1` 影响了 runtime data override，却没有同步影响 table/field/index codegen 过滤，会造成 schema 与数据不一致。
- MetaOnly generator export task 可以并行生成，并统一通过 `ArtifactGenerator::generate_to_sink()` 直接写入 shared `ArtifactSink`；全局路径冲突由 artifact 模块在 `create()` / convenience write API 中统一检查，dry-run / finish entries 由 sink 按 normalized path 稳定输出。
- `schema_doc` 的 generator language/type id 为 `schema_doc`，当前支持 `format = "markdown"`、`index_file`、`title`，未知格式或未知选项显式报错；它基于 linked Meta 构建 `SchemaDoc` normalized model，并通过 Markdown renderer 写出 `index.md` 和每表 `<table>.md`。
- 未来 JSON Schema、Mermaid ER、HTML docs 等如果仍是“只读 Meta 并输出 artifact”，优先按统一 artifact generator + requirements + doc facet 扩展，而不是在 build 中增加特殊分支，也不要继续把非代码文档任务塞进 code-specific `CodeGenerator` trait。

当前 generator 抽象：

```rust
pub(crate) struct GeneratorRequirements {
    pub(crate) needs_meta: bool,
    pub(crate) needs_data: bool,
    pub(crate) needs_assets: bool,
    pub(crate) needs_validation_report: bool,
}

pub(crate) trait ArtifactGenerator: Send + Sync {
    fn type_id(&self) -> &'static str;
    fn aliases(&self) -> &'static [&'static str] { &[] }
    fn requirements(&self) -> GeneratorRequirements;
    fn generate_to_sink(&self, input: &GeneratorCommonInput<'_>, sink: &ArtifactSink) -> Result<()>;
    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>>;
    fn generate_with_data_to_sink(&self, input: &GeneratorDataInput<'_, '_>, sink: &ArtifactSink) -> Result<()>;
}
```

Build-facing registry 只暴露 `require_generator(type_id) -> &dyn ArtifactGenerator` 和 `requirements(type_id) -> GeneratorRequirements`，不再把调度入口拆成 `code_generators`、`direct_generators`、`DataFormatExporter` 等多种调用路径；这些分类在迁移期仅作为 toolkit 内部复用实现，目标态 concrete generator 归属 `src/generator/<type_id>/`。领域复用应通过 facet / helper 表达：

- `CodeArtifactOptions` / `CodeWriter` / naming / formatting / keyword conflict：供 Rust/C++/TypeScript/Java/C# 和 Lua 这类 code-like generator 复用。
- `RuntimeTableDataFacet`：供 JSON / MessagePack / bin 这类 per-table runtime data generator 复用。
- `AggregateArtifactFacet`：供 POT / Asset Manifest / Lua 等跨表产物复用。
- `DocArtifactFacet`：供 `schema_doc`、JSON Schema、Mermaid ER、HTML docs 等文档/结构说明产物复用。

Lua 是该模型的关键校验用例：它 requirements 包含 Meta + Data，但输出形态明显是 code-like artifact，因此应复用 code artifact options，而不应因为需要 Data 就被排除在 code 能力之外。

下一阶段的目标是把当前 adapter facade 进一步收敛为单一 task generator 模型：从任务调度角度看，`DataFormatExporter`、`AggregateDataExporter`、`CodeGenerator` 和 direct `ArtifactGenerator` 不应是四种调度对象，而只是迁移期 toolkit 内部的实现复用方式。Build 只面向一种对象：根据 export task 的 `type_id` 匹配到 generator，读取其 requirements，准备满足 requirements 的上下文，然后把 task instance 输入交给 generator 执行。generator 内部可以再把输入转换成 `CodeExportInput`、`TargetDataConfig`、table payload、doc model 或 report renderer 所需的形态；这些转换不应回流到 Build 分支逻辑。

建议的长期 build-facing trait 形态：

```rust
pub(crate) struct GeneratorDescriptor {
    pub(crate) type_id: &'static str,
    pub(crate) aliases: &'static [&'static str],
    pub(crate) requirements: GeneratorRequirements,
}

pub(crate) struct GeneratorTaskInput<'a, 'meta> {
    pub(crate) task: &'a NormalizedExportTask,
    pub(crate) common: GeneratorCommonInput<'meta>,
    pub(crate) prepared: PreparedGeneratorInputs<'a>,
}

pub(crate) enum PreparedGeneratorInputs<'a> {
    MetaOnly,
    MetaAndData {
        data_set: &'a DataSet,
        validation_report: &'a ValidationReport,
        pool: &'a ThreadPool,
    },
}

pub(crate) trait TaskGenerator: Send + Sync {
    fn descriptor(&self) -> GeneratorDescriptor;
    fn plan_artifacts(&self, input: &GeneratorTaskInput<'_, '_>) -> Result<Vec<PathBuf>>;
    fn generate(&self, input: &GeneratorTaskInput<'_, '_>, sink: &ArtifactSink) -> Result<()>;
}
```

这里的 “task name” 属于 export task instance（`[[targets.exports]].name`），不是 generator 类型本身的全局名字；同一个 generator type 可以被同一 target 以不同 task name / output / options 声明多次。scheduler 需要知道的是 generator descriptor 中的 requirements，而不是 generator 内部属于 code、table data、aggregate data 还是 doc。`MetaOnly` / `MetaAndData` 只是当前 branch pipeline 对 `requirements.needs_data` 的执行分组名称；未来若出现 assets-only、validation-report-only 或更细粒度缓存，也应继续由 requirements 扩展，而不是新增调度侧 generator 子类。

## 10.6a 稳定性与回归基线

并行执行、阶段式 pipeline 和不同 artifact write mode 都不能改变用户可见产物语义。Build 稳定性基线按三层覆盖：

1. **执行资源稳定性**：同一 Project / target / effective tags 下，`--serial` 或 `--jobs 1` 与 `--jobs N` 必须生成相同 artifact manifest 和相同文件内容。若代码生成默认文件头包含 `{{datetime}}` 或当前时间，则测试应使用固定 `file_header` 消除时间因素，避免把有意的时间戳差异误判为并发非确定性。
2. **写入模式稳定性**：`direct` 与 `temp-commit` 应写出相同相对路径、字节数和文件内容；`dry-run` 应返回同一 artifact plan（路径 + bytes），但所有 entry 的 `written` 必须为 `false` 且不得创建输出文件。
3. **Artifact result 顺序稳定性**：不论 exporter/codegen 完成顺序如何，build 不再在 Artifact Writer 前汇总 artifact 内容；所有任务直接写入 shared `ArtifactSink`，由 artifact 模块完成路径规范化、冲突检查，并在 `finish()` 中按 normalized path 稳定输出 `ArtifactWriteResult.entries`。并行完成顺序不能影响 dry-run stdout、manifest 或第一个稳定冲突。

回归测试建议组合：

- CLI integration 使用混合 target（至少包含一个 MetaAndData exporter 和一个 MetaOnly codegen exporter）比较 `--serial` 与 `--jobs N` 的实际输出树。
- 使用同一 mixed target 分别运行 `--write-mode direct`、`--write-mode temp-commit` 和 `--dry-run`：direct/temp-commit 比较文件内容；dry-run 比较 plan 中的相对路径与 byte count，并断言未写出文件。
- 单元测试继续覆盖内部 branch 时序，例如 `LinkMeta -> codegen` 可在 `SourceRead` 完成前开始；CLI 测试覆盖端到端产物稳定性。

## 10.7 依赖边界

Build 是顶层业务编排层，因此目标态可以依赖：

- `project`：工程配置和 Meta 抽取；
- `meta`：linked meta 和 target tags；
- `data`：load plan / loader；
- `validation`：normalize 和 validation report；
- `generator`：build-facing artifact generator registry、requirements 查询和 generator 执行；
- `runtime_contract`：`RuntimeDataFile` 和 binary schema revision 等共享底层契约；
- `artifact`：统一制品写入、dry-run 和写入结果。

但反向依赖禁止：上述模块不能为了调用方便引用 `build` 中的 normalized target 类型或 runtime export result。若类型需要跨模块共享，应下沉到更合适的模块，或由 build 做转换。迁移期 Build 中对 `exporter` / `codegen` 的直接引用应逐步收敛到 `GeneratorRegistry` adapter 或 concrete generator 内部；新增 Build 逻辑不得直接按 data/code toolkit 分类分发。

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
