# Artifact Generator Registry 重构 TODO

本文把 Generator / Requirements / Facets 的设计讨论拆成可分阶段实施的重构计划。目标是在不破坏现有 `[[targets.exports]]` 用户配置和 build pipeline 行为的前提下，把 build-facing 抽象从 “codegen registry + data exporter registry + 临时 schema_doc 接入” 收敛为统一 artifact generator 模型。

核心原则：

- `ArtifactGenerator` 是 build-facing 抽象：根据 export task、上下文输入和 `ArtifactSink` 生成一个或多个 artifact。
- `GeneratorRequirements` 声明 generator 需要哪些输入，例如 Meta、Data、assets、validation report。`MetaOnly` / `MetaAndData` 只是由 requirements 派生出的调度属性，不是继承体系。
- code / data / doc / report / manifest 是可组合能力 facet 或领域 helper。Lua 这类 data-backed code-like artifact 必须能复用 code artifact options，而不需要伪装成 MetaOnly codegen。
- 领域模块继续保持单向依赖：`codegen` 不依赖 `data` / `exporter`，`exporter` 不依赖 `codegen`，`schema_doc` 不依赖 `build`。统一 facade / adapter 应位于 build-facing 层或独立 `generator` 模块中。

## Step 0：确认当前行为基线

目标：在重构前锁定现有行为，确保后续每一步都可验证。

任务：

- 记录当前内置 export type 与 dependency：Rust/C++/C#/Java/TypeScript/schema_doc 为 MetaOnly；JSON/MessagePack/bin/POT/Lua/Asset Manifest 为 MetaAndData。
- 补充或确认测试覆盖：docs-only target 不需要 `project.input`；mixed target 中 MetaOnly branch 不等待 SourceRead；Lua 需要 `project.input`；schema_doc 输出路径与内容稳定。
- 保持 `[[targets.exports]]` TOML schema 不变。

验收：

- `cargo test`
- `cargo test schema_doc`
- `cargo test --test cli_integration schema_doc`
- 至少一个 mixed target integration 仍覆盖 MetaOnly + MetaAndData 同 target 构建。

建议提交：`test(build): lock generator dependency behavior`

## Step 1：引入 `generator` 模块的纯类型定义

目标：先引入不改变执行路径的基础类型，避免一次性迁移过大。

任务：

- 新增 `src/generator/`，定义：
  - `GeneratorRequirements`：`needs_meta`、`needs_data`、可选 `needs_assets`、`needs_validation_report`。
  - `ArtifactGeneratorId` / type matching helper（如有必要）。
  - `GeneratorTaskInput` / `GeneratorCommonInput` 的草案结构，只包含 shared context：linked Meta、target tags、project info、project root、target name、task name、type id、output、options、generated_at。
- 为 `GeneratorRequirements` 提供到当前 `ExportDependency` 的转换：`needs_data = false -> MetaOnly`，`needs_data = true -> MetaAndData`。
- 暂不修改 build 调度，不迁移任何 generator 实现。

验收：

- 编译通过。
- 新增单元测试覆盖 requirements 到 dependency 的转换。

建议提交：`feat(generator): add requirements model`

## Step 2：建立 Build-facing `GeneratorRegistry` facade

目标：把 `src/build/target.rs` 中的局部 registry 查询替换为统一 facade，但执行仍委托现有 codegen/exporter registry。

任务：

- 新增 `GeneratorRegistry`，提供：
  - `with_builtin_generators()`；
  - `find(type_id)` / `dependency(type_id)`；
  - alias 冲突检测和 ambiguous type 诊断。
- 初版 registry 内部可以持有：
  - `DataFormatRegistry`；
  - `Vec<Box<dyn CodeGenerator>>`；
  - 后续 direct artifact generators 预留列表。
- 将 `src/build/target.rs` 的 `ExporterRegistry` 改名/迁移为 `GeneratorRegistry` facade。
- 保持现有 dependency 结果与错误文案尽量稳定。

验收：

- target normalize 测试仍通过。
- unknown type、ambiguous type、MetaOnly 禁止 `data` 的诊断仍稳定。

建议提交：`refactor(build): use generator registry facade`

## Step 3：抽出可复用 `CodeArtifactOptions` facet

目标：把 code-like artifact 需要的基础配置从 “只能给 `CodeGenerator` 用” 改成可被 Lua / 未来 data-backed code generator 复用的 helper。

任务：

- 在不改变 public TOML 的前提下，从 `CodegenOptions` 中抽出或包裹：
  - file header template；
  - keyword conflict；
  - i18n options；
  - naming；
  - formatting；
  - generator-specific option bucket。
- 保持现有 `CodegenOptions` API 兼容，可以先让 `CodegenOptions` 内部包含 `CodeArtifactOptions`，或提供 `From` / builder。
- 提供 `CodeArtifactDefaults` / merge helper，使普通 codegen 和 Lua 能按各自默认值合并 partial config。
- 不在本步骤迁移 Lua，只准备复用点。

验收：

- 所有现有 codegen tests 通过。
- 文件头、formatting、naming 默认值测试继续覆盖 Rust/C++/TypeScript 等现有 backend。

建议提交：`refactor(codegen): extract code artifact options`

## Step 4：把 `schema_doc` 从 `CodeGenerator` 迁到 direct artifact generator

目标：修正 schema_doc 借用 codegen trait 的过渡状态。

任务：

- 让 `SchemaDocGenerator` 直接实现新的 `ArtifactGenerator` 或 direct generator adapter。
- requirements 声明为 `needs_meta = true`、`needs_data = false`。
- `schema_doc` 选项解析继续保留在 `schema_doc` 模块内，不再通过 `CodegenTaskConfig` / `CodeExportInput` 承载。
- `schema_doc` 不再需要实现 `default_naming()`、`line_comment_prefix()` 等 code-specific API。
- 从 `codegen::builtin_generators()` 中移除 `SchemaDocGenerator`，改由 `GeneratorRegistry::with_builtin_generators()` 注册。

验收：

- `cargo test schema_doc`
- `cargo test --test cli_integration schema_doc`
- 确认 docs-only target 仍不需要 `project.input`。
- 确认 build 中没有 `type_id == "schema_doc"` 特殊分支。

建议提交：`refactor(schema-doc): implement artifact generator directly`

## Step 5：把 MetaOnly branch 从 `execute_codegen_export_nodes` 改为 `execute_meta_only_generator_nodes`

目标：让 build 的执行命名和输入构造不再把所有 MetaOnly task 视为 codegen。

任务：

- 重命名 build 内部结构：
  - `codegen_exports` -> `meta_only_exports`；
  - `CodegenTaskContext` -> `MetaOnlyGeneratorContext`；
  - `execute_codegen_export_nodes` -> `execute_meta_only_generator_nodes`。
- 对 language codegen 使用 adapter 构造 `CodeExportInput` 并调用原 `CodeGenerator::generate_to_sink()`。
- 对 direct artifact generator 使用统一 `GeneratorInput`。
- 保持 parallelism、错误顺序、artifact sink 行为稳定。

验收：

- mixed target 测试仍证明 MetaOnly branch 可与 SourceRead 并行。
- 所有 codegen / schema_doc integration 通过。

建议提交：`refactor(build): execute meta-only generators through facade`

## Step 6：将 data-backed exporters 适配为 `ArtifactGenerator`

目标：让 Build-facing 执行层也不直接依赖 `DataFormatRegistry` 的 table/aggregate 分裂。

任务：

- 为 `DataFormatExporter` 和 `AggregateDataExporter` 提供 adapter，requirements 均包含 Meta + Data。
- `export_meta_and_data_tasks()` 可以继续作为 exporter 模块内部 helper，但 build-facing 分派应通过 `GeneratorRegistry` 找到 data-backed generator。
- 处理 table exporter 的特殊能力：runtime table artifact planning、per-table streaming、`RuntimeDataFile` metadata 返回。
- 处理 aggregate exporter 的特殊能力：`plan_artifacts()`、`export_to_sink()`。

验收：

- JSON / MessagePack / bin / POT / Asset Manifest / Lua 现有测试全部通过。
- 跨 export task artifact path 冲突诊断稳定。

建议提交：`refactor(exporter): adapt data exporters as artifact generators`

## Step 7：迁移 Lua 为 mixed generator 并复用 code facet

目标：验证 “requirements 与 code 能力解耦” 的核心设计。

任务：

- 将 `LuaTableExporter` 从纯 `AggregateDataExporter` 语义迁移为 mixed artifact generator，或保留 exporter 内部实现但通过 dedicated adapter 暴露为 mixed generator。
- Lua 配置解析复用 `CodeArtifactOptions` 中的 file header / formatting / naming 等通用能力；保留 Lua 专属选项如 `root_file`、`row_layout`、`lazy_require`。
- 保持 `type = "lua_table"` / alias `lua` 不变。
- 避免引入 `codegen -> data` 或 `exporter -> codegen` 的模块依赖；共享 code facet 应位于可被双方依赖的下层模块，或通过 `generator`/shared writer helper 暴露。

验收：

- Lua 现有导出测试通过。
- 新增测试覆盖 Lua 使用通用 formatting / file header / naming 选项（按实际支持范围）。
- 确认普通 Rust/C++/TS codegen 不受 Lua data dependency 影响。

建议提交：`refactor(lua): reuse code artifact options`

## Step 8：清理旧命名和文档

目标：完成概念收敛，避免新旧抽象并存造成误导。

任务：

- 文档中把 build-facing “exporter dependency contract” 统一改为 “generator requirements / derived dependency”。
- `CodeGenerator` 文档明确仅为 language codegen / code artifact facet，不再承担 schema_doc/doc/report 基类职责。
- `DataFormatRegistry` 文档明确为 exporter 模块内部 registry，Build-facing 查询走 `GeneratorRegistry`。
- 清理过渡 TODO 和已过期描述。

验收：

- grep 检查不再出现 “schema_doc 长期注册到 codegen registry” 这类过时表述。
- docs 与代码中的模块边界一致。

建议提交：`docs(generator): finalize registry refactor docs`

## Open Questions

- `GeneratorRequirements` 是否需要一开始就建模 `needs_assets` / `needs_validation_report`，还是先只保留 `needs_data` 并为未来留扩展字段？
- `validation_report` 当前是 data-backed 附加产物，是否应在 Step 6 一并纳入 `ArtifactGenerator`，还是单独拆 Step？
- `CodeArtifactOptions` 应放在 `codegen`、`generator`，还是新建更底层 `code_artifact` / `text_writer` 模块，以避免 `exporter -> codegen` 依赖倒置？
- `RuntimeDataFile` metadata 是否仍由 data exporter adapter 返回给未来 manifest，还是统一进入 `GeneratorOutput` side effects？

## Follow-up：单一 TaskGenerator 调度模型 TODO

当前实现已经把 Build-facing 执行入口收敛到 `GeneratorRegistry::require_generator(type_id)`，但 registry 内部仍保留 `DataFormatExporter`、`AggregateDataExporter`、`CodeGenerator`、direct `ArtifactGenerator` 四类来源，并通过 `ArtifactGeneratorRef` adapter 在调用点统一。下一阶段目标是继续把“统一”从调用点推进到模型层：任务系统只认识一种 task generator；四类对象只作为领域模块内部复用 helper 或 adapter source 存在，不再形成调度概念。

### Step F0：确认概念边界和命名

目标：先明确“task name”和“generator type id”的边界，避免把 export task instance 和 generator implementation 混为一谈。

任务：

- 明确 `[[targets.exports]].name` 是 task instance name，同一个 `type` 可以在同一 target 中被不同 name / output / options 多次实例化。
- 明确 generator 的全局标识是 `type_id + aliases`，用于匹配 `[[targets.exports]].type`。
- 将下一阶段 build-facing trait 暂命名为 `TaskGenerator` 或继续沿用 `ArtifactGenerator`，但文档必须说明它是 scheduler-facing trait，不是 code/data/doc 的领域基类。
- 更新 glossary / implementation docs 中容易混淆的 “generator name / task name / type id” 表述。

验收：

- 文档中不再把 `CodeGenerator`、`DataFormatExporter` 称为 Build 调度对象。
- `name` / `type_id` / `aliases` 的含义一致。

建议提交：`docs(generator): define task generator identity`

### Step F1：引入 `GeneratorDescriptor` 与统一匹配模型

目标：把 `type_id()`、`aliases()`、`requirements()` 聚合成 registry 可统一索引的 descriptor。

任务：

- 新增 `GeneratorDescriptor { type_id, aliases, requirements }`。
- 让现有 `ArtifactGenerator` 或新 `TaskGenerator` 返回 descriptor。
- 将 `matches(type_id)` 从 trait default method 改为 descriptor helper。
- 在 `GeneratorRegistry::with_builtin_generators()` 建立统一 descriptor index，启动时检测重复 type / alias。
- 保持当前 unknown type 与 ambiguous registration 诊断稳定或更清晰。

验收：

- registry 单元测试覆盖 type id、alias、重复注册、dependency 派生。
- `cargo test generator` / `cargo test build::target` 通过。

建议提交：`refactor(generator): add descriptor based registry`

### Step F2：统一 task 输入为 `GeneratorTaskInput`

目标：让调度系统不再选择 `generate_to_sink()` vs `generate_with_data_to_sink()` 两套执行入口，而是准备好 requirements 对应的 inputs 后调用单一 `generate()`。

任务：

- 新增 `GeneratorTaskInput`：包含 task instance metadata、`GeneratorCommonInput` 和 prepared inputs。
- 新增 `PreparedGeneratorInputs`，初期至少包含：
  - `MetaOnly`；
  - `MetaAndData { data_set, validation_report, pool }`。
- 将当前 `GeneratorDataInput` 迁移为 `PreparedGeneratorInputs::MetaAndData` 的内部结构或兼容 type alias。
- 保留 `plan_artifacts()`，但也改为接收 `GeneratorTaskInput`，便于需要 validation report / assets 的 generator 规划路径。

验收：

- MetaOnly codegen / schema_doc 仍不加载数据。
- JSON / MessagePack / bin / POT / Lua / validation_report 仍共享一次 DataSet 和 validation report。
- `cargo test` 通过。

建议提交：`refactor(generator): unify task input contract`

### Step F3：统一执行入口为 `TaskGenerator::generate()`

目标：Build 不再根据 dependency 调用不同 trait method，只负责准备输入和调用 generator。

任务：

- 将 `ArtifactGenerator::{generate_to_sink, generate_with_data_to_sink}` 收敛为 `generate(input, sink)`。
- MetaOnly branch 构造 `GeneratorTaskInput { prepared: MetaOnly }` 并调用 `generate()`。
- MetaAndData branch 构造 `GeneratorTaskInput { prepared: MetaAndData { ... } }` 并调用 `generate()`。
- generator 自己检查 prepared inputs 是否满足 descriptor requirements，并返回明确诊断。
- 删除调度侧 “该 generator 是否 data-backed” 的实现分类判断，只保留 requirements 驱动的 branch grouping。

验收：

- Build 中不再出现 `generate_with_data_to_sink`。
- Build 中不再出现 `require_data_generator` / `code_generator` / `direct_generator` 调用路径。
- `cargo test` 通过。

建议提交：`refactor(build): execute generators through single task entry`

### Step F4：收敛 registry 内部来源类型

目标：从 registry 结构上减少“四类对象”的可见性。

任务：

- 将 `GeneratorRegistry` 内部主索引改为 `Vec<Box<dyn TaskGenerator>>` 或 equivalent descriptor-indexed collection。
- `DataFormatExporter`、`AggregateDataExporter`、`CodeGenerator` 只在 `with_builtin_generators()` 或领域模块提供的 adapter factory 中转换为 `Box<dyn TaskGenerator>`。
- 评估 `DataFormatRegistry` 是否仍需要作为 exporter 内部 registry 保留；若保留，它不应被 Build-facing registry 作为公开字段或公开概念描述。
- 移除 `ArtifactGeneratorRef::{Code, Data, Direct}` 这类调度期 enum，改为注册期 adapter box。

验收：

- `GeneratorRegistry` 对 Build 只暴露 `require_generator()` / `dependency()` / descriptor 查询。
- 添加新 generator 时不需要修改 Build 分支代码。
- `cargo test` 通过。

建议提交：`refactor(generator): store unified task generators`

### Step F5：领域 facet 文档与 Lua 归属复查

目标：完成统一调度模型后，再复查 Lua / code artifact options 的模块归属，避免 exporter 与 codegen 互相倒置依赖。

任务：

- 重新评估 `CodeArtifactOptions` 是否继续位于 `codegen`，或下沉到 `generator` / `text_artifact` / `code_artifact` helper 模块。
- 如果 Lua 继续在 `exporter` 内部实现，明确它通过 adapter 暴露为 data-backed code-like task generator。
- 如果 Lua 迁入 `codegen/lua_table`，明确 data input 只通过 task generator contract 注入，不引入 `codegen -> build` 依赖。
- 更新 `08-exporter`、`09-codegen`、`10-build` 和 Lua 专题文档。

验收：

- Lua 仍保持 `type = "lua_table"` / alias `lua`。
- 普通 language codegen 不受 Data dependency 影响。
- 文档中 code/data/doc/report 被描述为 facet/helper，而不是调度分类。

建议提交：`docs(generator): document unified task generator facets`
