# 12. Struct Source Layout 与 Expanded Value 专项实现详细设计

相关文档：[Meta 模型](../Design/04-meta-model.md)、[数据校验](../Design/05-data-validation.md)、[Data 模块](05-data-module-detail-design.md)、[Data Artifact Toolkit](08-exporter-module-detail-design.md)。

## 12.1 当前状态与职责

Struct JSON cell / expanded value、recursive nested struct、typed fragment payload、标量 list element-level path 均已实现，共享 DataResolver、typed value parser 和 validation view。Reader 不依赖 Meta，raw DataRow 保持扁平。

| 实现 | 职责 |
| --- | --- |
| [data/resolver.rs](../../src/data/resolver.rs) | path 分类、occurrence、default、visibility 与结构化值。 |
| [data/typed_value.rs](../../src/data/typed_value.rs) | scalar/enum/list/color/vector parse。 |
| [validation/struct_value.rs](../../src/validation/struct_value.rs) | required、unknown child 和结构错误边界。 |
| [validation/context.rs](../../src/validation/context.rs) | raw/resolved companion 的共享入口。 |
| [data_artifact/](../../src/data_artifact/) | 从 resolved value 生成 runtime payload。 |

## 12.2 Source layout 自动判定

Named struct 字段不显式配置 layout。Resolver 按每个 occurrence 的 parent/descendant key 判定：

| Source keys | 结果 |
| --- | --- |
| 只有 attributes | parent JSON cell。 |
| 只有 attributes.attack 等合法 descendant | expanded value。 |
| parent 与 descendant 同时存在 | 混用错误，即使部分 cell 为空也不能绕过。 |
| 两者均无 | Missing，由 required/default/visibility 处理。 |

CollectionTable 以每条 row 的 key set 判定，CSV/XLSX header 保留到 row；ObjectTable 以 singleton 的 key set 判定。不同 source 可以使用不同布局，不能按整张表缓存单一 layout。

Nested struct 对每一级重复同样规则；attributes.offense 可以是 JSON cell，也可由 attributes.offense.attack 展开。路径使用字段名，不使用类型名。

## 12.3 Field path 分类

- top-level key 先匹配 table field；CollectionTable 未映射的普通列可作注释。
- dotted path 必须按 struct schema 或 active fragment 解析；unknown dotted path 不能静默忽略。
- fragment name 与 common field name 不能冲突；inactive fragment path 报错。
- field、struct child 和 fragment 名不能包含保留字符 `.`, `[`, `]`。
- 标量 list 可写 JSON array cell 或 `item_ids[0]`、`item_ids[1]`，也支持 `starter_reward.item_ids[0]`；编号必须从 0 连续，parent 与 element paths 不能混用。
- map、任意 struct-list path `items[0].id`、独立明细子表不属于当前能力。

完整语法见 [复合值与 Array FieldPath](../Design/04-meta-model.md#410-复合值书写方式)。

## 12.4 Occurrence、default 与 visibility

Resolver 归一化为 ResolvedValue / DataValue::Struct / DataValue::Array，并关联来源。Scalar 内部有 typed 值；JSON/MessagePack 的 scalar string contract 是序列化策略，内部模型不是 Scalar(String)。

- optional expanded parent 的 visible child 全空且无 default 时为 Missing，不触发 child required，也不输出空 object。
- required parent 至少需要一个有效 visible child 或 default，不等价于所有 child required。
- parent 不可见时跳过子值校验和导出；target 下无 visible child 时 parent required 不失败，可输出 empty object。
- child required/default/type/enum 独立执行；whole-struct parent default 不支持。

Validation/export 使用同一 resolved companion，不能分别重写 default/empty/visibility 判定。

## 12.5 Typed fragment payload

Fragment 可用 inline fields，或 `type = "AttributeSet"` 引用标准 structs，二者互斥。它仍表达 source 激活的可选数据块，不引入另一套 struct 系统。

Typed payload 复用 struct child path/default/filtering/source trace/runtime nested object。标准 fixture weapons/armors 使用 AttributeSet 共享 schema；fragment presence 由 nested object 存在性表达，不额外输出 fragments 数组。

可复用 field group / base table schema 尚未实现，与已经支持的 typed fragment payload 是不同概念。

## 12.6 Runtime data 与 code generation

CollectionTable/ObjectTable 在 resolve 后均为 logical row；serializer 不需要知道原值来自 CSV/XLSX、JSON cell 或 expanded path。

- JSON/MessagePack 保持 nested object / scalar list，scalar 遵守既有 payload contract。
- bin 使用 schema-aware positional layout。
- generated struct/fragment/list 类型由 Meta 决定，不因 source layout 改变。
- nested error 通过 raw/resolved trace 匹配最长 field path 对应源 cell。

## 12.7 限制与测试

当前不支持 map、whole-struct default、struct child validators 的完整推广、独立子表或 fragment join。未来继续复用 path parser/typed value/resolved view，避免每层自行解释。

[Data tests](../../src/data/tests.rs)、[Validation tests](../../src/validation/tests.rs)、[CLI integration](../../tests/cli_integration.rs)验证 JSON/expanded 等价、不同 occurrence/source、混用、nested/default/empty/hidden child、typed fragment 和 list 连续编号。[XLSX integration](../../tests/cli_integration/xlsx.rs)验证物理定位、混用与等价 payload。测试/coverage 遵守 [代码规范](../code-style.md)。
