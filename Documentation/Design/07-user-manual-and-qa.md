# 07. 用户手册与 QA 输入

## 7.1 最小用户工作流

### 1. 编写数据表

`Data/items.csv`：

```csv
id,name,icon,export_tags,exclude_tags
1001,Iron Sword,assets/icons/iron_sword.png,,
9001,Debug Sword,assets/icons/debug_sword.png,debug,shipping
```

### 2. 编写 Meta

`Meta/items.toml`：

```toml
[[tables]]
name = "items"
kind = "collection"
desc = "所有可获得物品的基础配置。"
source = "items.csv"
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

[[tables.fields]]
name = "icon"
type = "asset"
asset_kind = "texture"
required = true
desc = "物品图标资源路径。"

[[tables.fields]]
name = "export_tags"
type = "string"
export = false
desc = "行级导出要求的 target tags，逗号分隔；空值表示所有 target。"

[[tables.fields]]
name = "exclude_tags"
type = "string"
export = false
desc = "命中这些 target tags 时排除该行，逗号分隔。"

[tables.row_export]
required_tags_field = "export_tags"
excluded_tags_field = "exclude_tags"
match = "any"

[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
primary = true

```

### 3. 编写 Project Config

`ConfigForge.toml`：

```toml
imports = ["Meta/items.toml"]

[project]
name = "sample"

[[targets]]
name = "client_json_shipping"
tags = ["client", "shipping"]

[[targets.data]]
input = "Data"
output = "Generated/Data"
format = "json"

[[targets.code]]
language = "typescript"
output = "Generated/Client"

```

### 4. 校验

```bash
config-forge --project ConfigForge.toml --target client_json_shipping
```

### 5. 导出

```bash
config-forge --project ConfigForge.toml --target client_json_shipping
```

预期：

- `Debug Sword` 因 `exclude_tags = "shipping"` 被 shipping target 排除。
- `export_tags` / `exclude_tags` 字段不进入 runtime data。
- debug sword 的 icon 不进入 asset manifest。

## 7.2 用户常见任务

### 添加新字段

1. 在 source 表格添加列。
2. 在 Meta 中添加 field。
3. 为 field 添加 `desc`，说明字段业务含义和使用限制。
4. 如旧数据未填写，配置 `default` 或保持 `required = false`。
5. 运行 validate。

### 添加描述

1. 在 enum、enum value、struct、struct field、table、field、fragment、index 等定义上添加 `desc`。
2. 生成目标语言代码或辅助文档，检查合并后的 MetaModel 描述是否被保留。
3. 运行 `export` 检查生成代码中的注释是否符合目标语言风格。

### 添加索引

1. 在 CollectionTable 添加 `[[tables.indexes]]`。
2. 如果作为主索引，声明 `primary = true` 或 table-level `primary_index`。
3. 运行 validate 检查重复 key。

### 添加 override source

1. 在表中添加 `override_sources`。
2. CollectionTable 必须有 primary index。
3. override source 行必须能提供完整 primary key。
4. 后声明的 override source 覆盖先声明的结果。

### 添加 shipping 过滤

1. 在 collection 表中添加 `export_tags` / `exclude_tags` 等行级 tag 字段，并设置 `export = false`。
2. 添加 `[tables.row_export]`，通过 `required_tags_field` 和 `excluded_tags_field` 引用这些字段。
3. 在 target 中配置 `tags = ["client", "shipping"]` 等上下文。
4. 通过不同 target tags 验证行级过滤效果。

## 7.3 QA 测试维度

### 配置解析

- Project Config 不存在。
- Project Config 中 meta 路径不存在。
- import 路径不存在。
- import 循环。
- TOML / XML 语法错误。
- TOML 与 XML equivalent semantics。

### Source Provider

- CSV 正常读取。
- CSV 文件不存在。
- CSV header 缺失。
- CSV 重复 header。
- 空行、空列和未映射注释列不会进入 DataSet。
- XLSX 多 sheet 未指定 sheet。
- 远程 provider 缺少 auth_ref。

### Field Type

- int/float/bool/string 正常解析。
- required 缺失报错。
- default 补齐成功。
- default 类型错误报错。
- color channel byte / normalized 成功。
- color_channel = auto 报 Meta error。
- vector3 正常解析。
- datetime 缺 timezone policy 的错误场景。

### Multi-source

- 多 base sources 正常合并。
- base sources unique index 冲突。
- ObjectTable override field path 成功。
- CollectionTable override 整行替换成功。
- override source 找不到 base row 报错。
- 后声明 override source 覆盖先声明结果。

### Index / Foreign Key

- unique index 无重复。
- unique index 重复报错。
- `primary_index` 指向不存在 index 报错。
- `primary = true` 多个 index 报错。
- FK 指向存在记录成功。
- required FK 缺失报 required error。
- optional FK 空值跳过。
- composite FK 部分为空报错。

### Export Filtering

- shipping tag 排除 `exclude_tags` 包含 `shipping` 的行。
- debug target 导出 `export_tags` 包含 `debug` 的行。
- client/server target 分别只导出匹配对应 tag 的表、字段和行。
- `export = false` 字段不进入 runtime data，但可以被 `row_export` 显式引用。
- `row_export` 引用不存在字段时报错。
- 过滤后 collection validator 只检查剩余记录。

### Special Exporter

- POT 只包含 ExportView 中 text。
- Asset manifest 只包含 ExportView 中 asset。
- 被 shipping filter 排除的记录不进入 POT / asset manifest。
- POT source reference 使用 source 表名和行列号。
- text 字段的额外注释列/行能进入 translator comments。
- special exporter 输出路径冲突报错。

### Artifact Writer

- `temp-commit` 模式下先写临时文件再提交；写临时文件失败不提交最终产物，提交阶段失败可能保留已提交文件。
- `direct` 模式下按项目配置直接写目标路径。
- 路径冲突在任何写入前报错。
- 并行 artifact 生成不影响 manifest 稳定排序。

### CLI

- `validate` 成功退出码 0。
- `validate` 有 error 退出码 1。
- CLI 参数错误退出码 2。
- `--diagnostic-format json` 输出可解析 JSON。
- `--target` 多 target 选择。
- `--tag` 与 target 默认 tags 合并为 effective target tags，并驱动 table/field/index `export_when`、`row_export`、`override_sources[].export_when`、runtime export 和 codegen。
- 对 S1 赛季覆盖用例，未传 `--tag s1` 时 Oak 材料保持 base 售价 3；传入 `--tag s1` 时 `materials_s1.csv` 覆盖 Oak row，售价变为 5。

### Desc / Code Comments

- table `desc` 出现在生成的表类型或访问模块注释中。
- field `desc` 出现在生成字段、属性或 getter 注释中。
- enum `desc` 出现在生成 enum 类型注释中。
- enum value `desc` 出现在生成 enum member 注释中。
- named struct / struct field `desc` 出现在生成结构体和成员注释中。
- 多行 `desc` 被正确拆分和转义。
- 包含目标语言注释结束符的 `desc` 不应生成非法代码。
- 修改 `desc` 不应改变 runtime data 内容，只影响生成代码注释或辅助文档。

## 7.4 QA 断言建议

QA 测试不应只断言文本 message，应优先断言：

- exit code；
- diagnostic code；
- severity；
- table；
- field_path；
- record_key；
- artifact path；
- output manifest 内容；
- runtime data 中是否包含/排除某 record/field。

## 7.5 用户可见错误质量要求

错误必须尽量回答：

1. 哪里错了？表、字段、行列、source 文件。
2. 为什么错？类型、required、重复、引用不存在等。
3. 应该怎么修？提供 hint。
4. 是否还有关联位置？例如重复 key 的另一行。

示例：

```text
error[index.unique_duplicate]: duplicate key "1001" for unique index items.by_id
  first: Data/items_base.csv:12:1
  second: Data/items_event.csv:8:1
  hint: keep item IDs unique across all base sources, or move replacement rows to override_sources
```
