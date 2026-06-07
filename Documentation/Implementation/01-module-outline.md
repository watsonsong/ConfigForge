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
| Runtime Contract | `src/runtime_contract.rs` | data artifact toolkit / code artifact toolkit / generator / build 共享的底层运行时契约，包括 runtime data file metadata 和 binary schema revision 计算。 | `LinkedTableMeta<'meta>`、`TargetTags`。 | `RuntimeDataFile`、schema revision。 | 本文档总览。 |
| Data | `src/data/` | 根据加载计划读取 CSV 等数据源，形成 raw `DataSet`，并提供 Meta-aware resolver。 | `DataLoadPlan`、source files、`LinkedMetaModel<'meta>`。 | `DataSet`、`ResolvedDataSet` / resolved view。 | `05-data-module-detail-design.md` |
| Asset Resolver | `src/asset.rs` | 根据 Meta `asset_kind` 与 Project asset search paths 解析资源引用，生成 canonical export ref，并执行存在性、sidecar、命名、prefix / regex、target-gated deny 等检查。 | `LinkedMetaModel<'meta>`、`AssetsConfig`、`TargetTags`、raw asset ref。 | `ResolvedAssetRef` / validation error。 | `08-asset-validation-and-manifest.md` |
| Validation | `src/validation/` | 数据归一化、基础规则校验、target-aware ExportValidationView、asset reference validation、注册式 validator；入口、table、FK、struct value、default、asset 逻辑分文件维护。 | `LinkedMetaModel<'meta>`、`DataSet`、`TargetTags`、可选 `AssetsConfig`。 | `ValidationReport` / `ValidationIssue`。 | `06-validation-module-detail-design.md` |
| Artifact | `src/artifact/` | 统一制品模型、路径冲突检查、dry-run、direct/temp-commit 写入策略和写入结果。 | `Artifact` 兼容列表或 `ArtifactSink` stream 写入请求、写入模式。 | `ArtifactWriteResult` / write entries / 文件系统产物。 | `07-artifact-module-detail-design.md` |
| Artifact Info | `src/artifact_info.rs` | 跨 codegen、schema_doc、validation report / manifest 等 artifact 生成器共享的轻量 Project 信息 facade，避免文档/导出模块依赖 codegen 专属类型。 | `ProjectConfigMeta`。 | `ArtifactProjectInfo`。 | 本文档总览。 |
| Generator | `src/generator/` | Build-facing artifact generator facade、generator registry、input requirements 与 capability facet 适配层；目标态也作为所有具体 export type generator 的归属目录。它把 `MetaOnly` / `MetaAndData` 作为 requirements 派生出的调度属性，而不是继承体系；Rust / C++ / JSON / MessagePack / `bin` / POT / Lua / Asset Manifest / Schema Doc / report 等具体产物最终应作为平级 generator 接入。 | `NormalizedExportTask`、`LinkedMetaModel<'meta>`、可选 `DataSet` / validation report / assets、`ArtifactProjectInfo`、`ArtifactSink`。 | artifact generator dispatch / `GeneratorRequirements`。 | `10-build-module-detail-design.md` |
| Data Artifact Toolkit（当前 `Exporter`） | 当前 `src/exporter/`，目标 `src/data_artifact/` | Target-aware runtime payload、`ExportedTablePayload` / `ExportedValue`、payload builder、runtime table serialization helper、binary layout helper 和 legacy data exporter adapter。`exporter` 这个名称在具体 generator 迁入 `src/generator/` 后容易误导为“生成器归属地”，因此目标命名为 data artifact toolkit。 | `LinkedMetaModel<'meta>`、`TargetTags`、`DataSet`、`ArtifactSink`。 | payload model、runtime table artifact helper、`RuntimeDataFile` metadata。 | `08-exporter-module-detail-design.md` |
| SchemaDoc | `src/schema_doc/` | 基于 linked Meta、Project info 与 TargetTags 生成表结构文档 normalized model，并规划/写入 Markdown schema docs artifact。当前直接实现 `ArtifactGenerator`，作为 requirements 只含 Meta 的 doc artifact generator 注册到 `GeneratorRegistry`。它不属于 runtime data exporter，也不读取 source data。 | `LinkedMetaModel<'meta>`、`TargetTags`、`ArtifactProjectInfo`、`ArtifactSink`。 | `SchemaDoc` / schema docs artifacts。 | `../Design/11-schema-doc-generator.md` |
| Code Artifact Toolkit（当前 `Codegen`） | 当前 `src/codegen/`，目标 `src/code_artifact/` | 语言无关 CodeModel IR、CodeWriter、naming / formatting / keyword conflict、`CodeArtifactOptions`、runtime reader contract helper 和 legacy `CodeGenerator` adapter。`codegen` 这个名称在具体语言 generator 迁入 `src/generator/` 后容易误导为“生成器归属地”，因此目标命名为 code artifact toolkit。 | `LinkedMetaModel<'meta>`、target tags、`ArtifactSink`。 | CodeModel / writer / code artifact helper。 | `09-codegen-module-detail-design.md` |
| Build | `src/build/` | 工程构建主编排：归一化 target，并按 generator requirements 调度 data load、validation、artifact generator 执行和 artifact 写入。 | `ProjectConfig`、CLI options。 | `BuildOutput`、文件系统产物。 | `10-build-module-detail-design.md` |

## 1.3 强制单向依赖规则

模块之间必须保持**单向依赖**，禁止形成循环依赖。任何新增 API、re-export 或 helper 都必须先判断是否会让下层模块反向依赖上层模块。

模块公共入口必须遵循**最小公开 API**原则：目录拆分是内部实现组织，不代表子模块都需要公开。
默认使用私有 `mod`，再由模块根按稳定契约选择性 `pub use`。只有配置 schema、跨模块输入/输出、
插件 trait / registry、用户可依赖的 facade 才应暴露为 `pub`；纯 builder helper、格式实现文件、
运行时中间产物和测试辅助不应因为位于独立文件而变成公共 API。

目标依赖方向（当前代码仍在从历史 `codegen` / `exporter` 分类迁移到 generator-first 结构；迁移期可通过 adapter 保持兼容，但新设计应按下图约束）：

```text
main / cli
  └─ build
      ├─ project
      │   └─ meta
      ├─ meta
      ├─ data
      │   ├─ meta
      │   └─ diagnostic
      ├─ validation
      │   ├─ asset
      │   ├─ meta
      │   ├─ data
      │   └─ diagnostic
      ├─ asset
      │   ├─ meta
      │   └─ project
      ├─ generator      # build-facing facade + concrete artifact generators
      │   ├─ meta
      │   ├─ data
      │   ├─ validation
      │   ├─ artifact
      │   ├─ artifact_info
      │   ├─ code_artifact   # 当前代码路径为 codegen；仅作为工具箱依赖
      │   ├─ data_artifact   # 当前代码路径为 exporter；仅作为工具箱依赖
      │   └─ runtime_contract
      ├─ runtime_contract
      │   └─ meta
      ├─ artifact_info
      └─ artifact

diagnostic foundation 位于底层共享层；`diagnostic::error` 是 crate API 错误 facade，用于在边界层聚合各模块诊断。
`runtime_contract` 是 data artifact toolkit / code artifact toolkit / generator / build 的共享底层契约层；`code_artifact` 与 `data_artifact` 是被具体 generator 依赖的工具箱，不是 Build 直接编排的平级 generator 分类。迁移期的物理路径仍可能是 `src/codegen/` 与 `src/exporter/`，但新依赖不应扩大 Build 对它们的直接认知。
```

强制约束：

- `diagnostic` 的基础类型只能依赖标准库和通用 serde 等底层库，不能依赖 `meta`、`data`、`validation`、`exporter`、`codegen`、`project`、`build`。
- `diagnostic::error` 作为 crate 错误桥接层可以引用稳定诊断类型和各模块诊断集合，但不能承载业务逻辑，也不能让业务模块为了错误格式化反向依赖上层模块。
- `meta` 不依赖 `project`、`data`、`validation`、`exporter`、`codegen`、`build`。Meta 只描述 schema 与 link/validate 结果。
- `data` 可以依赖 `meta` 做 resolver，但 source reader 层应保持 source-neutral；CSV reader 不能读取 exporter/codegen/validation 语义。
- `runtime_contract` 可以依赖 `meta` 表达 target-visible runtime schema，但不能依赖 `data`、`validation`、`data_artifact`、`code_artifact`、`generator`、`project`、`build`。
- `asset` 可以依赖 `meta` 与 `project` 的资产配置模型，不能依赖 `data`、`validation`、`exporter`、`codegen`、`build`。
- `validation` 可以依赖 `meta`、`data`、`diagnostic`、`asset`，不能依赖 `data_artifact`、`code_artifact`、`generator`、`build`。
- `artifact` 可以依赖标准库、`diagnostic::error` 等底层能力，不能依赖 `meta`、`data`、`validation`、`data_artifact`、`code_artifact`、`generator`、`project`、`build`。
- `artifact_info` 是轻量共享 facade，只依赖标准库；`project` 可从 `ProjectConfigMeta` 构造它，code-like / doc / report / manifest generator 可消费它，避免互相依赖专属 Project 类型。
- `generator` 是 build-facing facade 与具体 artifact generator 的归属层，可以依赖满足 generator requirements 所需的共享输入类型，也可以按需依赖 `code_artifact` / `data_artifact` 工具箱；不能让工具箱或下层领域模块反向依赖 build。`MetaOnly` / `MetaAndData` 在此层只是 `GeneratorRequirements` 派生出的调度属性。
- `data_artifact`（当前 `src/exporter/`）可以依赖 `meta`、`data`、`artifact`、`diagnostic`、`runtime_contract` 和 code artifact option facet，不能依赖 `validation`、`generator` 或 `build`；导出前校验由 build 编排层调用 validation。Build-facing data-backed 执行通过具体 generator 或迁移期 adapter 暴露为 requirements 包含 Data 的 artifact generator。
- `schema_doc` 目标态应迁入 `src/generator/schema_doc/`，作为 requirements 只含 Meta 的 doc artifact generator；迁移期独立模块可以依赖 `meta`、`artifact`、`artifact_info` 和 `generator` trait，但不能依赖 `data`、`validation`、`data_artifact`、`code_artifact` 或 `build`，也不能读取 source data。
- `code_artifact`（当前 `src/codegen/`）可以依赖 `meta`、`artifact` 和 `runtime_contract`；生成代码使用逻辑 table name 作为 runtime load key，不依赖 data artifact toolkit 真实写出的 runtime data artifact path / format；不能依赖 `data_artifact`、`data`、`validation`、`generator`、`build`。`CodeGenerator` 表达 code artifact 能力，不应长期承担 doc/report/manifest 等非代码 generator 的统一基类职责。
- `project` 只解析 `TargetExportConfig` 这一统一 Project TOML 结构，不引用 data/code artifact toolkit 内部 task 配置，也不能依赖 `build`。
- `build` 是顶层业务编排层，可以依赖 `project`、`meta`、`data`、`validation`、`generator`、`runtime_contract` 和 `artifact`；不应新增对具体 code/data artifact toolkit 的直接调度依赖。迁移期遗留的 `codegen` / `exporter` 直接引用应逐步收敛到 generator adapter 内部。
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
4. **Target Normalize**：`build` 归一化 target 与统一 export task，并根据 generator requirements 构造执行视图。
5. **Data Load**：`data` 根据 target 可见表和 source declaration 生成加载计划并读取 `DataSet`。
6. **Data Normalize / Resolve**：`validation` 调用 DataResolver 应用 default、struct JSON / expanded value、fragment 等规则，形成可校验输入。
7. **Target-aware Validation**：`validation` 派生 `ExportValidationView`，执行 required/type/enum/index/FK/validator/asset reference 等规则并输出 `ValidationReport`。
8. **Data-backed Generators**：`build` 在 validation 通过后复用 `AssetResolver` 将 target-visible asset 字段物化为 canonical export ref，再由 requirements 包含 Data 的 generator 生成 JSON / MessagePack / `bin` runtime data，以及 POT / Lua / Asset Manifest 等聚合或 mixed artifact；这些 generator 可复用 data artifact toolkit 的 payload / serialization helper。
9. **Meta-only Generators**：requirements 只含 Meta 的 generator 在 MetaOnly 分支基于 `LinkedMetaModel<'meta>`、Project info 与 target tags 生成代码、Schema Docs 或其他结构产物；不触发 data load / validation。
10. **Artifact Write**：`artifact` 统一检查制品路径冲突并执行 dry-run / direct / temp-commit 写入；由 build 创建共享 sink 并传给 generator。

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
| `08-exporter-module-detail-design.md` | 模块 / 迁移说明 | 当前 Exporter 模块与目标 data artifact toolkit 边界、runtime payload、格式 registry、JSON / MessagePack / `bin`、ObjectTable payload。 |
| `09-codegen-module-detail-design.md` | 模块总览 / 迁移说明 | 当前 Codegen 模块与目标 code artifact toolkit 边界、CodeModel / CodeWriter / options、专题索引和 backend capability matrix；详细设计见 `09a`~`09l`。 |
| `09a-codegen-runtime-reader-contract.md` | Codegen 专题 | Runtime reader contract、eager/lazy loading、TypeScript sync API、逻辑 table name 与 schema revision。 |
| `09b-codegen-rust-backend.md` | Codegen 专题 | Rust backend generated API 与 lazy loading。 |
| `09c-codegen-cpp-backend.md` | Codegen 专题 | C++ backend generated API、namespace/ownership 与 lazy loading。 |
| `09d-codegen-typescript-backend.md` | Codegen 专题 | TypeScript backend generated API、async/sync loading 与 lazy loading。 |
| `09e-codegen-runtime-adapters.md` | Codegen 专题 | 标准 runtime adapters、C++ CMake、TS packages、Rust templates 与 compliance fixtures。 |
| `09f-codegen-core-model-and-options.md` | Codegen 专题 | 核心输入输出模型、CodeModel IR、命名策略、类型映射、`CodeWriter` 与文件头模板。 |
| `09g-codegen-index-and-relation-helpers.md` | Codegen 专题 | CollectionTable index lookup、`linked_field` relation helper、row-index cache 与 `link_all`。 |
| `09h-codegen-generator-api-and-tests.md` | Codegen 专题 | `CodeGenerator` 接口、新语言接入、文件组织、诊断和测试计划。 |
| `09i-lua-embedded-data-generator.md` | Codegen / Exporter 专题 | Lua data+code 合一导出；作为 `type = "lua_table"` 的 MetaAndData export task 消费 Meta + Data，生成每表 `.lua`、root facade、row array/metatable 和索引。 |
| `09j-codegen-cpp-unreal-backend.md` | Codegen 专题 | Unreal C++ backend generated API、containers / Blueprint 支持、shim 测试和与标准 C++ backend 的复用边界。 |
| `09k-codegen-csharp-backend.md` | Codegen 专题 | C# backend generated API、runtime reader contract、index/relation/text/lazy 设计和测试策略。 |
| `09l-codegen-java-backend.md` | Codegen 专题 | Java backend generated API、单文件嵌套 public API、runtime reader contract、index/relation/text/lazy 设计和测试策略。 |
| `10-build-module-detail-design.md` | 模块 | 构建编排、target/task binding、并行执行边界和 artifact 写入策略。 |
| `11-foreign-key-detail-design.md` | 专项 | 外键 raw/link model、引用完整性校验、target 语义、codegen staging。 |
| `12-struct-expanded-value-detail-design.md` | 专项 | Struct source layout 自动判定和 expanded CSV field-path。 |
| `13-runtime-binary-format-detail-design.md` | 专项 | `bin` runtime data 紧密二进制格式、schema revision、编码规则和 reader 方案。 |

## 1.6 测试分层

| 测试层级 | 位置 | 覆盖内容 |
| --- | --- | --- |
| 模块单元测试 | `src/<module>/tests.rs` 或模块内 `#[cfg(test)]` | 单模块数据结构、边界条件、错误诊断。 |
| 跨模块单元测试 | build / exporter / codegen 模块测试 | target normalization、export task dependency 分发、runtime data artifact 与 codegen 解耦。 |
| 集成测试 | `tests/cli_integration.rs` | CLI 从工程配置到执行结果的跨模块流程。 |
| fixture 回归测试 | `tests/fixtures/standard_project/` | 标准工程样例稳定输出、runtime reader 编译与读取。 |

新增模块或专项能力时，应同时明确其单元测试归属和至少一个端到端回归入口；如果能力只影响模块内部，不应为了测试方便突破单向依赖规则。
