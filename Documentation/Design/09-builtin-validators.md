# 09. 游戏常用内置 Validators 设计

Related docs：[`05-data-validation.md`](05-data-validation.md)、[`04-meta-model.md`](04-meta-model.md)、[`../Implementation/06-validation-module-detail-design.md`](../Implementation/06-validation-module-detail-design.md)。

本文聚焦 P1-2 阶段扩展的**游戏常用内置 validators**。目标不是替代 `05-data-validation.md` 的通用校验语义，而是在 validator registry 与既有 `range` validator 基础上，为 ConfigForge 提供一组更贴近游戏配置场景的内置规则，并明确它们的配置模型、作用域、目标视图语义、诊断口径和落地状态。

## 9.1 当前状态

当前代码与设计基线：

- validator registry、`ValidatorMeta`、field/table validator TOML 承载方式已实现。
- registry 已注册游戏常用内置 validators：field-scope `range`、`regex`、`string_length`；record-scope `required_when`、`compare`；table-scope `sum`、`monotonic`。
- required / type / enum / unique / foreign key integrity 仍主要由 validation 主流程或专门模块承担，不是完整的 registry built-in 集合。
- 因此 validator 能力的当前状态是 **Implemented MVP**：显式 built-in validator 已覆盖 P1-2 高频规则，基础 required/type/enum/unique/FK 仍保留在原有校验路径中。

P1-2 已把最常见、最稳定、最不依赖表达式语言的游戏规则沉淀为内置 validators，覆盖：

1. `range` 增强；
2. `regex`；
3. `string_length`；
4. `sum`；
5. `monotonic`；
6. `required_when`；
7. `compare`。

## 9.2 设计目标

### 9.2.1 要解决的问题

这些 validator 主要服务游戏配置中的高频场景：

- 资源 key、文本 key、逻辑 ID、命名规范；
- UI 文本、昵称、描述、道具名长度；
- 抽卡池、掉落池、随机事件、AI 权重总和；
- 等级经验、成长曲线、关卡难度、技能升级消耗；
- 按类型驱动的条件字段必填；
- 跨字段上下界、时间窗口、等级区间等比较关系。

### 9.2.2 非目标

P1-2 不做以下事情：

- 不引入通用表达式语言。
- 不支持任意 SQL/脚本式 group/aggregate/filter。
- 不支持跨表聚合 validator；跨表规则仍优先通过 FK 或后续 dataset-level validator 设计解决。
- 不支持 fragment field validator / struct child validator 的完整推广；它们仍遵守当前 nested validator 边界。
- 不把外键关系重新建模为普通 validator。

## 9.3 总体设计原则

### 9.3.1 继续使用 `kind`，不切回 `type`

当前 canonical TOML 形态已经是：

```toml
[[tables.fields.validators]]
kind = "range"
```

因此 P1-2 文档中的 canonical 配置统一继续使用 `kind`，不改为 `type`。如果后续为了兼容用户习惯考虑给 `type` 增加 alias，应作为**解析兼容层**讨论，而不是改变设计文档主口径。

### 9.3.2 默认仍运行在当前 target ExportView 上

所有新增 built-in validators 默认遵守当前 target ExportView：

- 被 table `export_when` 排除的表不参与；
- 被 `row_export` 排除的行不参与；
- field-scope validator 默认只检查当前 target 可见字段；
- record/table validator 默认只遍历当前 target 可见 rows。

但对 record/table validator，需要额外允许**显式引用 support fields**：

- 某些 validator 虽运行在 visible rows 上，但可以显式读取同一 row 中被当前 target 隐藏的辅助字段；
- 这些辅助字段不会因为被 validator 读取而自动进入 runtime export；
- 这种能力只对显式列在 validator 参数里的字段生效，不能隐式读取所有 hidden fields。

典型例子：

- `required_when` 用隐藏的 `type` / `category` 字段判断另一个字段是否必填；
- `sum` 使用隐藏的 `pool_id` 分组；
- `monotonic` 使用隐藏的 `level` 做排序。

### 9.3.3 保持 typed config，而不是把逻辑塞进 `params`

对用户来说仍写 flatten TOML；但实现层每个 validator factory 必须把参数编译成 typed config：

- compile 阶段检查必填参数；
- compile 阶段检查参数类型；
- compile 阶段拒绝 unknown parameters；
- compile 阶段解析字段引用、group_by、order_by、条件表达式；
- run 阶段不再重复解析 TOML。

### 9.3.4 优先做“少而硬”的结构化能力

P1-2 不追求一下子覆盖所有业务规则，而是优先提供：

- 语义稳定；
- 需求高频；
- 可清晰诊断；
- 不需要脚本解释器；
- 可在 code review 中看懂。

## 9.4 推荐分阶段落地顺序

### P1-2a：低风险 field validators

1. `regex`
2. `string_length`
3. `range` 增强

特点：

- 不需要 group/order；
- 只作用单字段；
- 最容易测试和推广；
- 与现有 registry 结构最一致。

### P1-2b：record validators

4. `required_when`
5. `compare`

特点：

- 需要 row 内多字段读取；
- 要明确 support field / hidden field 语义；
- 但还不涉及表级排序或聚合。

### P1-2c：table validators

6. `sum`
7. `monotonic`

特点：

- 需要 group_by / order_by；
- 需要更强的 deterministic 排序与分组定义；
- 更容易引入“歧义配置”与性能问题，因此放在最后。

## 9.5 通用配置约定

所有新增 built-in validators 都复用现有公共字段：

```toml
[[tables.fields.validators]]
kind = "regex"
name = "item_id_snake_case"
severity = "error"
message = "item id must be lowercase snake_case"
export_when = { any = ["client"] }
pattern = "^[a-z0-9_]+$"
```

公共约定：

- `kind`：validator 类型。
- `name`：可选稳定标识，推荐用于 group/table validator，便于诊断和测试断言。
- `severity`：`error` / `warning`。
- `message`：用户自定义诊断消息；未提供时使用内置默认文案。
- `export_when`：控制 validator 在哪些 target tags 下执行。

专属参数约定：

- field validator 放在 `[[tables.fields.validators]]`；
- record/table validator 放在 `[[tables.validators]]`；
- 参数名优先使用直观短名，例如 `field`、`group_by`、`order_by`、`equals`；
- 同一 validator 内，字段引用统一使用 field path 字符串，不引入额外路径语言。

## 9.6 共享条件模型：`when`

`required_when` 不是唯一需要条件表达式的 validator。未来很多 record-level 规则也会复用条件判断，因此建议在 P1-2 同时收敛一个**受限的行内条件模型**，供 `required_when` 和后续 built-in 共享。

### 9.6.1 叶子条件

首版建议支持：

```toml
when = { field = "type", equals = "active" }
```

可选叶子操作：

| 字段 | 含义 |
| --- | --- |
| `field` | 参与判断的 field path。 |
| `equals` | 等于某个字面值。 |
| `not_equals` | 不等于某个字面值。 |
| `in` | 值在给定列表中。 |
| `not_in` | 值不在给定列表中。 |
| `is_empty` | 字段为空或缺失。 |
| `not_empty` | 字段非空。 |

约束：

- 同一个叶子条件只允许声明一种比较操作；
- `equals` / `not_equals` / `in` / `not_in` 的字面值类型必须与字段类型兼容；
- `field` 可以引用 support field；
- 不支持任意函数调用。

### 9.6.2 组合条件

建议支持轻量布尔组合：

```toml
when = {
  all = [
    { field = "type", equals = "active" },
    { field = "enabled", equals = true }
  ]
}
```

支持字段：

- `all`：全部满足；
- `any`：任意满足；
- `none`：全部不满足。

组合规则：

- 叶子条件和组合条件不能在同一层混用；
- 允许递归嵌套，但不引入脚本语言；
- compile 阶段要检测空组合、未知字段、类型不兼容。

## 9.7 `regex` validator

### 9.7.1 定位

- **scope**：`Field`
- **典型用途**：资源 key、文本 key、逻辑 ID、配置名称、外部系统对接 key。

### 9.7.2 Canonical 配置

```toml
[[tables.fields.validators]]
kind = "regex"
pattern = "^[a-z0-9_]+$"
```

可带自定义消息：

```toml
[[tables.fields.validators]]
kind = "regex"
pattern = "^[A-Z][A-Za-z0-9_]*$"
message = "skill class name must use UpperCamelCase"
```

### 9.7.3 参数

| 参数 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `pattern` | string | 必填 | Rust regex 语法。 |
| `case_sensitive` | bool | `true` | 是否区分大小写。 |
| `allow_empty` | bool | 跟随字段 optional 语义 | 空值是否跳过。 |
| `match_mode` | `full` / `search` | `full` | `full` 要求整个字符串匹配；`search` 只要求存在匹配片段。 |

### 9.7.4 支持类型

首版建议支持：

- `string`
- `text`
- `asset`
- enum 的 raw string 表示

不支持：

- `int` / `float` / `bool`
- `struct`
- `list<T>`

### 9.7.5 执行规则

- 空值且允许跳过时，不报错；
- `full` 模式下，执行语义等价于“整串匹配”；
- `search` 模式用于检查“必须包含某段模式”的场景，但不应作为默认；
- compile 阶段必须预编译 regex；
- invalid regex 是配置错误，不是数据错误。

### 9.7.6 诊断

推荐 code：`validator.regex_mismatch`

诊断应包含：

- table
- field_path
- record_key
- validator name/kind
- pattern
- value

## 9.8 `string_length` validator

### 9.8.1 定位

- **scope**：`Field`
- **典型用途**：UI 文本、昵称、道具名、描述字段长度限制。

### 9.8.2 Canonical 配置

```toml
[[tables.fields.validators]]
kind = "string_length"
max = 32
```

或：

```toml
[[tables.fields.validators]]
kind = "string_length"
min = 1
max = 64
unit = "unicode_scalar"
```

### 9.8.3 参数

| 参数 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `min` | integer | 无 | 最小长度。 |
| `max` | integer | 无 | 最大长度。 |
| `exact` | integer | 无 | 固定长度；与 `min` / `max` 互斥。 |
| `allow_empty` | bool | 跟随字段 optional 语义 | 空值是否跳过。 |
| `unit` | `unicode_scalar` / `utf8_bytes` | `unicode_scalar` | 长度计量单位。 |
| `trim_before_count` | bool | `false` | 是否在计数前做首尾空白裁剪。 |

### 9.8.4 长度单位选择

首版建议：

- 默认用 `unicode_scalar`，即 Unicode scalar value 数量；
- 不默认用 grapheme cluster，因为它需要更重的文本语义依赖，且跨语言运行时一致性更难保障；
- 如确实有包体、协议或存档长度限制，可用 `utf8_bytes`。

这意味着：

- `string_length` 主要用于**配置层稳定校验**；
- 如果项目需要严格面向“用户感知字符数”的 UI 规则，可在后续扩展 `grapheme_cluster`，但不作为 P1-2 必备项。

### 9.8.5 支持类型

首版建议仅支持：

- `string`
- `text`
- `asset`

### 9.8.6 诊断

推荐 code：

- `validator.string_too_short`
- `validator.string_too_long`
- `validator.string_length_mismatch`

## 9.9 `sum` validator

### 9.9.1 定位

- **scope**：`Table`
- **典型用途**：抽卡池、掉落池、随机事件、AI 权重、概率分布。

### 9.9.2 Canonical 配置

```toml
[[tables.validators]]
kind = "sum"
field = "weight"
group_by = ["pool_id"]
equals = 10000
```

也允许范围式：

```toml
[[tables.validators]]
kind = "sum"
field = "probability"
group_by = ["pool_id", "region"]
min = 0.999
max = 1.001
epsilon = 0.000001
```

### 9.9.3 参数

| 参数 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `field` | string | 必填 | 参与求和的 numeric field。 |
| `group_by` | string array | `[]` | 分组键；空数组表示整表一组。 |
| `equals` | int/float | 无 | 要求 sum 等于某个值。 |
| `min` | int/float | 无 | 要求 sum 不小于该值。 |
| `max` | int/float | 无 | 要求 sum 不大于该值。 |
| `epsilon` | float | `0.0` | float 比较容差。 |
| `skip_empty` | bool | `false` | value 为空时是否跳过该 row。 |

约束：

- `equals` 与 `min`/`max` 可二选一；
- `equals` 与 `min`/`max` 同时存在时，`equals` 优先视为非法配置，避免歧义；
- `group_by` 可以引用 support fields。

### 9.9.4 支持类型

- `field` 必须是 `int` 或 `float`；
- `group_by` 字段必须是可稳定转字符串 key 的标量类型；
- 不支持 `struct` / `list<T>` 分组键；
- 不支持多字段求和表达式，如 `weight * rate`。

### 9.9.5 执行规则

- 只遍历当前 target visible rows；
- 先按 `group_by` 构造稳定 group key；
- 再把 `field` 的 normalized numeric value 累加；
- `skip_empty = false` 时，空值由 sum validator 直接报错，不静默按 0 处理；
- float 比较使用 `epsilon`；int 比较必须精确相等；
- diagnostics 必须包含 group key，便于定位某个 pool。

### 9.9.6 为什么不命名为 `weight_sum`

首版更建议统一命名为 `sum`：

- 规则本质是聚合求和，不只适用于权重；
- 抽卡概率、AI 分数、经济参数都可以复用；
- `weight_sum` 可以保留为未来 alias，但 canonical name 建议仍是 `sum`。

### 9.9.7 诊断

推荐 code：

- `validator.sum_mismatch`
- `validator.sum_too_small`
- `validator.sum_too_large`

## 9.10 `monotonic` validator

### 9.10.1 定位

- **scope**：`Table`
- **典型用途**：等级经验、养成曲线、关卡难度、技能升级消耗。

### 9.10.2 Canonical 配置

```toml
[[tables.validators]]
kind = "monotonic"
field = "exp"
group_by = ["class_id"]
order_by = "level"
direction = "increasing"
strict = true
```

### 9.10.3 参数

| 参数 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `field` | string | 必填 | 被检查的 numeric/comparable 字段。 |
| `group_by` | string array | `[]` | 分组键；空数组表示整表。 |
| `order_by` | string | 必填 | 组内排序字段。 |
| `direction` | `increasing` / `decreasing` | 必填 | 单调方向。 |
| `strict` | bool | `true` | 是否要求严格单调。 |
| `skip_equal_order_key` | bool | `false` | 是否允许组内出现相同 `order_by` 值并跳过后续比较。 |

### 9.10.4 支持类型

- `field`：首版建议支持 `int`、`float`、`string`、`text`、`asset`、enum；
- `order_by`：首版建议支持能稳定排序的标量类型；
- 最常见场景仍是 `order_by = int`、`field = int/float`。

### 9.10.5 执行规则

- 先按 `group_by` 分组；
- 再按 `order_by` 做稳定排序；
- 若同组中出现重复 `order_by` 且 `skip_equal_order_key = false`，应报配置/数据错误，因为单调顺序不唯一；
- `strict = true` 时：
  - `increasing` 要求后值 `>` 前值；
  - `decreasing` 要求后值 `<` 前值；
- `strict = false` 时：
  - `increasing` 要求后值 `>=` 前值；
  - `decreasing` 要求后值 `<=` 前值。

### 9.10.6 为什么默认 `strict = true`

游戏成长曲线的大多数实际需求是“不能倒退，且通常必须递增”。默认 `strict = true` 更能尽早发现配置异常；如果业务确实允许平级平台期，应显式写 `strict = false`。

### 9.10.7 诊断

推荐 code：

- `validator.monotonic_violation`
- `validator.monotonic_duplicate_order_key`

诊断应包含：

- group key
- 当前 row 与前一 row 的 order/value
- direction
- strict

## 9.11 `required_when` validator

### 9.11.1 定位

- **scope**：`Record`
- **典型用途**：
  - `type = active` 时必须填 `skill_id`；
  - `type = shop_item` 时必须填 `price`；
  - `type = quest` 时必须填 `unlock_condition`。

### 9.11.2 Canonical 配置

```toml
[[tables.validators]]
kind = "required_when"
field = "skill_id"
when = { field = "type", equals = "active" }
```

复合条件：

```toml
[[tables.validators]]
kind = "required_when"
field = "price"
when = {
  all = [
    { field = "type", equals = "shop_item" },
    { field = "currency_type", not_empty = true }
  ]
}
```

### 9.11.3 参数

| 参数 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `field` | string | 必填 | 条件满足时必须填写的字段。 |
| `when` | condition object | 必填 | 条件表达式。 |
| `treat_empty_string_as_missing` | bool | `true` | 空字符串是否视为缺失。 |

### 9.11.4 执行规则

- 对当前 target visible rows 逐行执行；
- `when` 可读取 support fields；
- 若条件不满足，validator 不做任何要求；
- 若条件满足，则 `field` 必须存在且非空；
- 若 `field` 本身有 type/enum/range 等错误，这些错误仍应独立报告，不被 `required_when` 吞掉；
- 如果 `field` 在当前 target 被隐藏，但 validator 在该 target 仍 active，应视为配置错误，建议用户用 `export_when` 对齐 validator 与 field 可见性。

### 9.11.5 诊断

推荐 code：`validator.required_when_missing`

## 9.12 `compare` validator

### 9.12.1 定位

- **scope**：`Record`
- **典型用途**：
  - `min_level <= max_level`
  - `start_time < end_time`
  - `unlock_level >= recommend_level`

### 9.12.2 Canonical 配置

```toml
[[tables.validators]]
kind = "compare"
left = "min_level"
op = "<="
right = "max_level"
```

### 9.12.3 参数

| 参数 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `left` | string | 必填 | 左字段。 |
| `op` | string | 必填 | `==` / `!=` / `<` / `<=` / `>` / `>=`。 |
| `right` | string | 必填 | 右字段。 |
| `skip_if_either_empty` | bool | `true` | 任一侧为空时是否跳过。 |

### 9.12.4 支持类型

首版建议支持：

- `int`
- `float`
- `string`
- `text`
- `asset`
- enum

要求：

- 两侧必须类型兼容；
- `<` / `<=` / `>` / `>=` 只允许用于可稳定排序的类型；
- 若后续引入 datetime/builtin date 类型，可自然接入 compare。

### 9.12.5 执行规则

- 按 row 执行；
- 如果 `skip_if_either_empty = true` 且任一侧为空，则跳过；
- 如果 `skip_if_either_empty = false`，任一侧为空时报错；
- compare 不负责做复杂类型转换；它应复用字段本身的 normalized typed value。

### 9.12.6 诊断

推荐 code：

- `validator.compare_failed`
- `validator.compare_operand_missing`

## 9.13 `range` 的增强建议

`range` 已经存在，因此 P1-2 不需要重新设计其基本语义，但建议顺手明确以下增强方向：

1. 保持当前 `min` / `max` / `exclusive_min` / `exclusive_max` / `allow_empty` 设计。
2. 允许更清晰的默认消息模板，便于 QA 与用户定位。
3. 统一与其他 built-in validator 的 compile error 风格：
   - unknown param
   - invalid bound type
   - `min > max`
   - scope mismatch
4. 后续如确有强需求，再讨论：
   - `multiple_of`
   - `non_zero`
   - 百分比/概率专用 alias

P1-2 不建议把这些增强拆成新的 validator 名称，避免用户面对多个高度重叠的 numeric validators。

## 9.14 诊断与可测试性要求

新增 built-in validators 必须满足：

### 9.14.1 稳定诊断 code

建议统一前缀：

- `validator.regex_*`
- `validator.string_*`
- `validator.sum_*`
- `validator.monotonic_*`
- `validator.required_when_*`
- `validator.compare_*`

### 9.14.2 诊断内容

至少包含：

- `table`
- `field_path` 或相关 fields
- `record_key`（field/record validator）
- `group_key`（group/table validator）
- `validator kind`
- `validator name`（若提供）
- `target`
- 相关 value / expected / actual

### 9.14.3 确定性

- group_by 输出顺序必须稳定；
- monotonic 的组内排序必须稳定；
- 并行执行不能改变 report 顺序；
- diagnostics 中的“前一条记录”必须按稳定排序语义确定，而不是按线程完成顺序决定。

## 9.15 测试关注点

### 9.15.1 `regex`

- pattern 合法/非法；
- full/search 模式差异；
- empty + allow_empty；
- string/text/asset/enum 支持与非支持类型拒绝。

### 9.15.2 `string_length`

- `min` / `max` / `exact`；
- `unicode_scalar` 与 `utf8_bytes`；
- `trim_before_count`；
- 中文、emoji、组合字符边界样例。

### 9.15.3 `sum`

- 空 group_by 与多字段 group_by；
- int 精确比较；
- float + epsilon；
- hidden support field 分组；
- row_export 过滤后分组结果变化。

### 9.15.4 `monotonic`

- increasing/decreasing；
- strict / non-strict；
- duplicate order key；
- target 过滤后序列变化。

### 9.15.5 `required_when`

- 简单叶子条件；
- `all` / `any` / `none`；
- hidden support field；
- target 中 field hidden 但 validator active 的配置错误。

### 9.15.6 `compare`

- 数值比较；
- 字符串/enum 比较；
- 空值处理；
- 类型不兼容错误。

## 9.16 需要讨论确认的开放问题

建议在进入实现前先确认以下几点：

1. **是否为 `ValidatorMeta.kind` 提供 `type` TOML alias？**
   - 当前文档建议：canonical 仍用 `kind`，alias 可选。

2. **`string_length` 默认长度单位是否用 `unicode_scalar`？**
   - 当前文档建议：是；避免引入 grapheme 级复杂度。

3. **`monotonic.strict` 默认值是否为 `true`？**
   - 当前文档建议：是；更符合成长曲线常见需求。

4. **`sum` 是否只提供通用名 `sum`，不再单独引入 `weight_sum`？**
   - 当前文档建议：是；必要时可加 alias。

5. **record/table validator 是否允许显式读取 hidden support fields？**
   - 当前文档建议：允许，但只限显式引用字段，且不改变 export 行为。

6. **`compare` 首版是否只支持 field-vs-field，不支持 field-vs-literal？**
   - 当前文档建议：是；保持配置简单，先覆盖最常见区间关系。

## 9.17 推荐讨论结论模板

为方便从设计讨论进入实现，建议按以下顺序确认：

1. 先确认 **P1-2a / P1-2b / P1-2c** 的实现顺序；
2. 再确认共享 `when` 条件模型是否接受；
3. 再确认 hidden support field 是否允许；
4. 最后再确认各 validator 的默认值和别名策略。

这样可以避免在实现前把所有边角参数一次性定死。
