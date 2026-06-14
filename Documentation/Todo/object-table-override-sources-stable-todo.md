# ObjectTable Multi-source / Override Sources Stable Todo

## 背景

`override_sources` 当前仍处于 **Implemented MVP**：已支持 CollectionTable CSV override 按 primary index 整行覆盖，并支持 `override_sources[].export_when` 通过 effective target tags 启用或过滤；但 ObjectTable 的 `sources` / `override_sources` 仍被 Meta validate 和 DataLoadPlan 显式拒绝。

目标是把 `override_sources` 推进到 **Stable**，补齐 ObjectTable 多 source / override 的稳定语义、实现、测试和文档闭环。该计划只聚焦当前 CSV provider；`xlsx` / `lark_sheet` / `google_sheet` 等非 CSV provider 仍保持 planned / unsupported。

## 当前实现锚点

- 状态矩阵仍标记 `override_sources = Implemented MVP`，原因是 ObjectTable override、base source `export_when` 和非 CSV provider 未实现：`Documentation/Design/README.md`。
- 设计目标已声明 ObjectTable 多 source 是“多个 source 提供同一对象的不同字段”，override source 以 field path 为单位覆盖：`Documentation/Design/04-meta-model.md`。
- 当前 Meta validate 明确拒绝 ObjectTable `sources` / `override_sources`：`src/meta/validate.rs`。
- 当前 DataLoadPlan 也防御性拒绝 ObjectTable `sources` / `override_sources`，并且 `DataSourcePlan::from_source_ref()` 默认把结构化 source 视为 CollectionRecords：`src/data/plan.rs`。
- 当前 DataLoader merge 只支持：base rows append；CollectionTable override rows 通过 primary key 整行替换。ObjectTable 的 singleton row 和 field-path-level patch 还没有单独 merge 分支：`src/data/loader.rs`。
- CSV ObjectTable reader 已能把 `key,value` / `key,title,value` 多行 source 归一化为单个 logical row，并保留 key 对应的 cell source：`src/data/csv.rs`。

## Stable 目标语义

### 1. Source declaration

- `source = "...csv"` 继续作为 ObjectTable 单 source 的兼容写法。
- `sources = [...]` 可用于 ObjectTable，表示多个基础 CSV key/value source 合并为一个 singleton object。
- `source` 与 `sources` 仍互斥。
- ObjectTable `override_sources = [...]` 可用，执行顺序在所有 base source 之后，且按列表顺序应用。
- `override_sources[].export_when` 继续使用 effective target tags；未匹配的 override source 不读取、不 merge、不参与 validation。
- ObjectTable base `sources[].export_when` 暂不纳入本次 Stable 范围，继续显式 unsupported，避免和 split merge / target-specific base materialization 同时扩大语义面。
- ObjectTable 仍不支持 indexes / primary_index / row_export / fragments / polymorphic / preserve_source_order / unique field indexes / 作为 FK target。

### 2. Base split merge

- 每个 ObjectTable CSV source 仍使用 `key,value` 或 `key,title,value` 布局；每行 key 是 field path。
- 多个 base source merge 为一个 logical object row。
- base source 间的 field path 应默认互斥：同一 field path 在两个 base source 中重复出现时报错，并提示“如果要替换值，应放到 override_sources”。
- 同一个 source 内部重复 key 继续由 CSV reader 报错。
- base source 可以为空；最终仍归一化为 singleton object row `{}`，后续 required/default/type validation 决定是否有效。

### 3. Override field-path patch

- ObjectTable override source 是 patch：只覆盖 override source 中出现的 field path。
- override source 只能覆盖已经由 base sources materialized 的 field path；缺失 target field path 应报错，避免把 override source 当成 target-specific append source 使用。
- 如果需要未来支持新增 field path，应另行设计显式 `override_mode = "upsert"` 或 target-specific base source；不混入本次 Stable。
- 后声明的 override source 可以覆盖先前 override source 已覆盖过的 field path。
- 覆盖时只替换对应 field path 的 value 与 cell source；未出现的 field path 保持原值。
- 覆盖 struct/list expanded path 时按精确 field path 替换，例如 `starter_reward.gold` 只替换该 child path，不隐式删除 sibling path。

### 4. Source trace / diagnostics

- 对 ObjectTable field-level validation，cell source 应成为诊断定位的优先来源，确保被 override 的 key 指向 override source 的真实 row。
- singleton row 的 `RowSourceInfo` 不能再被误解为整张 ObjectTable 唯一来源；需要明确它只是 fallback trace，或扩展一个 ObjectTable merge trace 表达多 source materialization。
- duplicate / missing target / unsupported declaration 等错误必须包含 table、source path、source row number（可用时）、field path/current value/expected/hint。

## 实施步骤

### Step 1 — 冻结 ObjectTable 多源语义和错误模型

- [x] 更新 Design / Implementation 文档中的 ObjectTable multi-source / override 语义，明确 base split merge、override patch、重复 key、缺失 target field path、source trace 的规则。
- [x] 在状态矩阵中先保持 `Implemented MVP`，只补充“Stable 迁移计划已存在”，避免文档先行宣称完成。
- [x] 梳理需要复用的诊断 code：优先复用 `CF_DATA_SOURCE_SCHEMA_INVALID` / `CF_DATA_DUPLICATE_KEY`，如语义不足再新增更精确 code。
- [x] 明确不做项：非 CSV provider、base source `export_when`、ObjectTable FK target、ObjectTable indexes、override upsert/add mode。

验收：文档中可直接回答“ObjectTable 多个 base source 重复 key 怎么办？”、“override source 覆盖不存在 key 怎么办？”、“被 override 字段的错误定位到哪里？”。

### Step 2 — 放开 Meta validate 的 ObjectTable source 声明

- [x] 修改 `validate_object_table_restrictions()`：不再因为 ObjectTable 声明 `sources` / `override_sources` 报 unsupported。
- [x] 保留 ObjectTable 对 row_export / fragments / indexes / primary_index / preserve_source_order / polymorphic / unique field indexes 的限制。
- [x] 修改 `validate_source_declaration()`：ObjectTable `sources.len() > 1` 不再走 CollectionTable variant source mode 限制；CollectionTable 的多 source 限制保持不变。
- [x] 继续拒绝 ObjectTable base `sources[].export_when`。
- [x] 增加 Meta unit tests：
  - [x] ObjectTable accepts single `sources = [...]`。
  - [x] ObjectTable accepts multiple `sources = [...]`。
  - [x] ObjectTable accepts `override_sources = [...]`。
  - [x] ObjectTable base source `export_when` still unsupported。
  - [x] CollectionTable 非 variant 多 source 限制不回退。

验收：Meta 能加载并 link 包含 ObjectTable `sources` / `override_sources` 的模型，且既有 unsupported 约束不被放宽。

### Step 3 — 扩展 DataLoadPlan 支持 ObjectTable source refs

- [x] 让 `DataSourcePlan::from_source_ref()` 或等价构造函数接收 table kind / layout / fields，不再默认 `CollectionRecords`。
- [x] 移除 `DataSourcePlan::from_table()` 中拒绝 ObjectTable `sources` 的防御性 guard，改为为每个 ObjectTable source 生成 `layout = ObjectKeyValueRows`。
- [x] 调整 `primary_key_fields_for_overrides()`：ObjectTable override 不需要 primary key fields，不应报错；CollectionTable override 仍必须有 primary index。
- [x] 为 ObjectTable source ref 应用默认 `data_start_row = 2` 规则，与 legacy `source` 行为一致。
- [x] 增加 Data plan tests：
  - [x] ObjectTable multiple sources produce multiple `DataSourcePlan` entries with `ObjectKeyValueRows`。
  - [x] ObjectTable override source role is `Override` and has empty primary key fields。
  - [x] ObjectTable `override_sources[].export_when` filtering works with target tags。
  - [x] ObjectTable base `sources[].export_when` still fails fast。

验收：不读取文件时，source discovery 已能正确表达 ObjectTable base / override source 序列和布局。

### Step 4 — 实现 ObjectTable materialization merge

- [x] 在 `PendingTable::apply_source()` 按 `DataSourceLayout` 或新的 table materialization kind 分流 CollectionTable 与 ObjectTable。
- [x] ObjectTable base merge：把每个 source 读出的 singleton row 合并到同一个 singleton row。
- [x] ObjectTable base duplicate field path 报错，诊断指向后出现的 source/key 行，并提示使用 override_sources。
- [x] ObjectTable override merge：遍历 override singleton row 中的 field path，替换已存在 field path 的 value 与 `CellSourceInfo`。
- [x] ObjectTable override missing target field path 报错，诊断指向 override source/key 行。
- [x] 支持多个 override source 顺序覆盖，最后一个匹配 source 生效。
- [x] 空 ObjectTable source 保持 singleton row，不因 merge 后无 key 而丢失 table。
- [x] 增加 DataLoader unit tests：
  - [x] two ObjectTable base sources with disjoint keys merge into one row。
  - [x] duplicate base key across sources fails。
  - [x] override partially patches one key and preserves other keys。
  - [x] override missing base key fails。
  - [x] later override wins。
  - [x] override filtered by target tags does not read/apply。
  - [x] cell source for patched key points to override file row。
  - [x] empty object source still yields one singleton row。

验收：DataSet 对 ObjectTable 始终只有一条 logical row，且该 row 是按 base split merge + override patch materialized 后的最终视图。

### Step 5 — 对齐 validation / export / runtime payload

- [x] 审计 validation field lookup 是否优先使用 `DataTable::cell_source()`；对 ObjectTable 被覆盖字段，诊断 source path / row number 应指向 override source。
- [x] 覆盖 required/default/type/enum/struct/list/color/vector/FK 校验在 ObjectTable multi-source materialized row 上的行为。
- [x] 确认 target-aware field filtering 在 merge 后执行，不要求每个 source 单独满足 required。
- [x] 确认 JSON / MessagePack / bin / Lua runtime payload 对 ObjectTable singleton object 不需要额外改动。
- [x] 增加 validation / artifact tests：
  - [x] required field 可由第二个 base source 提供。
  - [x] override source 的 invalid value 诊断定位到 override key row。
  - [x] struct expanded child path 跨 base source 合并后可通过校验。
  - [x] list expanded element path 跨 base source 合并后连续性校验仍正确。
  - [x] ObjectTable FK source 字段由 override source 覆盖后按最终值做引用完整性校验。
  - [x] runtime export 输出 final object，不输出 per-source 中间态。

验收：所有下游模块只看到 ObjectTable 的最终 materialized object，不需要知道其来自单 source 还是多 source。

### Step 6 — 标准工程与 CLI 集成覆盖

- [ ] 在 `tests/fixtures/standard_project` 增加一个 ObjectTable split source 示例，例如基础全局配置拆成 `gameplay.csv` + `economy.csv`。
- [ ] 增加一个 tag-gated ObjectTable override 示例，例如 `global_s1.csv` 只覆盖某个 season 参数。
- [ ] 更新 CLI integration：
  - [ ] 默认 target 不应用 tag-gated ObjectTable override。
  - [ ] `--tag s1` 或对应 target tags 下应用 ObjectTable override。
  - [ ] runtime JSON / MessagePack / bin payload 均包含 final object 值。
  - [ ] 生成代码读取 ObjectTable 字段时得到 final value。
- [ ] 如 schema docs source graph 已展示 source edges，补充 ObjectTable multi-source / override edge snapshot 或断言。

验收：标准工程证明该能力能从 Meta、DataLoadPlan、DataLoader、Validation、Export、Codegen runtime harness 全链路工作。

### Step 7 — 文档收敛并提升状态到 Stable

- [ ] 更新 `Documentation/Design/README.md` 状态矩阵：`override_sources` 从 `Implemented MVP` 提升为 `Stable`，说明范围为 CSV CollectionTable + CSV ObjectTable；仍列出非 CSV provider 和 base source `export_when` 为 planned / unsupported。
- [ ] 更新 `Documentation/Design/01-overview.md`、`02-execution-pipeline.md`、`03-configuration.md`、`04-meta-model.md`、`05-data-validation.md`、`07-user-manual-and-qa.md` 中仍描述 ObjectTable override 未实现的段落。
- [ ] 更新 `Documentation/Implementation/04-meta-module-detail-design.md`、`05-data-module-detail-design.md`、`06-validation-module-detail-design.md`、`10-build-module-detail-design.md` 中的限制说明和流程说明。
- [ ] 更新 `Documentation/Design/html/index.html` 或其生成流程对应源，避免静态站点继续展示旧状态。
- [ ] 在 DevLog 的下一个 phase 中记录 ObjectTable multi-source / override 的决策、实现范围和剩余不做项。
- [ ] 删除或归档本 Todo，避免 Stable 后 Todo 与状态矩阵冲突。

验收：文档体系不再出现“ObjectTable multi-source / override 未实现”的过期描述；状态矩阵、用户手册和实现文档范围一致。

### Step 8 — Final verification gate

- [ ] `cargo fmt`
- [ ] `cargo test`
- [ ] `cargo lint`
- [ ] 针对失败的跨语言 harness，按当前工具链策略区分缺工具 skip 与真实失败。
- [ ] `git diff --check`
- [ ] 手动 review：确认没有把 ObjectTable indexes / FK target / non-CSV provider / base source `export_when` 意外纳入 Stable。

验收：核心测试、相关 CLI integration 和 lint 全部通过后，才能将 `override_sources` 标记为 Stable。

## 风险与关注点

- **RowSourceInfo 语义风险**：ObjectTable 多 source 后 singleton row 不再有唯一物理来源，必须避免 field diagnostic 误用 row-level source。
- **覆盖与追加语义混淆**：如果允许 override 新增 field path，会和 base split merge / target-specific source 产生边界不清；Stable 阶段建议保持“override only”。
- **expanded struct/list 精确覆盖**：以 exact field path patch 最安全，但要确保 validation 对 sibling/default/required 的行为符合最终 materialized row。
- **文档状态漂移**：Design、Implementation、HTML 静态页、用户手册都有旧描述，最后提升 Stable 时必须同步更新。
- **CollectionTable 回归风险**：source planning 和 merge 分流时不能破坏现有 CollectionTable variant source mode 与 primary-key override 行为。

## Definition of Done

- ObjectTable CSV `sources` 可稳定 split merge 为 singleton object。
- ObjectTable CSV `override_sources` 可按 field path patch，支持 target-tag filtering 和声明顺序覆盖。
- 缺失 target key、base duplicate key、unsupported source declaration 都有明确诊断。
- Validation / export / generated-code runtime harness 均基于 final materialized object 工作。
- CollectionTable override 行为保持兼容。
- 文档状态矩阵把 `override_sources` 标为 Stable，并准确列出仍未实现的非 CSV / base source `export_when` 等范围外能力。
