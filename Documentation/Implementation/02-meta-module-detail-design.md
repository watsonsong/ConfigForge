# 02. Meta 模块详细设计

本文档用于指导下一步优先开发 Meta 模块，使基础 Meta 加载和单元测试先跑起来。产品语义以 `Documentation/Design/04-meta-model.md` 为准；本文档描述 Rust 实现中的类型边界、类/结构体关系、主要方法和测试组织。

## 2.1 设计目标

Meta 模块负责把一个或多个 Meta 文件转换为 canonical `MetaModel`。

近期目标聚焦基础能力：

1. TOML Meta 加载。
2. 单文件解析。
3. 基础 table / field / index / field type 建模。
4. `desc`、`kind`、`primary_index`、`required`、`default` 等字段保留。
5. 基础结构校验与稳定诊断。
6. 单元测试可独立运行。

暂缓但需预留扩展点：

- XML Meta。
- 多文件 import/include。
- enum、named struct、fragment、validator。
- source declaration 和 target reference 的完整解析。
- Protobuf `field_number` 等目标格式兼容字段。

## 2.2 Rust 中的“类”和继承策略

Rust 不使用面向对象继承。本文档中的“类”对应 Rust 的 `struct`、`enum`、`trait`：

- **数据类**：使用 `struct`，例如 `MetaModel`、`TableMeta`、`FieldMeta`。
- **多态变体**：使用 `enum`，例如 `TableKind`、`FieldType`、`MetaFormat`。
- **行为抽象**：使用 `trait`，例如未来的 `MetaParser` 或 `MetaSourceResolver`。
- **继承关系**：用组合、枚举分发或 trait 约束替代，不设计父类/子类继承树。

Meta 模块第一阶段不需要复杂 trait 层级；先使用清晰的数据结构和 builder/validator 函数即可。

## 2.3 建议模块结构

短期可继续放在 `src/meta.rs`，但实现时按以下边界组织代码块；迁移为目录模块时直接拆分：

```text
src/meta/
  mod.rs
  document.rs
  loader.rs
  syntax.rs
  model.rs
  field_type.rs
  path.rs
  index.rs
  builder.rs
  validate.rs
  diagnostic.rs
```

各子模块职责：

| 子模块 | 责任 |
| --- | --- |
| `document` | 表示 Meta 文档、格式、文件路径、导入来源。 |
| `loader` | 从文件系统读取 Meta 文档；未来处理 import 递归和循环检测。 |
| `syntax` | serde 反序列化结构，尽量贴近 TOML/XML 原始格式。 |
| `model` | canonical MetaModel 类型，对外暴露的主要数据结构。 |
| `field_type` | 字段类型、内置值类型、类型引用与解析。 |
| `path` | field path 和 array slot 解析，例如 `rewards[0].count`。 |
| `index` | index、primary index、唯一性规则。 |
| `builder` | 从 raw syntax 构建 canonical model，做默认值补齐和规范化。 |
| `validate` | Meta 结构校验。 |
| `diagnostic` | Meta 诊断 code、位置、排序。 |

## 2.4 核心数据结构

### 2.4.1 `MetaModel`

```rust
pub struct MetaModel {
    pub project: Option<ProjectMeta>,
    pub tables: Vec<TableMeta>,
    pub enums: Vec<EnumMeta>,
    pub structs: Vec<StructMeta>,
    pub fragments: Vec<FragmentMeta>,
    pub validators: Vec<ValidatorMeta>,
}
```

说明：

- 第一阶段可只实现 `tables`，其他字段可先预留为空集合或暂不暴露。
- `MetaModel` 是 canonical 输出，后续 `data`、`validation`、`exporter` 依赖它。
- `MetaModel` 不保留 TOML/XML 语法细节，但应保留业务字段、`desc` 和必要 source location。

主要方法：

```rust
impl MetaModel {
    pub fn from_toml_str(input: &str) -> Result<Self>;
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self>;
    pub fn table(&self, name: &str) -> Option<&TableMeta>;
    pub fn validate(&self) -> Result<(), Vec<MetaDiagnostic>>;
}
```

第一阶段如果保留现有 `MetaSet` 名称，建议将其定位为兼容别名或临时 raw set，最终对外名称收敛到 `MetaModel`。

### 2.4.2 `ProjectMeta`

```rust
pub struct ProjectMeta {
    pub name: Option<String>,
    pub copyright: Option<String>,
    pub desc: Option<String>,
}
```

说明：

- `copyright` 来自近期确认的设计决策，可用于生成文件头。
- Project Config 与 Meta 顶层 project 需要在后续设计中明确合并优先级；第一阶段只做 Meta 文件内保留。

### 2.4.3 `TableMeta`

```rust
pub struct TableMeta {
    pub name: String,
    pub kind: TableKind,
    pub desc: Option<String>,
    pub sources: Vec<SourceRef>,
    pub override_sources: Vec<SourceRef>,
    pub fields: Vec<FieldMeta>,
    pub indexes: Vec<IndexMeta>,
    pub primary_index: Option<String>,
    pub preserve_source_order: bool,
    pub polymorphic: bool,
}
```

说明：

- 当前 `src/meta.rs` 中的 `source: String` 是早期简化形式。下一步可兼容解析为 `sources = [{ provider = "csv", path = "..." }]`，也可短期保留单 source 字段用于 fixture。
- `kind` 默认策略建议：没有声明时先按 `collection` 处理，或要求必填。为了避免歧义，推荐第一阶段要求必填并在错误样例中覆盖。
- `ObjectTable` 和 `CollectionTable` 不需要定义成继承子类；通过 `TableKind` 区分，并在 validator 中执行不同规则。

### 2.4.4 `TableKind`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    Object,
    Collection,
}
```

规则：

- `Object`：不需要 index；override 可按 field path 覆盖。
- `Collection`：可定义 index；override 依赖 primary index。

### 2.4.5 `FieldMeta`

```rust
pub struct FieldMeta {
    pub name: String,
    pub desc: Option<String>,
    pub field_type: FieldType,
    pub required: bool,
    pub default: Option<DefaultValue>,
    pub layout: Option<ValueLayout>,
    pub export: bool,
    pub tool_only: bool,
    pub filter_field: bool,
    pub validator_only: bool,
    pub special_exports: SpecialExportPolicy,
    pub foreign_key: Option<ForeignKeyMeta>,
    pub field_number: Option<u32>,
}
```

第一阶段建议实现：

- `name`
- `desc`
- `field_type`
- `required`
- `default`
- `layout`
- `export` 默认 `true`
- `field_number` 先保留为可选字段但不参与逻辑

现有 `unique: bool` 不建议继续作为 field 级正式概念；唯一性应通过 `IndexMeta { unique: true }` 表达。短期为了兼容旧 fixture 可以解析，但新文档和新测试应使用 index。

### 2.4.6 `FieldType`

```rust
pub enum FieldType {
    Int,
    Float,
    Bool,
    String,
    Text,
    Asset,
    Builtin(BuiltinValueType),
    Enum(TypeName),
    Struct(TypeName),
    Array(Box<FieldType>),
}
```

第一阶段建议至少支持：

```rust
Int, Float, Bool, String, Text, Asset
```

为了尽快跑通基础测试，可以先使用 serde string 解析：

| Meta 字符串 | FieldType |
| --- | --- |
| `int` | `FieldType::Int` |
| `float` | `FieldType::Float` |
| `bool` | `FieldType::Bool` |
| `string` | `FieldType::String` |
| `text` | `FieldType::Text` |
| `asset` | `FieldType::Asset` |

历史兼容：当前代码使用 `i64` / `f64`。下一步可以临时同时接受 `i64 -> int`、`f64 -> float`，但 canonical 输出应使用 `int` / `float`。

### 2.4.7 `ValueLayout`

```rust
#[serde(rename_all = "snake_case")]
pub enum ValueLayout {
    ScalarCell,
    JsonCell,
    ExpandedValue,
}
```

规则：

- primitive 默认 `scalar_cell`。
- struct/array 如果未声明 layout，后续 builder 可根据类型给出默认值。
- 第一阶段只解析并保留，不必实现 sheet mapping。

### 2.4.8 `IndexMeta`

```rust
pub struct IndexMeta {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<FieldPath>,
    pub unique: bool,
    pub primary: bool,
}
```

规则：

- `fields` 至少一个。
- `unique = true` 表示唯一索引。
- `primary = true` 与 table `primary_index = "..."` 等价；二者同时存在必须一致。
- primary index 必须是 unique index。
- index 引用的 field path 必须存在。

### 2.4.9 `FieldPath`

```rust
pub struct FieldPath {
    pub segments: Vec<FieldPathSegment>,
}

pub enum FieldPathSegment {
    Name(String),
    ArraySlot(usize),
}
```

示例：

```text
rewards[0].item_id
```

解析为：

```text
Name("rewards"), ArraySlot(0), Name("item_id")
```

规则：

- array slot 使用 0-based。
- 第一阶段 index 字段可以先只支持简单字段名；但 `FieldPath` 类型应预留 expanded value 需要的路径结构。

### 2.4.10 `MetaDiagnostic`

```rust
pub struct MetaDiagnostic {
    pub code: MetaDiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub location: Option<SourceLocation>,
}

pub enum MetaDiagnosticCode {
    DuplicateTableName,
    DuplicateFieldName,
    DuplicateIndexName,
    MissingFieldReference,
    InvalidPrimaryIndex,
    PrimaryIndexMismatch,
    EmptyIndexFields,
    UnsupportedFieldType,
    ParseError,
}
```

排序规则：

1. source path。
2. line。
3. column。
4. diagnostic code。
5. message。

第一阶段若还没有完整 `SourceLocation`，也应保证诊断向量按 code/message/table name 稳定排序。

## 2.5 Raw Syntax 与 Canonical Model 分层

建议区分两层结构：

```text
TomlMetaDocument / XmlMetaDocument  --parse-->  RawMetaDocument
RawMetaDocument                     --build-->  MetaModel
MetaModel                           --validate--> diagnostics
```

原因：

- serde 结构需要兼容 TOML/XML 写法，不应直接污染 canonical model。
- builder 可以集中处理默认值、别名、历史兼容和规范化。
- validator 可以只面对已经规范化的模型。

第一阶段可简化为：

```text
TomlMetaDocument --try_into--> MetaModel --validate
```

但代码中应保留 `build_from_document` 这样的边界函数，方便后续拆分。

## 2.6 主要 API 设计

### 2.6.1 加载入口

```rust
pub struct MetaLoader {
    options: MetaLoadOptions,
}

impl MetaLoader {
    pub fn new(options: MetaLoadOptions) -> Self;
    pub fn load_path(&self, path: impl AsRef<Path>) -> Result<MetaModel>;
    pub fn load_str(&self, input: &str, format: MetaFormat) -> Result<MetaModel>;
}
```

第一阶段可以先不引入 `MetaLoader`，直接提供：

```rust
impl MetaModel {
    pub fn from_toml_str(input: &str) -> Result<Self>;
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self>;
}
```

### 2.6.2 解析入口

```rust
pub fn parse_toml_document(input: &str) -> Result<TomlMetaDocument>;
pub fn build_meta_model(document: TomlMetaDocument) -> Result<MetaModel>;
```

要求：

- parse 阶段只处理语法错误。
- build 阶段处理默认值、字段别名、类型字符串到 enum 的转换。
- validate 阶段处理跨引用和唯一性规则。

### 2.6.3 查询方法

```rust
impl MetaModel {
    pub fn table(&self, name: &str) -> Option<&TableMeta>;
    pub fn collection_tables(&self) -> impl Iterator<Item = &TableMeta>;
}

impl TableMeta {
    pub fn field(&self, name: &str) -> Option<&FieldMeta>;
    pub fn index(&self, name: &str) -> Option<&IndexMeta>;
    pub fn primary_index(&self) -> Option<&IndexMeta>;
}
```

查询方法用于后续 data/validation/exporter，避免调用方反复手写 `iter().find(...)`。

### 2.6.4 校验方法

```rust
pub fn validate_meta_model(model: &MetaModel) -> Vec<MetaDiagnostic>;
```

校验内容按阶段逐步增加：

第一阶段：

- table name 非空且全局唯一。
- field name 在 table 内唯一。
- index name 在 table 内唯一。
- index fields 非空。
- index field path 引用存在字段。
- table `primary_index` 引用存在 index。
- index `primary = true` 与 table `primary_index` 一致。
- primary index 必须 `unique = true`。

第二阶段：

- enum/struct 类型引用存在。
- named struct 无循环依赖。
- default 能按 field type 解析。
- foreign key target 合法。
- fragment source mode 合法。

## 2.7 TOML 输入格式建议

第一阶段推荐测试以下 canonical TOML：

```toml
[project]
name = "demo"
copyright = "Copyright 2026 Demo Studio."

[[tables]]
name = "items"
kind = "collection"
desc = "所有物品配置。"
primary_index = "by_id"
sources = [{ provider = "csv", path = "items.csv" }]

[[tables.fields]]
name = "id"
type = "int"
required = true
desc = "物品 ID。"

[[tables.fields]]
name = "display_name"
type = "text"
required = true
desc = "展示名称。"

[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
primary = true
desc = "按物品 ID 查询。"
```

兼容当前最小 fixture 时，可临时接受：

```toml
[[tables]]
name = "items"
source = "items.csv"

[[tables.fields]]
name = "id"
type = "i64"
required = true
```

但新测试应优先使用 canonical 写法。

## 2.8 单元测试组织方式

### 2.8.1 模块内测试

基础 parser、builder、validator 测试放在 `src/meta.rs` 或未来 `src/meta/tests.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_collection_table_with_primary_index() { }

    #[test]
    fn rejects_duplicate_field_names() { }
}
```

优点：

- 与实现代码距离近。
- 适合快速 TDD。
- 不需要完整工程 fixture。

### 2.8.2 fixture 测试

当 Meta 样例变长后，放到：

```text
tests/fixtures/meta/
  valid_basic_collection.toml
  valid_object_table.toml
  invalid_duplicate_field.toml
  invalid_missing_index_field.toml
  invalid_primary_index_mismatch.toml
```

fixture 命名规则：

- `valid_*.toml`：期望成功。
- `invalid_*.toml`：期望返回指定 diagnostic code。
- 一个 invalid fixture 聚焦一个主要错误，避免断言不稳定。

### 2.8.3 集成测试

Meta 模块基础稳定后，再在 `tests/cli_integration.rs` 中补充：

- CLI inspect meta 能读取标准工程。
- CLI 对无效 Meta 返回非 0 exit code。
- 错误输出包含稳定 diagnostic code。

第一阶段不要求 CLI 集成测试先完成。

## 2.9 第一阶段测试内容要点

建议按以下顺序写测试：

1. `parses_basic_collection_table`
   - 输入一个 collection table。
   - 断言 table name、kind、field type、desc、source、index。
2. `parses_basic_object_table`
   - 输入一个 object table。
   - 断言无需 index 也合法。
3. `parses_project_copyright`
   - 输入 `[project] copyright = "..."`。
   - 断言保留到 `ProjectMeta`。
4. `accepts_canonical_primitive_field_types`
   - 覆盖 `int`、`float`、`bool`、`string`、`text`、`asset`。
5. `keeps_description_fields`
   - table、field、index 的 `desc` 都能保留。
6. `rejects_duplicate_table_names`
   - 两个 table 同名，产生 `DuplicateTableName`。
7. `rejects_duplicate_field_names`
   - 同一 table 下字段同名，产生 `DuplicateFieldName`。
8. `rejects_index_referencing_missing_field`
   - index fields 包含不存在字段，产生 `MissingFieldReference`。
9. `rejects_primary_index_mismatch`
   - table `primary_index = "by_id"`，另一个 index `primary = true`，产生 `PrimaryIndexMismatch`。
10. `rejects_non_unique_primary_index`
    - primary index 未设置 `unique = true`，产生 `InvalidPrimaryIndex`。

## 2.10 推荐实现顺序

### Step 1：扩展类型定义

- 增加 `MetaModel`。
- 增加 `ProjectMeta`、`TableKind`、`ValueLayout`。
- 将 `FieldType::I64/F64` 迁移或兼容到 `Int/Float`。
- 为 `desc`、`default`、`primary_index`、`primary` 增加字段。

### Step 2：增加 TOML 字符串加载入口

- 实现 `MetaModel::from_toml_str`。
- `MetaModel::from_path` 内部读取文件后调用 `from_toml_str`。
- 保留旧 `MetaSet::from_path` 时，可委托给新模型或作为兼容层。

### Step 3：实现基础校验

- 创建 `MetaDiagnosticCode`。
- 创建 `validate_meta_model`。
- 先返回 `Vec<MetaDiagnostic>`，由调用方决定是否转成 `Result`。

### Step 4：补齐单元测试

- 先使用内联 TOML 字符串。
- 每个错误测试只断言 diagnostic code 和关键 message。
- 不断运行 `cargo test meta`。

### Step 5：整理对外 API

- `src/lib.rs` 保持 `pub mod meta;`。
- 对外只暴露 canonical 类型和必要 loader。
- raw syntax 结构尽量保持私有，避免后续兼容负担。

## 2.11 与现有代码的迁移关系

当前 `src/meta.rs` 已有：

```rust
pub struct MetaSet { pub tables: Vec<TableMeta> }
pub struct TableMeta { pub name: String, pub source: String, ... }
pub enum FieldType { String, I64, F64, Bool }
pub struct IndexMeta { pub name: String, pub fields: Vec<String>, unique: bool }
```

建议迁移：

| 当前类型/字段 | 目标设计 | 迁移方式 |
| --- | --- | --- |
| `MetaSet` | `MetaModel` | 可先 `pub type MetaSet = MetaModel`，或保留兼容 wrapper。 |
| `TableMeta.source` | `TableMeta.sources` | 短期兼容 `source`；canonical 使用 `sources`。 |
| 缺少 `TableKind` | `TableKind` | 新增必填或默认字段。 |
| `FieldType::I64` | `FieldType::Int` | 解析兼容 `i64`，canonical 改为 `Int`。 |
| `FieldType::F64` | `FieldType::Float` | 解析兼容 `f64`，canonical 改为 `Float`。 |
| `FieldMeta.unique` | `IndexMeta.unique` | 新设计不再鼓励 field 级 unique。 |
| `IndexMeta.fields: Vec<String>` | `Vec<FieldPath>` | 第一阶段可仍用 String，后续替换为 FieldPath。 |

## 2.12 完成标准

Meta 模块第一阶段实现完成的判断标准：

- 新 canonical TOML 样例可以解析为 `MetaModel`。
- 所有第一阶段测试内容要点都有单元测试覆盖。
- 运行 `cargo test meta` 成功。
- 旧的基础 Meta fixture 如仍存在，应继续通过或明确迁移。
- 诊断 code 稳定，不依赖 HashMap 遍历顺序。
- 代码结构仍允许后续拆分为 `src/meta/` 目录模块。

