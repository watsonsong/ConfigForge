# 工程架构整理 TODO

本文承接当前工程整体 architecture review。原 `Documentation/Todo/diagnostic-meta-dependency-decoupling-todo.md` 与 `Documentation/Todo/diagnostic-schema-doc-architecture-review-todo.md` 中的专题均已完成或已被后续专题吸收，因此删除旧 TODO，并用本文统一追踪后续架构清理工作。

## 当前整体判断

ConfigForge 当前主干方向整体正确：

- Build 已基本按 `GeneratorRegistry` / `GeneratorRequirements` 调度，`MetaOnly` / `MetaAndData` 是输入需求派生出的调度属性，而不是继承体系。
- `schema_doc`、`validation_report` 已作为 direct `ArtifactGenerator` 接入。
- `DiagnosticReport<Diagnostic>`、stable diagnostic code catalog、`ValidationReport` / `ValidationOutcome` 已把诊断和 validation report 主路径收敛到更统一的模型。
- `ArtifactSink` 已成为生产路径的统一写出 authority。

本轮复核仍发现以下架构债务，需要分批清理：

1. **模块边界仍有少量侵蚀**：`schema_doc` 直接依赖 `project::lexical_normalize_path`，Build 对 `validation_report` 失败路径仍有具体 `type_id` 特判。
2. **依赖关系还可简化**：`GeneratorCommonInput` 同时承载 build-facing common context、data artifact adapter 和 code artifact adapter，`generator` facade 偏宽。
3. **术语仍不一致**：`Documentation/code-style.md` 和部分 Design / Implementation 文档仍把旧 `exporter` / `codegen` 当成现行 build-facing 分类。
4. **相同功能存在不同做法**：路径 normalize / root containment、generator option parsing、language generator glue、runtime reader safe join、relation missing-target 语义存在重复或漂移。
5. **单文件过大**：`asset.rs`、`artifact/mod.rs`、`build/mod.rs`、`validation/report.rs`、若干 language generator 文件超过或接近代码风格阈值。
6. **文档与实现局部不一致**：`validation_report` / `schema_doc` 的未实现 option 在设计文档中仍容易被误读为可配置；Design 配置文档的已支持 generator type 清单落后于源码注册表。

## 重点发现

### 模块边界 / 依赖方向

- `schema_doc` 当前通过 `crate::project::lexical_normalize_path` 复用 Project 内部路径 helper，见 `src/generator/schema_doc/format.rs:6`、`src/generator/schema_doc/format.rs:128`。这与 `Documentation/Implementation/01-module-outline.md` 中 `schema_doc` 不依赖 `project` 的边界不一致。
- Build 失败 fallback 对 `validation_report` 有具体 type 特判，见 `src/build/runtime.rs:187`、`src/build/runtime.rs:198`、`src/build/runtime.rs:207`。短期合理，但长期会诱导更多 generator-specific 分支进入 Build。
- `GeneratorCommonInput` 同时依赖 `code_artifact`、`data_artifact`、`data`、`validation`，并提供 `target_data_config()` / `code_export_input()`，见 `src/generator/input.rs:8`、`src/generator/input.rs:48`、`src/generator/input.rs:59`。这让 generator facade 同时承担调度输入和 toolkit adapter。
- crate root 暴露了大量 toolkit / concrete generator / validation helper，见 `src/lib.rs:33`、`src/lib.rs:45`、`src/lib.rs:53`、`src/lib.rs:73`，与最小 public API 原则存在张力。
- Build 测试直接依赖 `code_artifact` / `data_artifact` helper，见 `src/build/mod.rs:867`、`src/build/mod.rs:917`；production guard 已存在，见 `src/build/mod.rs:887`，但测试示范仍可能弱化边界约束。

### 重复实现 / 相同功能不同做法

- `lexical_normalize_path` 分散在 `project`、`data`、`artifact`，见 `src/project/mod.rs:203`、`src/data/loader.rs:177`，`artifact` 中也有同类逻辑。
- `filesystem_path_is_within_root` / filesystem-aware key 在 Data Loader 与 Artifact Writer 中各自实现，平台大小写、symlink、alias 策略存在继续漂移风险，见 `src/data/loader.rs:156`。
- `schema_doc` 与 `validation_report` option parser 都重复执行 clone/remove options、parse string、unknown key diagnostic，见 `src/generator/schema_doc/format.rs:204`、`src/generator/validation_report/config.rs:20`。
- MetaOnly 和 runtime 分支分别构造 `GeneratorCommonInput`，后续新增字段时易漏同步，见 `src/build/mod.rs:810`、`src/build/runtime.rs:502`。
- Rust / TypeScript / C++ reader、repository relation、names/type mapping 有大量同构逻辑；Unreal C++ relation missing target 行为与 Rust / TypeScript / C++ 不完全一致。

### 文件规模

按 `Documentation/code-style.md` 的阈值，非测试文件超过约 800 行应额外审查，超过约 1000 行通常应考虑拆分。当前优先关注：

- `src/asset.rs`：资源解析 / 策略 / 诊断集中在一个文件。
- `src/artifact/mod.rs`：writer、sink、path conflict、temp commit、测试集中。
- `src/build/mod.rs`：pipeline、trace、MetaOnly branch、runtime bridge、CLI report task 注入、测试集中。
- `src/validation/report.rs`：report model、enrichment、grouping、HTML / Markdown / CSV / JSON renderer 集中。
- `src/generator/java/mod.rs`、`src/generator/csharp/mod.rs`、`src/generator/cpp/repository.rs` 等 language generator 文件承担过多模板和 glue 职责。

### 文档 / 术语一致性

- `Documentation/code-style.md` 仍把 `exporter` / `codegen` 作为模块规则中的现行模块名，和当前 `generator` + `data_artifact` + `code_artifact` 结构不一致。
- `Documentation/Design/03-configuration.md` 中 `type` 仍称为 `exporter ID`，且内置 type 清单缺少 `asset_manifest`、`schema_doc`、`validation_report`、`cpp_unreal`、`csharp`、`java` 等现行注册项。
- `Documentation/Design/06-export-cli.md` 在辅助产物章节中容易把 `schema_doc` 混入 “非运行时 data exporter”，但源码中 `schema_doc` 是 MetaOnly doc artifact generator。
- `Documentation/Design/10-designer-validation-report.md` 将 `include_warnings` / `include_errors` / `group_by` 放在字段表中，但源码当前只接受 `format`、`file_name`、`title`；这些字段设置后会 unknown option。
- `Documentation/Design/11-schema-doc-generator.md` 中 `file_name_style` 出现在字段表，但源码当前只接受 `format`、`index_file`、`title`。

## 分步 TODO

### [x] Step 1：统一文档术语与当前 generator 架构

目的：先消除团队沟通中的旧模型噪音，确保后续清理以同一套术语推进。

实现项：

- [x] 更新 `Documentation/code-style.md`，将模块规则中的 `exporter` / `codegen` 改为 `generator` / `data_artifact` / `code_artifact`，并调整 commit scope 示例。
- [x] 更新 `Documentation/Design/03-configuration.md`，把 `exporter ID` 改为 `generator type ID`，并同步当前内置 generator type 清单或改为引用权威矩阵。
- [x] 更新 `Documentation/Design/06-export-cli.md`，将 “非运行时 data exporter” 拆成 data-backed auxiliary generator 与 MetaOnly doc generator，避免把 `schema_doc` 描述成 data exporter。
- [x] 更新 `Documentation/Design/10-designer-validation-report.md`，把 `include_warnings` / `include_errors` / `group_by` 移到 reserved / future options，并明确当前设置会报 unknown option。
- [x] 更新 `Documentation/Design/11-schema-doc-generator.md`，把 `file_name_style` 标为未开放 reserved option 或从当前字段表移除。
- [x] 更新 `Documentation/Implementation/03-project-module-detail-design.md`、`07-artifact-module-detail-design.md`、`09f-codegen-core-model-and-options.md`、`09i-lua-embedded-data-generator.md` 中旧 `exporter/codegen` build-facing 分类描述。

验证：

- [x] `cargo test cli`
- [x] 文档 grep：`exporter ID`、`exporter 专属字段`、`非运行时 data exporter` 不再用于描述现行 build-facing generator task。

可独立提交建议：

- `docs(architecture): align generator terminology`

### [x] Step 2：下沉并统一路径规范化 helper

目的：消除 Project / Data / Artifact / SchemaDoc 对路径 normalize 的重复实现，并修复 `schema_doc -> project` 边界侵蚀。

实现项：

- [x] 新增或选择一个底层 path helper 归属，例如 `src/path_util.rs` 或 `artifact` 下的 path 子模块；该模块不能依赖 `project`、`data`、`generator`、`build`。
- [x] 将 `lexical_normalize_path` 从 `project`、`data::loader`、`artifact` 重复实现迁入共享 helper。
- [x] 将 `schema_doc` 从 `crate::project::lexical_normalize_path` 改为依赖共享 helper。
- [x] 梳理 `filesystem_path_is_within_root`、filesystem-aware key、case-fold conflict key 的平台策略，至少保证 Data Loader 与 Artifact Writer 文档化差异；能统一则统一。
- [x] 增加边界回归测试：`src/generator/schema_doc/**/*.rs` 不应包含 `crate::project`。

验证：

- [x] `cargo test artifact`
- [x] `cargo test data`
- [x] `cargo test schema_doc`

可独立提交建议：

- `refactor(path): share lexical path normalization`

### [x] Step 3：抽取 generator option parser 与 common input 构造

目的：减少 schema_doc / validation_report / code artifact option parsing 重复，并避免 Build 分支分别构造 common input 后漂移。

实现项：

- [x] 抽取轻量 `GeneratorOptionParser` 或等价 helper，支持 `take_string`、typed parse、unknown option 检查、统一 diagnostic source/current/expected。
- [x] 迁移 `src/generator/schema_doc/format.rs` 与 `src/generator/validation_report/config.rs` 的 option parsing。
- [x] 评估 `src/code_artifact/mod.rs` 中 code artifact option parsing 是否可以复用同一 helper。
- [x] 在 build 层抽取 `GeneratorCommonInput` 构造 helper，替换 `src/build/mod.rs` 与 `src/build/runtime.rs` 的重复构造函数。
- [x] 保持 CLI `--report` 注入 task 与 Project Config 显式 task 的错误 code/source 稳定。

验证：

- [x] `cargo test generator`
- [x] `cargo test schema_doc`
- [x] `cargo test validation_report`
- [x] `cargo test build`

可独立提交建议：

- `refactor(generator): share option and input helpers`

### [x] Step 4：收敛 validation_report 失败路径到 registry 输入模型

目的：减少 Build 对具体 generator type 的特判，让失败路径也尽量通过统一 `ArtifactGenerator` facade。

实现项：

- [x] 设计 `PreparedGeneratorInputs::ValidationReportOnly` 或等价 input，表达 validation error 后可用 `ValidationReport` 但不可用 validated export `DataSet` 的状态。
- [x] 让 `validation_report` generator 支持该 input；Build 不再直接调用 `export_validation_report_to_sink()`。
- [x] 将 `export_validation_report_to_sink` 降为 `generator/validation_report` 私有 helper，移除 `src/generator/mod.rs` 的 re-export。
- [x] Build 仍可筛选“失败时允许写出”的 tasks，但应基于 requirements/capability，而不是硬编码 `type_id == "validation_report"`；如短期保留硬编码，需文档化并增加防扩散测试。
- [x] 保持 validation error 继续阻断其他 data-backed generator。

验证：

- [x] `cargo test build`
- [x] `cargo test --test cli_integration validation`
- [x] `cargo test validation_report`

可独立提交建议：

- `refactor(build): route validation report fallback through generator input`

### [x] Step 5：收窄 public facade 与测试越界依赖

目的：降低 crate public API 面积，避免外部和测试代码依赖内部 toolkit 细节。

实现项：

- [x] 盘点 `src/lib.rs` public re-export，按 “外部稳定 API / crate 内部 / 测试辅助 / 迁移期兼容” 分类。
- [x] 优先收窄 concrete exporter structs、renderer helper、table-level validation helper、toolkit facet 中不需要对外承诺的 re-export。
- [x] 将 Build 中直接测试 `code_artifact::default_module_name` 的用例迁到 code artifact 或 language generator 测试。
- [x] 将 Build 中直接测试 `data_artifact::build_exported_table_payload` 的用例迁到 data artifact 测试。
- [x] 保留或增强 Build production guard，确保 production `src/build/*.rs` 不直接调度 `code_artifact` / `data_artifact` toolkit。

验证：

- [x] `cargo test build`
- [x] `cargo test code_artifact`
- [x] `cargo test data_artifact`
- [x] `cargo test`

可独立提交建议：

- `refactor(api): narrow public facade boundaries`

### [ ] Step 6：拆分高风险大文件（不改行为）

目的：降低单文件维护复杂度，先做 mechanical split，避免与功能修改混杂。

实现项：

- [ ] 拆分 `src/validation/report.rs` 为 model / enrich / group / render/html / render/markdown / render/csv 等子模块。
- [ ] 拆分 `src/build/mod.rs` 的 trace、pipeline、MetaOnly branch、CLI validation report task 注入、时间 helper。
- [ ] 拆分 `src/artifact/mod.rs` 的 writer/sink、path conflict、temp commit、diagnostics。
- [ ] 拆分 `src/asset.rs` 的 resolver、policy、path candidates、diagnostics。
- [ ] 每次只拆一个文件，保持 public facade 不变或仅做 crate 内重导出。

验证：

- [ ] 每个拆分批次运行对应 `cargo test <module>`
- [ ] 拆完全部后运行 `cargo test`

可独立提交建议：

- `refactor(validation): split report rendering modules`
- `refactor(build): split pipeline helpers`
- `refactor(artifact): split writer path logic`

### [ ] Step 7：抽取多语言 generator 共享 plan

目的：降低 Rust / TypeScript / C++ / Unreal / C# / Java generator 的重复逻辑，并统一 relation / reader 语义。

实现项：

- [ ] 抽取语言无关 `ReaderShapePlan` / `ReaderFieldPlan`，先覆盖 Rust / TypeScript / C++ reader 中 optional slot、enum、struct、fragment、row reader 的共同结构。
- [ ] 抽取 naming / type mapping 的 shared plan，语言层只处理 casing、keyword、type syntax 差异。
- [ ] 抽取 relation plan，统一 required / optional key、partial empty、many relation、missing target 策略。
- [ ] 明确 Unreal C++ relation missing target 行为是否应与 Rust / TypeScript / C++ 一致；若保留差异，文档和测试必须说明原因。
- [ ] 抽取 language generator `ArtifactGenerator` glue，减少新增语言时的样板代码。

验证：

- [ ] `cargo test generator::rust`
- [ ] `cargo test generator::typescript`
- [ ] `cargo test generator::cpp`
- [ ] `cargo test generator::cpp_unreal`
- [ ] `cargo test generator::csharp`
- [ ] `cargo test generator::java`
- [ ] 相关 runtime compliance tests

可独立提交建议：

- `refactor(generator): share reader shape planning`
- `refactor(generator): share relation planning`

### [ ] Step 8：统一 runtime reader 路径与加载策略

目的：让 JSON / MessagePack / binary runtime reader 对 logical path、root escape、file size 等基础行为一致。

实现项：

- [ ] 比较 `runtime/rust/serde_json_reader`、`serde_msgpack_reader`、`binary_reader` 中 `open_reader` / `safe_join` / `check_file_size` 行为。
- [ ] 抽取 shared runtime loader/path helper 到 `reader_core` 或独立共享模块。
- [ ] 明确是否允许不逃逸 root 的 `a/../b` logical path；三种 reader 必须一致或文档化差异。
- [ ] 补充 runtime compliance 覆盖 path traversal、non-escaping parent dir、file size limit。

验证：

- [ ] `cargo test -p config-forge-runtime-reader-core`
- [ ] `cargo test runtime_compliance`
- [ ] `cargo test csharp_java_runtime_compliance`
- [ ] `cargo test typescript_runtime_compliance`

可独立提交建议：

- `refactor(runtime): share reader path validation`

### [ ] Step 9：补强模块边界自动化回归

目的：把 architecture review 结论固化为测试，防止后续继续扩大依赖面。

实现项：

- [ ] 增加 diagnostic 边界 guard：`src/diagnostic/**/*.rs` 不依赖 meta/data/validation/generator/build。
- [ ] 增加 validation 边界 guard：`src/validation/**/*.rs` 不依赖 code_artifact/data_artifact/generator/build。
- [ ] 增加 code_artifact 边界 guard：`src/code_artifact/**/*.rs` 不依赖 data/data_artifact/validation/generator/build。
- [ ] 增加 data_artifact production guard：不依赖 validation/generator/build。
- [ ] 增加 schema_doc guard：不依赖 data/validation/code_artifact/data_artifact/build/project。
- [ ] 将边界规则与 `Documentation/Implementation/01-module-outline.md` 保持同步。

验证：

- [ ] `cargo test architecture`
- [ ] `cargo test diagnostic`
- [ ] `cargo test validation`

可独立提交建议：

- `test(architecture): guard module dependency boundaries`

## 非目标

- 不在本 TODO 中一次性重写 Build 调度为通用 DAG executor；当前阶段式 pipeline 仍是当前实现权威。
- 不改变 Project Config 用户 schema；本文中的 option parser 和 terminology 清理必须保持现有 TOML 兼容。
- 不改变 validation error 阻断 data-backed export 的产品语义。
- 不一次性重构所有语言 generator；共享 plan 应按语言能力逐步接入。
- 不把 DevLogs 改写成当前设计；只更新 Design / Implementation / Tech 中仍作为当前合同的文档。

## 完成后期望状态

- 文档统一使用 `artifact generator`、`generator type ID`、`data artifact toolkit`、`code artifact toolkit` 等当前术语；旧 `exporter/codegen` 只在历史文件名或明确历史上下文中出现。
- 底层路径 normalize / root containment 策略有单一实现或清晰分层，`schema_doc` 不再依赖 `project`。
- Build 不再扩散具体 generator type 的特判；validation report 失败路径有清晰 capability / input model。
- `generator` facade 更窄，toolkit adapter 逻辑回到具体 generator 或 toolkit 内部。
- crate public API 与测试依赖更接近最小公开原则。
- 高风险大文件被拆成职责清晰的子模块，单文件规模回到可审查范围。
- 多语言 generator 和 runtime reader 的共同语义由 shared plan/helper 承载，差异被显式测试和文档化。
