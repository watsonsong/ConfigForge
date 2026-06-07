# 10. Designer Validation Report 设计

Related docs：[`05-data-validation.md`](05-data-validation.md)、[`06-export-cli.md`](06-export-cli.md)、[`09-builtin-validators.md`](09-builtin-validators.md)、[`../Implementation/02-diagnostic-module-detail-design.md`](../Implementation/02-diagnostic-module-detail-design.md)、[`../Implementation/06-validation-module-detail-design.md`](../Implementation/06-validation-module-detail-design.md)。

本文定义面向策划、QA、制作人和 CI/MR 平台的 **Designer Validation Report**。它不替代现有 CLI/Rust 友好的 diagnostics，而是在现有 `Diagnostic` / `ValidationIssue` / `ValidationReport` 基础上增加更适合人工阅读、筛选、归档和平台消费的报告导出能力。

## 10.1 背景与问题

当前诊断系统已经具备：

- severity：区分 error / warning；
- location：source path、table、field、row index、source row number；
- entry formatting：统一命令行展示；
- report aggregation：validation report 可聚合多条诊断并按稳定 key 排序。

这些能力对 CLI、测试和 Rust 工具链已经足够，但对策划和 QA 仍有明显门槛：

1. **不知道去哪张表哪一行改**：CLI 输出可能包含 table/field/row，但缺少更直观的“源文件 + 行号 + 当前值 + 目标 target 是否可见”。
2. **不知道为什么错**：validator code 对程序稳定，但普通用户需要规则说明、期望值、实际值和修复建议。
3. **不知道该找谁修**：大型项目通常按表、字段、玩法域或 owner 归属不同团队，需要报告能按 owner/domain 聚合。
4. **不知道是否阻塞上线**：warning 与 error 的业务含义需要在报告顶部明确，并展示是否阻塞当前 target 构建。
5. **QA 需要留档与追踪**：本次构建所有 warning/error 需要可下载、可上传平台、可贴 MR comment、可导入表格工具。

P1-1 的目标是把现有结构化 diagnostics 转换为可读、可筛选、可归档的多格式报告。

## 10.2 目标与非目标

### 10.2.1 目标

- 提供 `validate` 命令的报告导出能力：

```bash
config-forge validate \
  --project ConfigForge.toml \
  --target client \
  --report html \
  --output out/report/validation.html
```

- 支持作为 build 的附加产物：

```toml
[[targets.exports]]
type = "validation_report"
output = "out/report"
format = "html"
```

- 支持四类输出：`html`、`markdown`、`json`、`csv`。
- 报告必须能按 severity、table、owner/domain、code、target 等维度阅读或筛选。
- 报告必须保留机器可消费的稳定字段，同时为人工展示补充解释性信息。
- 报告内容必须与 validation 使用同一套 Full DataSet / ExportView 语义，不能为了生成报告重复执行另一套校验逻辑。

### 10.2.2 非目标

- 不在 P1-1 引入在线协作平台、Web 服务或数据库存储。
- 不实现表格回写修改；CSV 输出只用于外部表格工具导入/过滤。
- 不要求所有现有 diagnostics 一次性补齐 span/column/current value/hint；报告模型需要允许字段缺失并明确展示“未提供”。
- 不改变 validation 失败阻塞 build 的语义。报告导出只是展示和归档；error 是否阻塞仍由 validation result 决定。
- 不把报告渲染逻辑耦合到 validator 规则内部；validator 只产出结构化 issue，报告层负责聚合和展示。

## 10.3 入口设计

### 10.3.1 `validate` 命令参数

建议为 `validate` 增加：

| 参数 | 说明 |
| --- | --- |
| `--report <format>` | 可选。启用报告导出，取值 `html` / `markdown` / `json` / `csv`。 |
| `--output <path>` | 报告输出路径。`html` / `markdown` / `json` 推荐为文件路径；`csv` 可为单文件或目录，见 10.8。 |
| `--report-title <title>` | 可选。覆盖报告标题，默认使用 project name + target + timestamp。 |
| `--include-passed` | 可选。默认只输出 warning/error；未来可输出通过项统计。P1-1 可先保留不实现。 |

行为：

- `--report` 未设置时，维持当前 CLI 输出。
- `--report` 设置时，仍正常输出 CLI 摘要；同时把完整 report 写入 `--output`。
- 如果 validation 有 error：
  - 报告文件仍应尽量写出；
  - CLI 退出码仍为失败；
  - 写报告失败时应在 CLI 中额外提示，避免用户误以为没有错误。
- 如果 validation 只有 warning：
  - CLI 退出码成功；
  - 报告顶部显示“Non-blocking warnings”。

示例：

```bash
config-forge validate --project ConfigForge.toml --report markdown --output out/report/validation.md
config-forge validate --project ConfigForge.toml --target client --report json --output out/report/client-validation.json
config-forge validate --project ConfigForge.toml --target client --report csv --output out/report/client-validation.csv
```

### 10.3.2 Build 附加产物

`validation_report` 是一个聚合 export task。它依赖 Meta + Data，并读取当前 target 的 validation result：

```toml
[[targets]]
name = "client"
tags = ["client"]

[[targets.exports]]
type = "validation_report"
output = "out/report"
format = "html"
file_name = "client-validation.html"
```

字段：

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `type` | string | 必填 | 固定为 `validation_report`，可考虑 alias `validation_reporter`，但 canonical 使用 `validation_report`。 |
| `output` | path | 必填 | 输出目录。 |
| `format` | string | `html` | `html` / `markdown` / `json` / `csv`。 |
| `file_name` | string | 按 target + format 默认 | 输出文件名。CSV 如选择多文件模式时作为 basename。 |
| `include_warnings` | bool | `true` | 是否包含 warning。P1-1 默认包含。 |
| `include_errors` | bool | `true` | 是否包含 error。一般不建议关闭。 |
| `group_by` | string[] | `[]` | 可选展示分组优先级，例如 `["severity", "owner", "table"]`。P1-1 可先固定内置分组。 |

Build 语义：

- `validation_report` 不应绕过 validation 阶段；它消费 validation 阶段产生的结构化 report。
- 如果同一 target 有 runtime data/codegen export，且 validation 出现 error，则 runtime/codegen 仍阻塞；但 `validation_report` 应尽量作为失败产物写出，便于用户定位问题。
- 如果 build 还没进入 Data validation（例如 Project/Meta parse 失败），后续可以扩展为 “diagnostic report exporter” 覆盖 Meta/Data/Build diagnostics；P1-1 聚焦 Validation Report，Meta parse 失败可先保持 CLI 输出。

## 10.4 报告数据模型

报告层建议引入一个面向导出的 normalized model，例如 `DesignerValidationReport`。它由现有 `ValidationReport`、Project/Target 上下文、Meta 信息和 Data source trace 补充而来。

### 10.4.1 顶层结构

```text
DesignerValidationReport
  metadata
  summary
  groups
  issues[]
```

`metadata`：

| 字段 | 说明 |
| --- | --- |
| `project_name` | Project Config 中的项目名。 |
| `project_path` | 本次使用的 ConfigForge.toml。 |
| `target` | target 名称；Full DataSet validate 时为空或 `<full>`。 |
| `target_tags` | effective target tags。 |
| `generated_at` | 报告生成时间。 |
| `config_forge_version` | 工具版本，便于 QA 留档。 |
| `input_root` | source data root。 |

`summary`：

| 字段 | 说明 |
| --- | --- |
| `error_count` | error 数量。 |
| `warning_count` | warning 数量。 |
| `blocking` | 是否阻塞当前 validate/build。存在 error 时为 true。 |
| `tables_with_issues` | 涉及表数量。 |
| `owners_with_issues` | 涉及 owner 数量；无法解析 owner 时归入 `<unassigned>`。 |
| `filtered_context_count` | 与 export_when / row_export 过滤相关的 issue 数量；P1-1 可先只记录 issue 自身 target。 |

### 10.4.2 Issue 字段

每条 issue 的导出字段：

| 字段 | 来源 | 说明 |
| --- | --- | --- |
| `id` | 报告层生成 | 稳定序号，例如 `E0001` / `W0003`，用于 QA 沟通。 |
| `severity` | `DiagnosticSeverity` | `error` / `warning`。 |
| `blocking` | severity 推导 | error 为 true，warning 为 false。 |
| `code` | Diagnostic code | 例如 `validator.sum_mismatch`。 |
| `validator` / `source` | `ValidationIssue.validator` / `Diagnostic.source` | validator kind/name 或领域 source。 |
| `message` | Diagnostic message | 面向 CLI 的原始消息。 |
| `friendly_title` | 报告层生成 | 简短人类标题，例如“掉落池权重总和不等于 100”。P1-1 可从 code 映射或 fallback message。 |
| `explanation` | code catalog | 错误原因说明。P1-1 可先对内置 validators 建立 catalog。 |
| `hint` | Diagnostic hint + code catalog | 修复建议；现有 hint 缺失时由 code catalog 兜底。 |
| `table` | location | 逻辑表名。 |
| `field` | location | 字段或 field path。 |
| `row_index` | location | 0-based internal row index。 |
| `source_row_number` | location | 用户可见源文件行号。 |
| `source_path` | location / RowSourceInfo | 源 CSV / source 文件。 |
| `source_column` | source trace | 可选。当前多数诊断尚未提供，后续补齐。 |
| `current_value` | DataSet + field | 可选。能定位到单字段时读取当前 normalized/raw value。 |
| `expected` | validator structured context | 可选。期望规则，例如 `1 <= value <= 999`。P1-1 可先由 message/catalog 推导或留空。 |
| `target` | Diagnostic target | 当前 target tags 字符串。 |
| `export_visibility` | ExportView context | `visible` / `table_filtered` / `row_filtered` / `field_filtered` / `not_applicable`。P1-1 可先输出 `visible` / `not_applicable`，后续补细。 |
| `owner` | Meta annotation / mapping | 表或字段 owner。缺失为 `<unassigned>`。 |
| `domain` | Meta annotation / mapping | 玩法域，例如 combat/economy/ui。缺失为 `<uncategorized>`。 |

### 10.4.3 Owner / Domain 来源

P1-1 可先不修改 Meta schema，采用以下优先级：

1. 未来显式字段：`tables.owner`、`tables.domain`、`tables.fields.owner`、`tables.fields.domain`。
2. 现有 `desc` 中不解析 owner，只用于展示。
3. Project 级可选映射文件或后续配置，例如：

```toml
[validation_report.owners]
classes = "system-design"
class_levels = "combat-design"
items = "economy-design"
```

4. 未配置时归入 `<unassigned>` / `<uncategorized>`。

建议实现顺序：先在报告层支持 owner/domain 字段和 fallback；Meta schema 扩展作为后续小步实现。

## 10.5 分组与阅读体验

### 10.5.1 必备分组

报告至少提供以下视角：

1. **按 severity 分组**：Error 在前，Warning 在后，明确是否阻塞。
2. **按 table 分组**：策划可直接跳到自己负责的表。
3. **按 owner/domain 分组**：QA/制作人可分派给负责人。
4. **按 code 分组**：工程/工具维护者可发现高频规则问题。

HTML 可以通过折叠面板或侧边栏切换视角；Markdown 可按固定顺序输出多个 section；JSON/CSV 通过字段让外部系统自行 group。

### 10.5.2 Summary 区域

报告顶部必须给非工程用户一眼可读的结论：

```text
Validation Report: client
Status: BLOCKED
Errors: 12
Warnings: 7
Target tags: client,shipping
Generated at: 2026-06-07T12:34:56Z
```

并展示 Top N：

- 错误最多的 tables；
- 错误最多的 owners/domains；
- 高频 diagnostic codes；
- 所有 warning 列表入口。

### 10.5.3 Issue 详情卡片

每条 issue 在 HTML/Markdown 中建议展示为：

```text
[E0003] ERROR validator.sum_mismatch
表：drops
字段：weight
分组：pool=common
源文件：data/drops.csv:18
当前值：sum = 97
期望：sum == 100
影响：阻塞 client 构建
原因：掉落池权重总和不满足 sum validator。
建议：检查同一 pool 下的 weight 是否漏填、重复或被 row_export 过滤。
```

如果字段缺失，应显示 `未提供`，而不是省略整个行，避免用户误以为报告损坏。

## 10.6 过滤语义展示

报告需要解释 target-aware validation 中“为什么这条数据参与或不参与”。

P1-1 的最小要求：

- 每条 issue 输出 `target_tags`。
- 每条 issue 输出其 location 是否来自当前 ExportView 的 visible table/row/field。
- 对由 target visibility 造成的配置错误（例如 `required_when` 目标字段在当前 target hidden）必须在 message/hint 中明确建议用户对齐 `export_when`。

后续增强：

- 对被过滤的行/字段输出“未检查原因”统计，例如：
  - table 被 `export_when` 排除；
  - row 被 `row_export` 排除；
  - field 被 `export = false` 或 `export_when` 排除。
- 支持报告中展示“本次 target 实际检查了多少表/行/字段”。

## 10.7 输出格式

### 10.7.1 HTML

面向策划 / QA 的默认推荐格式。

要求：

- 单文件 HTML，默认内联 CSS，方便作为 CI artifact 下载。
- 顶部 summary + 状态色：error 红色、warning 黄色、success 绿色。
- 支持按 severity/table/owner/domain/code 折叠浏览。
- issue 详情包含 source、row、field、current value、expected、hint、target tags。
- 不依赖外网 CDN。

P1-1 可以先生成静态 HTML，不做复杂 JS；如需要搜索/过滤，可使用纯 HTML details/summary 或极少量内联 JS。

### 10.7.2 Markdown

面向 MR / CI comment。

要求：

- 输出 summary 表格。
- Error 和 Warning 分节。
- 每条 issue 用短格式，避免 MR comment 过长。
- 可配置最大 issue 数；超出时提示查看完整 HTML/JSON artifact。

示例：

```markdown
## ConfigForge Validation Report: client

| Status | Errors | Warnings |
| --- | ---: | ---: |
| BLOCKED | 12 | 7 |

### Errors

- `validator.sum_mismatch` `drops.weight` data/drops.csv:18 — pool `common` sum is 97, expected 100.
```

### 10.7.3 JSON

面向平台系统、CI、质量看板和自动分派。

要求：

- 字段稳定、snake_case。
- 保留完整 `metadata`、`summary`、`issues`。
- `issues` 顺序与 ValidationReport stable order 一致。
- 所有可选字段用 `null`，不要省略，便于平台 schema 固定。

建议 schema：

```json
{
  "schema_version": 1,
  "metadata": {
    "project_name": "reference_game",
    "target": "client",
    "target_tags": ["client"],
    "generated_at": "2026-06-07T12:34:56Z"
  },
  "summary": {
    "blocking": true,
    "error_count": 12,
    "warning_count": 7
  },
  "issues": [
    {
      "id": "E0001",
      "severity": "error",
      "blocking": true,
      "code": "validator.sum_mismatch",
      "table": "drops",
      "field": "weight",
      "source_path": "data/drops.csv",
      "source_row_number": 18,
      "current_value": "97",
      "expected": "100",
      "hint": "检查同一 pool 下的 weight 是否漏填、重复或被 target 过滤。"
    }
  ]
}
```

### 10.7.4 CSV

面向表格工具回填、QA 批量筛选。

默认单文件 CSV，列固定：

```text
id,severity,blocking,code,owner,domain,table,field,source_path,source_row_number,current_value,expected,message,hint,target,export_visibility
```

规则：

- 所有字段按 RFC4180 转义。
- `current_value`、`expected` 中的换行应转义为单行展示文本，避免破坏表格。
- source path 使用相对 project root 的路径，便于策划定位。
- 如果未来需要多文件 CSV，可按 `issues.csv`、`summary_by_table.csv`、`summary_by_owner.csv` 拆分；P1-1 先实现单文件即可。

## 10.8 Code Catalog 与修复建议

为了把稳定 code 转换成“为什么错 / 怎么修”，报告层需要一个内置 code catalog。

Catalog 条目：

| 字段 | 说明 |
| --- | --- |
| `code` | 稳定 diagnostic code。 |
| `title` | 人类短标题。 |
| `explanation` | 原因解释。 |
| `default_hint` | 默认修复建议。 |
| `blocking_by_default` | 是否默认阻塞；通常 error=true。 |
| `docs_url` | 可选，指向相关设计或用户手册章节。 |

P1-1 优先覆盖：

- required/type/enum/unique/FK 基础规则；
- `field.range_violation`；
- `validator.regex_mismatch`；
- `validator.string_*`；
- `validator.required_when_*`；
- `validator.compare_*`；
- `validator.sum_*`；
- `validator.monotonic_*`；
- `asset.reference`。

未知 code 仍必须展示，title/explanation/hint fallback 到原始 message/hint。

## 10.9 与现有诊断系统的关系

现有 `Diagnostic` 保持作为跨模块最小公共结构。Designer Report 不应反向污染所有模块，而是采用“增强视图”方式：

```text
ValidationReport / DiagnosticReport
  + Project / Target context
  + LinkedMetaModel
  + DataSet / RowSourceInfo
  + CodeCatalog
  -> DesignerValidationReport
  -> HtmlRenderer / MarkdownRenderer / JsonRenderer / CsvRenderer
```

需要补齐的结构化信息按优先级推进：

1. **source trace**：尽量从 `RowSourceInfo` 补 `source_path`、`source_row_number`。
2. **current value**：当 issue 定位到 table + field + row 时，从 normalized/full row 读取。
3. **hint**：validator issue 优先直接设置 hint；没有时由 code catalog fallback。
4. **expected / actual**：后续可让 validators 输出结构化 context，P1-1 可先从 message/catalog 兜底。
5. **column / span**：CSV reader 后续可记录 header column index；P1-1 不强制。

## 10.10 实现分期建议

### Phase A：最小可用报告

- 新增 report model 和 renderer trait。
- 从 `ValidationReport` 生成 `DesignerValidationReport`。
- 支持 `validate --report html|markdown|json|csv --output ...`。
- HTML/Markdown/CSV/JSON 都输出 summary + issue list。
- 补 source path / row number / current value 的 best-effort enrich。
- 建立最小 code catalog，覆盖现有 validators。

验收：

- 有 error 时仍写报告，CLI 退出失败。
- 只有 warning 时报告显示 non-blocking，CLI 成功。
- target validation 下报告包含 target tags。
- 输出顺序稳定。

### Phase B：Build export task

- 新增 `type = "validation_report"` export task。
- 让 build 在 validation 失败时也尽量输出 validation report artifact。
- 支持 `format`、`file_name`、`include_warnings` 等配置。
- 标准工程增加 HTML report target 或附加 export task。

验收：

- `config-forge build --target client` 可生成 validation report。
- runtime/codegen 因 validation error 阻塞时，report artifact 仍可用于定位。

### Phase C：Owner/domain 与更强 source trace

- 支持 owner/domain 配置或映射。
- CSV source trace 增加 column/header 信息。
- validators 输出更结构化的 expected/actual/context。
- HTML 增加 owner/domain/code 多视角聚合。

验收：

- QA 可按 owner/domain 分派问题。
- CSV 可回填到表格工具。

## 10.11 测试计划

单元测试：

- report model 从 ValidationReport enrichment 的字段映射。
- severity summary、blocking 推导。
- source row/current value best-effort 填充。
- code catalog fallback。
- HTML/Markdown/JSON/CSV renderer snapshot 或关键字段断言。
- CSV 转义：逗号、引号、换行。

集成测试：

- `validate --report html --output ...` 在有 error 时写出文件且返回失败。
- `validate --report json --output ...` 在只有 warning 时写出 warning 且返回成功。
- target validation report 包含 target tags。
- build `validation_report` export task 生成 artifact。
- 并发 build 下报告顺序稳定。

标准工程：

- 为标准工程增加一个正常通过的 validation report 产物示例。
- 另用临时 fixture 制造 warning/error，避免把错误数据提交到标准工程。

## 10.12 当前状态

当前实现状态：**Planned / Not implemented**。

现有基础已经具备 `Diagnostic`、`ValidationIssue`、`ValidationReport`、target-aware validation 和 source row trace；P1-1 的主要工作是把这些结构化信息 enrich 成 designer-facing report model，并实现 CLI/build 两种导出入口与四种 renderer。
