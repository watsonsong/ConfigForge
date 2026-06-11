# Schema Doc HTML 输出格式实现 TODO

本文替代旧版 Schema / Table 文档生成器实现 TODO。旧版 Markdown MVP 已完成，不再在本文追踪；后续目标收敛为：在现有 `schema_doc` MetaOnly export task 和 `SchemaDoc` normalized model 基础上，新增 **HTML 输出格式**。

明确不做：

- 不支持 JSON Schema / 自定义 schema JSON。
- 不支持 Mermaid ER 图或 FK 图。
- 不新增新的 build 分支或独立 export type；继续使用 `type = "schema_doc"`，通过 `format = "html"` 选择 renderer。

## 当前基线

- `schema_doc` 已位于 `src/generator/schema_doc/`。
- 当前已支持 `format = "markdown"`，生成 `index.md` 与每表 `*.md`。
- 已有 `SchemaDoc` normalized model、`SchemaDocOptions`、`SchemaDocExportConfig`、`SchemaDocFormat`、Markdown renderer 和 ArtifactSink 写入入口。
- Build 已支持 `[[targets.exports]] type = "schema_doc"`，且 schema doc 是 MetaOnly generator，不读取 source data，不依赖 `project.input`。
- 标准工程已有 docs-only target / fixture 覆盖 Markdown MVP。

## 目标行为

用户可在 target export task 中声明：

```toml
[[targets.exports]]
name = "schema_html"
type = "schema_doc"
output = "out/schema-html"
format = "html"
title = "Game Config Schema"
index_file = "index.html"
```

生成输出：

```text
out/schema-html/
  index.html
  items.html
  skills.html
  class_levels.html
```

HTML 输出必须满足：

1. 与 Markdown 共用同一个 `SchemaDoc` normalized model。
2. 输出为静态 HTML 文件，不依赖外部 CDN、外部 CSS、外部 JS。
3. 默认生成多页文档：一个 index 页 + 每表一个详情页。
4. 内容覆盖 Markdown MVP 已有信息：project / target / tags、table summary、fields、indexes、relations、asset/text/validator/struct/list 摘要。
5. 路径、文件名、表/字段顺序 deterministic。
6. 所有用户输入文本、desc、字段名、路径等必须 HTML escape。

## [ ] Step 1：收敛格式契约与配置解析

目标：把 `html` 作为 `SchemaDocFormat` 的正式支持值，并明确它和 Markdown 的文件扩展 / 默认入口文件差异。

实现项：

- [ ] 扩展 `SchemaDocFormat`，新增 `Html`。
- [ ] 解析 `format = "html"`，未知格式继续输出结构化 diagnostic。
- [ ] 当 `format = "html"` 且用户未配置 `index_file` 时，默认入口文件使用 `index.html`。
- [ ] 保持 Markdown 默认行为不变：`format = "markdown"` 默认 `index.md`。
- [ ] 校验 `index_file`：仍不允许路径分隔符；HTML 模式建议或要求 `.html` 后缀。
- [ ] 单表文件名在 HTML 模式下使用 `<table>.html`，Markdown 模式继续使用 `<table>.md`。

测试项：

- [ ] unit test：`format = "html"` 能解析为 `SchemaDocFormat::Html`。
- [ ] unit test：HTML / Markdown 默认 index 文件不同。
- [ ] unit test：非法 HTML index 文件名仍报稳定 diagnostic code 和 current / expected。

建议提交：`feat(schema_doc): parse html output format`

## [ ] Step 2：抽象 renderer 输出规划

目标：避免 HTML renderer 复制 Markdown 的 artifact path 规划逻辑；让格式差异集中在 renderer / extension / entry file。

实现项：

- [ ] 梳理当前 Markdown 写入入口，提取共享的 artifact planning helper。
- [ ] 让 renderer 返回统一的 `SchemaDocRenderedArtifact` 列表，包含 relative path 与 bytes/string content。
- [ ] 或保留现有写入结构，但把“index + table pages”的路径生成封装成 format-aware helper。
- [ ] 确保 artifact path collision / path escape 仍由现有 ArtifactSink / artifact path 机制兜底。
- [ ] 确保 HTML table 页面链接使用相对路径，不产生绝对路径或平台相关分隔符。

测试项：

- [ ] unit test：HTML 输出计划包含 `index.html` 和每表 `.html`。
- [ ] unit test：Markdown 输出计划仍包含 `index.md` 和每表 `.md`。
- [ ] unit test：HTML table link 与 artifact path 一致。

建议提交：`refactor(schema_doc): share document artifact planning`

## [ ] Step 3：实现 HTML escape 与基础模板

目标：先实现安全、可读、无外部依赖的静态 HTML，不做复杂交互。

实现项：

- [ ] 新增 HTML renderer 模块，例如 `src/generator/schema_doc/html.rs`。
- [ ] 实现最小 HTML escape helper，覆盖 `&`、`<`、`>`、`"`、`'`。
- [ ] 实现通用页面骨架：`<!doctype html>`、`<html lang="zh-CN">`、`<meta charset="utf-8">`、`<title>`。
- [ ] 内联一份小型 CSS，保证表格、badge、code、目录、summary card 可读。
- [ ] 不引入外部 JS；如需要折叠，优先使用 `<details>/<summary>`。
- [ ] 所有 Meta desc、table name、field name、validator summary、asset kind 等文本统一走 escape。

测试项：

- [ ] unit test：HTML escape 覆盖特殊字符与引号。
- [ ] unit test：table / field desc 中的 `<script>` 会被 escape，不会原样输出为标签。
- [ ] snapshot 或关键字段断言：HTML 包含 doctype、charset、内联 style、标题。

建议提交：`feat(schema_doc): add static html renderer shell`

## [ ] Step 4：实现 HTML Index 页

目标：HTML index 页作为 Schema 文档入口，覆盖 Markdown index 的信息并提升可读性。

实现项：

- [ ] 顶部 header：title、project、target、effective tags、generated_at。
- [ ] Summary cards：table 数、field 数、index 数、relation 数、asset/text 字段数。
- [ ] Tables 总览表：Table、Kind、Description、Fields、Indexes、Relations、Visibility。
- [ ] Table 名称链接到对应 `<table>.html`。
- [ ] 可选 sections：Asset fields、Text/POT fields、Relation overview，复用现有 model 中的摘要信息。
- [ ] 对 hidden / target-specific visibility 使用 badge 展示，例如 `all`、`client`、`server`、`hidden`。

测试项：

- [ ] unit test：index HTML 包含 project / target / tags。
- [ ] unit test：index HTML 表格链接到单表 HTML 文件。
- [ ] unit test：asset/text/relation 摘要字段出现在 index HTML。

建议提交：`feat(schema_doc): render html index page`

## [ ] Step 5：实现 HTML 单表详情页

目标：单表页覆盖 Markdown 单表文档全部内容，并用 HTML 结构提升浏览效率。

实现项：

- [ ] 页面顶部：返回 index 链接、table name、description、kind、visibility、source、row_export、primary index。
- [ ] Fields section：字段表展示 Field、Type、Required、Default、Export、Description。
- [ ] Field detail：展示 enum values、validator summary、asset kind/rules、text/POT 标记、struct/list element summary。
- [ ] Indexes section：展示 index name、fields、unique、primary、ordered/preserve source order、visibility。
- [ ] Relations section：展示 source field、target table/index/fields、cardinality、linked helper、allow_empty、visibility。
- [ ] ObjectTable、fragment、struct/list 等特殊语义用 callout 或 badge 展示，避免只靠纯文本说明。

测试项：

- [ ] unit test：单表 HTML 包含 Fields / Indexes / Relations sections。
- [ ] unit test：struct/list/enum/asset/text 字段摘要可渲染。
- [ ] unit test：FK relation 链接或文本包含 target table/index。

建议提交：`feat(schema_doc): render html table pages`

## [ ] Step 6：Build / CLI 集成测试

目标：验证 `type = "schema_doc" format = "html"` 在真实 build 中生成预期 artifact。

实现项：

- [ ] 在 schema_doc 相关 unit / integration 测试中增加 HTML 模式。
- [ ] 标准工程或专用 fixture 增加一个 HTML docs target，或在现有 docs target 测试中临时覆盖 format。
- [ ] 验证 docs-only target 不读取 data、不要求 `project.input`。
- [ ] 验证 `--dry-run` artifact plan 中包含 HTML 文件。
- [ ] 验证实际 build 后 `index.html` 和至少一个 table `.html` 存在且内容包含核心字段。

测试项：

- [ ] `cargo test schema_doc` 或等价 focused tests。
- [ ] CLI integration：HTML schema docs target 成功构建。
- [ ] 全量 `cargo test` 通过。

建议提交：`test(schema_doc): cover html schema docs export`

## [ ] Step 7：同步设计与实现文档状态

目标：HTML 支持落地后，canonical 文档不再说 HTML 是泛泛“后续扩展”，而是明确当前支持 Markdown + HTML，且 JSON/Mermaid 不在近期目标内。

实现项：

- [ ] 更新 `Documentation/Design/11-schema-doc-generator.md`：
  - [ ] `format` 支持值改为 `markdown` / `html`。
  - [ ] 输出结构增加 HTML 模式说明。
  - [ ] 非目标明确 JSON Schema / Mermaid 不支持。
- [ ] 更新 `Documentation/Design/README.md` schema / table docs 状态边界。
- [ ] 更新相关 Implementation 文档中 `schema_doc` 当前支持格式描述。
- [ ] 如 HTML 阅读版仍对外使用，更新 `Documentation/Design/html/index.html` 中 Schema / Table Docs 状态摘要；否则明确 canonical Markdown 为准。

测试项：

- [ ] 文档 grep 不再出现“Schema Doc 当前仅支持 Markdown，HTML / JSON Schema / Mermaid 仍是后续扩展”这类过期表述。
- [ ] 保留“不支持 JSON Schema / Mermaid”的明确说明。

建议提交：`docs(schema_doc): document html schema docs support`

## 验收标准

完成后应满足：

- [ ] `schema_doc format = "markdown"` 行为完全兼容现有输出。
- [ ] `schema_doc format = "html"` 生成 `index.html` 和每表 `.html`。
- [ ] HTML 输出为自包含静态文件，无外部 CSS/JS 依赖。
- [ ] HTML 内容覆盖 Markdown MVP 的核心 schema 信息。
- [ ] 所有用户可控文本均 escape。
- [ ] JSON Schema / Mermaid 不作为本轮目标进入代码或配置支持。
- [ ] focused tests 与 `cargo test` 通过。

## 后续不在本文追踪的增强

- 搜索、过滤、排序等交互能力。
- 单页 HTML bundle 模式。
- 暗色主题 / 打印样式。
- Lark / Confluence 发布集成。
- JSON Schema、schema JSON、Mermaid ER 图。
