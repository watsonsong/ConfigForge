# 09i. Lua Embedded Data Generator

本文档描述当前内置的 Lua 导出支持。Lua 目标与 Rust / C++ / TypeScript 的 format-agnostic codegen 不同：Lua 产物把**数据与访问代码合并在同一组 `.lua` 文件**中，直接由 Lua VM `require` 加载，不再通过 JSON / MessagePack / `bin` runtime reader 间接读取。因此它必须同时依赖 target-aware Meta 与已归一化、已校验的数据视图。

设计定位：Lua 是 artifact generator requirements 与能力 facet 解耦的关键用例。它的 requirements 包含 Meta + Data，因此调度上走 data-backed 分支；但它的输出是 code-like `.lua` artifact，因此应该复用 code artifact 选项、命名、格式化和 writer helper。不要因为它需要 Data 就把 “是否为代码生成” 绑定到 MetaOnly 继承体系，也不要复制一套与 `CodegenOptions` 平行但语义重复的 Lua 专用基础选项。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — code artifact toolkit 总览和专题索引。
- [`08-exporter-module-detail-design.md`](08-exporter-module-detail-design.md) — data artifact toolkit 与 runtime data artifact 输出。
- [`09f-codegen-core-model-and-options.md`](09f-codegen-core-model-and-options.md) — 当前 format-agnostic codegen 输入输出模型。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、Lua table format 与 CLI 行为。
- [`01-module-outline.md`](01-module-outline.md) — Rust 模块边界与依赖方向。

## 9i.1 目标与非目标

### 目标

1. 为 Lua 运行时生成可直接 `require` 的配置模块，不要求业务侧再携带 JSON/MessagePack/bin reader。
2. 每张可见 table 生成一个独立 `.lua` 文件，便于按需加载与包内资源拆分。
3. 生成一个 root facade 文件，根据访问情况 lazy `require` 对应 table 文件，并可提供 `preload_all()`。
4. CollectionTable 的每一行默认以**数组**存储字段值，通过共享 metatable 支持 `row.field_name` 访问，降低字符串 key 带来的内存成本。
5. 生成 primary / unique / non-unique index lookup，索引值引用 row 数组对象，不复制 row 数据。
6. 产物稳定、可 diff，支持 target tags、row_export、field export_when、ObjectTable、struct/list、enum/text 等现有 ExportView 语义。

### 非目标

1. 不把 Lua 伪装成现有 `CodeGenerator` 的第四种 format-agnostic backend；Lua 产物依赖数据，执行边界不同。长期可通过统一 artifact generator + code artifact facet 复用 code options，而不是直接实现当前 MetaOnly 假设的 `CodeGenerator`。
2. 不生成 Lua runtime reader contract，不读取 JSON / MessagePack / `bin`。
3. 不为“超大 Lua 表”单独引入 out-of-core DataSet 或按 target 重读 source；当前产物写出仍应走 `ArtifactSink`。若未来需要降低 Lua runtime 侧单文件 parse / require 峰值，可评估 chunked files，而不是改变 ConfigForge 的 Meta/Data 全量内存模型。
4. 首版不实现增量 require 热更新、二进制 Lua bytecode 编译、LuaJIT FFI 或引擎私有 asset bundle 打包。

## 9i.2 模块归属与依赖边界

当前 Lua 支持实现为 `type = "lua_table"` / alias `lua` 的 MetaAndData concrete generator，并直接注册到 `GeneratorRegistry`，而不是普通 codegen backend：

```text
src/generator/lua/
  mod.rs             # LuaTableExporter / ArtifactGenerator glue
  options.rs         # lua_table task options
  naming.rs          # Lua module / identifier naming helper
  render.rs          # root / table Lua renderer
  tests.rs           # focused generator tests
```

原因：

- `generator` 中的 concrete data-backed generator 可以依赖 `meta`、`data`、`artifact` 和 `diagnostic`，适合消费 target-aware resolved data；
- 现有 `code_artifact` 文档明确约束 Rust/C++/TypeScript 这类 format-agnostic language generator 只依赖 Meta 和 target tags，不绑定任何 MetaAndData export task；
- Lua 的输出既是运行时数据，又包含访问 API，因此语义上更接近 MetaAndData export type，而不是纯代码 stub。

当前模块边界：`lua_table` 作为 `src/generator/lua/` 内的 concrete generator 实现，继续复用 aggregate artifact 的路径规划和 sink 写出能力；`GeneratorRegistry` 直接注册为 `requirements = Meta + Data` 的 build-facing `ArtifactGenerator`。Lua 基础代码文本选项复用 `code_artifact::CodeArtifactOptions`（file header、i18n、naming / formatting 等通用能力），但 data input 只通过 task generator contract 注入；不引入 `code_artifact -> data` 或 `code_artifact -> build` 依赖。

Build 编排层应按 generator requirements 分发：

```text
ProjectConfig::from_path
  -> MetaModel::link
  -> Target normalize
  -> DataLoadPlan / DataLoader / normalize / validation
  -> GeneratorRegistry dispatch(type = "lua_table")
  -> LuaTableGenerator(meta + export view + resolved data, code artifact options)
  -> ArtifactWriter
```

这保持现有单向依赖：concrete generator 组合 `data_artifact` / `code_artifact` toolkit，不需要 `code_artifact -> data`，也不需要让 `code_artifact` 反向依赖 `data_artifact`。

## 9i.3 配置模型

Lua 导出作为 MetaAndData export task 配置：

```toml
[[targets]]
name = "client_lua"
tags = ["client"]

[[targets.exports]]
name = "lua"
output = "Generated/LuaConfig"
type = "lua_table"
module_name = "game.config"
root_file = "init.lua"
table_file_naming = "snake_case"
row_layout = "array_with_metatable"
index_key = "nested_table"
lazy_require = true
```

字段建议：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `type` | 必填 | 规范名 `lua_table`；可接受 alias `lua`，诊断和文档统一使用 `lua_table`。 |
| `module_name` | `config` | Lua require 前缀，例如 `game.config`。root 文件与 table 文件内部使用该前缀拼接 require path。 |
| `root_file` | `init.lua` | root facade 文件名。若项目不使用目录模块，可设为 `config.lua`。 |
| `table_file_naming` | `snake_case` | table 文件名派生策略，默认使用 canonical table name 的 snake_case。 |
| `row_layout` | `array_with_metatable` | CollectionTable row 存储策略；首版仅实现该策略。 |
| `object_layout` | `keyed_table` | ObjectTable 输出策略；首版推荐 keyed table，后续可支持 array_with_metatable。 |
| `index_key` | `nested_table` | 复合 index key 表达方式；首版推荐 nested table，避免字符串拼接碰撞。 |
| `lazy_require` | `true` | root facade 是否按需 require table 文件。关闭时 root 文件 require 所有 table。 |
| `emit_schema` | `false` | 是否在 table 文件中输出 field/index schema debug 信息；默认关闭以节省包体。 |
| `readonly` | `true` | 是否为 module/table/row metatable 增加只读保护。首版可只保护 API table，不保护 row array。 |

`lua_table` 是 MetaAndData export type，因此会使用 project-level `input` 加载 source data；若工程没有 source data，不能生成 Lua data+code 产物。

## 9i.4 产物布局

给定 `module_name = "game.config"`、`output = "Generated/LuaConfig"`：

```text
Generated/LuaConfig/
  init.lua              # root facade: require("game.config.items") 等
  items.lua             # CollectionTable items 数据 + index + table API
  classes.lua
  class_levels.lua
  game_rules.lua        # ObjectTable 数据 + object API
```

每个 table 文件只返回该 table 的 repository/object，不主动 require 其他 table。跨表 relation helper 若首版实现，应由 root facade 或调用方传入 `data` facade，避免 table 文件之间形成 require 环。

root facade 示例：

```lua
-- Generated by ConfigForge. DO NOT EDIT.
local M = {}
local cache = {}

local function load_items()
  local value = cache.items
  if value == nil then
    value = require("game.config.items")
    cache.items = value
  end
  return value
end

function M.items()
  return load_items()
end

function M.preload_all()
  load_items()
  -- load other tables...
  return M
end

return M
```

可选地，root facade 可以提供 `__index`：

```lua
return setmetatable(M, {
  __index = function(_, key)
    if key == "items" then return load_items() end
  end,
})
```

但首版推荐显式函数 `config.items()`，避免 `config.items` 同时可能表示函数和 table 的歧义。

## 9i.5 CollectionTable 行布局

CollectionTable 每一行使用 numeric array 存储字段值：

```lua
local FIELD = {
  id = 1,
  name = 2,
  price = 3,
  stack = 4,
}

local ROW_METHODS = {}

local ROW_MT = {
  __index = function(row, key)
    local index = FIELD[key]
    if index ~= nil then
      return row[index]
    end
    return ROW_METHODS[key]
  end,
}

local rows = {
  setmetatable({ 2001, "Iron Ore", 5, { max_stack = 999 } }, ROW_MT),
  setmetatable({ 2002, "Iron Sword", 100, nil }, ROW_MT),
}
```

设计要点：

- `FIELD` 是生成期稳定字段序号，必须只包含 target-visible 字段。
- `row.name` 通过 `ROW_MT.__index` 映射到 `row[FIELD.name]`。
- `ROW_METHODS` 可承载 relation helper、text helper 或 debug helper，避免方法名与字段名混淆；字段名优先级高于方法名，冲突应在生成前诊断。
- optional 字段值为 `nil`；生成器不得依赖 `#row` 或 `ipairs(row)`，因为 Lua array 中间有 nil 时长度语义不稳定。
- 当 nil 出现在非尾部且后续字段有值时，生成器应使用显式 numeric key：`{ [1] = 1, [3] = "x" }`，保证字段位置稳定。
- struct 值默认输出 keyed table；struct 内字段数量很大时，后续可引入 struct-level array layout，但首版不做。

### 9i.5.1 字段访问与内存权衡

数组 row 的主要收益是避免每行重复存储字符串 key。每行仍需一个 table 和一个 metatable 指针；与 `{ id = 1, name = "..." }` 相比，字段越多、行数越大收益越明显。

首版接受 `__index` 函数调用带来的访问开销，因为配置读取通常以按 key 查询 + 少量字段访问为主。如需极致访问性能，业务可直接使用 `row[FIELD.name]`；可以通过 `emit_schema = true` 暴露 `Table.FIELD` 常量供热路径使用。

## 9i.6 ObjectTable 布局

ObjectTable 数据量通常小，首版使用 keyed table 提升可读性：

```lua
local object = {
  max_player_level = 60,
  starter_item_ids = { 2001, 2002 },
}

return object
```

如果 ObjectTable 字段数量很大，后续可让 `object_layout = "array_with_metatable"` 复用 CollectionTable row 布局。

ObjectTable 不生成 collection rows，不生成 index map；若 ObjectTable 字段声明 `linked_field`，helper 应作为 object method，接收 root facade 或目标 table：

```lua
function object:default_class(data)
  return data.classes().get_by_id(self.default_class_id)
end
```

## 9i.7 类型和值序列化

Lua 值序列化规则：

| ConfigForge 类型 | Lua 表达 |
| --- | --- |
| `int` / `long` 等整数 | Lua number；若需要 64-bit 精确性，后续通过 option 改为 string。 |
| `float` / `double` | Lua number，必须处理 NaN/Infinity 禁止或诊断。 |
| `bool` | `true` / `false`。 |
| `string` / `text` / enum value | Lua string，按 Lua 字符串转义规则输出。 |
| optional missing | `nil`。 |
| list | Lua array table；元素为 nil 不合法，需沿用现有 validation。 |
| struct | keyed Lua table；字段名使用 Lua 字段命名策略。 |
| fragment | keyed table，包含 discriminator 和 target-visible payload。 |

字符串转义必须覆盖：反斜杠、引号、换行、回车、tab、控制字符和非 UTF-8 输入。源数据已按 UTF-8 处理；Lua 产物按 UTF-8 输出。

## 9i.8 索引生成

每个 CollectionTable 输出一个 repository table：

```lua
local T = {}
T.rows = rows

local by_id = {}
for _, row in ipairs(rows) do
  by_id[row[FIELD.id]] = row
end

function T.get_by_id(id)
  return by_id[id]
end

return T
```

索引规则：

- primary / unique index：返回单个 row 或 `nil`。
- non-unique index：返回 row array；没有匹配时返回 shared empty table 或 `nil`，首版推荐 `nil`，避免调用方误改共享空数组。
- 复合 index 默认使用 nested table：`index[k1][k2] = row`，避免 `k1 .. "\0" .. k2` 类字符串 key 的转义和碰撞问题。
- index 构建在 module require 时执行。优点是 Lua 文件小、row 不复制；缺点是首次 require 有构建成本。后续可提供 `index_build = "literal"` 预生成 index literal。
- 生成器不应为 target-hidden 字段生成 index key；若 index 引用了隐藏字段，应沿用当前 codegen/export validation 的错误语义。

复合 unique 示例：

```lua
local by_class_level = {}
for _, row in ipairs(rows) do
  local k1 = row[FIELD.class_id]
  local k2 = row[FIELD.level]
  local level_1 = by_class_level[k1]
  if level_1 == nil then
    level_1 = {}
    by_class_level[k1] = level_1
  end
  level_1[k2] = row
end

function T.get_by_class_level(class_id, level)
  local level_1 = by_class_level[class_id]
  if level_1 == nil then return nil end
  return level_1[level]
end
```

## 9i.9 Lazy require 与关系 helper

root facade 负责 lazy require，table 文件不互相 require。relation helper 应避免造成循环依赖：

```lua
function ROW_METHODS.skill(row, data)
  local skill_id = row[FIELD.skill_id]
  if skill_id == nil then return nil end
  return data.skills().get_by_id(skill_id)
end
```

调用形态：

```lua
local config = require("game.config")
local item = config.items().get_by_id(2001)
local skill = item:skill(config)
```

首版也可以暂不生成 relation helper，只生成 index；但如果生成，必须与 Rust/C++/TypeScript 的 `linked_field` target-aware 语义一致。

## 9i.10 Text i18n helper

Lua table data 中 `text` 字段仍保存 raw text。可为 `text` 字段生成 helper method：

```lua
function ROW_METHODS.name_text(row, translator)
  local msgid = row[FIELD.name]
  if msgid == nil then return nil end
  local translated = translator("items.name", msgid)
  if translated == nil then return msgid end
  return translated
end
```

如果 Lua target 也支持 `global_function`，则 helper 可直接调用配置的全局函数。首版可以先只输出 raw text，后续补齐 helper；但 POT exporter 与 runtime data 不应因 Lua 产物而改变。

## 9i.11 诊断与路径冲突

Lua exporter 需要在写文件前诊断：

1. `module_name` / table file name / function name 非法；
2. table 文件名冲突，例如 `item-types` 与 `item_types` 归一化后同名；
3. 字段名与生成方法名冲突；
4. index helper 名冲突；
5. Lua 关键字冲突，例如 `end`、`local`、`function`；
6. 输出路径与其他 artifact 冲突；
7. 数据值无法安全表达为 Lua literal，例如非有限浮点数。

Lua 关键字冲突策略应复用 Project Config 的 `keyword_conflict` 概念；`lua_table` options 应提供同名字段，默认 `suffix_underscore`。

## 9i.12 当前实现状态

当前 `lua_table` / alias `lua` 已作为内置 aggregate / embedded data generator 落地，主要实现点包括：

1. **配置与注册**
   - 已迁入 `src/generator/lua/`，并在 `GeneratorRegistry` 注册 `lua_table` / alias `lua`。
   - 已定义 `LuaTableOptions`，从统一 export task 的 flattened options 解析配置。
2. **命名与符号检查**
   - 在 `src/generator/lua/` 内部实现 Lua naming / keyword helper；generator 只复用 code artifact options / writer helper，不依赖 language codegen backend。
   - 检查 table file、field、index helper、method helper 冲突。
3. **Lua literal writer**
   - 实现字符串、number、bool、nil、list、struct、fragment 的稳定渲染。
   - 单元测试覆盖转义、nil hole、nested struct/list。
4. **table file renderer**
   - CollectionTable：rows array + `FIELD` + `ROW_MT` + index maps + repository API。
   - ObjectTable：keyed object + optional helper。
5. **root facade renderer**
   - 生成 `init.lua` / root file，按需 require table modules。
   - 支持 `preload_all()`。
6. **Build / artifact 集成**
   - 复用当前 target branch executor 中共享的加载、归一化、校验结果。
   - 产物走 Artifact Writer，参与路径冲突检查。
7. **测试**
   - 单元测试覆盖 options parse、literal writer、symbol conflict、index renderer。
   - 集成测试使用标准 fixture 生成 Lua，并在系统 `lua` / `lua5.4` / `luajit` 存在时执行 require、index lookup、field access。

## 9i.13 测试策略

最小集成测试脚本：

```lua
package.path = generated_dir .. "/?.lua;" .. generated_dir .. "/?/init.lua;" .. package.path
local config = require("game.config")
local item = config.items().get_by_id(2001)
assert(item.id == 2001)
assert(item.name == "Iron Ore")
assert(item.price == 5)
local rules = config.game_rules()
assert(rules.max_player_level == 60)
```

测试查找 Lua 解释器顺序：`lua5.4`、`lua5.3`、`lua`、`luajit`。若本机不存在解释器，集成测试应跳过 runtime 执行，但仍检查生成文件内容。

必须覆盖的 fixture：

- CollectionTable primary index lookup；
- unique composite index lookup；
- non-unique index lookup；
- ObjectTable root access；
- row array metatable field access；
- optional nil hole；
- struct/list/fragment literal；
- target-hidden field 不进入 row field order；
- lazy require：访问一个 table 不应 require 全部 table（可通过 package.loaded 断言）。

## 9i.14 未来规划

- Lua 5.1 / 5.2 / 5.3 / 5.4 / LuaJIT 兼容矩阵；首版应避免使用 5.4-only 语法。
- 是否为大表生成 chunked files，例如 `items_001.lua` + `items.lua` 聚合，以降低单文件解析峰值。
- 是否支持 prebuilt index literal，换取更快 require 与更大文件。
- 是否生成 `FIELD` 公开常量，供热路径使用 `row[FIELD.name]` 绕开 metatable。
- 是否为 ObjectTable 也启用 array layout。
- Lua i18n helper 与 `global_function` 配置如何复用现有 `I18nOptions`。
