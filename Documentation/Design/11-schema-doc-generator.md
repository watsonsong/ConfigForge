# 11. Schema / Table 文档生成器设计

Related docs：[`04-meta-model.md`](04-meta-model.md)、[`05-data-validation.md`](05-data-validation.md)、[`06-export-cli.md`](06-export-cli.md)、[`08-asset-validation-and-manifest.md`](08-asset-validation-and-manifest.md)、[`10-designer-validation-report.md`](10-designer-validation-report.md)、[`../Implementation/08-exporter-module-detail-design.md`](../Implementation/08-exporter-module-detail-design.md)、[`../Implementation/10-build-module-detail-design.md`](../Implementation/10-build-module-detail-design.md)。

本文定义 P1-3 **Schema / Table 文档生成器**。它面向策划、QA、服务端、客户端和平台协作场景，把 Project / Meta / Target 中已有的表结构、字段说明、索引、外键、导出可见性、资产和国际化信息导出为稳定、可审阅、可归档的表结构说明。

## 11.1 背景与问题

多语言 codegen 已经解决“程序如何类型安全访问配置”的问题，但团队协作还需要“人如何理解配置结构”。当前用户经常需要跨 Meta TOML、数据表、生成代码和口头约定回答：

- 这张表做什么？
- 字段含义是什么？
- 字段是否必填？默认值是什么？
- 哪些字段会导出到当前 target？哪些是 client-only / server-only？
- index 有哪些？是否唯一？primary index 是哪个？
- foreign key 指向哪里？是否会生成 linked helper？
- 哪些字段会进入 POT？
- 哪些字段是 asset？asset kind 和资源规则是什么？
- ObjectTable、fragment、struct/list 字段如何书写和导出？

这些信息已经分散存在于 MetaModel、target-aware ExportView、validation/codegen IR 和 exporter 规则中。Schema / Table 文档生成器的目标是把它们以文档产物形式稳定输出，减少人工维护 wiki、表结构说明和 MR 注释的重复成本。

## 11.2 目标与非目标

### 11.2.1 目标

- 新增 export task：

```toml
[[targets.exports]]
type = "schema_doc"
output = "out/docs"
format = "markdown"
```

- 默认 Markdown 生成一个文档目录：

```text
out/docs/
  index.md
  items.md
  skills.md
  class_levels.md
```

- 也可使用 `format = "html"` 生成自包含静态 HTML 文档：

```text
out/docs/
  index.html
  items.html
  skills.html
  class_levels.html
```

- Markdown 便于提交 MR、上传 Lark / Confluence、放入 CI artifact 或随版本归档；HTML 便于直接浏览，使用内联 CSS，不依赖外部 CDN、外部 CSS 或外部 JS。
- 文档内容必须来自当前 linked Meta 与当前 target 的 effective target tags，避免和实际 export/codegen 可见性漂移。
- 每张 table 至少输出 Fields、Indexes、Relations 三个区域。
- 字段表必须展示字段名、类型、required、default、export visibility、description。
- 文档必须标识 client-only / server-only / all / hidden 等 target 可见性。
- 文档必须展示 FK 关系、asset 字段、text/POT 相关字段和 index 信息。
- 输出顺序必须 deterministic：table 按 Meta 顺序或可配置排序，字段、索引、关系默认保持 Meta 声明顺序。

### 11.2.2 非目标

- P1-3 不生成 runtime data，不读取或扫描数据行；它是 schema / metadata 文档，不是数据内容预览。
- P1-3 不替代 Designer Validation Report。Validation Report 展示当前数据问题；Schema Doc 展示表结构契约。
- P1-3 不要求在线发布、权限管理、Lark / Confluence API 写入或数据库存储；Markdown 产物可由外部 CI 上传。
- P1-3 不强制引入新的 Meta schema。已有 `desc`、`required`、`default`、`export_when`、`asset_kind`、FK/index 配置应先被完整利用。
- P1-3 不保证生成语言级 API 文档；多语言 codegen 注释仍由 codegen backend 控制。
- JSON Schema、自定义 schema JSON、Mermaid ER 图和 FK 图当前不支持；本生成器当前只维护 Markdown 与 HTML 两类文档 renderer。

## 11.3 Export Task 设计

### 11.3.1 基础配置

`schema_doc` 是一个 **MetaOnly** export task：它只依赖 linked Meta、project config 和 target tags，不依赖 source data root，也不执行 data validation。这样可用于 code-only / docs-only target，并能在数据尚未准备好时提前生成表结构文档。

```toml
[[targets]]
name = "docs"
tags = ["client"]

[[targets.exports]]
name = "schema"
type = "schema_doc"
output = "out/docs"
format = "markdown"
```

字段：

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `type` | string | 必填 | 固定为 `schema_doc`。 |
| `output` | path | 必填 | 输出目录。Markdown 下包含 `index.md` 和每表一个 `*.md`；HTML 下包含 `index.html` 和每表一个 `*.html`。 |
| `format` | string | `markdown` | 当前实现支持 `markdown` / `html`。未知格式显式报错；JSON Schema / Mermaid 不支持。 |
| `index_file` | string | format-aware：Markdown 为 `index.md`，HTML 为 `index.html` | 文档入口文件名，不允许包含路径分隔符；HTML 模式要求 `.html` 后缀。 |
| `file_name_style` | string | `table_name` | 单表文件命名策略。当前固定为 canonical table name + format 扩展名（`.md` 或 `.html`）。 |
| `title` | string | project name 或 `ConfigForge Schema` | index 顶部标题。 |
| `include_hidden` | bool | `false` | 是否输出当前 target 下不可见的 table/field/index，并标记为 hidden。 |
| `include_internal` | bool | `false` | 是否输出 `export = false` 的 tool-only/support 字段。 |
| `include_struct_details` | bool | `true` | 是否展开 struct / fragment 字段结构说明。 |
| `include_asset_rules` | bool | `true` | 是否输出 asset kind、路径规则、大小写/命名约束摘要。 |
| `include_i18n` | bool | `true` | 是否标识会进入 POT 的 `text` 字段。 |

当前 MVP 实现 `format = "markdown"` / `format = "html"`、`index_file`、`title`；`include_hidden` / `include_internal` / `include_struct_details` / `include_asset_rules` / `include_i18n` 作为模型层选项存在，但尚未开放为 Project Config 配置项。用户在 export task 中设置未支持字段时会显式报错，不能静默忽略。

### 11.3.2 与 target tags 的关系

Schema Doc 必须明确区分“Meta 中存在”和“当前 target 会导出”：

- table `export_when` 不匹配当前 effective tags 时：
  - `include_hidden = false`：不生成该 table 的单表页面，只在 index 的隐藏统计中可选展示数量；
  - `include_hidden = true`：生成页面并标记 `Hidden for this target`。
- field `export = false`：默认不作为 runtime 字段展示；如 `include_internal = true`，输出并标记 `internal / not exported`。
- field `export_when` 不匹配当前 target：按 hidden 字段处理。
- index 中引用了隐藏字段或自身 `export_when` 不匹配：在当前 target 的 index 表中不展示；`include_hidden = true` 时展示并标记不可用原因。
- row-level `row_export` 是数据行可见性，不改变 schema。文档应在 table summary 中标记“Rows may be filtered by row_export”，但不扫描数据。

`--tag` 追加的 CLI extra tags 必须进入 effective target tags，并影响 schema doc 的可见性标记，与 runtime export/codegen 保持一致。

## 11.4 输出结构

### 11.4.1 Index 文档

`index.md` 是面向团队入口的目录页，建议结构：

```markdown
# ConfigForge Schema: client

Project: game_config
Target: client
Tags: client, shipping
Generated at: 2026-06-07T12:34:56Z

## Tables

| Table | Kind | Description | Fields | Indexes | Relations | Visibility |
| --- | --- | --- | ---: | ---: | ---: | --- |
| [items](items.md) | collection | 道具表 | 12 | 2 | 1 | all |
| [settings](settings.md) | object | 全局配置 | 8 | 0 | 0 | server |
```

Index 应包含：

- project / target / tags / generated_at；
- 所有可见 table 的链接；
- table kind：CollectionTable / ObjectTable；
- table description；
- 字段数量、index 数量、relation 数量；
- 可见性摘要；
- 可选 Top-level sections：Asset tables、Text/POT fields、Relation overview。

### 11.4.2 单表文档

单表文档建议结构：

```markdown
# items

道具表。

| Property | Value |
| --- | --- |
| Kind | CollectionTable |
| Source | items.csv |
| Visibility | all |
| Row export | no |
| Primary index | primary(id) |

## Fields

| Field | Type | Required | Default | Export | Description |
| --- | --- | --- | --- | --- | --- |
| id | int | yes | - | all | 道具 ID |
| name | text | yes | - | client | 道具名 |
| icon | asset | no | - | client | 图标资源 |

## Indexes

| Name | Fields | Unique | Primary | Export |
| --- | --- | --- | --- | --- |
| primary | id | yes | yes | all |

## Relations

| Field | Target | Cardinality | Linked Helper | Description |
| --- | --- | --- | --- | --- |
| class_id | classes.id | many-to-one | class | 关联职业 |
```

### 11.4.2.1 HTML 输出

`format = "html"` 使用与 Markdown 相同的 `SchemaDoc` normalized model，默认生成多页静态 HTML：一个 `index.html` 与每张可见 table 的 `<table>.html`。HTML renderer 当前使用内联 CSS，不引入外部 JS / CSS / CDN；所有 table name、field path、desc、validator summary、asset rules 等用户可控文本必须 HTML escape。

HTML index 页包含：

- 顶部 metadata：title、project、target、effective tags、generated_at；
- summary cards：table、field、index、relation、asset field、text field 数量；
- Tables 总览表：Table、Kind、Description、Fields、Indexes、Relations、Visibility；
- 可选跨表 sections：Asset fields、Text / POT fields、Relation overview。

HTML 单表页包含：

- 返回 index 链接、table name、description、kind、source、visibility、row_export、primary index；
- Fields、Indexes、Relations；
- Assets、Text / POT、Validators 摘要；
- visibility / severity 等短状态使用 badge 展示。

HTML 输出路径与 Markdown 一样通过 ArtifactSink 统一写入和检查冲突；单表链接只使用同目录相对文件名，例如 `items.html`。

### 11.4.3 Fields 区域

字段表至少包含：

| 列 | 说明 |
| --- | --- |
| `Field` | canonical field path；struct child 可用 `parent.child` 展示。 |
| `Type` | primitive / enum / struct / list / asset / text 等用户可理解类型。 |
| `Required` | `yes` / `no`，结合 default 解释“缺失时是否报错”。 |
| `Default` | Meta default；缺失显示 `-`。 |
| `Export` | `all`、target tag 表达式、`hidden`、`internal`。 |
| `Description` | 来自 `desc`；缺失显示 `-`。 |

建议补充扩展列或字段详情块：

- enum 值列表；
- validator 摘要，例如 `range: 1..10`、`regex: ^[a-z_]+$`；
- `text` 字段是否会进入 POT；
- asset 字段的 `asset_kind`、允许扩展名、路径规则；
- struct/list 元素类型与子字段；
- target-specific 可见性说明。

### 11.4.4 Indexes 区域

Indexes 区域展示当前 target 中可用的索引：

| 列 | 说明 |
| --- | --- |
| `Name` | index 名称；primary index 标记为 `primary`。 |
| `Fields` | index fields，复合索引用逗号连接。 |
| `Unique` | 是否唯一。 |
| `Primary` | 是否 primary。 |
| `Export` | 当前 target 可见性。 |

ObjectTable 默认不支持 indexes，应输出 `No indexes`，并在 table summary 中标记 `ObjectTable`。

### 11.4.5 Relations 区域

Relations 区域展示 FK 和生成 helper 相关信息：

| 列 | 说明 |
| --- | --- |
| `Field` | FK source field 或 source fields。 |
| `Target` | `target_table.target_field` 或 `target_table(index_name)`。 |
| `Cardinality` | `many-to-one`、`one-to-one`、`list-to-many` 等展示口径。 |
| `Linked Helper` | `linked_field` 或 generated relation helper 名称；未声明显示 `-`。 |
| `Description` | FK 说明或 fallback。 |

字段级 FK、table-level FK、ObjectTable 作为 FK source、list scalar FK 都应能映射到统一 relation 行。target table / target index 在当前 target 不可见时，若 include_hidden=false 则隐藏；若 include_hidden=true 则标记为 `not available for this target`。

### 11.4.6 Asset 与 POT 摘要

为解决跨团队常见问题，单表文档应额外提供两个可选摘要：

```markdown
## Assets

| Field | Asset Kind | Required | Export | Rules |
| --- | --- | --- | --- | --- |
| icon | ui_icon | no | client | .png/.webp, prefix UI_ |

## Text / POT

| Field | Export | POT |
| --- | --- | --- |
| name | client | yes |
```

P1-3 可先只标记字段是否为 `asset` / `text`，asset kind 与 POT 是否进入当前聚合 exporter 的精确判定可按现有设计逐步增强：

- `text` 字段在当前 target 可见时，默认认为可被 `gettext_pot` 聚合 exporter 抽取；
- `asset` 字段在当前 target 可见时，展示 `asset_kind` 和 Project/Meta 中可静态解析的规则摘要；
- 不读取实际数据，因此不列出具体 asset reference 或 text value。

## 11.5 Markdown 渲染规则

Markdown 是 P1-3 默认格式。

要求：

- 所有用户输入内容必须做 Markdown table escape，至少处理 `|`、换行和控制字符。
- 表和字段链接使用相对路径，保证整个 `out/docs/` 目录可移动。
- 空值统一展示为 `-`，不省略列。
- 布尔值面向非工程用户显示 `yes` / `no`。
- target visibility 使用短文本：`all`、`client`、`server`、`client,shipping`、`hidden`、`internal`。
- 生成文件头必须包含 generated at 和 tool 版本，便于 QA 留档。
- 文件名必须 deterministic，并拒绝 path separator / parent escape，避免 artifact 写出越界。

## 11.6 数据模型建议

实现层建议先构建 normalized documentation model，再由 renderer 输出，避免 Markdown / HTML / JSON Schema 各自解释 Meta：

```text
SchemaDoc
  metadata
  tables[]

SchemaDocMetadata
  project_name
  project_path
  target
  target_tags[]
  generated_at
  config_forge_version

SchemaTableDoc
  name
  kind
  description
  source_summary
  visibility
  row_export_summary
  fields[]
  indexes[]
  relations[]
  asset_fields[]
  text_fields[]

SchemaFieldDoc
  path
  type_display
  required
  default_display
  visibility
  description
  validators[]
  asset_kind
  enters_pot
```

Builder 输入：

- Project config：project name、path、copyright、input root；
- LinkedMetaModel：tables、fields、indexes、foreign keys、asset kinds、desc、defaults、validators；
- TargetTags：当前 effective tags；
- Export visibility helper：复用 runtime export / codegen 使用的 target-aware 判定逻辑。

输出模型必须只包含 renderer 所需的字符串或结构化摘要，不应把大量 Rust 内部引用泄漏给 renderer。

## 11.7 与现有模块的关系

### 11.7.1 与 Exporter Registry

`schema_doc` 是 requirements 只包含 Meta 的 doc artifact generator。当前实现已直接实现 `ArtifactGenerator` 并由 `GeneratorRegistry` 注册；它不再注册到 `codegen::builtin_generators()`，也不再实现 code-specific `CodeGenerator` API。registry 行为如下：

- requirements：linked Meta + effective target tags + project / task context；`needs_data = false`，因此进入 `MetaOnly` 分支；
- output planning：`index.md` + visible table markdown files；
- artifact conflict check：纳入现有路径冲突检测；
- writer：使用 `ArtifactSink`，支持 direct / temp_commit / dry_run。

如果现有 runtime data exporter trait 只服务 data-backed artifact，不应为了 schema doc 读取 data；`CodeGenerator` trait 只服务 language code artifact，也不要求 schema doc 实现 code-specific naming / formatting / line comment API。统一 `ArtifactGenerator` facade + doc renderer 表达 schema doc。

### 11.7.2 与 Codegen

Schema Doc 不复用某个语言 codegen backend 的字符串输出，但可以复用 CodeModel IR 中已经整理好的 target-aware tables / fields / indexes / relations 语义。需要注意：

- 文档展示应使用 Meta canonical name，而不是语言命名策略后的符号名；
- relation helper 名称可以同时展示 Meta `linked_field` 和 generated helper 名称；
- codegen 不支持的 target/field 组合，Schema Doc 仍应解释其 schema 可见性，而不是静默省略。

### 11.7.3 与 Designer Validation Report

两者都服务非纯程序访问场景，但职责不同：

| 能力 | Designer Validation Report | Schema / Table Doc |
| --- | --- | --- |
| 输入 | ValidationReport / DiagnosticReport | Project / LinkedMeta / TargetTags |
| 是否依赖数据 | 是 | 否 |
| 关注点 | 当前数据有哪些错误/警告 | 表结构契约是什么 |
| 典型输出 | validation.html / validation.json | index.md / table.md |
| 阻塞语义 | validation error 仍阻塞 build | docs 生成不改变 validation gate |

### 11.7.4 与 Asset Manifest / POT

Schema Doc 只输出字段级 schema 信息，不输出聚合数据清单：

- Asset Manifest 输出当前数据中实际引用了哪些资源；Schema Doc 输出哪些字段是 asset、asset kind 是什么、静态规则是什么。
- Gettext POT 输出当前数据中实际待翻译文本；Schema Doc 输出哪些 `text` 字段在当前 target 可见、理论上会被 POT exporter 处理。

## 11.8 错误处理与确定性

错误处理要求：

- 未知 `format` 必须报错，不 fallback。
- `output` 缺失、`index_file` 非纯文件名、单表文件名冲突、输出路径冲突必须报错。
- 当前未实现的配置项如果被用户设置，必须显式报错或按文档已实现语义处理，不能静默忽略。
- Meta link/validate 失败时不生成 schema doc；文档必须基于已 link 的一致 Meta。
- docs-only target 不需要 `project.input`，也不因为数据缺失失败。

确定性要求：

- table 顺序默认为 MetaModel linked table 顺序；
- fields / indexes / foreign keys 默认为声明顺序；
- relation overview 如跨表聚合，应按 source table、source field、target table、target field 稳定排序；
- generated_at 是唯一允许随时间变化的字段；如未来需要 snapshot 测试，可支持 fixed timestamp 注入。

## 11.9 示例

### 11.9.1 配置示例

```toml
[[targets]]
name = "client_docs"
tags = ["client"]

[[targets.exports]]
type = "schema_doc"
output = "out/docs/client"
format = "markdown"
title = "Client Config Schema"
```

### 11.9.2 `index.md` 示例

```markdown
# Client Config Schema

| Property | Value |
| --- | --- |
| Project | demo_game |
| Target | client_docs |
| Tags | client |

## Tables

| Table | Kind | Description | Fields | Indexes | Relations | Visibility |
| --- | --- | --- | ---: | ---: | ---: | --- |
| [items](items.md) | CollectionTable | 道具表 | 3 | 1 | 1 | client |
| [classes](classes.md) | CollectionTable | 职业表 | 2 | 1 | 0 | all |
```

### 11.9.3 `items.md` 示例

```markdown
# items

道具表。

## Fields

| Field | Type | Required | Default | Export | Description |
| --- | --- | --- | --- | --- | --- |
| id | int | yes | - | all | 道具 ID |
| name | text | yes | - | client | 道具名 |
| icon | asset(ui_icon) | no | - | client | 图标资源 |

## Indexes

| Name | Fields | Unique | Primary | Export |
| --- | --- | --- | --- | --- |
| primary | id | yes | yes | all |

## Relations

| Field | Target | Cardinality | Linked Helper | Description |
| --- | --- | --- | --- | --- |
| class_id | classes.id | many-to-one | class | 关联职业 |

## Assets

| Field | Asset Kind | Required | Export | Rules |
| --- | --- | --- | --- | --- |
| icon | ui_icon | no | client | prefix UI_, extensions .png/.webp |

## Text / POT

| Field | Export | POT |
| --- | --- | --- |
| name | client | yes |
```

## 11.10 实现分期建议

### Phase A：Markdown MVP（已实现 MVP）

- 新增 `schema_doc` export type，dependency 为 MetaOnly。
- 构建 `SchemaDoc` normalized model。
- 输出 `index.md` 和每张可见 table 的 `*.md`。
- 覆盖 Fields / Indexes / Relations 基础表格。
- 支持 target-aware table/field/index visibility。
- 单元测试覆盖 Markdown escape、文件名冲突、docs-only target、target filtering。

### Phase B：资产、POT 与复杂类型增强

- 展示 asset kind 规则摘要。
- 标记 text 字段是否会进入 POT。
- 展开 struct / fragment / list 字段结构。
- 展示 built-in validators 摘要。
- 增加 relation overview 和跨表 FK 索引页。

### Phase C：HTML 输出格式（已实现 MVP）

- 多页静态 HTML：`index.html` 与每表 `<table>.html`。
- 与 Markdown 共享 `SchemaDoc` normalized model 和 artifact path planning。
- 内联 CSS、自包含页面、无外部 JS/CSS/CDN。
- Index 页包含 summary cards、Tables 总览、Asset fields、Text / POT fields、Relation overview。
- 单表页包含 Fields / Indexes / Relations / Assets / Text-POT / Validators。
- 对用户可控文本统一 HTML escape。
- CLI integration 覆盖 docs-only target、dry-run artifact plan 和实际写入。

### Phase D：更多平台集成

- HTML 单页 bundle、搜索、过滤、排序等增强交互。
- JSON Schema 或自定义 schema JSON 如需支持，需另行设计；当前不支持。
- Mermaid ER 图和 FK 关系图如需支持，需另行设计；当前不支持。
- 生成适配 Lark / Confluence 的 Markdown 方言。
- 支持 owner/domain 或表负责人映射后，在 index 中按负责人分组。

## 11.11 测试计划

单元测试：

- SchemaDoc builder：空 Meta、单表、多表、ObjectTable、struct/list、asset/text 字段；
- target visibility：table hidden、field hidden、index hidden、include_hidden；
- FK relation：field-level FK、table-level FK、list FK、linked_field；
- Markdown renderer：表格 escape、空值、中文描述、换行、链接；
- HTML renderer：HTML escape、静态 shell、index summary、单表 sections、相对链接；
- path planning：index file、table file、文件名冲突、路径逃逸拒绝。

集成测试：

- `type = "schema_doc" format = "markdown"` 生成 `index.md` 与单表 `.md` 文档；
- `type = "schema_doc" format = "html"` 生成 `index.html` 与单表 `.html` 文档；
- docs-only target 不需要 `project.input`；
- 与 runtime data/codegen export 同 target 时路径冲突能提前报错；
- target tags 变化会影响文档可见性；
- dry-run / temp-commit 写入模式下 artifact plan 与实际写入一致。

文档回归：

- 标准工程 fixture 增加 schema_doc export task 示例；
- Design / Implementation / HTML 阅读版在实现后同步状态；
- Markdown golden/snapshot 使用固定 generated_at 或忽略时间字段。

## 11.12 当前状态

当前实现状态：**Implemented MVP**。

已实现内容：

- `schema_doc` 已迁入 `src/generator/schema_doc/`，提供 `SchemaDoc` normalized model、`SchemaDocOptions`、`SchemaDocExportConfig`、`SchemaDocFormat`、Markdown / HTML renderer 和 ArtifactSink 写入入口。
- Build 支持 `[[targets.exports]] type = "schema_doc"`，当前作为注册到 `GeneratorRegistry` 的 direct `ArtifactGenerator` 运行；它读取 linked Meta、Project info 与 target tags 后输出 artifact，不属于 `DataFormatRegistry`，docs-only target 不需要 `project.input`。
- Markdown 输出 `index.md` 与每张当前 target 可见 table 的 `<table>.md`；HTML 输出 `index.html` 与每表 `<table>.html`。
- 单表文档包含 Fields、Indexes、Relations，并补充 Assets、Text / POT、Validators 摘要；HTML index 额外提供 summary cards、asset/text/relation 跨表 overview。
- Builder 使用 linked Meta 与 current target tags 计算 table / field / index / FK visibility；默认过滤当前 target 不可见或 `export = false` 的内容。
- 输出路径通过 `ArtifactSink` 写入，支持 direct / temp-commit / dry-run 的统一 artifact 行为；`index_file` 和 table 文件名会拒绝非法文件名和重复 normalized path。
- 已有单元测试与 CLI integration tests 覆盖 docs-only target、Markdown / HTML 输出、target filtering、unknown format、artifact 写入、dry-run plan 和路径规划。

当前边界：

- 当前只支持 `format = "markdown"` 与 `format = "html"`；`json_schema`、`mermaid` 不支持并会报错。
- export task 当前只开放 `format`、`index_file`、`title`；`include_hidden`、`include_internal`、`include_struct_details`、`include_asset_rules`、`include_i18n` 尚未开放到 Project Config。
- Asset rules 摘要只展示可静态解析的 `extensions`、prefix/suffix、require existing、allow/deny path 等基础规则，不读取实际资源引用。
- Text / POT 摘要只标记当前 target 可见 `text` 字段理论上会被 POT exporter 处理，不读取具体文本值。
- Struct / fragment 子字段已有基础展开；复杂平台化输出、owner/domain 分组、HTML 单页 bundle / 交互搜索、ER 图和 Lark / Confluence 发布仍是后续扩展。
- 当后续新增 JSON Schema、Mermaid ER 等“只读 Meta 并输出 artifact”的任务时，优先按统一 artifact generator + requirements + doc facet 扩展，而不是在 build 中增加特殊分支，也不要继续把非代码文档任务塞进 code-specific `CodeGenerator` trait。
