# 02. Meta 模块详细设计

本文档描述当前 Meta 模块的实际实现设计，用于指导后续继续扩展 Meta 能力。产品语义以 `Documentation/Design/04-meta-model.md` 为准；本文档聚焦 Rust 类型边界、生命周期表达、Load-then-Link 策略、主要方法和测试组织。

## 2.1 设计目标

Meta 模块负责把一个或多个 Meta 文件转换为可被后续模块使用的两类数据：

1. **`MetaModel`**：纯 serde-owned 配置数据，保留从 TOML 加载出的业务声明。
2. **`LinkedMetaModel<'meta>`**：从 `&'meta MetaModel` 派生出的只读借用视图，包含运行期索引和解析后的跨对象引用。

当前已实现的基础能力：

- TOML Meta 加载。
- `imports` 多文件递归加载和 canonical path 去重。
- project / enum / struct / table / field / index 建模。
- `FieldMeta.field_type` 以字符串保存，Link 后解析为 primitive / enum / struct。
- `IndexMeta.fields` 以字符串保存，Link 后解析为 `Vec<&FieldMeta>`。
- target tags / `export_when` / `row_export` 建模与公共结构校验。
- validator raw meta 建模：`ValidatorMeta`、`ValidationSeverity`、table / field validators。
- 基础结构校验与稳定诊断。
- 模块内单元测试覆盖加载、Link、imports、诊断。

计划字段说明：`validators: Vec<ValidatorMeta>` 已在 `src/meta/model.rs` 落地，当前 TOML 接受 `[[tables.validators]]` 与 `[[tables.fields.validators]]`，并由 validation registry 在 compile 阶段解释具体 `kind` 与参数；当前内置已实现 `range` field validator。`foreign_keys: Vec<ForeignKeyMeta>` 仍属于后续实现目标，当前 TOML 尚不接受 foreign key 配置。validator 详细实现见 `05-validator-framework-detail-design.md`。

## 2.2 Rust 中的“类”和继承策略

Rust 不使用面向对象继承。本文档中的“类”对应 Rust 的 `struct`、`enum`、`trait`：

- **数据结构**：`MetaModel`、`TableMeta`、`FieldMeta`、`LinkedMetaModel<'meta>`。
- **多态变体**：`TableKind`、`PrimitiveFieldType`、`LinkedField<'meta>`。
- **行为入口**：`impl MetaModel`、`link_meta_model`、`validate_meta_model`。
- **继承关系**：不设计父类/子类；用组合、枚举分发和借用视图表达关系。

Object table 与 collection table 通过 `TableKind` 区分，不拆成继承体系。Field 的 primitive / enum / struct 类型在 Link 阶段通过 `LinkedField<'meta>` 区分。

## 2.3 当前模块结构

```text
src/meta/
  mod.rs              # 对外 API 与 re-export
  model.rs            # 纯配置模型、Linked* 借用视图、查询方法
  loader.rs           # TOML 字符串/路径加载、imports 递归、合并、加载后校验
  link.rs             # MetaModel -> LinkedMetaModel<'meta> 的链接过程
  validate.rs         # Meta 结构校验
  diagnostic.rs       # MetaDiagnosticCode、MetaDiagnostic
  tests.rs            # Meta 单元测试
```

职责约束：

| 子模块 | 责任 |
| --- | --- |
| `mod.rs` | 只负责声明子模块和 re-export 对外 API。 |
| `model.rs` | 定义 serde 数据结构、linked 数据结构、查询方法和 primitive 类型解析。 |
| `loader.rs` | Meta 文件读取、TOML 反序列化、显式路径列表合并、加载后 link/validate。 |
| `link.rs` | 构建 `LinkedMetaModel<'meta>`，解析字段类型和索引字段引用。 |
| `validate.rs` | 独立结构校验；不构造运行时数据。 |
| `diagnostic.rs` | 诊断 code、message、排序和展示。 |
| `tests.rs` | 使用内联 TOML 和临时目录 fixture 测试 Meta 行为。 |

## 2.4 Load-then-Link 架构

### 2.4.1 阶段划分

```text
TOML input
  --serde load--> MetaModel
  --merge paths-> MetaModel
  --link--------> LinkedMetaModel<'meta>
  --validate----> diagnostics
```

Load 阶段只得到可拥有、可序列化、可反序列化的配置数据；Link 阶段要求所有 Meta 都已加载完成，因此能用 Rust 生命周期表达“linked view 不能长于原始 MetaModel”。

### 2.4.2 为什么不把引用写回 `MetaModel`

如果把 `LinkedField` 或 `&FieldMeta` 写回 `MetaModel` 内部，会形成“结构体拥有数据，同时字段又引用自己内部数据”的自引用结构，普通 Rust 安全借用模型不适合这样表达。

当前设计避免该问题：

```text
MetaModel owns all strings/vectors
LinkedMetaModel<'meta> borrows from &'meta MetaModel
```

因此：

- `MetaModel` 可以自由移动、序列化、测试比较。
- `LinkedMetaModel<'meta>` 不能比被借用的 `MetaModel` 活得更久。
- Link 完成后整体只读，不需要单独释放其中某个对象。
- 运行期索引可以保存 `&'meta str`、`&'meta FieldMeta`、`&'meta EnumMeta` 等引用。

## 2.5 纯配置数据结构

### 2.5.1 `MetaModel`

```rust
pub struct MetaModel {
    pub enums: Vec<EnumMeta>,
    pub structs: Vec<StructMeta>,
    pub tables: Vec<TableMeta>,
}
```

说明：

- `MetaModel` 是 serde 直接加载的 owned model。
- 不包含 `HashMap` 索引，不包含内部引用。
- `MetaSet` 当前是 `MetaModel` 的兼容别名。
- Project 信息和配置片段导入关系属于 Project Config，不属于 MetaModel。

主要方法：

```rust
impl MetaModel {
    pub fn from_toml_str(input: &str) -> Result<Self>;
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self>;
    pub fn from_paths<I, P>(paths: I) -> Result<Self>;
    pub fn link(&self) -> Result<LinkedMetaModel<'_>, Vec<MetaDiagnostic>>;
    pub fn validate(&self) -> Result<(), Vec<MetaDiagnostic>>;

    pub fn table(&self, name: &str) -> Option<&TableMeta>;
    pub fn enum_meta(&self, name: &str) -> Option<&EnumMeta>;
    pub fn struct_meta(&self, name: &str) -> Option<&StructMeta>;
    pub fn collection_tables(&self) -> impl Iterator<Item = &TableMeta>;
}
```

### 2.5.2 Project 信息边界

Project 信息由 `ProjectConfig` 管理，不再在 Meta 模块中重复定义 `ProjectMeta`。Meta 片段只描述 enum、struct、table、field、index 等数据结构语义；Project Config 顶层 `imports` 负责组合多个配置片段，Project Config 的 `[project]` 负责提供生成文件头所需的 `name`、`desc`、`copyright`。

### 2.5.3 enum 与 struct

```rust
pub struct EnumMeta {
    pub name: String,
    pub desc: Option<String>,
    pub values: Vec<EnumValueMeta>,
}

pub struct EnumValueMeta {
    pub name: String,
    pub desc: Option<String>,
}

pub struct StructMeta {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<FieldMeta>,
}
```

说明：

- enum value 名称在 enum 内唯一。
- struct field 与 table field 共用 `FieldMeta`。
- struct 的 field type 同样在 Link 阶段解析为 primitive / enum / struct。

### 2.5.4 `TableMeta`

```rust
pub struct TableMeta {
    pub name: String,
    pub kind: TableKind,
    pub desc: Option<String>,
    pub export: bool,
    pub export_when: ExportCondition,
    pub row_export: Option<RowExportPolicy>,
    pub validators: Vec<ValidatorMeta>,
    pub source: Option<String>,
    pub sources: Vec<SourceRef>,
    pub override_sources: Vec<SourceRef>,
    pub fields: Vec<FieldMeta>,
    pub fragments: Vec<FragmentMeta>,
    pub indexes: Vec<IndexMeta>,
    pub primary_index: Option<String>,
    pub preserve_source_order: bool,
    pub polymorphic: bool,
}
```

说明：

- `source` 是历史兼容入口；canonical 写法应使用 `sources`。
- `source_path()` 优先返回 `sources.first().path`，否则回退到 `source`。
- `kind` 默认 `collection`。
- `primary_index` 当前仍是字符串，校验阶段确认引用 index 是否存在并且唯一。
- `export = false` 是整表导出的硬关闭；默认 `true`。
- `export_when` 根据 target tags 判断整表是否进入 runtime data 与 codegen。
- `row_export` 只作用于 collection runtime data 行过滤，常用于 `shipping` / `debug` 等 target 差异。
- `validators` 保存 table-level / record-level validator 的 raw 配置；公共字段由 Meta validate 做轻量检查，scope、unknown kind 与专属参数由 validation registry compile 阶段诊断。当前实现已解析 TOML，但内置 validator 仅提供 field-scope `range`，因此 table-level `range` 会被 scope 校验拒绝。
- `foreign_keys` 尚未在当前 `TableMeta` 中落地。外键不是普通 validator，未来会作为一等关系数据保存：它既会隐式生成引用完整性校验，也会进入 CodeModel 影响生成代码的关联查询 API。该能力仍是 Phase 3 计划。
- `fragments` 是下一阶段 polymorphic variant source mode 的 table-local fragment schema。当前代码尚未落地该字段；实现前继续对 `SourceRef.fragments` 报 unsupported。

### 2.5.5 `FragmentMeta`

```rust
pub struct FragmentMeta {
    pub name: String,
    pub desc: Option<String>,
    pub export: bool,
    pub export_when: ExportCondition,
    pub fields: Vec<FieldMeta>,
}
```

说明：

- fragment 是 table-local 的字段组，只用于 `polymorphic = true` 的 CollectionTable。
- `export = false` 是 fragment 级运行时导出硬关闭；默认 `true`。
- `export_when` 根据 target tags 判断整个 fragment 是否进入当前 ExportView。
- `fields` 复用 `FieldMeta`，但 Phase 1 只支持基础字段语义：required、default、type、export、export_when。
- Phase 1 不支持 fragment-level validators、fragment field validators、fragment indexes、fragment row_export，也不支持 fragment fields 作为 table index key。
- fragment names 在 table scope 内唯一，且不能与 table common field names 冲突。
- fragment field names 只要求在所属 fragment 内唯一；不同 fragments 可以复用相同 field name，fragment field 也可以与 common field 同名。
- CSV header 使用 `fragment.field` 表达 fragment field，例如 `stats.attack`；common field 使用裸字段名，例如 `attack`。

### 2.5.6 `TableKind`

```rust
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    Object,
    Collection,
}
```

规则：

- `Object`：可以没有 index。
- `Collection`：可以定义 index；override 等后续逻辑会依赖 primary index。

### 2.5.7 `FieldMeta`

```rust
pub struct FieldMeta {
    pub name: String,
    pub desc: Option<String>,
    #[serde(rename = "type")]
    pub field_type: String,
    pub required: bool,
    pub default: Option<toml::Value>,
    pub layout: Option<ValueLayout>,
    pub export: bool,
    pub export_when: ExportCondition,
    pub validators: Vec<ValidatorMeta>,
    pub tool_only: Option<bool>,
    pub filter_field: Option<bool>,
    pub validator_only: Option<bool>,
    pub field_number: Option<u32>,
    pub unique: bool,
}
```

关键点：

- `field_type` 必须保持 `String`，因为它可能是 primitive，也可能是后续文件中定义的 enum / struct 名称。
- `default` 暂保留 `toml::Value`；后续可在 Link 或单独 default-link 阶段按字段类型解析为 typed default。
- `export = false` 是字段导出的硬关闭；默认 `true`。
- `export_when` 根据 target tags 判断字段是否进入 runtime row 与 codegen row type。
- `validators` 保存 field-level validator 的 raw 配置，例如 range、regex、length、asset ref syntax 等；Meta 模块只做公共字段和轻量结构校验。当前实现已接受 TOML 配置，并通过 validation registry 编译执行；已内置 `range` validator，支持 `min`、`max`、`exclusive_min`、`exclusive_max`、`allow_empty`、`severity`、`message`、`export_when`。
- `tool_only`、`filter_field`、`validator_only` 已由 target tags 机制替代；当前仅保留为 `Option<bool>` 以便检测用户仍在使用并输出 `UnsupportedFeature` 诊断，不再参与导出判断。
- `unique` 是内置便捷字段；当前基础 validation 会直接校验 single-field unique，codegen 也会生成对应 single-field unique index。复合唯一性建议优先通过 `IndexMeta { unique = true }` 表达；未来也可迁移为 implicit validator。

### 2.5.8 `ValidatorMeta`

Validator 采用注册式框架。Meta 层只保存 raw 配置，不依赖具体 validator 实现：

状态：已实现 raw meta 承载与公共结构校验。当前已定义 `ValidatorMeta` / `ValidationSeverity`，并会解析 `[[tables.fields.validators]]` / `[[tables.validators]]` TOML；具体 validator 的 `kind`、scope 与专属参数由 `src/validation/registry.rs` 和内置 validator factory 在 compile 阶段校验。当前内置执行器已实现 `range` field validator，其余 validator kind 会报告 unknown validator。

```rust
pub struct ValidatorMeta {
    pub kind: String,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub severity: ValidationSeverity,
    pub export_when: ExportCondition,
    pub message: Option<String>,
    pub params: BTreeMap<String, toml::Value>,
}
```

设计要点：

- `kind` 是 validator registry 的查找 key，例如 `range`、`unique`、`weight_sum`。
- `params` 通过 serde flatten 保存 validator 专属配置，避免每新增一个 validator 就修改 `FieldMeta` / `TableMeta` 顶层字段。
- `export_when` 控制 validator 是否在当前 target tags 下执行。
- `severity` 支持 `error` / `warning`；error 会阻断导出，warning 会保留在 `ValidationReport` 中并由 CLI 以 warning 输出但不阻断。
- Meta validate 阶段只检查公共字段、tag 合法性和明显结构错误；unknown kind、参数类型、字段引用等由 validation compile 阶段根据 registry 诊断。

详细框架见 `05-validator-framework-detail-design.md`。

### 2.5.8 `ForeignKeyMeta`

外键关系是一等 Meta 数据，不放在 `validators` 中：

状态：Phase 3 计划新增。当前实现尚未定义 `ForeignKeyMeta`，也不会解析 `[[tables.foreign_keys]]` TOML。

```rust
pub struct ForeignKeyMeta {
    pub name: String,
    pub desc: Option<String>,
    pub export: bool,
    pub export_when: ExportCondition,
    pub fields: Vec<String>,
    pub target_table: String,
    pub target_index: Option<String>,
    pub allow_empty: bool,
}
```

设计要点：

- `fields` 是 source table 中参与引用的字段列表。
- `target_table` / `target_index` 描述被引用的目标表和唯一索引；`target_index` 为空时使用目标表 primary index。
- `allow_empty` 允许 optional FK 全空时跳过引用完整性检查。
- `export` / `export_when` 控制该关系是否进入当前 target 的 CodeModel；validation 也默认只检查当前 target 可见关系。
- Link 阶段应解析为 `LinkedForeignKeyMeta<'meta>`，供 validation 隐式生成 referential-integrity validator，并供 codegen 生成关联查询 helper。

### 2.5.9 Target Tags 与导出条件

```rust
pub struct TargetTags {
    tags: BTreeSet<String>,
}

pub struct ExportCondition {
    pub any: Vec<String>,
    pub all: Vec<String>,
    pub none: Vec<String>,
}

pub struct RowExportPolicy {
    pub required_tags_field: Option<String>,
    pub excluded_tags_field: Option<String>,
    #[serde(rename = "match")]
    pub match_mode: TagMatchMode,
}

pub enum TagMatchMode {
    Any,
    All,
}
```

判断规则：

- `ExportCondition::matches(target)` 同时满足：`any` 为空或命中任意 tag；`all` 全部命中；`none` 全部未命中。
- `TableMeta::is_exported_for`、`FieldMeta::is_exported_for`、`IndexMeta::is_exported_for` 均使用 `export && export_when.matches(target)`。
- `RowExportPolicy` 从数据行中的 `required_tags_field` / `excluded_tags_field` 读取逗号分隔 tag 列表；命中 excluded tag 时该行不导出；required tags 为空时默认导出；非空时按 `match = "any" | "all"` 与 target tags 匹配。
- Meta 校验阶段确认 `row_export` 引用的字段存在，但不把它们 link 成字段引用；当前 row 过滤只需要字段名查值。

### 2.5.10 primitive 类型

```rust
pub enum PrimitiveFieldType {
    Int,
    Float,
    Bool,
    String,
    Text,
    Asset,
}
```

`PrimitiveFieldType::parse` 支持：

| Meta 字符串 | Primitive |
| --- | --- |
| `int` / `i64` | `Int` |
| `float` / `f64` | `Float` |
| `bool` | `Bool` |
| `string` | `String` |
| `text` | `Text` |
| `asset` | `Asset` |

### 2.5.11 `IndexMeta`

```rust
pub struct IndexMeta {
    pub name: String,
    pub desc: Option<String>,
    pub export: bool,
    pub export_when: ExportCondition,
    pub fields: Vec<String>,
    pub unique: bool,
    pub primary: bool,
}
```

说明：

- Raw `fields` 仍保存字段名字符串，方便 serde 加载和错误展示。
- Link 后通过 `LinkedIndexMeta.fields: Vec<&FieldMeta>` 获得真实字段引用。
- `export = false` / `export_when` 控制 index 是否生成到目标代码；primary index 如果引用了当前 target 不导出的字段，应报错；非 primary index 引用了不导出的字段时跳过该 index。
- 当前只支持 table 顶层简单字段名；后续如支持 `a.b[0].c`，应引入 `FieldPath` 和 `LinkedFieldPath<'meta>`。

## 2.6 Linked 数据结构

### 2.6.1 `LinkedField<'meta>`

```rust
pub enum LinkedField<'meta> {
    Primitive(PrimitiveFieldType),
    Enum(&'meta EnumMeta),
    Struct(&'meta StructMeta),
}
```

这是 Link 字段类型的核心结果：

- primitive 保存 copyable enum。
- enum / struct 保存对 `MetaModel` 内部 owned meta 的借用引用。
- 未解析成功时产生 `UnknownFieldType`，该字段不会进入 linked fields。

### 2.6.2 `LinkedFieldMeta<'meta>`

```rust
pub struct LinkedFieldMeta<'meta> {
    pub meta: &'meta FieldMeta,
    pub linked_field: LinkedField<'meta>,
}
```

`meta` 指向原始字段配置；`linked_field` 是解析后的字段类型。后续 validation/exporter 需要字段名、desc、default 等原始属性时从 `meta` 读取，需要类型时从 `linked_field` 读取。

### 2.6.3 `LinkedIndexMeta<'meta>`

```rust
pub struct LinkedIndexMeta<'meta> {
    pub meta: &'meta IndexMeta,
    pub fields: Vec<&'meta FieldMeta>,
}
```

`meta.fields` 保留原始字符串；`fields` 是解析到同一 table 字段列表中的真实字段引用。

### 2.6.4 `LinkedTableMeta<'meta>`

```rust
pub struct LinkedTableMeta<'meta> {
    pub meta: &'meta TableMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    pub indexes: Vec<LinkedIndexMeta<'meta>>,
    primary_index: Option<usize>,
    field_index: HashMap<&'meta str, usize>,
    index_index: HashMap<&'meta str, usize>,
}
```

查询方法：

```rust
impl<'meta> LinkedTableMeta<'meta> {
    pub fn field(&self, name: &str) -> Option<&LinkedFieldMeta<'meta>>;
    pub fn index(&self, name: &str) -> Option<&LinkedIndexMeta<'meta>>;
    pub fn primary_index(&self) -> Option<&LinkedIndexMeta<'meta>>;
}
```

说明：

- table 内字段和索引用 `HashMap<&str, usize>` 加速查询。
- `primary_index` 保存选中主索引在 `indexes` 中的下标，避免在结构体内保存指向自身 `Vec` 元素的自引用；查询时通过 `primary_index()` 返回 `&LinkedIndexMeta<'meta>`。
- `fields` 中只包含类型成功解析的字段；加载入口会把 Link diagnostics 和 validate diagnostics 合并后返回错误，避免下游拿到部分错误模型。

### 2.6.5 `LinkedStructMeta<'meta>`

```rust
pub struct LinkedStructMeta<'meta> {
    pub meta: &'meta StructMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    field_index: HashMap<&'meta str, usize>,
}
```

struct 的字段 Link 规则与 table 相同。

### 2.6.6 `LinkedMetaModel<'meta>`

```rust
pub struct LinkedMetaModel<'meta> {
    pub model: &'meta MetaModel,
    pub tables: Vec<LinkedTableMeta<'meta>>,
    pub structs: Vec<LinkedStructMeta<'meta>>,
    enum_index: HashMap<&'meta str, &'meta EnumMeta>,
    struct_meta_index: HashMap<&'meta str, &'meta StructMeta>,
    table_index: HashMap<&'meta str, usize>,
    struct_index: HashMap<&'meta str, usize>,
}
```

查询方法：

```rust
impl<'meta> LinkedMetaModel<'meta> {
    pub fn enum_meta(&self, name: &str) -> Option<&'meta EnumMeta>;
    pub fn raw_struct_meta(&self, name: &str) -> Option<&'meta StructMeta>;
    pub fn table(&self, name: &str) -> Option<&LinkedTableMeta<'meta>>;
    pub fn struct_meta(&self, name: &str) -> Option<&LinkedStructMeta<'meta>>;
}
```

`HashMap` 用于运行期查询加速。需要稳定诊断顺序时，不依赖 HashMap 遍历顺序，而是在诊断列表上排序。

## 2.7 Link 流程

入口：

```rust
pub fn link_meta_model<'meta>(
    model: &'meta MetaModel,
) -> (Option<LinkedMetaModel<'meta>>, Vec<MetaDiagnostic>);

impl MetaModel {
    pub fn link(&self) -> Result<LinkedMetaModel<'_>, Vec<MetaDiagnostic>>;
}
```

流程：

1. 从 `model.enums` 构建临时 `HashMap<&str, &EnumMeta>`。
2. 从 `model.structs` 构建临时 `HashMap<&str, &StructMeta>`。
3. 遍历 tables：
   - 为当前 table 构建 field name -> `&FieldMeta` 的临时 HashMap。
   - 调用 `link_fields` 解析每个 field 的 `field_type`。
   - 调用 `link_indexes` 将 `IndexMeta.fields` 解析为 `Vec<&FieldMeta>`。
   - 根据 `TableMeta.primary_index` 或 `IndexMeta.primary` 解析当前 table 选中的 primary index。
   - 构造 `LinkedTableMeta<'meta>`。
4. 遍历 structs，按同样方式构造 `LinkedStructMeta<'meta>`。
5. 如果没有 Link diagnostics，返回 `Some(LinkedMetaModel<'meta>)`；否则返回 `None` 和 diagnostics。

字段类型解析顺序：

```text
PrimitiveFieldType::parse(field.field_type)
  -> enum_index.get(field.field_type)
  -> struct_index.get(field.field_type)
  -> UnknownFieldType
```

## 2.8 Loader 与配置片段边界

入口：

```rust
impl MetaModel {
    pub fn from_toml_str(input: &str) -> Result<Self>;
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self>;
    pub fn from_paths<I, P>(paths: I) -> Result<Self>;
}

pub fn load_meta_path(path: &Path) -> Result<MetaModel>;
```

加载策略：

- `from_toml_str`：直接 TOML 反序列化为 `MetaModel`，随后执行 `finish_loading`。
- `from_path`：调用 `load_meta_path`。
- `from_paths`：合并多个显式传入的 Meta TOML 文件；不处理 imports。
- Project Config 的 `imports` 递归、去重、片段合并由 exporter/config resolve 层负责；Meta loader 只负责把已给定的 Meta 内容加载成 `MetaModel` 并完成 link/validate。
- `merge_models`：按传入顺序合并 enums、structs、tables。
- `finish_loading`：先 `link_meta_model` 收集未知类型等 Link diagnostics，再 `validate_meta_model` 收集结构 diagnostics，排序并去重后返回。

## 2.9 校验与诊断

### 2.9.1 `MetaDiagnostic`

```rust
pub struct MetaDiagnostic {
    pub code: MetaDiagnosticCode,
    pub message: String,
}
```

当前诊断不包含 source location；通过 `PartialOrd` / `Ord` 对 code 和 message 排序，保证输出稳定。

### 2.9.2 `MetaDiagnosticCode`

```rust
pub enum MetaDiagnosticCode {
    DuplicateTableName,
    DuplicateEnumName,
    DuplicateStructName,
    DuplicateEnumValueName,
    DuplicateFieldName,
    DuplicateIndexName,
    MissingFieldReference,
    InvalidPrimaryIndex,
    PrimaryIndexMismatch,
    EmptyIndexFields,
    UnknownFieldType,
}
```

### 2.9.3 当前校验内容

`validate_meta_model` 当前覆盖：

- table name 全局唯一。
- enum name 全局唯一。
- struct name 全局唯一。
- enum value name 在 enum 内唯一。
- struct field name 在 struct 内唯一。
- table field name 在 table 内唯一。
- table index name 在 table 内唯一。
- index fields 非空。
- index fields 引用存在字段。
- `index.primary = true` 时 index 必须 unique。
- table `primary_index` 必须引用存在的 index。
- table `primary_index` 指向的 index 必须 unique。
- 多个 `primary = true` 或 `primary_index` 与 `primary = true` 冲突时产生 `PrimaryIndexMismatch`。

`link_meta_model` 当前覆盖：

- field type 不是 primitive、enum、struct 时产生 `UnknownFieldType`。

## 2.10 已 Link 与后续可 Link 的部分

### 2.10.1 当前已 Link

| 原始配置位置 | Raw 表达 | Linked 表达 | 当前收益 |
| --- | --- | --- | --- |
| table field `type` | `FieldMeta.field_type: String` | `LinkedFieldMeta.linked_field` | 支持 primitive / enum / struct，发现未知类型。 |
| struct field `type` | `FieldMeta.field_type: String` | `LinkedFieldMeta.linked_field` | struct 字段与 table 字段共享解析逻辑。 |
| table index fields | `IndexMeta.fields: Vec<String>` | `LinkedIndexMeta.fields: Vec<&FieldMeta>` | 索引字段引用不再需要下游重复查找。 |
| table primary index | `TableMeta.primary_index: Option<String>` + `IndexMeta.primary` | `LinkedTableMeta::primary_index() -> Option<&LinkedIndexMeta>` | 直接取得当前 table 选中的主索引。 |
| table 查询 | `MetaModel.tables: Vec<TableMeta>` | `LinkedMetaModel.table_index` | 按名称快速查 linked table。 |
| struct 查询 | `MetaModel.structs: Vec<StructMeta>` | `LinkedMetaModel.struct_index` / `struct_meta_index` | 同时支持 linked struct 和 raw struct 查询。 |
| enum 查询 | `MetaModel.enums: Vec<EnumMeta>` | `LinkedMetaModel.enum_index` | enum 类型解析和查询加速。 |

### 2.10.2 建议后续 Link

| 候选位置 | Raw 表达 | 建议 Linked 表达 | 触发条件 |
| --- | --- | --- | --- |
| field path | `String` | `LinkedFieldPath<'meta>` | 支持 expanded value、嵌套 struct、数组 slot 后补充。 |
| foreign key | `ForeignKeyMeta { fields, target_table, target_index }` | `LinkedForeignKeyMeta<'meta>` | 外键是一等关系数据，validation 用于引用完整性检查，codegen 用于关联查询 helper。 |
| source fragments | `SourceRef.fragments: Vec<String>` | `Vec<&FragmentMeta>` | FragmentMeta 落地后补充。 |
| validator 引用 | validator 名字符串 | `LinkedValidator<'meta>` 或 compiled handle | validator registry 落地后补充。 |
| default value | `toml::Value` | `LinkedDefaultValue` / typed value | 需要按 `LinkedField` 判断 TOML 值是否合法。 |
| export/filter 字段引用 | 字符串或表达式 | linked field refs / compiled predicate | export view 和 filter 表达式设计落地后补充。 |
| struct dependency graph | struct field type 字符串 | DAG / topological order | 代码生成需要 struct 输出顺序或循环检测时补充。 |
| source provider | `SourceRef.provider: String` | provider registry handle | source plugin/registry 落地后补充。 |

## 2.11 TOML 输入格式

推荐 canonical TOML：

```toml
[project]
name = "demo"
copyright = "Copyright 2026 Demo Studio."

[[enums]]
name = "ItemRarity"
[[enums.values]]
name = "common"

[[structs]]
name = "Reward"
[[structs.fields]]
name = "count"
type = "int"

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
name = "rarity"
type = "ItemRarity"

[[tables.fields]]
name = "reward"
type = "Reward"

[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
primary = true
desc = "按物品 ID 查询。"
```

兼容写法：

```toml
[[tables]]
name = "items"
source = "items.csv"

[[tables.fields]]
name = "id"
type = "i64"
required = true
```

## 2.12 单元测试组织

Meta 模块测试集中在 `src/meta/tests.rs`，使用内联 TOML 字符串和临时目录构造多文件 imports。

当前测试重点：

1. `parses_basic_collection_table`
   - project copyright、collection kind、desc、source、primary index、primitive link。
2. `parses_basic_object_table_without_index`
   - object table 不需要 index，兼容 `source`。
3. `links_custom_enum_and_struct_types_after_loading_all_meta`
   - enum / struct 自定义类型在全部加载后解析为引用。
4. `supports_imported_meta_files_before_linking_field_types`
   - root 文件通过 `imports` 引入类型文件，再成功 Link。
5. `accepts_canonical_and_legacy_primitive_field_types`
   - `int` / `float` 与 `i64` / `f64` 兼容。
6. `rejects_unknown_field_type_after_linking`
   - 未知类型产生 `UnknownFieldType`。
7. duplicate 系列
   - table / field 等重复命名产生对应 diagnostic。
8. index 系列
   - 缺失字段、primary mismatch、非 unique primary index。

后续建议补充：

- duplicate enum / struct / enum value 的显式测试。
- duplicate import / import cycle 的去重测试。
- `LinkedIndexMeta.fields` 持有真实 `&FieldMeta` 的断言。
- `LinkedTableMeta::primary_index()` 在 `primary_index = "..."` 和 `primary = true` 两种写法下都能返回选中的 linked index。
- `LinkedStructMeta.field` 查询测试。
- `MetaModel::from_paths` 多入口合并测试。

## 2.13 与下游模块的接口约束

- Loader 返回 `MetaModel`，因为它是 owned 数据，便于调用方控制生命周期。
- 下游在需要类型解析或快速查询时，应在持有 `MetaModel` 的作用域内调用 `meta.link()` 得到 `LinkedMetaModel<'_>`。
- `LinkedMetaModel<'_>` 不应跨越 `MetaModel` 生命周期；通常应在 export/validate 函数栈内临时创建并传递引用。
- validation/exporter 不应再自行解析 `FieldMeta.field_type` 字符串；应读取 `LinkedFieldMeta.linked_field` 或 `primitive_type()`。
- 对 index 逻辑，应优先使用 `LinkedIndexMeta.fields`，避免重复做 field name 查找。

## 2.14 当前完成标准

Meta 模块当前阶段完成的判断标准：

- `MetaModel` 可从 TOML 字符串、路径、多路径加载。
- `imports` 能递归加载，重复 canonical path 只加载一次。
- 所有 Meta 文件加载完成后再执行 Link。
- `MetaModel` 保持纯配置；运行期 HashMap 索引只存在于 `LinkedMetaModel<'meta>` 及子结构。
- field type 从字符串 Link 为 primitive / enum / struct 引用。
- index field 从字符串 Link 为 `&FieldMeta` 引用。
- primary index 从 table/index 声明 Link 为 `LinkedTableMeta` 上可查询的 selected linked index。
- Link 和 validate diagnostics 会在加载完成前合并返回，避免下游使用不完整模型。
- 单元测试覆盖基础加载、primitive 兼容、自定义类型、imports、未知类型、重复命名和 primary index 校验。
