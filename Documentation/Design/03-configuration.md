# 03. 配置文件设计

## 3.1 配置层次

ConfigForge 推荐使用以下配置层次：

```text
Project Config
  -> Meta Files
      -> Tables / Fields / Indexes / Validators
      -> Imports
  -> Target Config
      -> Runtime Data Export
      -> Code Generation
      -> Special Exporters
  -> Plugin Config
  -> CLI Overrides
```

可以把所有内容写在一个文件中，也可以按工程规模拆分为多个文件。大项目推荐拆分：

```text
ConfigForge.toml
Meta/
  common.toml
  items.toml
  quests.toml
Targets/
  client.toml
  server.toml
```

## 3.2 Project Config

Project Config 定义工程根、Meta 根文件、默认输出目录、插件、默认诊断格式等。

示例：

```toml
[project]
name = "game_config"
root = "."
copyright = "Copyright 2026 Example Studio. All rights reserved."

meta = [
  "Meta/common.toml",
  "Meta/items.toml",
  "Meta/quests.toml",
]

targets = [
  "Targets/client.toml",
  "Targets/server.toml",
]

default_output = "Generated"

[diagnostics]
format = "human"
fail_on_warning = false

[artifact_writer]
write_mode = "temp_commit"

[[plugins]]
id = "com.game.lark_sheet"
version = "1.2.0"
path = "Plugins/lark_sheet.dylib"
override = false
```

要求：

- `project.name` 必须稳定。
- `project.copyright` 可选；用于生成代码、schema、manifest 或其他文本产物的文件头。
- `meta` 可指向 TOML 或 XML。
- `targets` 可指向独立 target 文件，也可直接内联在 Project Config。
- 插件 path 不应写入 target config；target 只引用稳定 exporter/provider/generator ID。

Artifact Writer 配置：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `artifact_writer.write_mode` | `temp_commit` | 产物写入策略。`temp_commit` 表示先写临时文件再确定性提交；`direct` 表示直接写目标路径。 |

要求：

- `temp_commit` 是推荐默认值，适合 CI 和正式导出，失败时不应留下部分最终产物。
- `direct` 可用于本地调试或项目明确接受部分写入风险的场景。
- 无论使用哪种写入策略，输出路径冲突都必须在写入前发现。

## 3.3 Meta 文件与 Import

Meta 文件定义数据逻辑结构。TOML 示例：

```toml
imports = [
  "common/enums.toml",
  "common/structs.toml",
]

[[tables]]
name = "items"
kind = "collection"
desc = "所有可获得物品的基础配置。"
source = { provider = "csv", path = "Data/items.csv" }
primary_index = "by_id"

[[tables.fields]]
name = "id"
type = "int"
required = true
desc = "物品唯一 ID。"

[[tables.fields]]
name = "name"
type = "text"
required = true
desc = "物品展示名称。"

[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
primary = true
```

XML 示例：

```xml
<meta>
  <imports>
    <import path="common/enums.xml" />
  </imports>

  <table name="items" kind="collection" primaryIndex="by_id" desc="所有可获得物品的基础配置。">
    <source provider="csv" path="Data/items.csv" />
    <field name="id" type="int" required="true" desc="物品唯一 ID。" />
    <field name="name" type="text" required="true" desc="物品展示名称。" />
    <index name="by_id" unique="true" primary="true">
      <fieldRef path="id" />
    </index>
  </table>
</meta>
```

命名约定：

- MetaModel canonical 字段名使用 snake_case。
- TOML 使用 snake_case。
- XML 使用 lowerCamelCase。
- 诊断中应尽量同时保留 source spelling 和 canonical spelling。

描述约定：

- Meta 中各层级定义可声明 `desc`，用于说明业务含义。
- `desc` 会进入 canonical MetaModel，可被 inspect 命令、代码生成器和辅助文档生成器使用。
- 普通 TOML/XML 注释不会进入 MetaModel；需要生成代码注释时应使用 `desc`。

## 3.4 Source Declaration

Source declaration 描述 provider 和定位信息。

推荐 provider：

| Provider | 用途 | 必填字段 |
| --- | --- | --- |
| `csv` | 本地 CSV 文件 | `path` |
| `xlsx` | 本地 Excel 文件 | `path`，多 sheet 时需要 `sheet` |
| `lark_sheet` | Lark/飞书表格 | `uri` 或 provider-specific id，通常需要 `auth_ref` |
| `google_sheet` | Google Sheet | `uri` 或 sheet id，通常需要 `auth_ref` |

Source sheet 允许出现空行和空列。未映射到 field path 的行/列不会进入导出结果，可用于策划注释、公式中间结果或视觉分隔。

单 source：

```toml
source = { provider = "csv", path = "Data/items.csv" }
```

兼容 shorthand：

```toml
source = "Data/items.csv"
```

语义：根据文件后缀推断为 `{ provider = "csv", path = "Data/items.csv" }`。

多 source：

```toml
sources = [
  { provider = "csv", path = "Data/items_base.csv" },
  { provider = "csv", path = "Data/items_event.csv" },
]
```

Override sources：

```toml
override_sources = [
  { provider = "csv", path = "Data/items_hotfix.csv" },
  { provider = "csv", path = "Data/items_shipping.csv" },
]
```

覆盖顺序：列表越靠后越晚应用，晚应用的覆盖早应用的结果。

## 3.5 Target Config

Target Config 定义一次导出的目标平台、语言、runtime data format、tag、输出目录和特殊导出器。

```toml
[[targets]]
name = "client_lua_shipping"
language = "lua"
engine = "unity"
data_format = "lua_table"
output = "Generated/ClientLua"
tags = ["shipping", "global"]

[targets.data]
split = "by_table"
include_manifest = true

[[targets.special_exporters]]
kind = "gettext_pot"
output = "Generated/Localization/messages.pot"

[[targets.special_exporters]]
kind = "asset_manifest_csv"
output = "Generated/Build/assets.csv"
deduplicate = true
```

推荐 runtime data format：

| Format | 状态 | 说明 |
| --- | --- | --- |
| `json` | 第一阶段默认 | 易调试，适合 Web/TS 和通用场景。 |
| `lua_table` | 允许 | Lua 中代码和数据可能合一，作为特殊 runtime format。 |
| `yaml` | 可扩展 | 可读性高，但运行时解析成本需谨慎。 |
| `msgpack` | 可扩展 | 二进制 JSON-like 格式。 |
| `sqlite` | 可扩展 | 适合查询型运行时。 |
| `protobuf` | 未来设计 | 当前不要求实现；未来需要字段级 `field_number`。 |

不使用模糊的 `custom`。项目自定义格式必须注册明确的 exporter ID 和 format ID。

当前不引入通用配置 schema version 或自动兼容机制。表、字段、enum、struct 等定义依赖稳定字符串 `name` 作为长期身份；如果未来启用 Protobuf，则通过显式 `field_number` 和对应 schema evolution 策略处理二进制格式兼容。

## 3.6 Field Export Policy 配置

字段可控制是否进入运行时数据、是否参与过滤、是否参与特殊导出。

```toml
[[tables.fields]]
name = "test"
type = "bool"
default = false
tool_only = true
filter_field = true
export = false
```

默认值：

| 属性 | 默认值 | 说明 |
| --- | --- | --- |
| `required` | `false` | 是否必须有值；若有 `default` 可由 default 补齐。 |
| `default` | 未设置 | 源数据缺失时使用的默认值。 |
| `export` | `true` | 是否进入常规 runtime data。 |
| `tool_only` | `false` | 仅工具使用；等价于默认不进入 runtime data。 |
| `filter_field` | 普通字段默认可用，tool-only 字段需显式声明 | 是否允许被 export filter 引用。 |
| `validator_only` | `false` | 仅参与校验，默认不参与 runtime data。 |
| `special_exports` | 未设置 | 控制是否可被特殊导出器使用。 |
