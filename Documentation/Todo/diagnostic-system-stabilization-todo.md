# Diagnostics System Stabilization TODO

本文拆分 ConfigForge 诊断系统从当前 `Implemented MVP` 演进到 `Stable` 的分步骤计划。当前 `src/diagnostic/` 已提供 severity、location、formatting、report aggregation 和 crate error facade；后续稳定化的重点不是增加更多自然语言错误文本，而是建立一套跨 Meta / Data / Validation / Artifact / Generator / Build / CLI 一致使用的结构化诊断协议。

目标状态：任意阶段产生的问题，都能以稳定 code、稳定 severity、完整 source trace、明确 current / expected、可机器读取的 report 输出，并且 CLI、Designer Validation Report、build error、测试快照使用同一套诊断模型。

## 可行性评估（2026-06-10）

结论：整体方向可行，但应按“先扩展协议、再迁移 producer、最后收口 report/CLI”的顺序推进；当前代码已经具备 `validation_report` export task、CLI `--report`、`DesignerValidationReport` 与 HTML / Markdown / JSON / CSV renderer，因此本提议不需要重新引入报告导出入口，重点应改为补齐底层 `Diagnostic` / `ValidationIssue` 不能表达的字段，并避免在 report 层继续 fallback。

需要修正的边界：

- 诊断码当前是 `String` 契约，`ValidationIssue::code` 与 `Diagnostic::code` 都接受字符串；本计划应建立 code catalog / 常量与元数据，不应在第一批强制改成 enum，否则会影响外部扩展 generator / validator 的自定义 code。
- `validation_report` build export type 已实现，当前支持 `format`、`file_name`、`title`；`include_warnings`、`include_errors`、`group_by` 仍未实现。后续任务应围绕 option 扩展和结构化字段贯通，而不是把 `validation_report` 当成缺失类型处理。
- `DesignerValidationReport` 模型已有 `source_column`、`current_value`、`expected`、`owner`、`domain` 字段，但 `enrich_issues()` 当前只能填 fallback：`source_column/current_value/expected = None`、`owner = <unassigned>`、`domain = <uncategorized>`。这些字段必须先从 `Diagnostic` / `ValidationIssue` 与 producer 侧补齐，report renderer 不应自行猜测。
- `DiagnosticLocation` 当前只有 `source_path`、`table`、`field`、`row_index`、`source_row_number`；列、sheet/provider、target/export task/generator option 定位都需要显式扩展。source-provider/sheet 属于未来多源能力，建议设计预留但不要作为最小 Stable 的阻塞项。
- `ConfigForgeError::Validation(String)` 使用面很广，包括 artifact、asset、build、generator option、data loading 等。迁移必须按高价值路径分批，不应要求一次性全量替换。
- 当前 `DiagnosticSeverity` 只有 `Error` / `Warning`，本计划不引入 `Info` 级别，避免扩大 CLI exit code 和 report 语义范围。

因此，下面步骤保持可执行，但已把“新增能力”与“已有 MVP 的补齐/扩展”区分开，并明确哪些内容属于最小 Stable 闭环、哪些属于后续 P2。

## 范围判断

该工作流同时影响：

1. `src/diagnostic/` 基础模型：`Diagnostic`、`DiagnosticLocation`、`DiagnosticEntry`、`DiagnosticReport<T>` 与 `ConfigForgeError`。
2. `src/validation/` 与 `src/data/`：数据行、字段、CSV cell、FK、asset、validator 诊断需要完整 source trace。
3. `src/artifact/`：路径逃逸、大小写冲突、symlink / hardlink alias 等输出安全错误需要稳定 code。
4. `src/generator/` 与 `src/build/`：unknown generator、ambiguous alias、unsupported option、requirements mismatch 等扩展类错误需要结构化诊断。
5. Designer Validation Report：HTML / Markdown / JSON / CSV report 应消费底层诊断 enrichment，而不是在 renderer 中猜测业务字段。
6. 测试与 CI：断言重点应从完整自然语言 message 迁移到 code / severity / location / current / expected / hint。

因此推荐先稳定基础协议和 generator / artifact 这类边界错误，再迁移 data validation 的高价值错误，最后收口 report enrichment、CLI 格式和文档状态矩阵。不要一次性迁移所有 `ConfigForgeError::Validation(String)`，避免大范围噪声提交。

## 执行原则

- 每一步都应独立可提交；除非当前步骤显式要求，不同时迁移所有模块的错误。
- 新增业务校验错误默认使用结构化 `Diagnostic` / `DiagnosticReport`，不继续新增裸 `ConfigForgeError::Validation(format!(...))`。
- message 可以面向人类持续优化，但测试、CI、report 和外部工具判断必须依赖 stable diagnostic code。
- renderer 只负责展示，不负责推导业务含义；`current_value`、`expected`、`source_column`、`owner`、`domain` 等信息应由产生诊断的模块提供。
- 诊断基础层保持底层依赖方向：`diagnostic` foundation 不能依赖 `meta`、`data`、`validation`、`artifact`、`generator`、`project` 或 `build`。
- `Validation(String)` 可保留为 legacy fallback，但应逐步收敛使用范围并可统计债务。

## Step 1：扩展诊断基础模型

当前状态：`Diagnostic` / `ValidationIssue` 已有 `code`、`severity`、`message`、`location`、`source` / `validator`、`target`、`hint`；尚不能承载 Designer Validation Report 中已经声明的 `current_value`、`expected`、`explanation`、`owner`、`domain`。本步骤可行，且应作为后续 report enrichment 的前置条件。

- [ ] 扩展 `Diagnostic`，增加 `current_value`、`expected`、`explanation`、`domain`、`owner` 等 enrichment 字段。
- [ ] 扩展 `DiagnosticEntry` default methods，让 `MetaDiagnostic`、`ValidationIssue`、generator / artifact diagnostic 能渐进式接入新字段。
- [ ] 同步扩展 `ValidationIssue` 或提供从 `ValidationIssue` 到 `Diagnostic` 的 enrichment bridge，避免 `DesignerValidationReport::from_validation_report()` 与 `from_diagnostic_report()` 继续丢失字段。
- [ ] 为 `Diagnostic` 增加 builder-style API，例如 `with_hint()`、`with_current_value()`、`with_expected()`、`with_explanation()`、`with_source()`、`with_target()`，避免构造函数参数膨胀。
- [ ] 明确 `Diagnostic.source` 的语义为诊断生产者或模块来源，例如 `generator.java`、`validation.foreign_key`；不要与 `DiagnosticLocation.source_path` 混淆。
- [ ] 保持现有 `Diagnostic::new()` 的简单入口，减少迁移成本。

交付标准：新的诊断模型能够承载 Designer Validation Report 需要的核心字段，且旧的 `DiagnosticEntry` 实现可以通过 default methods 平滑编译。

## Step 2：扩展 source trace 与 location 模型

当前状态：`DiagnosticLocation` 只有 `source_path`、`table`、`field`、`row_index`、`source_row_number`。列信息是最小 Stable 必需项；sheet/provider 和 generator option 精确定位可先设计为可选字段或后续 P2，避免阻塞基础诊断稳定化。

- [ ] 给 `DiagnosticLocation` 增加 column 信息：`source_column_number`、`source_column_name`、`column_index`。
- [ ] 预留多源表格能力：`source_provider`、`source_sheet` 或等价结构，为 XLSX / Lark Sheet / Google Sheet 做准备；该项可作为 P2，不进入最小 Stable 阻塞清单。
- [ ] 明确 logical location 与 physical source location 的关系：table / field / row_index 属于逻辑定位；source path / row / column / sheet 属于原始输入定位。
- [ ] 支持 target / export task / generator option 这类非数据行诊断的定位方式；短期可通过 `source`、`target` 和 message 表达，长期再引入独立 `DiagnosticContext`，不要把配置定位强塞进数据行字段。
- [ ] 更新 location `Display` 格式，确保 CLI 中依然简洁可读。

交付标准：CSV 类型错误、FK 错误、asset 错误可以定位到表、字段、源文件、源行和源列；generator 配置错误可以定位到 target / export task / type_id / option。

## Step 3：建立 Diagnostic Code Catalog

当前状态：诊断 code 是字符串契约，且已有内置 validator 使用 `validator.*` 风格 code。为保持扩展性，本步骤建立 catalog / 常量 / metadata，不引入强制 enum；外部 generator / validator 可以继续提供自定义字符串 code，但内置 code 必须进入 catalog。

- [ ] 新增诊断码目录，建议放在 `src/diagnostic/code.rs` 或等价模块中。
- [ ] 定义第一批稳定 code 常量，覆盖 `CF_META_*`、`CF_DATA_*`、`CF_VALIDATION_*`、`CF_ARTIFACT_*`、`CF_GENERATOR_*`、`CF_BUILD_*`、`CF_RUNTIME_CONTRACT_*`；保留 `String` 作为 public storage/API，不强制 enum 化。
- [ ] 为每个 code 维护 metadata：default severity、domain、title、explanation、default hint、since、deprecated。
- [ ] 增加 code catalog 单元测试，保证 code 唯一、domain 合法、默认 severity 明确。
- [ ] 明确新增 code 的 review 规则：优先复用已有 code；只有语义不同才新增 code。

第一批建议 code：

```text
CF_GENERATOR_UNKNOWN_TYPE
CF_GENERATOR_AMBIGUOUS_TYPE
CF_GENERATOR_INVALID_OPTION
CF_GENERATOR_UNSUPPORTED_OPTION
CF_GENERATOR_UNSUPPORTED_DATA_INPUT
CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH

CF_ARTIFACT_PATH_ESCAPE
CF_ARTIFACT_PATH_COLLISION
CF_ARTIFACT_CASE_INSENSITIVE_COLLISION
CF_ARTIFACT_SYMLINK_ALIAS_COLLISION
CF_ARTIFACT_HARDLINK_ALIAS_COLLISION

CF_DATA_INVALID_TYPE
CF_DATA_MISSING_REQUIRED_FIELD
CF_DATA_INVALID_ENUM
CF_DATA_INVALID_FOREIGN_KEY
CF_DATA_INVALID_ASSET_REF
CF_DATA_DUPLICATE_KEY

CF_BUILD_TARGET_NOT_FOUND
CF_BUILD_INPUT_REQUIRED
CF_BUILD_EXPORT_TASK_FAILED
```

交付标准：新旧测试可以稳定断言 diagnostic code；message 文案调整不影响 CI 判断。

## Step 4：为 `ConfigForgeError` 增加结构化诊断 helper

当前状态：`ConfigForgeError` 已有 `ValidationReport(DiagnosticReport<Diagnostic>)`，但创建单条结构化诊断仍不够顺手；`Validation(String)` 仍是大量模块的 legacy fallback。本步骤可行，重点是降低 producer 迁移成本，而不是删除 legacy variant。

- [ ] 给 `ConfigForgeError` 增加 `from_diagnostic(diagnostic: Diagnostic)` helper。
- [ ] 给 `ConfigForgeError` 增加 `from_diagnostics(diagnostics: Vec<Diagnostic>)` 或 `diagnostic_report(report)` helper，内部复用现有 `ValidationReport(DiagnosticReport<Diagnostic>)` variant 或后续更名后的通用 report variant。
- [ ] 给 `DiagnosticReport<T>` 增加常用构造 helper，例如 `from_one()`、`extend()`、`is_empty()`。
- [ ] 为 `Validation(String)` 加注释，标记为 legacy fallback，只用于尚未迁移的边界错误或第三方错误包装。
- [ ] 增加使用点统计方式，例如通过 grep / lint 文档化 `ConfigForgeError::Validation(` 数量，作为迁移债务指标。

交付标准：业务模块创建结构化 validation error 的成本低于手写字符串错误；新代码有自然的迁移入口。

## Step 5：迁移 Generator / Build 边界诊断

当前状态：`validation_report` generator 已注册且 requirements 已能声明 `with_validation_report()`；generator / build 边界仍大量返回 `Validation(String)`。本步骤应聚焦 registry、option parsing、requirements mismatch 和 prepared input 缺失等扩展边界，不需要新增 `validation_report` 类型。

- [ ] 将 unknown generator type 迁移为 `CF_GENERATOR_UNKNOWN_TYPE`。
- [ ] 将 ambiguous generator registration / alias collision 迁移为 `CF_GENERATOR_AMBIGUOUS_TYPE`。
- [ ] 将 generator requirements mismatch，例如 MetaOnly 分支执行 Data-backed generator，迁移为结构化 diagnostic。
- [ ] 将 `PreparedGeneratorInputs` 中缺少 data set / validation report / pool 的错误迁移为 `CF_GENERATOR_UNSUPPORTED_DATA_INPUT` 或更准确 code。
- [ ] 将 generator option 解析错误统一到 `CF_GENERATOR_INVALID_OPTION` / `CF_GENERATOR_UNSUPPORTED_OPTION`。
- [ ] 补充 registry 健康检查：builtin `type_id` 唯一、alias 唯一、alias 不与其他 `type_id` 冲突。

交付标准：新增 generator 时的接入错误可被 CLI / report / tests 稳定识别，Build 不需要因为新增 generator 增加具体 type 分支。

## Step 6：迁移 Artifact 安全诊断

- [ ] 将 output root escape 错误迁移为 `CF_ARTIFACT_PATH_ESCAPE`。
- [ ] 将 normalized path collision 错误迁移为 `CF_ARTIFACT_PATH_COLLISION`。
- [ ] 将 case-insensitive collision 错误迁移为 `CF_ARTIFACT_CASE_INSENSITIVE_COLLISION`。
- [ ] 将 symlink alias / hardlink alias collision 错误迁移为专属 code。
- [ ] 在 diagnostic 中填充 `current_value` 为用户声明的输出路径或冲突路径，`expected` 为 output root 内唯一 normalized artifact path。
- [ ] 补充 artifact diagnostic tests，断言 code、path、current / expected，而不是只匹配 message。

交付标准：所有 artifact path safety 错误都有稳定 code 和足够路径上下文，便于 CI annotation 和用户定位。

## Step 7：迁移 Data / Validation 高价值诊断

当前状态：`ValidationIssue` 已作为 validation report 的结构化单元，但 Data loading、typed value、asset 解析等仍有不少 `Validation(String)`。本步骤应按用户可见高频错误分批迁移；CSV column/current/expected 依赖 Step 1/2 的字段先落地。

- [ ] 优先迁移 CSV 类型转换错误，填充 table、field、source_path、source_row_number、source_column_name、current_value、expected。
- [ ] 迁移 required / missing field 错误，填充字段名、源行、expected requirement。
- [ ] 迁移 enum 错误，填充 current enum string / ordinal、expected enum candidates。
- [ ] 迁移 duplicate key / index 错误，填充 index name、duplicate key value、冲突行号。
- [ ] 迁移 FK 错误，填充 current key、expected target table / index、source field。
- [ ] 迁移 asset reference 错误，填充 raw asset ref、expected search path / asset kind rule、canonicalization hint。
- [ ] 迁移 builtin validator 错误，至少填充 validator source、current_value、expected、hint。

交付标准：设计师看到一条 data validation 错误后，不需要阅读源码或猜测上下文，即可知道哪个表、哪行、哪列、当前值、期望值和修复建议。

## Step 8：打通 Designer Validation Report enrichment

当前状态：`DesignerValidationReport`、renderers、CLI `--report` 和 build `type = "validation_report"` 已存在；但 report enrichment 当前只从 code catalog 和基础 location 填充，`source_column/current_value/expected/owner/domain` 多数为空或 fallback。本步骤是“贯通字段”，不是“从零实现报告导出”。

- [ ] 让 `DesignerValidationReport::from_diagnostic_report()` 消费底层 `Diagnostic` 的 `current_value`、`expected`、`source_column`、`explanation`、`hint`、`domain`、`owner`。
- [ ] 让 `DesignerValidationReport::from_validation_report()` 同样保留 `ValidationIssue` 侧新增 enrichment，避免 build `validation_report` 走 validation report 路径时丢字段。
- [ ] 删除或减少 report renderer 中的业务猜测逻辑；renderer 只做格式化。
- [ ] HTML / Markdown / JSON / CSV 四种输出保持字段一致，空字段有明确 fallback 策略。
- [ ] 增加 report snapshot / golden tests，覆盖 source column、current / expected、owner / domain。
- [ ] CLI `--report` 导出的 validation report 在 validation 失败时也能尽量生成包含结构化诊断的报告。
- [ ] 扩展 build `validation_report` option 时单独处理 `include_warnings`、`include_errors`、`group_by`；这些不是当前字段贯通的前置条件，可放在本步骤末尾或后续独立批次。

交付标准：Designer Validation Report 从“错误列表渲染器”升级为“可用于策划 / QA / CI 分派问题的结构化报告”。

## Step 9：完善 CLI / machine-readable 输出格式

- [ ] 保持默认 human 格式简洁，优先展示 code、location、message、hint。
- [ ] 增加或完善 JSON diagnostic 输出，稳定 schema 字段：code、severity、message、location、source、target、current_value、expected、hint。
- [ ] 评估 CI annotation 格式，例如 GitHub / GitLab compatible text，或后续 SARIF。
- [ ] 错误汇总按 severity / domain / table / owner 可读排序。
- [ ] warning 与 error 的退出码策略保持稳定并文档化。

交付标准：CLI 输出同时服务本地人类阅读和 CI / IDE 工具读取，不因自然语言 message 微调破坏机器消费。

## Step 10：建立诊断测试策略与迁移防回归

- [ ] 新增 diagnostic snapshot / golden tests，覆盖 generator unknown type、invalid option、artifact path escape、path collision、CSV invalid type、FK missing target、asset missing。
- [ ] 测试断言优先级：code > severity > location > current_value / expected > hint > message。
- [ ] 对 legacy `Validation(String)` 使用点做统计，迁移批次完成后更新数量。
- [ ] 为每个新增 diagnostic code 增加至少一个 producer 测试或 catalog 测试。
- [ ] 对 Designer Validation Report JSON / CSV 输出增加字段级断言，避免字段回退为空。

交付标准：诊断体验不会随着新增 generator、validator 或 artifact 安全规则而退化；message 微调不会造成大量测试脆弱失败。

## Step 11：文档与状态矩阵收口

- [ ] 更新 `Documentation/Implementation/02-diagnostic-module-detail-design.md`，记录 Stable 诊断模型、code catalog、location/source trace 和 error facade 规则。
- [ ] 更新 `Documentation/Design/10-designer-validation-report.md`，把 current / expected / source_column / owner / domain 的实现状态与限制同步到实际代码。
- [ ] 更新 `Documentation/Design/05-data-validation.md`，说明 data validation 的 source trace 和 stable code 语义。
- [ ] 更新 generator API 文档，要求新增 generator 使用结构化诊断，不新增裸字符串业务错误。
- [ ] 根据实际完成情况更新 Design 当前实现状态矩阵；只有当 Data / Artifact / Generator 主要错误都结构化并有测试后，才把 Diagnostics 从 `Implemented MVP` 调整为 `Stable`。

交付标准：Design / Implementation / Tech 文档对 Diagnostics 能力、边界和状态表述一致；不会出现代码已经 Stable 但文档仍写 MVP，或文档提前承诺未实现能力。

## 建议执行批次

1. **Batch A：基础协议** — Step 1、Step 2、Step 3、Step 4。先补模型、location、code catalog 和 error helper，不大规模迁移业务模块。
2. **Batch B：扩展边界稳定** — Step 5、Step 6。优先迁移 Generator / Build / Artifact 错误，服务开闭原则和路径安全。
3. **Batch C：数据诊断产品化** — Step 7、Step 8。把 Data / Validation 的高频用户错误补齐 source trace，并打通 Designer Validation Report。
4. **Batch D：CLI 与测试防回归** — Step 9、Step 10。稳定 machine-readable 输出和 snapshot / golden tests。
5. **Batch E：文档状态收口** — Step 11。只根据已完成事实更新状态矩阵，不提前标 Stable。

## 最小 Stable 闭环

如果只追求最小可接受 Stable，至少完成以下事项：

- [ ] `Diagnostic` 和 `DiagnosticEntry` 支持 current / expected / explanation / owner / domain。
- [ ] `DiagnosticLocation` 支持 source column。
- [ ] 建立 diagnostic code catalog，并覆盖 Data / Artifact / Generator 的高价值错误。
- [ ] 业务 validation 新代码不再新增裸 `Validation(String)`。
- [ ] Designer Validation Report 能真实展示 source column、current value、expected 和 hint。
- [ ] 核心诊断测试断言 stable code 和 location，而不是只断言自然语言 message。

达到以上闭环后，Diagnostics 才适合从 `Implemented MVP` 调整为 `Stable`。
