# 04. Meta 模型设计

## 4.0 当前实现状态

本文档同时包含目标态设计和当前阶段实现。当前代码实现状态如下：

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| TOML Meta | Implemented | 当前 loader 支持 TOML parse、imports merge、link 与基础 validate。 |
| XML Meta | Planned | 文档中的 XML 仅为目标态示例，当前 loader 不支持 XML。 |
| primitive types | Implemented | 当前支持 `int`、`float`、`bool`、`string`、`text`、`asset`。 |
| enum | Partial | 当前支持 enum 类型链接、default 校验与数据值校验。 |
| struct | Implemented MVP | 当前支持 named struct 链接、struct JSON cell 数据校验与 nested object 导出；支持由 CSV/source 形态自动决定同一个 struct field occurrence 使用 JSON cell 或 field path 展开，并支持 recursive nested struct、JSON/expanded 混用错误、target-aware child filtering 与 child default；array/map、struct child validators 与 ObjectTable key-level source location 暂未实现。 |
| default | Partial | 当前支持 primitive / enum scalar default。 |
| multi-source / override / fragments | Partial | 当前已支持 polymorphic CollectionTable 的 fragment variant source mode：多个 CSV source 可追加完整 records，并由 source `fragments` 声明激活 fragment set；也已支持 CollectionTable CSV `override_sources` 按 primary index 整行覆盖并由 `export_when` 按 effective target tags 启用。通用 multi-source split merge、ObjectTable 多源/override、base source `export_when` 与 fragment join mode 仍未实现。 |
| builtin value types | Planned | `vector3` 等 builtin value types 仍为目标态，当前尚未实现。 |
| foreign key | Implemented MVP | 当前支持 field-level/table-level FK、link/meta validate、target-aware referential integrity 和 CodeModel IR；source table 可为 CollectionTable 或 ObjectTable，target table 必须是 CollectionTable unique/primary index。 |
| shared table schema / typed fragments | Partial | 当前没有 table inheritance / base fields / reusable field group；field group 仍是方向性目标，不进入近期实现。typed fragment payload 已支持 `[[tables.fragments]] type = "StructName"` 复用标准 struct，并把该 struct 的字段按 fragment object 展开；inline fragment fields 仍继续支持。标准参考项目通过同一 `items` polymorphic table 的多个 variant sources 解决道具公共字段复用，并通过 typed `attributes -> AttributeSet` 避免重复声明属性字段。 |
| ObjectTable 特化语义 | Implemented MVP | 当前已支持 CSV `key,value` 行式 singleton ObjectTable：每行一个 top-level field key，导出单个 object，并支持 struct JSON/expanded value 与作为 FK source 引用 CollectionTable。ObjectTable 多源/override、作为 FK target、key-level source row 精确诊断、后续格子 expand 暂不实现。 |
| target tags / export_when / row_export | Implemented | 当前 Project target 支持 `tags`；Meta 中 table / field / index 支持 `export` 与 `export_when`；collection table 支持 `row_export`，并已接入 validation、runtime data export 与 codegen 的 target-aware 视图。旧字段 `tool_only` / `filter_field` / `validator_only` 仅用于报 deprecated/unsupported 诊断，不再参与导出判断。 |

## 4.1 MetaModel

MetaModel 是所有 Meta 文件 import、parse、merge 后得到的 canonical 模型。

包含：

- tables；
- enums；
- named structs；
- builtin type options；
- validators；
- source declarations；
- target references 或内联 target；
- imports metadata。

MetaModel 不包含 target-specific engine 类型名，例如 `UnityEngine.Vector3`、`FLinearColor`。这些属于 target type mapping。

## 4.2 `desc` 描述字段

Meta 支持在各个定义层级声明 `desc`，用于描述该定义的业务含义、使用方式或注意事项。

`desc` 的用途：

1. 作为 Meta 配置中的原生文档说明，替代只能依赖 TOML/XML 注释的做法。
2. 被代码生成器转换为目标语言中的文档注释，例如 TypeScript `/** ... */`、C# XML doc、Lua annotation 注释等。
3. 被特殊导出器或辅助文档生成器用作 translator comment、字段说明或配置文档内容。

支持 `desc` 的推荐层级：

| 层级 | 示例 |
| --- | --- |
| enum | 枚举整体说明。 |
| enum value | 单个枚举值的说明。 |
| named struct | 结构体整体说明。 |
| struct field | 结构体字段说明。 |
| table | 表整体说明。 |
| table field | 表字段说明。 |
| fragment | 多态 fragment 说明。 |
| fragment field | fragment 字段说明。 |
| index | 索引用途说明。 |
| foreign key | 引用关系说明。 |
| validator / export filter | 业务规则说明。 |

规则：

- `desc` 是可选字段，不参与数据解析、校验结果和 runtime data，除非某个 exporter 明确把它作为生成注释或辅助文档输入。
- `desc` 可以是单行字符串，也可以是多行字符串；实现应保留换行。
- `desc` 内容不应包含 target language 专属语法；代码生成器负责按目标语言转义和格式化。
- 如果 TOML/XML 本身还有普通注释，普通注释只用于人读源文件；`desc` 才是进入 MetaModel 和生成代码的结构化说明。
- `desc` 在 import/merge 后随定义一起保留。若同一逻辑定义被重复声明且 `desc` 不一致，应按重复定义冲突处理，除非未来设计明确的扩展/覆盖机制。

TOML 示例：

```toml
[[enums]]
name = "ItemRarity"
desc = "物品稀有度，影响掉落表现和 UI 颜色。"

[[enums.values]]
name = "common"
desc = "普通物品。"

[[enums.values]]
name = "epic"
desc = "史诗物品，通常有特殊表现。"

[[tables]]
name = "items"
kind = "collection"
desc = "所有可获得物品的基础配置。"

[[tables.fields]]
name = "display_name"
type = "text"
required = true
desc = "物品在背包和详情界面展示的名称。"
```

XML 示例：

```xml
<enum name="ItemRarity" desc="物品稀有度，影响掉落表现和 UI 颜色。">
  <value name="common" desc="普通物品。" />
  <value name="epic" desc="史诗物品，通常有特殊表现。" />
</enum>

<table name="items" kind="collection" desc="所有可获得物品的基础配置。">
  <field name="display_name" type="text" required="true" desc="物品在背包和详情界面展示的名称。" />
</table>
```

## 4.3 Table

ConfigForge 支持两类表。

### ObjectTable

ObjectTable 表示一个整体配置对象，例如全局游戏规则、经济参数、服务器开关。

特点：

- 导出一个 object。
- 不需要 index。
- 多 source 时，不同 source 可提供不同 field path。
- override source 以 field path 为单位覆盖。

Step 1 目标：ObjectTable 先支持 CSV `key,value` 行式常量表。CSV 第一行仍是 header，但每个数据行表示一个 top-level field key，而不是一条 record。默认约定如下：

```csv
key,value
max_level,100
start_gold,500
economy,"{""gold_rate"":""1.2"",""drop_rate"":""0.8""}"
start_item,"{""id"":""1"",""count"":""10""}"
```

Step 1 语义：

- `key` 列保存 Meta top-level field name；field path rows 留到后续 `expanded_value` 设计。
- `value` 列保存该 field 的 raw cell value；scalar runtime export Step 1 保持与 CollectionTable 一致，仍输出 string。
- named struct 字段可写当前已支持的 JSON object；后续也可由 CSV/source 形态决定写成 field path 展开；array/map schema 留到后续类型系统扩展。
- 同一个 ObjectTable source 归一化为一个逻辑 object row，runtime data 导出为单个 object，而不是 records array。
- 即使 source 只有 header、没有数据行，也必须归一化为一个 empty logical row `{}`，确保 required/default 在单对象上执行。
- table-level `export/export_when` 和 field-level `export/export_when` 对 ObjectTable 生效；table 可见但所有字段被过滤时导出 `{}`。
- ObjectTable Step 1 不支持 index、primary index、row_export、作为 foreign key target、variant source fragments；可以作为 foreign key source，字段值引用 CollectionTable 的 unique / primary index。
- ObjectTable Step 1 只支持 `source = "...csv"`，不支持 `sources` / `override_sources`；后续应以 field path 为覆盖粒度扩展。
- ObjectTable Step 1 不支持 field path 展开或“后续格子自动 expand 为数组元素”；复杂对象先使用 named struct JSON cell。后续 struct `expanded_value` 应支持 `field.child` 形式的 field path rows；array/map 和数组自动扩展另行设计。

Step 1 推荐 Meta：

```toml
[[tables]]
name = "game_config"
kind = "object"
source = "game_config.csv"

[[tables.fields]]
name = "max_level"
type = "int"
required = true

[[tables.fields]]
name = "economy"
type = "EconomyConfig"

[[tables.fields]]
name = "start_item"
type = "RewardItem"
```

### CollectionTable

CollectionTable 表示多条记录，例如 items、quests、levels。

特点：

- 每行 source data 通常对应一条 record。
- 可定义一个或多个 index。
- 可定义 `primary_index`。
- 可定义 `row_export`，按 target tags 裁剪集合记录。
- override source 以整条 record 为单位覆盖，必须通过 primary index 匹配。
- 可选择 `preserve_source_order = true` 暴露源顺序数组访问。

### Shared table schema / reusable field groups（目标态）

实际配置中常见“多张 source 表共享基础字段，进入同一运行时逻辑表”的建模方式。例如标准参考游戏中，策划侧可拆成 `materials.csv` / `weapons.csv` / `armors.csv` 三个 source，但它们都属于 `items` 运行时表，共享 `id`、`name`、`price`，并要求全局 ID 不重复；weapons / armors 还都拥有 attributes fragment。当前 variant source mode 已能表达“多个 source 合并为同一 polymorphic table”。Meta 仍没有 field group、mixin 或 reusable common field schema，因此 common fields 仍需要在 `items` table 内手工声明；fragment payload 复用则通过 typed fragment 引用标准 `AttributeSet` struct 表达。

目标态可考虑新增 reusable field group：

```toml
[[field_groups]]
name = "ItemCommonFields"

[[field_groups.fields]]
name = "id"
type = "int"
required = true

[[field_groups.fields]]
name = "name"
type = "text"
required = true
export_when = { any = ["client"] }

[[field_groups.fields]]
name = "price"
type = "int"
required = true

[[tables]]
name = "weapons"
use_fields = ["ItemCommonFields"]
```

field group 展开后应等价于在 table 中内联声明字段，并继续参与 duplicate field、index、foreign key、target filtering 和 codegen。该能力与 variant fragments 不同：field group 是 schema 复用机制，不表达 record 的 runtime presence。

当前优先级判断：field group 仍保持 Planned，不作为 typed fragment payload 之前的近期实现项。若未来实现，建议采用最小保守规则：

- `field_groups` 只包含 fields，不包含 index、foreign key、fragment、row_export 或 table-level validators；
- 展开发生在 Project Config / Meta imports 全部 merge 完成之后、link 之前；
- 多个 group 按 table `use_fields` 声明顺序稳定展开，再追加 table 本地 fields；
- group field 与 table 本地 field 或其他 group 展开字段重名直接报错，不支持局部 override；
- 展开后复用现有 duplicate field、index/FK 引用、target filtering、validation 和 codegen 逻辑；
- group 名称使用全局 Meta namespace，重复 group name 按 duplicate meta definition 报错；
- group 不被 struct 或 fragment typed payload 直接引用；fragment payload 复用标准 struct，而不是 field group。

### Typed fragment payload（Implemented MVP）

当前 fragment 是一组局部字段，runtime payload 输出为：

```json
{ "attributes": { "attack": "12", "defence": "3" } }
```

如果希望 `attributes` fragment 复用 named struct `AttributeSet`，当前实现允许 fragment 直接声明 `type = "AttributeSet"`；不需要重复声明 `max_health/max_stamina/attack/defence` 四个 fragment fields，也不会引入额外 wrapper field。

推荐方案：**不新增一套专门的 fragment struct 类型**。fragment 的 payload shape 直接复用标准 `[[structs]]`；`[[tables.fragments]]` 仍然需要存在，但它的职责是声明“这个 fragment 名称、激活条件、导出策略和 payload 类型”，而不是再定义一套独立结构系统。

当前实现允许 fragment 直接声明标准 struct 作为 payload type：

```toml
[[structs]]
name = "AttributeSet"

[[structs.fields]]
name = "max_health"
type = "int"
default = 0

[[structs.fields]]
name = "attack"
type = "int"
default = 0

[[tables.fragments]]
name = "attributes"
type = "AttributeSet"
```

其语义是：

- fragment object 的字段由 named struct `AttributeSet` 提供，source header 使用 `attributes.max_health`、`attributes.attack` 等平铺路径；
- validation / resolver 对 fragment payload 复用 struct JSON / expanded value、child default、target-aware child filtering；
- runtime payload 仍是 fragment name 对应的 object，不额外包裹 wrapper field；
- codegen 可以复用 `AttributeSet` 类型，避免为每张表重复生成等价 fragment shape。
- `type` 指向已有标准 struct；不引入 `[[fragment_structs]]` / `[[tables.fragment_structs]]` 之类的新顶层概念，避免 struct 与 fragment struct 两套 schema 规则漂移。

为了兼容非常小的一次性 fragment，`[[tables.fragments.fields]]` 可继续作为 inline payload 的简写；但推荐准则是：只要 payload 有复用可能、需要 codegen 命名稳定、或会被文档/工具单独理解，就定义标准 struct 并在 fragment 上引用 `type`。同一个 fragment 不应同时配置 `type` 和 `fields`，Meta validation 应明确拒绝这种混用。

typed fragment payload 的详细规则：

| 维度 | 规则 |
| --- | --- |
| Raw TOML | `[[tables.fragments]] type = "StructName"` 通过 serde rename 映射到 `FragmentMeta.type_name`；`type` 与 `fields` 互斥。 |
| Link | `type` 必须能链接到标准 named struct；linked model 中 payload 表达为 `LinkedFragmentPayload::Struct`。 |
| Empty fragment | inline fragment 可继续允许空 fields 以表达纯 presence；typed fragment 的 struct 可以没有 visible child，但 raw struct definition 仍必须合法。 |
| Source parent path | fragment name 是 struct occurrence parent path。例如 `attributes` + `type = "AttributeSet"` 可读取 parent JSON cell `attributes = "{...}"`，也可读取 expanded descendant `attributes.attack`。 |
| Nested path | typed fragment 复用 recursive struct path 规则，允许 `attributes.offense.attack` 这类 nested struct child path；旧的“fragment path 仅两段”限制只适用于 inline fragment fields。 |
| Layout mixing | 同一 typed fragment occurrence 不允许同时出现 parent JSON cell 与任一 descendant expanded path，例如同一 row/source 中 `attributes` 与 `attributes.attack` 同时出现应报 mixing error。 |
| Required/default | source 激活 typed fragment 时，该 fragment occurrence 被视为 active。struct child required/default 按普通 struct occurrence 规则执行；visible child default 可以构成 effective value。fragment 本身不新增 required 字段，presence 由 source fragments 决定。 |
| Target filtering | 先判断 row 是否 visible，再判断 fragment `export_when`，最后判断 struct child `export_when`。fragment hidden 时跳过其 payload value validation/export/codegen；fragment visible 但某 child hidden 时跳过该 child。 |
| Runtime payload | 输出仍是 `fragment_name -> struct children`，不增加 wrapper。source 激活且 fragment 对当前 target 可见时，即使没有 effective child value，也默认保留空 object 以表达 presence，与 inline fragment 行为一致。 |
| Codegen | row 字段应使用 optional presence：`attributes: Option<AttributeSet>`（Rust）、`attributes?: AttributeSet`（TypeScript）、`std::optional<AttributeSet>`（C++）。target filtering 后如果 struct child 被裁剪，生成 target-specific 的可见 shape；若语言生成器无法生成 partial named struct，应生成 target-specific projected type，不能把 hidden child 暴露到该 target API。 |

## 4.4 Field

字段定义逻辑数据形状和工具策略。

示例：

```toml
[[tables.fields]]
name = "price"
type = "int"
required = true
default = 0
desc = "商店购买价格。"
```

核心属性：

| 属性 | 含义 |
| --- | --- |
| `name` | 字段名。 |
| `desc` | 字段描述，可用于 Meta 文档和生成代码注释。 |
| `type` | 字段逻辑类型。 |
| `required` | 当前验证视图中是否必须有值。 |
| `default` | 数据未提供值时使用的默认值。 |
| `layout` | 复合值 source layout 的历史字段；named struct 字段不再通过 Meta 指定 JSON cell 或 expanded value，必须由 CSV/source 中实际出现的是 parent path 还是 child paths 自动决定。 |
| `export` | 是否允许进入 runtime ExportView；`false` 表示强制不导出。 |
| `export_when` | 按 target tags 判断是否进入当前 ExportView。 |
| `special_exports` | 是否允许特殊导出器读取。 |

字段路径相关命名规则：

- table field name、struct field name、fragment name 是 canonical field path 的组成部分，第一阶段均不允许包含 `.`, `[`, `]`。
- `.` 保留为 struct / fragment field path 分隔符；`[`、`]` 预留给未来固定槽 array path，例如 `rewards[0].item_id`。
- 如果需要在 CSV 中展示更友好的表头，例如 `Attributes.Attack`、`属性.攻击`，应通过后续 header alias / source mapping 解决，不应进入 canonical name。
- Meta validation 应在加载/校验阶段阻断包含保留字符的名称，避免 source path parser 产生歧义。

## 4.5 Target Tags 与 ExportView 过滤

Target 使用 `tags` 描述导出上下文，例如 `client`、`server`、`debug`、`shipping`。这些 tag 不内建业务语义，只作为匹配条件。Meta 中表、字段和索引通过 `export_when` 匹配 target tags。

`export_when` 结构：

```toml
export_when = { any = ["client"], all = ["shipping"], none = ["debug"] }
```

匹配规则：

1. `any` 非空时，target tags 至少命中其中一个。
2. `all` 非空时，target tags 必须包含全部 tag。
3. `none` 非空时，target tags 不能包含其中任何 tag。
4. 三类条件都为空时表示不限制 target。
5. `export = false` 优先级最高，表示永不进入 runtime ExportView。

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
```

普通 index 在当前 target 中只有满足以下条件才进入 ExportView：

```text
index.exported_for(target)
  = index.export_when matches target.tags
  AND every index field is exported_for(target)
```

如果 primary index 在当前 target 下不可用，必须报错，不能静默删除。

## 4.6 Row Export Policy

CollectionTable 可以声明 `row_export`，由数据列控制某一行是否进入当前 target 的 ExportView。该机制用于替代旧的硬编码过滤字段，常见场景是 debug-only、shipping-only、平台、地区、渠道数据裁剪。

```toml
[[tables]]
name = "item"
source = "item.csv"

[tables.row_export]
required_tags_field = "export_tags"
excluded_tags_field = "exclude_tags"
match = "any"

[[tables.fields]]
name = "export_tags"
type = "string"
export = false

[[tables.fields]]
name = "exclude_tags"
type = "string"
export = false
```

语义：

| 字段 | 含义 |
| --- | --- |
| `required_tags_field` | 行要求 target 具备的 tags。空值表示不限制。 |
| `excluded_tags_field` | target 命中任一排除 tag 时该行不导出。空值表示不排除。 |
| `match = "any"` | `required_tags_field` 多个 tag 命中任一即可。 |
| `match = "all"` | `required_tags_field` 多个 tag 必须全部命中。 |

行数据示例：

```csv
id,name,export_tags,exclude_tags
1,Potion,,
2,Debug Sword,debug,
3,Shipping Gift,shipping,
4,Internal Test Item,,shipping
5,Server Reward,server,
6,Client Preview Item,client,
```

`export_tags` / `exclude_tags` 字段通常设置 `export = false`，因为它们只用于形成 ExportView，不进入 runtime data。字段是否可被行过滤引用不再需要额外 `filter_field` 开关。

## 4.7 Unsupported 导出字段

Meta 导出机制使用 `export`、`export_when` 与 `row_export`；以下字段在当前设计中不作为有效配置：

| 字段 | 推荐表达 |
| --- | --- |
| `tool_only = true` | `export = false`，或通过工具专用 target tags 单独导出。 |
| `validator_only = true` | `export = false`；校验仍基于 Full DataSet 或目标 ExportView 执行。 |
| `filter_field = true` | `row_export.required_tags_field` / `row_export.excluded_tags_field` 显式引用字段。 |

实现应对这些字段输出 unsupported 诊断，避免用户误以为它们会参与 ExportView 构造。

## 4.8 Missing、Default、Required

> 实现状态：Partial。当前实现已支持 primitive / enum 字段的 scalar `default` 校验，并会在数据验证/导出前把 Missing 或 Empty 值替换为 default；同时支持 primitive / enum list 字段的 TOML array default。struct、table 等复合 default 暂未实现。

字段值状态：

| 状态 | 含义 |
| --- | --- |
| Missing | source 中没有提供该字段或 field path。 |
| Empty | 提供了空字符串、空数组、空对象等值。 |
| Null | source 明确写入 null，通常仅 JSON cell 中出现。 |
| Value | 成功解析出的 typed value。 |

规则：

1. 如果字段 Missing 且配置了 `default`，使用 default。
2. 如果字段 Missing 且 `required = true` 且无 default，报 required error。
3. 如果字段 Empty 是否满足 required，由类型决定：
   - string/text：空字符串默认不满足 required，除非字段允许 empty；
   - array：空数组默认不满足 non-empty 要求，但是否满足 required 由 field policy 决定；
   - struct：空对象缺少 required child 时错误。
4. Null 默认不满足 required。
5. default 必须能按字段类型解析和校验。

## 4.9 字段类型

### Primitive Types

| 类型 | 含义 |
| --- | --- |
| `int` | 整数。 |
| `float` | 浮点数。 |
| `bool` | 布尔值。 |
| `string` | 普通字符串。 |
| `text` | 国际化文本逻辑值。 |
| `asset` | 资源引用逻辑值。 |

### Builtin Value Types

优先使用 builtin value type 表达通用游戏值：

| 类型 | 含义 |
| --- | --- |
| `color_rgb` / `color_rgba` | 颜色。 |
| `vector2` / `vector3` / `vector4` | 浮点向量。 |
| `vector2_int` / `vector3_int` | 整数向量。 |
| `rotation` | Euler 旋转。 |
| `datetime` / `date` / `time` / `duration` | 时间相关值。 |
| `int_range` / `float_range` | 范围。 |
| `rect` / `bounds3` | 区域和包围盒。 |

确认规则：

- `vector3` 等数学类型优先作为 builtin value type。
- 项目不应使用 named struct 重新定义同名 builtin 类型。
- `color_channel` 必须显式为 `byte` 或 `normalized`，禁止 `auto`。
- `rotation_unit` 推荐默认 `degrees`，可字段级或项目级配置。
- local datetime 如果要表示 instant，必须声明 timezone policy。

### Enum

Enum 定义允许值集合。

```toml
[[enums]]
name = "ItemRarity"
desc = "物品稀有度。"

[[enums.values]]
name = "common"
desc = "普通。"

[[enums.values]]
name = "rare"
desc = "稀有。"

[[enums.values]]
name = "epic"
desc = "史诗。"
```

### Named Struct

Named struct 用于业务复合对象，例如 Reward、Cost、Condition、StatBlock。

```toml
[[structs]]
name = "Reward"
desc = "奖励条目，表示获得指定数量的物品。"

[[structs.fields]]
name = "item_id"
type = "int"
required = true
desc = "奖励物品 ID，引用 items 表。"

[[structs.fields]]
name = "count"
type = "int"
default = 1
desc = "奖励数量。"
```

Named struct 不能形成直接或间接循环引用，除非未来引入显式 indirection 类型。

### List / Array

当前支持标量数组字段，语法为 `T[]` 或 `list<T>`，其中 `T` 可为 primitive 或 enum。CSV / ObjectTable cell 使用 JSON array 写法：

```toml
[[tables.fields]]
name = "starter_item_ids"
type = "int[]"
required = true
foreign_key = { target_table = "items", target_index = "by_id" }
```

```csv
key,value
starter_item_ids,"[2001,2002]"
```

规则：

- JSON / MessagePack runtime payload 中数组元素仍遵循当前 scalar string-compatible contract，例如 `int[]` 输出为 `["2001", "2002"]`；`bin` 使用 schema-aware typed layout，写入 length prefix 后按元素类型写入。
- 代码生成映射为 Rust `Vec<T>`、TypeScript `T[]`、C++ `std::vector<T>`；optional list 为对应语言的 optional 容器。
- list default 必须是 TOML array，且每个元素都必须匹配元素类型。
- 当前只支持 list 作为 JSON cell，不支持 `items[0]` / `items[0].field` 形式的 expanded array path；array field path 仍是后续能力。
- 当前只支持标量/enum元素数组；`list<Struct>`、嵌套 list、map 暂不作为稳定能力。

## 4.10 复合值书写方式

这里描述的是 **struct / array 等复合值如何在二维数据表中书写**，而不是固定使用“列展开”或“行展开”这样的物理名词。

ObjectTable 和 CollectionTable 的基础组织方式不同：

- ObjectTable 通常是一行表示一个 field path，例如 `field,value` 布局中每行写一个字段。
- CollectionTable 通常是一列表示一个 field path，每行写一条 record。

因此，同一个复合值的“展开书写”在不同表类型中会落到不同物理方向上：

- 在 CollectionTable 中，展开后的 field path 通常表现为多个列。
- 在 ObjectTable 中，展开后的 field path 通常表现为多行 field/value 项。

为避免把逻辑书写方式绑定到物理方向，本文档使用以下术语：

| Layout | 用途 |
| --- | --- |
| `scalar_cell` | primitive、builtin value 或简单值写在一个 cell 中。 |
| `json_cell` | struct、array、深层嵌套数据聚合写入一个 JSON cell。 |
| `expanded_value` | struct 或固定槽 array 按 field path 展开书写；具体表现为列还是行由 table kind 和 source mapping 决定。 |

当前不要求支持“独立子表式明细布局”，即不要求把某个 array 的元素放在另一张明细表中再通过 parent key / child key 关联回来。

### Struct Source Layout Auto Detection

Named struct 字段不要求在 Meta 中固定声明使用 JSON cell 还是 expanded value。具体写法由每个 CSV/source 中实际出现的 field path 决定：如果出现 parent field path 本身，则该 occurrence 按 JSON cell 解析；如果出现 child field paths，则该 occurrence 按 expanded value 解析。同一个 named struct type 可以在不同 table、不同父级字段、甚至同一 table 的不同字段 occurrence 中采用不同写法。

Field path 使用 **字段名** 逐级拼接，而不是使用 struct type name。比如 table field 名为 `attributes`，它的 child field 名为 `attack`，则展开路径是：

```text
attributes.attack
attributes.max_health
```

这里的 `attributes` 是 table field name，不是 `Attributes` 类型名。如果同一个 `Attributes` struct type 在同一个父级中出现多次，应使用不同父级字段路径区分，例如：

```csv
id,base_attributes.attack,growth_attributes.attack
1001,10,2
```

在 CSV header 中推荐使用 canonical dotted path。列名比较应按字段名大小写敏感；如果项目或表格作者更偏好 PascalCase header，例如 `Attributes.Attack`，应通过后续显式 header alias / source mapping 解决，不应改变 canonical field path。当前 canonical 规则仍是 Meta 中声明的各级 `field.name`。

CollectionTable 中，struct expanded value 表现为同一 record 的多个列：

```csv
id,name,attributes.attack,attributes.max_health
1001,Warrior,10,120
1002,Mage,4,70
```

ObjectTable 的 Step 1 source 仍是 `key,value` 两列；struct expanded value 表现为多个 key rows：

```csv
key,value
max_player_level,60
default_attributes.attack,10
default_attributes.max_health,120
```

Struct source layout 判定规则：

- 判定单位是 struct field occurrence 的完整 parent field path，而不是 struct type。例如 `base_attributes` 和 `growth_attributes` 是两个独立 occurrence，即使它们类型相同，也分别扫描和判定。
- 如果某个 occurrence 在当前 source 中出现 parent path，例如 `attributes`，且没有出现 `attributes.*` child paths，则按 JSON cell 解析。
- 如果某个 occurrence 在当前 source 中没有 parent path，但出现一个或多个 `attributes.*` child paths，则按 expanded value 解析。
- 不支持同一个 occurrence 同时使用 JSON cell 和 expanded value；如果同一 source 中同时出现 `attributes` 与 `attributes.attack`，必须报 source schema error。
- nested struct occurrence 使用同样规则递归判定。某一级选择 expanded value 后，其内部 struct child 可以继续选择 JSON cell 或继续 expanded value；某一级选择 JSON cell 后，该 JSON cell 内部被视为 opaque object，source 中不允许再提供它的展开子项。
- 例如 `attributes.offense` 可以是 JSON cell；如果没有 `attributes.offense` 但出现 `attributes.offense.attack`，则 `offense` 这个 occurrence 按 expanded value 解析；如果同时出现 `attributes.offense` 与 `attributes.offense.attack`，则属于同一 nested occurrence 混用，必须报错。
- required/default/type/enum 校验作用于 child field path；child 未提供且有 default 时使用 default；child required 且没有有效值时报 `field.child is required`。
- parent struct field 自身的 `required = true` 表示该 nested object 在当前验证视图中必须“有效存在”：至少一个当前可见 child path 提供非空值，或至少一个当前可见 child 有 default 被应用。child 是否必填仍由 child 自己的 `required` 控制。如果 parent 可见但当前 target 没有任何 visible child，parent required 不触发失败，runtime payload 可输出 empty object。
- 第一阶段不支持 whole-struct parent default；struct field 不能通过 `default` 提供整个 object。expanded value 中仅支持 struct child fields 自己的 default。
- target-aware export 先判断 parent field 是否可见，再判断 child field 是否可见；parent 可见但所有 child 被过滤时 runtime payload 输出 empty object。
- runtime data shape 与 `json_cell` 保持一致，仍输出 nested object，例如 `{ "attributes": { "attack": "10", "max_health": "120" } }`；scalar child 第一阶段仍保持 string contract。

Struct expanded value 与 fragment dotted key 相似但语义不同：fragment key 的第一段是 table fragment name，表示 polymorphic presence；struct expanded key 的每一段都是从当前 table field 或 struct field 继续解析得到的字段名路径。解析时必须先用 table meta 判定第一段属于 common field、fragment 还是未知名称，再沿 struct field schema 逐级解析，不能把 `attributes` 当作 type name 处理，也不能只根据是否包含 `.` 做统一处理。

Nested struct 示例：

```toml
[[structs]]
name = "Offense"

[[structs.fields]]
name = "attack"
type = "int"

[[structs]]
name = "Attributes"

[[structs.fields]]
name = "offense"
type = "Offense"
```

如果 source 中出现 direct child `attributes.offense`，且没有出现 `attributes.offense.*`，则 `offense` occurrence 按 JSON cell 解析：

```csv
id,attributes.offense
1001,"{""attack"":""10""}"
```

如果 source 中没有 `attributes.offense`，但出现 `attributes.offense.attack`，则 `offense` occurrence 按 expanded value 解析。递归展开仍按每一级字段名判定，不能按 `Offense` 类型名判定。若同一 source 同时提供 `attributes.offense` 和 `attributes.offense.attack`，表示 `offense` 既作为 JSON cell 又继续展开，必须报混用错误。

Layout 归属规则：

- struct 是可复用类型，JSON cell / expanded value 是某个 source 中某个 field occurrence 的书写方式。为了让同一 struct type 能根据不同表格的编辑习惯复用，默认由 CSV/source 实际 field paths 决定。
- 自动判定需要扫描当前 source 的 header/key 集合。只出现 parent path 时是 JSON cell；只出现 child paths 时是 expanded value；两者同时出现是混用错误。
- Meta 仍负责定义字段类型、required/default/export_when 等逻辑 schema，不负责把某个 struct type 或 field occurrence 固定为展开或 JSON。
- named struct 字段上的 `layout = "json_cell"` / `layout = "expanded_value"` 不再提供任何 source policy 语义；新设计要求省略该字段，避免 Meta 与 source header/key 之间出现第二套控制来源。

### Array FieldPath

当前 list 字段以 JSON cell 解析。以下 `expanded_value` 固定槽 array path 是后续设计，尚未作为当前实现能力；如果未来复合值使用 `expanded_value`，固定槽 array 可用数组 slot：

```text
rewards[0].item_id
rewards[0].count
rewards[1].item_id
rewards[1].count
```

设计建议：

- 数组 slot 使用 0-based index。
- 对 Lua 等习惯 1-based 下标的目标语言，数组下标差异由代码生成器或 runtime adapter 处理；Meta 和 source mapping 的 canonical 表达仍使用 0-based。
- 同一元素的字段应相邻排列，避免 `rewards[0].x, rewards[1].x, rewards[0].y` 这种交错布局。
- 如果能通过连续相邻列推断元素分组，未来可考虑省略显式 `[0]`，但当前 canonical 写法仍保留 index。
- expanded value 中的 array index 默认必须连续；不支持 sparse，除非未来显式设计。

## 4.11 Index 与 Primary Index

CollectionTable 可定义多个 index。

```toml
[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
primary = true
desc = "按物品 ID 精确查找。"
```

规则：

- 取消 `primary_key` 概念。
- `primary_index = "by_id"` 和 index 上 `primary = true` 都可表达主索引；如果两者同时存在必须一致。
- Primary index 必须是 unique index。
- Foreign key 默认指向目标表 primary index。
- Override source 对 CollectionTable 的行替换依赖 primary index。

Index cardinality：

- `unique = true`：一个 key 至多对应一条 record。
- `unique = false`：一个 key 可对应多条 record。

## 4.12 Foreign Key

外键声明字段值引用另一张表的 unique index。

单字段外键推荐直接写在字段上，作为便捷语法：

```toml
[[tables.fields]]
name = "item_id"
type = "int"
required = true
foreign_key = { target_table = "items", target_index = "by_id" }
```

如果省略 `target_index`，默认引用目标表 primary index：

```toml
[[tables.fields]]
name = "item_id"
type = "int"
foreign_key = { target_table = "items" }
```

多字段外键或需要引用复合 unique index 时，使用 table-level 配置：

```toml
[[tables.foreign_keys]]
name = "item_in_region"
fields = ["item_id", "region"]
target_table = "items"
target_index = "by_id_region"
```

规则：

- 单字段外键优先使用字段级 `foreign_key` 语法糖；table-level `[[tables.foreign_keys]]` 主要用于复合外键。
- 字段级外键只允许引用目标表的一字段 unique / primary index；如果要引用复合 unique index，必须使用 table-level 配置。
- 标量数组字段可声明外键，语义是数组中的每个元素独立引用目标表的一个 key；数组外键只支持一个 source list field 引用一个单字段 unique / primary index。
- 数组字段不能参与复合 FK，例如 `fields = ["item_ids", "region"]` 或多个数组字段组成 tuple 都不支持。若需要复合 key 数组，后续设计应使用 `list<StructRef>` 及元素级 field path FK（例如 `level_refs[].class_id + level_refs[].level`），当前不实现。
- 字段级外键是否可为空默认由字段 `required` 决定：`required = true` 时不允许空，`required = false` 时允许空；也可以通过 `allow_empty` 显式覆盖。
- table-level 外键省略 `allow_empty` 时，默认按 source fields 的 `required` 派生：所有 source fields 都是 optional 时允许全空，否则不允许全空；也可以通过 `allow_empty` 显式覆盖。
- required foreign key 缺值时报 required error。
- optional foreign key 全空时跳过 FK resolution。
- composite foreign key 仅在 `allow_empty = true` 或默认派生为允许空时，全空可跳过；部分为空始终报 partial key error。
- foreign key target 必须是 unique index 或 primary index。
- FK source table 可为 CollectionTable 或 ObjectTable；ObjectTable source 按其 singleton logical row 执行引用完整性校验。
- FK target table 必须是 CollectionTable，且 target index 必须是 unique 或 primary；ObjectTable 仍不能作为 FK target，因为它没有 index / primary index 语义。

## 4.13 Multi-source 与 Override

> 实现状态：Partial。当前版本已支持 fragment variant source mode 这一受限多源能力：polymorphic CollectionTable 可声明多个 CSV `sources`，但至少一个 source 必须声明 `fragments`；同时已支持 CollectionTable CSV `override_sources` 按 primary index 匹配整行覆盖，并支持 source-level `export_when` 按 effective target tags 启用。ObjectTable 多源/override、base source `export_when`、通用 split merge 与非 `csv` provider 仍会在 Meta 校验或 DataLoadPlan 构造阶段报错，避免静默忽略。

本节的 multi-source split merge / override 是通用多源能力，仍为 Reserved。下一节的 polymorphic variant source mode 是多源能力的受限子集：仅允许 `polymorphic = true` 的 CollectionTable 声明多个 variant sources，且每个 source 追加完整 records，不提供通用 split merge、ObjectTable 多源合并或 override 语义。

Base `sources` 用于 split merge：

- CollectionTable：多个 source 的 records 追加为一个集合。
- ObjectTable：多个 source 提供同一对象的不同字段。
- 唯一索引冲突在最终组合视图中检查。

`override_sources` 用于覆盖：

- ObjectTable：按 field path 覆盖。
- CollectionTable：按 primary index 匹配整条 record 并替换。
- 按列表顺序应用，不使用 priority。
- `override_sources[].export_when` 使用 effective target tags 匹配，未匹配的 override source 视为不存在。该能力用于赛季、热修、地区灰度等临时覆盖；例如 `materials_s1.csv` 只在包含 `s1` tag 的构建中覆盖 Oak 材料售价。
- CollectionTable override source 必须提供 primary index 所需字段，并且默认只能覆盖已存在的 base row；找不到 base row 应报错，避免把覆盖表误用为追加表。

## 4.14 Polymorphic Fragment

> 实现状态：Implemented MVP。当前版本已按 **variant source mode** 支持 polymorphic fragments，包括 `FragmentMeta`、source `fragments` 声明、按行 source/fragments 元数据、fragment scoped required/default/type 校验、target-aware validation、runtime nested object export，以及 Rust / TypeScript / C++ fragment 类型生成。fragment join mode 仍不实现。

多态表用于表达“同一逻辑集合中，不同 record 拥有不同附加字段集合”的场景，例如物品表中的 weapon、equipment、usable。ConfigForge 的多态模型使用 composition，不使用继承：record 始终由 table common fields 加零个或多个 fragment fields 组成。

### 4.14.1 配置形态

variant source mode 中，每个 source 声明自己贡献的 fragment 集合：

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
```

当前实现中 fragment payload 只能定义为 table 内的 inline 字段组，复用 `FieldMeta` 的字段属性：

```toml
[[tables.fragments]]
name = "stats"
desc = "战斗属性 fragment。"

[[tables.fragments.fields]]
name = "attack"
type = "int"
default = 0

[[tables.fragments.fields]]
name = "defence"
type = "int"
default = 0

[[tables.fragments]]
name = "usable"

[[tables.fragments.fields]]
name = "cooldown"
type = "float"
required = true
```

> 说明：当前代码已落地 `FragmentMeta` / `LinkedFragmentMeta`，`TableMeta` 包含 `fragments: Vec<FragmentMeta>`，link 阶段会把 table-local fragments 链接为可查询结构。`SourceRef.fragments` 保留为每个 source 的 active fragment 名称集合，由 DataLoader 写入每行 `RowSourceInfo`，再由 Validation 阶段按行解释。

目标态中，fragment payload 可以改为引用标准 struct，而不是继续重复 inline fields：

```toml
[[structs]]
name = "ItemStack"

[[structs.fields]]
name = "max_stack"
type = "int"
required = true

[[structs]]
name = "SkillGrant"

[[structs.fields]]
name = "skill_asset"
type = "asset"
required = true
export_when = { any = ["client"] }

[[tables.fragments]]
name = "stack"
type = "ItemStack"

[[tables.fragments]]
name = "attributes"
type = "AttributeSet"

[[tables.fragments]]
name = "skill"
type = "SkillGrant"
export_when = { any = ["client"] }
```

这里 `[[tables.fragments]]` 不是“专门的结构体定义”，而是 polymorphic presence 的声明：source 通过 `fragments = ["attributes"]` 激活该 fragment，运行时 payload key 仍为 `attributes`，payload shape 来自标准 struct。这样保持一套 struct 规则，同时保留 fragment 的按行 presence、source variant、target policy 和 runtime wrapper key 语义。

目标 Rust schema：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FragmentMeta {
    pub name: String,
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub export_when: ExportCondition,
    #[serde(default)]
    pub fields: Vec<FieldMeta>,
}
```

Phase 1 中 fragment 只表达一组可 target-filter 的字段，不引入完整 table 能力：

- 不支持 fragment-level validators。
- 不支持 fragment field validators，直到 validator framework 支持 fragment/nested field scope。
- 不支持 fragment indexes。
- 不支持 fragment row_export；`row_export` 仍只作用于整条 record。
- 不支持 fragment fields 作为 primary/unique index key。

### 4.14.2 语义规则

规则：

- variant source mode 只允许 `polymorphic = true` 的 CollectionTable 使用多个 sources，且至少一个 source 必须声明非空 `fragments`，避免把无 fragment 的多 CSV append 与未来通用 multi-source merge 语义混淆。
- 每个 source 贡献完整 record 加声明的 fragments。
- base sources 中同 ID 冲突，即使 fragments 不同也冲突。
- 不支持多个 source 按 ID 拼接 fragment。
- source 声明了某 fragment，则该 source 的每行都必须满足该 fragment schema。
- source 未声明的 fragment 字段即使在 CSV 中出现也应报错，避免把 weapon-only 字段静默带入 usable 记录。
- common fields 对所有 source 生效；fragment fields 只对声明了对应 fragment 的 source 生效。
- `required` / `default` 在 fragment schema 内按普通字段规则处理：声明了 fragment 的 source 中，缺失 fragment required field 且无 default 时失败；缺失但有 default 时补齐。
- fragment 字段可以使用 `export` / `export_when`，运行时 ExportView 按 target tags 过滤 fragment 字段；如果某个 fragment 在当前 target 下没有任何可导出字段，仍可保留 fragment presence 信息供代码访问。
- target validation 只对当前 target 的 visible rows 生效；但 visible row 中出现的 `fragment.field` 仍必须引用已存在、且被该行 source 激活的 fragment field。`export_when` 只影响 required/type/export/codegen 是否参与当前 target，不表示 CSV 中可以出现任意 hidden dotted key。
- CollectionTable 的 primary/unique index 第一阶段只允许引用 common fields，不允许引用 fragment fields。这样可以保证不同 variant source 之间使用同一套主键冲突规则。
- `polymorphic = true` 的 table 才允许 source 声明 `fragments`；非 polymorphic table 配置 fragments 应报 Meta validation 错误。

命名空间与 CSV header 规则：

- fragment 是 table record 中的一个具名结构体，拥有自己的字段命名空间；runtime payload 使用 fragment name 作为 nested object key，例如 `"stats": { "attack": "10" }`。
- fragment names 在 table scope 内唯一。
- fragment name 不能与 table common field name 冲突，因为 runtime payload 的同一层级需要区分 common scalar/object field 与 fragment object。
- fragment field names 只要求在所属 fragment 内唯一；不同 fragments 之间可以复用相同 field name，例如 `stats.value` 与 `cost.value`。
- fragment field name 可以与 common field name 相同，因为 CSV header 和 runtime payload 都通过 fragment name 隔离该字段，例如 common field `value` 与 fragment field `stats.value` 可以同时存在。
- CSV header 中 inline fragment field 使用 `fragment.field` 形式，例如 `stats.attack` / `stats.defence`；common field 仍使用裸 field name，例如 `id` / `name`。
- Inline fragment fields 第一阶段不支持多级 fragment path；`stats.attack` 中 `stats` 必须是当前 table 的 fragment name，`attack` 必须是该 fragment 内的直接 field name。
- Typed fragment payload 复用 struct occurrence path 规则，因此允许 `fragment.child.grandchild`。source path classifier 应先匹配 table common field，再匹配 active fragment name；若 fragment payload 是 inline fields，则按 direct field 匹配；若 payload 是 struct，则把剩余 path 作为 struct child path 递归解析。

### 4.14.3 与 fragment join mode 的边界

第一阶段明确不支持 fragment join mode：

```text
items_base.csv      提供 id/name/kind
items_weapon.csv    只提供 id/attack/skill
items_usable.csv    只提供 id/cooldown
```

上述多个 source 按同一 ID 拼成一条 record 的能力不在本阶段范围内。variant source mode 要求每个 source 自己提供一条完整 record 所需的 common fields 和声明 fragments 的字段。这能降低 merge、诊断和代码生成复杂度，也更适合先服务“不同物品类型拆不同 CSV 文件”的需求。
