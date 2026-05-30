# 03. 配置文件设计

## 3.1 配置层次

ConfigForge 推荐使用以下配置层次：

```text
Project Config
  -> Imports
  -> Meta Files
      -> Tables / Fields / Indexes / Validators
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
meta/
  characters.toml
  items.toml
  misc.toml
data/
  classes.csv
  class_levels.csv
  materials.csv
  weapons.csv
  armors.csv
  game_rules.csv
```

拆分时使用 Project Config 顶层 `imports` 统一导入其他工程配置片段。被导入的文件与根配置使用同一套格式：可以直接声明 `[[enums]]`、`[[structs]]`、`[[tables]]`、`[[targets]]`，也可以继续声明 `imports`。不再需要单独的 `meta = [...]` 列表；所有导入片段中的 Meta 定义和 target 定义最终合并为一次构建使用的工程配置。标准参考游戏 fixture 采用 `ConfigForge.toml` 作为总控入口，按 `meta/characters.toml`、`meta/items.toml`、`meta/misc.toml` 拆分角色、道具、杂项定义，并在总控文件中声明 client/server 构建 target。

## 3.2 Project Config

Project Config 定义工程元信息、Meta 定义和可构建的 targets。CLI 只接收 Project Config 路径和 target 名称；Data 输入、runtime data 输出、代码生成语言和代码输出目录都由 target 配置决定。

示例：

```toml
imports = [
  "Meta/common.toml",
  "Meta/items.toml",
  "Meta/quests.toml",
  "Targets/client.toml",
]

[project]
name = "game_config"
desc = "Game config export project."
copyright = "Copyright 2026 Example Studio. All rights reserved."

[[targets]]
name = "client"

[[targets.data]]
input = "Data"
output = "Generated/Data"
format = "json"

[[targets.code]]
language = "cpp"
output = "Generated/Cpp"
namespace = "game::config"

[[targets.code]]
language = "typescript"
output = "Generated/TypeScript"
```

要求：

- `project.name` 必须稳定。
- `project.copyright` 可选；用于生成代码、schema、manifest 或其他文本产物的文件头。
- `imports` 属于 Project Config 顶层，用于导入其他 Project Config 片段，可递归导入；重复导入同一文件只加载一次。
- Meta 定义直接使用 `[[enums]]`、`[[structs]]`、`[[tables]]` 写在任意 Project Config 片段中；构建前会合并所有导入片段中的 Meta 定义。
- `targets` 可以直接内联在根 Project Config 中，也可以放在被 `imports` 引入的配置片段中；每个 target 描述一次完整构建。
- 每个配置片段中的相对路径都相对该片段自身所在目录解析。
- 推荐由根 `ConfigForge.toml` 充当总控文件：集中声明工程元信息、meta fragment 引用和常用构建 target；领域 meta 文件只描述领域 schema，避免一个超大的 `meta.toml` 同时承担所有职责。

要求：

- CLI 不提供 `--meta`、`--data`、`--out`、`validate/export` 子命令等重复入口。
- 构建时根据 target 自动判断是否需要 Data：code-only target 只加载 Meta；声明 `[[targets.data]]` 的 target 才加载 Data。

## 3.3 Meta 文件

Meta 片段只定义数据逻辑结构，可以被 Project Config 顶层 `imports` 直接导入。多个 Meta 片段中的 `enums`、`structs`、`tables` 会与 target 片段一起按统一 Project Config 格式合并。

TOML 示例：

```toml
[[tables]]
name = "items"
kind = "collection"
desc = "所有可获得物品的基础配置。"
source = { provider = "csv", path = "items.csv" }
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
| `xlsx` | Planned / Not implemented，本地 Excel 文件 | `path`，多 sheet 时需要 `sheet` |
| `lark_sheet` | Planned / Not implemented，Lark/飞书表格 | `uri` 或 provider-specific id，通常需要 `auth_ref` |
| `google_sheet` | Planned / Not implemented，Google Sheet | `uri` 或 sheet id，通常需要 `auth_ref` |

当前实现状态以 `Documentation/Design/README.md` 的状态矩阵为准：CSV provider 已稳定支持；同一逻辑表的多 CSV source 追加 records 已形成 MVP，主要服务 polymorphic variant source mode；`override_sources` 与非 CSV provider 仍属于目标态设计，当前会显式拒绝或不提供入口。

Source sheet 允许出现空行和空列。未映射到 field path 的行/列不会进入导出结果，可用于策划注释、公式中间结果或视觉分隔。

单 source：

```toml
source = { provider = "csv", path = "items.csv" }
```

兼容 shorthand：

```toml
source = "items.csv"
```

语义：根据文件后缀推断为 `{ provider = "csv", path = "items.csv" }`。

多 source（Implemented MVP，限同一逻辑表多个 CSV source 按声明顺序 append records，主要服务 polymorphic variant source mode）：

```toml
sources = [
  { provider = "csv", path = "Data/items_base.csv" },
  { provider = "csv", path = "Data/items_event.csv" },
]
```

Override sources（Planned / Not implemented）：

```toml
override_sources = [
  { provider = "csv", path = "Data/items_hotfix.csv" },
  { provider = "csv", path = "Data/items_shipping.csv" },
]
```

覆盖顺序：列表越靠后越晚应用，晚应用的覆盖早应用的结果。

## 3.5 Target Config

Target Config 定义一次可构建的导出目标。一个 target 共享同一组 `TargetTags` 和同一套 Meta-level filtering 规则，并可以挂载 0 个、1 个或多个 runtime data 导出任务，以及 0 个、1 个或多个代码生成任务。具体的 DataSet-dependent ExportView 必须按 `(target tags, data input/DataSet)` 派生；只有 `input` 完全相同的 data task 才能复用同一次 source load、normalize、validation 和 concrete ExportView。

路径基准规则：

- `ConfigForge.toml` 是工程入口文件；工程根目录（project root）默认是入口文件所在目录。
- 写在入口 `ConfigForge.toml` 中的 `targets.data.input`、`targets.data.output`、`targets.code.output` 相对路径都以 project root 为基准。
- 写在被 `imports` 引入的 Project Config 片段中的相对路径，以该片段文件所在目录为基准，并在加载阶段规范化为绝对路径后再合并；这样被复用的配置片段可以携带自己的相对资源目录。
- `targets.data.input` 是 source data 根目录。Meta 中 table `source`、`sources[].path`、`override_sources[].path` 均相对该 data root 查找，而不是相对 Meta 文件或代码输出目录。
- `targets.data.output` 和 `targets.code.output` 是产物输出根目录；二者彼此独立，所有 artifact 在写入前必须做路径冲突检查。

```toml
[[targets]]
name = "client_runtime"
tags = ["client", "shipping"]

[[targets.data]]
name = "json"
input = "Data"
output = "Generated/Data/Json"
format = "json"

[[targets.data]]
name = "messagepack"
input = "Data"
output = "Generated/Data/MessagePack"
format = "messagepack"

[[targets.code]]
language = "rust"
output = "Generated/Code/Rust"
module_name = "game_config"

[[targets.code]]
language = "typescript"
output = "Generated/Code/TypeScript"
module_name = "gameConfig"
```

Runtime data 导出任务使用 `[[targets.data]]` 数组形式；每个任务有独立 `name`、`input`、`output` 和 `format`。这样同一个 target 可以同时输出 JSON、MessagePack 等多套 runtime data。Project Config 使用 `[[targets.data]]` 数组表表达 runtime data 导出任务。

一个 target 可以声明 0 个、1 个或多个代码生成任务。代码生成任务放在 `[[targets.code]]` 中；每个任务有独立语言、输出目录和语言专属参数。这样同一个 target 可以同时输出 runtime data、Rust 服务端访问代码、C++ 客户端访问代码和 TypeScript 工具链访问代码。

生成代码是格式无关的：code task 只依赖 Meta 和 target tags，不绑定某个 data task，也不能配置 `data = "..."`。生成代码中的 table data file 常量使用逻辑表名（例如 `items`），实际打开 `items.json`、`items.msgpack` 或 `items.bin` 由接入方选择的 RuntimeDataLoader / RuntimeDataReader 实现负责。

`tags` 是 target 的核心上下文。每个 tag 代表一个导出特性，例如：

| Tag | 推荐语义 |
| --- | --- |
| `client` | 客户端目标。 |
| `server` | 服务端目标。 |
| `debug` | 开发、调试或内部验证目标。 |
| `shipping` | 正式发布目标。 |

这些名称只是推荐约定，机制本身允许项目继续扩展平台、地区、渠道或业务线 tag，例如 `ios`、`android`、`cn`、`global`、`playtest`。表、字段和 collection 行根据这些 tags 生成 target 专属 ExportView。

```toml
[[targets]]
name = "client_debug"
tags = ["client", "debug"]

[[targets.data]]
name = "json"
input = "Data"
output = "Generated/Data/ClientDebug"
format = "json"

[[targets]]
name = "client_shipping"
tags = ["client", "shipping"]

[[targets.data]]
name = "messagepack"
input = "Data"
output = "Generated/Data/ClientShipping"
format = "messagepack"

[[targets]]
name = "server_shipping"
tags = ["server", "shipping"]

[[targets.data]]
name = "json"
input = "Data"
output = "Generated/Data/ServerShipping"
format = "json"

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

[targets.code.formatting]
indent_style = "spaces"
indent_width = 4
trailing_newline = true
trailing_blank_line = false

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

`targets.data` 字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 单 data task 可省略，规范化为 `default` | 当前 target 内稳定唯一的 data task 名称；多 data task 时必填。 |
| `input` | 必填 | Source data 根目录。相对路径以 project root 或所在 import 片段目录为基准；Meta source path 以该目录为搜索基础。只有声明 `targets.data` 时才读取 source data。 |
| `output` | 必填 | Runtime data 输出目录。相对路径以 project root 或所在 import 片段目录为基准。 |
| `format` | `json` | Runtime data 格式。当前实现支持 `json` 和 `messagepack`。 |

Data task 继承所在 target 的 `tags`；当前没有 `[[targets.data]].tags` 字段。

data task `name` 规范：

- 如果用户显式写了 `name`，使用用户名称；如果单 data task 省略 `name`，规范化名称固定为 `default`。
- 多 data task 时每个 task 必须显式写非空 `name`。
- `name` 大小写敏感；只允许 ASCII 字母、数字、`_`、`-`，且必须以字母或数字开头和结尾。
- 同一 target 下多个 data task 的规范化 `name` 必须唯一；保留名 `default` 允许作为显式名称，但仍遵守唯一性规则。

多个 data task 可以共享同一个 `input`，实现应复用同一次加载、归一化和校验结果后写出不同格式。不同 `input` 表示同一 target tags 下的不同数据集，必须分别加载、归一化、校验并派生 DataSet-dependent ExportView。不同 data task 的最终输出文件路径不能相互覆盖，也不能与 code task 或未来 special exporter 的输出冲突；冲突检查基于规范化绝对 artifact path。

`RuntimeDataFile.path` 表示相对当前 data artifact root / runtime loader root 的真实资源路径，例如 `items.json`。生成代码不再透传真实文件路径，而是使用逻辑表名（例如 `items`）；具体 runtime loader 决定如何把逻辑名映射到文件系统、包内资源或引擎资源中的具体格式文件。

Planned / Not implemented：`engine`、`special_exporters`、POT、Asset Manifest、target-level `language` / `data_format` / `output` 属于未来扩展，当前 Project Config 不应使用这些字段。

`targets.code` 字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `language` | 必填 | 代码生成语言，例如 `rust`、`cpp`、`typescript`。 |
| `output` | 必填 | 当前代码生成任务的输出目录。相对路径以 project root 或所在 import 片段目录为基准。 |
| `file_name` | language 默认值 | 可选生成代码文件名。Rust / TypeScript 会补齐默认扩展；C++ 以该名称生成头文件，并用同一 stem 生成 `.cpp`。只能是文件名，不能包含路径分隔符。 |
| `module_name` | language 默认值 | 模块名、包名、namespace 或生成文件前缀。具体语义由语言生成器决定。 |
| `data` | 不支持 | 禁止设置。生成代码格式无关，只依赖 Meta；格式选择由运行时传入的 loader / reader 实现决定。 |
| `namespace` | 未设置 | 可选命名空间；C++ 可映射为 namespace，Rust/TS 可作为模块组织提示。 |
| `file_header` | 生成器默认文件头 | 可选文件头模板。为空字符串表示不生成文件头。 |
| `keyword_conflict` | `suffix_underscore` | 生成标识符与目标语言关键字冲突时的处理策略。 |
| `naming` | 语言默认命名策略 | 当前语言各类符号的大小写组织方式。 |
| `formatting` | 语言默认格式策略 | 生成源码的缩进与文件尾换行/空行策略。 |

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

`targets.code.formatting` 是语言生成器的通用格式配置。第一阶段统一支持以下 key：

| Key | 默认值 | 说明 |
| --- | --- | --- |
| `indent_style` | `"spaces"` | 缩进 token，可选 `"spaces"` 或 `"tab"`。 |
| `indent_width` | 语言默认 | `indent_style = "spaces"` 时每一级缩进的空格数；tab 模式下保留该配置但每级缩进输出一个 tab。 |
| `trailing_newline` | `true` | 文件结束是否至少保留一个换行。 |
| `trailing_blank_line` | `false` | 文件结束是否额外保留一个空行；启用时内容以两个换行结束，并优先于 `trailing_newline`。 |

语言默认缩进：Rust / C++ 使用 4 空格，TypeScript 使用 2 空格。Project Config 只需要声明与语言默认值不同的字段；未声明的 formatting 字段由对应语言生成器默认值补齐。

原则：

- Project Config 只声明“想要的命名策略”，不应写死生成器内部类型名。
- 新语言生成器可以扩展自己的 `targets.code.<language_specific>` 配置，但不应要求修改通用 target schema。
- 若配置中的命名策略导致同一作用域生成名冲突，必须在写文件前报错。
- 同一 target 下多个 code task 的输出路径不能互相覆盖，也不能与 runtime data/special exporter 输出冲突。

推荐 runtime data format：

| Format | 状态 | 说明 |
| --- | --- | --- |
| `json` | 第一阶段默认 | 易调试，适合 Web/TS 和通用场景。 |
| `messagepack` | 第一阶段支持 | 紧凑二进制格式，适合包体和加载性能更敏感的 runtime。 |
| `lua_table` | 允许 | Lua 中代码和数据可能合一，作为特殊 runtime format。 |
| `yaml` | 可扩展 | 可读性高，但运行时解析成本需谨慎。 |
| `sqlite` | 可扩展 | 适合查询型运行时。 |
| `protobuf` | 未来设计 | 当前不要求实现；未来需要字段级 `field_number`。 |

不使用模糊的 `custom`。项目自定义格式必须注册明确的 exporter ID 和 format ID。

当前不引入通用配置 schema version 或自动兼容机制。表、字段、enum、struct 等定义依赖稳定字符串 `name` 作为长期身份；如果未来启用 Protobuf，则通过显式 `field_number` 和对应 schema evolution 策略处理二进制格式兼容。

## 3.6 Target Tags 与导出条件配置

表、字段和索引都可以声明 `export_when`，用于按 target tags 生成导出视图。`export_when` 是结构化条件，不使用字符串表达式。

```toml
export_when = { any = ["client"], all = ["shipping"], none = ["debug"] }
```

匹配语义：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `any` | `[]` | 若非空，target tags 至少命中其中一个。 |
| `all` | `[]` | 若非空，target tags 必须包含全部 tag。 |
| `none` | `[]` | 若非空，target tags 不能包含其中任何 tag。 |

空条件表示不限制 target。`export = false` 是强制不导出，优先级高于 `export_when`。不再设计 `tool_only`、`validator_only`、`filter_field` 作为导出控制字段；可以被 `export = false`、`export_when` 和 `row_export` 完整替代。

表级示例：

```toml
[[tables]]
name = "client_ui"
source = "client_ui.csv"
export_when = { any = ["client"] }

[[tables]]
name = "server_drop"
source = "server_drop.csv"
export_when = { any = ["server"] }
```

字段级示例：

```toml
[[tables]]
name = "skill"
source = "skill.csv"

[[tables.fields]]
name = "id"
type = "int"
unique = true

[[tables.fields]]
name = "icon"
type = "asset"
export_when = { any = ["client"] }

[[tables.fields]]
name = "damage_formula"
type = "string"
export_when = { any = ["server"] }

[[tables.fields]]
name = "debug_comment"
type = "string"
export_when = { any = ["debug"] }
```

索引规则：普通 index 只有在 index 自身满足 `export_when` 且所有 index fields 都可导出时才进入 ExportView；primary index 若在当前 target 下不可用必须报错，不能静默删除。

## 3.7 Row Export Policy 配置

collection 表可以通过 `row_export` 声明由哪些数据列控制行级导出。推荐用于 `debug` / `shipping` 行裁剪，也可用于平台、地区或渠道行裁剪。

```toml
[[tables]]
name = "item"
source = "item.csv"

[tables.row_export]
required_tags_field = "export_tags"
excluded_tags_field = "exclude_tags"
match = "any"

[[tables.fields]]
name = "id"
type = "int"
unique = true

[[tables.fields]]
name = "name"
type = "string"

[[tables.fields]]
name = "export_tags"
type = "string"
export = false

[[tables.fields]]
name = "exclude_tags"
type = "string"
export = false
```

`row_export` 字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `required_tags_field` | 未设置 | 行上声明“需要哪些 target tag 才导出”的字段；空值表示不限制。 |
| `excluded_tags_field` | 未设置 | 行上声明“target 包含哪些 tag 时不导出”的字段；空值表示不排除。 |
| `match` | `any` | `required_tags_field` 中多个 tag 的匹配方式，`any` 表示命中任意一个，`all` 表示必须全部命中。 |

数据列中的 tag 推荐用逗号分隔：

```csv
id,name,export_tags,exclude_tags
1,Potion,,
2,Debug Sword,debug,
3,Shipping Gift,shipping,
4,Internal Test Item,,shipping
5,Server Reward,server,
6,Client Preview Item,client,
```

在 `tags = ["client", "debug"]` 的 target 中导出默认行、debug 行和 client 行；在 `tags = ["client", "shipping"]` 的 target 中导出默认行、shipping 行和 client 行，并排除 `exclude_tags = "shipping"` 的行。

## 3.8 Field 基础配置

字段可控制类型、缺省值、是否强制不导出以及特殊导出器读取策略。

```toml
[[tables.fields]]
name = "test"
type = "bool"
default = false
export = false
```

默认值：

| 属性 | 默认值 | 说明 |
| --- | --- | --- |
| `required` | `false` | 是否必须有值；若有 `default` 可由 default 补齐。 |
| `default` | 未设置 | 源数据缺失时使用的默认值。 |
| `export` | `true` | 是否允许进入 runtime ExportView。`false` 表示强制不导出。 |
| `export_when` | 未设置 | 按 target tags 判断字段是否进入当前 ExportView。 |
| `special_exports` | 未设置 | 控制是否可被特殊导出器使用。 |
