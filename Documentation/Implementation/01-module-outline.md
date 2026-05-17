# 01. 程序实现模块划分大纲

本文档是 ConfigForge 的程序实现概要设计，用于把 `Documentation/Design/` 中的产品设计落到 Rust 工程模块边界。产品语义仍以 `Documentation/Design/` 为准；本目录只约束实现结构、依赖方向、阶段目标和测试组织。

## 1.1 近期开发目标

当前阶段以 **Meta 模块可用闭环** 为中心，目标是让后续 Source / Validation / Exporter 都依赖同一份已校验、已链接的 Meta：

1. 能从单个或多个 TOML Meta 文件加载配置。
2. 能处理 `imports`，递归加载依赖文件并做路径去重。
3. 能把 serde 反序列化得到的纯配置 `MetaModel` 保留下来，避免在 owned 数据内保存自引用。
4. 能在所有 Meta 加载完成后执行 Link，把字段类型、索引字段等字符串引用解析为借用引用。
5. 能执行基础结构校验，例如重复命名、未知字段类型、索引字段引用、主索引一致性。
6. 能通过 Meta 模块单元测试覆盖加载、Link、校验、imports 等基础行为。

## 1.2 顶层模块划分

仓库已采用 `src/meta/` 目录模块承载 Meta 实现。其他扁平模块可在职责增长后再按同样方式拆分。

| 模块 | 建议路径 | 责任 | 主要输入 | 主要输出 |
| --- | --- | --- | --- | --- |
| CLI | `src/cli.rs` | 命令行参数、用户入口、错误展示。 | argv、环境变量。 | `ExportCommand`、用户可读输出。 |
| Project Config | `src/config/` | 加载 `ConfigForge.toml`、解析工程级配置。 | Project Config TOML。 | `ProjectConfig`。 |
| Meta | `src/meta/` | 加载、合并、链接、校验 Meta。 | TOML/XML Meta 文件。 | `MetaModel`、`LinkedMetaModel<'meta>`、`MetaDiagnostic`。 |
| Source | `src/source.rs` / `src/source/` | 将 CSV/XLSX/远程表格归一化为二维 `SourceSheet`。 | source declaration。 | `SourceSheet` / `DataTable`。 |
| Data Model | `src/data/` | 将 source cell 解析为 typed value 和逻辑表数据。 | `LinkedMetaModel<'meta>`、`SourceSheet`。 | `DataSet`。 |
| Validation | `src/validation.rs` / `src/validation/` | 业务规则、索引、外键、注册式 validator、过滤视图校验。 | `LinkedMetaModel<'meta>`、`DataSet`、`ExportContext`、`ValidatorRegistry`。 | 结构化诊断。 |
| Exporter | `src/exporter.rs` / `src/exporter/` | 生成 runtime data、代码、special artifacts。 | `LinkedMetaModel<'meta>`、`TargetTags`、target config。 | `RuntimeDataFile` / generated files。 |
| Artifact Writer | `src/artifact/` | direct/temp_commit 写入策略、manifest 输出。 | `ArtifactPlan`。 | 文件系统产物。 |
| Diagnostics | `src/diagnostics/` | 统一诊断模型、稳定排序、报告格式。 | 各模块诊断。 | `DiagnosticReport`。 |
| Runtime/Executor | `src/executor/` | 异步 I/O、线程池、任务编排。 | 阶段任务。 | 阶段执行结果。 |
| Plugin | `src/plugin/` | 插件声明、能力注册、扩展点。 | plugin config。 | registry/capability。 |

## 1.3 模块依赖方向

依赖应尽量单向，避免循环引用：

```text
cli
 └─ orchestrator/exporter
     ├─ config
     ├─ meta
     │   └─ diagnostics
     ├─ source
     │   └─ diagnostics
     ├─ data
     │   ├─ meta
     │   └─ source
     ├─ validation
     │   ├─ meta
     │   └─ data
     ├─ artifact
     └─ executor
```

约束：

- `meta` 不依赖 `source`、`data`、`validation`、`exporter`。
- `MetaModel` 是纯 serde-owned 配置数据，不保存运行期索引和内部引用。
- `LinkedMetaModel<'meta>` 是从 `&'meta MetaModel` 派生出的只读借用视图，运行期索引和跨对象引用都放在这里。
- `source` 不理解字段类型和业务语义，只输出二维表与 source location。
- `data`、`validation`、`exporter` 对需要类型解析和快速查询的逻辑，应优先使用 `LinkedMetaModel<'meta>` / `LinkedTableMeta<'meta>`。
- `diagnostics` 是底层共享模块，不能反向依赖业务模块。

## 1.4 执行流水线与模块边界

实现应对应产品设计中的执行阶段：

1. **CLI / Config Resolve**：解析命令，读取工程配置，并递归展开 Project Config 顶层 `imports` 指向的配置片段。
2. **Meta Extract / Merge**：从合并后的 Project Config 中提取 `enums`、`structs`、`tables`，组成纯配置 `MetaModel`。
3. **Meta Link**：在所有 enum、struct、table 都加载完成后，构造 `LinkedMetaModel<'meta>`，解析类型和索引引用。
4. **Meta Validate**：对纯配置和可解析引用执行结构校验，输出稳定排序的 `MetaDiagnostic`。
5. **Source Load**：按 provider 加载二维表，可并发 I/O。
6. **Sheet Mapping / Data Parse**：根据已链接 Meta 映射 field path 并解析 typed value。
7. **DataSet Compose**：base source split merge 与 override apply。
8. **ExportView Derive / Target Tags Filter**：按当前 `ValidationMode` 和 target tags 派生 validation/export 输入视图，过滤 table / field / index / runtime rows；`validate --all-targets` 会为每个 target 分别派生视图。
9. **Validation on ExportView**：required/default、类型、索引、外键、validator、row export 引用等规则在当前 ExportView 上执行；需要完整数据集语义的规则必须显式声明 `input = "full_dataset"`。
10. **Artifact Generate**：复用同一个 target ExportView 生成 runtime data、代码、POT、asset manifest 等，避免 validation 与实际导出语义不一致。
11. **Artifact Write**：按 direct 或 temp_commit 写入，并输出 manifest。

当前已重点落地第 2～5 步中与 TOML Meta 相关的部分。

## 1.5 当前 Meta 目录结构

当前 Meta 模块已经拆分为目录模块：

```text
src/meta/
  mod.rs              # 对外 API 与 re-export
  model.rs            # 纯配置模型、Linked* 借用视图、查询方法
  loader.rs           # TOML 字符串/路径加载、imports 递归、合并、加载后校验
  link.rs             # MetaModel -> LinkedMetaModel<'meta> 的链接过程
  validate.rs         # Meta 结构校验
  diagnostic.rs       # MetaDiagnosticCode、MetaDiagnostic
  tests.rs            # Meta 单元测试
```

后续增长时可再补充：

```text
src/meta/
  syntax.rs           # TOML/XML raw syntax 与兼容层；目前直接 serde 到 MetaModel
  path.rs             # FieldPath、ArraySlot、path parser
  source_ref.rs       # SourceRef、fragment、provider 解析规则
  default_value.rs    # default 按 LinkedField 类型解析后的结构
  validator_ref.rs    # validator 声明与引用解析
```

Validator 的注册、编译和执行框架详见 `05-validator-framework-detail-design.md`。Meta 模块只承载 raw validator 配置和轻量结构校验，不依赖具体 validator 实现。

## 1.6 Load-then-Link 策略

Meta 模块采用两阶段策略：

```text
TOML files/imports
  --load/merge--> MetaModel
  --link-------> LinkedMetaModel<'meta>
  --validate---> diagnostics
```

核心规则：

- `MetaModel` 只保存 `String`、`Vec`、`Option` 等 serde 可直接加载的数据。
- 需要跨对象解析的字段先保持字符串形式，例如 `FieldMeta.field_type`、`IndexMeta.fields`、`TableMeta.primary_index`。
- Link 阶段要求所有 Meta 文件已经加载完成，因此可以安全地从 `LinkedMetaModel<'meta>` 借用 `&'meta MetaModel` 内部对象。
- Link 结果是只读视图；不把引用写回 `MetaModel`，避免 Rust 自引用结构问题。
- 运行期查询加速结构使用 `HashMap`，并放在 `LinkedMetaModel<'meta>` / `LinkedTableMeta<'meta>` / `LinkedStructMeta<'meta>` 内。

## 1.7 已 Link 与后续可 Link 的部分

当前已链接：

| 原始配置位置 | Raw 表达 | Linked 表达 | 目的 |
| --- | --- | --- | --- |
| `FieldMeta.field_type` | `String` | `LinkedField::Primitive` / `Enum(&EnumMeta)` / `Struct(&StructMeta)` | 区分 primitive、enum、struct，并发现未知类型。 |
| `TableMeta.indexes[].fields` | `Vec<String>` | `LinkedIndexMeta.fields: Vec<&FieldMeta>` | 索引校验和数据唯一性检查不再重复查字段名。 |
| `TableMeta.primary_index` / `IndexMeta.primary` | `Option<String>` / `bool` | `LinkedTableMeta::primary_index() -> Option<&LinkedIndexMeta>` | 解析出 table 当前选中的主索引。 |
| table / struct fields | `Vec<FieldMeta>` | `LinkedFieldMeta { meta, linked_field }` | 保留原始字段配置并附加解析后的字段类型。 |
| 全局查询索引 | 线性扫描 | `HashMap<&str, ...>` | 加速 table、struct、enum 查询。 |

后续适合继续 Link 的部分：

| 候选位置 | 当前/未来 Raw 表达 | 建议 Linked 表达 | 说明 |
| --- | --- | --- | --- |
| foreign key | `ForeignKeyMeta { fields, target_table, target_index }` | `LinkedForeignKeyMeta<'meta>` | 外键是一等关系数据，供 validation 隐式生成 referential-integrity validator，并供 codegen 生成关联查询 helper。 |
| `SourceRef.fragments` | `Vec<String>` | `Vec<&FragmentMeta>` | fragment 元数据落地后再解析。 |
| validator 引用 | validator 名字符串 | `&ValidatorMeta` 或 compiled validator handle | 需要 validator registry。 |
| export/filter 字段引用 | 字符串 field path | `LinkedFieldPath<'meta>` | 需要 field path parser 和 struct/array 展开规则。 |
| default value | `toml::Value` | typed default value | 需要依据 `LinkedField` 做类型化解析。 |
| struct 依赖图 | struct field type 字符串 | struct dependency graph | 可用于循环依赖检测和生成顺序。 |

## 1.8 测试分层

| 测试层级 | 位置 | 覆盖内容 |
| --- | --- | --- |
| 单元测试 | `src/meta/tests.rs` 与对应模块内 `#[cfg(test)]` | loader、link、validate、primitive/custom type、imports、错误诊断。 |
| fixture 单元测试 | `tests/fixtures/meta/` 或模块内字符串 fixture | 多表、多文件、错误样例。 |
| 集成测试 | `tests/cli_integration.rs` | CLI 从工程配置到执行结果的跨模块流程。 |
| 回归测试 | `tests/fixtures/standard_project/` | 标准工程样例稳定输出。 |

Meta 测试以模块内内联 TOML 为主，避免过早引入完整 CLI 流程；标准工程 fixture 用于覆盖跨模块回归。

## 1.9 当前完成标准

当前 Meta 阶段完成时应满足：

- `MetaModel` 能从 TOML 字符串、单文件路径和多路径加载。
- `imports` 支持相对路径递归加载，并通过 canonical path 去重。
- `MetaModel` 保持纯配置结构；`MetaSet` 仅作为 `MetaModel` 兼容别名。
- `FieldMeta.field_type` 保持字符串，Link 后解析为 primitive / enum / struct 引用。
- `IndexMeta.fields` 保持字符串，Link 后解析为 `LinkedIndexMeta.fields: Vec<&FieldMeta>`。
- `TableMeta.primary_index` 和 `IndexMeta.primary` Link 后可通过 `LinkedTableMeta::primary_index()` 取得选中的 `LinkedIndexMeta`。
- `LinkedMetaModel<'meta>` 及其子结构使用 `HashMap` 提供查询加速。
- `TableKind::Object` / `TableKind::Collection` 可解析，默认 `collection`。
- primitive 覆盖 `int`、`float`、`bool`、`string`、`text`、`asset`，并兼容 `i64` / `f64`。
- `desc` 可在 project、table、field、index、enum、struct 上保留。
- `primary_index` 与 index `primary = true` 的一致性可校验。
- 重复 table / enum / struct / enum value / field / index，索引引用不存在字段，未知 field type 都能产生确定性错误。
- Meta 单元测试可通过 `cargo test meta` 或 `cargo test` 运行；若本地工具链不支持依赖 edition，需要在环境说明中标注。
