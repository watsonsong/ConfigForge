# 02. Diagnostic 与 Error 模块详细设计

本文档描述 ConfigForge 的共享诊断基础设施和 crate 级错误桥接。产品层错误展示语义以 `Documentation/Design/05-data-validation.md` 和 `Documentation/Design/06-export-cli.md` 为准；本文档聚焦 Rust 模块边界、诊断数据结构、格式化和依赖约束。

## 2.1 设计目标

Diagnostic 模块为 Meta、Data、Validation、Exporter、Build 等工作模块提供统一诊断基础，并集中承载 crate 级错误 facade：

1. 统一严重级别：Error / Warning。
2. 统一定位信息：source path、table、field、row index、source row number、source column。
3. 统一 enrichment 字段：source、target、hint、current value、expected、explanation、owner、domain。
4. 统一格式化：不同模块保留自己的 code 和上下文字段，但输出规则一致。
5. 统一报告聚合：支持在多线程任务中收集诊断，并在上层按 error / warning 分流。
6. 保持依赖边界：diagnostic foundation 不依赖任何业务模块；`diagnostic::error` 只作为边界错误 facade 聚合模块诊断，不承载业务逻辑。

`ConfigForgeError` 和 crate `Result<T>` 已归入 `src/diagnostic/error.rs`。crate root 继续 re-export `ConfigForgeError` / `Result`，并保留 `crate::error::{...}` 兼容 re-export，外部调用方不需要直接依赖内部文件路径。

## 2.2 模块结构

```text
src/diagnostic/
  mod.rs              # DiagnosticSeverity / DiagnosticLocation / Diagnostic / DiagnosticEntry / DiagnosticReport
  code.rs             # 内置 diagnostic code 常量与 metadata catalog
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
    pub source_column_number: Option<usize>,
    pub source_column_name: Option<String>,
    pub column_index: Option<usize>,
}
```

定位字段含义：

- `source_path`：物理 source 文件路径，例如 CSV。
- `table`：逻辑表名。
- `field`：字段或 field path。
- `row_index`：内部 0-based row index。
- `source_row_number`：用户可见 source row number。展示时优先使用该字段；缺失时可由 `row_index + 1` 兜底。
- `source_column_number`：用户可见 source column number。展示时低于 column name 优先级。
- `source_column_name`：用户可见 source column/header 名称。CSV cell source trace 已在 DataTable 层保留该字段。
- `column_index`：内部 0-based column index。展示时可由 `column_index + 1` 兜底。

逻辑定位与物理定位分开维护：`table` / `field` / `row_index` 描述 ConfigForge 逻辑视图，`source_path` / `source_row_number` / `source_column_*` 描述原始输入位置。非表格类诊断（如 generator option、artifact path）不强塞 row/column，而通过 `source`、`target`、`current_value`、`expected` 与 message 表达上下文。

### 2.3.3 `Diagnostic`

```rust
pub struct Diagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub location: DiagnosticLocation,
    pub source: Option<String>,
    pub target: Option<String>,
    pub hint: Option<String>,
    pub current_value: Option<String>,
    pub expected: Option<String>,
    pub explanation: Option<String>,
    pub domain: Option<String>,
    pub owner: Option<String>,
}
```

`Diagnostic` 是跨 Data / Validation / Artifact / Generator / Build 边界的结构化诊断载体。`Diagnostic::new()` 保持最小构造入口；补充上下文使用 builder-style API，例如 `with_source()`、`with_target()`、`with_hint()`、`with_current_value()`、`with_expected()`、`with_explanation()`、`with_domain()`、`with_owner()`。

字段语义：

- `code`：稳定机器可读 code。内置 code 使用 `src/diagnostic/code.rs` 常量；外部扩展仍可使用自定义字符串。
- `source`：诊断生产者或模块来源，例如 `validation.type`、`validation.foreign_key`、`artifact.writer`、`generator.registry`。不要与 `location.source_path` 混淆。
- `target`：相关 build target / export target；非 target-aware 诊断可为空。
- `current_value` / `expected`：机器可读或短文本形式的当前值与期望规则，优先供 CLI JSON、Designer Report、CI annotation 消费。
- `explanation` / `hint`：原因说明与修复建议；缺失时上层 report 可从 code catalog fallback。
- `owner` / `domain`：用于 QA 分派和分组；当前主要由 producer 显式传入或 report fallback。

### 2.3.4 `DiagnosticEntry`

```rust
pub trait DiagnosticEntry {
    fn code(&self) -> String;
    fn severity(&self) -> DiagnosticSeverity;
    fn message(&self) -> &str;
    fn location(&self) -> &DiagnosticLocation;
    fn source(&self) -> Option<&str>;
    fn target(&self) -> Option<&str>;
    fn hint(&self) -> Option<&str>;
    fn current_value(&self) -> Option<&str>;
    fn expected(&self) -> Option<&str>;
    fn explanation(&self) -> Option<&str>;
    fn domain(&self) -> Option<&str>;
    fn owner(&self) -> Option<&str>;
}
```

所有 enrichment getter 都有 default implementation，旧的 `MetaDiagnostic` 等实现不需要一次性补齐字段即可保持编译。`Diagnostic::from_entry()` 会完整复制这些字段，用于 Meta / Validation 专属诊断向通用 report 或 CLI JSON 桥接。

各模块可以保留专属诊断结构：

- Meta 保留 `MetaDiagnosticCode`，`MetaDiagnostic` 实现 `DiagnosticEntry`。
- Validation 保留 `ValidationIssue` 的 validator / target / hint 字段，并实现 `DiagnosticEntry`。
- Data / Artifact / Generator 的高价值边界错误优先直接构造通用 `Diagnostic`，并通过 `ConfigForgeError::from_diagnostic()` 返回。

### 2.3.5 `DiagnosticReport<T>`

```rust
pub struct DiagnosticReport<T> {
    pub diagnostics: Vec<T>,
}
```

报告聚合提供：

- `new()` / `from_one()`：构造空报告或单条报告；
- `push()` / `extend()`：追加诊断；
- `is_empty()`：是否为空；
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
| Meta 结构诊断 | `MetaValidation(DiagnosticReport<Diagnostic>)` |
| CSV 解析失败 | `ParseCsv` |
| Runtime data 序列化失败 | `Serialize` / `SerializeMessagePack` |
| Legacy 裸业务错误 | `Validation(String)`，仅作为尚未迁移边界的 fallback |
| 结构化业务诊断 | `ValidationReport(DiagnosticReport<Diagnostic>)` |

后续演进方向：

1. 保持 `ConfigForgeError` 作为 public error facade。
2. 各模块内部优先产生结构化诊断。
3. 新增 ConfigForge 业务错误优先使用 `ConfigForgeError::from_diagnostic()`、`from_diagnostics()` 或 `from_diagnostic_report()`，不要新增裸 `Validation(format!(...))`。
4. CLI / Build 边界将结构化诊断转换成退出码、warning 输出、人类 error 输出和 `--diagnostic-format json` 机器输出。
5. 不把所有领域 code 都塞入 `ConfigForgeError` enum，避免 error 层反向拥有业务语义。

## 2.5 Diagnostic Code Catalog

`src/diagnostic/code.rs` 维护内置 code 常量和 `DiagnosticCodeMetadata` 数组。code storage/API 仍是 `String`，不引入强制 enum，以便外部 generator / validator 扩展自定义 code。

当前 catalog 覆盖：

- Generator / Build 边界：unknown type、ambiguous type、invalid / unsupported option、unsupported data input、runtime contract mismatch。
- Artifact path safety：path escape、normalized collision、case-insensitive collision、symlink alias collision、hardlink alias collision。
- Meta 结构诊断：`CF_META_*` stable codes 覆盖 `MetaDiagnosticCode` 的所有当前变体；Meta 内部仍保留 `MetaDiagnosticCode` enum，边界输出通过 `stable_code()` 转为 catalog code，并使用 `source = "meta.validate"`、`domain = "meta"`、catalog explanation / hint enrich JSON payload。
- Data high-value validation：invalid type、missing required field、invalid enum、invalid foreign key、invalid asset ref、duplicate key。
- Build：target not found、input required、export task failed。
- Legacy validator/data code：`field.range_violation`、`validator.*`、`field.*`、`foreign_key.*`、`asset.reference` 等以 deprecated metadata 形式保留，保证报告层能继续给出 title/explanation/hint fallback。

新增内置 code 的规则：优先复用已有 code；只有语义不同才新增。新增 code 必须补 metadata，并至少有 catalog test 或 producer test 覆盖。

## 2.6 多线程诊断收集原则

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

## 2.7 CLI / Machine-readable 输出

默认 CLI human 输出保持简洁：`error: validation failed: <formatted diagnostics>`，每条 formatted diagnostic 优先展示 source、severity、code、message、location、target、hint。

`--diagnostic-format json` 面向 CI / IDE / 平台消费，在失败时向 stderr 输出稳定顶层结构：

```json
{
  "status": "error",
  "message": "validation failed: ...",
  "diagnostics": [
    {
      "code": "CF_DATA_INVALID_TYPE",
      "severity": "error",
      "message": "...",
      "location": { "table": "items", "field": "level", "source_column_name": "level" },
      "source": "validation.type",
      "current_value": "abc",
      "expected": "int"
    },
    {
      "code": "CF_META_DUPLICATE_TABLE_NAME",
      "severity": "error",
      "message": "...",
      "location": { "table": "items" },
      "source": "meta.validate",
      "domain": "meta",
      "hint": "...",
      "explanation": "..."
    }
  ]
}
```

当错误不是 `ConfigForgeError::ValidationReport` / `MetaValidation` 时，`diagnostics` 可以为空，但 `status` 与 `message` 仍稳定输出。

## 2.8 测试边界

Diagnostic 模块测试应覆盖：

- `DiagnosticLocation` 在有 source path / table / row / column / field 时的展示顺序。
- `DiagnosticReport<T>` 对 error / warning 的分流。
- `Diagnostic::from_entry()`、`MetaDiagnostic` 和 `ValidationIssue` 接入统一格式化后不破坏 enrichment 字段。
- `ConfigForgeError::MetaValidation` 使用统一格式化输出诊断列表。
- CLI JSON 输出保留 code、severity、location、source、target、current_value、expected、hint 等字段。
- 高价值 producer 测试优先断言 code / severity / location / current_value / expected / hint，再断言 message 片段。
