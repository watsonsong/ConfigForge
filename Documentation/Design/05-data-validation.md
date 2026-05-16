# 05. 数据视图与校验设计

## 5.1 数据视图

ConfigForge 使用两个主要数据视图。

### Full DataSet

Full DataSet 是所有 source 读取、解析、默认值应用、base merge、override 之后得到的完整逻辑数据集。

它保留：

- 所有记录；
- 所有字段；
- tool-only 字段；
- validator-only 字段；
- source location；
- override 来源信息。

### ExportView

ExportView 是针对某个 target/context 派生的视图。

它会应用：

- record filters；
- field export rules；
- fragment filters；
- `tool_only` / `export = false` 常规运行时裁剪；
- target-specific type mapping。

常规 runtime exporter、special exporter 和 target-specific validation 默认读取 ExportView。

## 5.2 Export Filter

示例：shipping 包排除测试数据。

```toml
[[tables.export_filters]]
scope = "record"
when_tag = "shipping"
field = "test"
op = "=="
value = true
action = "exclude"
```

规则：

- filter 必须引用已声明 field path。
- tool-only 字段若要参与 filter，必须显式 `filter_field = true`。
- filter operator 必须和字段类型兼容。
- record filter 在 ExportView 派生阶段执行。
- 被 filter 排除的数据默认不参与该 target 的 runtime data、special exporter 和集合级校验。

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

## 5.4 当前确认后的校验时机

功能性校验面向所有 source 读取完成后的最终视图，而不是每个单独 source 或 override source。

明确规则：

- required 不要求每个单独 source 或 override source 都满足。
- ObjectTable override 可只覆盖指定 field path。
- CollectionTable override 是整行替换。
- 集合级检查默认对过滤后的 ExportView 执行。
- validate 命令若未指定 target/context，可默认检查 Full DataSet；如果指定 target/context，则检查对应 ExportView。

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
