# Artifact Generator Registry 重构 TODO

本文把 Generator / Requirements / Facets 的设计讨论拆成可分阶段实施的重构计划。目标是在不破坏现有 `[[targets.exports]]` 用户配置和 build pipeline 行为的前提下，把 build-facing 抽象从 “codegen registry + data exporter registry + 临时 schema_doc 接入” 收敛为统一 artifact generator 模型。

核心原则：

- `ArtifactGenerator` 是 build-facing 抽象：根据 export task、上下文输入和 `ArtifactSink` 生成一个或多个 artifact。
- `GeneratorRequirements` 声明 generator 需要哪些输入，例如 Meta、Data、assets、validation report。`MetaOnly` / `MetaAndData` 只是由 requirements 派生出的调度属性，不是继承体系。
- code / data / doc / report / manifest 是可组合能力 facet 或领域 helper。Lua 这类 data-backed code-like artifact 必须能复用 code artifact options，而不需要伪装成 MetaOnly codegen。
- 领域模块继续保持单向依赖：`codegen` 不依赖 `data` / `exporter`，`exporter` 不依赖 `codegen`，`schema_doc` 不依赖 `build`。统一 facade / adapter 应位于 build-facing 层或独立 `generator` 模块中。

## Current status

已完成的过渡步骤不再在本 TODO 中逐项保留；当前代码已经具备以下基线：

- `GeneratorRegistry` 对 Build 只暴露统一的 `ArtifactGenerator` 查询入口。
- `GeneratorDescriptor { type_id, aliases, requirements }` 负责 type / alias 匹配。
- `GeneratorTaskInput` + `PreparedGeneratorInputs` 是 Build 传给 generator 的统一 task 输入。
- Build 的 MetaOnly / MetaAndData 分支都调用 `ArtifactGenerator::generate()`；`generate_to_sink()` / `generate_with_data_to_sink()` 只是 adapter 内部兼容入口。
- `DataFormatExporter`、`AggregateDataExporter`、`CodeGenerator` 仅作为领域模块 helper 或 adapter source，不再直接表达 Build 调度 dependency。
- `ExportDependency` 只作为当前 branch pipeline 的二值调度视图，由 `GeneratorRequirements::needs_data` 派生；新的 generator 能力声明应放在 `GeneratorRequirements`。

## Remaining TODO

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
