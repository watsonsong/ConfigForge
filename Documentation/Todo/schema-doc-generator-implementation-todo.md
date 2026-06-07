# Schema / Table 文档生成器实现 TODO

本文把 [`Design/11-schema-doc-generator.md`](../Design/11-schema-doc-generator.md) 中的 P1-3 Schema / Table 文档生成器设计拆成可独立开发、验证和提交的实现步骤。当前实现状态是：Step 1～9 的 Markdown MVP 已完成，`type = "schema_doc"` MetaOnly export task、`SchemaDoc` normalized model、Markdown renderer、artifact path planning / `ArtifactSink` 写入、target-aware visibility、asset / POT / validator / FK / index / struct 摘要以及标准工程端到端 fixture 均已落地。本文后续只作为“已完成记录 + 剩余高级扩展”使用；当前状态说明以 canonical Design 和 Implementation 文档为准。

## 总体原则

- 每个 Step 必须能独立编译、独立测试、独立提交。
- `schema_doc` 是 **MetaOnly** export task，不读取数据、不执行 data validation、不依赖 `project.input`。
- 文档内容必须来自 linked Meta、Project config 和当前 target effective tags；不能用另一套可见性规则解释 table / field / index。
- Markdown / HTML / JSON Schema / Mermaid 等未来 renderer 必须共享同一个 normalized documentation model，避免各 renderer 自行解释 Meta。
- P1-3 MVP 只实现 Markdown；未实现的 format 或配置项必须显式报错，不能静默忽略。
- 输出顺序必须 deterministic：table、field、index、FK 默认保持 Meta/link 后的稳定顺序；跨表摘要按固定 key 排序。
- 所有输出路径必须纳入 ArtifactSink / artifact conflict check；文件名必须拒绝 path separator、parent escape 和重复 normalized path。
- Markdown renderer 必须转义 `|`、换行等破坏表格的字符；空值统一显示 `-`。
- 文档状态必须始终区分“目标态设计”和“当前实现边界”：实现完成前保持 `Planned / Not implemented`。

## [x] Step 1：建立 SchemaDoc 核心模型与 builder 骨架

目标：引入面向文档生成的 normalized model，把 Project / LinkedMeta / TargetTags 转成稳定、可测试、可被多 renderer 复用的数据结构。

范围：

- 在合适模块下新增 schema doc 子模块，例如 `src/schema_doc/`，或在 exporter/build 下新增 meta-only document exporter 子模块；保持依赖方向不反向污染 `meta` / `validation` / `codegen`。
- 定义 `SchemaDoc`、`SchemaDocMetadata`、`SchemaTableDoc`、`SchemaFieldDoc`、`SchemaIndexDoc`、`SchemaRelationDoc`、`SchemaDocOptions` 等核心结构。
- 从 Project / target / LinkedMeta 中填充 project name、project path、target、target tags、generated_at、table name、table kind、desc、source summary、field path、type display、required、default、export visibility 等基础信息。
- 先支持 CollectionTable / ObjectTable 的 table-level summary；复杂 struct/list/fragment 可先以字段 type display 表达，不展开详情。
- 明确缺失字段表现：desc / default / source / primary index 不存在时用 `None` 或 `-`，不能编造内容。

验收：

- 单元测试覆盖空 Meta、单表、多表、ObjectTable、带 desc/default/required 的字段。
- 单元测试覆盖 builder 输出顺序 deterministic。
- 不接入 export registry，不写任何文件产物。

建议提交：`feat(schema-doc): add schema documentation model`

## [x] Step 2：实现 target-aware 可见性摘要

目标：让 SchemaDoc 准确展示当前 target 下 table、field、index 的导出可见性，为 client-only / server-only / all / hidden / internal 奠定基础。

范围：

- 复用现有 target tags / export_when / export=false 判断逻辑，避免重新实现一套不一致规则。
- 为 table / field / index 填充 `visibility` 展示值：`all`、tag 列表、`hidden`、`internal` 等。
- 支持 `include_hidden` / `include_internal` 选项的模型层过滤语义；如果本 Step 暂不开放配置，也要在 model tests 中固化默认行为。
- table `row_export` 不扫描数据，只在 table summary 中标记是否存在 row-level filtering。
- index 引用隐藏字段、index 自身隐藏、ObjectTable 不支持 index 等场景需要有明确展示或过滤策略。

验收：

- 单元测试覆盖 table hidden、field hidden、export=false internal field、index hidden、row_export summary。
- 同一 Meta 在不同 target tags 下输出不同 visibility，但顺序稳定。
- 未实现配置项如果进入公开解析层，必须显式报错。

建议提交：`feat(schema-doc): summarize target visibility`

## [x] Step 3：实现 Fields / Indexes / Relations 文档数据

目标：补齐单表文档 MVP 的三大核心区块：Fields、Indexes、Relations。

范围：

- Fields：输出 field path、type display、required、default、export visibility、description。
- Indexes：输出 index name、fields、unique、primary、export visibility。
- Relations：把 field-level FK、table-level FK、ObjectTable 作为 FK source、list scalar FK 归一为 `SchemaRelationDoc`。
- Relation target 展示为 `target_table.target_field` 或 `target_table(index_name)`；linked helper 展示 `linked_field`，缺失显示 `-`。
- Cardinality 使用面向文档的稳定枚举或字符串：`many-to-one`、`one-to-one`、`list-to-many` 等；不确定时使用保守 fallback。

验收：

- 单元测试覆盖 primary/unique/non-unique index、复合 index、field-level FK、table-level FK、list FK、linked_field。
- ObjectTable 无 index 时输出空 indexes，而不是 panic。
- target hidden FK target/index 的处理符合 Step 2 可见性策略。

建议提交：`feat(schema-doc): collect fields indexes and relations`

## [x] Step 4：实现 Markdown renderer

目标：先落地最重要、最容易集成到 MR / Lark / Confluence / CI artifact 的 Markdown 输出。

范围：

- 新增 Markdown renderer，共享 Step 1~3 的 normalized `SchemaDoc` model。
- 输出 `index.md` 内容：title、project、target、tags、generated_at、table 列表和 summary。
- 输出单表 Markdown 内容：table title/desc、properties、Fields、Indexes、Relations。
- Markdown table escape：处理 `|`、换行、``、多行 desc、反引号等易破坏表格的内容。
- 空字段统一显示 `-`；布尔值使用 `yes` / `no`。
- 链接使用相对路径，如 `[items](items.md)`。

验收：

- 单元测试覆盖 index renderer、单表 renderer、空表、无 index / 无 relations、中文描述、Markdown escape。
- 输出顺序 deterministic，可用于 snapshot/golden 测试。
- 不接入 build/export task；renderer 可由测试直接调用。

建议提交：`feat(schema-doc): render markdown documentation`

## [x] Step 5：实现输出路径规划与 ArtifactSink 写入

目标：把 SchemaDoc Markdown renderer 变成可写入 artifact 的内部能力，并接入路径冲突检查所需的 output planning。

范围：

- 定义 `SchemaDocFormat`，P1-3 只支持 `markdown`。
- 定义 `SchemaDocExportConfig`，至少包含 `output`、`format`、`index_file`、`title`。
- 实现输出规划：`index.md` + 每张可见 table 的 `<table>.md`。
- 文件名策略先使用 canonical table name + `.md`；拒绝 table 文件名冲突、路径分隔符、parent escape。
- 使用 `ArtifactSink` 写入所有文档，支持 direct / temp_commit / dry_run 统一行为。

验收：

- 单元测试覆盖文件名规划、index_file 非纯文件名、table 文件名冲突、normalized path conflict、空文档输出。
- dry-run 下能返回 artifact plan，不实际写文件。
- 不接入 Project config export task 解析。

建议提交：`feat(schema-doc): plan and write markdown artifacts`

## [x] Step 6：接入 export registry / build target planning

目标：让用户可以在 `[[targets.exports]]` 中声明 `type = "schema_doc"`，并把它作为 MetaOnly export task 纳入 build orchestration。

范围：

- 在 export dependency planning 中声明 `schema_doc` 为 MetaOnly。
- 解析 `schema_doc` export task 选项：`format`、`index_file`、`title`；暂未实现选项显式报错。
- 在 MetaOnly 分支中构建 SchemaDoc 并写入 artifact。
- 输出路径纳入现有 artifact conflict check，与 codegen/runtime data/validation_report 等任务互相检测冲突。
- docs-only target 不要求 `project.input`。

验收：

- Build integration tests 覆盖合法 `schema_doc` 生成 `index.md` 与单表 `.md`。
- 覆盖 docs-only target 无 `project.input` 也可生成文档。
- 覆盖未知 format、未知 option、缺 output、路径冲突等错误路径。
- 现有 runtime data/codegen build 行为不回退。

建议提交：`feat(build): add schema_doc export task`

## [x] Step 7：补充 asset / text(POT) / validator 摘要

目标：覆盖用户痛点中的 asset、POT 和规则说明，让文档不仅展示字段列表，还能解释字段用途和约束。

范围：

- Asset：识别 `asset` 字段，展示 `asset_kind`，并从 Meta/Project asset kind 中提取可静态展示的规则摘要。
- Text/POT：识别当前 target 可见的 `text` 字段，标记理论上会被 `gettext_pot` / `pot` 聚合 exporter 抽取。
- Validators：展示内置 validators 的简短摘要，例如 range、regex、string_length、required_when、compare、sum、monotonic。
- 单表文档增加 `Assets`、`Text / POT`、`Validators` 或字段扩展列/详情块。
- 保持不读取数据：不列出具体 asset reference 或 text value。

验收：

- 单元测试覆盖 asset kind、无 asset kind 错误由 Meta validate 负责、text 字段 target filtering、内置 validator 摘要。
- Markdown 输出包含 Assets / Text-POT 摘要，并在无相关字段时稳定显示或省略（策略固定）。
- 与 Asset Manifest / POT exporter 的职责边界在注释或文档中清晰。

建议提交：`feat(schema-doc): document assets text and validators`

## [x] Step 8：补充 struct / list / fragment 展开说明

目标：让复杂字段的文档更接近用户实际表结构，解释 struct/list/fragment 的书写和导出形态。

范围：

- 对 named struct 字段输出子字段列表或嵌套详情表。
- 对 list 字段输出 element type display；标量数组 FK 需要在 Relations 中体现。
- 对 fragment / polymorphic variant source mode 输出 variant/fragment 结构摘要。
- 对 ObjectTable 中 struct expanded value 的 source path 规则增加说明。
- `include_struct_details` 选项控制是否展开；未实现公开选项前可默认展开。

验收：

- 单元测试覆盖 nested struct、list scalar、list enum、fragment-scoped fields、ObjectTable expanded value。
- Markdown 输出复杂字段仍保持可读，表格不被嵌套内容破坏。
- 不改变 runtime export/codegen 语义。

建议提交：`feat(schema-doc): expand complex field documentation`

## [x] Step 9：端到端 fixtures、文档状态和 HTML 阅读版更新

目标：把 Schema / Table 文档生成器从功能闭环推进到可维护状态，并同步用户面文档状态。

范围：

- 在标准工程或专用 fixture 中增加 `schema_doc` export task 示例。
- 增加端到端测试覆盖 Markdown artifact 内容、target filtering、docs-only target、artifact conflict。
- 更新 `Documentation/Design/11-schema-doc-generator.md` 当前状态，从 `Planned / Not implemented` 调整为 `Implemented MVP` 或 `Partial`，并准确列出实现边界。
- 更新 `Documentation/Design/README.md` 状态矩阵、`Documentation/Design/html/index.html`、`Documentation/README.md`、`AGENTS.md` 中相关描述。
- 如新增模块或 exporter trait，更新对应 Implementation 文档，尤其是 exporter/build 模块边界。

验收：

- `cargo fmt`、相关单元测试、CLI/build integration tests、必要的 `cargo test` 通过。
- Design / README / HTML 阅读版状态一致。
- TODO 文档中已完成步骤可逐步标记；全部完成后判断是否删除或归档本 TODO。

建议提交：`docs(schema-doc): document implemented schema docs MVP`

## [ ] Step 10：后续格式扩展预留（HTML / JSON Schema / Mermaid）

目标：在 Markdown MVP 稳定后，为后续平台消费和图形化关系文档扩展留出清晰边界。

范围：

- 评估是否新增 `html` renderer：多页/单页、自包含 CSS、无需外部 CDN。
- 评估 `json_schema` 或自定义 schema JSON：供平台、Lark bot、Confluence 同步器消费。
- 评估 Mermaid ER 图：从 `SchemaRelationDoc` 生成 table/FK graph。
- 评估 Lark / Confluence Markdown 方言：链接、表格、折叠区块适配。
- 明确这些扩展是否作为同一个 `schema_doc` format，或拆成 `schema_graph` / `schema_json` 等独立 type。

验收：

- 形成后续设计或 TODO，不影响 Markdown MVP 的稳定契约。
- 如果实现任一格式，必须复用 `SchemaDoc` normalized model，并补齐 renderer 单测。
- 未实现格式继续显式报错。

建议提交：`docs(schema-doc): plan advanced documentation formats`
