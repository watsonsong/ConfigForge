# 05. Data 模块详细设计

## 文档导航

Related docs：[`../Design/05-data-validation.md`](../Design/05-data-validation.md)、[`12-struct-expanded-value-detail-design.md`](12-struct-expanded-value-detail-design.md)、[`08-exporter-module-detail-design.md`](08-exporter-module-detail-design.md)。

- [5.1 设计目标](#51-设计目标)
- [5.4 核心数据结构](#54-核心数据结构)
- [5.5 加载计划结构](#55-加载计划结构)
- [5.6 CSV 读取器](#56-csv-读取器)
- [5.7 DataLoader](#57-dataloader)
- [5.8 与 Meta / Validation / Exporter 的流程关系](#58-与-meta--validation--exporter-的流程关系)
- [5.10 测试计划](#510-测试计划)


本文档描述 Data 模块的实现设计，用于把 Meta 中声明的 source 配置加载为独立的数据快照，并在 Meta 完成 Link 后把 raw source 数据解析为可供 Validation / Export 共享的结构化数据视图。产品语义以 `Documentation/Design/` 为准；本文档聚焦 Rust 模块边界、核心类型、主要方法、流程阶段和测试组织。

## 5.1 设计目标

Data 模块负责把外部数据源读取为 ConfigForge 内部统一的数据模型，并在 Data Load 后提供 Meta-aware resolver。核心分层结论：**Reader 保持 source-neutral；Resolver 负责 Meta-aware 结构化；Validation 和 Exporter 应逐步只消费结构化后的统一数据模型。**

当前已覆盖 CSV raw load，并已在不破坏 raw `DataSet` 的前提下引入 `ResolvedDataSet` / `DataResolver` MVP。后续目标是在 Validation / Exporter 中进一步收敛到 resolved view：

1. 支持从 `TableMeta.source` / `TableMeta.sources` 中解析单个 CSV source。
2. 支持按表加载 CSV，得到与 Meta 生命周期无关的 owned `DataSet`。
3. Data 加载阶段不依赖 `LinkedMetaModel<'meta>`，只依赖可拥有的加载计划，因此可以与 Meta Link / Meta Validate 在流程上并行。
4. 在 Meta Link 完成后，由 Data Resolver 使用 `LinkedMetaModel<'meta>` + raw `DataSet` 解析 JSON cell / expanded path / default / target visibility，得到结构化 `ResolvedDataSet` 或 target-specific resolved view；runtime export 主路径已消费 resolved value。
5. Validation 仍处于迁移中：source schema 与 value resolve 已通过 resolver 统一，required/type/index/FK 等主流程仍保留部分 raw `DataRow` 逻辑，后续应迁移到 `validate_resolved_*`。

当前阶段已实现 CSV source、DataSet、target-aware validation/export 的基础流程，并已按 **variant source mode** 支持 polymorphic fragments：polymorphic CollectionTable 可声明多个 CSV sources，每个 source 追加完整 records 并声明 active fragments。`override_sources` 已稳定支持 CSV CollectionTable 整行覆盖：按 primary index 匹配 base row，在匹配 effective target tags 时替换该 row，服务赛季/热修类临时覆盖。ObjectTable CSV multi-source / override 也已稳定支持：多个 base source 以 field path 互斥方式 split merge 为 singleton object，override source 以 exact field path patch 已存在字段。

仍不实现：

- fragment join mode；
- override insert 策略；
- 非 CSV provider；
- array / map 的 typed parse；
- source location 精确到单元格的诊断。

这些能力在 Meta validate 阶段应显式报 unsupported，避免 Data 阶段静默忽略。CollectionTable CSV `override_sources` 必须要求目标表拥有 primary index，且 override row 必须能匹配已存在 base row。ObjectTable CSV `sources` / `override_sources` 已进入 Stable，但仍不改变非 CSV provider、base source `export_when`、override insert/upsert 的 unsupported 状态。

设计判断：不建议把 Meta-aware 解析下沉到 `CsvDataReader`。CSV / Xlsx / Lark 等 reader 只负责语法读取、raw string 和 source location；struct source layout、default、target visibility、fragment active set 和 typed scalar parsing 都需要 linked meta 与 target context，更适合放在 Data Load 之后的 resolver 阶段。Validation / Exporter 应收敛为共享 resolver，避免两边重复解释 JSON 字符串和 dotted path。

variant source mode 下，`sources` 可以为同一 polymorphic collection table 声明多个 CSV source，但至少一个 source 必须声明非空 `fragments`，每个 source 必须贡献完整 record，不做按 ID 拼接。

标准参考游戏 fixture 暴露出的后续差距：材料、武器、防具是同一个 `items` 运行时表的三类 source variants，共享 `id/name/price` 并要求 primary index `by_id` 在合并后的 items 表中全局唯一；武器与防具都拥有 attributes fragment，且 attributes 的 shape 与 `AttributeSet` named struct 相同。当前实现可以通过 variant source mode 合并 `materials.csv` / `weapons.csv` / `armors.csv`，并在一个 `items` table 内声明 common fields 与 fragments；但仍缺少 field group / base table schema / typed fragment payload 复用机制。Data 层后续若支持 typed fragment payload，应让 fragment 引用标准 `[[structs]]` 作为 payload type，而不是新增专门的 fragment struct 系统。resolver 中把 fragment payload 当作以 fragment name 为 parent path 的 struct occurrence 解析，但 runtime payload 仍直接输出在 fragment object 下，不增加 wrapper field。

standard_project 作为参考游戏 fixture，当前采用工程总控 + 领域 meta 拆分：`ConfigForge.toml` 负责导入 `meta/items.toml`、`meta/characters.toml`、`meta/misc.toml` 并声明 client/server targets；领域 meta 文件只保存对应 tables / structs。数据 CSV 直接放在 `data/` 下，材料、武器、防具分别使用 `materials.csv`、`weapons.csv`、`armors.csv`，不再额外创建 `rpg/` 子目录，也不再保留额外的 `items.csv`。游戏语义中“职业”采用更常见的英文 `class`，因此表名和字段为 `classes`、`class_levels`、`class_id`，避免与现实职业 `profession` 混淆。

## 5.2 Rust 中的“类”和职责表达

Rust 不使用传统 OO class。本文档中的“类”对应 Rust 的 `struct`、`enum`、`trait` 与 `impl`：

| 设计概念 | Rust 表达 | 责任 |
| --- | --- | --- |
| DataSet | `struct DataSet` | 一次加载得到的全部表数据快照。 |
| DataTable | `struct DataTable` | 单张逻辑表的行集合。 |
| DataRow | `type DataRow = BTreeMap<String, String>` | 单行原始 cell 文本，key 为字段/列名。 |
| DataSourcePlan | `struct DataSourcePlan` | 单张表的数据加载计划，已解析 provider 和相对路径。 |
| SourceReadPlan | `struct SourceReadPlan` | raw source discovery 的 owned 结果，描述需要读取的物理 source 集合。 |
| RawSourceData | `struct RawSourceData` | source read 的 owned 输出，包装尚未 typed normalize 的 raw `DataSet`。 |
| DataLoadPlan | `struct DataLoadPlan` | 兼容层计划，当前包装 `SourceReadPlan`，便于旧 `DataLoader::load()` 调用继续返回 `DataSet`。 |
| SourceFragmentSet | `struct SourceFragmentSet` / `Vec<String>` | 某个 source 声明的 fragment 名称集合。 |
| RowSourceInfo | `struct RowSourceInfo` | 行来源元数据，记录 source ordinal/path/fragments/source row number。 |
| DataProvider | `enum DataProvider` | 当前支持的 provider，第一阶段只有 `Csv`。 |
| DataLoader | `struct DataLoader` | 根据 `SourceReadPlan` 读取 `RawSourceData`，并通过兼容入口支持 `DataLoadPlan -> DataSet`。 |
| SourceProviderResource | `enum SourceProviderResource` | provider 调度资源类型：`BlockingLocal` / `AsyncIo` / `Network`。 |
| AsyncSourceReadShell | `struct AsyncSourceReadShell` | 远端 provider 准备阶段的 source read shell，按 resource lane 限流并稳定合并结果。 |
| CsvDataReader | `struct CsvDataReader` | CSV provider 的具体读取器。 |
| DataResolver | `struct DataResolver` | 使用 linked meta 将 raw DataSet 解析为 ResolvedDataSet / target resolved view。 |
| ResolvedDataSet | `struct ResolvedDataSet` | Meta-aware 的结构化数据快照，隐藏 JSON cell / expanded path 的 source 差异。 |
| DataValue | `enum DataValue` | 结构化值，表达 typed scalar、nested struct 与 JSON cell array，未来扩展 map/path array。 |

## 5.3 建议模块结构

当前已采用目录模块 `src/data/` 承载 CSV 读取、数据加载计划、逻辑表模型和 resolved view：

```text
src/data/
  mod.rs          # 对外 API 与 re-export
  model.rs        # DataSet / DataTable / DataRow / RawSourceData / 查询方法
  resolved.rs     # ResolvedDataSet / ResolvedTable / DataValue / SourceTrace
  resolver.rs     # DataResolver / StructOccurrenceEvaluator
  plan.rs         # SourceReadPlan / DataLoadPlan / DataSourcePlan / DataProvider
  loader.rs       # DataLoader，编排按计划加载各表
  provider.rs     # SourceProvider / AsyncSourceReadShell / provider resource lane
  csv.rs          # CsvDataReader，CSV 文件读取实现
  tests.rs        # Data 模块单元测试
```

迁移状态：

- `DataRow`、`DataTable`、`DataSet`、`SourceReadPlan`、`RawSourceData`、`DataLoadPlan`、`DataLoader` 等已由 `src/data/mod.rs` 统一 re-export。

## 5.4 核心数据结构

### 5.4.1 `DataSet`

```rust
pub struct DataSet {
    pub tables: Vec<DataTable>,
    table_index: HashMap<String, usize>,
}
```

说明：

- `DataSet` 是 Data Load 的 owned 输出，不借用 `MetaModel` 或 `LinkedMetaModel<'meta>`。
- `table_index` 是运行期查询索引，便于按 table name 快速取得数据。
- 第一阶段 `DataSet` 不保存 typed value，只保存 CSV cell 原始字符串。

主要方法：

```rust
impl DataSet {
    pub fn new(tables: Vec<DataTable>) -> Self;
    pub fn table(&self, name: &str) -> Option<&DataTable>;
    pub fn tables(&self) -> &[DataTable];
}
```

### 5.4.2 `DataTable`

```rust
pub struct DataTable {
    pub name: String,
    pub rows: Vec<DataRow>,
    pub row_sources: Vec<RowSourceInfo>,
}
```

说明：

- `name` 对应 `TableMeta.name`。
- `rows` 是 CSV 读取后的行集合。
- `row_sources` 与 `rows` 等长，记录每行来自哪个 physical source。非 polymorphic 单 source 表也应填充，便于统一诊断。
- 每个 row 的 key 来自 CSV header；fragment field 在 DataRow 中保留 `fragment.field` 形式，例如 `stats.attack`；struct source layout 自动判定相关的 child field path 也保留 `field.child` 形式，例如 `attributes.attack`。Data 模块不在读取阶段展开为嵌套结构。
- 第一阶段不在 Data Load 阶段判断 header 是否匹配 Meta 字段；该判断放到 Validation 阶段。

主要方法：

```rust
impl DataTable {
    pub fn new(name: impl Into<String>, rows: Vec<DataRow>) -> Self;
    pub fn new_with_sources(
        name: impl Into<String>,
        rows: Vec<DataRow>,
        row_sources: Vec<RowSourceInfo>,
    ) -> Result<Self>;
    pub fn row_count(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

`new()` 用于测试、内存构造和非 source 数据构造，会自动为每行填充默认 `RowSourceInfo`：`source_index = 0`、`source_path = "<memory>"`、`source_row_number = row_index + 1`、`fragments = []`。`new_with_sources()` 用于真实 DataLoader，必须校验 `rows.len() == row_sources.len()`，不一致时返回 validation error。

### 5.4.3 `RowSourceInfo`

variant source mode 需要保留每行的 source fragment set，否则 validation 无法判断该行应该应用哪些 fragment fields。

```rust
pub struct RowSourceInfo {
    pub source_index: usize,
    pub source_path: PathBuf,
    pub source_row_number: usize,
    pub fragments: Vec<String>,
    pub source_role: SourceRole,
    pub replaces: Option<RowSourceTrace>,
}

pub enum SourceRole {
    Base,
    Override,
}

pub struct RowSourceTrace {
    pub source_index: usize,
    pub source_path: PathBuf,
    pub source_row_number: usize,
}
```

说明：

- `source_index` 对应 `DataSourcePlan.sources` 中该 table 的 source 顺序，用于稳定诊断和后续 merge 规则。
- `source_path` 用于错误信息，例如 `items/weapons.csv`。
- `source_row_number` 固定为 record number，从 1 开始，不包含 CSV header；这与当前 validation 中 `row_index + 1` 的错误信息语义保持一致。未来如果需要显示真实文件行号，应新增 `source_line_number: Option<usize>`，不要复用 `source_row_number`。
- `fragments` 来自 `SourceRef.fragments`。Data 模块只保留名称；是否存在、字段 schema、target 过滤由 Meta Link / Validation 处理。
- `source_role` 区分 base source 与 override source。最终 `DataTable.rows` 中保留的是 materialized 后的有效 row；被 override 替换掉的 base row 不再作为独立 row 参与 validation/export。
- `replaces` 用于保留被覆盖 base row 的来源。推荐：最终 row 的 `source_path/source_row_number/fragments/source_role` 指向实际生效的 override row，`replaces` 记录原 base row trace，便于诊断“该 S1 行来自 materials_s1.csv，并替换了 materials.csv 的 row N”。

如果短期不想修改 `DataTable` public shape，也可以先引入：

```rust
pub struct DataRecord {
    pub row: DataRow,
    pub source: RowSourceInfo,
}
```

然后将 `DataTable.rows` 从 `Vec<DataRow>` 演进为 `Vec<DataRecord>`。考虑当前代码大量使用 `table.rows`，更小改动是保留 `rows` 并新增并行 `row_sources`，但需要测试保证长度一致。

### 5.4.4 `DataRow`

```rust
pub type DataRow = BTreeMap<String, String>;
```

说明：

- 使用 `BTreeMap` 保持稳定序列化和测试输出。
- 值保持原始字符串。Raw DataRow 是 source-neutral 的事实层，不表达 typed value、默认值、target visibility 或 nested object。
- 后续 resolver 可在此基础上生成结构化值；Validation / Exporter 不应长期直接从 raw `DataRow` 重复解释 struct JSON cell 和 dotted path。

### 5.4.5 `ResolvedDataSet` / `DataValue`（已实现 MVP，继续迁移）

`ResolvedDataSet` 是 raw `DataSet` 之后的 Meta-aware 解析结果。它不替代 raw `DataSet`，而是与 raw source snapshot 并存：raw 层用于诊断、inspect、debug、source diff；resolved 层用于 validation/export 共享同一套结构化语义。

建议类型：

```rust
pub struct ResolvedDataSet {
    pub tables: Vec<ResolvedTable>,
    table_index: HashMap<String, usize>,
}

pub struct ResolvedTable {
    pub name: String,
    pub rows: Vec<ResolvedRow>,
    pub row_sources: Vec<RowSourceInfo>,
}

pub type ResolvedRow = BTreeMap<String, ResolvedValue>;

pub struct ResolvedValue {
    pub value: DataValue,
    pub source: ValueSourceTrace,
}

pub struct ArrayElementValue {
    pub value: DataValue,
    pub raw_value: Option<String>,
}

pub enum DataValue {
    Scalar(ScalarValue),
    Struct(BTreeMap<String, ResolvedValue>),
    Array(Vec<ArrayElementValue>),
}

pub enum ScalarValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Enum(String),
}
```

`ValueSourceTrace` 至少需要表达：

- raw field path，例如 `attributes.attack`；
- source kind：explicit cell、default、computed/empty object；
- source row / source path；
- 对 ObjectTable，cell source 已记录 key-level source path / row number；field-level validation 应优先使用 cell source，singleton logical row source 仅作为 fallback trace。

Resolved 后，无论 source 是：

```csv
id,attributes
hero,"{""attack"":10,""max_health"":100}"
```

还是：

```csv
id,attributes.attack,attributes.max_health
hero,10,100
```

Validation / Exporter 都看到统一结构：

```rust
{
    "id": Scalar(String("hero")),
    "attributes": Struct({
        "attack": Scalar(Int(10)),
        "max_health": Scalar(Int(100)),
    }),
}
```

Resolver 职责：

1. 使用 linked meta 解析 primitive / enum / struct / list；
2. 判定 struct occurrence 是 JSON cell、expanded value 还是 missing；
3. 检查 JSON/expanded 混用、unknown dotted path、inactive fragment；
4. 应用 scalar / enum / list / struct child default；
5. 按 target tags 生成 target-specific resolved view，或在 `ResolvedValue` 中保留 visibility 信息供 ExportView 裁剪；
6. 保留 raw string 和 source trace 以生成精确诊断。

Resolver 内部通过只读 `ResolveContext` / `ValueContext` 传递当前 table、row、row source、source row number、target tags 与 field path。各层 helper 不再反复扩展 `table_name` / `row_number` / `row_source` / `target_tags` 参数列表；当递归进入 struct child、fragment field 或未来 expanded list/map path 时，只需要派生新的 `ValueContext`，诊断上下文、source trace 与 target visibility 会沿用同一入口。

Typed value 解析规则集中在 `src/data/typed_value.rs`：

- `parse_scalar_field_value()` / `parse_scalar_for_type()` 负责 primitive 与 enum 的字符串单元格到 `ScalarValue` 转换；bool、finite float、enum value membership 等规则只在这里定义。
- `parse_list_cell_value()` 负责把 list JSON cell 解析为 JSON array 后委托 `parse_list_json_value()`；后者逐元素复用 scalar 转换规则，并在错误发生时按需生成 `field[index]` 形式的诊断路径。成功解析后的 scalar list element 使用 `ArrayElementValue` 保存 `DataValue + raw_value`，不再为每个元素常驻完整 `ResolvedValue`、source trace 和 duplicated field path string。
- `parse_color_field_value()` 负责 `color` builtin value 的 source cell 解析，输入为 `ColorFieldOptions` 和 raw cell。hex 格式解析 `#RRGGBB` / `#RRGGBBAA`；tuple 格式接受逗号分隔或 JSON array；object 格式接受 JSON object。解析后生成 canonical RGBA 值，并保留 source channel、color space 和 raw string 以便诊断与文档输出。`color_channel = "byte"` 时数值范围为 `0..255`；`normalized` 时范围为 `0.0..1.0`；`alpha = "default_opaque"` 时可补默认 alpha，否则缺失 alpha 报错。
- `parse_vector_field_value()` 负责 `vector2` / `vector3` / `vector4` / `vector*_int`。tuple / array / object 由单 cell 解析；`vector_format = "expanded"` 由 resolver 在 source schema 阶段把 `field.x`、`field.y`、`field.z` 等列或 ObjectTable key 行收集成一个逻辑值，再委托 typed helper 做维度、finite float、整数性和缺失组件检查。
- `DataResolver`、struct value validation 和 list foreign-key validation 共用这些 helper。默认值是否生效、空值是否跳过由调用方基于字段语义决定；一旦需要解析 typed scalar/list element，必须进入该共享路径。

Color / Vector 的 `DataValue` 规划形态：

```rust
pub enum DataValue {
    Scalar(ScalarValue),
    List(Vec<ArrayElementValue>),
    Struct(BTreeMap<String, ResolvedValue>),
    Color(ColorValue),
    Vector(VectorValue),
}

pub struct ColorValue {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
    pub channel: ColorChannel,
    pub color_space: ColorSpace,
}

pub enum VectorValue {
    Float { components: Vec<f64> },
    Int { components: Vec<i64> },
}
```

Runtime data exporter 的 canonical 输出策略应由 data artifact toolkit 统一完成，而不是各 exporter 重新解析 raw cell：JSON / MessagePack 第一阶段建议输出 object shape（Color：`r/g/b/a`；Vector：按 components 输出 `x/y/z/w` 或用户指定 component 名），`bin` 使用 schema-aware positional layout。是否把 Color byte 通道导出为 normalized float 是 export policy；解析层只保证 raw source 合法并保存 canonical numeric value。

当前实现状态与差距：

- 已实现 `ResolvedDataSet` / `DataResolver` 基础类型、shared typed scalar parse、shared list JSON cell parse、struct JSON cell / expanded value 统一解析、source schema 校验、target-aware row resolve、active fragment resolve、runtime export 主路径消费 resolved value。
- 过渡中：Validation 仍保留 raw `DataRow` 上的 required/type/enum/index/FK 主流程，但 type/enum/list element/FK list element conversion 已复用 data typed value helper；后续应迁移到 `validate_resolved_*` 让更多规则直接消费 `ResolvedValue/DataValue`。
- 未实现：typed fragment payload（例如 `attributes` fragment 直接复用 `AttributeSet`）、field group/base table schema 复用、asset existence/text POT special export、expanded array/map path。

注意：是否在 resolver 阶段把 scalar parse 成 `Int/Float/Bool` 需要结合现有 runtime scalar string contract 决定。短期可以先在 resolver 内保留 typed parse 结果与 raw string，runtime export 仍输出 string；长期如果 runtime artifact 改为 typed scalar，可直接复用 `ScalarValue`。

## 5.5 加载计划结构

### 5.5.1 `DataProvider`

```rust
pub enum DataProvider {
    Csv,
}
```

主要方法：

```rust
impl DataProvider {
    pub fn parse(value: &str) -> Option<Self>;
    pub fn as_str(&self) -> &'static str;
}
```

第一阶段只接受 `csv`。非 `csv` provider 应在 Meta validate 或 SourceReadPlan 构造阶段返回错误，不进入读取器分发。

### 5.5.2 `DataSourcePlan`

```rust
pub struct DataSourcePlan {
    pub table_name: String,
    pub provider: DataProvider,
    pub path: PathBuf,
    pub header_row: usize,
    pub data_start_row: usize,
    pub fragments: Vec<String>,
    pub layout: DataSourceLayout,
    pub fields: Vec<String>,
    pub role: SourceRole,
    pub primary_key_fields: Vec<String>,
}

pub enum DataSourceLayout {
    CollectionRecords,
    ObjectKeyValueRows,
}
```

说明：

- `table_name` 是目标逻辑表名。
- `path` 是相对 data root 的路径，来自 `SourceRef.path` 或 legacy `TableMeta.source`。
- `header_row` / `data_start_row` 是从 `SourceRef` 继承的 1-based CSV 物理行布局；CollectionTable 默认 `header_row = 1`、`data_start_row = 3`，ObjectTable 默认 `header_row = 1`、`data_start_row = 2`，因此可为策划标题行/标题列留出空间。
- `fragments` 是该 source 声明的 polymorphic fragment 名称；普通表和 legacy `source` 为空。
- `layout` 描述 reader 物理布局：CollectionTable 使用 `CollectionRecords`，ObjectTable 使用 `ObjectKeyValueRows`。
- `fields` 是 ObjectTable 的 top-level field 名称快照，用于后续 key 校验和 source trace 增强；CollectionTable 可为空。
- `role` 区分 base `source` / `sources` 与 `override_sources`。
- `primary_key_fields` 是当前 table primary index 的字段列表快照，仅在 `role = Override` 时用于 DataLoader 定位被覆盖 row；base source 可为空或携带同一份信息以便统一构造 per-table key map。字段路径保持 raw source header 使用的 field path 字符串，例如 `id` 或未来复合 key `region,id` 对应 `Vec<["region", "id"]>`。
- variant source mode 下，polymorphic collection table 可以拥有多个 `DataSourcePlan`；ObjectTable 可以拥有多个 `ObjectKeyValueRows` base source 并 split merge 为 singleton object。为了避免把普通 CollectionTable append mode 与未来通用 multi-source merge 混淆，CollectionTable 多 source 必须由至少一个 source 的非空 `fragments` 声明驱动。

### 5.5.3 `SourceReadPlan` / `DataLoadPlan`

```rust
pub struct SourceReadPlan {
    sources: Vec<DataSourcePlan>,
}

pub struct DataLoadPlan {
    source_read_plan: SourceReadPlan,
}
```

主要方法：

```rust
impl SourceReadPlan {
    pub fn new(sources: Vec<DataSourcePlan>) -> Self;
    pub fn from_meta_model(model: &MetaModel) -> Result<Self>;
    pub fn from_tables<'a>(tables: impl IntoIterator<Item = &'a TableMeta>, target_tags: Option<&TargetTags>) -> Result<Self>;
    pub fn sources(&self) -> &[DataSourcePlan];
}

impl DataLoadPlan {
    pub fn new(source_read_plan: SourceReadPlan) -> Self;
    pub fn from_meta_model(model: &MetaModel) -> Result<Self>;
    pub fn from_tables<'a>(tables: impl IntoIterator<Item = &'a TableMeta>, target_tags: Option<&TargetTags>) -> Result<Self>;
    pub fn source_read_plan(&self) -> &SourceReadPlan;
    pub fn sources(&self) -> &[DataSourcePlan];
}
```

设计要点：

- `SourceReadPlan` 从 raw `MetaModel` / `TableMeta` 中提取 source 信息，不需要 `LinkedMetaModel<'meta>`。
- 一旦 `SourceReadPlan` 构造完成，就拥有 table name、provider、path，后续 source read 不再借用 Meta。
- `DataLoadPlan` 当前作为兼容层包装 `SourceReadPlan`；旧调用可以继续使用 `DataLoadPlan::from_*` 与 `DataLoader::load()`，新调度应优先使用 `SourceReadPlan` + `DataLoader::read_sources*()` 表达 raw source read 节点。
- variant source mode 下，`SourceReadPlan` 需要展开 table 的每个 `SourceRef`，为同一 `table_name` 生成多个 `DataSourcePlan`，并保留 source 声明顺序。
- `override_sources` 展开为 `role = Override` 的 `DataSourcePlan`，并排在该 table 的 base sources 之后；多个 override source 保持声明顺序。`target_tags` 存在时，`export_when` 不匹配的 override source 不进入 plan，因此不会被读取。
- `SourceReadPlan::from_tables` 只做 source declaration 的轻量契约检查和搬运：provider 必须支持；CollectionTable 多个 sources 只允许 `TableMeta::is_variant_source_mode()` 为 true 的表，ObjectTable 多个 sources 则生成 `ObjectKeyValueRows` split-merge 计划；base source 不允许声明 `export_when`。fragment 是否存在、字段是否属于 fragment 等 schema 合法性由 Meta validation/link 负责。
- CollectionTable override source 必须有可用于匹配的 primary index。由于 `SourceReadPlan` 只持有 raw `TableMeta`，它需要根据 `primary_index` 在 `TableMeta.indexes` 中找到对应 index 并复制 `fields` 到 `primary_key_fields`；primary index 是否唯一、字段是否存在由 Meta link/validation 负责，但 SourceReadPlan 找不到 index 声明时也应 fail-fast。
- 非 CSV override source、以及缺 primary index 的 CollectionTable override source 必须 fail-fast，不能静默忽略；ObjectTable override source 不需要 primary index，而是以 exact field path patch 已存在字段。
- provider 是否支持属于 source declaration 的基础结构检查，可以由 Meta validation fail-fast，也可以由 SourceReadPlan parse 时返回错误；除 provider parse 以外，SourceReadPlan 不应理解 fragment schema 语义。
- 因此流程上可以在 raw Meta load/merge 完成后：
  - 一边做 `MetaModel::link()` / Meta validate；
  - 一边执行 `DataLoader::read_sources(&source_read_plan)`。

## 5.6 CSV 读取器

### 5.6.1 `CsvDataReader`

```rust
pub struct CsvDataReader;
```

主要方法：

```rust
impl CsvDataReader {
    pub fn read_table(table_name: impl Into<String>, path: &Path) -> Result<DataTable>;
    pub fn read_table_with_layout(
        table_name: impl Into<String>,
        path: &Path,
        header_row: usize,
        data_start_row: usize,
    ) -> Result<DataTable>;
    pub fn read_object_table(
        table_name: impl Into<String>,
        path: &Path,
        fields: &[String],
    ) -> Result<DataTable>;
    pub fn read_object_table_with_layout(
        table_name: impl Into<String>,
        path: &Path,
        fields: &[String],
        header_row: usize,
        data_start_row: usize,
    ) -> Result<DataTable>;
}
```

读取规则：

1. 使用 `csv::ReaderBuilder::new().has_headers(false)` 打开文件，CSV crate 不自动消费 header。
2. `header_row` / `data_start_row` 均为 1-based，且必须非 0、`data_start_row > header_row`。CollectionTable 默认 `header_row = 1`、`data_start_row = 3`，第 2 行通常留作标题说明；可由每个 table source 的 `source = { path = "...", header_row = ..., data_start_row = ... }` 覆盖。
3. `header_row` 指定的 record 作为 header；`header_row` 与 `data_start_row` 之间的行会被跳过，不进入 `DataTable.rows`。
4. 从 `data_start_row` 开始，每条 record 与 header zip 成 `DataRow`；header 原样保留，包含 `stats.attack` 这类 fragment field path，以及 `attributes.attack` 这类 struct field path。
5. 缺失 cell 当前不在读取阶段补齐；Validation 阶段按 required/default 处理。
6. 多余 cell 当前由 csv crate 的 record 行为处理；后续如果需要严格列数，可在 Data 模块增加 header/record 长度校验。
7. CsvDataReader 不理解 fragment schema；它只返回原始 row。DataLoader 在组装 DataTable 时为每行附加 `RowSourceInfo`，其中 `source_row_number` 使用真实 CSV 物理行号。

当前 standard_project 参考游戏表格采用两种 source pattern：

- 普通 collection：`classes.csv`、`class_levels.csv` 使用单 source，`class_levels` 通过 `attributes.max_health` 等 header 表达 expanded struct。
- variant source collection：`materials.csv` / `weapons.csv` / `armors.csv` 都是同一个 `items` table 的 source variants。DataLoader 按 source 声明顺序把它们 append 到单张 `items` DataTable，并为每行记录 fragment presence；`by_id` unique/primary 校验在合并后的整张 items 表上执行，因此三类道具 ID 必须全局不重复。

规划中的第三种 pattern 是 source-order collection：例如 `character_surnames.csv`、`character_male_given_names.csv`、`character_female_given_names.csv` 作为随机角色名字池，每张表只有一个 `string value` 字段、无 index，并在 Meta 中声明 `preserve_source_order = true`。DataLoader 不需要特殊读取逻辑，仍按 CSV 行顺序形成 `DataTable.rows`；语义差异发生在 codegen：这类表显式生成 row count / row_at / rows 顺序访问，业务用外部 RNG 随机选择下标。普通 indexed table 则默认不再通过 generated public API 暴露 `rows` 顺序数组。

variant source mode 要求每个 source 贡献完整 record：common fields 必须在每个 source 中完整出现，不做跨 source join；fragment fields 只允许出现在声明了对应 active fragment 的 source 中。

### 5.6.2 ObjectTable `key,value` / `key,title,value` source layout

ObjectTable Step 1 的 source 过程文档已并入 Data 模块。当前 `kind = "object"` 的 table 使用 CSV `key,value` 或 `key,title,value` 行式布局；标准工程推荐三列形式，把第二列作为面向策划的标题/说明列，读取时忽略：

```csv
key,title,value
max_level,最大等级,100
start_gold,初始金币,500
economy,经济参数,"{""gold_rate"":""1.2"",""drop_rate"":""0.8""}"
```

读取规则：

1. header 必须恰好两列或三列，trim 后分别为 `key,value` 或 `key,title,value`。
2. 每个数据行第一列是 top-level field key 或后续支持的 field path；最后一列是 raw value。三列布局中的 `title` 列只供人类阅读，不参与 `DataRow`。
3. `key` 不能为空。
4. 同一个 ObjectTable source 中 key 不能重复。
5. `key` 必须能被当前 table schema 识别；unknown key 应失败而不是静默忽略。
6. 默认 `header_row = 1`、`data_start_row = 2`；如果 source 配置覆盖行号，仍按同样的 1-based 语义读取。
7. 只有 header、没有数据行时，也必须产出一个 empty logical row。
8. ObjectTable source 归一化后始终是一个 logical row：`DataTable.rows.len() == 1`。

归一化示例：

```text
CSV rows:
  key=max_level, title=最大等级, value=100
  key=start_gold, title=初始金币, value=500

DataTable rows:
  [ { "max_level": "100", "start_gold": "500" } ]
```

这样做的原因：

- required/default/type/enum validation 可以复用普通 row 校验逻辑。
- exporter 可以在 `TableKind::Object` 分支上选择 object payload shape。
- codegen IR 仍只需要识别 table kind，不需要理解 CSV 物理布局。

当前限制与 Stable 语义：

- ObjectTable `source = "...csv"` 保持兼容；`sources = [...]` 表示多个 CSV key/value source 为同一个 singleton object 提供互斥 field path。base source 间重复 field path 报错，并提示改用 `override_sources` 表达替换。
- ObjectTable `override_sources = [...]` 是 field-path-level patch：只替换 override source 中出现的 exact field path，不隐式删除 sibling path，也不新增 base 中不存在的 field path；找不到 target field path 时 fail-fast。
- ObjectTable 不支持 fragments / row_export / indexes；ObjectTable 可作为 foreign key source，归一化后的 singleton logical row 会进入 FK 校验，但不能作为 foreign key target。相关限制由 Meta / Validation 模块维护。
- ObjectTable key-level source trace 是 Stable 关键要求：field-level validation 应优先使用 cell source，使被 override 字段定位到 override source 的真实 key row；singleton row source 只能作为 fallback trace。

错误映射：

- 文件无法打开、CSV parse 失败统一映射为 `ConfigForgeError::ParseCsv { path, source }`。
- source declaration 缺失或 provider 不支持可以映射为 `ConfigForgeError::Validation`，中期应迁移到结构化 Data diagnostic。

## 5.7 DataLoader

```rust
pub struct DataLoader {
    data_root: PathBuf,
}
```

主要方法：

```rust
impl DataLoader {
    pub fn new(data_root: impl Into<PathBuf>) -> Self;
    pub fn read_sources(&self, plan: &SourceReadPlan) -> Result<RawSourceData>;
    pub fn read_sources_with_pool(&self, plan: &SourceReadPlan, pool: &rayon::ThreadPool) -> Result<RawSourceData>;
    pub fn load(&self, plan: &DataLoadPlan) -> Result<DataSet>;
    pub fn load_with_pool(&self, plan: &DataLoadPlan, pool: &rayon::ThreadPool) -> Result<DataSet>;
    pub fn load_source(&self, source: &DataSourcePlan) -> Result<DataTable>;
}
```

设计要点：

- `DataLoader` 不持有 Meta 引用。
- `read_sources()` / `read_sources_with_pool()` 读取 `SourceReadPlan` 并返回 `RawSourceData`，这是后续 typed normalize 的输入。
- `load()` / `load_with_pool()` 是兼容入口，内部委托 `read_sources*()` 并解包为旧的 `DataSet` 返回值。
- `read_sources_with_pool()` 可并行加载多个 source，但组装 DataSet 时必须保持 `SourceReadPlan.sources()` 声明顺序；即使 source 完成顺序不同，merge 和错误选择也按 plan index 稳定处理。
- 当前本地同步 provider 的并发度由调用方传入的 Rayon pool 决定；build 主流程用 `ExecutionOptions.jobs` 创建该 pool，`--serial` 已在 options normalize 阶段退化为 `jobs = 1`。
- `load_source()` 根据 `DataProvider` 分发到 `CsvDataReader`。
- 对同一 `table_name` 的多个 `DataSourcePlan`，DataLoader 需要把 rows append 到同一个 `DataTable`，并同步 append 对应 `RowSourceInfo`。
- `DataSet::try_new()` 仍应拒绝重复 table name；多 source 合并必须发生在 DataLoader 内部，而不是先生成多个同名 DataTable 再交给 DataSet。

### 5.7.1 Variant source merge 与 override 流程

variant source mode 的 DataLoader 组装流程：

```text
for source in SourceReadPlan.sources() in declaration order:
  table = read source.path as DataTable(source.table_name)
  if source.role == Base:
    for each row in table.rows:
      append row to pending_tables[source.table_name].rows
      append RowSourceInfo { source_index, source_path, source_row_number, fragments, source_role: Base } to row_sources
  if source.role == Override:
    for each row in table.rows:
      key = primary index key extracted from override row by source.primary_key_fields
      target_row = pending_tables[source.table_name].primary_key_index[key]
      replace pending_tables[source.table_name].rows[target_row] with override row
      replace row_sources[target_row] with RowSourceInfo { source_index, source_path, source_row_number, fragments, source_role: Override, replaces: previous row trace }

DataSet::try_new(pending_tables in first-seen table order)
```

注意：

- DataLoader 不做 primary key 去重；unique/primary index 冲突仍在 Validation 阶段基于 normalized DataTable 检查。
- 例外：为了应用 CollectionTable override，DataLoader 需要按 `primary_key_fields` 构造临时 key map。该 map 只用于定位 override 目标，不替代 Validation 的 unique/primary index 检查。base sources 中 duplicate primary key 仍应在 Validation 阶段报错；如果 duplicate key 导致 override 匹配到多个 base row，DataLoader 应报 ambiguous override 或依赖后续 validation fail-fast，推荐直接报错以避免覆盖不确定。
- Override row 必须提供 primary index 字段且字段值非空；缺失时报 Data/Validation error，错误包含 table、override source path、row number 和 index name。
- Override row 找不到 base key 时报错，不 append 新 row。未来如果要允许 insert，必须新增显式策略，例如 `on_missing = "insert"`，不能改变默认语义。
- DataLoader 不做 fragment field required/default；Validation 阶段根据 `row_sources[row_index].fragments` 决定应用哪些 fragment schemas。
- 如果某个 source 读取失败，错误需要包含 source path；如果后续引入 source ordinal，可在 Data diagnostic 中补充。
- ObjectTable base merge 需要把多个 singleton row 合并为一个 singleton object；base source 间重复 field path 应报错，并提示把替换值放到 `override_sources`。
- ObjectTable override merge 需要以 exact field path 替换已存在 value 与 cell source；找不到 base field path 应报错，避免把 patch source 误用为 append/upsert。

S1 Oak 覆盖示例在 DataLoader 中的 materialization 结果：

1. `materials.csv` 先 append `id = 2002, name = Oak Wood, price = 3, max_stack = 999`，fragments 为 `["stack"]`。
2. 构建 effective target tags 不包含 `s1` 时，`materials_s1.csv` 不进入 `SourceReadPlan`，最终 row 保持 price 3。
3. 构建 effective target tags 包含 `s1` 时，`materials_s1.csv` 进入 plan；其唯一 row 用 primary index `by_id(id=2002)` 匹配 base row，并整行替换为 price 5。
4. 替换后的 row 再进入后续 normalize / validation / exporter；因此 unique index、FK、row_export 都只看到 S1 的最终视图。

### 5.7.2 Async provider shell 与 resource lane

Phase 4 引入 `SourceProvider` / `AsyncSourceReadShell` 作为远端 provider 的调度外壳，但不改变本地 CSV 主路径，也不强制引入 Tokio：

```rust
pub enum SourceProviderResource {
    BlockingLocal,
    AsyncIo,
    Network,
}

pub struct SourceProviderConcurrency {
    pub blocking_local: usize,
    pub async_io: usize,
    pub network: usize,
}

pub trait SourceProvider: Sync {
    fn resource(&self, source: &DataSourcePlan) -> SourceProviderResource;
    fn read_source(&self, source: &DataSourcePlan) -> Result<DataTable>;
}

pub struct AsyncSourceReadShell { ... }
```

设计约束：

- `SourceReadPlan` 仍是 provider-independent 输入；provider 不允许隐式读取 `LinkedMetaModel`。
- resource lane 用于区分本地阻塞 I/O、未来 async I/O、远端网络 source。Google Sheet / 飞书 provider 应声明为 `Network`，并使用 `network` concurrency lane。
- shell 按 plan index 稳定合并结果和选择错误；即使后面的 source 更早失败，也应返回最早 plan index 的错误。
- 当前 shell 在遇到错误后停止调度尚未启动的 source，并等待已启动 source 完成；Phase 5 可在此基础上扩展 cancellation token。
- 远端 provider 策略由 `RemoteProviderPolicy` 预留，包括 retry、base backoff 与 rate-limit。认证错误、source location diagnostics、cell/bytes 下载后的 CPU parse 切换，应在正式 provider 实现时落地。
- 本阶段不在 async worker 上做 typed normalize；normalize 仍在 raw source read 完成后进入 CPU 阶段。

## 5.8 与 Meta / Validation / Exporter 的流程关系

### 5.8.1 当前建议执行流程

```text
MetaModel::from_path(meta_path)
  -> raw MetaModel owns source declarations
  -> SourceReadPlan::from_meta_model(&meta_model)

阶段 A：Meta link / validate
  MetaModel::link() -> LinkedMetaModel<'meta>

阶段 B：Data load
  DataLoader::new(data_root).read_sources(&source_read_plan) -> RawSourceData

阶段 C：Data resolve
  normalize_data_set(&linked_meta, raw_sources.as_data_set()) -> normalized DataSet
  DataResolver::new(&linked_meta).resolve(normalized_data_set, target_tags?) -> ResolvedDataSet / ResolvedExportView

阶段 D：Data validation
  validate_resolved_data_set(&linked_meta, &resolved_data_set)

阶段 E：Export
  exporter consumes ResolvedDataSet / ResolvedExportView
```

当前计划是以 `SourceReadPlan -> DataLoader -> RawSourceData -> normalize_data_set -> DataResolver -> ResolvedDataSet` 作为 Data 模块主链路。`DataLoadPlan` 仍保留为兼容 facade。顺序执行示例：

```rust
let meta = MetaModel::from_path(meta_path)?;
let source_read_plan = SourceReadPlan::from_meta_model(&meta)?;
let linked_meta = meta.link().map_err(ConfigForgeError::MetaValidation)?;
let raw_sources = DataLoader::new(data_root).read_sources(&source_read_plan)?;
let normalized = normalize_data_set(&linked_meta, raw_sources.as_data_set())?;
let resolved = DataResolver::new(&linked_meta).resolve(&normalized, None)?;
validate_resolved_data_set(&linked_meta, &resolved)?;
```

未来并行化时，`linked_meta` 和 `data_set` 的构造可以拆成两个任务，因为：

- `SourceReadPlan` 是 owned；
- `DataLoader` 不借用 Meta；
- raw `DataSet` 不保存 Meta 引用；
- `DataResolver` 才需要同时拿到 `LinkedMetaModel<'meta>` 和 raw `DataSet`。

### 5.8.2 Resolver 分阶段迁移路线

不建议一次性替换现有 Validation / Exporter。推荐分阶段落地：

**Phase 1：抽 StructOccurrenceEvaluator。**

把当前 struct JSON cell / expanded path / child default / target-aware visibility / empty cell occurrence 判断从 validation 与 exporter 中抽为共享 helper：

```rust
pub struct StructOccurrenceEvaluator<'meta, 'data> {
    table: &'meta LinkedTableMeta<'meta>,
    row: &'data DataRow,
    target_tags: Option<&'data TargetTags>,
}

pub struct ResolvedStructOccurrence<'meta> {
    pub path: String,
    pub layout: StructSourceLayout,
    pub has_effective_value: bool,
    pub children: BTreeMap<String, ResolvedValue>,
    pub diagnostics: Vec<DataDiagnostic>,
    pub field: &'meta LinkedFieldMeta<'meta>,
}
```

Validation 使用 evaluator 结果做 required/type/enum/validator 报错；Exporter 使用同一个结果构建 `ExportedValue::Struct`。这一步优先解决 default/export 不一致、空 expanded cell 语义、target-hidden child 影响 occurrence 等问题。

**Phase 2：沉淀通用 `DataValue`。**

将 evaluator 的输出从 struct-only 扩展为 `DataValue::Scalar` / `DataValue::Struct`，并让 runtime exporter 优先消费 `DataValue`。

**Phase 3：Validation 迁移到 `ResolvedDataSet`。**

Validation 不再直接读 raw `DataRow`，而是读取 resolved row。RawDataSet 仍保留用于 source diagnostic、inspect、debug、reload 和 diff。

**Phase 4：支持 array/map 与更多 source path。**

等 `DataValue` 稳定后，再扩展 `items[0].id`、`rewards[gold]` 等 array/map path，避免 validation/export 各自实现 path parser。

### 5.8.3 Validation API 调整

当前已有单表校验：

```rust
pub fn validate_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()>;
```

Data 模块落地后已有数据集级入口。引入 resolved 层后建议增加 resolved validation 入口，并保留 raw validation 作为兼容 facade：

```rust
pub fn validate_data_set(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<()>;
pub fn validate_resolved_data_set(
    meta: &LinkedMetaModel<'_>,
    data_set: &ResolvedDataSet,
) -> Result<()>;
```

长期职责：

1. 遍历 `meta.tables`。
2. 在 `ResolvedDataSet` 中按 table name 查找 `ResolvedTable`。
3. 缺失表时报错。
4. 对每张表调用 resolved table validation。
5. required/type/enum/range 等规则读取 `DataValue` 和 source trace，不再重复解析 raw JSON / dotted path。

### 5.8.4 Exporter API 调整

当前 exporter 内部按表直接读取 CSV。Data 模块落地后应调整为：

```rust
pub fn validate_project(meta_path: &Path, data_dir: &Path) -> Result<()> {
    let meta = MetaModel::from_path(meta_path)?;
    let data_plan = DataLoadPlan::from_meta_model(&meta)?;
    let linked_meta = meta.link().map_err(ConfigForgeError::MetaValidation)?;
    let data_set = DataLoader::new(data_dir).load(&data_plan)?;
    validate_data_set(&linked_meta, &data_set)
}
```

`export_project` 同理，应先得到 `RawSourceData`，再按 target 生成 `ResolvedExportView`。构建 `SourceReadPlan` 时可以先用当前 target 的 `TargetTags` 过滤整表 `export_when`，避免加载不会导出的表；写出 runtime data 时继续按同一组 tags 过滤字段和 `row_export` 行。长期 exporter 只负责 `DataValue -> JSON/MessagePack/future binary`，不再理解 source layout、default、target visibility 或 nested struct boundary。

## 5.9 错误与诊断边界

第一阶段沿用 `ConfigForgeError`：

| 场景 | 错误 |
| --- | --- |
| CSV 文件打不开 | `ConfigForgeError::ParseCsv` |
| CSV record parse 失败 | `ConfigForgeError::ParseCsv` |
| table 未声明 source | `ConfigForgeError::Validation` |
| provider 不支持 | `ConfigForgeError::Validation` 或 Meta diagnostic |
| DataSet 缺少 Meta 需要的 table | `ConfigForgeError::Validation` |

中期建议新增 Data diagnostic：

```rust
pub struct DataDiagnostic {
    pub code: DataDiagnosticCode,
    pub message: String,
    pub table: Option<String>,
    pub row: Option<usize>,
    pub field: Option<String>,
}
```

但第一阶段不引入，避免过早扩大诊断体系。

Resolved 层落地后，诊断模型需要同时支持 raw source 与 resolved value：

- raw source error：CSV parse、duplicate header、ObjectTable duplicate key、invalid JSON cell、unknown dotted path；
- resolve error：type parse、enum value、JSON/expanded mixing、inactive fragment、default 不可转换；
- validation error：required、range、foreign key、unique index、target-aware visibility conflict。

为了避免 resolved 后丢失定位信息，`ResolvedValue` 必须携带 `ValueSourceTrace`。ObjectTable 的 `key,value` source 在 raw `DataTable` 中已经通过 cell source 保留 key-level source path / row number；resolved 层应继续传递该 trace，避免回退到 singleton logical row 的 fallback location。

## 5.10 测试计划

### Data 模块单元测试

建议放在 `src/data/tests.rs`：

1. `CsvDataReader` 能读取 header 和多行 record。
2. CSV 读取失败时返回 `ConfigForgeError::ParseCsv`。
3. `DataLoadPlan::from_meta_model` 能从 legacy `source` 生成 csv plan。
4. `DataLoadPlan::from_meta_model` 能从单个 `sources = [{ provider = "csv", path = "..." }]` 生成 csv plan。
5. `DataLoader::load` 能读取多张表并生成 `DataSet`。
6. `DataSet::table` 能按名称查询表。
7. polymorphic table 的多个 variant sources 会合并成一个 `DataTable`，行顺序保持 source 声明顺序。
8. `RowSourceInfo` 与 `rows` 等长，并正确记录 source path、source row number 和 fragments。
9. 非 polymorphic table 声明 fragments 时，Meta validation/link 应失败，不能静默忽略；SourceReadPlan 只搬运 fragments 名称，不负责判断 table 是否 polymorphic。

### Validation 层测试

建议放在 `src/validation.rs` 测试模块：

1. `validate_data_set` 对完整 DataSet 通过。
2. 缺少 Meta 需要的 table 时失败。
3. CSV 中缺少 required field 且无 default 时失败。
4. CSV 中缺少 required field 但有 default 时通过。
5. enum 字段非法值失败。
6. variant source 中声明的 fragment required field 缺失且无 default 时失败，错误包含 table/source/row/fragment/field。
7. variant source 中声明的 fragment field 缺失但有 default 时 normalize 后通过并参与 export。
8. source 未声明 fragment 的 `fragment.field` 列出现在 CSV header 时失败；不同 fragments 复用同名字段时，`stats.value` 与 `cost.value` 应分别按对应 fragment 校验。
9. 两个 variant sources 中 primary key 冲突时失败，即使它们声明不同 fragments。
10. target tags 能过滤 fragment fields，并且不影响 common primary key validation。

### 集成测试

继续使用 `tests/fixtures/standard_project/`：

1. CLI validate/build 通过标准工程。
2. CLI export 输出 JSON / MessagePack 和 Rust/C++/TypeScript stub。
3. 标准参考工程根 `ConfigForge.toml` 应保持可执行；集成测试应直接运行该入口，或先复制整个 fixture 到 temp dir 后运行复制出的入口，避免根配置 imports、relative data path、targets 漂移。
4. 后续可增加错误 fixture，覆盖非法 CSV 值和缺表场景。
5. polymorphic items fixture：`materials.csv` / `weapons.csv` / `armors.csv` 三个 CSV source 合并为单一 `items` runtime data。
6. 检查 runtime JSON/MessagePack 中包含 nested fragment objects，并以 object 是否存在表达 fragment presence；不额外要求 `fragments` metadata 数组。
7. 检查生成代码包含 fragment 类型与 `has/get fragment` API。
8. 增加跨 source duplicate item id 负向测试，确保 `materials` / `weapons` / `armors` 合并后按 `items.by_id` 全局唯一校验。
9. 增加 `class_levels` 复合 primary index 负向测试，构造重复 `(class_id, level)` 并断言 CLI 失败。
10. 增加 inactive fragment header 负向测试，例如 `materials.csv` 出现 `attributes.attack` 应失败。

## 5.11 第一阶段完成标准

Data 模块第一阶段完成时应满足：

- `src/data/` 目录模块存在，并提供 Data 对外 API。
- CSV 读取逻辑从 exporter 内部逐表读取迁移到 `DataLoader`。
- `DataSet` 是 owned 数据，不保存 Meta 引用。
- `SourceReadPlan` 可从 raw `MetaModel` 提取，构造后不借用 Meta。
- `validate_project` / `export_project` 使用 `SourceReadPlan -> DataLoader -> RawSourceData -> normalize_data_set -> validate_data_set` 流程；旧 `DataLoadPlan -> DataSet` 入口仅作兼容 facade。
- 当前已支持的 validation 行为不回退：required/default、primitive type、enum value、unique field/index。
- `cargo fmt`、`cargo lint`、`cargo test` 通过。
- 修改后按代码规范运行覆盖率评估，确认 Data / Validation 新增分支有合理测试覆盖。
