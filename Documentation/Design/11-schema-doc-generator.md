# 11. Schema / Table 文档生成器设计

## 文档导航

- [11.1 背景与问题](#111-背景与问题)
- [11.2 目标与非目标](#112-目标与非目标)
- [11.3 Export Task 设计](#113-export-task-设计)
- [11.4 输出结构](#114-输出结构)
- [11.5 Markdown 渲染规则](#115-markdown-渲染规则)
- [11.6 数据模型建议](#116-数据模型建议)
- [11.7 与现有模块的关系](#117-与现有模块的关系)
- [11.8 错误处理与确定性](#118-错误处理与确定性)
- [11.9 示例](#119-示例)
- [11.10 当前契约与扩展](#1110-当前契约与扩展)
- [11.11 测试计划](#1111-测试计划)
- [11.12 当前状态](#1112-当前状态)

Related docs：[`04-meta-model.md`](04-meta-model.md)、[`05-data-validation.md`](05-data-validation.md)、[`06-export-cli.md`](06-export-cli.md)、[`08-asset-validation-and-manifest.md`](08-asset-validation-and-manifest.md)、[`10-designer-validation-report.md`](10-designer-validation-report.md)、[`../Implementation/08-exporter-module-detail-design.md`](../Implementation/08-exporter-module-detail-design.md)、[`../Implementation/10-build-module-detail-design.md`](../Implementation/10-build-module-detail-design.md)。

本文定义 **Schema / Table 文档生成器**。它面向策划、QA、服务端、客户端和平台协作场景，把 Project / Meta / Target 中已有的表结构、字段说明、索引、外键、导出可见性、资产和国际化信息导出为稳定、可审阅、可归档的表结构说明。

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

- 不生成 runtime data，不读取或扫描数据行；它是 schema / metadata 文档，不是数据内容预览。
- 不替代 Designer Validation Report。Validation Report 展示当前数据问题；Schema Doc 展示表结构契约。
- 不要求在线发布、权限管理、Lark / Confluence API 写入或数据库存储；Markdown 产物可由外部 CI 上传。
- 不强制引入新的 Meta schema。已有 `desc`、`required`、`default`、`export_when`、`asset_kind`、FK/index 配置应先被完整利用。
- 不保证生成语言级 API 文档；多语言 codegen 注释仍由 codegen backend 控制。
- 本生成器当前只维护 Markdown 与 HTML 两类文档 renderer；其它输出格式不属于当前 schema / table docs 契约。

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
| `format` | string | `markdown` | 当前实现支持 `markdown` / `html`。未知格式显式报错。 |
| `index_file` | string | format-aware：Markdown 为 `index.md`，HTML 为 `index.html` | 文档入口文件名，不允许包含路径分隔符；HTML 模式要求 `.html` 后缀。 |
| `file_name_style` | string | Reserved / 未开放 | 单表文件命名策略的未来选项。当前 Project Config 不接受该字段；设置后会作为 unknown option 报错。当前固定为 canonical table name + format 扩展名（`.md` 或 `.html`）。 |
| `title` | string | project name 或 `ConfigForge Schema` | index 顶部标题。 |
| `include_hidden` | bool | `false` | 是否输出当前 target 下不可见的 table/field/index，并标记为 hidden。 |
| `include_internal` | bool | `false` | 是否输出 `export = false` 的 tool-only/support 字段。 |
| `include_struct_details` | bool | `true` | 是否展开 struct / fragment 字段结构说明。 |
| `include_asset_rules` | bool | `true` | 是否输出 asset kind、路径规则、大小写/命名约束摘要。 |
| `include_i18n` | bool | `true` | 是否标识会进入 POT 的 `text` 字段。 |
| `include_diagrams` / `diagram_detail` / `diagram_format` | - | 未开放 | 当前关系图、source map、fragment composition、Index/FK 与 Asset/Text flow 图例默认随 Markdown / HTML 输出启用，不提供额外配置开关。 |

当前实现支持 `format = "markdown"` / `format = "html"`、`index_file`、`title`、`include_hidden`、`include_internal`、`include_struct_details`、`include_asset_rules`、`include_i18n`。HTML/CSS 图例与 Markdown 结构摘要默认启用，暂不开放 `include_diagrams`、`diagram_detail` 或 `diagram_format`，以避免配置面过早膨胀。用户在 export task 中设置未支持字段时会显式报错，不能静默忽略。

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
- Relationship graph、Source map；
- 可选跨表 sections：Asset fields、Text / POT fields、Relation overview，并在相关区域展示 Index/FK contract 与 Asset/Text flow legend。

HTML 单表页包含：

- 返回 index 链接、table name、description、kind、source、visibility、row_export、primary index；
- Table neighborhood、Source map、Fragment composition；
- Fields、Indexes、Relations；
- Assets、Text / POT、Validators 摘要；
- visibility / severity 等短状态使用 badge 展示。

HTML 输出路径与 Markdown 一样通过 ArtifactSink 统一写入和检查冲突；单表链接只使用同目录相对文件名，例如 `items.html`。

### 11.4.2.2 HTML 阅读体验优化方向

HTML 是更适合“快速浏览、跨表跳转、给非程序同学解释结构”的格式，因此后续优化不应只把 Markdown 表格换成 HTML 表格，还应提供更清晰的信息层级和视觉提示。

页面开头 summary 区建议从当前简单 `dl` 逐步升级为“紧凑元信息条 + 统计卡片 + 关键关系入口”：

- 元信息条使用单行或响应式横排展示，避免 `Project` 与 `standard_reference_game` 这类 key/value 被默认块级布局拆成上下两行。推荐渲染为 `Project: standard_reference_game`、`Target: client`、`Tags: client, shipping` 这种 inline item；小屏幕下允许自然换行，但同一 key/value 内部应保持不拆行。
- `Generated at`、tool version 等审计信息视觉权重应低于 Project / Target / Tags，可使用 muted text，避免干扰阅读主线。
- summary cards 除数量外应提供解释性 subtitle，例如 `Relations: foreign-key helpers`、`Asset fields: schema-level asset refs`，让非工程用户知道统计口径。
- 单表页开头应先回答“这张表是什么、从哪里来、和谁有关”：table name、desc、kind、source、primary index、outgoing relation count、incoming relation count、fragment count 应尽量集中在首屏。
- 页内锚点建议固定为 `#fields`、`#indexes`、`#relations`、`#assets`、`#text-pot`、`#validators`，summary 区提供快速跳转 chips。

推荐的 HTML summary 信息结构：

```text
Header
  Title
  Meta bar
    Project: standard_reference_game   Target: client   Tags: client   Generated: 2026-06-14T...
  Summary cards
    Tables / Fields / Indexes / Relations / Asset fields / Text fields
  Quick links
    Tables | Relationship graph | Source map | Assets | Text/POT
```

样式约束：

- 保持静态、自包含、无外部 JS/CSS/CDN；优先使用 CSS grid/flex 完成响应式排版。
- key/value 使用 `.meta-bar`、`.meta-item`、`.meta-key`、`.meta-value` 等语义化 class；`white-space: nowrap` 可用于避免同一 key/value 内部换行。
- 如果未来增加可折叠区域，应优先使用原生 `<details>/<summary>`，避免引入脚本依赖。
- 所有链接、badge、节点标题、边标签仍必须 HTML escape。

### 11.4.2.3 面向人类理解的图例与关系可视化

Schema Doc 后续应提供一组“可读图例”（legend / diagram），帮助读者在不打开 Meta TOML 的情况下快速理解表与表、表与 source、表与 fragment、字段与 FK helper 的关系。这些图例属于 schema-level 结构图，不读取具体数据行。

推荐优先实现以下辅助功能：

1. **全局表关系图（Schema Relationship Graph）**
   - 节点：table；节点内展示 table kind、主要 source、字段数、primary index。
   - 边：foreign key；边标签展示 `source_field -> target_table.target_index`，并标记 cardinality：`many-to-one`、`list-to-many`、`object-to-one` 等。
   - 方向：从 FK source table 指向 target table。
   - 视觉编码：CollectionTable / ObjectTable 使用不同节点样式；hidden / target-filtered relation 使用虚线或灰色；list FK 使用双线或 `[]` 标记。
   - 用途：快速回答“这张表依赖哪些表”“删除/修改某个 index 会影响谁”。

2. **单表上下游关系图（Table Neighborhood Graph）**
   - 每张单表页展示当前 table 周边一跳关系：outgoing FK、incoming FK、self-reference。
   - 当前 table 高亮居中；被引用表和引用当前表的 source table 分列展示。
   - 每条边链接到对应 table 页面和 relation 行锚点。
   - 对于 `items.rarity -> rarity_levels.by_id` 这类关系，应能直观看到 `items` 通过 `rarity_id` 依赖 `rarity_levels`，并显示 generated helper 名称 `rarity`。

3. **Source Map（数据来源图）**
   - 节点：table 与 source file/provider。
   - 边：source feeds table；边标签展示 fragments / override source / export_when。
   - 支持展示 polymorphic table：例如 `materials.csv`、`weapons.csv`、`armors.csv` 都汇入 `items`，并在边上标记 active fragments：`stack`、`attributes`、`skill`。
   - ObjectTable source 使用独立样式，提示它是 key/value rows，不是普通 collection rows。
   - 用途：快速回答“这张表由哪些 CSV/XLSX 组成”“某个 fragment 来源在哪个文件”。

4. **Fragment Composition 图**
   - 节点：base table、fragment、fragment fields。
   - 表达 fragment 是“条件存在的结构片段”，而不是独立 table。
   - 对 polymorphic item 表尤其有价值：读者能看到 `items` 基础字段与 `stack` / `attributes` / `skill` fragment 的组成关系。
   - 可用紧凑卡片表达：`items` base card + fragment cards，每个 fragment card 列出字段数量和关键字段。

5. **Index & FK Contract 图例**
   - 在关系表旁增加 legend，解释 `primary`、`unique`、`target_index`、`linked_field/helper`、`allow_empty`、`list FK` 等概念。
   - 在 relation 行中用 badge 标记：`primary target`、`generated helper`、`list source`、`optional source`。
   - 用途：减少非程序读者对 `by_id`、`by_class_level` 等 index 名称的理解成本。

6. **Asset / Text Flow 图例**
   - Asset field flow：`table.field(asset_kind) -> asset manifest exporter`，说明 schema doc 只列字段规则，不列实际资源引用。
   - Text/POT flow：`text field -> gettext_pot`，说明哪些字段理论上进入 POT。
   - 用途：帮助策划、美术、LOC 同学理解资源校验和本地化抽取边界。

图形渲染建议分层推进：

- **当前：HTML/CSS 图例（已实现）**。不引入 graph layout，只用卡片、箭头、两列/三列布局表达一跳关系和 source composition；实现成本低且 deterministic。
- **未来：内联 SVG 图（可选增强）**。如果 HTML/CSS card 在大型项目中可读性不足，可由 SchemaDoc model 生成节点/边并使用简单稳定布局（按 table order 分层或网格排列）输出内联 SVG；仍保持自包含、可复制、无外部依赖。该能力不作为进入 Stable 的前置条件。

图例的数据模型不应由 renderer 直接扫描 `SchemaTableDoc` 字符串拼接，建议在 builder 层补充结构化视图：

```text
SchemaDoc
  relationship_graph
    nodes[]: table_name, kind, source_summary, visibility, file_name
    edges[]: source_table, source_field, target_table, target_index, cardinality, linked_helper, visibility
  source_graph
    table_nodes[]
    source_nodes[]: provider, path, row_layout, export_when
    edges[]: source_path -> table_name, fragments[], override, visibility
  fragment_graphs[]
    table_name
    fragments[]: name, fields[], visibility, source_paths[]
```

这些结构化图模型可同时服务 HTML 图例、Markdown 文本图与测试断言，避免每个 renderer 重新解释 Meta。

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

可先只标记字段是否为 `asset` / `text`，asset kind 与 POT 是否进入当前聚合 exporter 的精确判定可按现有设计逐步增强：

- `text` 字段在当前 target 可见时，默认认为可被 `gettext_pot` 聚合 exporter 抽取；
- `asset` 字段在当前 target 可见时，展示 `asset_kind` 和 Project/Meta 中可静态解析的规则摘要；
- 不读取实际数据，因此不列出具体 asset reference 或 text value。

## 11.5 Markdown 渲染规则

Markdown 是 默认格式。

要求：

- 所有用户输入内容必须做 Markdown table escape，至少处理 `|`、换行和控制字符。
- 表和字段链接使用相对路径，保证整个 `out/docs/` 目录可移动。
- 空值统一展示为 `-`，不省略列。
- 布尔值面向非工程用户显示 `yes` / `no`。
- target visibility 使用短文本：`all`、`client`、`server`、`client,shipping`、`hidden`、`internal`。
- 生成文件头必须包含 generated at 和 tool 版本，便于 QA 留档。
- 文件名必须 deterministic，并拒绝 path separator / parent escape，避免 artifact 写出越界。

## 11.6 数据模型建议

实现层建议先构建 normalized documentation model，再由 Markdown / HTML renderer 输出，避免不同 renderer 各自解释 Meta：

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

### 11.7.1 与 Generator Registry

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

## 11.10 当前契约与扩展

Markdown、HTML、复杂类型/asset/POT/validator 摘要、target-aware 可见性与关系/source/fragment 图例均属于当前 Stable 契约，见 [11.12 当前状态](#1112-当前状态)。

JSON Schema、Mermaid/DOT、搜索过滤、Lark / Confluence 发布和 owner/domain 分组为后续扩展；它们不属于当前 export task 的可配置选项。

## 11.11 测试计划

单元测试：

- SchemaDoc builder 已覆盖空 Meta、单表、多表、ObjectTable、struct/list、asset/text 字段；
- target visibility 已覆盖 table hidden、field hidden、index hidden、relation hidden、`export = false` internal 内容，以及 `include_hidden` / `include_internal` 开启后的稳定标记；
- FK relation 已覆盖 field-level FK、table-level FK、list FK、linked_field、自引用与 ObjectTable source；
- deterministic order 已覆盖大型 schema 的 table、relationship graph、source graph、table neighborhood、fragment graph 与相对链接；
- Markdown renderer 已覆盖表格 escape、空值、中文描述、换行、链接、hidden/internal 文案和 detail toggle；
- HTML renderer 已覆盖 HTML escape、自包含静态 shell、index summary、单表 sections、相对链接、legend badges、无外部 JS/CSS/CDN；
- path planning 已覆盖 index file、table file、文件名冲突、路径逃逸拒绝和 table/index 输出冲突。

集成测试：

- `type = "schema_doc" format = "markdown"` 生成 `index.md` 与单表 `.md` 文档；
- `type = "schema_doc" format = "html"` 生成 `index.html` 与单表 `.html` 文档；
- docs-only target 不需要 `project.input`；
- 与 runtime data/codegen export 同 target 时路径冲突能提前报错；
- target tags 变化会影响文档可见性，`include_hidden` / `include_internal` 可显式输出 hidden/internal 内容；
- dry-run / temp-commit / direct 写入模式下 artifact plan 与实际写入一致。

文档回归：

- 标准工程 fixture 已纳入 Markdown / HTML schema_doc export task 示例；
- 标准工程输出回归覆盖 relationship overview、source map、fragment composition、Asset/Text flow legend；
- Design / Implementation 文档同步标记 Markdown / HTML schema docs 为 Stable；
- `generated_at` 是唯一允许随构建时间变化的字段；测试断言主体内容和稳定路径，不把 JSON Schema / Mermaid 等后续格式作为 Stable 前置条件。

## 11.12 当前状态

当前实现状态：**Stable**。

已实现内容：

- `schema_doc` 已迁入 `src/generator/schema_doc/`，提供 `SchemaDoc` normalized model、`SchemaDocOptions`、`SchemaDocExportConfig`、`SchemaDocFormat`、Markdown / HTML renderer 和 ArtifactSink 写入入口。
- Build 支持 `[[targets.exports]] type = "schema_doc"`，当前作为注册到 `GeneratorRegistry` 的 direct `ArtifactGenerator` 运行；它读取 linked Meta、Project info 与 target tags 后输出 artifact，不属于 `DataFormatRegistry`，docs-only target 不需要 `project.input`。
- Markdown 输出 `index.md` 与每张当前 target 可见 table 的 `<table>.md`；HTML 输出 `index.html` 与每表 `<table>.html`。
- 单表文档包含 Fields、Indexes、Relations，并补充 Assets、Text / POT、Validators、Source map、Fragments 摘要；HTML index 额外提供 summary cards、asset/text/relation 跨表 overview，HTML table 页提供 relationship/source/fragment 等纯静态图例。
- Builder 使用 linked Meta 与 current target tags 计算 table / field / index / FK visibility；默认过滤当前 target 不可见或 `export = false` 的内容。`include_hidden` / `include_internal` 开启后，对应 table / field / index / relation / source / fragment / asset / text / validator 内容进入 normalized model 并标记 hidden/internal，renderer 不重新解释 Meta。
- `SchemaDocExportConfig` 稳定开放 `format`、`index_file`、`title`、`include_hidden`、`include_internal`、`include_struct_details`、`include_asset_rules`、`include_i18n`；未开放的 `include_diagrams` / `diagram_detail` / `diagram_format` 等字段会作为 unknown option 显式报错。
- deterministic 输出已纳入回归：table、field、index、relation、source graph、relationship graph、table neighborhood、fragment graph 和相对链接顺序稳定；`generated_at` 是唯一允许随时间变化的字段。
- 输出路径通过 `ArtifactSink` 写入，支持 direct / temp-commit / dry-run 的统一 artifact 行为；`index_file` 和 table 文件名会拒绝非法文件名和重复 normalized path。
- 单元测试、CLI integration tests 和标准工程 fixture 回归覆盖 docs-only target、Markdown / HTML 输出、target filtering、hidden/internal include、unknown format/option、artifact 写入、dry-run plan、路径规划、fixture relationship/source/fragment 图例和输出质量约束。

当前边界：

- 当前只支持 `format = "markdown"` 与 `format = "html"`；未知格式会显式报错。
- export task 当前开放 `format`、`index_file`、`title`、`include_hidden`、`include_internal`、`include_struct_details`、`include_asset_rules`、`include_i18n`；其他未支持字段会作为 unknown option 显式报错。
- Asset rules 摘要只展示可静态解析的 `extensions`、prefix/suffix、require existing、allow/deny path 等基础规则，不读取实际资源引用。
- Text / POT 摘要只标记当前 target 可见 `text` 字段理论上会被 POT exporter 处理，不读取具体文本值。
- Struct / fragment 子字段已有基础展开；复杂平台化输出、owner/domain 分组、HTML 单页 bundle / 交互搜索、ER 图和 Lark / Confluence 发布仍是后续扩展。

Stable 后续扩展方向：

- JSON Schema、Mermaid、DOT、ER 图导出、内联 SVG、HTML 单页 bundle、搜索、过滤、排序等属于新增格式或阅读体验增强，不改变当前 Markdown / HTML 多页静态文档 Stable 契约。
- Lark / Confluence 方言发布、owner/domain 分组、读取真实数据后的 asset reference / text value 清单可作为独立 feature 推进；当前 schema_doc 保持只读 Meta / Project / TargetTags，不读取 source/runtime data。
