# 02. Diagnostic 与 Error 模块详细设计

本文档描述 ConfigForge 的共享诊断基础设施和 crate 级错误桥接。产品层错误展示语义以 `Documentation/Design/05-data-validation.md` 和 `Documentation/Design/06-export-cli.md` 为准；本文档聚焦 Rust 模块边界、诊断数据结构、格式化和依赖约束。

## 2.1 设计目标

Diagnostic 模块为 Meta、Data、Validation、Exporter、Build 等工作模块提供统一诊断基础，并集中承载 crate 级错误 facade：

1. 统一严重级别：Error / Warning。
2. 统一定位信息：source path、table、field、row index、source row number。
3. 统一格式化：不同模块保留自己的 code 和上下文字段，但输出规则一致。
4. 统一报告聚合：支持在多线程任务中收集诊断，并在上层按 error / warning 分流。
5. 保持依赖边界：diagnostic foundation 不依赖任何业务模块；`diagnostic::error` 只作为边界错误 facade 聚合模块诊断，不承载业务逻辑。

`ConfigForgeError` 和 crate `Result<T>` 已归入 `src/diagnostic/error.rs`。crate root 继续 re-export `ConfigForgeError` / `Result`，并保留 `crate::error::{...}` 兼容 re-export，外部调用方不需要直接依赖内部文件路径。

## 2.2 模块结构

```text
src/diagnostic/
  mod.rs              # DiagnosticSeverity / DiagnosticLocation / DiagnosticEntry / DiagnosticReport
  error.rs            # ConfigForgeError 与 crate Result<T>
```

依赖规则：

- `diagnostic::mod` 中的 foundation 类型只依赖标准库和 serde。
- 业务模块实现 `DiagnosticEntry` 接入统一格式化。
- `diagnostic::error` 可以引用稳定诊断类型和具体业务诊断集合，但不能承载业务校验逻辑。

## 2.3 核心类型

### 2.3.1 `DiagnosticSeverity`

```rust
pub enum DiagnosticSeverity {
    Error,
    Warning,
}
```

该类型是跨模块共享的严重级别。Meta 中原先的 `ValidationSeverity` 现在是 `DiagnosticSeverity` 的兼容别名，用于保持 TOML serde 兼容和历史 API 名称稳定。

### 2.3.2 `DiagnosticLocation`

```rust
pub struct DiagnosticLocation {
    pub source_path: Option<PathBuf>,
    pub table: Option<String>,
    pub field: Option<String>,
    pub row_index: Option<usize>,
    pub source_row_number: Option<usize>,
}
```

定位字段含义：

- `source_path`：物理 source 文件路径，例如 CSV。
- `table`：逻辑表名。
- `field`：字段或 field path。
- `row_index`：内部 0-based row index。
- `source_row_number`：用户可见 source row number。展示时优先使用该字段；缺失时可由 `row_index + 1` 兜底。

### 2.3.3 `DiagnosticEntry`

```rust
pub trait DiagnosticEntry {
    fn code(&self) -> String;
    fn severity(&self) -> DiagnosticSeverity;
    fn message(&self) -> &str;
    fn location(&self) -> &DiagnosticLocation;
    fn source(&self) -> Option<&str>;
    fn target(&self) -> Option<&str>;
    fn hint(&self) -> Option<&str>;
}
```

各模块可以保留专属诊断结构：

- Meta 保留 `MetaDiagnosticCode`，`MetaDiagnostic` 实现 `DiagnosticEntry`。
- Validation 保留 `ValidationIssue` 的 validator / target / hint 字段，并实现 `DiagnosticEntry`。
- 后续 Data / Exporter 若需要结构化诊断，应各自定义领域 code，再接入该 trait。

### 2.3.4 `DiagnosticReport<T>`

```rust
pub struct DiagnosticReport<T> {
    pub diagnostics: Vec<T>,
}
```

报告聚合提供：

- `push()`：追加诊断；
- `has_error()`：是否存在 error；
- `warning_messages()`：统一格式化 warning；
- `error_messages()`：统一格式化 error。

该结构可用于多线程任务的局部收集再合并。合并顺序由上层业务模块控制；需要稳定输出时，应在模块诊断结构上实现排序或在上层按 deterministic key 排序。

## 2.4 与 `ConfigForgeError` 的关系

`ConfigForgeError` 是 API 边界上的错误类型：

```rust
pub type Result<T> = std::result::Result<T, ConfigForgeError>;
```

当前错误桥接策略：

| 场景 | 错误形式 |
| --- | --- |
| 文件读写失败 | `ReadFile` / `WriteFile` |
| TOML 解析失败 | `ParseMeta` / `ParseProject` |
| Meta 结构诊断 | `MetaValidation(Vec<MetaDiagnostic>)` |
| CSV 解析失败 | `ParseCsv` |
| Runtime data 序列化失败 | `Serialize` / `SerializeMessagePack` |
| Data validation 失败 | `Validation(String)`，由 `ValidationReport` 聚合得到 |

后续演进方向：

1. 保持 `ConfigForgeError` 作为 public error facade。
2. 各模块内部优先产生结构化诊断。
3. CLI / Build 边界将结构化诊断转换成退出码、warning 输出和 error 输出。
4. 不把所有领域 code 都塞入 `ConfigForgeError` enum，避免 error 层反向拥有业务语义。

## 2.5 多线程诊断收集原则

多线程任务不应直接打印错误。推荐流程：

```text
worker task
  -> Vec<ModuleDiagnostic>
  -> join point stable merge
  -> DiagnosticReport<T>
  -> BuildOutput warnings / ConfigForgeError
```

原则：

- worker 只返回结构化诊断或 `Result<T>`。
- join 点负责保持任务声明顺序，避免并发调度导致输出顺序漂移。
- warning 不阻塞构建；error 阻塞当前阶段。
- 如果某个阶段需要 fail-fast，可以只返回第一个 fatal error，但不能丢失已收集 warning，除非该阶段尚未产生可用报告。

## 2.6 测试边界

Diagnostic 模块测试应覆盖：

- `DiagnosticLocation` 在有 source path / table / row / field 时的展示顺序。
- `DiagnosticReport<T>` 对 error / warning 的分流。
- `MetaDiagnostic` 和 `ValidationIssue` 接入统一格式化后不破坏原有语义。
- `ConfigForgeError::MetaValidation` 使用统一格式化输出诊断列表。
