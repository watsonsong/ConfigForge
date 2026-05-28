# 12. Struct Source Layout 与 Expanded Value 专项实现详细设计

本文档描述 named struct 字段在 CSV 中根据 source 形态自动选择 JSON cell 或 expanded value 的实现方案。产品语义以 `Documentation/Design/04-meta-model.md` 和 `Documentation/Design/05-data-validation.md` 为准；本文档聚焦 Rust 实现边界、数据模型、校验、导出和测试计划。

## 12.1 结论：当前结构不能直接支持

当前代码已有 `ValueLayout::ExpandedValue` 枚举，但还不能完成 struct source layout 自动判定和展开：

- Meta validation 当前拒绝 struct + `expanded_value`，但仍可能把 `json_cell` 当作 struct JSON cell 的显式写法；目标设计要求 named struct 字段不再由 Meta `layout` 控制，代码需统一改为由 source header/key 自动判定。
- Runtime validation 对 struct 字段的 JSON cell / expanded value 校验已拆到 `src/validation/struct_value.rs`，由 table validation 在遇到 `LinkedField::Struct` 时调用。
- DataRow 当前是 `BTreeMap<String, String>`，可以容纳 `attributes.attack` 这类 dotted key，但 validation/export 还没有把 dotted key 归并为 parent struct object 的逻辑。
- Export 当前的 struct nested object builder 基于 parent field 的 JSON cell 字符串；没有从多个 child cells 构建 nested object。
- Fragment 已经使用 `fragment.field` dotted key，但那是 polymorphic fragment presence 语义，不能直接复用为 struct expanded value，否则会和 common field / fragment 解析冲突。

因此需要开发。推荐保留现有 row-oriented `DataRow`，新增 field path 解析和 struct expanded object 构建逻辑，而不是重写 Data 模块为 typed tree。

## 12.2 目标

实现 named struct 字段的 source layout 自动判定与 expanded value：

1. CollectionTable 中，同一个 struct field occurrence 可由 CSV header 决定使用 parent JSON cell 或多个 child columns。
2. ObjectTable 中，同一个 struct field occurrence 可由 `key,value` rows 决定使用 parent JSON row 或多行 child field paths。
3. Validation 对展开后的 child field 执行 required/default/type/enum 和 target-aware 过滤。
4. Runtime export shape 与 struct JSON cell 一致，输出 nested object。
5. Codegen 无需改变类型形态；同一个 named struct 仍生成同样的目标语言 struct/interface。

示例 Meta：

```toml
[[structs]]
name = "Attributes"

[[structs.fields]]
name = "attack"
type = "int"
default = 0

[[structs.fields]]
name = "max_health"
type = "int"
required = true

[[tables]]
name = "character_levels"
source = "character_levels.csv"

[[tables.fields]]
name = "attributes"
type = "Attributes"
```

Meta 不需要为 `attributes` 指定 `layout = "expanded_value"`；下面的 CSV header 决定该 field occurrence 使用展开写法。

CollectionTable CSV：

```csv
id,level,attributes.attack,attributes.max_health
hero,1,10,120
hero,2,12,140
```

ObjectTable CSV：

```csv
key,value
default_attributes.attack,10
default_attributes.max_health,120
```

Runtime payload：

```json
{
  "default_attributes": {
    "attack": "10",
    "max_health": "120"
  }
}
```

## 12.3 非目标

第一阶段不实现：

- array/map schema。
- 固定槽 array 的 `rewards[0].item_id` 展开。
- array/map 内部元素的递归展开；nested struct field path 本身按字段路径递归支持，但 array/map schema 仍另行设计。
- header alias，例如把 canonical `attributes.attack` 映射到表头 `Attributes.Attack`。
- 独立子表式 detail table。
- runtime scalar typed export；scalar child 仍沿用 string payload contract。
- struct child validators 的完整 registry 化；第一阶段只复用 child 的 required/default/type/enum 和 export filter。

## 12.4 Layout 归属决策

Struct 使用 JSON cell 还是 expanded value 应由 CSV/source 中出现的 field paths 决定，而不是由 named struct type 或 table field Meta 写死。

推荐规则：

- 判定单位是 struct field occurrence 的完整 parent field path，而不是 struct type。`base_attributes` 和 `growth_attributes` 即使都是 `Attributes` 类型，也必须分别扫描和判定。
- CollectionTable 扫描当前 source 的 header 集合；ObjectTable 扫描当前 source 的 key 集合。
- 对某个 occurrence，如果出现 parent path（如 `attributes`）且没有出现其 child paths（如 `attributes.attack`），则按 JSON cell。
- 对某个 occurrence，如果没有 parent path，但出现一个或多个 child paths，则按 expanded value。
- 同一个 occurrence 不支持混用：如果同时出现 parent path 和 child paths，例如 `attributes` 与 `attributes.attack`，必须报 source schema error。选择 JSON cell 的 occurrence 被视为 opaque object，scanner 不再允许该 occurrence 的任何 descendant path。
- Meta 仍负责定义字段类型、required/default/export_when 等逻辑 schema；CSV 只决定该 source 对该 occurrence 的物理书写形态。

这样选择的原因是：struct 是可复用类型，同一个 struct 在不同 Table、不同父级字段、不同数据编辑场景中可能更适合不同写法。把 layout 固定在 Meta field 上会让同一结构体复用时不够灵活；由 source field paths 判定可以让表格作者根据表格密度自行选择 JSON cell 或展开列，同时通过“禁止同一 occurrence 混用”避免覆盖顺序和 required/default 歧义。递归场景下，某一级展开后，内部 struct child 仍可继续选择 JSON 或展开；某一级选择 JSON 后，该 JSON cell 下方不再允许展开子项。

## 12.5 数据模型策略

### 12.5.1 保持 Raw DataRow 扁平，新增 Resolved 层作为长期方向

继续使用：

```rust
pub type DataRow = BTreeMap<String, String>;
```

CollectionTable 读取 CSV header 时保留原始 dotted key，例如 `attributes.attack`。这层应视为 RawDataSet：source-neutral、保留 raw string、便于诊断和 inspect。

ObjectTable `read_object_table()` 当前把 key/value rows 归一化为单 logical row。支持 expanded value 后，只需允许 key 是合法 field path，并把：

```text
default_attributes.attack -> "10"
default_attributes.max_health -> "120"
```

写入同一个 DataRow。ObjectTable 的 duplicate key 检查仍按完整 key/path 检查。

Data 层方向是：在 Data Load 之后、Validation / Export 之前增加 Meta-aware resolver，生成 `ResolvedDataSet` / `ResolvedExportView`，让 Validation 和 Exporter 不再各自解释 JSON cell 与 dotted path。

推荐链路：

```text
CSV / ObjectTable / future Sheet providers
        ↓
RawDataSet
flat string + source location
        ↓
DataResolver / StructOccurrenceEvaluator
Meta-aware: JSON cell / expanded path / default / target visibility
        ↓
ResolvedDataSet or ResolvedExportView
typed scalar + nested struct + source trace
        ↓
Validation / Exporter
```

不建议把 struct expanded value 解析放进 `CsvDataReader`，因为 reader 不应依赖 linked meta、target tags、default、fragment active set 或 validator 语义；否则未来新增 xlsx、Lark、Google Sheet provider 时会复制 Meta-aware 解析逻辑。

### 12.5.2 必须新增 source layout scanner 与 row key classifier

必须在 validation 或 data/schema helper 中增加 source layout scanner 与统一 row key classifier。scanner 负责基于当前 row/source 的 key 集合为每个 struct field occurrence 判定 JSON cell 或 expanded value；classifier 负责把具体 row key 归类为 table field、struct leaf、fragment field 或错误。required/default/type 校验、unknown dotted key 校验、fragment validation 和 export builder 都应使用同一套扫描/classification 结果；不能让 fragment validation 继续默认把所有 dotted key 的第一段都当成 fragment name。

如果引入 `DataResolver`，scanner/classifier 应作为 resolver 的内部能力，并输出可复用的 struct occurrence evaluation，而不是分别在 validation/export 中重复实现。

判定结果不应缓存为 table-level 全局 layout。CollectionTable 以每个 logical row 的 key set 作为 scanner 输入；由于 CSV reader 为每行保留 header keys，这等价于该 row 所属 physical source 的 header set。ObjectTable 以 singleton logical row 的 key set 作为 scanner 输入。Polymorphic multi-source 下，不同 source/row 可以对同一个 occurrence 使用不同 layout。

建议新增 source layout 结果：

```rust
enum StructSourceLayout {
    JsonCell,
    ExpandedValue,
    Missing,
}

struct StructOccurrenceLayout {
    parent_path: String,
    layout: StructSourceLayout,
}
```

如果未来需要严格按 physical source header/key set 做 source-level 校验，可在 Data 模块补充 source schema metadata：

```rust
pub struct DataSourceSchemaInfo {
    pub source_index: usize,
    pub source_path: PathBuf,
    pub keys: BTreeSet<String>,
}
```

扫描规则：

1. 对每个 logical row / ObjectTable singleton row 的 key 集合，按 table meta 枚举所有可能的 struct field occurrence path。
2. 如果集合包含 `parent_path`，记录 parent present。
3. 如果集合包含任意 `parent_path.*` 且这些 child paths 能按 struct schema 解析，记录 children present。
4. parent present 且 children present 时，报混用错误；错误应包含 parent path 和至少一个冲突 descendant path。
5. 只有 parent present 时为 `JsonCell`。
6. 只有 children present 时为 `ExpandedValue`。
7. 两者都没有时为 `Missing`，后续由 required/default 规则处理。
8. nested struct 递归适用同样规则：`attributes.offense` 是 `offense` occurrence 的 parent path；`attributes.offense.attack` 是它的 child path。如果 `attributes.offense` 判定为 `JsonCell`，则 `attributes.offense.attack` 等 descendant paths 都必须报混用错误；如果 `attributes.offense` 判定为 `ExpandedValue`，则继续向下扫描其 children。
建议解析结果：

```rust
enum RowKeyKind<'meta> {
    TableField(&'meta LinkedFieldMeta<'meta>),
    StructLeaf {
        occurrence_path: String,
        occurrence: StructOccurrenceRef<'meta>,
        leaf: &'meta LinkedFieldMeta<'meta>,
        full_path: String,
    },
    FragmentField {
        fragment: &'meta LinkedFragmentMeta<'meta>,
        field: &'meta LinkedFieldMeta<'meta>,
    },
    UnknownPlainColumn,
    UnknownDottedPath,
    InactiveFragmentField,
}
```

递归 path 不能只用一层 parent/child 表达；classifier 返回的必须是完整 field path 解析结果。建议引入 occurrence path 模型：

```rust
struct StructOccurrenceRef<'meta> {
    path: String,
    field: &'meta LinkedFieldMeta<'meta>,
    struct_type: &'meta LinkedStructType<'meta>,
    parent: Option<Box<StructOccurrenceRef<'meta>>>,
}
```

解析规则：

1. 不含 `.` 的 key 先匹配 table top-level field；CollectionTable 中不匹配时归为 `UnknownPlainColumn`，继续沿用注释列/中间列兼容策略；ObjectTable 中不匹配时应报 unknown field key。
2. 含 `.` 的 key 拆分第一段和剩余 path。
3. 第一段匹配 table field 且该 field 是 named struct 时，剩余 path 继续按该 struct 的 child field name 解析；能解析到 child occurrence 时归为 `StructChild`。
4. 第一段匹配 table field 但该 field 不是 struct 时，应报该 field 不支持 child path。
5. 第一段匹配 active fragment 时，按现有 fragment 规则解析。
6. 第一段匹配 fragment 但当前 source 未激活该 fragment 时，归为 `InactiveFragmentField` 并报 inactive fragment。
7. 两边都不匹配时报 unknown dotted field path；dotted path 不作为普通注释列静默忽略。
8. 如果第一段既是 table field 又是 fragment name，应由 Meta validation 提前禁止 fragment name 与 common field name 冲突；解析 helper 可以依赖这个约束。

Struct path 可以递归解析到 nested struct child，但每一级 occurrence 仍独立判定 JSON cell 或 expanded value。`attributes.offense.attack` 不是按 `Offense` 类型名解析，而是按字段路径 `attributes` -> `offense` -> `attack` 解析。

### 12.5.3 StructOccurrenceEvaluator 与 ResolvedDataSet

为避免 Validation 和 Exporter 重复实现 default / empty cell / target visibility / nested occurrence 规则，建议把 struct occurrence 解析提炼为共享 evaluator：

```rust
struct StructOccurrenceEvaluator<'meta, 'data> {
    table: &'meta LinkedTableMeta<'meta>,
    row: &'data DataRow,
    target_tags: Option<&'data TargetTags>,
}

struct ResolvedStructOccurrence<'meta> {
    path: String,
    field: &'meta LinkedFieldMeta<'meta>,
    layout: StructSourceLayout,
    has_effective_value: bool,
    value: Option<DataValue>,
    source_trace: ValueSourceTrace,
}
```

`has_effective_value` 的统一规则：

- parent JSON cell 非空；或
- 任一 target-visible descendant 有非空 source value；或
- 任一 target-visible child 有非空 default；
- 如果 target 下没有 visible child，则 parent required 不失败，可解析为空 object。

空 expanded cell 不应单独构成 occurrence。CollectionTable 由于 header 会在每行都生成 key，如果某一行所有 target-visible child cell 都为空且没有 child default，则 optional parent struct 在该行视为 missing，不应触发 child required，也不应 export `{}`。

`DataValue` 可先支持当前 struct MVP：

```rust
enum DataValue {
    Scalar(String), // 短期保持 runtime scalar string contract
    Struct(BTreeMap<String, DataValue>),
}
```

长期可扩展为 typed scalar：

```rust
enum ScalarValue { String(String), Int(i64), Float(f64), Bool(bool), Enum(String) }
```

这样 JSON cell 与 expanded value 最终都解析为同一 `DataValue::Struct`，Validation 只验证结构化值，Exporter 只序列化结构化值。

当前已落地基础 `DataResolver` / `ResolvedDataSet`，runtime exporter 主路径已消费 resolved value。标准参考游戏 fixture 进一步暴露出两个后续扩展方向：

1. **Struct as top-level table field** 已支持：`class_levels.attributes.max_health` 等 CSV header 会解析为 `attributes: AttributeSet`，并导出 nested object。
2. **Struct as fragment payload** 尚未支持：当前 fragment 只能声明字段列表，不能直接写 `[[tables.fragments]] name = "attributes" type = "AttributeSet"` 并把 `attributes.max_health` 解析为该 fragment payload 的 child。因此 weapons / armors fixture 需要在 fragment 中重复声明 `max_health/max_stamina/attack/defence`。下一步应让 fragment payload 也走同一个 StructOccurrenceEvaluator/DataResolver path，保持和 table field struct 完全一致的 JSON/expanded/default/target filtering 语义。设计上不新增专门的 fragment struct；fragment payload 复用标准 `[[structs]]`，`[[tables.fragments]]` 只负责声明 fragment name、target policy 和 payload type。

建议的 typed fragment payload 实现方向：

```rust
enum LinkedFragmentPayload<'meta> {
    InlineFields(Vec<LinkedFieldMeta<'meta>>),
    Struct(LinkedStructType<'meta>),
}
```

对应 raw meta 可以保持 `FragmentMeta` 为 table-local fragment 声明，并新增可选 `type_name` / TOML `type` 字段：

```rust
pub struct FragmentMeta {
    pub name: String,
    pub type_name: Option<String>, // #[serde(rename = "type")]
    pub fields: Vec<FieldMeta>,
    // desc/export/export_when...
}
```

Meta validation 规则：

- `type` 必须引用已存在的标准 struct；
- `type` 与 `fields` 互斥，避免 inline payload 和 typed payload 混用；
- fragment 仍不能直接声明 indexes / row_export / table-level validators；这些能力属于 table 或未来 validator scope，不属于 payload shape；
- 如果 fragment 仅一次性使用且非常简单，可以继续使用 inline `fields` 作为兼容简写；推荐新业务优先使用标准 struct，减少 schema 概念数量。

Source path classifier 规则：

| Source key/path | inline fragment payload | typed struct payload |
| --- | --- | --- |
| `fragment` | 非法，inline fragment 没有 parent JSON cell。 | parent JSON cell，按 struct JSON cell 解析。 |
| `fragment.field` | `field` 必须是 inline fragment direct field。 | `field` 作为 struct direct child，可继续按 child 类型判断 scalar / nested struct。 |
| `fragment.child.grandchild` | 非法，inline fragment 第一阶段不支持多级 path。 | 递归 struct child path，例如 nested struct expanded value。 |
| inactive `fragment.*` | 报 inactive fragment field。 | 同样报 inactive fragment field；不能因为 struct child 合法而绕过 active fragment 检查。 |
| unknown `fragment.*` | 如果第一段不是 table field 或任一 fragment name，按 unknown dotted field path 报错。 | 同 inline。 |

Layout mixing：typed fragment 与普通 struct occurrence 使用同一套规则。同一 row/source 中如果出现 parent JSON cell `attributes`，则不允许再出现 `attributes.attack` / `attributes.offense.attack` 等 descendant expanded path；反之亦然。诊断 field path 使用 fragment parent path，例如 `items.attributes`。

Required/default 与 target filtering：source 声明 fragment 后，fragment presence 对该 source 的所有 rows 生效；typed payload 不再额外配置 `required`。当 fragment 在当前 target 可见时，struct child required/default/type 按普通 struct occurrence 规则执行，visible child default 可以构成 effective object。执行顺序为 row visibility -> fragment `export_when` -> struct child `export_when`。fragment 对 target 不可见时跳过 payload value validation/export/codegen；fragment 可见但某些 child 不可见时只跳过对应 child。

Codegen：typed fragment 的 row member 应保留 optional presence，但成员类型复用标准 struct 或 target-specific projected struct：Rust 为 `Option<AttributeSet>`，TypeScript 为 `attributes?: AttributeSet`，C++ 为 `std::optional<AttributeSet>`。如果 target filtering 隐藏了 struct 部分 child，而生成器当前不能表达 partial named struct，应生成 target-specific fragment payload type，避免把 hidden child 暴露在当前 target API 中。

Resolver 处理 active fragment 时：

- inline fields 保持当前逻辑；
- struct payload 将 fragment name 作为 occurrence parent path，调用 struct occurrence resolver。例如 fragment `attributes` + `type = "AttributeSet"` 读取 CSV header `attributes.attack`；
- runtime payload key 仍为 fragment name；如果 payload 是 struct，直接输出 struct children，不额外包裹同名 child；
- source schema validator 需要把 `fragment.child` 同时识别为 fragment inline field 或 fragment struct child，若二者混用/冲突应由 Meta validation 提前禁止。

## 12.6 Meta validation 修改

需要修改 `validate_field_layout()`：

- named struct field：不再通过 `layout` 控制 source shape；`layout` 应省略，JSON cell / expanded value 完全由 CSV header/key 自动判定。
- struct field + `scalar_cell` 继续拒绝。
- struct field + `json_cell` / `expanded_value` 不再支持，应由 Meta validation 拒绝，避免 Meta 与 source header/key 之间出现第二套控制来源。
- non-struct field + `json_cell` / `expanded_value` 继续拒绝。

建议新增限制：

- table field name、struct field name、fragment name 不允许包含 `.`, `[`, `]`，这些字符分别用于当前 dotted field path 和未来 array slot path。
- struct child field 如果是 named struct，不需要显式 layout；它作为 nested occurrence 时同样由 source scanner 根据 `parent.child` / `parent.child.*` 判定 JSON cell 或 expanded value。
- struct child validators 如果当前执行框架无法定位 nested path，应继续作为 unsupported 或只允许基础规则。

需要更新 meta tests：

- struct 字段省略 layout 时可根据 source 自动判定 JSON cell / expanded value。
- struct `json_cell` / `expanded_value` 不再通过 meta validation，诊断应提示删除 `layout`，改由 CSV header/key 决定 JSON cell 或 expanded value。
- primitive/enum `expanded_value` 仍失败。
- struct `scalar_cell` 仍失败。
- table field / struct child / fragment name 包含 `.`, `[`, `]` 时失败。

## 12.7 Data load 修改

### 12.7.1 CollectionTable

CSV reader 可继续原样读取 headers，不需要理解 meta。主要变化在 validation：原来未知 header `attributes.attack` 会被视为未知字段；现在应识别为 expanded struct child。

### 12.7.2 ObjectTable

`read_object_table()` 不应理解 linked struct schema。支持 expanded value 后，`DataSourcePlan.fields` 仅有 top-level 字段，不够判断 dotted key；如果继续在 loader 中做 schema 判断，会把 Meta-aware 解析提前到 source adapter。建议两种方案：

方案 A：ObjectTable CSV loader 只做基础 key 非空 / duplicate / header 检查，unknown key/path 下沉到 validation。

方案 B：扩展 `DataSourcePlan`，为 fields 增加 layout 和 direct child schema 快照，使 loader 可提前判断合法 field path。

推荐采用 A：所有 key/path 的 schema 合法性下沉到 resolver / validation，因为它依赖 linked struct schema；loader 只检查 key 非空、完整 key/path 不重复、header 为 `key,value`。不含 `.` 的 top-level unknown key 也由 resolver / validation 报错，以保持 ObjectTable 与未来 Sheet provider 的 source adapter 行为一致。

key schema 下沉会带来诊断精度风险。因此 ObjectTable field source map 是 resolver 阶段的重要补充：`read_object_table()` 在生成 singleton logical row 时，未来应同时记录每个 key/path 的 source row number；resolver / validation 对 ObjectTable 的 unknown path、type、required 等错误优先使用该映射输出真实 CSV row。如果短期不改 `DataTable` public shape，可先增加可选 sidecar map 或 ObjectTable 专用 source map。

实施方案 A 时：

- ObjectTable loader 允许 `key` 中出现 `.`。
- 仅检查 key 非空、完整 key/path 不重复、header 为 `key,value`。
- 对所有 key，只做 duplicate/source row 记录，schema 检查交给 unified row key classifier；source row 记录后续应进入 ObjectTable field source map。

## 12.8 Validation 修改

### 12.8.1 Source schema validation

新增 row key schema validation：

- Source schema validation 是 target-independent 的第一层校验：所有 dotted key 必须能按 full schema 解析，JSON/expanded 混用也在这一层报错，不受 `export_when` 影响。
- CollectionTable 的 dotted key 必须匹配 active fragment field 或 struct child field path；普通不含 `.` 的 unknown column 沿用既有注释列/中间列策略，不因 expanded value 隐式变为错误。
- ObjectTable singleton row 的 key 必须匹配 top-level field 或 struct child field path；ObjectTable Step 1 仍不支持 fragment。
- 同一 struct occurrence 不允许同时出现 parent JSON cell 与 child expanded cells。
- common expanded struct key 与 fragment key 可以在同一 table 中共存，但必须由 unified row key classifier 根据 table field、active fragment 和 source fragments 统一判定。

### 12.8.2 Required/default/type

当前 validation 对每个 top-level field 调用 scalar 或 struct JSON 校验。需要拆分 struct field 分支：

```text
if field is struct:
  match source_layout_for(field_path):
    json_cell/default -> validate_struct_json_cell(parent raw value)
    expanded_value -> validate_struct_expanded_value(row, parent, struct_type)
    missing -> apply parent required semantics; whole-struct parent default remains unsupported
else:
  validate scalar cell
```

`validate_struct_expanded_value()` 逻辑：

1. 遍历 struct direct children。
2. 按 `parent.child` 从 row 中取 raw value。
3. 如果 child 不匹配当前 target 的 export_when，在 target validation 中跳过。
4. 如果缺失且 child 有 default，使用 default。
5. 如果 child required 且缺失或 empty，报 required error，field path 为 `parent.child`。
6. 对 primitive / enum child 使用现有 scalar/enum 校验。
7. 对 struct child 递归调用同一 source layout scanner 结果：如果 child occurrence 是 JSON cell，则调用 `validate_struct_json_cell()`；如果 child occurrence 是 expanded value，则继续递归校验其 children。
8. Value validation 是 target-aware 的：如果 parent field 本身不可见，跳过 parent required 和所有 child value validation；如果 child 不可见，跳过该 child 的 required/type/enum/range 等值校验。
9. 如果 parent field `required = true`，则在当前验证视图中要求该 nested object 有效存在：至少一个当前可见 child path 提供非空值，或至少一个当前可见 child 有 default 被应用；parent required 不等价于所有 child required。如果 parent 可见但当前 target 没有任何 visible child，parent required 不失败，runtime payload 可输出 empty object。
10. 第一阶段不支持 whole-struct parent default；struct field 上的 `default` 仍由 Meta validation 拒绝。expanded object 内只应用 child defaults。

Nested struct 第一阶段行为示例：

```csv
id,attributes.offense
1001,"{""attack"":""10""}"
```

其中 `attributes.offense` 是 nested occurrence 的 parent path，value 按 JSON cell 校验；此时 source 中不允许再提供 `attributes.offense.attack`。如果 source 改为：

```csv
id,attributes.offense.attack
1001,10
```

则 `attributes.offense` occurrence 按 expanded value 判定，并继续校验 `attack` child。

Validation 和 Export 必须共享 child effective value 解析逻辑，避免 default/empty/target visibility 规则漂移。下一阶段应优先抽到 `StructOccurrenceEvaluator` / `DataResolver`，可以新增 view/helper：

```rust
struct StructObjectView<'a> {
    parent_path: String,
    layout: StructSourceLayout,
    children: Vec<StructChildValue<'a>>,
}

struct StructChildValue<'a> {
    field: &'a LinkedFieldMeta<'a>,
    path: String,
    raw: Option<String>,
    source: StructValueSource,
}
```

或至少提供共享函数：

```rust
fn struct_child_effective_value(row, parent, child, target_tags) -> Option<Cow<'_, str>>
```

当前已确认的 effective existence 语义：

- required parent struct 可以由 visible child default 构成 effective value；validation 通过时 export 也必须输出对应 nested object。
- optional expanded struct 中，只有 header/key 存在但所有 visible child cell 都为空且没有 default 时，视为 missing，不触发 child required，也不输出空 object。
- target-hidden descendant 不应让当前 target 认为该 struct occurrence 已出现；occurrence 判断必须只统计 target-visible child path / default。

标准参考游戏 fixture 中 `class_levels.attributes.*` 使用 expanded struct source，直接复用 `AttributeSet` named struct；`items` 表的 `weapons.csv` / `armors.csv` source variants 中，`attributes.*` 目前仍是 fragment scalar fields 的重复声明，作为后续 typed fragment payload 的测试入口。该 fixture 同时验证游戏语义中使用 `class` 表达角色职业，并把相关 CSV 放在统一 `data/` 根目录下。

### 12.8.3 Field validators

现有 field validator 主要绑定 top-level field。第一阶段建议：

- parent struct field 上的 validators 暂不应用到 nested object，除非 validator 明确支持 struct scope。
- struct child field validators 暂不执行或在 Meta validation 中报 unsupported，避免看似配置生效但实际跳过。
- range 等基础 scalar validator 未来可扩展为 child field path validator。

## 12.9 Runtime export 修改

Export builder 需要按 layout 构建 nested object：

- struct occurrence 为 `JsonCell`：沿用当前 JSON cell -> nested object。
- struct occurrence 为 `ExpandedValue`：从 `row["parent.child"]` 收集 child values，构建 `ExportedValue::Object`。
- struct occurrence 为 `Missing`：按 required/target visibility 结果决定报错或省略；第一阶段不输出 whole-struct parent default object。

建议新增 helper：

```rust
fn export_struct_expanded_value(
    row: &DataRow,
    parent: &LinkedFieldMeta<'_>,
    struct_type: &LinkedStructType<'_>,
    source_layouts: &StructSourceLayouts,
    target_tags: &TargetTags,
) -> Result<ExportedValue>
```

导出规则：

- parent field 不可见时不输出。
- child 不可见时不输出。
- child missing 但有 default 时输出 default。
- child missing 且非 required 时不输出。
- parent 可见但所有 child 被过滤时输出 empty object；parent 可见、有 visible child、但所有 visible child 都缺失且无 default 时，validation 已负责按 parent required 规则处理。
- unknown child key 已在 validation 阶段报错；export 可保留防御式错误。

CollectionTable 与 ObjectTable 共用同一 helper，因为二者进入 export 时都是 logical row。

Export 不应重新实现一套 default/empty/target visibility 判断。它应复用 validation 使用的 `StructObjectView` / `struct_child_effective_value` 等共享 helper，确保“validation 通过的 effective value”和“runtime payload 输出的 value”一致。

## 12.10 Codegen 影响

Codegen 类型不需要因 layout 改变：

- named struct 仍生成同样的 Rust struct / TypeScript interface / C++ struct。
- table row 或 object field 类型仍引用该 named struct。
- loader 接口仍接收 runtime data artifact 的 nested object，由业务 runtime loader 解码为 typed struct。

如果未来生成 loader decoder，则 decoder 读取的是 runtime payload shape，不需要知道 source layout 是 JSON cell 还是 expanded value。

## 12.11 测试计划

### 12.11.1 Meta tests

- struct 字段省略 layout 时允许通过，并可由 source scanner 判定 JSON cell / expanded value。
- struct 字段上的 `json_cell` / `expanded_value` 不再允许通过；诊断应提示删除 `layout`，改由 source header/key 自动判定。
- primitive `expanded_value` 失败。
- enum `expanded_value` 失败。
- struct `scalar_cell` 失败。
- field / struct child / fragment name 包含 `.`, `[`, `]` 时失败。

### 12.11.2 Validation tests

- CollectionTable expanded struct child type 校验通过。
- CollectionTable expanded struct required child 缺失失败。
- CollectionTable 普通 unknown non-dotted column 沿用注释列策略。
- CollectionTable unknown dotted key 失败。
- expanded struct key 与 active fragment key 同表共存。
- CollectionTable parent JSON key 与 child expanded key 同时出现失败。
- struct 字段显式写 `layout = "json_cell"` / `layout = "expanded_value"` 时 Meta validation 失败，避免该字段被误解为 source policy。
- CollectionTable 同一个 struct type 的两个 field occurrence 分别使用 JSON cell 和 expanded value 时通过。
- ObjectTable `key,value` 中 `parent.child` 校验通过。
- ObjectTable expanded child type 错误失败。
- parent `required = true` + 有 visible child 但无 effective value 失败。
- parent `required = true` + 所有 child target-hidden 不失败，并可导出 empty object。
- parent `required = true` + child default 通过。
- whole-struct parent default 在 Meta validation 中失败；child default 在 expanded object 中生效。
- nested struct child JSON cell 通过。
- nested struct child expanded value 通过，例如 `attributes.offense.attack`。
- 同一 nested occurrence 的 `attributes.offense` 与 `attributes.offense.attack` 混用失败。
- 某一级 JSON cell 下继续展开 descendant path 失败，例如 `attributes.offense` 与 `attributes.offense.attack` 同时出现失败。
- 某一级 expanded value 下的内部 struct child 可继续选择 JSON cell 或 expanded value。
- child default 在 validation 和 export 中一致。
- target-aware child export_when 在 validation/export 中生效。
- target-hidden child 的 unknown dotted path 仍作为 source schema error。
- multi-source table 中 source A 使用 JSON cell、source B 使用 expanded value 时分别按 row/source 判定。
- ObjectTable dotted key type error 能定位到真实 CSV row。
- dotted key 不匹配 struct 或 fragment 时失败。

### 12.11.3 Export tests

- JSON CollectionTable 输出 nested object。
- JSON ObjectTable 输出 nested object。
- MessagePack shape 与 JSON 一致。
- parent visible + all children filtered 输出 empty object。
- child default 输出。
- child enum/primitive scalar 仍是 string。

### 12.11.4 Integration fixture

建议在 `tests/fixtures/standard_project` 中给角色等级、武器或装备添加复用的 `Attributes` struct：

```toml
[[structs]]
name = "Attributes"

[[structs.fields]]
name = "attack"
type = "int"

[[structs.fields]]
name = "max_health"
type = "int"
```

然后在至少一张 CollectionTable 与一张 ObjectTable 中通过 CSV/header key 使用 child paths，验证两类表都能导出 nested object；另保留一张表用 parent JSON cell，验证同一个 named struct type 可在不同 source 中选择不同写法。

## 12.12 建议实施顺序

当前 source-driven expanded values 已支持：Meta 约束、ObjectTable dotted key 放行、source schema validation、recursive nested struct、JSON/expanded mixing error、target-aware child filtering、child default、JSON/MessagePack export shape 与回归测试。计划按 Data resolver 方向演进：

1. 抽 `StructOccurrenceEvaluator`：把 validation/export 中关于 JSON cell、expanded path、default、empty cell、target visibility、nested occurrence 的判断集中到同一个 helper。
2. 为 ObjectTable 增加 key/path -> source row number 的 sidecar source map，并让 evaluator / validation 使用真实 CSV key 行输出诊断。
3. 引入最小 `DataValue::Scalar(String) | DataValue::Struct(...)`，让 evaluator 输出结构化 value；runtime exporter 改为优先消费该结构。
4. 引入 `ResolvedDataSet` / `ResolvedExportView`：按 target tags 生成结构化视图，Validation 和 Exporter 不再直接解释 raw `DataRow`。
5. 将现有 validation 迁移为 resolved validation：required/type/enum/range/foreign key/unique 读取 `DataValue` 和 `ValueSourceTrace`。
6. 补集成 fixture：ObjectTable `key,value` nested expanded struct、同 named struct type 多 occurrence 分别使用 JSON/expanded、MessagePack shape。
7. 在 `DataValue` 稳定后再设计 array/map expanded path，例如 `items[0].id`、`rewards[gold]`。
