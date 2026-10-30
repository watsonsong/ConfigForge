# Phase 0007：产物规划修复与架构简化评估

## 时间与范围

- 日期：2026-10-30。
- 评估基线：`d844162 fix(build): plan all target artifacts before generation`。
- 范围：build 编排、generator 接口、data provider、validation、代码与数据产物 toolkit、ArtifactSink、CI 和设计文档。
- 本记录是源码与文档审查快照。下述简化建议尚未实施，不代表新的产品契约，也不替代 `Design/`、`Implementation/`、`Tech/`。
- 本轮先提交已有产物规划修复，再进行评估；未执行后续实现重构，也未 push。

## 总体判断

当前主分层合理：Project / Meta / Data / Validation / Generator / Artifact 的职责有明确边界，build 通过 generator facade 编排，data artifact 与 code artifact toolkit 提供共享能力。没有发现需要整体重写架构的依据。

最值得减少的是三类维护负担：迁移后并存的执行接口、没有进入当前生产调用链的扩展骨架、多个文档重复描述同一实现。简化应优先收敛内部实现，保留用户配置、公共 API 的兼容性、target 可见性、诊断顺序和 streaming 写出能力。

本评估没有进行性能基准测试。下面的建议以减少重复分配、计算和维护分支为依据；实际重构仍需用输出一致性、并发行为和必要的性能测量验证，不能直接承诺性能提升数值。

## 建议与依据

### 1. 收敛 generator 的输入与执行入口

优先级：高。收益：高。风险：中。

依据：

- [`src/generator/artifact_generator.rs:25`](../../src/generator/artifact_generator.rs#L25) 同时维护 `generate_to_sink`、`generate_with_data_to_sink`、`generate_with_validation_report_to_sink` 和统一分派 `generate`。Data-backed generator 还必须实现一个只返回不支持错误的 Meta-only 入口，例如 [`json/mod.rs:64`](../../src/generator/json/mod.rs#L64)。
- `plan_task_artifacts`（同文件第 81 行）在当前实现中只有默认转发，没有具体 generator override；它最终只使用 `GeneratorCommonInput`，但调用方仍需构造 `Planning` 输入状态。
- [`src/generator/input.rs:25`](../../src/generator/input.rs#L25) 的 common input 拥有 options、paths、strings 等字段；分派时再次 clone common，再转成 `TargetDataConfig` / `CodegenTaskConfig`，带来重复适配和分配。
- [`src/generator/requirements.rs:11`](../../src/generator/requirements.rs#L11) 的 `needs_meta` 当前构造值恒为 true，`needs_assets` 也没有被生产调度代码消费；真正影响分支的主要是 `needs_data`，`needs_validation_report` 还参与 validation 失败后的报告导出。

建议：保留一个基于 common input 的规划入口，以及一个使用借用输入的执行入口；通过小型输入 enum 表达 Meta-only、带 Data、仅报告三种有效执行状态。common 尽量借用已有上下文。把当前没有调度作用的 flags 从能力模型中收回，或明确其仅为声明信息；报告输入依赖与“validation 失败仍可导出”的策略要分清。

保持：type ID / alias、所有 generator 类型、validation 失败时报告仍能导出、Meta-only 不等待 data。不要为消除旧 trait 再引入一套更大的 trait 继承体系。

### 2. 合并 CSV 读取实现，推迟未接入的远端执行框架

优先级：高。收益：高。风险：中，涉及已有 public API。

依据：

- [`src/data/loader.rs:94`](../../src/data/loader.rs#L94) 与 [`src/data/provider.rs:150`](../../src/data/provider.rs#L150) 都按 provider / layout 分派到 CSV reader。
- 两者已经出现行为差异：DataLoader 经 `resolve_source_path` 执行 root containment / 文件系统路径检查，LocalCsvSourceProvider 直接 join 路径读取。复用必须以保留路径检查的一套为准。
- build 实际走 `DataLoader::read_sources_with_pool`。`AsyncSourceReadShell` 没有接入 build；其资源 lane 测试通过，不能说明 CLI 主流程使用了它。
- [`src/data/provider.rs:173`](../../src/data/provider.rs#L173) 的 shell 使用 `thread::scope`，对每个获准启动的 source 创建线程，并非异步 runtime。`RemoteProviderPolicy` 目前只有类型和默认值，没有实际 retry / backoff / rate-limit 消费者。
- `io_concurrency` / `network_concurrency` 当前被解析、保存、校验和输出，但没有约束生产 build 的读取或写出调度；本地读取主要受 `jobs` 控制。

建议：共用一个带路径检查的 source read 实现，继续使用已有 Rayon pool 处理本地 CSV。public provider / shell 入口先作为兼容 wrapper 保留；远端 provider、retry 和 async runtime 等到真实接入时再实现。现有 CLI 参数的保留状态与实际作用应准确文档化，不能静默删除或声称已生效。

保持：source merge 顺序、source layout、路径检查、`--jobs` / `--serial` 行为和 public API 兼容性。不要为了统一接口，把高效的本地 CSV 路径强行改为 Tokio。

### 3. Validation 的多个外部入口共用一个内部流程

优先级：高。收益：中到高。风险：中。

依据：[`src/validation/entry.rs:56`](../../src/validation/entry.rs#L56) 和第 82 行的 full validation 表遍历基本重复；第 153 / 178 行的两个 finish 函数重复 FK、validator plan、context、稳定排序与结果转换，主要差异是 asset validation。target-aware 路径已有部分共用 finish 实现，说明无需重做 validation 模型。

建议：用私有 options/context 表达可选 target、assets 与执行方式，让现有 public 函数成为薄 wrapper，复用表遍历与 finish 核心。优先统一 full validation 中已经确认的重复，再决定是否需要更广的入口收敛。

保持：normalize 与 validation 的边界、full / target 语义差异、assets 校验、warning / error gate、缺表诊断、稳定顺序和现有并行粒度。不能把 `ValidationReport` 简化成第一个错误即返回。

### 4. 单 target 编排使用单 target 计划

优先级：中。收益：中。风险：低。

依据：[`src/build/plan.rs:14`](../../src/build/plan.rs#L14) 的 `BuildDependencyPlan` 使用 `Vec<TargetBuildPlan>`，但唯一构造函数永远放入一个 target；随后第 45 行又通过可失败的 `single_target_plan()` 取出。`DataDependencyKey` 当前只保存在 target-local plan，没有跨 target cache 消费者。第 120 行把两个分支的 indices 合并后排序以还原声明顺序，而原始 `target.exports` 已保留该顺序。

建议：直接构造 `TargetBuildPlan`，保留两种分支任务的索引与是否需要 Data；trace 需要声明顺序时直接遍历 `target.exports`。真正支持 multi-target 共享构建时，再设计 dependency/cache key。

保持：当前 source read 与 Meta link / Meta-only generation 的重叠执行，以及 generator 之间的数据准备复用。此项不需要引入 DAG executor，也不意味着取消计划阶段。

### 5. 提取代码后端中确实相同的基础操作

优先级：中。收益：中。风险：低到中。

依据：多个语言后端各自实现 `write_file_header`；例如 [`csharp/mod.rs:207`](../../src/generator/csharp/mod.rs#L207) 与 [`java/mod.rs:227`](../../src/generator/java/mod.rs#L227) 重复 project 模板替换、datetime、默认版权和生成提示。创建 stream、包装 `CheckedStreamingCodeWriter`、render、映射写入错误、finish 也在多个后端重复。`CodeGenerator` 内还有用于测试的文件头路径。

建议：在 `code_artifact` 提供面向 `CodeWriterLike` 的 header helper，以及小型 streaming render helper。生成器继续提供语言专属内容。

保持：格式选项、注释前缀、精确换行、文件路径、错误位置和 streaming。C++ 与 Unreal 的容器、ownership、UHT / Blueprint 有实际差异，不建议合并成大量布尔开关控制的后端；也不建议引入跨语言通用模板 DSL。

### 6. 降低规划与生成之间的重复工作

优先级：中。收益：中。风险：中。

依据：[`src/generator/schema_doc/mod.rs:41`](../../src/generator/schema_doc/mod.rs#L41) 在 generation 和 planning 中分别构造完整 `SchemaDoc`；而输出路径实际依赖可见表名和配置。runtime data 在 [`src/data_artifact/runtime.rs:19`](../../src/data_artifact/runtime.rs#L19) / 第 88 行也分别构造 table artifact plan。多个 indexed `par_iter().map().collect::<Vec<_>>()` 后仍按原 index 排序；这类 collect 本身保持输入顺序。

建议：先共用选项解析、可见表筛选和 output-path helper，schema_doc 规划仅收集路径所需信息。只有性能证据表明值得时，再让具体 generator 复用准备结果，避免立刻增加通用 prepared-plan 框架。对能够证明 indexed 保序的局部循环，可以去掉 index 包装和重复排序。

保持：全 target 规划屏障、规划与实际输出完全一致、错误和 manifest 稳定顺序。不可把所有排序删除；来自无序集合或并发完成流的结果仍需排序。`reserve_paths` 与 `create` 对文件系统别名的复查承担不同阶段的保护，不能作为普通重复检查直接删掉。

### 7. 区分有共享算法的 trait 与仅转发的旧 facade

优先级：中到低。收益：中。风险：中。

依据：[`src/data_artifact/registry.rs:79`](../../src/data_artifact/registry.rs#L79) 的 `AggregateDataExporter` 与 build-facing `ArtifactGenerator` 重复 name / alias、planning 和 execution 入口；POT、Lua、Asset Manifest 的后者大多只是适配输入并转发前者。当前没有 aggregate registry 或共享 aggregate 调度算法需要这一层。相比之下，同文件的 `RuntimeTableFormat` 被 JSON / MessagePack / bin 的共同逐表导出算法实际消费，具有明确价值。

建议：聚合 generator 的实现可逐步直接落在具体模块，共用提取、渲染和选项 helper；把仅服务测试的 materialized 入口收窄到测试，生产继续走 sink。保留确有复用的 `RuntimeTableFormat`。旧 API 是否公开、是否存在外部消费者应逐项确认，不能一次删除所有 legacy 接口。

保持：POT 提取、Lua 数据与代码混合导出、Asset Manifest enrichments，以及独立的内容断言与 golden tests。减少接口不应减少测试覆盖。

### 8. 文档按主题保留一个权威描述，清理现状与规划的混写

优先级：高，可与上述小型重构分别提交。收益：高。风险：低。

依据：

- [`Documentation/Tech/task-system-design.md:617`](../Tech/task-system-design.md#L617) 和第 1011 行仍把 filesystem-aware path key 作为未实现事项，但当前 ArtifactSink 已通过 artifact path helper 使用文件系统语义处理路径；应以 [`platform-filesystem-semantics.md`](../Tech/platform-filesystem-semantics.md) 为准。
- [`Implementation/07-artifact-module-detail-design.md:25`](../Implementation/07-artifact-module-detail-design.md#L25) 仍写“建议新增目录”“第一阶段可以先合并”，与当前已拆分的 artifact 模块不符。
- [`Implementation/10-build-module-detail-design.md:28`](../Implementation/10-build-module-detail-design.md#L28) 的文件职责仍把 branch pipeline 放在 `mod.rs`，实际已拆入 `pipeline.rs`；第 102 行还保留阶段实施步骤。
- task-system 文档包含大量未来 executor / async API 形状，同时再次描述当前 build pipeline，容易与 build 模块文档分别演进。

建议：保留 Design 的产品契约、Implementation 的模块与算法、Tech 的资源策略与验证矩阵三层；每个主题只在一个位置描述细节，其他文档链接过去。当前执行拓扑归 build implementation，路径语义归 filesystem 文档；历史阶段步骤移 DevLogs。未来设计先记录需求触发条件和约束，待真实需求出现后再定完整 API。

文档数量本身不是问题。不要通过合并所有设计文档来减少文件数，也不要把尚未实施的建议写成已完成状态。

## 重构之前需要补齐的验证条件

1. [`tests/architecture.rs:259`](../../tests/architecture.rs#L259) 用第一个 `#[cfg(test)]` 截断文件。若文件顶部声明测试模块，后面的生产实现会被跳过；例如 `schema_doc/mod.rs`。这只能作为轻量 guard，当前通过不能证明依赖规则完整覆盖。应修正测试区域识别并覆盖此情况，无需扩展成通用架构分析框架。
2. [当前 CI](../../.github/workflows/ci.yml) 只在 Linux 执行 `cargo test --lib`，跨平台 job 仅编译 release binary；CLI integration、architecture guard 与 runtime harness 未进入这两个 workflow 的自动测试命令。应按现有 [`test-ci-matrix.md`](../Tech/test-ci-matrix.md) 选择核心 integration 与跨平台测试，而不是新造另一套 profile 配置。
3. `cargo lint` 允许 warning，CI Clippy 使用 `-D warnings`。已有 `src/generator/typescript/reader.rs:111` 的 `format_in_format_args` warning 需要单独处理，不能用本地 lint 退出 0 推断 CI 必过。
4. 不把 pedantic lint 数量作为架构复杂度指标。本次 `cargo lint-strict` 退出 0，但有较多既有 warning；应只处理与具体重构相关、有实际收益的项目。

## 建议实施顺序

1. 恢复可信验证基线：处理已有平台断言差异，补齐核心 integration 的 CI 入口，修正架构 guard；同步修正文档的现状描述。
2. 先做小型、独立重构：单 target plan、文件头 helper、统一 CSV read 与路径检查、validation 的重复 finish / 表遍历。
3. 再收敛 generator 输入与执行接口、聚合转发 facade；最后依据测量决定是否复用 prepared result。每项独立提交，避免将 API 清理、调度变化和格式变化混在一起。

每一步都需要比较生成内容、路径集合、诊断顺序、`--serial` / 并行输出、三种写入模式及失败场景；影响某语言生成器时，补跑该语言现有 runtime harness。涉及准备结果缓存或并发粒度时，再测大表耗时和峰值内存。

## 应保留的设计

- Raw Meta 与 Linked Meta 的分离，以及 target-aware view；它们表达配置、引用解析与可见性三种不同职责。
- 统一 generator registry 与稳定 type ID / aliases；14 个内置类型的线性查找不是已确认的性能瓶颈。
- CodeModel IR、语言各自的类型 / 命名 / runtime contract 适配。
- `RuntimeTableFormat` 的共同逐表调度，以及各格式独立序列化。
- 集中的 ArtifactSink、严格路径规划、streaming、dry-run 和 temp-commit；不能为了减少类型，把整个产物重新收集为内存中的 `String` / `Vec<u8>`。
- Rayon 的局部并行和稳定结果合并；当前无需通用 DAG、事件总线、插件容器或额外缓存层。

## 提交验证快照

针对 `d844162` 已执行：

- `cargo fmt -- --check`：通过。
- `cargo lint`：退出 0，存在上述既有 TypeScript warning。
- 两项新增 CLI integration：全内置 generator 在三种写入模式下共享完整规划，以及跨分支 / 辅助文件冲突在写出前失败，均通过。
- `cargo test`：lib 阶段 550 passed / 3 failed，未完成后续全部 integration。失败为 `src/data_artifact/tests.rs:223`、第 655 行的 Windows 路径分隔符断言，以及 `src/project/mod.rs:607` 的 Windows canonical 路径前缀断言；前轮已用独立基线检出复现。
- 本轮评估追加 `cargo lint-strict`：退出 0，有既有 pedantic warning。
- 未安装 cargo-llvm-cov；本次没有覆盖率结果，也没有将上述验证表述为全套测试通过。

本记录新增后仅进行文档链接与 diff 检查，不重复执行未发生代码变更的完整测试。
