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
