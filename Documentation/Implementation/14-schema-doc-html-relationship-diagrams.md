# 14. Schema Doc HTML 阅读体验与关系图例实现设计

Related docs：[`../Design/11-schema-doc-generator.md`](../Design/11-schema-doc-generator.md)、[`01-module-outline.md`](01-module-outline.md)、[`10-build-module-detail-design.md`](10-build-module-detail-design.md)、[`11-foreign-key-detail-design.md`](11-foreign-key-detail-design.md)、[`12-struct-expanded-value-detail-design.md`](12-struct-expanded-value-detail-design.md)、[`../Design/08-asset-validation-and-manifest.md`](../Design/08-asset-validation-and-manifest.md)。

本文把 `schema_doc` HTML 阅读体验、表关系图例、source map、fragment composition 等产品设计落到 `src/generator/schema_doc/` 的实现结构。产品语义以 [`Design/11-schema-doc-generator.md`](../Design/11-schema-doc-generator.md) 为准；本文约束 Rust 模块拆分、数据模型、renderer 边界、测试入口和分阶段落地顺序。

## 14.1 当前实现基线

`schema_doc` 当前是 `MetaOnly` artifact generator，位于 `src/generator/schema_doc/`，直接实现 build-facing `ArtifactGenerator`，不读取 source data，也不依赖 validation / data artifact / code artifact toolkit。模块边界由 `01-module-outline.md` 定义。

当前核心结构：

| 文件 | 当前职责 | 关键参考 |
| --- | --- | --- |
| `model.rs` | 定义 `SchemaDoc`、`SchemaTableDoc`、字段 / index / relation / asset / text / validator 文档模型。 | `src/generator/schema_doc/model.rs:28`、`src/generator/schema_doc/model.rs:45` |
| `build.rs` | 从 `LinkedMetaModel` 构建 target-aware normalized doc model。 | `src/generator/schema_doc/build.rs:15` |
| `html.rs` | 渲染多页静态 HTML shell、index、table 页面与 section 表格。 | `src/generator/schema_doc/html.rs:25`、`src/generator/schema_doc/html.rs:210` |
| `markdown.rs` | 渲染 Markdown index / table 页面。 | `src/generator/schema_doc/markdown.rs:18` |
| `format.rs` | 解析 `format`、规划输出路径、调用 renderer 生成 artifacts。 | `src/generator/schema_doc/format.rs:17` |

当前能力和限制：

- `SchemaDoc` 已包含 `relationship_graph` 与 `source_graph`，每张 `SchemaTableDoc` 已包含 `incoming_relations` 与可选 `fragment_graph`，图例数据由 builder 生成 normalized model 后交给 renderer。
- Source graph 展示 provider、base / override source、fragments、visibility、ObjectTable `key/value rows` layout 与 header/data row 摘要；仍只读取 Meta source declaration，不读取 source data。
- Fragment 子字段仍会展开到 Fields 表；同时 table 页的 Fragment composition 以 base table card + fragment card 展示 fragment fields、visibility、payload 与 source hints，并明确 fragment 不是独立 table。
- HTML 顶部 metadata 使用 `.meta-bar` / `.meta-item` 横向展示，并提供固定 anchors 与 quick links。
- HTML index / table 页已提供 Relationship graph、Table neighborhood、Source map、Fragment composition、Index/FK Contract、Asset/Text Flow legend；所有图例均为内联 HTML/CSS，无外部 JS/CSS/CDN。

## 14.2 实现目标

### 14.2.1 近期目标

1. 优化 HTML 顶部 summary 排版：Project / Target / Tags / Generated at 使用紧凑横排 meta bar，同一 key/value 不拆行。
2. 给 index 与 table 页面增加面向阅读的 quick links / chips，能快速跳转到 Tables、Relationship graph、Source map、Assets、Text/POT 等区域。
3. 增加无 JS、无外部依赖的 HTML/CSS 图例：
   - 全局表关系图；
   - 单表上下游关系图；
   - Source Map；
   - Fragment Composition；
   - Index & FK Contract legend；
   - Asset / Text Flow legend。
4. 扩展 normalized model，使图例由 builder 生成结构化数据，renderer 只负责展示，避免 HTML / Markdown / 未来 SVG/Mermaid 各自重新解释 Meta。

### 14.2.2 非目标

- 不读取 CSV / JSON / runtime data 行；所有图例都是 schema-level。
- 不引入外部 JS / CSS / CDN。
- 不在第一阶段实现复杂自动 graph layout；优先用 CSS card / lane / arrow 表达 deterministic 关系。
- 不改变 build pipeline、artifact sink、target requirements；`schema_doc` 仍是 MetaOnly。
- 不把 Mermaid / DOT 作为默认 HTML 依赖；如需支持，应作为后续可选 artifact 或 fenced block。

## 14.3 模块边界与文件拆分

建议在 `src/generator/schema_doc/` 内保持以下职责分层：

```text
schema_doc/
  model.rs          # SchemaDoc 当前模型 + diagram/graph 结构化模型
  build.rs          # table/field/index/relation/source/fragment/graph builder
  html.rs           # HTML renderer：layout、cards、graph/legend render helper
  markdown.rs       # Markdown renderer：可选文本图 / 关系摘要，避免 HTML-only 数据模型
  format.rs         # format config、path planning、artifact render dispatch
  tests.rs          # builder / renderer / path tests
```

如 `html.rs` 继续增长，可进一步拆出私有子模块，但仍通过 `mod.rs` 私有挂载：

```text
html.rs             # page shell + high-level section order
html_layout.rs      # meta bar、cards、quick links
html_diagram.rs     # graph/card/legend renderer
```

拆分原则：

- `build.rs` 可以依赖 `meta` linked view；`html.rs` / `markdown.rs` 不直接依赖 `meta`。
- renderer 只消费 `model.rs` 中的字符串或结构化摘要，不读取 `LinkedMetaModel`。
- `schema_doc` 不依赖 `data`、`validation`、`data_artifact`、`code_artifact` 或 `build`。
- 图模型必须 deterministic：节点顺序按 linked table order，边顺序按 source table order、foreign key declaration order、target table order。

## 14.4 Normalized model 扩展

当前 `SchemaDoc` 结构过于接近表格渲染。后续应扩展为“表格详情 + 图关系视图”的组合模型。

### 14.4.1 顶层图模型

```rust
pub struct SchemaDoc {
    pub metadata: SchemaDocMetadata,
    pub tables: Vec<SchemaTableDoc>,
    pub relationship_graph: SchemaRelationshipGraph,
    pub source_graph: SchemaSourceGraph,
}

pub struct SchemaRelationshipGraph {
    pub nodes: Vec<SchemaTableNodeDoc>,
    pub edges: Vec<SchemaRelationEdgeDoc>,
}

pub struct SchemaTableNodeDoc {
    pub table: String,
    pub kind: String,
    pub description: Option<String>,
    pub source_summary: Option<String>,
    pub primary_index: Option<String>,
    pub field_count: usize,
    pub index_count: usize,
    pub relation_count: usize,
    pub visibility: String,
    pub file_name: Option<String>,
}

pub struct SchemaRelationEdgeDoc {
    pub source_table: String,
    pub source_field: String,
    pub target_table: String,
    pub target_index: String,
    pub target_display: String,
    pub cardinality: String,
    pub linked_helper: Option<String>,
    pub visibility: String,
    pub anchor: Option<String>,
}
```

`file_name` 与 `anchor` 可在 path planning 之后补齐，或由 renderer 根据 stable file-name helper 计算。为了让 model 测试更稳定，建议 builder 先不写文件名，renderer 通过 `table_file_names` map 解决链接。

### 14.4.2 Source graph 模型

```rust
pub struct SchemaSourceGraph {
    pub sources: Vec<SchemaSourceNodeDoc>,
    pub table_edges: Vec<SchemaSourceTableEdgeDoc>,
}

pub struct SchemaSourceNodeDoc {
    pub id: String,
    pub provider: String,
    pub path: String,
    pub row_layout: Option<String>,
    pub visibility: String,
}

pub struct SchemaSourceTableEdgeDoc {
    pub source_id: String,
    pub table: String,
    pub fragments: Vec<String>,
    pub override_source: bool,
    pub visibility: String,
}
```

当前 `LinkedTableMeta::meta.source_paths()` 只能提供路径摘要，足以支持 Phase 1 的 `Source -> Table` card；如果要展示 provider、override source、fragments、row layout，需要 builder 读取 raw table source declaration 的结构化信息。实现时应优先在 `meta` linked view 暴露只读 helper，而不是在 schema_doc 内解析 TOML。

### 14.4.3 Fragment graph 模型

```rust
pub struct SchemaFragmentGraphDoc {
    pub table: String,
    pub base_fields: Vec<String>,
    pub fragments: Vec<SchemaFragmentNodeDoc>,
}

pub struct SchemaFragmentNodeDoc {
    pub name: String,
    pub fields: Vec<String>,
    pub visibility: String,
    pub source_hints: Vec<String>,
}
```

Fragment graph 可挂在 `SchemaTableDoc` 上：

```rust
pub struct SchemaTableDoc {
    // existing fields...
    pub incoming_relations: Vec<SchemaRelationEdgeDoc>,
    pub outgoing_relations: Vec<SchemaRelationEdgeDoc>,
    pub fragment_graph: Option<SchemaFragmentGraphDoc>,
}
```

为了避免重复维护 `relations[]` 与 `outgoing_relations[]`，可在 Phase 1 保留现有 `relations[]`，新增 `incoming_relations[]`，由 renderer 组合展示；Phase 2 再统一 relation table 与 graph edge 的数据来源。

## 14.5 Builder 设计

### 14.5.1 构建顺序

`SchemaDoc::from_meta()` 当前直接遍历 tables 并调用 `table_doc()`，见 `src/generator/schema_doc/model.rs:120`。扩展后推荐流程：

1. 构建所有 target-visible `SchemaTableDoc`，保持现有行为。
2. 建立 `visible_table_names: BTreeSet<String>` 与 `table_index_by_name`。
3. 从所有 visible table 的 `foreign_keys` 构建 `SchemaRelationEdgeDoc`：
   - outgoing edge 挂到 source table；
   - incoming edge 挂到 target table；
   - 同时汇总到 `relationship_graph.edges`。
4. 从 table sources 构建 `SchemaSourceGraph`。
5. 从 table fragments 构建每表 `SchemaFragmentGraphDoc`。
6. 计算 summary counts：表、字段、index、outgoing relations、incoming relations、asset/text、fragment/source 数量。

### 14.5.2 关系边语义

关系边应复用现有 relation 语义，不应另起口径：

- target display 继续复用 `relation_target_display()` 的逻辑；当前实现见 `src/generator/schema_doc/build.rs:250`。
- cardinality 继续复用 `relation_cardinality()`，并扩展 ObjectTable、optional source、自引用标记；当前实现见 `src/generator/schema_doc/build.rs:279`。
- visibility 继续复用 `export_visibility()`，确保 target filtering 与表格一致；当前实现见 `src/generator/schema_doc/build.rs:410`。
- relation edge 的 description / linked helper 与 `SchemaRelationDoc` 一致。

### 14.5.3 Source / fragment 语义

Source Map 不读取实际数据，只展示 Meta source declaration：

- 单 source collection：`source.csv -> table`。
- 多 source / polymorphic table：多个 source node 指向同一 table，边标记 fragments。
- override source：边标记 `override`，并展示其 `export_when` visibility。
- ObjectTable：source node 或 edge 标记 `key/value rows`。
- fragment：fragment node 不当作 table，不参与 FK graph，只在 Source Map edge 和 Fragment Composition 中出现。

## 14.6 HTML renderer 设计

### 14.6.1 Meta bar

替换当前 `<dl>` summary card 的视觉布局，但可保留语义上可读的结构。推荐 HTML：

```html
<section class="card hero">
  <h1>Reference Game Meta Schema Documentation</h1>
  <div class="meta-bar" aria-label="Schema metadata">
    <span class="meta-item"><span class="meta-key">Project</span>: <strong class="meta-value">standard_reference_game</strong></span>
    <span class="meta-item"><span class="meta-key">Target</span>: <strong class="meta-value">client</strong></span>
    <span class="meta-item"><span class="meta-key">Tags</span>: <strong class="meta-value">client</strong></span>
    <span class="meta-item muted"><span class="meta-key">Generated</span>: <span class="meta-value">2026-06-14T...</span></span>
  </div>
</section>
```

推荐 CSS：

```css
.meta-bar { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; }
.meta-item { white-space: nowrap; }
.meta-key { color: #6b7280; font-weight: 600; }
.meta-value { color: #111827; }
```

### 14.6.2 Quick links 与 anchors

index 页固定锚点：

- `#tables`
- `#relationship-graph`
- `#source-map`
- `#asset-fields`
- `#text-pot-fields`
- `#relations`

table 页固定锚点：

- `#overview`
- `#neighborhood`
- `#source-map`
- `#fragments`
- `#fields`
- `#indexes`
- `#relations`
- `#assets`
- `#text-pot`
- `#validators`

Renderer 应使用 helper 统一生成 anchor id，避免 table / field name 直接拼接导致非法 fragment id。若需要 field-level anchor，建议使用 `slugify("field-" + field.path)`，并保留 stable fallback。

### 14.6.3 CSS card graph

Phase 1 图例不用自动 layout，建议用三类组件表达：

```text
.graph-section
  .graph-lane / .graph-grid
    .graph-node.table-node
    .graph-edge-card
    .graph-node.table-node
```

关系图示例：

```html
<section id="relationship-graph">
  <h2>Relationship graph</h2>
  <div class="edge-list">
    <article class="edge-card">
      <a class="node" href="items.html"><code>items</code></a>
      <span class="arrow">rarity_id → by_id</span>
      <a class="node" href="rarity_levels.html"><code>rarity_levels</code></a>
      <span class="badge">many-to-one</span>
      <span class="badge">helper: rarity</span>
    </article>
  </div>
</section>
```

Source Map 示例：

```html
<article class="edge-card source-edge">
  <span class="source-node"><code>materials.csv</code></span>
  <span class="arrow">feeds</span>
  <a class="node" href="items.html"><code>items</code></a>
  <span class="badge">fragment: stack</span>
</article>
```

### 14.6.4 Legend

Legend 应放在图表附近，解释视觉编码，而不是只在文档末尾说明：

- CollectionTable：实线 table node。
- ObjectTable：带 `ObjectTable` badge 或不同背景。
- FK edge：`source field → target index`。
- List FK：`[]` badge 或 `list-to-many` badge。
- Hidden / target-filtered：灰色或 dashed edge。
- Fragment：puzzle / fragment badge，明确“不是独立 table”。
- Asset/Text flow：说明 schema doc 展示字段契约，不展示实际数据值。

## 14.7 Markdown renderer 兼容策略

Markdown 不要求复杂图形，但不应丢失结构信息。建议：

- index.md 增加 `## Relationship overview`：使用表格展示 `Source Table | Field | Target Table | Target Index | Cardinality | Helper`。
- index.md 增加 `## Source map`：使用表格展示 `Source | Table | Fragments | Override | Visibility`。
- table.md 增加 `## Related tables`：分 outgoing / incoming 两个小表。
- table.md 增加 `## Fragments`：列 fragment name、fields、source hints。

这样同一 normalized model 能同时服务 HTML 图例与 Markdown 文本摘要。

## 14.8 配置与 artifact 输出

当前 Project Config 稳定开放 `format`、`index_file`、`title`、`include_hidden`、`include_internal`、`include_struct_details`、`include_asset_rules`、`include_i18n`。图例优化默认启用，不新增 `include_diagrams` / `diagram_detail` / `diagram_format` 配置，避免配置面过早膨胀；用户设置未知 option 仍显式报错。

后续如需要控制，可增加：

| Option | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `include_diagrams` | bool | `true` | 是否输出 HTML/CSS 图例和 Markdown relation/source overview。 |
| `diagram_detail` | string | `summary` | `summary` / `full`。`summary` 只输出一跳和 top-level；`full` 输出所有 edges。 |
| `diagram_format` | string | `html_css` | 后续可选 `html_css` / `inline_svg` / `mermaid` / `dot`。Phase 1 不开放。 |

未知 option 仍必须显式报错，遵循现有 `schema_doc` options 行为。

Artifact 输出仍是：

- HTML：`index.html` + 每表 `<table>.html`；diagram 内联在这些页面中。
- Markdown：`index.md` + 每表 `<table>.md`；diagram 以文本表格表达。
- 不额外写出 `.svg` / `.mmd` / `.dot`，除非后续新增明确配置。

## 14.9 测试计划

单元测试：

- builder 已覆盖 global relationship graph，包括普通 FK、list FK、ObjectTable FK、自引用、target-hidden relation。
- builder 已覆盖 incoming relation，确保目标表能看到引用自己的 source tables。
- builder 已覆盖 source graph，包括 single source、multi-source polymorphic table、override source、fragment edge。
- builder 已覆盖 fragment graph，包括 fragment 字段、target visibility、source hints，以及 `include_struct_details = false` 时隐藏 fragment section。
- HTML renderer 已覆盖 meta bar 中 `Project: standard_reference_game` 不被 `<dt>/<dd>` 分裂；用户文本仍 HTML escape。
- HTML renderer 已覆盖 relationship graph / source map / fragment composition 输出 stable anchors、relative links、legend badges，且无外部 JS/CSS/CDN。
- Markdown renderer 已覆盖 relation overview / source map 的表格 escape 与 deterministic order。
- path planning 已覆盖 diagram section 不新增 artifact path；现有 output conflict 测试保持有效。

集成测试：

- `schema_doc format = "html"` docs-only target 生成 index 与 table 页面，并包含 `relationship-graph`、`source-map` anchor。
- standard fixture 已覆盖 HTML schema docs 显示 `items -> rarity_levels` FK、`materials.csv/weapons.csv/armors.csv -> items` source map、`stack/attributes/skill` fragment composition。
- target filtering 已覆盖 server/client target 下 hidden field / relation 不进入默认图例，并覆盖显式 `include_hidden` / `include_internal` 输出差异。
- dry-run artifact plan 仍只包含 index + per-table docs，不因 inline diagrams 增加额外 artifact。

可视回归：

- 不强制引入截图测试。若未来增加 snapshot，应固定 generated_at 或过滤时间字段。
- HTML shell 继续无 `<script>`，无外部 URL 依赖。

## 14.10 分阶段落地建议

1. **Phase 1：HTML summary 与 anchors（已完成）**
   - 将 `<dl>` metadata 改为 `.meta-bar`。
   - 增加 quick links 与固定 section anchors。
   - 补充 HTML renderer 单元测试。

2. **Phase 2：Relationship graph model + HTML/CSS 图例（已完成）**
   - 在 model/build 中新增 relationship graph 与 incoming relation。
   - HTML index 输出全局关系图；table 页输出 neighborhood graph。
   - Markdown 输出 relation overview。

3. **Phase 3：Source map + Fragment composition（已完成）**
   - 扩展 source graph / fragment graph 模型。
   - HTML 输出 source map 与 fragment cards。
   - standard fixture integration 覆盖 polymorphic `items` 表。

4. **Phase 4：Legend polish（已完成）与可选 SVG/Mermaid 预研（后续扩展）**
   - 已完善 Index & FK Contract、Asset/Text Flow legend。
   - 如 HTML/CSS card 无法满足复杂项目可读性，再设计 inline SVG 或 Mermaid/DOT 可选输出；该方向不属于当前 Stable 必备范围。

每个 phase 应独立提交，并在提交前运行 `cargo fmt`、相关 unit tests 和 `cargo test --test cli_integration schema_doc` 或完整 `cargo test`。
