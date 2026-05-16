# 01. 程序实现模块划分大纲

本文档是 ConfigForge 的程序实现概要设计，用于把 `Documentation/Design/` 中的产品设计落到 Rust 工程模块边界。产品语义仍以 `Documentation/Design/` 为准；本目录只约束实现结构、依赖方向、阶段目标和测试组织。

## 1.1 近期开发目标

第一阶段先完成 **Meta 模块最小可运行闭环**：

1. 能从 TOML Meta 文件加载基础表定义。
2. 能解析 table、field、field type、index、primary index、desc 等基础信息。
3. 能执行基础结构校验，例如重复命名、索引字段引用、主索引一致性。
4. 能通过 `cargo test` 跑通 Meta 模块单元测试。
5. 保持后续扩展 XML、import、enum、struct、fragment、validator 时不需要推翻当前类型边界。

## 1.2 顶层模块划分

建议将当前 `src/*.rs` 的扁平模块逐步演进为目录模块。短期可以先在 `src/meta.rs` 内完成最小闭环；当代码增长后迁移到 `src/meta/` 子模块。

| 模块 | 建议路径 | 责任 | 主要输入 | 主要输出 |
| --- | --- | --- | --- | --- |
| CLI | `src/cli.rs` | 命令行参数、用户入口、错误展示。 | argv、环境变量。 | `ExportCommand`、用户可读输出。 |
| Project Config | `src/config/` | 加载 `ConfigForge.toml`、解析工程级配置。 | Project Config TOML。 | `ProjectConfig`。 |
| Meta | `src/meta/` | 加载、解析、合并、链接、校验 Meta。 | TOML/XML Meta 文件。 | `MetaModel`、`MetaDiagnostic`。 |
| Source | `src/source/` | 将 CSV/XLSX/远程表格归一化为二维 `SourceSheet`。 | source declaration。 | `SourceSheet`。 |
| Data Model | `src/data/` | 将 source cell 解析为 typed value 和逻辑表数据。 | `MetaModel`、`SourceSheet`。 | `DataSet`。 |
| Validation | `src/validation/` | 业务规则、索引、外键、过滤视图校验。 | `MetaModel`、`DataSet`、`ExportContext`。 | 结构化诊断。 |
| Exporter | `src/exporter/` | 生成 runtime data、代码、special artifacts。 | `ExportView`、target config。 | `ArtifactPlan`。 |
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
- `source` 不理解字段类型和业务语义，只输出二维表与 source location。
- `data` 可以依赖 `meta`，负责把 raw cell 转换成 typed value。
- `validation` 可以依赖 `meta` 与 `data`，但不直接写 artifact。
- `exporter` 使用已经校验过的 `ExportView`，不重新解释原始 source。
- `diagnostics` 是底层共享模块，不能反向依赖业务模块。

## 1.4 执行流水线与模块边界

实现应对应产品设计中的执行阶段：

1. **CLI / Config Resolve**：解析命令、读取工程配置。
2. **Meta Load**：读取 Meta 文档，解析为 raw document AST。
3. **Meta Build**：import 展开、合并、链接、规范化为 `MetaModel`。
4. **Source Load**：按 provider 加载二维表，可并发 I/O。
5. **Sheet Mapping / Data Parse**：根据 `MetaModel` 映射 field path 并解析 typed value。
6. **DataSet Compose**：base source split merge 与 override apply。
7. **Validation**：required/default、类型、索引、外键、validator、export filter。
8. **ExportView Build**：按 target/context 派生过滤视图。
9. **Artifact Generate**：生成 runtime data、代码、POT、asset manifest 等。
10. **Artifact Write**：按 direct 或 temp_commit 写入，并输出 manifest。

第一阶段只落地第 2、3 步中与基础 TOML Meta 相关的部分。

## 1.5 目录演进建议

当前仓库已有：

```text
src/meta.rs
src/source.rs
src/validation.rs
src/exporter.rs
```

Meta 模块变大后建议迁移为：

```text
src/meta/
  mod.rs              # 对外 API 与 re-export
  document.rs         # MetaDocument、MetaFormat、RawMetaDocument
  loader.rs           # MetaLoader、文件读取、import 递归
  syntax.rs           # TOML/XML 反序列化结构
  model.rs            # canonical MetaModel、TableMeta、FieldMeta 等
  field_type.rs       # FieldType、BuiltinValueType、TypeRef
  path.rs             # FieldPath、ArraySlot、path parser
  index.rs            # IndexMeta、PrimaryIndex 规则
  builder.rs          # Raw document -> MetaModel
  validate.rs         # Meta 结构校验
  diagnostic.rs       # MetaDiagnosticCode、MetaDiagnostic
  tests.rs            # 单元测试入口；复杂 fixture 可放 tests/fixtures/meta
```

迁移策略：

- 第一步可继续使用 `src/meta.rs`，先把 public 类型命名稳定下来。
- 当 `src/meta.rs` 超过约 500 行或出现 3 个以上独立职责时，迁移为 `src/meta/mod.rs` 加子模块。
- 迁移时保持 `pub mod meta;` 对外路径不变，避免影响调用方。

## 1.6 测试分层

| 测试层级 | 位置 | 覆盖内容 |
| --- | --- | --- |
| 单元测试 | 对应模块内 `#[cfg(test)]` | parser、builder、field path、index 校验、错误定位。 |
| fixture 单元测试 | `tests/fixtures/meta/` 或模块内字符串 fixture | 多表、多文件、错误样例。 |
| 集成测试 | `tests/cli_integration.rs` | CLI 从工程配置到执行结果的跨模块流程。 |
| 回归测试 | `tests/fixtures/standard_project/` | 标准工程样例稳定输出。 |

第一阶段 Meta 测试以单元测试为主，避免过早引入完整 CLI 流程。

## 1.7 第一阶段完成标准

第一阶段开发完成时应满足：

- `MetaSet` 或新的 `MetaModel` 能从 TOML 字符串和文件路径加载。
- `TableKind::Object` / `TableKind::Collection` 可解析。
- 基础 `FieldType` 覆盖 `int`、`float`、`bool`、`string`、`text`、`asset`。
- `desc` 可在 table、field、index 上保留。
- `primary_index` 与 index `primary = true` 的一致性可校验。
- 重复 table name、重复 field name、索引引用不存在字段能产生确定性错误。
- Meta 单元测试可通过 `cargo test meta` 或 `cargo test` 运行。

