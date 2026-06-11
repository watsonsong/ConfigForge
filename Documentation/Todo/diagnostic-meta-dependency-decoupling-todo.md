# Diagnostic / Meta 反向依赖解耦 TODO

本文拆解“移除 `diagnostic -> meta` 反向依赖”专题。目标是在不改变用户可见 CLI human / JSON 诊断语义的前提下，让 `src/diagnostic/` 不再引用 `src/meta/` 的具体类型，保持模块依赖方向为 `meta -> diagnostic`。

## 背景与问题

当前唯一明确的反向依赖位于 `src/diagnostic/error.rs`：

- `ConfigForgeError::MetaValidation(Vec<MetaDiagnostic>)` 直接持有 `meta::MetaDiagnostic`。
- `diagnostic` 因此需要 `use crate::meta::MetaDiagnostic`，形成 `diagnostic -> meta` 依赖。
- CLI JSON 输出又需要把 `MetaDiagnostic` 转为通用 `Diagnostic`，导致 Meta 诊断在错误边界和 CLI 边界存在重复转换。

这会削弱 `diagnostic` 作为基础模块的独立性，也让后续统一 code catalog、Meta JSON 输出增强、错误类型分层时更难拆分。

## 本轮执行范围

### [x] Step 1：收敛 `ConfigForgeError::MetaValidation` 为通用诊断报告

实现项：

- [x] 将 `ConfigForgeError::MetaValidation` payload 从 `Vec<MetaDiagnostic>` 改为 `DiagnosticReport<Diagnostic>`。
- [x] 删除 `src/diagnostic/error.rs` 中对 `crate::meta::MetaDiagnostic` 的引用。
- [x] 复用 `DiagnosticReport` 的 message formatting，避免保留 meta-specific formatter。
- [x] 保持 human error 文案仍为 `meta validation failed: ...`。

验证：

- [x] `cargo test diagnostic`

已提交：`ba963fc refactor(diagnostic): decouple meta validation errors`

### [x] Step 2：在 Meta 边界完成类型到通用诊断的转换

实现项：

- [x] 在 `meta` 模块内提供从 `Vec<MetaDiagnostic>` 到 `DiagnosticReport<Diagnostic>` 的转换 helper。
- [x] 调整 `MetaModel::from_toml_str` / `from_path` / `from_paths` / `finish_loading` 的错误构造。
- [x] 保留 `MetaModel::link()` 返回 `Vec<MetaDiagnostic>` 的内部/测试友好 API，不在本轮强制改为通用诊断。
- [x] 更新 Meta 单元测试，使其通过通用错误报告验证 code / location / message。

验证：

- [x] `cargo test meta`

已提交：`ba963fc refactor(diagnostic): decouple meta validation errors`

### [x] Step 3：校验 CLI human / JSON 输出兼容性

实现项：

- [x] 调整 CLI JSON 诊断提取逻辑，直接复用 `MetaValidation` 内的 `DiagnosticReport<Diagnostic>`。
- [x] 增加或更新测试覆盖 Meta validation error 的 JSON diagnostics 输出。
- [x] 回归 validation report JSON 输出，确保 Step 1 不影响业务校验错误。

验证：

- [x] `cargo test cli`
- [x] `cargo test validation`

已提交：`ba963fc refactor(diagnostic): decouple meta validation errors`

## 本轮暂不执行事项

- 不在本轮把 `MetaDiagnosticCode` 纳入统一 stable code catalog；当前保持原有 Debug 字符串 code，避免扩大兼容性影响。
- 不移除 `MetaDiagnostic` 类型；它仍是 meta link / validate 内部结构化诊断模型。
- 不改变 `ValidationSeverity = DiagnosticSeverity` 别名；该方向是 `meta -> diagnostic`，不属于本次反向依赖问题。
- 不重构 CLI 顶层 anyhow 错误处理，只保证现有 human / JSON 输出不退化。

## 完成后期望状态

- `src/diagnostic/` 不再引用 `src/meta/`。
- `ConfigForgeError` 只依赖通用 `Diagnostic` / `DiagnosticReport`。
- Meta validation 错误在 loader 边界转换为通用诊断，CLI JSON 输出无需了解 `MetaDiagnostic`。
- 相关 diagnostic、meta、cli、validation 测试均通过。

## 本轮执行结果

- 已完成 `diagnostic -> meta` 反向依赖移除，`src/diagnostic/` 已无 `crate::meta` / `MetaDiagnostic` 引用。
- Meta loader / build 中 `MetaDiagnostic` 到通用 `DiagnosticReport<Diagnostic>` 的转换集中在 `meta` 模块边界。
- CLI JSON 直接输出通用 diagnostic payload，并新增 Meta validation JSON 输出覆盖。
- 已通过 `cargo fmt`、`cargo test meta`、`cargo test cli`、`cargo test validation`、`cargo test diagnostic`。
