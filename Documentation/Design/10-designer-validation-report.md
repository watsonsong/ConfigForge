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

报告的目标是把现有结构化 diagnostics 转换为可读、可筛选、可归档的多格式报告。

## 10.2 目标与非目标

### 10.2.1 目标

- 提供当前 CLI 主入口的报告导出能力：

```bash
config-forge \
  --project ConfigForge.toml \
  --target client \
  --report html \
  --report-output out/report/validation.html
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

- 当前不引入在线协作平台、Web 服务或数据库存储。
- 不实现表格回写修改；CSV 输出只用于外部表格工具导入/过滤。
- 不要求所有现有 diagnostics 一次性补齐 span/column/current value/hint；报告模型需要允许字段缺失并明确展示“未提供”。
- 不改变 validation 失败阻塞 build 的语义。报告导出只是展示和归档；error 是否阻塞仍由 validation result 决定。
- 不把报告渲染逻辑耦合到 validator 规则内部；validator 只产出结构化 issue，报告层负责聚合和展示。

## 10.3 入口设计

### 10.3.1 CLI 报告参数

CLI 提供以下报告参数。报告路径使用 report-output，与工程产物目录区分；命名和兼容边界见 [CLI 设计](06-export-cli.md#661-命名与兼容边界)。

| 参数 | 说明 |
| --- | --- |
| `--report <format>` | 可选。启用报告导出，取值 `html` / `markdown` / `json` / `csv`。 |
| `--report-output <path>` | 当前为额外验证报告的文件路径，四种格式均输出单个文件；不覆盖工程 export task 的输出目录。 |
| `--report-title <title>` | 可选。覆盖报告标题，默认使用 project name + target + timestamp。 |
| `--include-passed` | 未实现。默认只输出 warning/error；未来可输出通过项统计。 |

行为：

- `--report` 未设置时，维持当前 CLI 输出。
- `--report` 设置时，仍正常输出 CLI 摘要；同时把完整 report 写入 `--report-output`。
- 如果 validation 有 error：
  - 报告文件仍应尽量写出；
  - CLI 退出码仍为失败；
  - 写报告失败时应在 CLI 中额外提示，避免用户误以为没有错误。
- 如果 validation 只有 warning：
  - CLI 退出码成功；
  - 报告顶部显示“Non-blocking warnings”。

示例：

```bash
config-forge --project ConfigForge.toml --target client --report markdown --report-output out/report/validation.md
config-forge --project ConfigForge.toml --target client --report json --report-output out/report/client-validation.json
config-forge --project ConfigForge.toml --target client --report csv --report-output out/report/client-validation.csv
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
| `file_name` | string | `validation-report.<ext>` | 输出文件名。CSV 当前为单文件模式。 |
| `title` | string | 自动标题 | 覆盖报告标题。 |
| `include_warnings` | bool | Reserved / 未开放 | 当前 Project Config 不接受该字段；设置后会作为 unknown option 报错。当前始终包含 warning。 |
| `include_errors` | bool | Reserved / 未开放 | 当前 Project Config 不接受该字段；设置后会作为 unknown option 报错。当前始终包含 error。 |
| `group_by` | string[] | Reserved / 未开放 | 当前 Project Config 不接受该字段；设置后会作为 unknown option 报错。当前固定输出 severity、table、owner、domain、code 分组。 |

Build 语义：

- `validation_report` 不应绕过 validation 阶段；它消费 validation 阶段产生的结构化 report。
- 如果同一 target 有多个 export task，且 validation 出现 error，则 runtime 和其他 data-backed generator 阻塞；MetaOnly codegen/docs 可能已经生成，不回滚；`validation_report` 应尽量作为失败产物写出，便于用户定位问题。
- 失败报告逐个尝试；report 写入失败附加到原始 validation 错误，不替换主错误。所有报告仍通过共享 ArtifactSink，sink 进入失败状态后可能拒绝后续写入。
- 如果 build 还没进入 Data validation（例如 Project/Meta parse 失败），后续可以扩展为 “diagnostic report exporter” 覆盖 Meta/Data/Build diagnostics；当前报告聚焦 Validation Report，Meta parse 失败可先保持 CLI 输出。

## 10.4 报告数据模型

报告层使用面向导出的 normalized model `DesignerValidationReport`，由 `ValidationReport`、Project/Target 上下文、Meta 信息和 Data source trace 补充而来。

### 10.4.1 顶层结构

```text
DesignerValidationReport
  schema_version = 1
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
| `filtered_context_count` | `table_filtered` / `field_filtered` issue 数量；不计 `not_applicable`，不是被过滤 row 总数或已检查记录数。 |

### 10.4.2 Issue 字段

每条 issue 的导出字段：

| 字段 | 来源 | 说明 |
| --- | --- | --- |
| `id` | 报告层生成 | 稳定序号，例如 `E0001` / `W0003`，用于 QA 沟通。 |
| `severity` | `DiagnosticSeverity` | `error` / `warning`。 |
| `blocking` | severity 推导 | error 为 true，warning 为 false。 |
| `code` | Diagnostic code | 例如 `validator.sum_mismatch`。 |
| `validator` | `ValidationIssue.validator` | validator kind/name 或领域 source；JSON issue 没有独立 source 字段。 |
| `message` | Diagnostic message | 面向 CLI 的原始消息。 |
| `friendly_title` | 报告层生成 | 简短人类标题，例如“掉落池权重总和不等于 100”。由 code 映射或 fallback message。 |
| `explanation` | code catalog | 错误原因说明。使用现有 code catalog。 |
| `hint` | Diagnostic hint + code catalog | 修复建议；现有 hint 缺失时由 code catalog 兜底。 |
| `table` | location | 逻辑表名。 |
| `field` | location | 字段或 field path。 |
| `row_index` | location | 0-based internal row index。 |
| `source_row_number` | location | 用户可见源文件行号。 |
| `source_path` | location / RowSourceInfo | 源 CSV / XLSX 文件路径；project root 内相对化，根外保留原路径。关联位置采用同一规则。 |
| `source_sheet` | location / cell source | XLSX 工作表名称；CSV 缺省省略。 |
| `source_column` | source trace | 可选。CSV/XLSX-backed data validation 高价值错误已可从 cell source trace 补齐；其他 producer 可留空。 |
| `current_value` | producer structured context | 可选。required/type/duplicate/FK/artifact/generator 等已迁移路径由 producer 显式填充；尚未迁移 producer 可留空。 |
| `schema_metadata` | linked Meta enrichment | builtin 类型和字段选项等 schema 提示。 |
| `expected` | producer structured context / code catalog | 可选。期望规则，例如 `int`、`target table/index key`、`output root 内唯一路径`。 |
| `target` | Diagnostic target | 当前 target tags 字符串。 |
| `export_visibility` | linked Meta + target tags | 当前使用 `visible` / `table_filtered` / `field_filtered` / `not_applicable`。检查表、字段及可解析的 struct / fragment 点分路径；缺失 schema/target 或路径无法解析时为 `not_applicable`。`visible` 表示 schema 可见，不保证 row 可见；`row_filtered` 尚未实现。 |
| `record_key` | producer / raw key policy | 主位置所在记录的 raw key，没有可用 key 时为空。 |
| `context` | producer | 有序 map，保留 group、asset_kind、asset_ref、target_index 等上下文。 |
| `related_locations` | producer / 声明来源 | 关联记录或声明位置；JSON 保留结构，HTML/Markdown/CSV 展示位置文本。 |
| `owner` | producer diagnostic | 当前由 producer 提供，缺失为 `<unassigned>`；Meta/Project 映射为 Planned。 |
| `domain` | producer diagnostic | 例如 combat/economy/ui，缺失为 `<uncategorized>`；Meta/Project 映射为 Planned。 |

### 10.4.3 Owner / Domain 来源

当前 owner/domain 来自 producer 并提供 fallback；以下 Meta/Project 映射仅为 Planned，不是当前配置：

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

报告层 owner/domain 字段和 fallback 已存在；Meta schema / Project 映射为后续扩展。

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

当前要求：

- 报告 metadata 输出 target_tags，issue 的 target 字段保留 producer 上下文。
- 每条 issue 从 linked schema 与 target tags 判断表和字段可见性；当前未把 row_export 的行可见性纳入报告模型，不把 `visible` 当成“该行已检查”的证明。
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

当前生成静态 HTML，不做复杂 JS；如需要搜索/过滤，可使用纯 HTML details/summary 或极少量内联 JS。

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
- 可选字段使用 `null`；为兼容 CSV，缺失的 `source_sheet` 省略。`context` / `related_locations` 缺省为空对象 / 数组。
- 顶层 `schema_version = 1`；消费者应忽略兼容新增字段，破坏性结构变更提升版本。issue ID 是单份报告确定排序后的序号，不是跨构建固定标识。

schema 示例（省略部分字段）：

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
id,severity,blocking,code,friendly_title,message,hint,table,field,row_index,source_row_number,source_column,source_path,source_sheet,current_value,expected,schema_metadata,explanation,target,owner,domain,record_key,context,related_locations
```

规则：

- 所有字段按 RFC4180 转义。
- `current_value`、`expected` 中的换行应转义为单行展示文本，避免破坏表格。
- source path 在 project root 内使用相对路径，外部文件保留原路径；context 和 related locations 在 CSV 中合并为可读文本，需要无损结构时使用 JSON。
- 如果未来需要多文件 CSV，可按 `issues.csv`、`summary_by_table.csv`、`summary_by_owner.csv` 拆分；当前输出单文件。

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

当前 code catalog 覆盖：

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

结构化信息来源按优先级推进：

1. **source trace**：优先使用 `DiagnosticLocation`；CSV reader / DataTable cell source trace 已提供 source path、source row、source column。
2. **current value / expected**：优先由 producer 填充 `ValidationIssue` / `Diagnostic` enrichment；report renderer 不再从 message 猜测业务含义。
3. **hint / explanation**：producer 显式字段优先；没有时由 code catalog fallback。
4. **owner / domain**：producer 字段优先；当前缺失时 fallback 到 `<unassigned>` / `<uncategorized>`。
5. **未迁移 producer**：字段保持空值/null，报告仍保留稳定 schema。

## 10.10 实现边界

CLI 附加报告与 `validation_report` export task 均已实现；同一 normalized model 渲染 HTML / Markdown / JSON / CSV。validation error 时尽量写出报告后返回失败；配置、source 或 resolver 等前置失败不保证形成 Designer Validation Report。

Owner/domain 的项目映射、剩余 producer enrichment 和可配置分组仍为后续扩展。完整边界见 [10.12 当前状态](#1012-当前状态)。

## 10.11 测试计划

单元测试：

- report model 从 ValidationReport enrichment 的字段映射。
- severity summary、blocking 推导。
- source row/source column/current value/expected 字段级断言。
- code catalog fallback。
- HTML/Markdown/JSON/CSV renderer snapshot 或关键字段断言。
- CSV 转义：逗号、引号、换行。

集成测试：

- `config-forge --report html --report-output ...` 在有 error 时写出文件且返回失败。
- `config-forge --report json --report-output ...` 在只有 warning 时写出 warning 且返回成功。
- target validation report 包含 target tags。
- build `validation_report` export task 生成 artifact。
- 并发 build 下报告顺序稳定。

标准工程：

- 为标准工程增加一个正常通过的 validation report 产物示例。
- 另用临时 fixture 制造 warning/error，避免把错误数据提交到标准工程。

## 10.12 当前状态

当前实现状态：**Implemented MVP**。

已实现内容：

- `src/validation/report.rs` 提供 `DesignerValidationReport`、`ReportMetadata`、`ReportSummary`、`DesignerValidationIssue`、`IssueGroup` 和 `DesignerReportFormat`。
- 报告模型可从 `ValidationReport` / `DiagnosticReport` enrichment 生成，保留 stable code、severity、table、field、row/source row/source column、target、hint、current_value、expected、explanation、owner 和 domain，并生成稳定 issue id（如 `E0001` / `W0001`）。
- 顶层提供 `schema_version = 1`，保留 record_key、context、related_locations；四种格式共用字段，source path 按 project root 归一化。
- 内置 code catalog 已覆盖当前常用 validator code 和基础 required/type/enum/unique/FK/asset code；未知 code fallback 到原始 message/hint。
- 支持 HTML / Markdown / JSON / CSV 四种 renderer，输出共享同一个 normalized report model；Markdown/CSV/HTML 已展示 source_column、current_value、expected 等结构化字段，JSON 保留完整字段。
- CLI 支持 `--report <html|markdown|json|csv>`、`--report-output <path>`、`--report-title <title>`；validation error 时仍尽量写出报告，再保持失败退出码。
- Build 支持 `[[targets.exports]] type = "validation_report"`，作为 data-backed 附加产物消费 validation 结果；validation error 阻塞 runtime/codegen 时仍尽量写出 report artifact。
- 已有单元测试和 CLI integration tests 覆盖 report model、renderer enrichment、CLI 失败写报告、CLI machine-readable diagnostic JSON、build HTML report artifact。

当前边界：

- CLI 当前是顶层参数形式，不存在独立 `validate` 子命令；文档示例使用当前可执行接口。
- owner/domain 当前主要依赖 producer 字段；未提供时使用 `<unassigned>` / `<uncategorized>` fallback，Project 级 owner/domain 映射未实现。
- `current_value`、`expected`、`source_column` 已在核心 data validation、generator/artifact 结构化诊断和 report renderer 中贯通；asset、部分 builtin validator、code artifact 等尚未迁移 producer 仍可能为空字段。
- Build export task 当前支持 `format`、`file_name`、`title`；`include_warnings`、`include_errors`、`group_by` 尚未实现，当前始终输出全部 issue 并使用固定分组。
- `export_visibility` 基于 schema + target tags 判断；未统计 row_filtered、已检查数量和全部未参与检查的记录，无法解析的路径为 not_applicable。issue ID 只在单份报告内稳定。
- validation 和报告写入同时失败时保留两者，但不保证所有报告都写入成功；共享 sink 失败约束、声明定位和 producer 的剩余边界见 [Diagnostic 契约](../Implementation/02-diagnostic-module-detail-design.md#29-当前实现缺口与演进约束)。
