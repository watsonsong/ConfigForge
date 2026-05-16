# ConfigForge Consolidated Design

本目录是 ConfigForge 的**收敛版设计说明**。它是当前唯一的产品设计入口，作为后续技术概要设计、详细设计、用户手册和 QA 测试用例设计的共同输入。

## 文档定位

- 本目录描述 ConfigForge 作为工具应具备的产品能力、配置模型、执行阶段和用户可见行为。
- 本目录表达**当前确认后的设计口径**；已收敛到本目录的旧设计文档不再保留。
- 本目录不是 Rust 模块设计，也不限定具体代码结构；技术概要设计和详细设计应在此基础上继续拆分实现方案。

## 阅读顺序

1. `01-overview.md` — 工具目的、用户角色、要解决的问题、术语和范围。
2. `02-execution-pipeline.md` — CLI 从读取配置到生成产物的完整阶段、输入输出和阶段要求。
3. `03-configuration.md` — Project、Meta、Source、Target、Plugin 等配置文件的定义和示例。
4. `04-meta-model.md` — 表、字段、类型、默认值、索引、外键、多源、Fragment 等 Meta 语义。
5. `05-data-validation.md` — 数据视图、过滤、缺省值、校验规则、诊断和确定性要求。
6. `06-export-cli.md` — Runtime data format、special exporter、artifact、CLI 命令和参数设计。
7. `07-user-manual-and-qa.md` — 用户工作流、错误处理期望和 QA 测试用例输入维度。

## HTML 阅读版

- `html/index.html` — 面向人类阅读和评审的单页 HTML 设计说明，包含流程图、配置图、数据视图图例和 QA 摘要。

## 当前明确的设计决策

- Meta 支持多文件，通过 `imports`/`include` 引入其他 TOML 或 XML Meta 定义。
- Source provider 第一阶段使用明确 provider：`csv`、`xlsx`、`lark_sheet`、`google_sheet` 等；不再单独设计 `local_file` provider。
- `override_sources` 不再定义 `priority`；覆盖顺序由列表顺序决定，后声明的 override source 覆盖先声明的结果。
- 取消 `primary_key` 概念，仅保留 `primary_index`；也允许在某个 index 上声明 `primary = true` 作为等价写法。
- 字段可声明 `default`。当数据未提供值且存在 default 时使用 default；`required = true` 且无值、无 default 时为错误。
- Meta 各层级定义可声明 `desc`，作为结构化描述进入 MetaModel，并用于 inspect 输出、生成代码注释和辅助文档。
- 禁止 `color_channel = "auto"`。颜色通道必须显式声明为 `byte` 或 `normalized`。
- `vector3` 等常见数学类型优先作为 builtin value type，而不是 named struct。
- 第一阶段不支持 fragment join mode；多态表只支持 variant source mode。
- 暂不设计“独立子表式明细布局”作为当前必备布局能力；复合值布局统一描述为聚合书写或展开书写，具体物理方向由表类型决定。
- Protobuf 暂作为未来扩展考量；如未来支持，应通过字段级 `field_number` 明确 Protobuf 编号。
- Special exporter 使用与目标导出一致的过滤后数据视图。被 shipping 等上下文排除的数据，其资源和国际化文本默认不进入最终特殊产物。
