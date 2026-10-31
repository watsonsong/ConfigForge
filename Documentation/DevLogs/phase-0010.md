# Phase 0010：文档结构与一致性审查

## Timebox

2026-10-31。

## Scope 与结论

本次审查 README（含中文）、AGENTS 和 Documentation，对照 CLI、source/resolver、validation、runtime metadata 和标准 fixture 修正现状。

- README 使用了不存在的 full_game_demo 和旧 export 子命令；更新为标准 fixture 的可运行命令，移除失效的 AI extension / CONTRIBUTING 入口与过度能力承诺。
- Design 混用了 Meta imports/XML、插件 registry、远端 source、异步/DAG 和 multi-target 目标态；改为当前 TOML Project imports、CSV/XLSX、单 target branch pipeline，并保留明确标注的未来边界。
- Data/Validation/Struct 文档把已经完成的 resolved view、typed fragment、scalar list 仍当作待开发；清除假想类型、旧迁移步骤与重复说明，保留当前职责、source provenance、key policy 和测试边界。
- reader contract 稳定性与 adapter 支持级别必须分开：C# MessagePack、Java JSON/MessagePack 仍为 legacy-bootstrap；删去无文件对应的 Todo 链接和未来目录树。
- 术语表曾把 tool_only/filter_field/validator_only、color_rgb/color_rgba、CLI 子命令列作推荐词；改为当前名称并区分 Planned 类型/插件能力。
- 删除未注明版本和运行范围的 coverage 百分比，今后只保留可复现测量规范，历史结果/临时待办归本目录。
- Design HTML 改为从 Markdown 完整生成，固定 Marked 依赖并检查同步与链接，避免手工摘要继续漂移。

## Validation Snapshot

- docs:check：52 份当前 Markdown，525 个本地链接，Design HTML 12 份文档与 322 个唯一锚点。
- README 的 client dry-run/build、schema_docs、XLSX client 均成功。
- 手册 CSV/TOML 代码块组装成临时工程后构建成功，断言 shipping 仅输出 Iron Sword。
- CLI 帮助与参数/配置/报告组合失败退出码核对为 0/2/1/1。
- 故意引入断链与过期 HTML 的检查均正确失败，恢复原文后检查成功。
- HTML 本地资源与无外部 JS/CSS 依赖检查成功；浏览器宿主不可用，未完成截图级视觉复核。
- 未改 Rust 行为，未重复执行完整 Rust/runtime 测试。

## 历史材料边界

下列原文只保留当时方案，不代表现在的能力、命令或下一步任务。Data、Validation、Struct、Runtime Adapter 原始长文可在审查前 commit [2db8bc4](https://github.com/watsonsong/ConfigForge/tree/2db8bc4/Documentation/Implementation) 查阅；未将整篇重复复制到日志。其设计结论已收敛到对应当前文档。

当前维护规则见 [文档维护流程](../Tech/documentation-maintenance.md)；当前状态见 [Design 矩阵](../Design/README.md)。

## 原文归档：Documentation/Design/README.md / HTML 阅读版

## HTML 阅读版

- `html/index.html` — 面向人类阅读和评审的单页 HTML 阅读摘要，包含流程图、配置图、数据视图图例和 QA 摘要。它不是 current canonical source；当前产品设计以本目录下的 Markdown 文档（`README.md` 与 `01`～`11`）为准。设计状态、术语或实现边界变化时，应优先更新 Markdown canonical docs；若 HTML 仍对外使用，同一变更应把 HTML 纳入必改检查或明确标注其摘要滞后风险。



## 原文归档：CLI 退出码草案

## 6.7 退出码

| Exit Code | 含义 |
| --- | --- |
| `0` | 成功，无 error。 |
| `1` | 构建、校验或导出失败。 |
| `2` | CLI 参数错误。 |
| `3` | Project/Meta 配置解析失败。 |
| `4` | Source 加载失败。 |

如果存在多个错误，退出码取最主要失败类别，但诊断中应保留全部错误。


## 原文归档：Documentation/Design/08-asset-validation-and-manifest.md / 8.10 分阶段落地建议

## 8.10 分阶段落地建议

### P0-1a：Meta / Project 配置和基础校验

- Meta parser 接受 `asset_kind` 并要求 `type = "asset"` 字段必须声明。
- Project Config 接受 `[assets]` 和 `[[assets.search_paths]]`；Meta 接受 `[[asset_kinds]]`。
- Validation 在 ExportView 上检查 kind、path syntax、extension、sidecar、prefix/suffix、deny prefix、regex、existence 和 case sensitivity。
- 添加诊断 code 与标准 reference_game asset fixture。

### P0-1b：Asset 引用补齐与 runtime export 集成

- 增加可复用 asset resolver，支持 relative / mounted / mounted_object、wrapper stripping、object name 模板、extension lookup 和唯一候选补齐。
- JSON / MessagePack / bin runtime export 输出 canonical asset string；未开启补齐或无需补齐时保持原值语义。
- 标准工程中已有 `skill.skill_asset`、`classes.icon` 等 asset 字段，可先按 Unreal mounted object / class 模式改造测试，并在集成测试临时目录中伪造 `.uasset` 文件验证存在性和补齐。

### P0-2：Asset Manifest exporter

状态：已实现。

- 新增 `type = "asset_manifest"` 聚合 data exporter。
- 输出 deterministic JSON manifest，包含 target tags、kind、path、referenced_by。
- 支持 `format = "csv"` 输出 Unreal DataTable 可导入 CSV。
- 与 JSON / MessagePack / bin runtime export 共用同一次 target validation、ExportView 和 canonicalized asset refs。
- 覆盖 target-aware 行过滤：shipping 排除的 debug row 不进入 manifest。

### P0-3：工具链集成和扩展格式

- 增加 CSV manifest 或 engine-specific transform 示例。
- 补充 Unity Addressables / Unreal Asset Manager / 自研打包器的消费说明。
- 评估 asset provider 插件接口，例如 Unreal Asset Registry、Unity GUID database、远程 CDN catalog。


## 原文归档：Documentation/Design/09-builtin-validators.md / 9.4 推荐分阶段落地顺序

## 9.4 推荐分阶段落地顺序

### P1-2a：低风险 field validators

1. `regex`
2. `string_length`
3. `range` 增强

特点：

- 不需要 group/order；
- 只作用单字段；
- 最容易测试和推广；
- 与现有 registry 结构最一致。

### P1-2b：record validators

4. `required_when`
5. `compare`

特点：

- 需要 row 内多字段读取；
- 要明确 support field / hidden field 语义；
- 但还不涉及表级排序或聚合。

### P1-2c：table validators

6. `sum`
7. `monotonic`

特点：

- 需要 group_by / order_by；
- 需要更强的 deterministic 排序与分组定义；
- 更容易引入“歧义配置”与性能问题，因此放在最后。



## 原文归档：Documentation/Design/09-builtin-validators.md / 9.16 需要讨论确认的开放问题

## 9.16 需要讨论确认的开放问题

建议在进入实现前先确认以下几点：

1. **是否为 `ValidatorMeta.kind` 提供 `type` TOML alias？**
   - 当前文档建议：canonical 仍用 `kind`，alias 可选。

2. **`string_length` 默认长度单位是否用 `unicode_scalar`？**
   - 当前文档建议：是；避免引入 grapheme 级复杂度。

3. **`monotonic.strict` 默认值是否为 `true`？**
   - 当前文档建议：是；更符合成长曲线常见需求。

4. **`sum` 是否只提供通用名 `sum`，不再单独引入 `weight_sum`？**
   - 当前文档建议：是；必要时可加 alias。

5. **record/table validator 是否允许显式读取 hidden support fields？**
   - 当前文档建议：允许，但只限显式引用字段，且不改变 export 行为。

6. **`compare` 首版是否只支持 field-vs-field，不支持 field-vs-literal？**
   - 当前文档建议：是；保持配置简单，先覆盖最常见区间关系。

## 9.17 推荐讨论结论模板

为方便从设计讨论进入实现，建议按以下顺序确认：

1. 先确认 **P1-2a / P1-2b / P1-2c** 的实现顺序；
2. 再确认共享 `when` 条件模型是否接受；
3. 再确认 hidden support field 是否允许；
4. 最后再确认各 validator 的默认值和别名策略。

这样可以避免在实现前把所有边角参数一次性定死。


## 原文归档：Documentation/Design/10-designer-validation-report.md / 10.10 实现分期建议

## 10.10 实现分期建议

### Phase A：最小可用报告（已实现 MVP）

- 新增 report model 和 renderer trait。
- 从 `ValidationReport` 生成 `DesignerValidationReport`。
- 支持 `config-forge --project ... --target ... --report html|markdown|json|csv --report-output ...`。
- HTML/Markdown/CSV/JSON 都输出 summary + issue list。
- 补 source path / row number / source column / current value / expected 的 structured enrichment 通道。
- 建立最小 code catalog，覆盖现有 validators。

验收：

- 有 error 时仍写报告，CLI 退出失败。
- 只有 warning 时报告显示 non-blocking，CLI 成功。
- target validation 下报告包含 target tags。
- 输出顺序稳定。

### Phase B：Build export task（已实现 MVP）

- 新增 `type = "validation_report"` export task。
- 让 build 在 validation 失败时也尽量输出 validation report artifact。
- 支持 `format`、`file_name`、`title` 等配置；`include_warnings` / `include_errors` / `group_by` 暂未实现，当前始终输出所有 issue 并使用固定分组。
- 标准工程增加 HTML report target 或附加 export task。

验收：

- `config-forge build --target client` 可生成 validation report。
- runtime/codegen 因 validation error 阻塞时，report artifact 仍可用于定位。

### Phase C：Owner/domain 与剩余 producer enrichment（Partial）

- 支持 owner/domain 配置或映射。
- 继续迁移 asset、builtin validators、code artifact 等尚未结构化的 producer，补齐 current/expected/hint。
- HTML 增加 owner/domain/code 多视角交互式聚合。

验收：

- QA 可按 owner/domain 分派问题。
- CSV 可回填到表格工具。



## 原文归档：Documentation/Design/11-schema-doc-generator.md / 11.10 实现分期建议

## 11.10 实现分期建议

### Phase A：Markdown 输出（已稳定）

- 新增 `schema_doc` export type，dependency 为 MetaOnly。
- 构建 `SchemaDoc` normalized model。
- 输出 `index.md` 和每张可见 table 的 `*.md`。
- 覆盖 Fields / Indexes / Relations 基础表格。
- 支持 target-aware table/field/index visibility。
- 单元测试覆盖 Markdown escape、文件名冲突、docs-only target、target filtering、hidden/internal include 口径与 deterministic order。

### Phase B：资产、POT 与复杂类型增强（已稳定）

- 展示 asset kind 规则摘要。
- 标记 text 字段是否会进入 POT。
- 展开 struct / fragment / list 字段结构。
- 展示 built-in validators 摘要。
- 增加 relation overview、source map、fragment composition 和跨表 FK 索引页。

### Phase C：HTML 输出格式（已稳定）

- 多页静态 HTML：`index.html` 与每表 `<table>.html`。
- 与 Markdown 共享 `SchemaDoc` normalized model 和 artifact path planning。
- 内联 CSS、自包含页面、无外部 JS/CSS/CDN。
- Index 页包含 summary cards、Tables 总览、Asset fields、Text / POT fields、Relation overview。
- 单表页包含 Fields / Indexes / Relations / Assets / Text-POT / Validators，以及 table neighborhood、source map、fragment composition、Index/FK Contract、Asset/Text Flow legend。
- 对用户可控文本统一 HTML escape。
- CLI integration 覆盖 docs-only target、dry-run artifact plan、temp-commit/direct 写入、path conflict 和实际写入。

### Phase D：更多平台集成

- HTML 单页 bundle、搜索、过滤、排序等增强交互。
- 生成适配 Lark / Confluence 的 Markdown 方言。
- 支持 owner/domain 或表负责人映射后，在 index 中按负责人分组。



## 原文归档：旧 coverage 工具链安装

如果本机 Rust/Cargo 版本较旧，最新 `cargo-llvm-cov` 可能无法编译。当前项目已验证可在旧工具链上安装：

```bash
cargo install cargo-llvm-cov --version 0.4.14 --locked
```



## 原文归档：未注明日期和 commit 的 coverage 快照

## 8. 当前项目覆盖率基线

最近一次覆盖率运行结果：

```text
TOTAL:
Regions   91.17%
Functions 90.60%
Lines     92.92%
Branches  -
```

重点文件：

| 文件 | 行覆盖率 | 说明 |
| --- | ---: | --- |
| `meta/link.rs` | 93.96% | Link 主流程覆盖较充分，仍有少量异常分支可补。 |
| `meta/validate.rs` | 95.63% | duplicate、primary index、export condition、validator meta 等主要校验路径已覆盖。 |
| `meta/loader.rs` | 93.55% | 正常加载、imports、I/O/parse 异常路径已有覆盖。 |
| `meta/model.rs` | 98.36% | 查询 helper、target tags、row export policy 等模型行为覆盖较充分。 |
| `validation/mod.rs` | 64.63% | 基础 required/type/enum/unique 与 target-aware validation 已覆盖；仍有 helper/边界分支未完全覆盖。 |
| `validation/builtins/range.rs` | 81.31% | range validator 的参数解析、target 过滤、warning/error、空值和边界比较已有覆盖。 |
| `validation/context.rs` | 93.58% | ExportValidationView / ExportTableView 主要路径已覆盖。 |
| `validation/diagnostic.rs` | 71.76% | Error/Warning 报告路径已覆盖，格式化分支仍可补充。 |
| `validation/registry.rs` | 88.46% | registry require、scope 校验、compile/run 主要路径已覆盖。 |

该基线不作为长期目标，只用于后续迭代比较。后续新增 Meta、Validation、Exporter 功能时，应逐步提高核心模块覆盖率。

## 9. 当前已识别的补测方向

优先补充：

1. Meta validate 异常分支：duplicate enum / struct / enum value / index、empty index fields、missing primary index、多 primary index。
2. Loader 异常分支：invalid import TOML、更多 import cycle / canonicalize 边界。
3. Data validation 异常分支：更多 target-aware unique/index 边界、ValidationReport warning 格式化分支。
4. Source / Exporter 错误路径：write output failure、重复 output path、重复 target 与 warning 输出组合场景。

这些补测项应结合后续功能迭代逐步完成。


## 原文归档：Documentation/Design/02-execution-pipeline.md / 2.1 总体流程

## 2.1 总体流程

ConfigForge 的一次执行以 Project 配置、Meta 文件、Source 数据和 Target 配置为输入，输出诊断、运行时数据、代码和辅助产物。

推荐总体流程：

```text
CLI Arguments
  -> Project Configuration Resolution
  -> Plugin Registry Initialization
  -> Meta Discovery / Import / Parse / Merge
  -> Source Declaration Resolution
  -> Source Loading
  -> Sheet Normalization and Field Mapping
  -> Full DataSet Materialization
  -> Target Tags Resolution
  -> ExportView Filtering
  -> Validation on Target View
  -> Export Planning
  -> Artifact Generation
  -> Manifest / Diagnostics Output
```

## 2.2 配置解析阶段

输入：

- CLI 参数；
- project config；
- environment variables；
- target selection；
- optional plugin config。

输出：

- resolved project root；
- selected meta roots；
- selected target list；
- selected target tags；
- output policy；
- diagnostic output policy。

要求：

- CLI 参数优先级高于 project 默认配置。
- 所有相对路径都应相对 project root 或声明文件所在目录解析，并在诊断中保留原始写法。
- 配置解析错误应尽量批量报告。

## 2.3 插件注册阶段

输入：

- project plugin manifest；
- CLI 指定的 plugin search path；
- 内置 provider/exporter/validator。

输出：

- SourceProviderRegistry；
- GeneratorRegistry（build-facing artifact generator facade；按 export type id 查询 generator requirements 与执行入口）；
- Data artifact toolkit facet（内部能力，用于复用 runtime table payload、data serialization helper 或 aggregate artifact helper；不作为 Build-facing registry）；
- Code artifact toolkit facet（内部能力，用于复用 CodeModel / writer helper / renderer；不作为 Build-facing registry）；
- ValidatorRegistry；
- BuiltinTypeRegistry。

要求：

- 内置能力先注册。
- 插件按配置声明顺序注册。
- 能力 ID 冲突默认报错；显式 `override = true` 才允许覆盖。
- 插件加载失败应包含路径、插件 ID、版本、API 版本和底层错误。

## 2.4 Meta 加载、Import 与合并阶段

Meta 支持多文件配置。一个 TOML 或 XML Meta 文件可以 import/include 其他 Meta 文件。

流程：

```text
Root Meta Files
  -> Resolve imports recursively
  -> Parse each document
  -> Merge declarations deterministically
  -> Link references
  -> Produce MetaModel
```

要求：

- import 路径默认相对当前 Meta 文件所在目录。
- import 图必须检测循环引用。
- 多文件中 table、enum、named struct、target、validator 等命名冲突必须报错，除非声明了明确的扩展/覆盖语义。
- 合并顺序必须稳定，不依赖文件系统遍历顺序。
- TOML 和 XML 都应映射到同一个 canonical MetaModel。



## 原文归档：Documentation/Design/03-configuration.md / XML 示例：

XML 示例：

```xml
<meta>
  <table name="items" kind="collection" primaryIndex="by_id" desc="所有可获得物品的基础配置。">
    <source provider="csv" path="items.csv" />
    <field name="id" type="int" required="true" desc="物品唯一 ID。" />
    <field name="name" type="text" required="true" desc="物品展示名称。" />
    <index name="by_id" unique="true" primary="true">
      <fieldRef path="id" />
    </index>
  </table>
</meta>
```



## 原文归档：Documentation/Implementation/11-foreign-key-detail-design.md / 11.8 Codegen 与 IR 预留

## 11.8 Codegen 与 IR 预留

Phase 1 不生成语言 helper，但建议在 CodeModel 中预留结构，便于后续演进。

```rust
pub struct CodeForeignKey {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub target_table: String,
    pub target_index: String,
    pub allow_empty: bool,
}
```

如果 Phase 1 想进一步降低范围，也可以暂不改 CodeModel，只保证 Meta / Validation 闭环。推荐策略：

- Phase 1a：只做 Meta + Validation，不动 codegen；
- Phase 1b：CodeModel 暴露 `CodeForeignKey`，但语言生成器不使用；其中 `allow_empty` 表示 helper 返回值是否天然 optional；
- Phase 2：生成关联查询 helper。

未来语言 helper 形态：

| 语言 | 可能生成 API |
| --- | --- |
| Rust | `pub fn drop_item<'a>(&self, row: &MonsterRow, data: &'a GameData) -> Option<&'a ItemsRow>` |
| TypeScript | `dropItem(row: MonstersRow, data: GameData): ItemsRow | undefined` |
| C++ | `const ItemsRow* GetDropItem(const MonstersRow& row, const GameData& data) const` |

具体 API 需要结合 repository ownership、lifetime 和 GameData 聚合结构另行设计。



## 原文归档：Documentation/Implementation/11-foreign-key-detail-design.md / 11.10 实施步骤

## 11.10 实施步骤

建议拆成小提交：

### Step 1：Meta raw model

- 新增 `ForeignKeyMeta`。
- 新增 `FieldForeignKeyMeta`。
- `TableMeta` 增加 `foreign_keys`。
- `FieldMeta` 增加 `foreign_key`。
- public re-export 增加 `ForeignKeyMeta` / `FieldForeignKeyMeta`。
- TOML parse 测试覆盖字段级语法糖、table-level 单字段、复合、默认 `allow_empty`、默认 `target_index`。

### Step 2：Meta validation

- 新增 `validate_foreign_keys()`。
- 新增或复用 diagnostic code。
- 测试：重复 FK name、字段级派生 name 与 table-level name 冲突、空 fields、未知 source field、unknown target table/index、target index 非 unique、target table 无 primary index 但省略 `target_index`、字段数量不一致、字段级 FK 指向复合 index、类型不兼容、fragment path 被拒绝、ObjectTable 作为 source 通过、ObjectTable 作为 target 被拒绝。

### Step 3：Link

- 新增 `LinkedForeignKeyMeta`。
- `LinkedTableMeta` 增加 `foreign_keys` 与 lookup。
- `link.rs` 解析 source fields / target names / target index name。
- 测试 linked lookup。

### Step 4：Validation

- 在 full dataset validation 中执行 FK。
- 在 target validation 中基于 `ExportValidationView` 执行 FK。
- 测试：有效引用、缺引用、allow_empty、部分空复合 key、ObjectTable source 有效引用/缺引用/allow_empty、target row 被 `row_export` 排除、FK `export_when` 跳过、FK 可见但 target table/index 不可见时报错。

### Step 5：文档与 fixture

- 更新 `Documentation/Design/04-meta-model.md` 当前状态。
- 更新 `Documentation/Design/05-data-validation.md` FK validation 语义。
- 在 standard project fixture 中增加一个小型 FK 示例。



## 原文归档：Documentation/Implementation/11-foreign-key-detail-design.md / 11.12 风险与开放问题

## 11.12 风险与开放问题

1. **Full validation 是否受 `export` / `export_when` 影响**
   建议 full validation 检查全部 FK；target validation 才按 target 过滤。若 CLI 的默认 validate 已经绑定 target，则以 target 语义为准。

2. **ObjectTable 外键语义**
   ObjectTable 已可作为 FK source：其 `key,value` source 会归一化为 singleton logical row，外键 source fields 与 CollectionTable rows 使用同一套 raw string key 语义。ObjectTable 仍不能作为 FK target，因为 target 需要 unique / primary index，而 ObjectTable 当前不支持 index / primary index；若未来需要 singleton target 关系，需要单独设计“引用对象字段”或 ObjectTable virtual index 语义。

3. **Codegen helper 的 ownership / lifetime**
   不在 Phase 1 解决，避免阻塞 validation 能力落地。

4. **诊断结构化程度**
   当前很多校验仍桥接到 `ConfigForgeError::Validation(String)`。Phase 1 可保持一致；后续可把 FK 错误迁移到结构化 `ValidationIssueLocation`。

5. **target visibility 下字段不可见时的行为**
   推荐严格报错：一个当前 target 可见的 FK，不应引用当前 target 不可见的 target table、target index、source field 或 target index field。否则 codegen / runtime relation helper 会缺少必要目标或字段。

## 11.13 完成标准

外键 Phase 1 完成时应满足：

- `ForeignKeyMeta` 可从 TOML 加载。
- `FieldForeignKeyMeta` 字段级语法糖可从 TOML 加载，并能规范化为单字段 FK。
- Meta validation 能稳定报告所有结构错误。
- Link 后可从 `LinkedTableMeta` 查询外键关系。
- Full validation 能发现缺引用、空 key、partial composite key。
- Target validation 与 ExportView / row_export / export_when 一致。
- 不支持 fragment/nested field FK 时有明确错误。
- `cargo fmt`、`cargo test`、`cargo clippy --all-targets --all-features` 通过。


## 原文归档：Documentation/Implementation/14-schema-doc-html-relationship-diagrams.md / 14.10 分阶段落地建议

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


## 原文归档：Documentation/Implementation/09a-codegen-runtime-reader-contract.md / 未来也可以支持更轻量的：

未来也可以支持更轻量的：

```json
[
  { "id": 1, "name": "Sword" }
]
```

第一阶段推荐导出 runtime data 时同步调整为 typed payload：

- int/float/bool 输出 number/bool；
- string/text/asset/enum 输出 string；
- struct 后续输出 object。

如果短期仍输出字符串 cell，生成代码必须在加载时做类型转换。这会导致各语言生成重复 parse helper，但不会要求业务 loader 按表/字段重复实现转换逻辑。

建议分两步：

1. Codegen 第一阶段先兼容当前 `{ meta, rows }`，由通用 reader 暴露 runtime value，并由生成代码把字段 parse 为目标类型。
2. Data typed export 落地后，生成代码的 parse helper 可以减少 string parsing；loader / reader 接口仍保持不变。



## 原文归档：Documentation/Implementation/09c-codegen-cpp-backend.md / Unreal C++ backend

## Unreal C++ backend

Unreal Engine 环境不由标准 `cpp` backend 增加一个“大开关”来兼容，而是使用独立 export type：`type = "cpp_unreal"`，内部实现命名为 `CppUnrealCodeGenerator`，输出目录仍由当前 `[[targets.exports]]` 的 `output` 指定。原因：

- 标准 C++ backend 的目标是引擎无关 C++17，public API 使用 `std::vector`、`std::unordered_map`、`std::string`、`std::unique_ptr`、异常等标准库约定。
- Unreal 项目倾向于使用 `TArray`、`TMap`、`FString`、`TUniquePtr` / `TSharedPtr`、`check` / `ensure` / `UE_LOG`、`TEXT()` 字面量和 PascalCase + 类型前缀命名；同一个 public API 混用两套习惯会降低接入体验。
- Blueprint 反射还要求 `USTRUCT` / `UCLASS` / `UFUNCTION` / `UPROPERTY` / `GENERATED_BODY()` 等 UnrealHeaderTool 可解析形态，这会显著影响文件布局、include、类型命名和可暴露 API，不适合塞进标准 C++ emitter 的局部分支。

### Unreal 习惯和生成约束

Unreal backend 需要遵守以下生成约束：

- **容器与字符串**：`std::vector<T>` 映射为 `TArray<T>`；unordered index 的唯一 key 映射为 `TMap<Key, int32>` 或 `TMap<Key, int64>` row handle；非唯一 key 映射为 `TMap<Key, TArray<int32>>`。配置文本字段优先使用 `FString`；如果未来区分本地化 display text，可额外支持 `FText`；name-like 轻量标识符可由选项映射到 `FName`，但默认不自动把 Meta string 改成 `FName`。
- **有序索引**：继续使用排序数组策略，不使用 `TMap` 的排序迭代语义承载 range query。存储形态为 `TArray<TPair<Key, int32>>`；Load 后调用 `StableSort`；`FindBy*`、`LowerBoundBy*`、`UpperBoundBy*`、`RangeBy*` 使用手写二分或 `Algo::LowerBoundBy` / `Algo::UpperBoundBy`。等价 key 的相对顺序必须保持 source row order。
- **复合 key**：生成 `F<Table><Index>Key` value struct，提供 `operator==`、`operator<` 和非成员 `GetTypeHash(const FKey&)`，使其可作为 `TMap` key。hash combine 使用 Unreal 提供的 hash combine helper 或显式组合各字段 `GetTypeHash`。
- **错误模型**：Unreal 模块通常关闭或避免依赖 C++ exceptions。`cpp_unreal` 不应默认生成 `throw std::runtime_error`。第一阶段 containers 模式推荐生成 `FConfigForgeResult<T>` / `FConfigForgeStatus` 或 `bool + OutError` 形态；不可恢复的内部不变量可用 `check`，可诊断但不中断的集成问题可用 `ensure` / `UE_LOG`。所有字符串字面量使用 `TEXT("...")`。
- **命名**：类型使用 Unreal 前缀：普通 value struct 使用 `F`，enum 使用 `E`，Blueprint function library 使用 `U`；bool 字段生成 `b` 前缀；函数、类型、属性默认 PascalCase。内部私有字段不沿用标准 C++ backend 的 `rows_` snake_case，而使用 `Rows` / `ById` 或明确 `Private` 后缀以避开 UPROPERTY 和方法名冲突。
- **文件布局**：containers 模式可生成普通 `.h` / `.cpp`；Blueprint 模式必须生成 UnrealHeaderTool 可处理的 header，包含 `#pragma once`、模块 API 宏、`.generated.h` 必须位于 include 列表末尾。生成器需要配置 `api_macro`，例如 `GAMECONFIG_API`，用于 `UCLASS` / `USTRUCT` 导出。
- **反射限制**：Blueprint public API 不暴露 `std::*`、裸 row pointer、模板返回值、`std::optional`、`std::reference_wrapper` 或复杂迭代器；用 `bool` 返回成功状态，结果通过 `UPARAM(ref)` / out 参数返回，或返回 Blueprint 支持的 `USTRUCT` / `TArray<USTRUCT>`。

### 支持级别

`cpp_unreal` 至少提供两个支持级别：

```toml
[[targets.exports]]
type = "cpp_unreal"
output = "Source/GameConfig/Generated"
module_name = "GameConfig"

[targets.exports.cpp_unreal]
support = "containers" # 或 "blueprint"
api_macro = "GAMECONFIG_API"
namespace = "GameConfig"
```

#### Level 1: `support = "containers"`

目标是让 C++ gameplay code 在 Unreal module 内自然使用生成代码，但不要求 Blueprint 可见：

- 生成普通 C++ `struct` / `class` / repository / `GameData` / `LazyGameData`，不加 `USTRUCT` / `UCLASS`。
- 类型和容器使用 Unreal API：`TArray`、`TMap`、`TPair`、`FString`、`TOptional`、`TUniquePtr` / `TSharedPtr`、`int32` / `int64`。
- reader/loader contract 改成 Unreal 风格，例如 `TUniquePtr<IConfigForgeRuntimeDataReader> Open(const FString& Path, FConfigForgeError& OutError)`；生成代码仍只依赖逻辑 table name，不绑定 Asset Manager、Pak、本地文件或网络。
- relation helper 和 text helper 复用标准 backend 的语义，但返回 Unreal 类型；例如 text helper 默认返回 `FString`，可选对接 `FText` translator。
- 索引 API 可以返回 `const FItemRow*` 给 C++ 使用；但这些指针不出现在 Blueprint 层。

#### Level 2: `support = "blueprint"`

目标是在 containers 模式基础上增加 Blueprint 可见 facade：

- row / object / fragment value type 生成 `USTRUCT(BlueprintType)`，首行 `GENERATED_BODY()`；字段生成 `UPROPERTY(BlueprintReadOnly, Category="ConfigForge")`。
- enum 生成 `UENUM(BlueprintType)`，枚举值按 Unreal 习惯生成 `EItemRarity::Common` 这类强类型枚举；必要时为 display name 添加 metadata。
- table repository 本体仍可保持非 UObject C++ 类型，避免大量配置行进入 GC 管理；Blueprint 层通过 `UBlueprintFunctionLibrary` 或 `UObject` facade 暴露静态/实例函数。
- lookup 函数生成 Blueprint 友好形态，例如：

```cpp
UCLASS()
class GAMECONFIG_API UGameConfigBlueprintLibrary : public UBlueprintFunctionLibrary
{
    GENERATED_BODY()

public:
    UFUNCTION(BlueprintPure, Category="ConfigForge")
    static bool GetItemsById(const FItemsTable& Table, int64 Id, FItemsRow& OutRow);

    UFUNCTION(BlueprintPure, Category="ConfigForge")
    static void FindItemsByRarity(const FItemsTable& Table, EItemRarity Rarity, TArray<FItemsRow>& OutRows);
};
```

- 当前 Blueprint facade 只包装已经加载好的 repository 查询，调用方显式传入 `F<Table>Table`，不会隐式触发 I/O，因此使用 `BlueprintPure`。unique 查询返回 `bool` 并通过 `OutRow` 复制结果；non-unique / ordered range 查询通过 `TArray<F<Row>>` out 参数复制结果。可能进行加载 / I/O 的 Blueprint API 若未来加入，应使用 `BlueprintCallable`，让执行顺序显式。
- 当前不为 `LazyGameData` 生成 Blueprint 加载入口；C++ 层仍可用 `FLazyGameData`。若未来暴露 lazy/eager 加载给 Blueprint，应明确初始化入口，例如 `LoadGameConfig` / `PreloadAll`，不在纯 getter 中隐式打开 runtime data。
- 若暴露全局 singleton，需要由项目 adapter 决定生命周期；生成代码不直接创建 `UGameInstanceSubsystem`，但可以提供可被 subsystem 包装的 facade 类。

### 与现有 C++ backend 的复用边界

`cpp_unreal` 应复用核心语义而不是复用标准 C++ 文本模板：

- **复用**：`CodeExportInput`、`CodeModel`、target tags 过滤后的表/字段/index/foreign key IR、命名冲突检查框架、索引与 relation helper 的语言无关规划、runtime reader 的 Meta-agnostic 分层原则、sink-based streaming 输出基础设施。
- **抽象成共享策略**：index 构建流程、ordered range API 的语义、复合 key 需要 equality / ordering / hash 的判定、`GameData` / `LazyGameData` eager/lazy 结构，可以提取为 backend-neutral helper 或 emitter plan。
- **不直接复用**：`CppNames` 的 `std::*` 类型映射、include 列表、异常生成、snake_case private 字段、`std::optional` / `std::unique_ptr` / `std::shared_ptr` 形态、标准 C++ 的 reader interface 和所有 Blueprint 相关输出。
- **测试复用**：继续使用同一 Meta fixture 验证导出可见性、索引、foreign key、text helper；新增 Unreal 专属 snapshot 断言 `TArray` / `TMap` / `GetTypeHash` / `USTRUCT(BlueprintType)` / `.generated.h` include 顺序 / 无 `std::` 和无 `throw`。


## 原文归档：Actions run 告警评审快照

[run 41168923791](https://github.com/watsonsong/ConfigForge/actions/runs/41168923791)
的五个 job 均成功，5 条 warning 是同一类 Node 20 弃用提示：quality job 引用了
`actions/checkout@v4` / `actions/setup-node@v4`，四个构建 job 各引用了 `checkout@v4`。
它们在 `action.yml` 中声明 Node 20，由 GitHub 强制切换到 Node 24；这不是 Rust 编译告警，
也不是项目通过 `setup-node` 安装的 Node 版本错误。

处理方式与维护规则：

- 检查被引用版本的官方 `action.yml` 中 `runs.using`，以及 release notes 的迁移约束。
  项目 `node-version: "24"` 只影响后续 shell 命令，不能替换 Action 自身的 runtime。
  不用屏蔽告警或设置强制 runtime 环境变量代替 Action 升级。
- CI / Release 的 checkout 统一使用 `actions/checkout@v7`，Node 安装使用
  `actions/setup-node@v7`。这两者原生使用 Node 24；当前仅使用普通 checkout、显式 npm cache，
  没有使用 v7 迁移涉及的 fork privileged checkout 或 registry token fallback。
- 同时检查较少运行的 Release：`upload-artifact@v7`、`download-artifact@v8` 和
  `softprops/action-gh-release@v3` 均原生使用 Node 24，避免发布 tag 时才遇到同类告警。
  产物上传显式 `archive: true`，下载显式 `merge-multiple: false`，保留每个 artifact
  一个目录、目录内包含 release archive / checksum 的布局，供后续 SHA256SUMS 和发布步骤使用。
- Node 24 Action 至少要求 Actions Runner 2.327.1；checkout 的跨 Docker 认证还要求
  2.329.0。该 run 的 hosted runner 为 2.338.2，满足要求；未来使用 self-hosted runner
  时须先核实 runner 版本。Action major、项目 Node major 和 runner agent 是三个独立版本。
- 该 run 另外有 **3 条 notice**：两个 Ubuntu job 的镜像迁移通知，以及 macOS ARM64
  容量通知。它们不是上述 5 条 warning，也不是失败信号。
  Linux job 固定 `ubuntu-24.04`，延续该次成功运行实际使用的 Ubuntu 24.04，避免
  `ubuntu-latest` 自动迁移至 Ubuntu 26 改变系统库和预装工具。固定 OS label 不会冻结
  镜像的每周软件更新；升级 OS 时仍要运行整个目标矩阵，并同步 CI / Release。
- macOS ARM64 排队容量由 GitHub 管理；不通过删除 ARM64 job、替换 Intel 架构或关闭检查
  消除提示。只有持续影响交付时，才评估服务容量或 runner 方案。
