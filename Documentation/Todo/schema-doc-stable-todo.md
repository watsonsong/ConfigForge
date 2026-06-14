# Schema / Table Docs Stable 推进 Todo

目标：将 `schema_doc` 从 **Implemented MVP** 推进到 **Stable**。Stable 的判断标准不是继续扩展 JSON Schema / Mermaid / 交互式搜索等新形态，而是把当前 Markdown / HTML 静态文档能力的配置面、可见性语义、确定性、fixture 回归与用户文档状态收口。

## Stable 范围

必须纳入 Stable：

- `type = "schema_doc"` 作为 MetaOnly artifact generator 的注册、规划、写入和 dry-run 行为。
- `format = "markdown"` 与 `format = "html"` 的多页静态输出。
- Fields / Indexes / Relations / Assets / Text-POT / Validators / Source map / Fragments / Relationship graph 的 normalized model 与渲染。
- target-aware table / field / index / relation visibility。
- docs-only target 不依赖 source data / runtime data。
- artifact path planning、非法文件名拒绝、文件名冲突拒绝、输出路径冲突检查。

不阻塞 Stable：

- JSON Schema、Mermaid、DOT、ER 图导出。
- HTML 单页 bundle、搜索、过滤、排序等交互增强。
- Lark / Confluence 方言发布。
- owner/domain 分组。
- 读取真实数据后的 asset reference / text value 清单。

## Step 1. 收口 Project Config 配置面

- 在 `SchemaDocExportConfig` 与 Project TOML 解析中明确支持或拒绝以下选项：
  - `include_hidden`
  - `include_internal`
  - `include_struct_details`
  - `include_asset_rules`
  - `include_i18n`
- 推荐 Stable 口径：
  - `include_hidden = false` 默认过滤 hidden 内容；显式开启时展示 hidden table / field / index / relation，并在可见性 badge 中标记 `hidden`。
  - `include_internal = false` 默认过滤 internal 内容；显式开启时展示 internal 内容，并标记 `internal`。
  - `include_struct_details = true` 默认保留当前已实现的 struct / fragment 基础展开；关闭时只保留顶层字段摘要。
  - `include_asset_rules = true` 默认展示当前可静态解析的 asset rule 摘要；关闭时仅标记 asset kind。
  - `include_i18n = true` 默认展示 Text / POT 摘要；关闭时隐藏 Text / POT 专区和 `enters_pot` 类列。
- 未纳入实现的 option 必须显式报错，不允许静默忽略。
- 更新 `Documentation/Design/11-schema-doc-generator.md` 的配置示例和当前边界说明。

验收标准：

- TOML 中设置上述选项后，build 行为与文档一致。
- 未知 schema_doc option 仍然报结构化错误。
- `cargo test schema_doc` 通过。

## Step 2. 补齐 hidden / internal 可见性语义测试

- 增加 builder 单元测试覆盖：
  - hidden table 默认不进入 index / source graph / relationship graph。
  - internal table 默认不进入 index / source graph / relationship graph。
  - hidden / internal field 默认不进入 Fields、Relations、Assets、Text-POT、Validators、Fragments。
  - `include_hidden` / `include_internal` 开启后，对应内容进入 normalized model，且 visibility 文案稳定。
- 增加 renderer 测试覆盖：
  - Markdown 输出 hidden / internal badge 或短文本。
  - HTML 输出 hidden / internal badge，且关系卡片、source edge、fragment card 不丢失标记。
- 增加 CLI integration 覆盖至少一个 target 组合：默认过滤与显式 include 的输出差异。

验收标准：

- hidden/internal 的过滤逻辑只在 builder / visibility helper 层发生，renderer 不重新解释 Meta。
- Markdown 与 HTML 对同一 normalized model 的可见性表达一致。
- `cargo test schema_doc` 与 `cargo test --test cli_integration schema_doc` 通过。

## Step 3. 强化大型 schema 与关系图确定性

- 增加包含多表、多 FK、self-reference、list FK、ObjectTable、fragment、variant source override 的测试 fixture 或单元构造。
- 断言以下顺序稳定：
  - index tables 顺序。
  - relationship graph nodes / edges 顺序。
  - source graph nodes / edges 顺序。
  - table neighborhood outgoing / incoming 顺序。
  - fragment graph fragments / fields 顺序。
- 统一 `generated_at` 处理策略：
  - 若已有固定时间注入能力，snapshot/golden 使用固定时间。
  - 若暂无固定时间注入能力，测试只断言非时间主体内容，并在文档中注明 `generated_at` 是唯一允许变化字段。
- 覆盖 HTML 文件名与 table link deterministic，避免大小写、空格、路径分隔符和重复 normalized path 导致不稳定。

验收标准：

- 相同输入连续生成两次，除 `generated_at` 外输出内容一致。
- 大型关系图不依赖 HashMap 随机迭代顺序。
- `cargo test schema_doc` 通过。

## Step 4. 将标准工程 schema_doc 输出纳入稳定回归

- 在 `tests/fixtures/standard_project/ConfigForge.toml` 中确认保留标准 schema_doc export task：
  - Markdown docs target。
  - HTML docs target。
- 更新标准工程输出 fixture：
  - `out/schema_docs/`
  - `out/schema_docs_html/`
- 在 CLI integration 中覆盖：
  - docs-only target 不需要 `project.input`。
  - dry-run artifact plan 包含 index 与所有 table pages。
  - direct / temp-commit 写入模式输出文件集合一致。
  - schema_doc 与 runtime data/codegen export 同 target 时，路径冲突能提前报错。
- 明确 fixture 输出中允许变动的字段；如 `generated_at` 影响版本库 diff，应引入固定时间或测试忽略策略。

验收标准：

- 标准工程重新执行：
  - `cargo run -- --project "tests/fixtures/standard_project/ConfigForge.toml" --target schema_docs`
  - `cargo run -- --project "tests/fixtures/standard_project/ConfigForge.toml" --target schema_docs_html`
- 生成结果与预期 fixture 一致。
- `cargo test` 不因旧生成文件或输出目录残留失败。

## Step 5. 错误处理与 Artifact 行为收口

- 为以下错误场景补齐测试或检查现有覆盖：
  - unknown `format`。
  - `output` 缺失。
  - `index_file` 非纯文件名。
  - table file name 包含 path separator / parent escape。
  - 不同 table 归一化后文件名冲突。
  - schema_doc artifact 与其他 export artifact 输出路径冲突。
  - Meta link / validation 失败时不生成 schema_doc。
- 确认错误使用结构化 diagnostic / `ConfigForgeError` 口径，不新增 legacy string-only 业务错误入口。
- 确认 dry-run、temp-commit、direct writer 使用统一 `ArtifactSink`，不绕过 artifact planner。

验收标准：

- 失败场景有明确错误码 / source / hint 或现有结构化错误格式。
- dry-run 只规划不写文件；temp-commit 不留下半成品。
- `cargo test schema_doc` 与相关 artifact writer tests 通过。

## Step 6. Markdown / HTML 输出质量冻结

- Markdown：
  - 表格 escape 覆盖 `|`、换行、控制字符。
  - 空值统一 `-`。
  - 布尔值统一 `yes` / `no`。
  - 表与字段链接使用相对路径。
- HTML：
  - 所有用户可控文本 HTML escape。
  - 无外部 JS / CSS / CDN。
  - 页面在无网络环境可打开。
  - index 与 table 页的导航链接均为相对链接。
  - relationship/source/fragment 图例默认启用，并保持纯静态 card graph 形态。
- 明确 Stable 后兼容策略：
  - 可新增 section 或列，但不应无理由重命名已有列标题。
  - relation cardinality 文案保持 `many-to-one` / `list-to-many`。
  - `Allow Empty` 列和 `allow_empty` badge 保持一致。

验收标准：

- Renderer 单元测试覆盖 Markdown escape 与 HTML escape。
- 标准工程 HTML index/table 页人工打开检查无明显布局断裂。
- `cargo test schema_doc` 通过。

## Step 7. 文档状态更新到 Stable

- 更新 `Documentation/Design/README.md`：将 `schema / table docs` 状态从 `Implemented MVP` 调整为 `Stable`。
- 更新 `Documentation/Design/11-schema-doc-generator.md`：
  - `## 11.12 当前状态` 改为 `Stable`。
  - 已实现内容补充 Stable 配置面与回归保障。
  - 当前边界保留 JSON Schema / Mermaid / 交互增强等非 Stable 范围说明。
  - 测试计划标记已覆盖项和后续扩展项。
- 如 Implementation 文档中仍称关系图为阶段性实现，更新为当前稳定行为。

验收标准：

- Design README 状态矩阵与 schema_doc 详细设计一致。
- 文档中不再把 JSON Schema / Mermaid 描述为进入 Stable 的必要条件。
- 用户能从配置示例理解 Stable 支持的全部 schema_doc options。

## Step 8. 最终质量门禁

- 运行格式化：
  - `cargo fmt`
- 运行聚焦测试：
  - `cargo test schema_doc`
  - `cargo test --test cli_integration schema_doc`
- 运行全量 Rust 测试：
  - `cargo test`
- 如本轮修改触及 lint 相关代码，运行：
  - `cargo lint`
- 重新生成标准工程 schema docs：
  - `cargo run -- --project "tests/fixtures/standard_project/ConfigForge.toml" --target schema_docs`
  - `cargo run -- --project "tests/fixtures/standard_project/ConfigForge.toml" --target schema_docs_html`

Stable 完成判定：

- 上述命令通过或有明确、可接受的环境性跳过说明。
- Design 状态已更新为 Stable。
- 标准工程 Markdown / HTML schema docs 已重新生成并纳入预期 fixture。
- 不存在未说明的 schema_doc TODO、配置项静默忽略或测试 fixture 过期问题。
