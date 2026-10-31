# 02. Diagnostic 与 Error 模块详细设计

本文档描述 ConfigForge 的共享诊断基础设施和 crate 级错误桥接。产品层错误展示语义以 `Documentation/Design/05-data-validation.md` 和 `Documentation/Design/06-export-cli.md` 为准；本文档聚焦 Rust 模块边界、诊断数据结构、格式化和依赖约束。

## 2.1 设计目标

Diagnostic 模块为 Meta、Data、Validation、Exporter、Build 等工作模块提供统一诊断基础，并集中承载 crate 级错误 facade：

1. 统一严重级别：Error / Warning。
2. 统一定位信息：source path、可选 source sheet、table、field、row index、source row number、source column。
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
    pub source_sheet: Option<String>,
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

- `source_path`：物理 source 文件路径，例如 CSV/XLSX。
- `source_sheet`：XLSX 工作表名；独立于路径，CSV 不提供。
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
    pub related_locations: Vec<DiagnosticLocation>,
    pub record_key: Option<String>,
    pub context: BTreeMap<String, String>,
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
- `related_locations`：关联声明或记录位置。重复 key 指向首次出现的记录，FK 指向 source 字段声明和 target 表声明，sum 指向参与该组的记录。
- `record_key`：主位置所在记录的 raw key；沿用显式 key policy，不做 typed canonicalization。
- `context`：独立于 field path 的结构化上下文，使用有序 map，例如 sum 的 `group`、FK 的 `target_index`、Asset 的 `asset_kind` / `asset_ref`。

### 2.3.4 `DiagnosticEntry`

```rust
pub trait DiagnosticEntry {
    fn related_locations(&self) -> &[DiagnosticLocation];
    fn record_key(&self) -> Option<&str>;
    fn context(&self) -> Option<&BTreeMap<String, String>>;
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

`ConfigForgeError::diagnostics()` 覆盖全部错误变体，文件、解析、序列化和 legacy 错误也有稳定 code。TOML 文件解析失败使用 parser span 的起始位置补充 1-based 行列，CSV 使用 parser 的物理行号；内存 Meta 字符串解析失败没有文件位置。`with_diagnostics()` 保留主错误后追加诊断，报告 / trace 写入失败不得覆盖原始 validation 错误。`BuildOutput.diagnostics` 保留结构化 warning，`warnings` 保持兼容的 human 字符串；后续 export、sink finish 或 trace 失败继续携带已收集诊断。

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
- 文件与解析：`CF_IO_READ_FAILED`、`CF_IO_WRITE_FAILED`、`CF_PROJECT_PARSE_FAILED`、`CF_META_PARSE_FAILED`、`CF_DATA_PARSE_CSV_FAILED`、`CF_OUTPUT_SERIALIZE_FAILED`。
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

`--diagnostic-format json` 面向 CI / IDE / 平台消费，在成功、业务失败和参数解析失败时均向 stderr 输出同一顶层结构。成功使用 `status = "success"` 并保留 warning；失败使用 `status = "error"`。stdout 的成功 / dry-run 信息仍是人类可读文本：

```json
{
  "schema_version": 1,
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

全部 `ConfigForgeError` 变体都会产出结构化诊断。参数解析失败在原始 argv 中识别 `--diagnostic-format json` 或 `--diagnostic-format=json`，输出 `CF_BUILD_INVALID_OPTION` 并保持退出码 2；help/version 保持正常文本与退出码 0。解析后的无效参数组合和业务错误保持退出码 1。JSON 新增字段采用兼容扩展，消费者应忽略未知字段；破坏性变更需提升 `schema_version`。

## 2.8 测试边界

Diagnostic 模块测试应覆盖：

- `DiagnosticLocation` 在有 source path / table / row / column / field 时的展示顺序。
- `DiagnosticReport<T>` 对 error / warning 的分流。
- `Diagnostic::from_entry()`、`MetaDiagnostic` 和 `ValidationIssue` 接入统一格式化后不破坏 enrichment 字段。
- `ConfigForgeError::MetaValidation` 使用统一格式化输出诊断列表。
- CLI JSON 输出保留 code、severity、location、source、target、current_value、expected、hint 等字段。
- 高价值 producer 测试优先断言 code / severity / location / current_value / expected / hint，再断言 message 片段。

XLSX 的 `DiagnosticLocation.source_sheet` 独立于 source_path，缺省时序列化省略以兼容 CSV。物理行列采用 1-based，human 输出显示 sheet；DesignerValidationIssue 同步保留 source_sheet，四种 renderer 展示工作表定位。禁止把 sheet 拼入真实文件路径。

## 2.9 当前实现缺口与演进约束

当前契约与真实边界：

| 能力 | 已实现 | 仍有限制 |
| --- | --- | --- |
| 错误聚合 | 主错误与 report / trace 错误并存；失败报告逐个尝试；export / finish 失败保留已有 warning。 | 共享 ArtifactSink 失败后拒绝后续写入，报告不能绕过路径规划和 sink 状态。 |
| 机器输出 | CLI 成功、业务失败、参数错误使用 schema version 1；I/O 与 parse 错误有 code / 可提取位置。 | stdout 仍保留 human 运行信息；JSON 从 stderr 读取。 |
| Producer | Asset regex warning 进入 validation；regex/sum 提供 current/expected；asset ref/kind、group 独立放入 context。 | 旧 `resolve_for_target()` 只返回值；需要 warning 的调用者使用 `resolve_for_target_with_diagnostics()`。 |
| 来源与关联 | Project imports / standalone Meta merge 保留声明 name 的文件、行列；duplicate、FK、sum 有关联位置，行级 issue 可补 raw record key。 | 不是完整 TOML token span；index / validator / fragment 的独立属性位置仍可能回退到所属声明。程序构造模型可无物理来源。 |
| Designer report | schema version 1，四格式传递新增上下文；根内 source path 相对化；基于 linked schema 与 target tags 判断 table/field 可见性。 | 不统计 row_filtered 或已检查行数；无法解析的路径标记 not_applicable。E/W ID 只在单份报告内稳定。 |

回归覆盖主错误与附加失败并存、warning 后 export 失败、I/O/parse/argv JSON、Asset warning、多文件声明、重复记录、FK/sum 上下文、四格式和可见性。诊断转换逐字段补充缺失 location，保留 producer 已提供的坐标。新增测试优先断言结构化字段，不仅检查 message。
