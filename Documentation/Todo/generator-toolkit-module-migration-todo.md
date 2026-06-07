# Generator-first module migration TODO

目标：把 build-facing 产物归属统一到 `src/generator/<type_id>/`，让当前 `src/codegen/` 和 `src/exporter/` 从“具体 generator 归属地”收敛为可复用工具箱。目标命名为：

- `code artifact toolkit`：当前物理路径 `src/codegen/`，长期可迁为 `src/code_artifact/`；提供 CodeModel IR、CodeWriter、naming / formatting / keyword conflict、`CodeArtifactOptions`、runtime reader contract helper。
- `data artifact toolkit`：当前物理路径 `src/exporter/`，长期可迁为 `src/data_artifact/`；提供 target-aware payload、`ExportedTablePayload` / `ExportedValue`、runtime table serialization、binary layout、legacy exporter adapter。

原则：

1. `src/generator/` 是所有具体 export type 的目标归属地。用户配置中的 `[[targets.exports]].type = "xxx"` 应能在 `src/generator/xxx/` 找到对应实现。
2. `build` 只面向 `GeneratorRegistry` / `ArtifactGenerator` / `GeneratorRequirements`，不按 code/data/doc 分类直接调度。
3. `code artifact toolkit` 与 `data artifact toolkit` 只提供 helper / facet / legacy adapter，不作为 Build-facing registry。
4. 每次迁移都保持 `cargo fmt`、相关单元测试和 CLI integration 通过；迁移期间允许保留 compatibility re-export，但不得新增 Build 对 toolkit 的直接调度依赖。

## Phase 0 — 文档与术语收敛

- [x] 在 Design / Implementation 文档中明确 generator-first 模型。
- [x] 明确 `codegen` / `exporter` 名称作为长期工具箱名称不够精确，目标术语为 `code artifact toolkit` / `data artifact toolkit`。
- [x] 创建本 TODO，记录迁移步骤。

## Phase 1 — 拆分 `src/generator/mod.rs`

- [x] 将 `GeneratorRequirements` 移到 `src/generator/requirements.rs`。
- [x] 将 `GeneratorCommonInput`、`GeneratorDataInput`、`GeneratorTaskInput`、`PreparedGeneratorInputs` 等输入模型移到 `src/generator/input.rs`。
- [x] 将 `GeneratorRegistry` 和 builtin registration 移到 `src/generator/registry.rs`。
- [x] 将迁移期 `CodeArtifactGenerator`、`DataArtifactGenerator` adapter 移到 `src/generator/adapter.rs`。
- [x] 将 `ValidationReportGenerator` 移到 `src/generator/validation_report.rs`。后续若需要更多文件，可再扩展为 `src/generator/validation_report/` 目录。
- [x] 保持 `src/generator/mod.rs` 只做模块声明与必要 re-export。

## Phase 2 — 先迁移 direct generators

- [x] 将 `src/schema_doc/` 迁入 `src/generator/schema_doc.rs`，保留必要 compatibility re-export 或更新引用。当前 schema doc 体量仍为单文件；后续如果继续膨胀，可再扩展成 `src/generator/schema_doc/` 目录。
- [x] 将 `validation_report` 作为独立 concrete generator 注册，而不是内联在 registry/facade 文件里。已在 Phase 1 完成，保留在本阶段作为 direct generator 迁移复核项。
- [x] 更新 `GeneratorRegistry::with_builtin_generators()`，直接注册 `schema_doc` / `validation_report` concrete generator。
- [x] 更新 Schema Doc 设计文档与相关测试路径引用。

## Phase 3 — 迁移 aggregate / mixed data-backed generators

- [x] 将 `src/exporter/lua_table.rs` 迁入 `src/generator/lua_table.rs`，直接实现 `ArtifactGenerator`；继续复用 `CodeArtifactOptions` 和 data artifact payload helper。当前 Lua 体量较大但仍为单文件；后续可再扩展成 `src/generator/lua_table/` 目录。
- [x] 将 `src/exporter/asset_manifest.rs` 迁入 `src/generator/asset_manifest.rs`，直接实现 `ArtifactGenerator`。
- [x] 将 `src/exporter/gettext_pot.rs` 迁入 `src/generator/gettext_pot.rs`，直接实现 `ArtifactGenerator`。
- [x] 从 `DataFormatRegistry` 中移除这些 aggregate generator 的 build-facing 注册职责；如需共享逻辑，保留在 data artifact toolkit helper 中。
- [x] 更新 `08-exporter-module-detail-design.md`、`09i-lua-embedded-data-generator.md`、Asset Manifest / POT 相关设计文档。

## Phase 4 — 迁移 language code-like generators

- [ ] 将 `src/codegen/rust/` 迁入 `src/generator/rust/`。
- [ ] 将 `src/codegen/cpp/` 迁入 `src/generator/cpp/`。
- [ ] 将 `src/codegen/cpp_unreal/` 迁入 `src/generator/cpp_unreal/`。
- [ ] 将 `src/codegen/typescript/` 迁入 `src/generator/typescript/`。
- [ ] 将 `src/codegen/csharp/` 迁入 `src/generator/csharp/`。
- [ ] 将 `src/codegen/java/` 迁入 `src/generator/java/`。
- [ ] 让每个 language generator 直接实现 `ArtifactGenerator`，或只保留很薄的 local adapter；逐步删除 `codegen::builtin_generators()` 作为 Build-facing 注册入口。
- [ ] 将语言无关 `CodeModel` / `CodeWriter` / options 留在 code artifact toolkit，并禁止语言专属逻辑回流。
- [ ] 更新 codegen capability matrix 中的 source module 路径。

## Phase 5 — 迁移 runtime table data generators

- [ ] 将 `src/exporter/json.rs` 迁入 `src/generator/json/`。
- [ ] 将 `src/exporter/messagepack.rs` 迁入 `src/generator/messagepack/`。
- [ ] 将 `src/exporter/binary.rs` 迁入 `src/generator/binary/`。
- [ ] 抽出 runtime table iteration / path planning / `RuntimeDataFile` helper，留在 data artifact toolkit。
- [ ] 评估 `DataFormatExporter` 是否仍有价值；若只剩内部 helper，可改名为 `RuntimeTableFormat` 或删除。
- [ ] 删除或降级 `DataFormatRegistry` 的 Build-facing 职责，`GeneratorRegistry` 直接注册 concrete runtime data generators。

## Phase 6 — 工具箱命名落地

- [ ] 评估是否把 `src/codegen/` 物理重命名为 `src/code_artifact/`；如果改名，提供一次性大迁移并更新全部文档、模块路径和 `use`。
- [ ] 评估是否把 `src/exporter/` 物理重命名为 `src/data_artifact/`；如果改名，提供一次性大迁移并更新全部文档、模块路径和 `use`。
- [ ] 如果短期不做物理改名，至少将模块级注释和文档统一表述为 “current codegen/exporter module, target code/data artifact toolkit”。
- [ ] 清理 glossary 中迁移期 legacy registry / trait 术语，保留最终术语。

## Phase 7 — Build 依赖收敛检查

- [ ] 确认 `src/build/` 不再直接依赖具体 `codegen` / `exporter` 类型；只依赖 `generator`、`artifact`、`runtime_contract` 和必要输入模型。
- [ ] 确认 `project` 不引用 toolkit 内部 task config 类型，只保留统一 `TargetExportConfig`。
- [ ] 确认 `runtime_contract` 不依赖 generator/toolkit 上层模块。
- [ ] 补充模块依赖检查或 grep-based regression，防止 Build 新增按 code/data toolkit 分类分发。

## Completion criteria

- [ ] 所有 concrete export type 均位于 `src/generator/<type_id>/` 或有明确保留理由。
- [ ] `build` 只通过 `GeneratorRegistry` 查询 requirements 与执行 generator。
- [ ] `code artifact toolkit` / `data artifact toolkit` 不再承担 concrete generator 归属职责。
- [ ] 文档中的 source layout、glossary、Design、Implementation 与实际代码路径一致。
- [ ] `cargo fmt`、`cargo test`、相关 CLI integration 和 runtime compliance 测试通过。
