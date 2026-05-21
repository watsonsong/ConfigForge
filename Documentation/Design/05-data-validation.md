# 05. 数据视图与校验设计

## 5.1 数据视图

ConfigForge 使用两个主要数据视图。

### Full DataSet

Full DataSet 是所有 source 读取、解析、默认值应用、base merge、override 之后得到的完整逻辑数据集。

它保留：

- 所有记录；
- 所有字段；
- `export = false` 字段；
- 不匹配当前 target 的字段；
- source location；
- override 来源信息。

### ExportView

ExportView 是针对某个 target tags 派生的视图。

它会应用：

- table `export_when`；
- field `export_when`；
- collection `row_export`；
- fragment filters；
- `export = false` 常规运行时裁剪；
- target-specific type mapping。

常规 runtime exporter、special exporter 和 target-specific validation 默认读取 ExportView。

## 5.2 ExportView 过滤规则

表和字段使用 `export_when` 匹配 target tags；collection 行使用 `row_export` 匹配数据行中的 tags。

```toml
[[tables]]
name = "item"
export_when = { any = ["client", "server"] }

[tables.row_export]
required_tags_field = "export_tags"
excluded_tags_field = "exclude_tags"
match = "any"
```

规则：

- `export_when.any` 非空时至少命中一个 tag。
- `export_when.all` 非空时必须命中全部 tag。
- `export_when.none` 非空时不能命中任一 tag。
- `export = false` 优先级最高，永不进入 runtime ExportView。
- `row_export` 引用的字段必须存在；这些字段可以 `export = false`，仍可参与行级过滤。
- 被 `row_export` 排除的数据默认不参与该 target 的 runtime data、special exporter 和集合级校验。

### 5.2.1 Polymorphic Fragment ExportView

variant source mode 落地后，ExportView 需要同时保留 record 的 fragment presence 和 target-visible fragment fields：

- table common fields 先按 table/field `export_when` 过滤。
- 每条 record 携带由其 source 声明得到的 fragment set，例如 `{ stats, skills }`。
- fragment 本身可选支持 `export` / `export_when`；不匹配当前 target 的 fragment 不进入 runtime payload，也不参与当前 target 的 fragment field validation。
- fragment 内字段继续按 field `export` / `export_when` 过滤。
- 如果 fragment 在当前 target 可见，但其所有字段都被过滤，ExportView 仍应能表达该 record 拥有该 fragment；runtime payload 可输出空 object，例如 `"stats": {}`，代码生成据此提供 `has_stats()` / `hasStats()` 之类查询。
- `row_export` 仍以整条 record 为单位过滤。被过滤掉的 record，其 common fields、fragment presence 和 fragment fields 都不进入当前 target。

fragment presence 的 target 判断顺序固定为：

1. row 是否被 `row_export` 保留。
2. 当前 row 的 source 是否声明该 fragment。
3. fragment meta 是否对当前 target 可见，即 `export = true` 且 `export_when` 匹配。
4. fragment fields 再按 field-level `export` / `export_when` 过滤。
5. 如果 fragment 可见但过滤后没有任何可见字段，runtime payload 仍输出 empty object。

因此：source 未声明的 fragment 不输出 empty object；fragment 对当前 target 不可见时不输出 empty object；fragment 可见但字段都 `export = false` 或都被 target tags 过滤时输出 empty object。

## 5.3 Validation Scope

| Scope | 例子 | 默认输入 |
| --- | --- | --- |
| Project | Meta import、插件、target config | Project config + MetaModel |
| Source | provider 声明、source 可读性 | Source declarations |
| Field | required、type、enum、range、asset ref syntax | 当前验证视图中的字段 |
| Record | 跨字段关系、fragment presence | 当前验证视图中的 record |
| Collection | unique index、权重、概率、行数 | 当前验证视图中的 collection |
| Multi-Collection | foreign key、跨表一致性 | 当前验证视图中的多个 collection |
| Export | target mapping、输出路径、artifact 能力 | Export plan |

ObjectTable Step 1 的输入视图不是多行 records，而是由 CSV `key,value` rows 归一化出的单个 object row。即使 source 只有 header、没有数据行，也必须归一化为一个 empty logical row `{}`，使 required/default/type/enum/struct JSON cell 等字段级规则复用现有 field validation；但 index、row_export、foreign key、fragment-scoped validation 不适用于 ObjectTable Step 1。ObjectTable 的 table-level `export/export_when` 与 field-level `export/export_when` 仍生效；table 可见但字段都被过滤时，导出 empty object。

## 5.4 当前确认后的校验时机

功能性校验面向所有 source 读取完成后的最终视图，而不是每个单独 source 或 override source。

明确规则：

- required 不要求每个单独 source 或 override source 都满足。
- ObjectTable override 可只覆盖指定 field path。
- CollectionTable override 是整行替换。
- 集合级检查默认对过滤后的 ExportView 执行。
- validate 命令若未指定 target，可默认检查 Full DataSet；如果指定 target，则检查对应 ExportView。

## 5.5 Required 与 Default

校验步骤：

1. 查找 source 是否提供值。
2. 如果未提供且有 `default`，使用 default。
3. 如果未提供、无 default 且 `required = true`，报错。
4. 如果提供值但解析失败，报 type parse error。
5. 如果 default 本身无法解析或违反约束，报 Meta/default error。

示例：

```toml
[[tables.fields]]
name = "max_stack"
type = "int"
required = true
default = 1
```

该字段在 source 缺失时使用 `1`，不会报 required error。

### 5.5.1 Fragment 字段 Required / Default

variant source mode 中，required/default 的输入范围由 source 声明的 fragments 决定：

1. common fields：所有 source 的每行都要应用 common field 的 required/default/type 规则。
2. fragment fields：只有当 source 声明了该 fragment 时，该 source 的每行才应用对应 fragment fields 的 required/default/type 规则。
3. source 未声明的 fragment，其字段在该 source 中不要求存在；如果 CSV header 提供了该 fragment 的 `fragment.field` 列，应报 unknown/inactive fragment field，避免误填被静默忽略。
4. fragment required field 缺失但有 default 时，normalize 阶段补 default 后通过。
5. fragment required field 缺失且无 default 时，错误信息应包含 table、source、row、fragment、field，例如：

```text
table `items` source `items/weapons.csv` row 3 fragment `stats` field `attack` is required
```

这意味着 validation 不能只看合并后的 `DataTable.rows`，还需要知道每行来自哪个 `DataSourcePlan` 以及该 source 声明的 fragment set。当前实现已在 Data 模块为每行保留 `RowSourceInfo { source_index, source_path, source_row_number, fragments }`，Validation 阶段据此决定每行 active fragments。

## 5.6 Polymorphic Fragment Validation

variant source mode 的校验分为 Meta、Data load/merge、Data validation 三层。

### 5.6.1 Meta validation

需要检查：

- `SourceRef.fragments` 只能用于 `polymorphic = true` 的 collection table。
- source 引用的 fragment 必须在当前 table 的 `[[tables.fragments]]` 中存在。
- 多 source variant source mode 只允许 `polymorphic = true` 的 CollectionTable，且至少一个 source 必须声明非空 `fragments`；table 仅声明 fragments 但所有 sources 都未激活 fragments 时，不视为合法 variant source mode。
- fragment names 在 table scope 内唯一。
- fragment name 不能与 table common field name 冲突，因为 runtime payload 使用 fragment name 作为顶层 nested object key。
- fragment field names 在所属 fragment 内唯一；不同 fragments 可以复用相同 field name，fragment field 也可以与 common field 同名。
- CSV header 中 fragment field 使用 `fragment.field` 形式，例如 `stats.attack`，common field 使用裸 field name，例如 `attack`。如果存在 common field `attack` 和 fragment field `stats.attack`，两者分别由 `attack` 与 `stats.attack` 两列提供。
- CSV header 中出现未知 `fragment.field`、未声明 fragment 的 `fragment.field`、或 source 未激活 fragment 的 `fragment.field`，都应报 validation error。
- fragment field validator 若未支持 nested/fragment scope，应像 struct child validators 一样先拒绝或明确只支持基础 type/default/required。
- primary/unique index 第一阶段只允许引用 common fields。

target validation 下，fragment 校验基于当前 target 的 ExportView：

- `row_export` 排除的 rows 不参与当前 target 的 fragment required/type/schema 校验。
- visible rows 中的 `fragment.field` dotted key 仍保持严格检查：fragment 必须存在、必须被该行 source 激活，field 必须属于该 fragment。
- fragment / field 的 `export_when` 只影响当前 target 是否执行 required/type/export/codegen，不表示 visible row 的 CSV 可以出现任意 hidden dotted key。

### 5.6.2 Data load / merge validation

需要检查：

- 每个 source 读出的每行都必须是完整 variant record：包含 common required/default 处理所需字段，以及该 source 声明 fragments 的 `fragment.field` 字段。
- 多个 variant sources 追加到同一逻辑 table；按 primary index 或 unique index 检查冲突。即使两个 source 声明不同 fragments，同一 primary key 也冲突。
- 不做按 ID 拼接；如果某 source 行只包含 fragment 字段但缺 common required 字段，应按普通 required 错误处理，而不是尝试从其他 source 补齐。

### 5.6.3 Runtime export validation

runtime payload 应固定 fragment 表达方式。第一阶段推荐直接用 fragment 对应的 nested object 表达 presence，不额外输出 `fragments` 数组：

```json
{
  "id": "1001",
  "name": "Iron Sword",
  "stats": { "attack": "10", "defense": "0" },
  "skills": { "skill_id": "slash" }
}
```

其中：

- common fields 保持当前顶层字段输出。
- 每个 fragment 输出为一个与 fragment 同名的 nested object，内部只包含当前 target 可见字段；fragment 内字段保持自己的局部名称。
- fragment object 出现即表示该 row 拥有该 fragment；缺失则表示该 row 没有该 fragment，或该 fragment 对当前 target 不可见。
- 如果 row 拥有某个当前 target 可见的 fragment，但该 fragment 没有任何 target-visible 字段，仍输出空 object，例如 `"stats": {}`，避免丢失 presence。
- 因为不再输出 `fragments` 数组，`fragments` 不需要作为 runtime payload 保留字段；但 fragment 名称不能与 common field 名称冲突，否则 nested object 与普通字段会歧义。fragment 内字段名可以与其他 fragment 或 common field 重复，因为它们位于不同 object scope。
- scalar 仍沿用当前 runtime payload 策略：先保持字符串输出；typed payload 可作为后续统一升级。

> 编号说明：原 `Index Validation` 章节后续编号可在文档整理时统一顺延；本节先插入 polymorphic fragment validation 语义，避免实现阶段缺少依据。

## 5.6 Index Validation

检查：

- index 字段存在；
- index 字段类型可作为 key；
- unique index 在当前验证视图中无重复 key；
- primary index 存在且 unique；
- `primary_index` 与 `primary = true` 一致；
- preserve_source_order 表不把 source order 当成 foreign key target。

## 5.7 Foreign Key Validation

检查：

- target table 存在；
- target index 存在或 target 有 primary index；
- target index 是 unique；
- source value 与 target key 类型兼容；
- optional FK 全空时跳过；
- composite FK 部分为空时报错；
- 非空 FK 必须解析到一条 target record。

默认 FK 在当前验证视图中解析。因此 shipping target 中被过滤掉的目标记录不会作为 shipping ExportView 中的可引用对象，除非 target policy 明确要求使用 Full DataSet 解析。

## 5.8 Asset 与 Text 校验

### Asset

Asset 字段应至少检查：

- ref 语法合法；
- asset_kind 合法；
- 如果 target 配置了 asset root/provider，则检查资源存在。

Asset existence 默认对 ExportView 执行。被 target filter 排除的记录，其 asset 不阻塞该 target 导出，也不进入 asset manifest。

### Text

Text 字段第一阶段推荐使用原文驱动模型：

- source cell 默认表示待翻译原文；
- 可选 text key/context 由字段或 target policy 补充；
- POT exporter 使用 ExportView 中的 text 生成 msgid 和 source reference；
- source reference 默认使用 source 表名和行列号，确保翻译人员和策划可以定位原始表格位置；
- text 字段可以通过额外的映射列或映射行提供 translator comment，用于解释翻译语境、语气、变量含义或 UI 使用场景；
- 被 filter 排除的记录不进入该 target 的 POT。

## 5.9 Special Exporter 输入

Special exporter 默认读取 target 的 ExportView，而不是 Full DataSet。

影响：

- shipping 排除的测试行，其文本不进入 POT。
- shipping 排除的测试行，其资源不进入 asset manifest。
- `export = false` 字段默认不进入 runtime data，但若 `special_exports` 或 exporter policy 允许，可在未被 record filter 排除的前提下被 special exporter 使用。

## 5.10 诊断要求

每条诊断应尽量包含：

- severity；
- stable code；
- message；
- table；
- field_path；
- record_key；
- source location；
- target；
- plugin/exporter；
- related locations；
- hint。

示例：

```text
error[field.required_missing]: items.name is required but missing
  table: items
  record: id=1001
  source: Data/items.csv:12:3
  hint: provide a value or configure a default
```

要求：

- 错误码稳定，可用于 QA 断言和 CI 分析。
- 多位置错误应提供 related locations。
- 并行执行不影响诊断顺序。
