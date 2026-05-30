# 10. Build 模块详细设计

本文档描述 `src/build/` 的工程构建编排职责。Build 是顶层业务 orchestrator，负责把 Project / Meta / Data / Validation / Exporter / Codegen 串成完整 target build；其他业务模块不能反向依赖 Build。

## 10.1 设计目标

Build 模块需要解决：

1. 从 CLI options 加载工程配置。
2. 选择并归一化 target。
3. 创建和管理 CPU worker pool。
4. 按 target tags 组织 runtime data export。
5. 在导出前加载、归一化、校验数据。
6. 支持一个 target 内多个 data task，以及多个格式无关 code task。
7. 运行时数据格式选择停留在 data task / runtime loader 层，不进入 codegen 绑定。
8. 收集 runtime data / generated code artifacts，交给 `artifact` 模块统一检查路径冲突、dry-run 和写出文件。

非目标：

- 不把 Project Config schema 放在 build 内；schema 属于 `project` / `exporter` / `codegen`。
- 不把 runtime data format 实现放在 build 内；格式属于 `exporter`。
- 不把语言生成器实现放在 build 内；生成器属于 `codegen`。

## 10.2 模块结构

```text
src/build/
  mod.rs              # build_target 主入口、codegen task 执行、artifact 收集与写入调用
  options.rs          # BuildOptions / ExecutionOptions / BuildOutput
  target.rs           # TargetConfig normalization、data task naming、code task format-agnostic validation
  runtime.rs          # runtime data task 编排、data load / validation / export
```

职责：

| 子模块 | 责任 |
| --- | --- |
| `options.rs` | CLI 传入 build 的稳定参数对象；执行资源限制默认值与合法性。 |
| `target.rs` | target tags、data task 默认命名、data task 名称校验、code task 禁止绑定 data 的格式无关校验。 |
| `runtime.rs` | runtime data task 执行：构造 DataLoadPlan、复用同 input 的 DataSet、收集 validation warning、调用 exporter。 |
| `mod.rs` | `build_target()` 主流程、codegen task 并行执行、artifact 收集与 Artifact Writer 调用。 |

## 10.3 Target normalization

Build 会把 project-level `TargetConfig` 归一化为内部结构：

```rust
pub(crate) struct NormalizedTargetConfig {
    pub(crate) name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) extra_tags: Vec<String>,
    pub(crate) data: Vec<NormalizedTargetDataConfig>,
    pub(crate) code: Vec<NormalizedCodegenTaskConfig>,
}
```

规则：

- 单个未命名 data task 归一化为 `default`。
- 多个 data task 必须全部显式命名。
- data task name 只能使用 ASCII 字母、数字、`_`、`-`，且首尾必须是字母或数字。
- 同一 target 内 data task name 不能重复。
- code task 禁止显式指定 `data`；若设置则配置校验失败。
- 无论 target 有 0 个、1 个还是多个 data task，code task 都只依赖 Meta 和 target tags。
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
  -> MetaModel::link()
  -> export_runtime_data_tasks(...)
      -> DataLoadPlan::from_tables(target-visible tables, effective target tags)
      -> DataLoader::load_with_pool(data input)
      -> normalize_data_set
      -> validate_normalized_data_set_for_target_with_report
      -> export_runtime_data
  -> write_generated_code(...)
      -> format-agnostic codegen input
      -> CodeGenerator::generate
      -> collect generated code artifacts
  -> ArtifactWriter::write(all artifacts)
  -> BuildOutput { warnings }
```

Build 当前仍先执行 runtime data 再执行 codegen，以保持统一 artifact 收集顺序；但生成代码不再引用某个 data task 的真实 runtime data file path。

## 10.5 Runtime data task 编排

`runtime.rs` 负责 target 内 data task 编排：

- 为当前 target 可见 tables 构造 `DataLoadPlan`。
- 构造 `DataLoadPlan` 时传入 effective target tags，用于过滤 `override_sources[].export_when`。例如 Project target 只有 `client, shipping`，CLI 传入 `--tag s1` 后，`materials_s1.csv` 才会进入 plan 并覆盖 Oak row。
- 同一个 `input` 目录的多个 data task 复用同一个 normalized `DataSet`，避免重复读取和重复 validation。
- 每个 data task 可使用不同 `format` 和 `output`，例如 JSON + MessagePack。
- 每个 data task 调用 `exporter::export_runtime_data()` 生成 runtime artifacts，并保留 `RuntimeDataFile` 列表供未来 manifest 或诊断使用；codegen 不绑定该列表。
- validation warning 被收集到 `BuildOutput.warnings`，error 直接阻断 build。

注意：当前 `build_target` 一次只构建一个 target，且同一 target 内所有 data/code task 共享同一组 effective target tags，所以 runtime task 内部可以只按 `input` 复用 normalized `DataSet`。如果未来同一次 build 支持多个 target 或同一 target 内 task-specific tags，DataSet reuse 的 cache key 必须扩展为 `(input, effective target tags)`，不能只按 `input` 复用；否则 source-level `override_sources[].export_when` 可能让普通 shipping 构建与 `--tag s1` 构建错误共享不同 materialized DataSet。

路径冲突分层：

- build 在所有 data tasks 维度提前检查 planned runtime output paths，避免第二个 data task 覆盖第一个。
- exporter 在单个 data task 内检查 table artifact path 冲突。

## 10.6 Codegen task 编排

`write_generated_code()` 负责 code task：

- 内置生成器由 `codegen::builtin_generators()` 提供。
- code task 不选择 data task，也不消费 `RuntimeDataFile`。
- 每个 code task 构造 `CodeExportInput`，包含 linked meta、target tags、project 信息、target language config、空 runtime data file list、generated_at。
- 传给 CodeModel 的 target tags 必须是 effective target tags，而不是仅 Project Config 中的 target tags；否则 CLI `--tag s1` 影响了 runtime data override，却没有同步影响 table/field/index codegen 过滤，会造成 schema 与数据不一致。
- code task 可以并行生成，但交给 Artifact Writer 前必须恢复 task 声明顺序；全局路径冲突由 artifact 模块统一检查。

## 10.7 依赖边界

Build 是顶层业务编排层，因此可以依赖：

- `project`：工程配置和 Meta 抽取；
- `meta`：linked meta 和 target tags；
- `data`：load plan / loader；
- `validation`：normalize 和 validation report；
- `exporter`：runtime data format export；
- `codegen`：生成器和 generated files；
- `artifact`：统一制品写入、dry-run 和写入结果。

但反向依赖禁止：上述模块不能为了调用方便引用 `build` 中的 normalized target 类型或 runtime export result。若类型需要跨模块共享，应下沉到更合适的模块，或由 build 做转换。

## 10.8 测试边界

Build 模块测试应覆盖：

- execution options 默认值和非法值。
- data task 默认命名、重复命名、非法命名。
- code task 禁止 `data` 字段的格式无关规则。
- 多 data task / code task / runtime-code 之间输出路径冲突在写文件前失败。
- CLI dry-run 完整执行但不写输出文件。
- runtime data task warning 不阻断 build，但 error 阻断 build。
- CLI extra tags 与 target tags 合并，并驱动 table/field/export_when、override source selection、runtime export 和 codegen。
- 同一 input 在不同 effective target tags 下不能复用 DataSet；同一 effective target tags 下多个格式 data task 可以复用 DataSet。
- code-only target 不加载数据。
- JSON + MessagePack + bin 多 data task 不影响同一份格式无关 codegen 输出。
- CLI integration 覆盖完整 build target 流程。
