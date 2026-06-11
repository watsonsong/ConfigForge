# Diagnostics / Schema Doc / Architecture Review 执行 TODO

本文整理对诊断系统、Schema Doc 实现和模块边界的 architecture review 后续执行计划。本文不再承载所有长期构想，而只追踪**本轮适合立即推进、可独立验证、可独立提交**的修复项。

## Review 结论

当前架构方向整体正确：

- `schema_doc` 作为 `MetaOnly` direct `ArtifactGenerator` 接入是合理的。
- Build 的 generator-first 调度方向正确，大部分路径只依赖 `GeneratorRegistry` / `GeneratorRequirements`。
- `ArtifactSink` 统一写出路径，集中处理 dry-run、路径冲突和 output root 安全。
- 诊断系统已具备 stable code catalog、`Diagnostic` enrichment 字段和 `ConfigForgeError::from_diagnostic()` helper。

本轮优先处理三类**高性价比问题**：

1. Schema Doc Markdown escaping 仍可能被用户可控文本破坏表格结构。
2. `SchemaDocGenerator` 尚未实现 `plan_artifacts()`，未来 MetaOnly 预规划时可能遗漏产物。
3. `ValidationReport(DiagnosticReport<Diagnostic>)` 转 `ValidationIssue` 时只保留第一条 diagnostic，存在静默丢诊断风险。

## 本轮执行范围

### [x] Step 1：修复 Schema Doc Markdown escaping

问题：

- `src/generator/schema_doc/markdown.rs` 当前只移除 `\r` 并转义 `|`。
- `desc`、default、validator summary 等用户可控文本如果包含 `\n`、`\t` 或控制字符，可能破坏 Markdown 表格。

实现项：

- [x] 将 `\n` 归一化为 `<br>` 或空格。
- [x] 将 `\t` 归一化为空格。
- [x] 移除或替换除换行 / tab 外的 ASCII 控制字符。
- [x] 保留 table cell 中 `| -> \|` 的行为。
- [x] 增加测试覆盖 desc 换行、validator summary 中 `|` + 换行、default string 控制字符。

验证：

- [x] `cargo test schema_doc`

已提交：`66a79d5 fix(schema_doc): harden markdown escaping`

### [x] Step 2：为 `SchemaDocGenerator` 实现 `plan_artifacts()`

问题：

- `ArtifactGenerator::plan_artifacts()` 默认返回空。
- `SchemaDocGenerator` 当前只通过 dry-run sink 间接得到 artifact plan。
- 当前不影响已存在 dry-run，但未来 MetaOnly 分支如果加入预规划，会遗漏 schema_doc 产物。

实现项：

- [x] 复用 `schema_doc_export_config_from_options()` 和 `schema_doc_output_paths()`。
- [x] 为 Markdown 返回 `index.md` + 每表 `.md`。
- [x] 为 HTML 返回 `index.html` + 每表 `.html`。
- [x] 补单元测试，确保 Markdown / HTML plan 与实际输出路径一致。

验证：

- [x] `cargo test schema_doc`

已提交：`6620b4c fix(schema_doc): plan generator output artifacts`

### [x] Step 3：修复 `error_issue_from_result` 丢多条 diagnostic

问题：

- `src/validation/diagnostic.rs` 的 `error_issue_from_result()` 当前只消费 `report.diagnostics.into_iter().next()`。
- 如果某个校验函数返回多条 `Diagnostic`，后续会静默丢弃其余问题。

实现项：

- [x] 将转换逻辑调整为可返回多条 `ValidationIssue`。
- [x] 调整调用方，确保多条 issue 全部 append，且顺序稳定。
- [x] 增加单元测试：构造包含多条 diagnostic 的 `DiagnosticReport`，验证转换后不丢失。

验证：

- [x] `cargo test validation`
- [x] `cargo test schema_doc`

已提交：`5fd464e fix(validation): preserve all diagnostics in issue conversion`

## 本轮暂不执行但保留判断的事项

以下 review 项合理，但不适合作为本轮连续执行目标：

- 移除 `diagnostic -> meta` 反向依赖：重要但影响 `ConfigForgeError`、Meta validate、CLI human / JSON 输出，应作为独立专题推进。
- Meta diagnostic 纳入 unified code catalog：应跟随上一项一起规划，避免重复改动。
- 收敛 validation_report 成功 / 失败双路径：架构收益高，但影响 build validation failure fallback，应单独批次推进。
- 拆分 `schema_doc/html.rs`、`artifact/mod.rs`、`build/mod.rs`：属于文件级结构清理，适合功能修复完成后独立提交。
- 收窄 `src/lib.rs` public facade、减少 `generator/mod.rs` prelude、统一 language defaults：需要先明确 public API / codegen 策略。
- 路径 normalize helper 去重、`ArtifactProducer` 去留、`SchemaDocOptions` 开放策略、test-only helper 边界清理：优先级较低，暂不在本文追踪。

## 完成后期望状态

- Schema Doc Markdown / HTML renderer 输出均不会因用户文本破坏文档结构。
- Schema Doc 的 artifact planning 与其他 generator 行为一致。
- ValidationReport 中的多条结构化 diagnostic 不会在转换到 `ValidationIssue` 时丢失。

## 本轮执行结果

- 已完成 3 个本轮高性价比问题修复，并分别通过对应单元测试验证后提交。
- 每步完成后复核当前实现，未发现需要扩大本轮范围的额外架构改动。
