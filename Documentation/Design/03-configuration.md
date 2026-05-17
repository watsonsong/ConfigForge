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

一个 target 可以声明 0 个、1 个或多个代码生成任务。代码生成任务放在 `[[targets.code]]` 中；每个任务有独立语言、输出目录和语言专属参数。这样同一个 target 可以同时输出 runtime data、Rust 服务端访问代码、C++ 客户端访问代码和 TypeScript 工具链访问代码。

```toml
[[targets]]
name = "client_runtime"
data_format = "json"
output = "Generated/Data"
tags = ["shipping", "global"]

[[targets.code]]
language = "rust"
output = "Generated/Code/Rust"
module_name = "game_config"
file_header = "// Generated for {{project.name}} at {{datetime}}\n// {{project.copyright}}"
keyword_conflict = "suffix_underscore"

[targets.code.naming]
type = "pascal_case"
enum_value = "pascal_case"
field = "snake_case"
function = "snake_case"

[targets.code.rust]
derive_serde = true

[[targets.code]]
language = "typescript"
output = "Generated/Code/TypeScript"
module_name = "gameConfig"
file_header = "// {{project.name}} - {{project.desc}}\n// Generated at {{datetime}}"
keyword_conflict = "suffix_underscore"

[targets.code.naming]
type = "pascal_case"
field = "camel_case"
function = "camel_case"

[targets.code.typescript]
readonly_fields = true
```

`targets.code` 字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `language` | 必填 | 代码生成语言，例如 `rust`、`cpp`、`typescript`。 |
| `output` | 必填 | 当前代码生成任务的输出目录。相对路径以 project root 为基准。 |
| `module_name` | language 默认值 | 模块名、包名、namespace 或生成文件前缀。具体语义由语言生成器决定。 |
| `namespace` | 未设置 | 可选命名空间；C++ 可映射为 namespace，Rust/TS 可作为模块组织提示。 |
| `file_header` | 生成器默认文件头 | 可选文件头模板。为空字符串表示不生成文件头。 |
| `keyword_conflict` | `suffix_underscore` | 生成标识符与目标语言关键字冲突时的处理策略。 |
| `naming` | 语言默认命名策略 | 当前语言各类符号的大小写组织方式。 |

语言特定配置必须放在当前 `[[targets.code]]` 任务下的语言子表中，采用 tagged enum 的语义解析，避免不同语言的配置字段互相污染：

| 子表 | 字段 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `[targets.code.rust]` | `derive_serde` | `false` | 为生成的 Rust enum/struct 派生 `serde::Serialize` / `serde::Deserialize`。 |
| `[targets.code.cpp]` | `namespace` | 继承 `targets.code.namespace` | C++ namespace 覆盖项。 |
| `[targets.code.typescript]` | `readonly_fields` | `false` | TypeScript interface 字段是否生成 `readonly`。 |

`file_header` 支持以下模板变量：

| 变量 | 含义 |
| --- | --- |
| `{{project.name}}` | Project Config 中的 `project.name`。 |
| `{{project.desc}}` | Project Config 中的 `project.desc`，未配置时为空。 |
| `{{project.copyright}}` | Project Config 中的 `project.copyright`，未配置时为空。 |
| `{{datetime}}` | 代码生成执行时刻，推荐 ISO-8601 或工具定义的稳定 datetime 字符串。 |

`keyword_conflict` 可选值：

| 值 | 说明 |
| --- | --- |
| `suffix_underscore` | 自动追加 `_`，例如 `type` -> `type_`。推荐默认值。 |
| `raw_identifier` | 使用目标语言原生转义方式，例如 Rust `r#type`；不支持该能力的语言应报错。 |
| `error` | 直接报错，要求修改 Meta 名称或显式配置命名覆盖。 |

`targets.code.naming` 是语言生成器的可扩展配置。第一阶段统一支持以下 key：

| Key | 适用符号 | 常用值 |
| --- | --- | --- |
| `type` | enum type、struct type、table row type、repository type | `pascal_case` |
| `enum_value` | enum 成员 | `pascal_case`、`screaming_snake_case` |
| `field` | struct/table 字段、facade 成员 | `snake_case`、`camel_case` |
| `function` | lookup function、loader function | `snake_case`、`camel_case`、`pascal_case` |
| `constant` | data file 常量等 | `screaming_snake_case`、`pascal_case` |

大小写策略可选值：

| 值 | 示例 |
| --- | --- |
| `preserve` | `item_id` -> `item_id` |
| `pascal_case` | `item_id` -> `ItemId` |
| `camel_case` | `item_id` -> `itemId` |
| `snake_case` | `item-id` -> `item_id` |
| `screaming_snake_case` | `item-id` -> `ITEM_ID` |

原则：

- Project Config 只声明“想要的命名策略”，不应写死生成器内部类型名。
- 新语言生成器可以扩展自己的 `targets.code.<language_specific>` 配置，但不应要求修改通用 target schema。
- 若配置中的命名策略导致同一作用域生成名冲突，必须在写文件前报错。
- 同一 target 下多个 code task 的输出路径不能互相覆盖，也不能与 runtime data/special exporter 输出冲突。

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
