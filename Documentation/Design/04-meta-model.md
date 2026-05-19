# 04. Meta 模型设计

## 4.0 当前实现状态

本文档同时包含目标态设计和当前阶段实现。当前代码实现状态如下：

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| TOML Meta | Implemented | 当前 loader 支持 TOML parse、imports merge、link 与基础 validate。 |
| XML Meta | Planned | 文档中的 XML 仅为目标态示例，当前 loader 不支持 XML。 |
| primitive types | Implemented | 当前支持 `int`、`float`、`bool`、`string`、`text`、`asset`。 |
| enum | Partial | 当前支持 enum 类型链接、default 校验与数据值校验。 |
| struct | Partial | 当前支持 named struct 链接、`json_cell` 数据校验与 nested object 导出；`expanded_value`、array/map 与 struct child validators 暂未实现。 |
| default | Partial | 当前支持 primitive / enum scalar default。 |
| multi-source / override / fragments | Partial | 当前已支持 polymorphic CollectionTable 的 fragment variant source mode：多个 CSV source 可追加完整 records，并由 source `fragments` 声明激活 fragment set；通用 multi-source split merge、ObjectTable 多源合并、`override_sources` 与 fragment join mode 仍未实现。 |
| builtin value types / foreign key / ObjectTable 特化语义 | Planned | 当前尚未实现。 |
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
2. 被 inspect/debug 命令展示，帮助用户理解合并后的 MetaModel。
3. 被代码生成器转换为目标语言中的文档注释，例如 TypeScript `/** ... */`、C# XML doc、Lua annotation 注释等。
4. 被特殊导出器或辅助文档生成器用作 translator comment、字段说明或配置文档内容。

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

### CollectionTable

CollectionTable 表示多条记录，例如 items、quests、levels。

特点：

- 每行 source data 通常对应一条 record。
- 可定义一个或多个 index。
- 可定义 `primary_index`。
- 可定义 `row_export`，按 target tags 裁剪集合记录。
- override source 以整条 record 为单位覆盖，必须通过 primary index 匹配。
- 可选择 `preserve_source_order = true` 暴露源顺序数组访问。

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
| `layout` | 复合值在 source 中的书写方式，例如 scalar cell、JSON cell、expanded value。 |
| `export` | 是否允许进入 runtime ExportView；`false` 表示强制不导出。 |
| `export_when` | 按 target tags 判断是否进入当前 ExportView。 |
| `special_exports` | 是否允许特殊导出器读取。 |

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

## 4.7 废弃的旧导出字段

目标态不再保留 `tool_only`、`validator_only`、`filter_field` 作为 Meta 导出机制：

| 旧字段 | 替代方式 |
| --- | --- |
| `tool_only = true` | `export = false`，或通过工具专用 target tags 单独导出。 |
| `validator_only = true` | `export = false`；校验仍基于 Full DataSet 或目标 ExportView 执行。 |
| `filter_field = true` | `row_export.required_tags_field` / `row_export.excluded_tags_field` 显式引用字段。 |

后续实现应直接迁移到新机制，不提供旧字段兼容语义。

## 4.8 Missing、Default、Required

> 实现状态：Partial。当前实现已支持 primitive / enum 字段的 scalar `default` 校验，并会在数据验证/导出前把 Missing 或 Empty 值替换为 default；struct、array、table 等复合 default 暂未实现。

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

### Array FieldPath

如果复合值使用 `expanded_value`，固定槽 array 可用数组 slot：

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

```toml
[[tables.fields]]
name = "item_id"
type = "int"
required = true

[tables.fields.foreign_key]
table = "items"
desc = "引用奖励物品。"
```

规则：

- 外键是否可为空由字段 `required` 决定。
- required foreign key 缺值时报 required error。
- optional foreign key 全空时跳过 FK resolution。
- composite foreign key 全空可跳过，部分为空应报 partial key error。
- foreign key target 必须是 unique index 或 primary index。

## 4.13 Multi-source 与 Override

> 实现状态：Partial。当前版本已支持 fragment variant source mode 这一受限多源能力：polymorphic CollectionTable 可声明多个 CSV `sources`，但至少一个 source 必须声明 `fragments`。通用 split merge、ObjectTable 多源合并、`override_sources` 与非 `csv` provider 仍会在 Meta 校验或 DataLoadPlan 构造阶段报错，避免静默忽略。

本节的 multi-source split merge / override 是通用多源能力，仍为 Reserved。下一节的 polymorphic variant source mode 是多源能力的受限子集：仅允许 `polymorphic = true` 的 CollectionTable 声明多个 variant sources，且每个 source 追加完整 records，不提供通用 split merge、ObjectTable 多源合并或 override 语义。

Base `sources` 用于 split merge：

- CollectionTable：多个 source 的 records 追加为一个集合。
- ObjectTable：多个 source 提供同一对象的不同字段。
- 唯一索引冲突在最终组合视图中检查。

`override_sources` 用于覆盖：

- ObjectTable：按 field path 覆盖。
- CollectionTable：按 primary index 匹配整条 record 并替换。
- 按列表顺序应用，不使用 priority。

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
  { provider = "csv", path = "items/weapons.csv", fragments = ["stats", "skills"] },
  { provider = "csv", path = "items/equipment.csv", fragments = ["stats"] },
  { provider = "csv", path = "items/usable.csv", fragments = ["usable"] },
]
```

fragment 本身建议定义为 table 内的命名字段组，复用 `FieldMeta` 的字段属性：

```toml
[[tables.fragments]]
name = "stats"
desc = "战斗属性 fragment。"

[[tables.fragments.fields]]
name = "attack"
type = "int"
default = 0

[[tables.fragments.fields]]
name = "defense"
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

目标 Rust schema：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FragmentMeta {
    pub name: String,
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
- CSV header 中 fragment field 使用 `fragment.field` 形式，例如 `stats.attack` / `stats.defense`；common field 仍使用裸 field name，例如 `id` / `name`。
- Phase 1 不支持多级 fragment path；`stats.attack` 中 `stats` 必须是当前 table 的 fragment name，`attack` 必须是该 fragment 内的直接 field name。

### 4.14.3 与 fragment join mode 的边界

第一阶段明确不支持 fragment join mode：

```text
items_base.csv      提供 id/name/kind
items_weapon.csv    只提供 id/attack/skill
items_usable.csv    只提供 id/cooldown
```

上述多个 source 按同一 ID 拼成一条 record 的能力不在本阶段范围内。variant source mode 要求每个 source 自己提供一条完整 record 所需的 common fields 和声明 fragments 的字段。这能降低 merge、诊断和代码生成复杂度，也更适合先服务“不同物品类型拆不同 CSV 文件”的需求。
