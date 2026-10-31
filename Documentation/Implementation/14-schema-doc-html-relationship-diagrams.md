# 14. Schema Doc HTML 阅读体验与关系图例实现设计

相关文档：[Schema Doc 产品设计](../Design/11-schema-doc-generator.md)、[模块大纲](01-module-outline.md)、[Build](10-build-module-detail-design.md)、[FK](11-foreign-key-detail-design.md)、[复合值](12-struct-expanded-value-detail-design.md)。本页描述 schema_doc 生成的工程文档，与由 Markdown 生成的 Design 阅读版是不同产物。

## 14.1 当前能力与边界

schema_doc 是 MetaOnly ArtifactGenerator，只读取 linked Meta、project 元信息、effective tags 和 doc options，不读取 CSV/XLSX 数据行，不依赖 data/validation/artifact toolkit/build。当前 HTML 已有 meta bar、quick links、全局关系、单表邻接、source map、fragment composition、Index/FK Contract 和 Asset/Text Flow legend。

规划阶段只遍历可见表名与公共路径 helper，不建立完整字段/关系图。所有 task 完成路径预留后，生成阶段才构建 SchemaDoc。include_hidden/include_internal 的可见性与 Markdown/HTML 路径集合共用规则，严格 sink reservation 保护 planning/rendering 一致。

## 14.2 模块分层

| 文件 | 当前职责 |
| --- | --- |
| [model.rs](../../src/generator/schema_doc/model.rs) | SchemaDoc、table/field/index/relation、relationship/source/fragment graph 与 display metadata。 |
| [build.rs](../../src/generator/schema_doc/build.rs) | linked Meta → target-aware normalized doc model，关系、source 与 fragment builder。 |
| [html.rs](../../src/generator/schema_doc/html.rs) | page shell、meta bar、quick links、table/card/legend renderer。 |
| [markdown.rs](../../src/generator/schema_doc/markdown.rs) | 相同模型的 Markdown index/table 与关系/source 摘要。 |
| [format.rs](../../src/generator/schema_doc/format.rs) | format options、公共 path planning 与 renderer dispatch。 |
| [tests.rs](../../src/generator/schema_doc/tests.rs) | builder、visibility、escape、顺序、renderer 与路径回归。 |

Renderer 消费 normalized model，不读取 linked Meta，不解析 TOML；graph 构造和 target 规则只在 builder 维护。文件如需拆分，保持 generator 私有模块与 facade，不新增 build 反向依赖。

## 14.3 当前图模型

SchemaDoc 包含 metadata、tables、relationship_graph 和 source_graph；SchemaTableDoc 包含 outgoing relations、incoming_relations 和可选 fragment_graph。

| 模型 | 当前信息与语义 |
| --- | --- |
| SchemaRelationshipGraph | Table nodes + FK edges；source table/field 指向 target table/index，记录 cardinality、allow_empty、linked_helper、visibility。 |
| SchemaTableDoc.incoming_relations | 反向引用边，用于单表一跳 neighborhood；与当前表 outgoing relations 组合显示。 |
| SchemaSourceGraph | Source nodes 与 source→table edges；provider、base/override role、fragments、layout、visibility、header/data 起点。 |
| SchemaFragmentGraphDoc | Base fields 与 fragment cards；fragment payload、fields、source hints、visibility；fragment 不是独立 table。 |

ObjectTable 使用 key/value rows layout；CollectionTable 使用 records layout。XLSX source 的展示路径附带 sheet 名，区分同一文件的多个工作表，但不修改真实文件路径。

Normalized model 的完整字段以 model.rs 为准，不再维护拟新增 GraphNode/Edge 类型或分阶段改名方案。关系表、图例和 Markdown 同源，防止可见性与显示内容各自演化。

## 14.4 关系与过滤约束

- 全局图包含当前文档可见 table；边覆盖普通/复合/list FK、ObjectTable source 和自引用。
- 默认只展示当前 target 可见的 schema；include_hidden/include_internal 显式纳入的内容保留 visibility 标记。
- Incoming relation 与全局 edge 使用一致的 source/target 语义；不能因绘图补回默认已过滤的表。
- Source/fragment 图只解释 Meta 声明，不推断物理行、实际记录数或资源真实依赖。
- Common fields、inline fragment fields、typed struct payload 使用相同 normalized field/path 信息，不在 renderer 重新 link。

## 14.5 HTML/Markdown 展示

HTML 使用内联 CSS card/lane/arrow 与文字图例，自包含、无 script 和外部 JS/CSS/CDN。Meta bar 按 key/value 紧凑横排，quick links 跳到固定 section anchors；用户名称、desc、source 等必须 HTML escape。

HTML 的 index 展示全局表关系和 source map；table 页面展示 neighborhood、source、fragment 与 Index/FK/Asset/Text legend。相对表链接与 section anchors 保持稳定，空图明确展示空状态。

Markdown 保留 relation/source overview 与表格摘要，复用相同 normalized model；无需复制 HTML 图形布局。表格中的 pipe、换行、反引号和中文按 renderer 规则转义。

## 14.6 配置与产物

当前开放 format、index_file、title、include_hidden、include_internal、include_struct_details、include_asset_rules、include_i18n。图例随现有文档生成，不增加 include_diagrams/diagram_detail/diagram_format 选项；未知 option 显式报错。

- HTML：index.html + 每表 table.html，图例内联。
- Markdown：index.md + 每表 table.md，关系/source 以文本表格表达。
- 当前不额外生成 SVG、Mermaid 或 DOT 文件；它们是未来可选扩展，需先定义配置与规划路径。

## 14.7 测试边界

Unit tests 覆盖关系方向、incoming、自引用、list/ObjectTable FK、source/override/fragment model、hidden/internal 可见性、转义、deterministic order、稳定 anchors 与无外部依赖。

[CLI integration](../../tests/cli_integration.rs)验证 docs-only target 无需 input、标准 fixture items 的多源/fragment/FK、跨 generator 路径冲突，以及 direct/temp-commit/dry-run 的规划与实际输出一致。测试固定或排除 generated_at，避免时间戳误报。

无需为了当前静态 card 图引入强制截图框架；修改布局仍应人工检查桌面/窄屏可读性。只有复杂项目的实测可读性不足时，才评估自动布局或 inline SVG，不将未来绘图库当作 Stable 前置条件。
