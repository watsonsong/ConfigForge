# Generator-first module migration TODO

目标：把 build-facing 产物归属统一到 `src/generator/<type_id>/`，让当前 `src/code_artifact/` 和 `src/data_artifact/` 从“具体 generator 归属地”收敛为可复用工具箱。目标命名为：

- `code artifact toolkit`：物理路径 `src/code_artifact/`；提供 CodeModel IR、CodeWriter、naming / formatting / keyword conflict、`CodeArtifactOptions`、runtime reader contract helper。
- `data artifact toolkit`：物理路径 `src/data_artifact/`；提供 target-aware payload、`ExportedTablePayload` / `ExportedValue`、runtime table serialization、binary layout、legacy exporter adapter。

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
- [x] 将 `ValidationReportGenerator` 移到 `src/generator/validation_report/` 目录，便于后续扩展更多 report format。
- [x] 保持 `src/generator/mod.rs` 只做模块声明与必要 re-export。

## Phase 2 — 先迁移 direct generators

- [x] 将 `src/schema_doc/` 迁入 `src/generator/schema_doc/`，保留必要 compatibility re-export 或更新引用；当前已拆分为 model / build / format / markdown / tests 子模块。
- [x] 将 `validation_report` 作为独立 concrete generator 注册，而不是内联在 registry/facade 文件里。已在 Phase 1 完成，保留在本阶段作为 direct generator 迁移复核项。
- [x] 更新 `GeneratorRegistry::with_builtin_generators()`，直接注册 `schema_doc` / `validation_report` concrete generator。
- [x] 更新 Schema Doc 设计文档与相关测试路径引用。

## Phase 3 — 迁移 aggregate / mixed data-backed generators

- [x] 将 `src/data_artifact/lua_table.rs` 迁入 `src/generator/lua/`，直接实现 `ArtifactGenerator`；继续复用 `CodeArtifactOptions` 和 data artifact payload helper；当前已拆分为 naming / options / render / tests 子模块。
- [x] 将 `src/data_artifact/asset_manifest.rs` 迁入 `src/generator/asset_manifest/`，直接实现 `ArtifactGenerator`。
- [x] 将 `src/data_artifact/gettext_pot.rs` 迁入 `src/generator/gettext_pot/`，直接实现 `ArtifactGenerator`。
- [x] 从 `DataFormatRegistry` 中移除这些 aggregate generator 的 build-facing 注册职责；如需共享逻辑，保留在 data artifact toolkit helper 中。
- [x] 更新 `08-exporter-module-detail-design.md`、`09i-lua-embedded-data-generator.md`、Asset Manifest / POT 相关设计文档。

## Phase 4 — 迁移 language code-like generators

- [x] 将 `src/code_artifact/rust/` 迁入 `src/generator/rust/`。
- [x] 将 `src/code_artifact/cpp/` 迁入 `src/generator/cpp/`。
- [x] 将 `src/code_artifact/cpp_unreal/` 迁入 `src/generator/cpp_unreal/`。
- [x] 将 `src/code_artifact/typescript/` 迁入 `src/generator/typescript/`。
- [x] 将 `src/code_artifact/csharp/` 迁入 `src/generator/csharp/`。
- [x] 将 `src/code_artifact/java/` 迁入 `src/generator/java/`。
- [x] 让 language generator 由 `GeneratorRegistry` 直接集中注册；迁移期保留 crate-internal `CodeGenerator` + `CodeArtifactGenerator` 薄 adapter，但删除 `codegen::builtin_generators()` 作为 Build-facing 注册入口。
- [x] 将语言无关 `CodeModel` / `CodeWriter` / options 留在 code artifact toolkit，并禁止语言专属逻辑回流。
- [x] 更新 codegen capability matrix 中的 source module 路径。

## Phase 5 — 迁移 runtime table data generators

- [x] 将 `src/data_artifact/json.rs` 迁入 `src/generator/json.rs`。
- [x] 将 `src/data_artifact/messagepack.rs` 迁入 `src/generator/messagepack.rs`。
- [x] 将 `src/data_artifact/binary.rs` 迁入 `src/generator/binary/`。
- [x] 抽出 runtime table iteration / path planning / `RuntimeDataFile` helper，留在 data artifact toolkit。
- [ ] 评估 `DataFormatExporter` 是否仍有价值；若只剩内部 helper，可改名为 `RuntimeTableFormat` 或删除。
- [x] 删除或降级 `DataFormatRegistry` 的 Build-facing 职责，`GeneratorRegistry` 直接注册 concrete runtime data generators。

## Phase 6 — 工具箱命名落地

- [x] 评估并将 `src/codegen/` 物理重命名为 `src/code_artifact/`，更新模块路径和 `use`。
- [x] 评估并将 `src/exporter/` 物理重命名为 `src/data_artifact/`，更新模块路径和 `use`。
- [x] 同步文档中的 source layout / toolkit 路径表述，不再描述为短期 legacy 物理路径。
- [x] 清理 glossary 中迁移期 legacy registry / trait 术语，保留最终术语。

## Phase 7 — Build 依赖收敛检查

- [x] 确认 `src/build/` production code 不再直接依赖具体 code/data artifact toolkit 类型；只依赖 `generator`、`artifact`、`runtime_contract` 和必要输入模型。测试代码可继续构造 toolkit facet stub。
- [x] 确认 `project` 不引用 toolkit 内部 task config 类型，只保留统一 `TargetExportConfig`。
- [x] 确认 `runtime_contract` 不依赖 generator/toolkit 上层模块。
- [x] 补充 grep-based regression，防止 Build production code 新增按 code/data toolkit 分类分发。

## Phase 8 — Registry / adapter 收口

目标：让 `GeneratorRegistry` 的 production 构造只接受 `Vec<Box<dyn ArtifactGenerator>>`，把 `DataFormatExporter` / `AggregateDataExporter` / `CodeGenerator` 退回 toolkit compatibility facet，不再作为 build-facing registration shape。

- [x] 内置 data-backed generators 直接注册 concrete `ArtifactGenerator`：JSON / MessagePack / bin / POT / Lua / Asset Manifest 不再通过 `DataArtifactGenerator` adapter 进入 builtin registry。
- [x] `GeneratorRegistry::with_builtin_generators()` 改为构造单一 `Vec<Box<dyn ArtifactGenerator>>`；不再把内置 generator 分成 `table_exporters` / `aggregate_exporters` / `code_generators` / `direct_generators` 四类。
- [x] 将多输入 registry constructor 降级为 `#[cfg(test)] from_compat_parts()`，只服务迁移期 data exporter compatibility facet 覆盖，不作为 production API 使用。
- [x] 将内置 language generators 从 `CodeGenerator` adapter 迁为 direct `ArtifactGenerator`；Rust / TypeScript / C++ / C# / Java / CppUnreal 均由 concrete language module 自己声明 `type_id` / alias / requirements，并继续复用 `code_artifact` toolkit 中的 renderer / naming / formatting / CodeModel。
- [x] 删除 `CodeArtifactGenerator` adapter；`CodeGenerator` 不再提供 build-facing alias / requirements 包装入口，alias 与 requirements 由 concrete language `ArtifactGenerator` 提供。
- [ ] 评估并改名或删除 `DataArtifactGenerator` adapter：若 external compatibility 仍需要把 `DataFormatExporter` / `AggregateDataExporter` 包装成 generator，应迁到明确命名的 compatibility module，并在文档中标注“不用于内置 generator 注册”；若没有外部插件入口，则删除 adapter。
- [ ] 评估 `DataFormatExporter` 名称是否仍会误导为 build-facing exporter registry；若继续保留，应改名为 `RuntimeTableFormat` / `RuntimeTableSerializer` 这类 table serialization facet，并只暴露 plan / serialize helper。
- [ ] 完成后删除 `from_compat_parts()`，测试改用直接的 stub `ArtifactGenerator` 或明确的 compatibility adapter 测试；grep 应确保 production code 中不存在 `GeneratorRegistry::from_compat_parts` / `DataArtifactGenerator` 注册路径。
- [ ] 同步更新 `08-exporter-module-detail-design.md`、`09-codegen-module-detail-design.md`、`09h-codegen-generator-api-and-tests.md` 和 `10-build-module-detail-design.md`，确保它们描述的是单一 build-facing generator 模型，而非四类 registry 输入。

## Completion criteria

- [ ] 所有 concrete export type 均位于 `src/generator/<type_id>/` 或有明确保留理由。
- [x] `build` 只通过 `GeneratorRegistry` 查询 requirements 与执行 generator。
- [x] `code artifact toolkit` / `data artifact toolkit` 不再承担 concrete generator 归属职责。
- [ ] `GeneratorRegistry` production API 只接受 direct `ArtifactGenerator`；adapter / compatibility constructor 不参与内置注册路径。
- [ ] 文档中的 source layout、glossary、Design、Implementation 与实际代码路径一致。
- [ ] `cargo fmt`、`cargo test`、相关 CLI integration 和 runtime compliance 测试通过。
