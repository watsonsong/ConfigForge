# 01. 程序实现模块划分大纲

本文档是 ConfigForge 的程序实现概要设计，用于把 `Documentation/Design/` 中的产品设计落到 Rust 工程模块边界。产品语义仍以 `Documentation/Design/` 为准；本目录只约束实现结构、依赖方向、阶段目标和测试组织。

## 1.1 文档组织原则

`Documentation/Implementation/` 按“模块文档优先，专项技术文档随后”的结构维护：

1. `01-module-outline.md`：总览模块职责、依赖方向、执行流水线和测试分层。
2. `02`～`10`：每个已落地或即将落地的核心模块各自维护一份实现文档，描述该模块内部结构、公共 API、依赖约束和测试边界。
3. `11` 起：跨模块专项技术点文档，例如 foreign key、struct expanded value 等。
4. 过程型阶段记录放在 `Documentation/DevLogs/`；模块文档只描述当前模块边界和计划中的实现状态。

## 1.2 顶层模块划分

仓库核心业务模块采用目录模块承载实现；`src/` 外层只保留 crate 入口、CLI 入口和少量稳定公共入口文件。

| 模块 | 路径 | 责任 | 主要输入 | 主要输出 | 模块文档 |
| --- | --- | --- | --- | --- | --- |
| CLI | `src/cli.rs` / `src/main.rs` | 命令行参数、用户入口、错误展示。CLI 只做输入解析和调用 build，不承载业务流程。 | argv、环境变量。 | `BuildOptions`、用户可读输出。 | 本文档总览。 |
| Diagnostic / Error | `src/diagnostic/` | 统一诊断基础类型、严重级别、位置、报告聚合、格式化，以及 crate 级错误桥接。 | 各模块诊断条目、底层错误。 | `DiagnosticSeverity`、`DiagnosticLocation`、`DiagnosticReport<T>`、`ConfigForgeError`。 | `02-diagnostic-module-detail-design.md` |
| Project | `src/project/` | 加载 `ConfigForge.toml`、解析工程级配置、合并 imports、归一化路径、抽取 Meta。 | Project Config TOML。 | `ProjectConfig`、`TargetConfig`、`MetaModel`。 | `03-project-module-detail-design.md` |
| Meta | `src/meta/` | Meta schema 加载、纯配置模型、link 借用视图、结构校验；模型定义按 raw / linked / types / conditions 拆分。 | Meta TOML 片段。 | `MetaModel`、`LinkedMetaModel<'meta>`、`MetaDiagnostic`。 | `04-meta-module-detail-design.md` |
| Data | `src/data/` | 根据加载计划读取 CSV 等数据源，形成 raw `DataSet`，并提供 Meta-aware resolver。 | `DataLoadPlan`、source files、`LinkedMetaModel<'meta>`。 | `DataSet`、`ResolvedDataSet` / resolved view。 | `05-data-module-detail-design.md` |
| Validation | `src/validation/` | 数据归一化、基础规则校验、target-aware ExportValidationView、注册式 validator；入口、table、FK、struct value、default 逻辑分文件维护。 | `LinkedMetaModel<'meta>`、`DataSet`、`TargetTags`。 | `ValidationReport` / `ValidationIssue`。 | `06-validation-module-detail-design.md` |
| Artifact | `src/artifact/` | 统一制品模型、路径冲突检查、dry-run、direct/temp-commit 写入策略和写入结果。 | `Artifact` 列表、写入模式。 | `ArtifactWriteResult` / write entries / 文件系统产物。 | `07-artifact-module-detail-design.md` |
| Exporter | `src/exporter/` | Runtime data export contract、JSON / MessagePack / `bin` exporter、data format registry、`TargetDataConfig`。 | `LinkedMetaModel<'meta>`、`TargetTags`、`DataSet`。 | `RuntimeDataFile` / runtime data artifacts。 | `08-exporter-module-detail-design.md` |
| Codegen | `src/codegen/` | 语言无关 CodeModel IR，Rust / TypeScript / C++ 代码生成；每种语言按 runtime / types / reader / repository / symbols 拆分。 | `LinkedMetaModel<'meta>`、target tags、runtime data files。 | `GeneratedFile`。 | `09-codegen-module-detail-design.md` |
| Build | `src/build/` | 工程构建主编排：归一化 target，绑定 data/code task，并按 target 调度 data load、validation、runtime export、codegen、artifact 写入。 | `ProjectConfig`、CLI options。 | `BuildOutput`、文件系统产物。 | `10-build-module-detail-design.md` |

## 1.3 强制单向依赖规则

模块之间必须保持**单向依赖**，禁止形成循环依赖。任何新增 API、re-export 或 helper 都必须先判断是否会让下层模块反向依赖上层模块。

模块公共入口必须遵循**最小公开 API**原则：目录拆分是内部实现组织，不代表子模块都需要公开。
默认使用私有 `mod`，再由模块根按稳定契约选择性 `pub use`。只有配置 schema、跨模块输入/输出、
插件 trait / registry、用户可依赖的 facade 才应暴露为 `pub`；纯 builder helper、格式实现文件、
运行时中间产物和测试辅助不应因为位于独立文件而变成公共 API。

当前目标依赖方向：

```text
main / cli
  └─ build
      ├─ project
      │   ├─ meta
      │   ├─ codegen        # 仅引用 task config 类型
      │   └─ exporter       # 仅引用 data task config 类型
      ├─ meta
      ├─ data
      │   ├─ meta
      │   └─ diagnostic
      ├─ validation
      │   ├─ meta
      │   ├─ data
      │   └─ diagnostic
      ├─ exporter
      │   ├─ meta
      │   ├─ data
      │   ├─ artifact
      │   └─ diagnostic
      ├─ codegen
      │   ├─ meta
      │   ├─ exporter       # 只消费 runtime data file 描述
      │   └─ artifact
      └─ artifact

diagnostic foundation 位于底层共享层；`diagnostic::error` 是 crate API 错误 facade，用于在边界层聚合各模块诊断。
```

强制约束：

- `diagnostic` 的基础类型只能依赖标准库和通用 serde 等底层库，不能依赖 `meta`、`data`、`validation`、`exporter`、`codegen`、`project`、`build`。
- `diagnostic::error` 作为 crate 错误桥接层可以引用稳定诊断类型和各模块诊断集合，但不能承载业务逻辑，也不能让业务模块为了错误格式化反向依赖上层模块。
- `meta` 不依赖 `project`、`data`、`validation`、`exporter`、`codegen`、`build`。Meta 只描述 schema 与 link/validate 结果。
- `data` 可以依赖 `meta` 做 resolver，但 source reader 层应保持 source-neutral；CSV reader 不能读取 exporter/codegen/validation 语义。
- `validation` 可以依赖 `meta`、`data`、`diagnostic`，不能依赖 `exporter`、`codegen`、`build`。
- `artifact` 可以依赖标准库、`diagnostic::error` 等底层能力，不能依赖 `meta`、`data`、`validation`、`exporter`、`codegen`、`project`、`build`。
- `exporter` 可以依赖 `meta`、`data`、`artifact`、`diagnostic`，不能依赖 `validation`、`codegen`、`build`；导出前校验由 build 编排层调用 validation。
- `codegen` 可以依赖 `meta`、`artifact` 和 exporter 暴露的 runtime data file 描述；不能依赖 `data`、`validation`、`build`。
- `project` 可以引用 `codegen::CodegenTaskConfig` 和 `exporter::TargetDataConfig` 来表达 Project TOML 聚合结构，但不能依赖 `build`。
- `build` 是顶层业务编排层，可以依赖 `project`、`meta`、`data`、`validation`、`exporter`、`codegen`；这些被编排模块不能反向依赖 `build`。
- `cli` 只能调用 `build` 和展示错误；不能绕过 build 直接串联底层模块。

如果一个新增能力需要跨模块协作，优先选择：

1. 把共享数据结构下沉到更底层模块；
2. 由上层 build 编排组合多个模块；
3. 使用 trait / input model 反转具体实现，而不是让下层模块引用上层模块。

## 1.4 执行流水线与模块边界

实现应对应产品设计中的执行阶段：

1. **CLI / Options**：`cli` 解析命令和参数，转换为 build options。
2. **Project Load / Merge**：`project` 加载 `ConfigForge.toml`，递归展开 imports，归一化工程路径。
3. **Meta Extract / Link / Validate**：`project` 抽取 `MetaModel`，`meta` 构造 `LinkedMetaModel<'meta>` 并输出结构诊断。
4. **Target Normalize**：`build` 归一化 target、data task、code task 和 task binding。
5. **Data Load**：`data` 根据 target 可见表和 source declaration 生成加载计划并读取 `DataSet`。
6. **Data Normalize / Resolve**：`validation` 调用 DataResolver 应用 default、struct JSON / expanded value、fragment 等规则，形成可校验输入。
7. **Target-aware Validation**：`validation` 派生 `ExportValidationView`，执行 required/type/enum/index/FK/validator 等规则并输出 `ValidationReport`。
8. **Runtime Data Export**：`exporter` 使用同一个 target tags 和 export view 语义生成 JSON / MessagePack / `bin` runtime data。
9. **Codegen**：`codegen` 基于 `LinkedMetaModel<'meta>`、target tags 和真实 runtime data file 列表生成代码。
10. **Artifact Write**：`artifact` 统一检查制品路径冲突并执行 dry-run / direct / temp-commit 写入；由 build 负责收集 exporter/codegen 产物并调用。

## 1.5 当前 Implementation 文档目录

| 文档 | 类型 | 内容 |
| --- | --- | --- |
| `01-module-outline.md` | 总览 | 模块职责、依赖方向、流水线和测试分层。 |
| `02-diagnostic-module-detail-design.md` | 模块 | 统一诊断与 crate 错误桥接。 |
| `03-project-module-detail-design.md` | 模块 | Project Config schema、imports、路径归一化和 Meta 抽取。 |
| `04-meta-module-detail-design.md` | 模块 | Meta 模型、Load-then-Link、结构校验和 Meta 诊断。 |
| `05-data-module-detail-design.md` | 模块 | Data load、CSV reader、DataSet、RowSourceInfo、DataResolver、ObjectTable source layout。 |
| `06-validation-module-detail-design.md` | 模块 | Validation 主流程、ExportValidationView、ValidationReport、validator registry。 |
| `07-artifact-module-detail-design.md` | 模块 | Artifact model、writer、dry-run、direct/temp-commit 策略和写入结果。 |
| `08-exporter-module-detail-design.md` | 模块 | Runtime data exporter、格式 registry、JSON / MessagePack / `bin`、ObjectTable payload。 |
| `09-codegen-module-detail-design.md` | 模块 | CodeModel IR、语言生成器、runtime reader contract 和按语言拆分的生成器结构。 |
| `10-build-module-detail-design.md` | 模块 | 构建编排、target/task binding、并行执行边界和 artifact 写入策略。 |
| `11-foreign-key-detail-design.md` | 专项 | 外键 raw/link model、引用完整性校验、target 语义、codegen staging。 |
| `12-struct-expanded-value-detail-design.md` | 专项 | Struct source layout 自动判定和 expanded CSV field-path。 |
| `13-runtime-binary-format-detail-design.md` | 专项 | `bin` runtime data 紧密二进制格式、schema revision、编码规则和 reader 方案。 |

## 1.6 测试分层

| 测试层级 | 位置 | 覆盖内容 |
| --- | --- | --- |
| 模块单元测试 | `src/<module>/tests.rs` 或模块内 `#[cfg(test)]` | 单模块数据结构、边界条件、错误诊断。 |
| 跨模块单元测试 | build / exporter / codegen 模块测试 | target normalization、data/code binding、runtime data file 与 codegen 对接。 |
| 集成测试 | `tests/cli_integration.rs` | CLI 从工程配置到执行结果的跨模块流程。 |
| fixture 回归测试 | `tests/fixtures/standard_project/` | 标准工程样例稳定输出、runtime reader 编译与读取。 |

新增模块或专项能力时，应同时明确其单元测试归属和至少一个端到端回归入口；如果能力只影响模块内部，不应为了测试方便突破单向依赖规则。
