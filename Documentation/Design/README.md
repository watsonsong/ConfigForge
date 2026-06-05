# ConfigForge Consolidated Design

本目录是 ConfigForge 的**收敛版设计说明**。它是当前唯一的产品设计入口，作为后续技术概要设计、详细设计、用户手册和 QA 测试用例设计的共同输入。

## 文档定位

- 本目录描述 ConfigForge 作为工具应具备的产品能力、配置模型、执行阶段和用户可见行为。
- 本目录表达**当前确认后的设计口径**；历史推导、临时方案和阶段记录放在 `../DevLogs/`。
- 本目录不是 Rust 模块设计，也不限定具体代码结构；技术概要设计和详细设计应在此基础上继续拆分实现方案。

## 阅读顺序

1. `01-overview.md` — 工具目的、用户角色、要解决的问题、术语和范围。
2. `02-execution-pipeline.md` — CLI 从读取配置到生成产物的完整阶段、输入输出和阶段要求。
3. `03-configuration.md` — Project、Meta、Source、Target、Plugin 等配置文件的定义和示例。
4. `04-meta-model.md` — 表、字段、类型、默认值、索引、外键、多源、Fragment 等 Meta 语义。
5. `05-data-validation.md` — 数据视图、过滤、缺省值、校验规则、诊断和确定性要求。
6. `06-export-cli.md` — Data export format、artifact、CLI 命令和参数设计。
7. `07-user-manual-and-qa.md` — 用户工作流、错误处理期望和 QA 测试用例输入维度。

## HTML 阅读版

- `html/index.html` — 面向人类阅读和评审的单页 HTML 设计说明，包含流程图、配置图、数据视图图例和 QA 摘要。

## 当前明确的设计决策

- Project Config 支持顶层 `imports` 递归导入其他工程配置片段；Meta 文件本身不再声明 imports。
- Source provider 第一阶段使用明确 provider：`csv`、`xlsx`、`lark_sheet`、`google_sheet` 等；不再单独设计 `local_file` provider。
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
- POT、asset manifest 等聚合 data exporter 使用与目标导出一致的过滤后数据视图。被 shipping 等上下文排除的数据，其资源和国际化文本默认不进入最终聚合数据产物。
- Target 使用 `tags` 描述导出上下文；表、字段、索引用 `export_when` 匹配 target tags，collection 行用 `row_export` 控制行级导出。
- 不再保留 `tool_only`、`validator_only`、`filter_field` 作为导出机制；用 `export = false`、`export_when`、`row_export` 替代。
- 表格允许空行和空列；未映射到 field path 的行/列默认不进入导出，可作为表格注释或中间数据使用。
- 并发执行应优先区分 I/O 与计算任务：I/O 使用异步接口或系统机制避免大量阻塞线程，计算任务使用线程池，并保持诊断和产物顺序确定。
- Project Config 支持 `copyright` 元信息，供代码生成器和导出器生成文件头。
- Artifact Writer 的写入策略可配置为直接写入或临时文件提交；项目可按构建流程选择。
- 当前不引入通用配置版本兼容机制；表和字段依赖稳定字符串 `name`，Protobuf 等格式未来通过显式 `field_number` 处理格式兼容。

## 当前实现状态矩阵

本目录描述目标态设计与当前确认行为。为避免把目标态误解为已完成能力，当前实现状态按以下口径阅读：

- **Stable**：已实现并有常规测试覆盖，可作为当前用户契约。
- **Implemented MVP**：已形成最小闭环，但后续 API、诊断或生成器能力仍可能扩展。
- **Partial**：只实现了目标能力中的明确子集，未实现部分必须显式报错或在文档中标注。
- **Planned / Not implemented**：目标态保留，当前代码不应静默接受。

| 能力 | 当前状态 | 当前实现边界 |
| --- | --- | --- |
| TOML Project / Meta loading | Stable | 支持 Project imports、多个 Meta 文件 merge、link + validate。XML 仅为历史/目标态示例，当前未实现。 |
| CSV source provider | Stable | 当前唯一可执行 provider。`xlsx`、`lark_sheet`、`google_sheet` 为 planned / not implemented。 |
| `sources` 多源 | Implemented MVP | 当前支持同一逻辑表多个 CSV source 追加 records，主要服务 polymorphic variant source mode。 |
| `override_sources` | Implemented MVP | 当前支持 CollectionTable CSV override：按 primary index 整行覆盖，支持 `export_when` 绑定赛季/热修 tag；ObjectTable override、base source `export_when` 和非 CSV provider 仍未实现并会显式拒绝。 |
| ObjectTable | Implemented MVP | 当前支持 CSV `key,value` 行式常量表：每行一个 top-level field key，导出单个 object；支持 ObjectTable 作为 FK source 引用 CollectionTable unique/primary index；多源/override、作为 FK target、后续格子 expand 暂不实现。 |
| primitive / enum fields | Stable | 支持 primitive、enum link、default 校验和数据值校验。 |
| struct/list fields | Implemented MVP | 支持 named struct、JSON cell 校验和 nested object 导出；支持 CollectionTable/ObjectTable 中由 source path 自动判定的 struct expanded value、recursive nested struct、JSON/expanded 混用错误、target-aware child filtering 与 child default；支持 `T[]` / `list<T>` JSON cell 标量数组、数组导出和代码生成。map、struct/list expanded path、struct child validators、ObjectTable key-level source location 暂未实现。 |
| scalar/list default | Stable | 支持 primitive / enum scalar default；支持 primitive / enum list default。复杂 struct/table default 当前不支持。 |
| target tags / export_when / row_export | Stable | 已接入 validation、runtime export 和 codegen target-aware 视图。 |
| validators | Implemented MVP | 已有 validator registry 和 `range` field validator；后续可扩展更多 built-in/plugin validators。 |
| fragments | Implemented MVP | 已支持 polymorphic variant source mode；fragment join mode 不实现。 |
| foreign keys | Implemented MVP + Rust/C++/TypeScript/C# linked helpers | 支持 field-level/table-level FK、link/meta validate、target-aware referential integrity 和 CodeModel IR；source table 可为 CollectionTable 或 ObjectTable，target table 必须是 CollectionTable unique/primary index；标量数组字段可作为单字段 FK，逐元素引用单字段 unique/primary index。FK 可声明 `linked_field`；Rust/C++/TypeScript/C# codegen 已生成 lazy cached relation getter、row-index cache 和 `link_all` / `linkAll` / `LinkAll` 预解析 API。 |
| runtime data export | Implemented MVP | 支持 JSON、MessagePack 与 `bin` registry；`bin` 私有紧密二进制格式已实现 CFGB header、schema revision 与 positional typed payload；`gettext_pot` / `pot` 聚合 exporter 已实现 MVP，用于抽取可见 `text` 字段生成 POT；`lua_table` / `lua` 内嵌数据 exporter 已实现 MVP，用于生成 Lua data+code 合一产物；大表 streaming writer 仍为中期优化。 |
| codegen backends | Implemented MVP + Rust/C++/TypeScript/C# lazy + linked + text helpers | 已有 Rust / TypeScript / C++ / C# 基础生成、loader/repository helper 和 target-aware IR；四种语言均已支持 `LazyGameData`、`linked_field` relation getter、row-index cache、table `link` / DataSet 级预链接 API，以及 `text` translation helper。 |
| generated runtime loader contract | Implemented MVP + lazy loading implemented | Rust / C++ / TypeScript / C# 生成代码使用 Meta-agnostic `open + RuntimeDataReader + generated parser` 契约；用户 loader 新增表/字段时不变。Eager facade 一次加载所有表后释放 loader；Lazy facade 保留 loader，首次访问 table/object 时加载并缓存。 |
| TypeScript sync loading | Implemented MVP | 已生成 `GameData.loadSync()`、lazy sync accessor、`preloadAllSync()` 与 `linkAllSync()`；sync API 要求 loader 同步返回 reader，遇到 Promise 应报错。 |
| standard runtime helpers | Partial | 顶层 `runtime/` 已有 Rust JSON/MessagePack copyable templates、TypeScript core/JSON/MessagePack/node-fs copyable package templates、C++ RapidJSON JSON adapter、C++ msgpack-cxx MessagePack adapter、C++ CMake targets 与初始 compliance fixtures；已声明 reader contract version。尚缺自动 compliance runner、Rust stable runtime crate / TypeScript stable npm packages 与 C++ stable interface header 完整接入。核心 generated code 仍不强依赖这些三方库。 |
| diagnostics | Implemented MVP | 已有 `src/diagnostic/` 共享 severity、location、entry formatting、report aggregation 和 crate error facade；MetaDiagnostic 与 ValidationIssue 已接入统一格式化。尚未为所有 Data/Exporter/Codegen 错误补齐领域 diagnostic code、span/hint 和完整 source trace。 |
