# Diagnostic / Meta 解耦后续专题 TODO

本文承接 [`diagnostic-schema-doc-architecture-review-todo.md`](diagnostic-schema-doc-architecture-review-todo.md) 中“移除 `diagnostic -> meta` 反向依赖：重要但影响 `ConfigForgeError`、Meta validate、CLI human / JSON 输出，应作为独立专题推进。”的 review 记录，整理该事项在项目当前状态下的后续专题计划。

## 当前状态判断

该 review 项的核心解耦和 Meta stable diagnostic code catalog 迁移均已完成，当前状态已收敛为**防回归已固化 + 文档已同步 + 后续观察**。

关键依据：

- `ConfigForgeError::MetaValidation` 当前已持有 `DiagnosticReport<Diagnostic>`，不再直接持有 `Vec<MetaDiagnostic>`，见 `src/diagnostic/error.rs:45`。
- `src/diagnostic/` 内已搜索不到 `crate::meta` / `MetaDiagnostic` 引用；`diagnostic` foundation 的依赖方向已满足 `Documentation/Implementation/01-module-outline.md:84` 的约束。
- `MetaDiagnostic` 到通用 `DiagnosticReport<Diagnostic>` 的转换 helper 已位于 Meta 模块边界，见 `src/meta/diagnostic.rs:30`。
- Build 在 Meta link / validate 失败处先调用 `crate::meta::meta_diagnostics_to_report`，再构造 `ConfigForgeError::MetaValidation`，见 `src/build/mod.rs:419`、`src/build/mod.rs:1161`、`src/build/mod.rs:1201`。
- CLI JSON 输出已经只识别 `ConfigForgeError::MetaValidation(report)` / `ValidationReport(report)` 的通用 report，不再需要知道 `MetaDiagnostic` 的 payload 结构，见 `src/cli.rs:251`。
- 已有执行记录 [`diagnostic-meta-dependency-decoupling-todo.md`](diagnostic-meta-dependency-decoupling-todo.md) 标记解耦完成，并记录通过 `cargo fmt`、`cargo test meta`、`cargo test cli`、`cargo test validation`、`cargo test diagnostic`。

已处理的尾项：

- Meta diagnostic 已纳入 unified code catalog；`MetaDiagnostic::code()` 输出 `CF_META_*` stable code，catalog 覆盖全部当前 `MetaDiagnosticCode` 变体。
- CLI JSON 中 Meta validation diagnostic 已包含 stable code、`source = "meta.validate"`、`domain = "meta"`、catalog hint / explanation。
- `Documentation/Implementation/02-diagnostic-module-detail-design.md` 已同步 `MetaValidation(DiagnosticReport<Diagnostic>)` 和 Meta catalog 说明。

## 专题目标

1. 防止 `src/diagnostic/` 重新引入 `meta` 依赖。
2. 将 Meta diagnostic code 纳入统一 stable code catalog，并让 CLI JSON 输出稳定机器码。
3. 在不一次性重写 Meta validate 主流程的前提下，逐步增强 Meta diagnostic 的 source / domain / hint / explanation 输出。
4. 同步 Implementation 文档，使文档描述与当前代码边界一致。

## 执行原则

- 每一步都必须能独立完成、独立运行单元测试、独立提交 git。
- 每一步都不能重新引入 `diagnostic -> meta` 依赖。
- 任何 user-visible JSON code 变更必须有 CLI JSON 测试覆盖。
- Meta 内部仍可保留 `MetaDiagnosticCode` enum，避免为了 catalog 迁移把 `meta/link.rs` 与 `meta/validate.rs` 的大量分支一次性改成字符串常量。

## 分步 TODO

### [x] Step 1：增加 `diagnostic` 不依赖 `meta` 的防回归测试

目的：把当前已完成的解耦状态固化为测试约束，避免后续修改 `ConfigForgeError` 或 formatter 时重新引用 `crate::meta`。

实现项：

- [x] 在 diagnostic 模块测试中增加 source scan 或更轻量的边界测试，断言 `src/diagnostic/**/*.rs` 不包含 `crate::meta`、`super::meta`、`MetaDiagnostic` 等反向依赖标记。
- [x] 保留 `ConfigForgeError::MetaValidation(DiagnosticReport<Diagnostic>)` 的现有 helper / formatting 测试，确保新增测试只约束依赖方向，不改变错误展示。

验证：

- [x] `cargo test diagnostic`

后续计划复核：Step 1 只固化依赖方向防回归，不改变 user-visible 输出；Step 2～Step 6 仍按原计划推进，无需调整。

可独立提交建议：

- `test(diagnostic): guard against meta dependency regression`

### [x] Step 2：在 unified catalog 中登记 Meta stable codes（暂不切换输出）

目的：先把 catalog 基础设施补齐，降低后续切换 `MetaDiagnostic::code()` 的风险。

实现项：

- [x] 在 `src/diagnostic/code.rs` 增加 `CF_META_*` stable code 常量，覆盖 `MetaDiagnosticCode` 当前所有变体。
- [x] 在 `DIAGNOSTIC_CODE_METADATA` 中登记 metadata，domain 使用 `meta`。
- [x] 将 `meta` 加入 catalog 测试允许的 domain 集合。
- [x] 增加 catalog 测试，断言至少包含 DuplicateTableName / UnknownFieldType / InvalidValidatorConfig 等高频 Meta code。

验证：

- [x] `cargo test diagnostic`

后续计划复核：Step 2 只新增 catalog 常量与 metadata，尚未改变 Meta diagnostic 对外 code；Step 3～Step 6 仍按原计划推进。

可独立提交建议：

- `feat(diagnostic): register stable meta diagnostic codes`

### [x] Step 3：在 Meta 模块内建立 enum 到 stable code 的完整映射

目的：把“Meta 内部 enum”与“外部 stable code string”的转换留在 `meta` 模块，保持依赖方向为 `meta -> diagnostic`。

实现项：

- [x] 为 `MetaDiagnosticCode` 增加 `stable_code()` 或等价 helper，返回 Step 2 中的 `CF_META_*` 常量。
- [x] 增加 exhaustive unit test，确保每个 `MetaDiagnosticCode` 变体都有 stable code 映射，且映射 code 存在于 catalog。
- [x] 暂不修改 `DiagnosticEntry for MetaDiagnostic::code()` 的对外输出，保证此步无 user-visible 行为变更。

验证：

- [x] `cargo test meta`
- [x] `cargo test diagnostic`

后续计划复核：Step 3 只建立 Meta 内部 enum 到 catalog code 的映射，尚未切换 CLI/human 输出；Step 4～Step 6 仍按原计划推进。

可独立提交建议：

- `feat(meta): map diagnostics to stable catalog codes`

### [x] Step 4：切换 Meta diagnostic 输出为 stable code，并更新 CLI JSON 测试

目的：完成 Meta diagnostic 纳入 unified code catalog 的对外切换，让 human / JSON 输出使用稳定机器码。

实现项：

- [x] 将 `DiagnosticEntry for MetaDiagnostic::code()` 从 Debug 字符串改为 Step 3 的 stable code。
- [x] 更新 Meta loader / build / CLI 测试中对 code 字符串的断言。
- [x] 更新 `json_error_output_preserves_meta_validation_diagnostics`，断言 JSON code 为 `CF_META_DUPLICATE_TABLE_NAME` 一类 stable code，同时保留 location / message 断言。
- [x] 检查 human error 文案变化是否可接受；本步接受 human error code 从 enum Debug 字符串变为 stable machine code，后续可由 report/catalog title 提升可读性。

验证：

- [x] `cargo test meta`
- [x] `cargo test cli`
- [x] `cargo test diagnostic`

后续计划复核：Step 4 已完成 user-visible code 切换；Step 5 继续补齐 source/domain/hint/explanation，Step 6 需同步文档中的 CLI JSON code 说明。

可独立提交建议：

- `refactor(meta): emit stable diagnostic codes`

### [x] Step 5：为 Meta diagnostics 补齐 source / domain / hint fallback

目的：利用 unified catalog 让 Meta validation 的 CLI JSON 与 validation/report 类诊断具备相同 enrichment 质量。

实现项：

- [x] 在 `DiagnosticEntry for MetaDiagnostic` 中返回 `source = "meta.validate"` 或按 link / validate 阶段细分 source。
- [x] 返回 `domain = "meta"`。
- [x] 通过 `diagnostic::code::metadata_for(self.code.stable_code())` 提供 explanation / hint fallback，或在转换 helper 中统一补齐。
- [x] 增加单元测试覆盖 Meta validation JSON 输出中的 source / domain / hint / explanation。

验证：

- [x] `cargo test meta`
- [x] `cargo test cli`
- [x] `cargo test diagnostic`

后续计划复核：Step 5 已补齐 JSON enrichment；Step 6 需同步 Implementation 文档中错误 payload、catalog 覆盖范围和 Meta JSON 输出字段说明。

可独立提交建议：

- `feat(meta): enrich diagnostics from code catalog`

### [x] Step 6：同步 Implementation 文档与迁移记录

目的：移除文档中残留的旧 payload 描述，避免后续开发者误以为 `diagnostic::error` 可以引用具体业务诊断类型。

实现项：

- [x] 更新 `Documentation/Implementation/02-diagnostic-module-detail-design.md`，将 `MetaValidation(Vec<MetaDiagnostic>)` 改为 `MetaValidation(DiagnosticReport<Diagnostic>)`。
- [x] 在 Diagnostic 文档中补充 Meta stable code catalog 的状态与迁移规则。
- [x] 如 Step 4 改变了 CLI JSON code，更新相关设计文档对 code 稳定性的说明。

验证：

- [x] `cargo test diagnostic`
- [x] `cargo test meta`
- [x] `cargo test cli`

后续计划复核：本专题计划中的 6 个步骤已全部完成；不再需要新增步骤。后续如要进一步区分 `meta.link` / `meta.validate` source，可另开低优先级专题，不影响当前目标完成。

可独立提交建议：

- `docs(diagnostic): update meta validation boundary`

## 非目标

- 不在本专题中移除 `MetaDiagnostic` / `MetaDiagnosticCode`；它们仍是 Meta link / validate 内部的结构化诊断模型。
- 不把 `meta/link.rs`、`meta/validate.rs` 的所有诊断构造一次性改成通用 `Diagnostic`。
- 不重构 CLI 的 `anyhow` 顶层错误处理；本专题只保证现有 human / JSON 输出质量不退化。
- 不处理 validation report 成功 / 失败双路径收敛；该项影响 build validation failure fallback，应保持独立专题。

## 完成后期望状态

- `src/diagnostic/` 有测试防止重新依赖 `src/meta/`。
- Meta diagnostic code 全部有 `CF_META_*` stable code 与 catalog metadata。
- `ConfigForgeError::MetaValidation`、Meta validate、CLI human / JSON 输出仍走通用 `DiagnosticReport<Diagnostic>`，且 JSON payload 包含稳定 code、source、domain、location、hint / explanation。
- Implementation 文档与源码边界一致，不再出现旧的 `MetaValidation(Vec<MetaDiagnostic>)` 描述。
