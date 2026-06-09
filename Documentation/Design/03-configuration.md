# 03. 配置文件设计

## 3.1 配置层次

ConfigForge 推荐使用以下配置层次：

```text
Project Config
  -> Imports
  -> Meta Files
      -> Tables / Fields / Indexes / Validators
  -> Target Config
      -> Data Export
      -> Code Generation
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

拆分时使用 Project Config 顶层 `imports` 统一导入其他工程配置片段。被导入的文件与根配置使用同一套格式：可以直接声明 `[[enums]]`、`[[structs]]`、`[[tables]]`、`[[asset_kinds]]`、`[[targets]]`，也可以继续声明 `imports`。不再需要单独的 `meta = [...]` 列表；所有导入片段中的 Meta 定义和 target 定义最终合并为一次构建使用的工程配置。标准参考游戏 fixture 采用 `ConfigForge.toml` 作为总控入口，按 `meta/common.toml`、`meta/characters.toml`、`meta/items.toml`、`meta/misc.toml` 拆分公共规则、角色、道具、杂项定义，并在总控文件中声明 client/server 构建 target。

## 3.2 Project Config

Project Config 定义工程元信息、Meta 定义和可构建的 targets。CLI 只接收 Project Config 路径和 target 名称；Data 输入、runtime data 输出、代码生成语言和代码输出目录都由 target 配置决定。

示例：

```toml
imports = [
  "meta/common.toml",
  "meta/items.toml",
  "meta/quests.toml",
  "targets/client.toml",
]

[project]
name = "game_config"
input = "Data"
desc = "Game config export project."
copyright = "Copyright 2026 Example Studio. All rights reserved."

[[targets]]
name = "client"

[[targets.exports]]
output = "Generated/Data"
type = "json"

[[targets.exports]]
type = "cpp"
output = "Generated/Cpp"
namespace = "game::config"

[[targets.exports]]
type = "typescript"
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
- 构建时根据 export task 对应 generator 的 requirements 自动判断是否需要 Data：只包含 MetaOnly generator 的 target 只加载 Meta；包含 MetaAndData generator 的 target 使用 `project.input` 加载 Data。

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
- `desc` 会进入 canonical MetaModel，可被代码生成器和辅助文档生成器使用。
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

当前实现状态以 `Documentation/Design/README.md` 的状态矩阵为准：CSV provider 已稳定支持；同一逻辑表的多 CSV source 按声明顺序追加 records 已稳定，主要服务 polymorphic variant source mode；`override_sources` 已支持 CollectionTable CSV 整行覆盖，并允许通过 source-level `export_when` 绑定到 effective target tags，用于赛季、热修或灰度包的临时覆盖。非 CSV provider、ObjectTable override 和 base source `export_when` 仍会显式拒绝或不提供入口。

CollectionTable CSV source 使用 source-level 行布局参数决定哪一行是机器可读 header、哪一行开始进入数据。`header_row` 和 `data_start_row` 都是 **1-based** 行号，默认值为 `header_row = 1`、`data_start_row = 3`；因此第 2 行默认保留给策划标题、字段说明或其它可读提示，不进入导出数据。`data_start_row` 必须大于 `header_row`。ObjectTable 是按行组织的 `key,value` 常量表，因此标题说明默认使用额外列而不是额外行：可写成 `key,title,value`，其中 `title` 列仅供策划阅读，实际值仍来自 `value` 列。Source sheet 允许出现空行和空列。未映射到 field path 的行/列不会进入导出结果，可用于策划注释、公式中间结果或视觉分隔。

单 source：

```toml
source = { provider = "csv", path = "items.csv" }
```

带自定义起始行的单 source：

```toml
source = { provider = "csv", path = "items.csv", header_row = 1, data_start_row = 3 }
```

兼容 shorthand：

```toml
source = "items.csv"
```

语义：根据文件后缀推断为 `{ provider = "csv", path = "items.csv", header_row = 1, data_start_row = 3 }`。

多 source（Stable，限同一逻辑表多个 CSV source 按声明顺序 append records，主要服务 polymorphic variant source mode）：

```toml
sources = [
  { provider = "csv", path = "items_base.csv", header_row = 1, data_start_row = 3 },
  { provider = "csv", path = "items_event.csv", header_row = 1, data_start_row = 3 },
]
```

Override sources（Implemented MVP，当前限 CollectionTable CSV 整行覆盖）：

```toml
override_sources = [
  { provider = "csv", path = "items_hotfix.csv", header_row = 1, data_start_row = 3 },
  { provider = "csv", path = "items_shipping.csv", header_row = 1, data_start_row = 3, export_when = { all = ["shipping"] } },
]
```

覆盖顺序：列表越靠后越晚应用，晚应用的覆盖早应用的结果。

`override_sources[].export_when` 与 table / field 的 `export_when` 使用同一套 `{ any, all, none }` 结构，匹配当前构建的 **effective target tags**。不声明时表示对所有 target 生效。被 `export_when` 过滤掉的 override source 不读取、不参与覆盖、不产生 required/default/type/index 等校验诊断。

示例：S1 赛季临时把 Oak 材料售价从 3 调整为 5。基础材料仍在 `materials.csv` 中维护；赛季临时表 `materials_s1.csv` 与 `materials.csv` 使用相同 header，但只包含待覆盖的一行：

```csv
id,name,price,max_stack
ID,名称,价格,最大堆叠
2002,Oak Wood,5,999
```

`items.toml` 中的 `items` 表可以声明：

```toml
[[tables]]
name = "items"
kind = "collection"
polymorphic = true
primary_index = "by_id"
sources = [
  { provider = "csv", path = "materials.csv", fragments = ["stack"] },
  { provider = "csv", path = "weapons.csv", fragments = ["attributes", "skill"] },
  { provider = "csv", path = "armors.csv", fragments = ["attributes"] },
]
override_sources = [
  { provider = "csv", path = "materials_s1.csv", fragments = ["stack"], export_when = { all = ["s1"] } },
]
```

当 effective target tags 包含 `s1` 时，`materials_s1.csv` 中 `id = 2002` 的完整 row 覆盖 base `materials.csv` 中相同 primary index 的 row；未包含 `s1` 时该 override source 完全不生效，Oak 仍使用 base 售价 3。

## 3.5 Target Config

Target Config 定义一次可构建的导出目标。一个 target 共享同一组 `TargetTags` 和同一套 Meta-level filtering 规则，并挂载一组 export task。每个 export task 通过 `type` 选择一个 artifact generator，声明输出目录和专属参数；generator 自身声明 input requirements（例如是否需要 Data）。Build 根据 requirements 拆分 MetaOnly / MetaAndData 调度分支，复用 Meta link、DataLoadPlan、normalize 和 validation 结果，再运行具体 generator。

### 3.5.1 统一 export task / generator 模型

Lua table 导出同时生成 Lua 代码和内嵌数据；gettext POT 依赖 Meta + Data 但不是 runtime data；format-agnostic codegen backend 只依赖 Meta；JSON/MessagePack/bin 依赖 Meta + Data；schema docs 只读 Meta 但输出文档 artifact。因此 target 下所有产物统一使用 `[[targets.exports]]` 建模，Build 层把它们都视为 artifact generator task，而不是用 “code / data / doc” 拆成多套用户配置：

```toml
[project]
input = "Data"

[[targets]]
name = "client"
tags = ["client", "shipping"]

[[targets.exports]]
name = "json"
type = "json"
output = "Generated/Data/Json"

[[targets.exports]]
name = "lua"
type = "lua_table"
output = "Generated/Lua"
module_name = "game_config"
lazy_require = true

[targets.exports.i18n]
mode = "global_function"
function = "globalTranslate"
returns = "optional"

[[targets.exports]]
name = "cpp"
type = "cpp"
output = "Generated/Cpp"
namespace = "game::config"

[[targets.exports]]
name = "pot"
type = "gettext_pot"
output = "Generated/I18n"
domain = "game_config"
```

统一 task 字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 单 export task 可省略，规范化为 `default` | 当前 target 内稳定唯一的 task 名称；用于诊断、trace、artifact manifest 和未来 task 间依赖引用。 |
| `type` | 必填 | generator ID，例如 `json`、`messagepack`、`bin`、`gettext_pot`、`lua_table`、`schema_doc`、`rust`、`cpp`、`typescript`。不再用 `format` / `language` 两套字段区分。 |
| `output` | 必填 | 该 generator 的产物输出根目录。所有 artifact 在写入前做全局规范化路径冲突检查。 |
| generator 专属字段 | 未设置 | 直接摊平在当前 `[[targets.exports]]` 下；通用子表如 `i18n`、`naming`、`formatting` 可被适用 generator facet 复用。 |

Generator 注册表需要声明每个 `type` 的 input requirements。`MetaOnly` / `MetaAndData` 是从 requirements 派生出的调度属性，不是 generator 的继承体系或领域身份：

| 派生 dependency | 例子 | 说明 |
| --- | --- | --- |
| `MetaOnly` | `rust`、`cpp`、`typescript`、`schema_doc` | requirements 不包含 Data；只需要 linked Meta、effective target tags、project / task options 等上下文；不加载 Data。 |
| `MetaAndData` | `json`、`messagepack`、`bin`、`gettext_pot`、`lua_table` | requirements 包含 Data；需要 DataLoadPlan、normalized DataSet、target validation 和 target-aware ExportView。 |

这样 Lua 不需要被强行归入 “data exporter” 或 “code generator” 的继承分支：它是一个 `type = "lua_table"`、requirements 包含 Meta + Data、同时具备 code artifact facet 与 aggregate artifact facet 的 generator。Rust / C++ / TypeScript 是 requirements 只含 Meta 的 code artifact generator；`schema_doc` 是 requirements 只含 Meta 的 doc artifact generator。未来 schema manifest、asset manifest、doc generator、engine-specific bundle、binary schema 等都可以按同一套 task 模型接入，并各自组合 code / data / doc / report / manifest 等能力。

### 3.5.2 依赖分支编排原则

统一 generator task 后，Build 不再写死 “先 data 后 code” 两段流程，而是按 generator requirements 建立 target-local execution view summary，并用阶段式 pipeline 执行两类分支：`SourceRead` 可与 `LinkMeta` 并行，`LinkMeta` 完成后立即启动不需要 Data 的 MetaOnly generator，因此 mixed target 中 format-agnostic codegen / schema docs 不再等待 source read / normalize / validation 完成。阶段式 tracing 已通过 `BuildTrace` / `--trace-tasks` 落地；当前不维护 node-level DAG key，也不引入完整通用 DAG 调度器。

当前 target-local pipeline 形态：

```text
ProjectConfig
  -> RawMeta
  -> LinkedMeta(target tags)
      -> MetaOnly generator tasks
      -> DataLoadPlan(project input, target tags)
          -> DataSet(project input, target tags)
          -> NormalizedDataSet(project input, target tags)
          -> Validation(project input, target tags)
          -> MetaAndData generator tasks sharing project.input
```

若未来重新引入跨 target cache 或 node-level scheduler，cache key 必须包含 dependency 语义。涉及 target tags 的 key 应使用已经合并 CLI extra tags 后的 effective target tags，避免把不同 CLI tag 构建误判为同一结果：

- `LinkedMeta` key：`(project config, effective target tags)`。
- `DataLoadPlan` / `DataSet` / `Validation` key：`(project.input, effective target tags)`，不能只按 `project.input` 缓存，因为 `override_sources[].export_when` 会受 CLI extra tags 影响。
- `ExporterTask` key：`(target name, task name)`。
- Artifact path 冲突检查是 shared sink / artifact writer 的全局步骤，不属于单个 exporter 私有逻辑。

当前分支 pipeline 已获得主要收益：只依赖 Meta 的 codegen 可与 data load 并行；同一 target 下的 JSON/MessagePack/bin/Lua/POT 共享同一次 DataLoadPlan、加载、归一化和校验；artifact manifest 仍可使用 export task name 作为稳定标识。多 target 批量构建、增量缓存、远程执行若未来需要，再重新评估是否引入 node-level DAG；当前不保留会造成“双轨复杂度”的 ready queue 或 DAG skeleton。

路径基准规则：

- `ConfigForge.toml` 是工程入口文件；工程根目录（project root）默认是入口文件所在目录。
- 写在入口 `ConfigForge.toml` 中的 `project.input` 和 `targets.exports.output` 相对路径都以 project root 为基准。
- 写在被 `imports` 引入的 Project Config 片段中的相对路径，以该片段文件所在目录为基准，并在加载阶段规范化为绝对路径后再合并；这样被复用的配置片段可以携带自己的相对资源目录。
- `project.input` 是 source data 根目录。Meta 中 table `source`、`sources[].path`、`override_sources[].path` 均相对该 data root 查找，而不是相对 Meta 文件、target 或代码输出目录。只依赖 Meta 的 project 可以省略 `project.input`；只要任一 target 包含 MetaAndData exporter，`project.input` 必须存在。
- `targets.exports.output` 是产物输出根目录；所有 artifact 在写入前必须做路径冲突检查。

```toml
[project]
input = "Data"

[[targets]]
name = "client_runtime"
tags = ["client", "shipping"]

[[targets.exports]]
name = "json"
output = "Generated/Data/Json"
type = "json"

[[targets.exports]]
name = "messagepack"
output = "Generated/Data/MessagePack"
type = "messagepack"

[[targets.exports]]
type = "rust"
output = "Generated/Code/Rust"
module_name = "game_config"

[[targets.exports]]
type = "typescript"
output = "Generated/Code/TypeScript"
module_name = "gameConfig"
```

一个 target 可以声明 0 个、1 个或多个 export task。MetaAndData exporter（如 JSON、MessagePack、bin、gettext POT、Lua table）统一使用 `project.input`；MetaOnly exporter（如 Rust、C++、TypeScript）不读取 Data。生成代码是格式无关的：MetaOnly codegen 不绑定某个数据 exporter，也不能配置 `data = "..."`。生成代码中的 table data file 常量使用逻辑表名（例如 `items`），实际打开 `items.json`、`items.msgpack` 或 `items.bin` 由接入方选择的 RuntimeDataLoader / RuntimeDataReader 实现负责。

`tags` 是 target 的核心上下文。每个 tag 代表一个导出特性，例如：

| Tag | 推荐语义 |
| --- | --- |
| `client` | 客户端目标。 |
| `server` | 服务端目标。 |
| `debug` | 开发、调试或内部验证目标。 |
| `shipping` | 正式发布目标。 |

这些名称只是推荐约定，机制本身允许项目继续扩展平台、地区、渠道、赛季或业务线 tag，例如 `ios`、`android`、`cn`、`global`、`playtest`、`s1`。表、字段、override source 和 collection 行根据这些 tags 生成 target 专属数据视图。

构建时实际使用的是 **effective target tags**：

```text
effective_target_tags = target.tags ∪ CLI extra tags
```

CLI extra tags 用于一次性构建上下文，例如 `--tag s1` 生成 S1 赛季包，而无需新增一个只多了 `s1` 的 target。重复 tag 去重；顺序只用于稳定诊断和 manifest，语义上按集合匹配。CLI extra tags 只能追加，不能移除 target 已声明的 tag。

示例：

```toml
[[targets]]
name = "client_shipping"
tags = ["client", "shipping"]
```

执行 `config-forge --target client_shipping --tag s1` 时，effective target tags 为 `{ client, shipping, s1 }`。因此 `export_when = { all = ["s1"] }` 的 `override_sources` 会生效，`export_when = { none = ["s1"] }` 的表、字段或行级策略则不会生效。

```toml
[project]
input = "Data"

[[targets]]
name = "client_debug"
tags = ["client", "debug"]

[[targets.exports]]
name = "json"
output = "Generated/Data/ClientDebug"
type = "json"

[[targets]]
name = "client_shipping"
tags = ["client", "shipping"]

[[targets.exports]]
name = "messagepack"
output = "Generated/Data/ClientShipping"
type = "messagepack"

[[targets]]
name = "server_shipping"
tags = ["server", "shipping"]

[[targets.exports]]
name = "json"
output = "Generated/Data/ServerShipping"
type = "json"

[[targets.exports]]
type = "rust"
output = "Generated/Code/Rust"
module_name = "game_config"
file_header = "// Generated for {{project.name}} at {{datetime}}\n// {{project.copyright}}"
keyword_conflict = "suffix_underscore"

[targets.exports.naming]
type = "pascal_case"
enum_value = "pascal_case"
field = "snake_case"
function = "snake_case"

[targets.exports.formatting]
indent_style = "spaces"
indent_width = 4
trailing_newline = true
trailing_blank_line = false

[targets.exports.rust]
derive_serde = true

[[targets.exports]]
type = "typescript"
output = "Generated/Code/TypeScript"
module_name = "gameConfig"
file_header = "// {{project.name}} - {{project.desc}}\n// Generated at {{datetime}}"
keyword_conflict = "suffix_underscore"

[targets.exports.naming]
type = "pascal_case"
field = "camel_case"
function = "camel_case"

[targets.exports.typescript]
readonly_fields = true
```

`targets.exports` 字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 单 export task 可省略，规范化为 `default`；多 task 省略时默认使用 `type` | 当前 target 内稳定唯一的 export task 名称。 |
| `type` | 必填 | exporter ID。当前支持 `json`、`messagepack` / `msgpack`、`bin` / `binary`、`gettext_pot` / `pot`、`lua_table` / `lua`、`rust`、`cpp` / `c++`、`typescript` / `ts`。 |
| `output` | 必填 | 产物输出目录。相对路径以 project root 或所在 import 片段目录为基准。 |
| exporter 专属字段 | 未设置 | 专属配置直接摊平在对应 `[[targets.exports]]` 下，例如 `type = "gettext_pot"` 可配置 `domain`、`file_name`；`type = "lua_table"` 可配置 `module_name`、`root_file`、`row_layout`、`lazy_require`；codegen 可配置 `file_name`、`module_name`、`namespace`、`i18n`、`naming`、`formatting` 等。 |

Export task 继承所在 target 的 effective target tags；当前没有 `[[targets.exports]].tags` 字段。CLI extra tags 对同一 target 下所有 export task 一致生效，因此同一个构建里的 JSON / MessagePack / bin / Lua 输出和格式无关 codegen 必须基于同一组 effective target tags。

同一 target 下多个 MetaAndData export task 共享 `project.input`，实现应复用同一次加载、归一化和校验结果后写出不同格式。不同 export task 的最终输出文件路径不能相互覆盖；冲突检查基于规范化绝对 artifact path。

`RuntimeDataFile.path` 表示相对当前 data artifact root / runtime loader root 的真实资源路径，例如 `items.json`。生成代码不再透传真实文件路径，而是使用逻辑表名（例如 `items`）；具体 runtime loader 决定如何把逻辑名映射到文件系统、包内资源或引擎资源中的具体格式文件。

Lua table 必须读取 `project.input` 中的数据，基于 Meta 与 target-aware ExportView 直接生成 Lua 数据模块。目标产物为一个 root facade 文件和每张表一个 `.lua` 文件；CollectionTable row 默认按数组存储，并通过 metatable 支持字段名访问。Lua 设计详见 [`../Implementation/09i-lua-embedded-data-generator.md`](../Implementation/09i-lua-embedded-data-generator.md)。

语言特定配置必须放在当前 `[[targets.exports]]` 任务下的语言子表中，采用 tagged enum 的语义解析，避免不同语言的配置字段互相污染：

| 子表 | 字段 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `[targets.exports.rust]` | `derive_serde` | `false` | 为生成的 Rust enum/struct 派生 `serde::Serialize` / `serde::Deserialize`。 |
| `[targets.exports.cpp]` | `namespace` | 继承 `targets.exports.namespace` | C++ namespace 覆盖项。 |
| `[targets.exports.typescript]` | `readonly_fields` | `false` | TypeScript interface 字段是否生成 `readonly`。 |

`targets.exports.i18n` 字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `mode` | `generic` | `generic` / `global_function`。`generic` helper 形如 `row.name_text(&translator)` / `row.NameText(translator)` / `itemsNameText(row, translator)`；`global_function` helper 不再接受 translator 参数，直接调用 `function(msgctxt, msgid)`。 |
| `function` | 未设置 | `global_function` 必填。按目标语言原样写入生成代码，例如 Rust `crate::i18n::translate`、C++ `GlobalTranslate`、TypeScript `globalTranslate`。 |
| `returns` | `string` | `string` 表示全局函数总是返回最终文案；`optional` 表示返回 missing/optional，helper 会 fallback 到 raw text。 |
| `cpp_include` | 未设置 | C++ `global_function` 可选 include，例如 `"<libintl.h>"` 或 `"\"i18n.hpp\""`。 |
| `typescript_import` | 未设置 | TypeScript `global_function` 可选 import 语句，例如 `"import { globalTranslate } from './i18n'"`。 |

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

`targets.exports.naming` 是语言生成器的可扩展配置。第一阶段统一支持以下 key：

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

`targets.exports.formatting` 是语言生成器的通用格式配置。第一阶段统一支持以下 key：

| Key | 默认值 | 说明 |
| --- | --- | --- |
| `indent_style` | `"spaces"` | 缩进 token，可选 `"spaces"` 或 `"tab"`。 |
| `indent_width` | 语言默认 | `indent_style = "spaces"` 时每一级缩进的空格数；tab 模式下保留该配置但每级缩进输出一个 tab。 |
| `trailing_newline` | `true` | 文件结束是否至少保留一个换行。 |
| `trailing_blank_line` | `false` | 文件结束是否额外保留一个空行；启用时内容以两个换行结束，并优先于 `trailing_newline`。 |

语言默认缩进：Rust / C++ 使用 4 空格，TypeScript 使用 2 空格。Project Config 只需要声明与语言默认值不同的字段；未声明的 formatting 字段由对应语言生成器默认值补齐。

原则：

- Project Config 只声明“想要的命名策略”，不应写死生成器内部类型名。
- 新语言生成器可以扩展自己的 `targets.exports.<language_specific>` 配置，但不应要求修改通用 target schema。
- 若配置中的命名策略导致同一作用域生成名冲突，必须在写文件前报错。
- 同一 target 下多个 export task 的输出路径不能互相覆盖。

推荐 runtime / aggregate / embedded data exporter type：

| Type | 状态 | 说明 |
| --- | --- | --- |
| `json` | 第一阶段默认 | 易调试，适合 Web/TS 和通用场景。 |
| `messagepack` | 第一阶段支持 | 紧凑二进制格式，适合包体和加载性能更敏感的 runtime。 |
| `bin` | 第一阶段支持 | ConfigForge 私有紧密二进制 runtime data export type。 |
| `gettext_pot` / `pot` | 第一阶段支持 | 聚合 exporter，从 target 可见 `text` 字段抽取 POT。 |
| `lua_table` / `lua` | 第一阶段支持 | 内嵌数据 exporter；Lua 中代码和数据合一，作为 MetaAndData export task 生成 root facade 和每表 `.lua` 文件。 |
| `yaml` | 可扩展 | 可读性高，但运行时解析成本需谨慎。 |
| `sqlite` | 可扩展 | 适合查询型运行时。 |
| `protobuf` | 未来设计 | 当前不要求实现；未来需要字段级 `field_number`。 |

不使用模糊的 `custom`。项目自定义格式必须注册明确的 exporter ID / type ID。

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

字段可控制类型、缺省值、是否强制不导出以及聚合 data exporter 辅助读取策略。

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
| `data_exports` | 未设置 | 控制是否可被特定聚合 data exporter 作为辅助字段读取。 |
