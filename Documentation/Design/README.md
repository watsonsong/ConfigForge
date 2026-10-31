# ConfigForge Consolidated Design

本目录是 ConfigForge 的**收敛版设计说明**。它是当前唯一的产品设计入口，作为后续技术概要设计、详细设计、用户手册和 QA 测试用例设计的共同输入。

## 文档定位

- 本目录描述 ConfigForge 作为工具应具备的产品能力、配置模型、执行阶段和用户可见行为。
- 本目录表达**当前确认后的设计口径**；历史推导、临时方案和阶段记录放在 `../DevLogs/`。
- 本目录不是 Rust 模块设计，也不限定具体代码结构；技术概要设计和详细设计应在此基础上继续拆分实现方案。

## 阅读顺序

1. `01-overview.md` — 工具目的、用户角色、要解决的问题、术语和范围。
2. `02-execution-pipeline.md` — CLI 从读取配置到生成产物的完整阶段、输入输出和阶段要求。
3. `03-configuration.md` — Project、Meta、Source、Target 的当前配置及未来插件边界。
4. `04-meta-model.md` — 表、字段、类型、默认值、索引、外键、多源、Fragment 等 Meta 语义。
5. `05-data-validation.md` — 数据视图、过滤、缺省值、校验规则、诊断和确定性要求。
6. `06-export-cli.md` — Data export format、artifact、CLI 命令和参数设计。
7. `07-user-manual-and-qa.md` — 用户工作流、错误处理期望和 QA 测试用例输入维度。
8. `08-asset-validation-and-manifest.md` — Asset 引用校验、asset kind、target-aware 资源策略、导出补齐和 Asset Manifest 设计。
9. `09-builtin-validators.md` — 游戏常用内置 validators 设计，包括 regex、string_length、sum、monotonic、required_when、compare 与 range 增强方向。
10. `10-designer-validation-report.md` — 面向策划 / QA / CI 平台的 Designer Validation Report 设计，包括 CLI/build 报告导出入口、HTML/Markdown/JSON/CSV 输出、issue enrichment、owner/domain 分组和当前扩展边界。
11. `11-schema-doc-generator.md` — 面向策划 / QA / 服务端 / 客户端协作的 Schema / Table 文档生成器设计，包括 `schema_doc` export task、Markdown / HTML 输出结构、target-aware 可见性、字段/index/FK/asset/POT 摘要和当前扩展边界。

## HTML 阅读版

[`html/index.html`](html/index.html) 是本目录 Markdown 的静态完整阅读版，包含目录、状态矩阵和所有设计正文。Markdown 是唯一编辑源；使用 `npm ci --prefix tools/docs --include=dev`、`npm run docs:build --prefix tools/docs` 从仓库根刷新 HTML，使用 `npm run docs:check --prefix tools/docs` 检查同步状态。工具仅用于维护文档，HTML 离线阅读无需 Node 或网络。详见 [文档维护流程](../Tech/documentation-maintenance.md)。

## 当前明确的设计决策

- Project Config 支持顶层 `imports` 递归导入其他工程配置片段；Meta 文件本身不再声明 imports。
- Source 使用明确 provider：当前支持本地 `csv`、`xlsx`；`lark_sheet`、`google_sheet` 为未来扩展。字符串 source 始终是 CSV 简写，不按扩展名推断 provider。
- `override_sources` 不再定义 `priority`；覆盖顺序由列表顺序决定，后声明的 override source 覆盖先声明的结果；source-level `export_when` 用 effective target tags 决定某个 override source 是否参与本次构建。
- 取消 `primary_key` 概念，仅保留 `primary_index`；也允许在某个 index 上声明 `primary = true` 作为等价写法。
- 字段可声明 `default`。当数据未提供值且存在 default 时使用 default；`required = true` 且无值、无 default 时为错误。
- Meta 各层级定义可声明 `desc`，作为结构化描述进入 MetaModel，并用于生成代码注释和辅助文档。
- 禁止 `color_channel = "auto"`。颜色通道必须显式声明为 `byte` 或 `normalized`。
- `vector3` 等常见数学类型优先作为 builtin value type，而不是 named struct。
- 第一阶段不支持 fragment join mode；多态表只支持 variant source mode。
- 暂不设计“独立子表式明细布局”作为当前必备布局能力；复合值布局统一描述为聚合书写或展开书写，具体物理方向由表类型决定。Struct source layout 由 CSV/source 中出现 parent path 还是 child paths 自动决定 JSON cell 或 expanded value；CollectionTable 展开表现为多列 `field.child`，ObjectTable 展开表现为多行 `field.child` key。
- Field path 保留字符为 `.`, `[`, `]`；table field、struct field、fragment name 第一阶段均不允许包含这些字符，特殊展示名应通过 header alias / source mapping 解决。
- Protobuf 暂作为未来扩展考量；如未来支持，应通过字段级 `field_number` 明确 Protobuf 编号。
- POT、asset manifest 等聚合 data-backed artifact generator 使用与目标导出一致的过滤后数据视图。被 shipping 等上下文排除的数据，其资源和国际化文本默认不进入最终聚合数据产物。
- `asset` 字段必须显式声明 `asset_kind`；资源存在性校验、导出补齐和 Asset Manifest 默认只处理当前 target ExportView 中可见且非空的 asset 引用。Asset Manifest 已作为同级聚合 data-backed artifact generator 接入，支持 deterministic JSON 与 Unreal DataTable 兼容 CSV。
- Target 使用 `tags` 描述导出上下文；表、字段、索引用 `export_when` 匹配 target tags，collection 行用 `row_export` 控制行级导出。
- 不再保留 `tool_only`、`validator_only`、`filter_field` 作为导出机制；用 `export = false`、`export_when`、`row_export` 替代。
- 表格允许空行和空列；未映射到 field path 的行/列默认不进入导出，可作为表格注释或中间数据使用。
- 本地 CSV/XLSX 读取和 CPU 工作复用局部 Rayon pool，artifact 使用同步 streaming sink；远端 I/O runtime 尚未引入。并行执行保持诊断和产物顺序确定，详见 [并发策略](../Tech/task-system-design.md)。
- Project Config 支持 `copyright` 元信息，供 artifact generator 生成文件头。
- Artifact Writer 的写入策略可配置为直接写入或临时文件提交；项目可按构建流程选择。
- 内存模型采用“Meta 与 Data 全量物化到内存，artifact 内容通过 sink-first streaming 输出”的简化原则。也就是说，设计上不引入 row paging、out-of-core DataSet、按 target 反复读取 source 等复杂机制来降低 Meta/Data 驻留内存；后续优化应优先保持 parsed Meta/DataSet 的不可变共享模型，并只在 artifact 输出边界避免把完整文件内容重新 materialize 为 `String` / `Vec<u8>`。
- 当前不引入通用配置版本兼容机制；表和字段依赖稳定字符串 `name`，Protobuf 等格式未来通过显式 `field_number` 处理格式兼容。

## 当前实现状态矩阵

本目录描述目标态设计与当前确认行为。为避免把目标态误解为已完成能力，当前实现状态按以下口径阅读：

- **Stable**：已实现并有常规测试覆盖，可作为当前用户契约。
- **Implemented MVP**：已形成最小闭环，但后续 API、诊断或生成器能力仍可能扩展。
- **Partial**：只实现了目标能力中的明确子集，未实现部分必须显式报错或在文档中标注。
- **Planned / Not implemented**：目标态保留，当前代码不应静默接受。

| 能力 | 当前状态 | 当前实现边界 |
| --- | --- | --- |
| TOML Project / Meta loading | Stable | 支持 Project imports、多个 Meta 文件 merge、link + validate。XML 当前未实现，也不作为当前配置契约。 |
| CSV source provider | Stable | 已实现的本地 provider 之一。CollectionTable CSV source 支持 source-level `header_row` / `data_start_row`，均为 1-based 行号，默认 `header_row = 1`、`data_start_row = 3`，第 2 行默认保留给策划标题/说明；ObjectTable `key,value` source 则默认用标题列表达为 `key,title,value`，不额外占用标题行。`lark_sheet`、`google_sheet` 为 planned / not implemented。 |
| `sources` 多源 | Stable | 当前稳定支持同一逻辑表多个 CSV/XLSX source（允许混用）：CollectionTable variant source mode 按声明顺序追加 records；ObjectTable 多 base source 按互斥 field path split merge 为一个 singleton object。通用 CollectionTable split merge、base source `export_when` 和远端 provider 仍未实现并会显式拒绝。 |
| `override_sources` | Stable | 当前稳定支持 CSV/XLSX override：CollectionTable 按 primary index 匹配整行覆盖；ObjectTable 按 exact field path patch 已存在字段。二者均支持 `override_sources[].export_when` 绑定 effective target tags，且按声明顺序后者覆盖前者。override insert/upsert、base source `export_when` 和远端 provider 仍未实现并会显式拒绝。 |
| XLSX source provider | Stable | 已支持本地工作簿、显式 sheet（唯一 sheet 可省略）、物理行列定位、稀疏行列、自定义 header/data 起点、公式缓存校验、CSV/XLSX 混用、CollectionTable/ObjectTable 和 override。日期/时长要求文本，公式不重算。见 [配置语义](03-configuration.md#341-本地-xlsx-输入)。 |
| ObjectTable | Stable | 当前稳定支持 CSV/XLSX `key,value` / `key,title,value` 行式常量表，`title` 列仅供策划阅读且不进入数据：每行一个 field path，导出单个 object；既支持 top-level field key，也支持 struct expanded value 的 dotted key，例如 `starter_reward.xp`、`starter_reward.gold`；list 字段既可用单个 JSON array cell，也可用 element-level path 展开为多行，例如 `starter_reward.item_ids[0]`、`starter_reward.item_ids[1]`。支持多个 CSV/XLSX base source 以互斥 field path split merge，支持 CSV/XLSX `override_sources` 按 exact field path patch 已存在字段，并支持 ObjectTable 作为 FK source 引用 CollectionTable unique/primary index；作为 FK target、独立明细子表/跨格区域式展开暂不实现。 |
| primitive / enum fields | Stable | 支持 primitive、enum link、default 校验和数据值校验。 |
| struct/list fields | Stable | 支持 named struct、JSON cell 校验和 nested object 导出；支持 CollectionTable/ObjectTable 中由 source path 自动判定的 struct expanded value、recursive nested struct、JSON/expanded 混用错误、target-aware child filtering 与 child default；支持 `T[]` / `list<T>` 标量数组的 JSON cell 书写、element-level expanded path 书写（如 `item_ids[0]` / `item_ids[1]`，含 struct child path `starter_reward.item_ids[0]`）、数组导出和代码生成；expanded list 要求从 0 开始连续编号。map、struct child validators 暂未实现。 |
| scalar/list default | Stable | 支持 primitive / enum scalar default；支持 primitive / enum list default。复杂 struct/table default 当前不支持。 |
| target tags / export_when / row_export | Stable | 已接入 validation、runtime export 和 codegen target-aware 视图。 |
| asset validation / completion / manifest | Implemented MVP | 已实现 `asset` 字段强制声明 `asset_kind`、Project `[assets]` / `[[assets.search_paths]]`、Meta `[[asset_kinds]]` + `extends`、filesystem resolver、target ExportView 可见 asset 校验、存在性 / sidecar / 命名 / prefix / regex / 基础大小写检查、runtime export canonical asset string 补齐，以及 `type = "asset_manifest"` 聚合 exporter。Asset Manifest 支持 deterministic JSON 和 Unreal DataTable 兼容 CSV；当前 validation 诊断使用 `CF_DATA_INVALID_ASSET_REF` code；当前使用 CF_DATA_INVALID_ASSET_REF；resolution cache 和 provider 扩展后续推进。 |
| validators | Implemented MVP | 已有 validator registry 和显式 built-in validators：`range`、`regex`、`string_length`、`required_when`、`compare`、`sum`、`monotonic`；基础 required/type/enum/unique/FK 仍由 validation 主流程或专门模块执行，尚未全部迁移为 registry implicit validators。专题设计见 [`09-builtin-validators.md`](09-builtin-validators.md)。 |
| fragments | Stable | 已稳定支持 polymorphic variant source mode，包括 source `fragments` 声明、fragment scoped required/default/type 校验、target-aware validation、runtime nested object export 和 code artifact 类型生成；fragment join mode 不实现。 |
| foreign keys | Stable | 支持 field-level/table-level FK、link/meta validate、target-aware referential integrity 和 CodeModel IR；source table 可为 CollectionTable 或 ObjectTable，target table 必须是 CollectionTable unique/primary index；标量数组字段可作为单字段 FK，逐元素引用单字段 unique/primary index。FK 可声明 `linked_field`；各 codegen backend 的 relation helper 覆盖范围见 [`Codegen backend capability matrix`](../Implementation/09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix)。 |
| runtime data export | Stable | 当前内置 JSON、MessagePack 与 `bin` artifact generator 可作为稳定 runtime table export 契约；`bin` 私有紧密二进制格式已实现 CFGB header、schema revision 与 positional typed payload；`gettext_pot` / `pot`、`lua_table` / `lua`、`asset_manifest` / `assets` 作为 data-backed / mixed generator 已接入同一 `GeneratorRegistry` 与 shared `ArtifactSink`。正常大表按“Meta/Data 内存驻留 + artifact 流式写出”处理；新增格式仍可作为扩展 generator 接入。 |
| codegen backends | Implemented MVP + lazy + linked + text helpers | 内置 backend、type id、输出形态、核心能力和测试/adapter 状态以 [`Codegen backend capability matrix`](../Implementation/09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) 为准。 |
| generated runtime loader contract | Stable | [`Codegen backend capability matrix`](../Implementation/09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) 中的 format-agnostic backend 使用 Meta-agnostic `open + RuntimeDataReader + generated parser` 契约；用户 loader 新增表/字段时不变。Eager facade 一次加载所有表后释放 loader；Lazy facade 保留 loader，首次访问 table/object 时加载并缓存。 |
| TypeScript sync loading | Stable | 已稳定生成 `GameData.loadSync()`、lazy sync accessor、`preloadAllSync()` 与 `linkAllSync()`；sync API 要求 loader 同步返回 reader，遇到 Promise 应报错。 |
| standard runtime helpers | Stable | 顶层 `runtime/` 已有 Rust `reader_core` / JSON / MessagePack / binary Cargo packages、TypeScript core/JSON/MessagePack/binary/node-fs CommonJS workspace packages、C++ core header + RapidJSON / msgpack-cxx / binary CMake targets、C# JSON/MessagePack/binary copyable templates、Java JDK-only JSON/MessagePack/binary copyable templates。标准 helpers 已声明 reader contract version、required/supported contract range、package/template version 与 distribution mode；runtime reader 错误已升级为跨语言 structured kind/code exception/error，并由 shared negative corpus 覆盖。Rust、TypeScript、C++、C# 与 Java 标准 reader 已接入 runtime compliance runner；standard_project 已覆盖 Rust/C++ `runtime_interface_emit = "reference"` manifest、C++ install + `find_package` smoke，以及 C# / Java template metadata。adapter 支持级别单独维护，C# MessagePack、Java JSON/MessagePack 为 legacy-bootstrap，见 [adapter 矩阵](../Implementation/09e-codegen-runtime-adapters.md#当前目录与支持矩阵)。新增 release metadata drift test 防止 generated constants、runtime package/template metadata 与文档约定漂移；Rust package list、TypeScript `npm pack --dry-run` 与 C++ install / `find_package` smoke 已作为发布门禁。正式 registry 发布门禁与 core → adapters 顺序见 Runtime Adapter 专题，核心 generated code 仍不强依赖这些三方库。 |
| diagnostics | Stable | `src/diagnostic/` 已提供 severity、location（含 source column）、entry formatting、report aggregation、code catalog、crate error facade、CLI `--diagnostic-format json` 机器输出和 legacy fallback 债务护栏。Project / Build / CLI、Data loading / resolver / validation、Asset resolver、Artifact writer、Generator registry / option / code artifact / data artifact 等用户可触发高价值错误已迁移为结构化 code/source/location/current/expected/hint；剩余 `ConfigForgeError::Validation` 匹配仅用于兼容旧错误解包或转译，不再作为 ConfigForge-produced 业务错误生成入口。 |
| designer validation report | Implemented MVP | 已在 `ValidationReport` / `DiagnosticReport` 基础上实现 `DesignerValidationReport` enrichment、code catalog、HTML / Markdown / JSON / CSV renderer、CLI `--report/--report-output/--report-title` 导出，以及 build `type = "validation_report"` 附加产物。核心 data validation 与 generator/artifact 结构化诊断的 `current_value` / `expected` / `source_column` 已贯通到 report；当前 owner/domain 主要仍是 fallback，部分 producer 尚未补齐 enrichment，build task 尚未支持 `include_warnings` / `include_errors` / `group_by`。设计见 [`10-designer-validation-report.md`](10-designer-validation-report.md)。 |
| schema / table docs | Stable | 已实现 `type = "schema_doc"` MetaOnly export task，可在 docs-only target 中生成 Markdown（`index.md` + 每表 `.md`）或 HTML（`index.html` + 每表 `.html`）文档，展示 fields、indexes、relations、target visibility、asset/POT/validator 摘要，以及 HTML relationship/source/fragment 图例；支持 `format = "markdown"` / `format = "html"`、`index_file`、`title`、`include_hidden`、`include_internal`、`include_struct_details`、`include_asset_rules`、`include_i18n`，未知 option 显式报错。Stable 契约覆盖 MetaOnly 调度、target-aware 可见性、deterministic 输出、路径冲突检查和标准 fixture 回归；JSON Schema、Mermaid/DOT、搜索过滤、Lark / Confluence 发布和 owner/domain 分组仍为后续扩展。设计见 [`11-schema-doc-generator.md`](11-schema-doc-generator.md)。 |
