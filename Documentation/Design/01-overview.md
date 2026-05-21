# 01. 工具概述

## 1.1 设计目的

ConfigForge 是面向游戏配置生产流程的命令行工具。它把策划在表格中维护的数据，按照工程师定义的 Meta 结构进行解析、组合、过滤、校验，并导出为游戏运行时或构建流水线可直接消费的产物。

核心目标：

1. **统一数据源真相**：Meta 是导出数据结构、字段类型、索引、外键和校验规则的唯一结构真相。
2. **降低策划表格维护成本**：支持 CSV、XLSX、远程表格等来源，让策划保留熟悉的二维表编辑方式。
3. **在导出前发现配置错误**：通过确定性的校验阶段发现缺字段、类型错误、索引冲突、外键缺失、资源缺失等问题。
4. **生成运行时友好产物**：输出 JSON、Lua table、SQLite 等运行时数据，以及代码、资源清单、国际化模板等辅助产物。
5. **支持多目标导出**：同一份 Meta 与源数据可按不同 target、tag、平台、地区或构建模式导出不同结果。
6. **支持长期扩展**：Source Provider、Data Exporter、Code Generator、Validator、Special Exporter 等能力可通过内置模块或插件扩展。

## 1.2 目标用户

| 用户角色 | 主要诉求 |
| --- | --- |
| 策划 / 内容设计师 | 在表格中维护配置，获得清晰错误定位，减少手写 JSON 或代码。 |
| 游戏客户端 / 服务器工程师 | 定义 Meta、索引、类型映射和导出目标，得到类型明确的运行时数据和访问代码。 |
| 构建 / 发布工程师 | 在 CI 中运行校验和导出，生成稳定产物、资源清单、国际化模板。 |
| QA | 基于明确规则设计测试用例，验证导出结果、错误提示和过滤行为。 |
| 工具开发者 | 在稳定产品规格上实现插件、导出器、校验器和代码生成器。 |

## 1.3 预期解决的问题

### 表结构与运行时代码脱节

传统流程中，表格字段、解析代码、运行时访问 API 可能各自维护，容易出现字段改名、类型变更后忘记同步的问题。ConfigForge 要求 Meta 描述逻辑结构，再由工具导出运行时数据和类型感知代码，减少重复 schema。

### 表格拆分与多人协作冲突

大型配置表往往需要按功能、活动、版本拆分。ConfigForge 的目标态支持一个逻辑表读取多个 `sources`，按声明顺序组合为最终逻辑表；同时通过 `override_sources` 对已有行或对象字段进行覆盖，便于版本分支和热修。当前实现状态以 `Design/README.md` 中的状态矩阵为准：已支持 CSV 多 source 追加 records，通用 override 合并仍未实现。

### 发布包按上下文裁剪数据

例如 shipping 包不应包含测试数据，某地区包不应包含其他地区配置。ConfigForge 支持 target tags，并通过 `export_when` 和 `row_export` 生成目标专属数据视图。过滤后的数据视图同时影响常规运行时数据和 special exporter 产物。

### 表内、表间一致性缺少自动检查

ConfigForge 支持 required/default、类型解析、枚举、索引唯一性、外键等检查，保证导出结果满足运行时约束。资源引用和更多集合级业务规则属于后续 validator/provider 扩展方向。

### 辅助产物依赖人工维护

国际化 POT、资源清单、运行时代码、数据 manifest 等可从同一份 DataSet / ExportView 生成，减少手工维护。

## 1.4 核心概念

| 概念 | 含义 |
| --- | --- |
| Project | 一个 ConfigForge 工程，包含 Meta、Source、Target、Plugin 等配置。 |
| Meta | 数据结构定义，包含 table、field、type、index、foreign key、validator 等。 |
| Source | 物理数据来源，例如 CSV 文件、XLSX sheet、Lark Sheet、Google Sheet。 |
| Source Provider | 负责读取 Source 并归一化为二维 Sheet 的加载器。 |
| ObjectTable | 对象表。导出一个常量对象，常用于全局配置。 |
| CollectionTable | 集合表。导出多行记录，常用于 item、level、quest 等配置。 |
| Field | 字段。由 Meta 声明名称、类型、是否必填、默认值、导出策略等。 |
| Index | 集合表查询入口，可 unique 或 non-unique，可声明为 primary index。 |
| Primary Index | 表的主索引。用于默认外键目标和 override identity，不是 primary key。 |
| DataSet | Source 读取、组合后的完整逻辑数据集合。 |
| ExportView | 针对某个 target tags 过滤后的导出数据视图。 |
| Target | 一个导出目标，定义 tags、runtime data 输出、代码生成任务、输出目录等。 |
| Special Exporter | 输出辅助产物的导出器，例如 POT、asset manifest。 |

## 1.5 当前范围

当前目标态设计应覆盖；各项已实现状态以 `Design/README.md` 的状态矩阵为准：

- TOML / XML Meta 配置和多文件 import。
- CSV、XLSX、本地和远程表格的 source provider 模型。
- ObjectTable、CollectionTable、字段类型、默认值、必填、索引、外键。
- 多 source split merge 和 override source 覆盖。
- 多态 fragment 的 variant source mode。
- 导出上下文 tag、记录/字段/fragment 过滤。
- 常规 runtime data export 和 special export。
- CLI validate/export/inspect/debug 等命令的用户级参数。
- 结构化诊断、确定性输出、可用于 CI 的报告格式。

## 1.6 暂不纳入当前范围

- 不支持 fragment join mode，即不支持多个 fragment source 按 ID 拼接为同一条记录。
- 不把“独立子表式明细布局”作为当前必须支持的 field layout；当前只要求支持复合值在同一张 source 表中的聚合书写或展开书写。
- 不把 Protobuf 作为当前必须实现的导出格式；未来如支持，需要字段级 `field_number`。
- 不支持不明确的 `custom` runtime format。扩展格式必须通过明确 exporter ID 和 format ID 注册。
- 不设计不可信插件沙箱；动态库插件默认视为受信任代码。
