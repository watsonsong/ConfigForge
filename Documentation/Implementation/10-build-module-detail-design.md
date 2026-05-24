# 10. Build 模块详细设计

本文档描述 `src/build/` 的工程构建编排职责。Build 是顶层业务 orchestrator，负责把 Project / Meta / Data / Validation / Exporter / Codegen 串成完整 target build；其他业务模块不能反向依赖 Build。

## 10.1 设计目标

Build 模块需要解决：

1. 从 CLI options 加载工程配置。
2. 选择并归一化 target。
3. 创建和管理 CPU worker pool。
4. 按 target tags 组织 runtime data export。
5. 在导出前加载、归一化、校验数据。
6. 支持一个 target 内多个 data task，以及多个 code task 绑定指定 data task。
7. 将 runtime data task 产出的真实 file list 传给 codegen。
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
  target.rs           # TargetConfig normalization、data/code task binding
  runtime.rs          # runtime data task 编排、data load / validation / export
```

职责：

| 子模块 | 责任 |
| --- | --- |
| `options.rs` | CLI 传入 build 的稳定参数对象；执行资源限制默认值与合法性。 |
| `target.rs` | target tags、data task 默认命名、data task 名称校验、code task 到 data task 的绑定规则。 |
| `runtime.rs` | runtime data task 执行：构造 DataLoadPlan、复用同 input 的 DataSet、收集 validation warning、调用 exporter。 |
| `mod.rs` | `build_target()` 主流程、codegen task 并行执行、artifact 收集与 Artifact Writer 调用。 |

## 10.3 Target normalization

Build 会把 project-level `TargetConfig` 归一化为内部结构：

```rust
pub(crate) struct NormalizedTargetConfig {
    pub(crate) name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) data: Vec<NormalizedTargetDataConfig>,
    pub(crate) code: Vec<NormalizedCodegenTaskConfig>,
}
```

规则：

- 单个未命名 data task 归一化为 `default`。
- 多个 data task 必须全部显式命名。
- data task name 只能使用 ASCII 字母、数字、`_`、`-`，且首尾必须是字母或数字。
- 同一 target 内 data task name 不能重复。
- code task 如果显式指定 `data`，必须引用存在的 data task。
- 无 data task 时，code task 可以不绑定 data，用于 code-only target。
- 只有一个 data task 时，未指定 `data` 的 code task 自动绑定该 task。
- 多个 data task 时，code task 必须显式指定 `data`，避免生成代码不知道应该引用哪组 runtime data files。

## 10.4 Build 主流程

```text
build_target(options)
  -> create rayon CPU worker pool
  -> ProjectConfig::from_path
  -> select target
  -> target.normalize()
  -> project.raw_meta_model()
  -> MetaModel::link()
  -> export_runtime_data_tasks(...)
      -> DataLoadPlan::from_tables(target-visible tables)
      -> DataLoader::load_with_pool(data input)
      -> normalize_data_set
      -> validate_normalized_data_set_for_target_with_report
      -> export_runtime_data
  -> write_generated_code(...)
      -> selected runtime data files by code task binding
      -> CodeGenerator::generate
      -> collect generated code artifacts
  -> ArtifactWriter::write(all artifacts)
  -> BuildOutput { warnings }
```

Build 保证 runtime data 先于 codegen 执行，因为生成代码需要引用当前 data task 实际产出的 runtime data file paths。

## 10.5 Runtime data task 编排

`runtime.rs` 负责 target 内 data task 编排：

- 为当前 target 可见 tables 构造 `DataLoadPlan`。
- 同一个 `input` 目录的多个 data task 复用同一个 normalized `DataSet`，避免重复读取和重复 validation。
- 每个 data task 可使用不同 `format` 和 `output`，例如 JSON + MessagePack。
- 每个 data task 调用 `exporter::export_runtime_data()`，返回 `RuntimeDataFile` 列表。
- validation warning 被收集到 `BuildOutput.warnings`，error 直接阻断 build。

路径冲突分层：

- build 在所有 data tasks 维度提前检查 planned runtime output paths，避免第二个 data task 覆盖第一个。
- exporter 在单个 data task 内检查 table artifact path 冲突。

## 10.6 Codegen task 编排

`write_generated_code()` 负责 code task：

- 内置生成器由 `codegen::builtin_generators()` 提供。
- code task 根据 normalized binding 选择对应 data task 的 `RuntimeDataFile`。
- 每个 code task 构造 `CodeExportInput`，包含 linked meta、target tags、project 信息、target language config、runtime data file list、generated_at。
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
- code task 与 data task binding 规则。
- 多 data task / code task / runtime-code 之间输出路径冲突在写文件前失败。
- CLI dry-run 完整执行但不写输出文件。
- runtime data task warning 不阻断 build，但 error 阻断 build。
- code-only target 不加载数据。
- JSON + MessagePack 多 data task 能被 codegen 正确引用。
- CLI integration 覆盖完整 build target 流程。
