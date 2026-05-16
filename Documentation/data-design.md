# Data Design

ConfigForge 的 Data 部分负责把不同 Source Provider 读取出的二维表格数据，按照 Meta 定义归一化、组合、验证、过滤，并导出为目标运行时需要的文件。本文档描述 Data 的内存模型、运行时数据格式扩展、特殊导出器、导出上下文 tag 过滤，以及工具字段和运行时字段的区分。

本文档是设计文档，不要求当前代码已经实现所有能力。

## 设计目标

- 把任意 Source Provider 读取结果统一成可按行列访问的二维 Sheet 数据。
- 把二维 Sheet 数据根据 Meta 转换成 typed logical dataset。
- 支持 ObjectTable、CollectionTable、Fragment、多 Source、Override Source 的统一组合流程。
- 支持同一份 validated dataset 导出到不同 Runtime Data Format。
- 支持未来扩展 JSON、YAML、MessagePack、Protocol Buffers、SQLite 等数据格式。
- 支持特殊导出器，例如 text 字段导出 POT、asset 字段导出资源清单 CSV。
- 支持 Export Context Tag，根据当前导出上下文过滤数据行、字段或特殊导出内容。
- 支持工具字段和运行时字段的区别：有些字段只参与导出过滤或校验，不出现在最终运行时数据中。
- 支持 Source Provider 和 Artifact Writer 的异步 I/O 形态，便于接入网络表格和大文件输出。
- 支持独立 source、table、target、artifact 的并行处理，同时保持诊断和产物清单顺序稳定。

实现层面的插件、并行、异步和诊断要求见 `Documentation/implementation-requirements.md`。

## 数据管线

推荐 ConfigForge 的整体数据管线如下：

```text
Source Provider
  -> Raw Sheet / Source Sheet
  -> Sheet Mapping + Field Layout
  -> Typed Table Data
  -> Source Composition
  -> Validation
  -> Export Context Filtering
  -> Export Dataset
  -> Data Exporter(s)
  -> Runtime Data Files / Special Artifacts
```

各阶段职责：

1. **Source Provider**：读取 CSV、XLSX、Lark Sheet、Google Sheet 等外部来源；长期 API 应支持异步加载和插件注册。
2. **Raw Sheet / Source Sheet**：统一成二维表格，可通过行列坐标读取 cell。
3. **Sheet Mapping + Field Layout**：根据 Meta 将 header、field path、JSON cell、expanded columns/rows 映射到字段。
4. **Typed Table Data**：把原始 cell 转成 Meta 声明的 typed value，例如 int、color_rgba、vector3、datetime、enum、struct、array。
5. **Source Composition**：合并 base `sources`，应用 `override_sources`。
6. **Validation**：执行内置和配置 Validator。
7. **Export Context Filtering**：根据本次导出上下文 tag 过滤数据。
8. **Export Dataset**：形成某个导出目标实际要输出的数据视图。
9. **Data Exporter**：输出 JSON、YAML、MessagePack、Protobuf、SQLite、POT、Asset CSV 等文件。

## 二维 Sheet 数据模型

每个数据源读取后都应归一化为二维 Sheet。二维 Sheet 是 Source Provider 与 Meta 解析之间的边界。

推荐能力：

- 获取总行数和总列数；
- 根据 `row_index` / `column_index` 获取 cell；
- 根据 header 名称获取列；
- 遍历行；
- 遍历列；
- 获取 source location，用于诊断；
- 保留 raw text，避免 provider 提前丢失信息。

概念性 Rust 草图：

```rust
struct SourceSheet {
    source_id: SourceId,
    name: Option<String>,
    rows: Vec<SheetRow>,
}

struct SheetRow {
    row_index: usize,
    cells: Vec<SheetCell>,
}

struct SheetCell {
    row_index: usize,
    column_index: usize,
    raw: String,
    location: SourceLocation,
}

impl SourceSheet {
    fn row_count(&self) -> usize;
    fn column_count(&self) -> usize;
    fn cell(&self, row: usize, column: usize) -> Option<&SheetCell>;
    fn row(&self, row: usize) -> Option<&SheetRow>;
}
```

设计原则：

- `SourceSheet` 不理解 Meta 类型，只保留二维结构和 source location。
- cell 的 raw 值应尽量保持原样，typed parse 留给后续阶段。
- Source Provider 可以提供额外 metadata，但后续流程应只依赖统一 Sheet 抽象。

### Async Source Provider

Source Provider 是最重要的异步边界。CSV 等本地小文件可以在实现内部同步读取，但 trait 形态应允许 Lark Sheet、Google Sheet、HTTP 或内部平台 provider 进行异步网络 I/O。

推荐要求：

- provider 通过 `SourceProviderRegistry` 注册，内置 provider 与插件 provider 使用同一解析流程；
- source loading 可以按 source/table 并行调度；
- 网络 provider 应支持超时、重试、取消、限流和 auth_ref 外部凭证解析；
- provider diagnostics 应保留 source URI、sheet/range、provider ID 和底层错误；
- 无论 provider 完成顺序如何，最终 source diagnostics 必须稳定排序。

## Typed Data Model

二维 Sheet 按 Meta 解析后，应进入 typed logical data model。

推荐核心结构：

```rust
struct DataSet {
    tables: BTreeMap<TableName, TableData>,
}

enum TableData {
    Object(ObjectData),
    Collection(CollectionData),
}

struct ObjectData {
    fields: BTreeMap<FieldPath, DataValue>,
    locations: BTreeMap<FieldPath, SourceLocation>,
}

struct CollectionData {
    records: Vec<RecordData>,
    indexes: BuiltIndexes,
    preserve_source_order: bool,
}

struct RecordData {
    values: BTreeMap<FieldPath, DataValue>,
    fragments: BTreeMap<FragmentName, FragmentData>,
    source_location: SourceLocation,
}

enum DataValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Text(LocalizedTextValue),
    Asset(AssetRefValue),
    ColorRgb(ColorRgb),
    ColorRgba(ColorRgba),
    Vector2(Vector2),
    Vector3(Vector3),
    Vector4(Vector4),
    Vector2Int(Vector2Int),
    Vector3Int(Vector3Int),
    Rotation(Rotation),
    DateTime(DateTimeValue),
    Date(DateValue),
    Time(TimeValue),
    Duration(DurationValue),
    IntRange(IntRange),
    FloatRange(FloatRange),
    Rect(Rect),
    Bounds3(Bounds3),
    Enum(EnumValue),
    Struct(BTreeMap<FieldName, DataValue>),
    Array(Vec<DataValue>),
}
```

`DataSet` 是完整逻辑数据集；`ExportDataset` 是经过上下文过滤和导出字段裁剪后的输出视图。

`CollectionData.records` should preserve final logical record order even when indexes are also built. For tables with `preserve_source_order = true`, this order becomes part of the runtime export contract and can be emitted as an ordered array. For indexed-only tables, the vector order is still useful for diagnostics and deterministic export, but generated runtime access may expose only index lookup APIs.

## Runtime Data Exporter

常规运行时数据导出器负责把 `ExportDataset` 写成游戏运行时加载的文件。

推荐抽象：

```rust
trait DataExporter {
    fn id(&self) -> &'static str;
    fn format(&self) -> RuntimeDataFormat;
    fn export(&self, dataset: &ExportDataset, ctx: &ExportContext, out: &mut ArtifactWriter) -> ExportResult;
}
```

长期实现可以把 `DataExporter` 演进为 async trait，或在 orchestration 层把同步 exporter 包装为任务。关键要求是：独立 artifact 可以并行生成，写入通过统一 `ArtifactWriter` 管理，最终 manifest 按路径稳定排序。

Data exporter、special exporter 和 code generator 都应允许通过插件注册。插件 exporter 必须声明可处理的 `RuntimeDataFormat`、artifact kind、配置 schema 和输出路径策略。

推荐目录结构：

```text
src/
  data/
    mod.rs
    sheet.rs
    value.rs
    dataset.rs
    filter.rs
  exporter/
    mod.rs
    data_exporter.rs
    artifact.rs
    formats/
      json.rs
      yaml.rs
      msgpack.rs
      protobuf.rs
      sqlite.rs
    special/
      gettext_pot.rs
      asset_manifest_csv.rs
```

## Runtime Data Format

`RuntimeDataFormat` 描述运行时数据文件格式。它不同于 code generation language，也不同于 source provider format。

初始实现可以只支持 JSON，但结构上应允许扩展。

| Format | Suggested Symbol | Notes |
| --- | --- | --- |
| JSON | `RuntimeDataFormat::Json` | 适合 JavaScript/TypeScript 平台，也适合调试和人工 diff。 |
| YAML | `RuntimeDataFormat::Yaml` | 可读性强，但运行时解析成本和规范差异需要谨慎。 |
| MessagePack | `RuntimeDataFormat::MessagePack` | 压缩 JSON-like 数据，适合需要体积优化但不想维护 schema 的场景。 |
| Protocol Buffers | `RuntimeDataFormat::Protobuf` | 需要同时导出 schema 和二进制数据，适合强 schema 和跨语言运行时。 |
| SQLite | `RuntimeDataFormat::Sqlite` | 适合需要运行时查询、分页、按需加载的数据集。 |
| Custom Binary | `RuntimeDataFormat::CustomBinary` | 留给后续性能极致优化。 |

### JSON Exporter

JSON 可以作为第一阶段默认格式。

优点：

- 实现简单；
- 对 JavaScript/TypeScript 原生友好；
- 易于调试和 diff；
- 适合早期验证整体数据模型。

注意事项：

- 大型数据集可能需要分表输出；
- int 精度在 JavaScript 环境需要考虑；
- asset/text/enum 的 runtime representation 应由 export config 决定。

### YAML Exporter

YAML 适合作为可读文本格式，但不建议作为默认运行时格式，除非目标平台已有成熟加载器。

### MessagePack Exporter

MessagePack 可看作压缩二进制 JSON。它适合在不想引入 Protobuf schema 的情况下减少体积和解析开销。

### Protobuf Exporter

Protobuf 导出器需要输出两类 artifact：

1. `.proto` 数据定义文件；
2. `.pb` 或类似二进制数据文件。

设计要求：

- Meta logical types 能映射到 protobuf field types；
- enum 和 named struct 能生成 protobuf enum/message；
- table 能生成顶层 message 或 repeated message；
- field number 需要稳定策略，不能每次生成变化；
- 需要处理 optional、repeated、map、oneof 等语义；
- schema artifact 和 data artifact 都应纳入 artifact manifest。

Protobuf 示例 artifact：

```text
Generated/Data/items.proto
Generated/Data/items.pb
```

### SQLite Exporter

SQLite 导出器适合运行时需要表查询的场景。

设计要求：

- 每个 CollectionTable 可以映射为一张 table；
- ObjectTable 可以映射为 key/value 表或单行 table；
- index meta 可以映射为 SQLite index；
- nested struct/array 需要策略：JSON column、拆表、或二进制 blob；
- foreign key 是否生成 SQLite foreign key 约束应由 target config 控制。

## Artifact Model

不同导出器可能生成一个或多个文件，因此需要统一 Artifact 模型。

```rust
struct ExportArtifact {
    path: PathBuf,
    kind: ArtifactKind,
    format: Option<RuntimeDataFormat>,
    target: ExportTargetName,
    source_tables: Vec<TableName>,
}

enum ArtifactKind {
    RuntimeData,
    RuntimeSchema,
    GeneratedCode,
    LocalizationTemplate,
    AssetManifest,
    Manifest,
}
```

Artifact manifest 可以记录本次导出生成了哪些文件，便于 CI、增量构建和打包工具消费。

## Special Exporters

特殊导出器不一定输出完整运行时数据集，而是从 `DataSet` 或 `ExportDataset` 中抽取某类字段生成辅助 artifact。

### Text POT Exporter

`text` 字段面向 i18n。除了在运行时数据中按目标策略导出，还应支持把所有需要翻译的内容导出为 gettext 可识别的 POT 模板。

POT 导出内容应包括：

- `msgid`：原文或稳定文本 key；
- source reference：表名、字段路径、source 文件、行列位置；
- translator comment：Meta 中的字段注释、设计备注、上下文说明；
- extracted comment：记录 ID、table name、field path、可选 tag；
- optional msgctxt：用于区分同样原文在不同上下文中的含义。

示例：

```pot
#. table=items field=display_name id=1001
#. note=Item display name shown in inventory
#: data/items.csv:12:3
msgctxt "items.display_name"
msgid "Iron Sword"
msgstr ""
```

设计原则：

- POT exporter 应读取 typed text 字段和 source locations；
- 是否导出被 context filter 排除的数据，需要由 target config 决定；
- text 字段如果运行时导出为 localization key，POT 仍应能保留原文和位置；
- POT 是特殊 artifact，不等同于 runtime data format。

### Asset Manifest CSV Exporter

`asset` 字段可以导出成独立资源清单，用于打包系统收集需要随版本发布的资源。

推荐 CSV 列：

```csv
asset_ref,asset_kind,table,field_path,record_key,source,tag
assets/icons/iron_sword.png,texture,items,icon,1001,data/items.csv:12:4,shipping
```

设计原则：

- Asset manifest exporter 应扫描所有运行时需要的 asset 字段；
- 是否包含被 context filter 排除的数据，应默认不包含；
- 同一个 asset 被多处引用时，可以保留多行引用，也可以输出去重 manifest + references；
- asset_kind、bundle、addressable group 等字段可以由 Meta 或 export config 扩展。

## Export Context Tags

一次导出操作可以携带上下文 tag，用来表示目标版本、国家、区域、渠道、测试/发布模式等。

示例 tag：

- `dev`
- `test`
- `shipping`
- `cn`
- `global`
- `ios`
- `android`
- `steam`

CLI 草案：

```bash
config-forge export --target unity_json --tag shipping --tag global
```

或：

```bash
config-forge export --target unity_json --context shipping,global
```

概念性结构：

```rust
struct ExportContext {
    target: ExportTargetName,
    tags: BTreeSet<String>,
    variables: BTreeMap<String, String>,
}
```

## Export Filtering

Export filtering 根据当前 `ExportContext` 决定某些 record、field、fragment 或 special artifact entry 是否进入导出结果。

### Row / Record Filtering

常见需求：shipping 模式下，`test = true` 的行不导出。

TOML 草案：

```toml
[[tables]]
name = "items"
kind = "collection"

[[tables.export_filters]]
scope = "record"
when_tag = "shipping"
field = "test"
op = "=="
value = true
action = "exclude"
```

语义：当本次导出 context 包含 `shipping` tag，且记录的 `test` 字段为 `true`，则该记录从 `ExportDataset` 中排除。

### Field Filtering

有些字段只在特定 target 或 tag 下导出。例如 debug 字段只在 dev/test 导出。

```toml
[[tables.fields]]
name = "debug_note"
type = "string"
export = false

[[tables.fields.export_rules]]
when_tag = "dev"
export = true
```

### Fragment Filtering

多态 fragment 也可以受 context 影响。例如某个地区不启用某类活动 fragment。

```toml
[[tables.fragment_export_filters]]
fragment = "event_shop"
when_tag = "global"
action = "exclude"
```

## Runtime Field vs Tool-Only Field

参与导出过滤或校验的字段不一定需要进入运行时数据。因此字段需要区分“是否参与工具流程”和“是否参与运行时导出”。

推荐字段属性：

| Property | Meaning |
| --- | --- |
| `export` | 是否进入常规 runtime data。默认 `true`。 |
| `tool_only` | 字段只用于工具校验/过滤/备注，不进入 runtime data。设置后等价于 `export = false`，但语义更明确。 |
| `filter_field` | 字段是否允许被 export filter 引用。默认普通字段也可引用；工具字段应显式允许。 |
| `validator_only` | 字段只用于 Validator，不进入 runtime data。 |
| `special_exports` | 是否参与 POT、Asset manifest 等特殊导出。 |

示例：`test` 字段只用于 shipping 过滤，不导出给运行时。

```toml
[[tables.fields]]
name = "test"
type = "bool"
required = true
tool_only = true
filter_field = true
export = false
```

示例：`test` 字段运行时也需要，因此参与过滤但仍导出。

```toml
[[tables.fields]]
name = "test"
type = "bool"
required = true
filter_field = true
export = true
```

设计原则：

- Meta 仍然描述 tool-only 字段，因为它们来自 source，并参与解析、校验、过滤。
- `tool_only` 字段应参与 validation 和 export filtering，但默认不进入 runtime data。
- `export = false` 字段仍可被 special exporter 使用，前提是 `special_exports` 或 exporter policy 允许。
- 过滤必须发生在 validation 之后还是之前要明确。默认建议：先验证完整数据，再过滤导出数据；必要时允许某些 Validator 声明只对 export view 运行。

## Export Dataset View

`ExportDataset` 是从完整 `DataSet` 派生出的导出视图。

生成步骤：

1. 从完整 `DataSet` 开始；
2. 应用 export context record/field/fragment filters；
3. 移除 `tool_only` 或 `export = false` 字段；
4. 根据 target type mapping 转换 enum、text、asset、named struct 表示；
5. 交给 runtime data exporter 或 special exporter。

注意：

- 完整 `DataSet` 应保留 source location 和 tool-only 字段，用于诊断与特殊导出。
- `ExportDataset` 不一定包含所有字段，也不一定包含所有记录。
- `preserve_source_order = true` 的 CollectionTable 在过滤后应保留剩余记录的相对顺序。
- 多个 target 可以从同一个 `DataSet` 派生不同 `ExportDataset`。

## Data Export Target Configuration

Export target 应扩展 runtime data 和 special exporter 配置。

TOML 草案：

```toml
[[targets]]
name = "web_json_shipping"
language = "typescript"
runtime = "javascript"
data_format = "json"
output = "Generated/Web/Data"
tags = ["shipping", "global"]

[targets.data]
split = "by_table"
pretty = false
include_manifest = true

[[targets.special_exporters]]
kind = "gettext_pot"
output = "Generated/Localization/messages.pot"
include_filtered_out = false

[[targets.special_exporters]]
kind = "asset_manifest_csv"
output = "Generated/Build/assets.csv"
deduplicate = true
```

Protobuf target 草案：

```toml
[[targets]]
name = "server_pb"
language = "go"
data_format = "protobuf"
output = "Generated/Server/Data"

[targets.protobuf]
package = "game.config"
schema_output = "Generated/Server/Proto"
field_number_strategy = "stable_hash"
```

SQLite target 草案：

```toml
[[targets]]
name = "client_sqlite"
data_format = "sqlite"
output = "Generated/Client/config.db"

[targets.sqlite]
create_indexes = true
nested_strategy = "json_column"
```

## Validation Rules

Data/export validation should check:

- SourceSheet row/column/cell locations are stable and available for diagnostics;
- every export filter references existing fields;
- filter field types are compatible with filter operators;
- `tool_only = true` fields are not emitted in runtime data unless explicitly allowed;
- `export = false` fields are not emitted in regular runtime data;
- field filters do not remove fields required by selected runtime schema unless target policy allows it;
- context tags use declared/known tag names when a tag registry is configured;
- special exporters reference supported field types;
- Protobuf exporter has stable field number strategy;
- SQLite exporter has a declared nested data strategy;
- artifact output paths do not conflict;
- data exporter plugin/format is known.

## 非目标

- 本文档不要求立即实现 YAML、MessagePack、Protobuf、SQLite。
- 本文档不定义最终 CLI 参数形式。
- 本文档不定义完整表达式语言，只给出 filter 配置方向。
- 本文档不要求 text i18n 完整管线立即实现。
- 本文档不要求 asset bundle/addressable 规则立即实现。
- 本文档不修改当前 `src/`、`tests/`、`Cargo.toml`、`Cargo.lock`。

## 设计决策

- Source Provider 输出统一二维 Sheet；Meta 解析阶段再决定如何解释行列。
- 完整 `DataSet` 保留所有 typed data、tool-only 字段和 source location。
- `ExportDataset` 是根据 target/context 派生的输出视图。
- Runtime data format 是可扩展 exporter，不与 source format 或 code language 绑定。
- JSON 可作为初始实现，YAML、MessagePack、Protobuf、SQLite 作为后续扩展。
- POT 和 Asset Manifest 属于 special exporter，不是普通 runtime data format。
- Export context tag filtering 属于 Data export 阶段，但依赖 Meta 中声明的字段属性和过滤规则。
- `tool_only` 字段属于 Meta，因为它们需要被读取、解析、校验和过滤引用；但默认不进入运行时数据。
