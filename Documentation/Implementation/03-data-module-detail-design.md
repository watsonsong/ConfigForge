# 03. Data 模块详细设计

本文档描述 Data 模块的第一阶段实现设计，用于把 Meta 中声明的 source 配置加载为独立的数据快照，并在 Meta 完成 Link 后支持数据合法性检测。产品语义以 `Documentation/Design/` 为准；本文档聚焦 Rust 模块边界、核心类型、主要方法、流程阶段和测试组织。

## 3.1 设计目标

Data 模块负责把外部数据源读取为 ConfigForge 内部统一的数据模型。第一阶段目标只覆盖 CSV：

1. 支持从 `TableMeta.source` / `TableMeta.sources` 中解析单个 CSV source。
2. 支持按表加载 CSV，得到与 Meta 生命周期无关的 owned `DataSet`。
3. Data 加载阶段不依赖 `LinkedMetaModel<'meta>`，只依赖可拥有的加载计划，因此可以与 Meta Link / Meta Validate 在流程上并行。
4. 在 Meta Link 完成后，由 Validation 阶段使用 `LinkedMetaModel<'meta>` + `DataSet` 做字段存在性、required/default、类型、enum、unique index 等合法性检测。
5. 暂不实现任务并行驱动，但类型设计保持 Data 与 Meta 的数据相互独立，后续可接入 executor 并行加载。

当前阶段不实现：

- multi-source merge；
- `override_sources`；
- polymorphic `fragments`；
- 非 CSV provider；
- struct / array / expanded value 的 typed parse；
- source location 精确到单元格的诊断。

这些能力在 Meta validate 阶段应显式报 unsupported，避免 Data 阶段静默忽略。

## 3.2 Rust 中的“类”和职责表达

Rust 不使用传统 OO class。本文档中的“类”对应 Rust 的 `struct`、`enum`、`trait` 与 `impl`：

| 设计概念 | Rust 表达 | 责任 |
| --- | --- | --- |
| DataSet | `struct DataSet` | 一次加载得到的全部表数据快照。 |
| DataTable | `struct DataTable` | 单张逻辑表的行集合。 |
| DataRow | `type DataRow = BTreeMap<String, String>` | 单行原始 cell 文本，key 为字段/列名。 |
| DataSourcePlan | `struct DataSourcePlan` | 单张表的数据加载计划，已解析 provider 和相对路径。 |
| DataLoadPlan | `struct DataLoadPlan` | 多张表的数据加载计划集合。 |
| DataProvider | `enum DataProvider` | 当前支持的 provider，第一阶段只有 `Csv`。 |
| DataLoader | `struct DataLoader` | 根据 `DataLoadPlan` 加载 `DataSet`。 |
| CsvDataReader | `struct CsvDataReader` | CSV provider 的具体读取器。 |

## 3.3 建议模块结构

第一阶段建议新增目录模块 `src/data/`，逐步替代当前 `src/source.rs` 中的 CSV 读取职责：

```text
src/data/
  mod.rs          # 对外 API 与 re-export
  model.rs        # DataSet / DataTable / DataRow / 查询方法
  plan.rs         # DataLoadPlan / DataSourcePlan / DataProvider
  loader.rs       # DataLoader，编排按计划加载各表
  csv.rs          # CsvDataReader，CSV 文件读取实现
  tests.rs        # Data 模块单元测试
```

兼容迁移策略：

- 短期可以保留 `src/source.rs`，但 exporter / validation 应逐步改为依赖 `crate::data::*`。
- `source` 如果继续存在，应只表达“外部 source provider 原始读取”概念；逻辑表集合、DataSet 查询和 Data validation 输入应归入 `data`。
- 待 Data 模块稳定后，可删除或收敛 `source.rs`。

## 3.4 核心数据结构

### 3.4.1 `DataSet`

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

### 3.4.2 `DataTable`

```rust
pub struct DataTable {
    pub name: String,
    pub rows: Vec<DataRow>,
}
```

说明：

- `name` 对应 `TableMeta.name`。
- `rows` 是 CSV 读取后的行集合。
- 每个 row 的 key 来自 CSV header。
- 第一阶段不在 Data Load 阶段判断 header 是否匹配 Meta 字段；该判断放到 Validation 阶段。

主要方法：

```rust
impl DataTable {
    pub fn new(name: impl Into<String>, rows: Vec<DataRow>) -> Self;
    pub fn row_count(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

### 3.4.3 `DataRow`

```rust
pub type DataRow = BTreeMap<String, String>;
```

说明：

- 使用 `BTreeMap` 保持稳定序列化和测试输出。
- 值保持原始字符串，后续 required/default/type/enum/unique 校验由 `validation` 根据 linked meta 执行。

## 3.5 加载计划结构

### 3.5.1 `DataProvider`

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

第一阶段只接受 `csv`。非 `csv` provider 应在 Meta validate 或 DataLoadPlan 构造阶段返回错误，不进入读取器分发。

### 3.5.2 `DataSourcePlan`

```rust
pub struct DataSourcePlan {
    pub table_name: String,
    pub provider: DataProvider,
    pub path: PathBuf,
}
```

说明：

- `table_name` 是目标逻辑表名。
- `path` 是相对 data root 的路径，来自 `SourceRef.path` 或 legacy `TableMeta.source`。
- 第一阶段每张表最多一个 `DataSourcePlan`。

### 3.5.3 `DataLoadPlan`

```rust
pub struct DataLoadPlan {
    pub sources: Vec<DataSourcePlan>,
}
```

主要方法：

```rust
impl DataLoadPlan {
    pub fn from_meta_model(model: &MetaModel) -> Result<Self>;
    pub fn from_tables<'a>(tables: impl IntoIterator<Item = &'a TableMeta>) -> Result<Self>;
    pub fn sources(&self) -> &[DataSourcePlan];
}
```

设计要点：

- `DataLoadPlan` 从 raw `MetaModel` / `TableMeta` 中提取 source 信息，不需要 `LinkedMetaModel<'meta>`。
- 一旦 `DataLoadPlan` 构造完成，就拥有 table name、provider、path，后续 Data Load 不再借用 Meta。
- 因此流程上可以在 raw Meta load/merge 完成后：
  - 一边做 `MetaModel::link()` / Meta validate；
  - 一边执行 `DataLoader::load(&plan)`。

## 3.6 CSV 读取器

### 3.6.1 `CsvDataReader`

```rust
pub struct CsvDataReader;
```

主要方法：

```rust
impl CsvDataReader {
    pub fn read_table(table_name: impl Into<String>, path: &Path) -> Result<DataTable>;
}
```

读取规则：

1. 使用 `csv::Reader::from_path` 打开文件。
2. 第一行作为 header。
3. 每条 record 与 header zip 成 `DataRow`。
4. 缺失 cell 当前不在读取阶段补齐；Validation 阶段按 required/default 处理。
5. 多余 cell 当前由 csv crate 的 record 行为处理；后续如果需要严格列数，可在 Data 模块增加 header/record 长度校验。

错误映射：

- 文件无法打开、CSV parse 失败统一映射为 `ConfigForgeError::ParseCsv { path, source }`。
- source declaration 缺失或 provider 不支持可以映射为 `ConfigForgeError::Validation`，中期应迁移到结构化 Data diagnostic。

## 3.7 DataLoader

```rust
pub struct DataLoader {
    data_root: PathBuf,
}
```

主要方法：

```rust
impl DataLoader {
    pub fn new(data_root: impl Into<PathBuf>) -> Self;
    pub fn load(&self, plan: &DataLoadPlan) -> Result<DataSet>;
    pub fn load_source(&self, source: &DataSourcePlan) -> Result<DataTable>;
}
```

设计要点：

- `DataLoader` 不持有 Meta 引用。
- `load()` 第一阶段顺序加载所有 source；未来可以替换为并行实现，但 public API 不需要变化。
- `load_source()` 根据 `DataProvider` 分发到 `CsvDataReader`。
- `DataSet::new()` 负责建立 table index；若出现重复 table name，应报错或以后由 Data diagnostic 表达。第一阶段由于 Meta 已校验重复 table name，DataLoader 可以假设输入计划无重复。

## 3.8 与 Meta / Validation / Exporter 的流程关系

### 3.8.1 当前建议执行流程

```text
MetaModel::from_path(meta_path)
  -> raw MetaModel owns source declarations
  -> DataLoadPlan::from_meta_model(&meta_model)

阶段 A：Meta link / validate
  MetaModel::link() -> LinkedMetaModel<'meta>

阶段 B：Data load
  DataLoader::new(data_root).load(&data_load_plan) -> DataSet

阶段 C：Data validation
  validate_data_set(&linked_meta, &data_set)

阶段 D：Export
  exporter consumes LinkedMetaModel + validated/default-applied DataSet
```

当前可以先顺序执行：

```rust
let meta = MetaModel::from_path(meta_path)?;
let data_plan = DataLoadPlan::from_meta_model(&meta)?;
let linked_meta = meta.link().map_err(ConfigForgeError::MetaValidation)?;
let data_set = DataLoader::new(data_root).load(&data_plan)?;
validate_data_set(&linked_meta, &data_set)?;
```

未来并行化时，`linked_meta` 和 `data_set` 的构造可以拆成两个任务，因为：

- `DataLoadPlan` 是 owned；
- `DataLoader` 不借用 Meta；
- `DataSet` 不保存 Meta 引用；
- 只有 `validate_data_set` 需要同时拿到 `LinkedMetaModel<'meta>` 和 `DataSet`。

### 3.8.2 Validation API 调整

当前已有单表校验：

```rust
pub fn validate_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()>;
```

Data 模块落地后建议增加数据集级入口：

```rust
pub fn validate_data_set(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<()>;
```

职责：

1. 遍历 `meta.tables`。
2. 在 `DataSet` 中按 table name 查找 `DataTable`。
3. 缺失表时报错。
4. 对每张表调用 `validate_table(table_meta, table)`。
5. `validate_table` 内部继续执行 default、required、类型、enum、unique 等规则。

### 3.8.3 Exporter API 调整

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

`export_project` 同理，应先得到 `DataSet`，再在导出阶段按 table name 查表，而不是在 exporter 中逐表读取 CSV。构建 `DataLoadPlan` 时应先用当前 target 的 `TargetTags` 过滤整表 `export_when`，避免加载不会导出的表；写出 runtime data 时继续按同一组 tags 过滤字段和 `row_export` 行。

## 3.9 与当前 `source.rs` 的关系

当前 `src/source.rs` 已有：

```rust
pub type DataRow = BTreeMap<String, String>;

pub struct DataTable {
    pub name: String,
    pub rows: Vec<DataRow>,
}

impl DataTable {
    pub fn from_csv_path(name: impl Into<String>, path: impl AsRef<Path>) -> Result<Self>;
}
```

迁移建议：

1. 将 `DataRow`、`DataTable` 移入 `src/data/model.rs`。
2. 将 `from_csv_path` 的实现移动到 `src/data/csv.rs::CsvDataReader::read_table`。
3. 对外由 `src/data/mod.rs` re-export `DataSet`、`DataTable`、`DataRow`、`DataLoadPlan`、`DataLoader`。
4. exporter 和 validation 改为依赖 `crate::data::{DataSet, DataTable}`。
5. 如果短期保留 `source.rs`，可以作为兼容 re-export，但不再新增逻辑。

## 3.10 错误与诊断边界

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

## 3.11 测试计划

### Data 模块单元测试

建议放在 `src/data/tests.rs`：

1. `CsvDataReader` 能读取 header 和多行 record。
2. CSV 读取失败时返回 `ConfigForgeError::ParseCsv`。
3. `DataLoadPlan::from_meta_model` 能从 legacy `source` 生成 csv plan。
4. `DataLoadPlan::from_meta_model` 能从单个 `sources = [{ provider = "csv", path = "..." }]` 生成 csv plan。
5. `DataLoader::load` 能读取多张表并生成 `DataSet`。
6. `DataSet::table` 能按名称查询表。

### Validation 层测试

建议放在 `src/validation.rs` 测试模块：

1. `validate_data_set` 对完整 DataSet 通过。
2. 缺少 Meta 需要的 table 时失败。
3. CSV 中缺少 required field 且无 default 时失败。
4. CSV 中缺少 required field 但有 default 时通过。
5. enum 字段非法值失败。

### 集成测试

继续使用 `tests/fixtures/standard_project/`：

1. CLI validate 通过标准工程。
2. CLI export 输出 JSON 和 Rust stub。
3. 后续可增加一个错误 fixture，覆盖非法 CSV 值和缺表场景。

## 3.12 第一阶段完成标准

Data 模块第一阶段完成时应满足：

- `src/data/` 目录模块存在，并提供 Data 对外 API。
- CSV 读取逻辑从 exporter 内部逐表读取迁移到 `DataLoader`。
- `DataSet` 是 owned 数据，不保存 Meta 引用。
- `DataLoadPlan` 可从 raw `MetaModel` 提取，构造后不借用 Meta。
- `validate_project` / `export_project` 使用 `DataLoadPlan -> DataLoader -> DataSet -> validate_data_set` 流程。
- 当前已支持的 validation 行为不回退：required/default、primitive type、enum value、unique field/index。
- `cargo fmt`、`cargo lint`、`cargo test` 通过。
- 修改后按代码规范运行覆盖率评估，确认 Data / Validation 新增分支有合理测试覆盖。
