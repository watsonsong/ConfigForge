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
- source-level `override_sources[].export_when`；
- field `export_when`；
- collection `row_export`；
- fragment filters；
- `export = false` 常规运行时裁剪；
- target-specific type mapping。

常规 runtime data exporter、POT / asset manifest 等聚合 data exporter 和 target-specific validation 默认读取 ExportView。

## 5.2 ExportView 过滤规则

表、字段和 override source 使用 `export_when` 匹配 effective target tags；collection 行使用 `row_export` 匹配数据行中的 tags。

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
- `override_sources[].export_when` 不匹配时，该 override source 不读取、不应用，也不参与后续校验。
- `export = false` 优先级最高，永不进入 runtime ExportView。
- `row_export` 引用的字段必须存在；这些字段可以 `export = false`，仍可参与行级过滤。
- 被 `row_export` 排除的数据默认不参与该 target 的 runtime data、POT / asset manifest 等聚合 data export 和集合级校验。

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

ObjectTable Step 1 的输入视图不是多行 records，而是由 CSV `key,value` rows 归一化出的单个 object row。即使 source 只有 header、没有数据行，也必须归一化为一个 empty logical row `{}`，使 required/default/type/enum/struct JSON cell 等字段级规则复用现有 field validation；index、row_export、fragment-scoped validation 不适用于 ObjectTable Step 1。ObjectTable 可以作为 foreign key source：其 singleton logical row 中的 FK source fields 按普通 raw cell key 引用 CollectionTable unique / primary index；ObjectTable 仍不能作为 FK target。ObjectTable 的 table-level `export/export_when` 与 field-level `export/export_when` 仍生效；table 可见但字段都被过滤时，导出 empty object。

Struct source layout 自动判定落地后，Full DataSet 与 ExportView 中的 logical row 仍保持 row-oriented map，但 source key 可以是 nested field path。CollectionTable 的 CSV header 使用 `field.child` 多列表达；ObjectTable 的 `key,value` source 使用多行 `field.child` 表达。Validation 需要按当前 row/source 的 key 集合扫描每个 struct field occurrence：只出现 parent path 时按 JSON cell 解析；只出现 child paths 时按 expanded value 解析；两者同时出现应视为同一 occurrence 混用 JSON cell 与 expanded cells 并报错。该规则递归作用于 nested struct：如果某一级 occurrence 选择 JSON cell，则该级以下不能再出现展开子项；如果某一级选择 expanded value，则其内部 struct child 可以继续独立选择 JSON cell 或 expanded value。Meta `layout` 不参与 named struct 的 source layout 判定。

Source schema validation 独立于 target filtering 执行：所有 source 中出现的 dotted key 都必须能按 full schema 解析为 struct child occurrence 或 active fragment field，JSON/expanded 混用也应在这一层报错；inactive fragment 或未知 schema 应报错。普通不含 `.` 的 unknown CollectionTable column 继续沿用既有注释列/中间列策略，不因引入 expanded value 隐式变为错误；若项目希望严格拒绝普通 unknown column，应通过后续显式 `strict_source_schema` 配置表达。ObjectTable 的 `key,value` rows 不作为注释列处理，不含 `.` 的 unknown key 仍应报 unknown field key。

Struct expanded value 的 ExportView 过滤顺序固定为：

1. table `export/export_when` 判断整表是否进入当前 target。
2. parent struct field `export/export_when` 判断整个 struct 是否可见。
3. struct child field `export/export_when` 判断 nested object 中的字段是否可见。
4. parent 可见但 child 全部不可见时，runtime payload 输出 empty object；parent 不可见时整个 nested object 不输出。

Value validation 是 target-aware 的：只有当前 target 可见的 parent/child 执行 required/type/enum/range 等值校验。Parent struct field 的 `required = true` 在 expanded value 写法下表示当前验证视图中的 nested object 必须有效存在：至少一个当前可见 child path 提供非空值，或至少一个当前可见 child 有 default 被应用。child 自己的 required/default/type/enum 仍独立执行；parent required 不等价于“所有 child 都 required”。如果 parent field 本身不可见，跳过 parent required 和所有 child value validation；如果 parent 可见但当前 target 没有任何 visible child，parent required 不失败，runtime payload 可输出 empty object。JSON cell 写法下 parent required 仍表示 parent cell 本身必须存在且有效。第一阶段不支持 whole-struct parent default，只支持 child defaults。

Struct expanded value 与 polymorphic fragment validation 共享 dotted key 形式，但二者的作用域不同：fragment dotted key 的第一段必须匹配 active fragment；struct expanded key 的第一段必须匹配 table field，并沿字段 schema 继续逐级匹配 struct child field。Validation 不应把未知 dotted key 静默归入任一机制，也不能把 path segment 当作 struct type name。

## 5.4 当前确认后的校验时机

功能性校验面向所有 source 读取完成后的最终视图，而不是每个单独 source 或 override source。

明确规则：

- required 不要求每个单独 source 或 override source 都满足。
- ObjectTable override 可只覆盖指定 field path。
- CollectionTable override 是整行替换。
- CollectionTable override 在 target-specific source materialization 阶段应用：只加载并应用匹配 effective target tags 的 override source；覆盖后的最终行再进入 required/default/type、row_export、index、foreign key 等校验。
- override row 找不到 base primary index 时默认报错；后续若支持 insert 必须显式配置策略。
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

typed fragment payload 目标态下，Validation 应把 `[[tables.fragments]] type = "StructName"` 视为一个以 fragment name 为 parent path 的 struct occurrence：

- source 激活该 fragment 时，`fragment` parent JSON cell 与 `fragment.child` expanded path 都合法，但同一 occurrence 不允许 JSON/expanded 混用；
- `fragment.child.grandchild` 按 named struct 的 recursive child path 解析，不受 inline fragment “两段 path” 限制；
- inactive typed fragment 的 `fragment.*` 仍报 inactive fragment field，而不是当作普通未知注释列；
- required/default/type/enum/range 等 child value validation 按 target-visible struct child 执行；fragment 自身的 presence 由 source fragments 决定，不另设 fragment-level required；
- runtime export 与聚合 data exporter 读取 target ExportView，server target 不应因 client-only typed fragment child 缺失而失败。

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
  "stats": { "attack": "10", "defence": "0" },
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

## 5.7 Index Validation

检查：

- index 字段存在；
- index 字段类型可作为 key；
- unique index 在当前验证视图中无重复 key；
- primary index 存在且 unique；
- `primary_index` 与 `primary = true` 一致；
- preserve_source_order 表不把 source order 当成 foreign key target。

## 5.8 Foreign Key Validation

检查：

- target table 存在；
- target index 存在或 target 有 primary index；
- target index 是 unique；
- source table 可为 CollectionTable 或 ObjectTable；ObjectTable source 只有一个 singleton logical row 参与校验；
- target table 必须是 CollectionTable，ObjectTable 不能作为 FK target；
- source value 与 target key 类型兼容；
- optional FK 全空时跳过；
- composite FK 部分为空时报错；
- 非空 FK 必须解析到一条 target record。

默认 FK 在当前验证视图中解析。因此 shipping target 中被过滤掉的目标记录不会作为 shipping ExportView 中的可引用对象，除非 target policy 明确要求使用 Full DataSet 解析。

FK 上的 `linked_field` 不引入新的数据校验规则；它只是要求生成代码为已校验通过的 FK 关系生成关联访问和缓存 API。Validation 仍只检查 source fields、target table/index、类型兼容、empty/partial key 与 target record 是否存在。若某个 target 中 FK 因 `export_when` 不匹配、source/target 字段不可见或 target row 被 `row_export` 过滤而不进入 ExportView，则对应 `linked_field` helper 也不得进入该 target 的 generated API。换言之，`linked_field` 的可见性完全继承 FK 本身在当前 ExportView 中的可见性，不应让运行时代码访问到当前 target 未导出的表、字段或行。

生成代码的 lazy relation getter 可以在运行时做防御性检查，但不替代导出前 validation：如果 validated artifact 中 relation lookup 失败，通常表示 runtime data 与 generated code 版本不匹配、手工篡改了数据文件，或业务跳过了 validation。此时 generated helper 应返回错误 / throw，而不是把缺失目标当成 optional empty。只有 `allow_empty = true` 且 source key 全空时才是合法空关系。

## 5.9 Asset 与 Text 校验

### Asset

Asset 字段应至少检查：

- ref 语法合法；
- asset_kind 合法；
- 如果 target 配置了 asset root/provider，则检查资源存在。

Asset existence 默认对 ExportView 执行。被 target filter 排除的记录，其 asset 不阻塞该 target 导出，也不进入 asset manifest。

当前实现状态：`asset` 仍按 string-like scalar 解析和导出，尚未实现 asset ref syntax、asset kind 和文件存在性校验。标准参考游戏 fixture 中 class icon、weapon skill asset 等字段用于锁定目标业务形态：这些字段一般只对 client target 可见，server target 不应因为 client-only asset 缺失而失败；client target 后续应能按配置的 asset root 检查真实资源是否存在，并生成 asset manifest。

### Text

Text 字段第一阶段推荐使用原文驱动模型：

- source cell 默认表示待翻译原文；
- 可选 text key/context 由字段或 target policy 补充；
- POT exporter 使用 ExportView 中的 text 生成 msgid 和 source reference；
- source reference 默认使用 source 表名和行列号，确保翻译人员和策划可以定位原始表格位置；
- text 字段可以通过额外的映射列或映射行提供 translator comment，用于解释翻译语境、语气、变量含义或 UI 使用场景；
- 被 filter 排除的记录不进入该 target 的 POT。

POT / text 相关校验目标态应补充：

- `text` 原文非空规则仍遵循 required / optional field 规则；POT exporter 不负责把空文本升级为 required error。
- 如果启用显式 context / key 列，validation 应检查同一 text field 的 key/context 语法合法、没有非法控制字符，且 optional text 的辅助列不会在主 text 为空时单独产生条目。
- 如果启用 placeholder schema，validation 应检查 source text、translator comment 和未来外部 catalog 中的占位符集合一致，避免运行时格式化失败。
- 如果启用 plural / gender / select 等高级国际化类型，应由对应 field type 或 validator 明确承担结构校验，不应把 primitive `text` 的语义无限扩张。
- target-aware validation 只检查当前 ExportView 中可见 text；被 `row_export`、`export_when` 或 fragment visibility 排除的文本不产生 POT 条目，也不产生 POT 相关错误。

当前实现状态：`text` 仍按 string-like scalar 解析和 runtime data 导出；已实现 `type = "gettext_pot"` 的首版聚合 exporter，按当前 target 可见 table / row / field 抽取 primitive `text` 原文作为 `msgid`，支持 `table.field` context、source location / record key reference、field `desc` 与自动 table/field/record translator comment、以及 `(msgctxt, msgid)` 去重。国际化 key/context 显式列、placeholder / plural 诊断、fragment / struct child text 深度抽取和外部 catalog 导入仍属于后续扩展。标准参考游戏 fixture 中 item/class name、class description 等字段用于表达目标语义：客户端需要 text，服务器通常不需要；target ExportView 过滤后，server target 不应输出这些字段，也不应把它们送入未来 POT / text validation。

## 5.10 聚合 Data Exporter 输入

POT、asset manifest 等聚合 data exporter 默认读取 target 的 ExportView，而不是 Full DataSet。

影响：

- shipping 排除的测试行，其文本不进入 POT。
- shipping 排除的测试行，其资源不进入 asset manifest。
- `export = false` 字段默认不进入 runtime data；若未来字段策略或 exporter policy 允许某个聚合格式读取辅助字段，可在未被 record filter 排除的前提下被该 data exporter 使用。

## 5.11 诊断要求

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
