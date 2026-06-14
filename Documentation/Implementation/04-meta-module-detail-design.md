# 04. Meta 模块详细设计

## 文档导航

Related docs：[`../Design/04-meta-model.md`](../Design/04-meta-model.md)、[`11-foreign-key-detail-design.md`](11-foreign-key-detail-design.md)、[`12-struct-expanded-value-detail-design.md`](12-struct-expanded-value-detail-design.md)。

- [4.1 设计目标](#41-设计目标)
- [4.3 当前模块结构](#43-当前模块结构)
- [4.4 Load-Link 架构](#44-loadlink-架构)
- [4.5 纯配置数据结构](#45-纯配置数据结构)
- [4.6 Linked 数据结构](#46-linked-数据结构)
- [4.9 校验与诊断](#49-校验与诊断)
- [4.11 TOML 输入格式](#411-toml-输入格式)


本文档描述当前 Meta 模块的实际实现设计，用于指导后续继续扩展 Meta 能力。产品语义以 `Documentation/Design/04-meta-model.md` 为准；本文档聚焦 Rust 类型边界、生命周期表达、Load-then-Link 策略、主要方法和测试组织。

## 4.1 设计目标

Meta 模块负责把一个或多个 Meta 文件转换为可被后续模块使用的两类数据：

1. **`MetaModel`**：纯 serde-owned 配置数据，保留从 TOML 加载出的业务声明。
2. **`LinkedMetaModel<'meta>`**：从 `&'meta MetaModel` 派生出的只读借用视图，包含运行期索引和解析后的跨对象引用。

当前已实现的基础能力：

- TOML Meta 加载。
- standalone Meta loader 支持单文件加载和显式 `from_paths` 合并；Project Config 层支持 `imports` 多文件递归加载和 canonical path 去重。
- project / enum / struct / table / field / index 建模。
- `FieldMeta.field_type` 以字符串保存，Link 后解析为 primitive / enum / struct。
- `IndexMeta.fields` 以字符串保存，Link 后解析为 `Vec<&FieldMeta>`。
- target tags / `export_when` / `row_export` 建模与公共结构校验。
- validator raw meta 建模：`ValidatorMeta`、`ValidationSeverity`、table / field validators。
- foreign key MVP 建模、link 和引用完整性校验。
- 基础结构校验与稳定诊断。
- 模块内单元测试覆盖加载、Link、显式多路径合并、诊断；ProjectConfig imports 由 exporter / CLI 测试覆盖。

计划字段说明：`validators: Vec<ValidatorMeta>` 已在 `src/meta/model/raw.rs` 落地并经 `src/meta/model.rs` re-export，当前 TOML 接受 `[[tables.validators]]` 与 `[[tables.fields.validators]]`，并由 validation registry 在 compile 阶段解释具体 `kind` 与参数；当前内置已实现 field-scope `range`、`regex`、`string_length`，record-scope `required_when`、`compare`，以及 table-scope `sum`、`monotonic`。`foreign_keys: Vec<ForeignKeyMeta>` 已完成 MVP，支持 table-level / field-level foreign key 声明、link、Meta 结构校验和数据引用完整性校验。validator 详细实现见 `06-validation-module-detail-design.md`。

## 4.2 Rust 中的“类”和继承策略

Rust 不使用面向对象继承。本文档中的“类”对应 Rust 的 `struct`、`enum`、`trait`：

- **数据结构**：`MetaModel`、`TableMeta`、`FieldMeta`、`LinkedMetaModel<'meta>`。
- **多态变体**：`TableKind`、`PrimitiveFieldType`、`LinkedField<'meta>`。
- **行为入口**：`impl MetaModel`；`link_meta_model` / `validate_meta_model` 是 Meta 模块内部诊断阶段函数，供 `MetaModel::link()` / `MetaModel::validate()` 复用。
- **继承关系**：不设计父类/子类；用组合、枚举分发和借用视图表达关系。

Object table 与 collection table 通过 `TableKind` 区分，不拆成继承体系。Field 的 primitive / enum / struct 类型在 Link 阶段通过 `LinkedField<'meta>` 区分。

## 4.3 当前模块结构

```text
src/meta/
  mod.rs              # 对外 API 与 re-export
  model.rs            # model 子模块声明与 re-export
  model/
    raw.rs            # serde-owned raw Meta 配置模型
    linked.rs         # Linked* 借用视图与查询方法
    types.rs          # TableKind、PrimitiveFieldType、ValueLayout 等类型枚举
    conditions.rs     # TargetTags、ExportCondition 等 target 条件模型
  loader.rs           # TOML 字符串/路径加载、显式多路径合并、加载后校验
  link.rs             # MetaModel -> LinkedMetaModel<'meta> 的链接过程
  validate.rs         # Meta 结构校验
  diagnostic.rs       # MetaDiagnosticCode、MetaDiagnostic
  tests.rs            # Meta 单元测试
```

职责约束：

| 子模块 | 责任 |
| --- | --- |
| `mod.rs` | 只负责声明子模块和 re-export 对外 API。 |
| `model.rs` | 只负责声明 `model/` 子模块并 re-export 稳定模型 API。 |
| `model/raw.rs` | 定义 serde-owned raw 配置数据结构。 |
| `model/linked.rs` | 定义 linked 借用视图、查询方法和解析后的关系结构。 |
| `model/types.rs` | 定义表类型、字段 primitive 类型、source/value layout 等枚举。 |
| `model/conditions.rs` | 定义 target tags、export condition、条件匹配逻辑。 |
| `loader.rs` | Meta 文件读取、TOML 反序列化、显式路径列表合并、加载后 link/validate。 |
| `link.rs` | 构建 `LinkedMetaModel<'meta>`，解析字段类型和索引字段引用。 |
| `validate.rs` | 独立结构校验；不构造运行时数据。 |
| `diagnostic.rs` | Meta 专属诊断 code 与 `MetaDiagnostic`；共享 severity、location 和格式化能力来自 `src/diagnostic/`。 |
| `tests.rs` | 使用内联 TOML 和临时目录 fixture 测试 Meta 行为。 |

## 4.4 Load-then-Link 架构

### 4.4.1 阶段划分

```text
TOML input
  --serde load--> MetaModel
  --merge paths-> MetaModel
  --link--------> LinkedMetaModel<'meta>
  --validate----> diagnostics
```

Load 阶段只得到可拥有、可序列化、可反序列化的配置数据；Link 阶段要求所有 Meta 都已加载完成，因此能用 Rust 生命周期表达“linked view 不能长于原始 MetaModel”。

### 4.4.2 为什么不把引用写回 `MetaModel`

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

## 4.5 纯配置数据结构

### 4.5.1 `MetaModel`

```rust
pub struct MetaModel {
    pub enums: Vec<EnumMeta>,
    pub structs: Vec<StructMeta>,
    pub tables: Vec<TableMeta>,
    pub asset_kinds: Vec<AssetKindMeta>,
}
```

说明：

- `MetaModel` 是 serde 直接加载的 owned model。
- 不包含 `HashMap` 索引，不包含内部引用。
- 对外模型统一使用 `MetaModel`。
- `asset_kinds` 是 Meta 级资源解析规则模板；Project Config 可以通过 imports 组合公共 Meta 片段，例如把 Unreal / Unity 等引擎通用规则放入 `meta/common.toml`，字段专用 kind 再用 `extends` 继承。
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

#### `AssetKindMeta`

Asset kind 是 `type = "asset"` 字段引用的 Meta 级规则模板，描述资源引用如何解析、校验和导出前补齐。资源搜索路径本身由 Project `[assets]` / `[[assets.search_paths]]` 提供，Meta 中只保存稳定的规则和 search path name 引用。

```rust
pub struct AssetKindMeta {
    pub name: String,
    pub extends: Option<String>,
    pub search_paths: Vec<String>,
    pub base_dirs: Vec<String>,
    pub extensions: Vec<String>,
    pub input: Option<AssetInputKind>,
    pub path_style: Option<AssetPathStyle>,
    pub extension_policy: Option<AssetExtensionPolicy>,
    pub strip_wrappers: Vec<String>,
    pub object_separator: Option<String>,
    pub object_name: Option<String>,
    pub case_sensitive: Option<bool>,
    pub require_existing: Option<bool>,
    pub require_sidecars: Vec<String>,
    pub allow_path_prefixes: Vec<String>,
    pub deny_path_prefixes: Vec<String>,
    pub deny_when_tags: Vec<String>,
    pub name_prefixes: Vec<String>,
    pub name_suffixes: Vec<String>,
    pub allow_patterns: Vec<String>,
    pub deny_patterns: Vec<String>,
    pub regex_rules: Vec<AssetRegexRuleMeta>,
    pub allow_auto_complete: bool,
}
```

```rust
#[serde(rename_all = "snake_case")]
pub enum AssetInputKind {
    Path,
    String,
}

#[serde(rename_all = "snake_case")]
pub enum AssetPathStyle {
    Relative,
    Mounted,
    MountedObject,
}

#[serde(rename_all = "snake_case")]
pub enum AssetExtensionPolicy {
    Required,
    Forbidden,
    Optional,
}

pub struct AssetRegexRuleMeta {
    pub name: String,
    pub target: Option<AssetRegexTarget>,
    pub pattern: String,
    pub severity: ValidationSeverity,
    pub message: Option<String>,
    pub when_tags: Vec<String>,
    pub invert: bool,
    pub case_sensitive: Option<bool>,
}
```

关键点：

- `name` 是全局唯一的 asset kind ID，供 `FieldMeta.asset_kind` 引用。
- `extends` 允许字段专用 kind 继承公共 kind，例如 `unreal_skill_class extends = "unreal_asset"`；Meta 校验拒绝未知父 kind、自继承和继承环。Resolver 使用合并后的 effective asset kind 执行校验和补齐。
- `search_paths` 引用 Project `assets.search_paths[].name`，用于把稳定 Meta 规则与本地资源根目录解耦；Project load 阶段会校验引用存在。
- `path_style` 支持 `relative`、`mounted`、`mounted_object`，覆盖常见相对路径、引擎 mount path 和 Unreal object path / class path。
- `extension_policy` 与 `extensions` 控制输入是否必须携带扩展名、禁止扩展名或可选；`allow_auto_complete` 为 `true` 时允许 resolver 在唯一候选命中时补齐扩展名、mount/object name 等 canonical ref 组成部分。
- `strip_wrappers` 用于剥离编辑器复制引用中的包装，例如 `BlueprintGeneratedClass'{value}'` 或 `Class'{value}'`。
- `object_separator` 与 `object_name` 描述 mounted object 引用中的对象名规则，例如 Unreal class `_C`。
- `require_existing` 覆盖 Project `[assets].require_existing` 的默认存在性策略；`require_sidecars` 用于 Unity `.meta` 等配对文件校验。
- `allow_path_prefixes` / `deny_path_prefixes` / `deny_when_tags` / `name_prefixes` / `name_suffixes` / `allow_patterns` / `deny_patterns` / `regex_rules` 是结构化与正则兜底校验规则；正则只参与校验，不参与路径重写或候选生成。
- 当前通过 `src/asset.rs` 的 `AssetResolver` 消费该 Meta，validation 与 runtime export 共享同一解析语义；Asset Manifest exporter 读取导出前已 canonicalize 的 DataSet，避免与 runtime data exporter 出现不同的 asset string 语义。

### 4.5.2 Project 信息边界

Project 信息由 `ProjectConfig` 管理，不再在 Meta 模块中重复定义 `ProjectMeta`。Meta 片段只描述 enum、struct、table、field、index 等数据结构语义；Project Config 顶层 `imports` 负责组合多个配置片段，Project Config 的 `[project]` 负责提供生成文件头所需的 `name`、`desc`、`copyright`。

### 4.5.3 enum 与 struct

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

### 4.5.4 `TableMeta`

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
    pub foreign_keys: Vec<ForeignKeyMeta>,
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
- `preserve_source_order` 只作用于 CollectionTable generated access 规划：为 `true` 时要求 CodeModel / 语言生成器保留 target visible rows 的稳定顺序并生成数组/下标访问 API；为 `false` 时有 index 的表默认只承诺 index lookup，不应让业务依赖 source order。它不创建 index，不参与 FK target 解析。
- `validators` 保存 table-level / record-level validator 的 raw 配置；公共字段由 Meta validate 做轻量检查，scope、unknown kind 与专属参数由 validation registry compile 阶段诊断。当前内置 record validators 包括 `required_when`、`compare`，table validators 包括 `sum`、`monotonic`；field-only validator（例如 `range`）声明在 table-level 时仍会被 scope 校验拒绝。
- `foreign_keys` 已在当前 `TableMeta` 中落地。外键不是普通 validator，而是一等关系数据：当前 MVP 会完成 link、Meta 结构校验和数据引用完整性校验；当 FK 声明 `linked_field` 时，CodeModel 会把该关系暴露给 codegen 生成关联查询 helper、cache slot 和 link/link_all API。
- `fragments` 是 polymorphic variant source mode 的 table-local fragment schema。当前代码已落地该字段，并要求多 source variant source mode 至少由一个 source 声明非空 `fragments` 驱动；`SourceRef.fragments` 的合法性由 Meta validation 检查。

`SourceRef` 用于 `sources` 与 `override_sources`：

```rust
pub struct SourceRef {
    pub provider: String,
    pub path: String,
    pub fragments: Vec<String>,
    pub export_when: ExportCondition,
}
```

说明：

- `provider` / `path` 描述物理 source；当前只支持 `csv`。
- `fragments` 表示该 source 中每行激活的 polymorphic fragments；override source 覆盖一条 variant row 时也必须声明与覆盖行语义一致的 fragments，例如 S1 的 `materials_s1.csv` 覆盖材料行时继续声明 `fragments = ["stack"]`。
- `export_when` 是 source-level target 条件，默认匹配所有 target。该字段主要用于 `override_sources`，也允许后续扩展到 base `sources` 的 target-specific source selection；实现第一阶段可以只在 override source 上启用，base source 若声明 `export_when` 可先报 unsupported 或按同一规则处理，但不能静默忽略。
- `export_when` 匹配 Build 传入的 effective target tags，即 Project target tags 与 CLI extra tags 的并集。

### 4.5.5 `FragmentMeta`

```rust
pub struct FragmentMeta {
    pub name: String,
    pub desc: Option<String>,
    pub type_name: Option<String>,
    pub export: bool,
    pub export_when: ExportCondition,
    pub fields: Vec<FieldMeta>,
}
```

说明：

- fragment 是 table-local 的字段组，只用于 `polymorphic = true` 的 CollectionTable。
- `type_name` 对应 TOML 中的 `type`，可引用已定义 `StructMeta` 作为 typed fragment payload；未设置时使用 inline fields。
- `export = false` 是 fragment 级运行时导出硬关闭；默认 `true`。
- `export_when` 根据 target tags 判断整个 fragment 是否进入当前 ExportView。
- `fields` 复用 `FieldMeta`，但 Phase 1 只支持基础字段语义：required、default、type、export、export_when。
- Phase 1 不支持 fragment-level validators、fragment field validators、fragment indexes、fragment row_export，也不支持 fragment fields 作为 table index key。
- fragment names 在 table scope 内唯一，且不能与 table common field names 冲突。
- fragment field names 只要求在所属 fragment 内唯一；不同 fragments 可以复用相同 field name，fragment field 也可以与 common field 同名。
- CSV header 使用 `fragment.field` 表达 fragment field，例如 `stats.attack`；common field 使用裸字段名，例如 `attack`。

### 4.5.6 `TableKind`

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
- ObjectTable Step 1 的过程文档已并入模块文档。当前 Meta 层限制 ObjectTable 不能声明 indexes / primary_index / row_export / fragments，也不能使用 `sources` / `override_sources`；ObjectTable 可以声明 field-level/table-level foreign keys 作为 source，但 FK target 必须是 CollectionTable unique / primary index。ObjectTable source layout 详见 `05-data-module-detail-design.md`。

### 4.5.7 `FieldMeta`

```rust
pub struct FieldMeta {
    pub name: String,
    pub desc: Option<String>,
    #[serde(rename = "type")]
    pub field_type: String,
    pub asset_kind: Option<String>,
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
    pub foreign_key: Option<FieldForeignKeyMeta>,
    pub color: Option<ColorFieldOptions>,
    pub vector: Option<VectorFieldOptions>,
}

pub struct ColorFieldOptions {
    pub format: ColorFormat,
    pub channel: ColorChannel,
    pub color_space: ColorSpace,
    pub alpha: ColorAlphaPolicy,
}

#[serde(rename_all = "snake_case")]
pub enum ColorFormat {
    HexRgb,
    HexRgba,
    RgbTuple,
    RgbaTuple,
    Object,
}

#[serde(rename_all = "snake_case")]
pub enum ColorChannel {
    Byte,
    Normalized,
}

#[serde(rename_all = "snake_case")]
pub enum ColorSpace {
    Srgb,
    Linear,
}

#[serde(rename_all = "snake_case")]
pub enum ColorAlphaPolicy {
    Required,
    DefaultOpaque,
}

pub struct VectorFieldOptions {
    pub format: VectorFormat,
    pub components: Vec<String>,
    pub unit: Option<String>,
}

#[serde(rename_all = "snake_case")]
pub enum VectorFormat {
    Tuple,
    Array,
    Object,
    Expanded,
}
```

关键点：

- `field_type` 必须保持 `String`，因为它可能是 primitive，也可能是后续文件中定义的 enum / struct 名称。
- `asset_kind` 只允许 `field_type = "asset"` 字段声明，且 asset 字段必须显式声明；它引用顶层 `MetaModel.asset_kinds[].name`，用于 target-aware asset validation 和 runtime export canonical ref 补齐。
- `default` 暂保留 `toml::Value`；后续可在 Link 或单独 default-link 阶段按字段类型解析为 typed default。
- `export = false` 是字段导出的硬关闭；默认 `true`。
- `export_when` 根据 target tags 判断字段是否进入 runtime row 与 codegen row type。
- `validators` 保存 field-level validator 的 raw 配置，例如 range、regex、string_length、asset ref syntax 等；Meta 模块只做公共字段和轻量结构校验。当前实现已接受 TOML 配置，并通过 validation registry 编译执行；已内置 field validators `range`、`regex`、`string_length`，支持公共字段 `severity`、`message`、`export_when`。
- `tool_only`、`filter_field`、`validator_only` 已由 target tags 机制替代；当前仅保留为 `Option<bool>` 以便检测用户仍在使用并输出 `UnsupportedFeature` 诊断，不再参与导出判断。
- `unique` 是内置便捷字段；当前基础 validation 会直接校验 single-field unique，codegen 也会生成对应 single-field unique index。复合唯一性建议优先通过 `IndexMeta { unique = true }` 表达；未来也可迁移为 implicit validator。
- `foreign_key` 是字段级 FK 语法糖，只适用于单 source field 引用目标表单字段 unique / primary index；加载后会在 link 阶段转换为与 table-level `ForeignKeyMeta` 等价的 linked relation。字段级 FK 可以声明 `linked_field` 以驱动 codegen 生成关联 helper。
- `color` / `vector` 是 builtin value type 的 typed option 容器。TOML 用户侧仍采用摊平字段（例如 `color_format = "hex_rgba"`、`vector_format = "tuple"`），Meta loader 在反序列化 / normalize 阶段归并为 typed option；只允许对应 `field_type = "color"` 或 `field_type` 属于 `vector2/vector3/vector4/vector*_int` 时出现。
- Color option 默认：`format = HexRgba`、`color_space = Srgb`、`alpha = Required`；`channel` 必须由用户显式声明为 `Byte` 或 `Normalized`。`ColorChannel` 不支持 `Auto`，hex 格式固定按 byte 解析；tuple / object 必须根据 `channel` 判定范围。
- Vector option 默认：`format = Tuple`、`components` 由类型维度派生（2D: `x/y`，3D: `x/y/z`，4D: `x/y/z/w`），`unit = None`。`components.len()` 必须与 vector 维度一致且组件名在同一字段内唯一。
- Link 阶段应把 builtin value type 解析为 `LinkedFieldType::BuiltinValue(BuiltinValueType)` 或等价结构，不允许用户定义同名 enum/struct 覆盖 builtin 类型名。字段 option 的结构合法性在 Meta validate 阶段诊断；source cell 的数值合法性在 DataResolver / Validation 阶段诊断。

### 4.5.8 `ValidatorMeta`

Validator 采用注册式框架。Meta 层只保存 raw 配置，不依赖具体 validator 实现：

状态：已实现 raw meta 承载与公共结构校验。当前已定义 `ValidatorMeta` / `ValidationSeverity`，并会解析 `[[tables.fields.validators]]` / `[[tables.validators]]` TOML；具体 validator 的 `kind`、scope 与专属参数由 `src/validation/registry.rs` 和内置 validator factory 在 compile 阶段校验。当前内置执行器已实现 `range`、`regex`、`string_length`、`required_when`、`compare`、`sum`、`monotonic`；其他未知 validator kind 会报告 unknown validator。

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

详细框架见 `06-validation-module-detail-design.md`。

### 4.5.9 `ForeignKeyMeta`

外键关系是一等 Meta 数据，不放在 `validators` 中：

状态：已实现 MVP。当前实现已定义 `ForeignKeyMeta` 与 field-level `FieldForeignKeyMeta`，会解析 `[[tables.foreign_keys]]` 和 `[tables.fields.foreign_key]` TOML，并在 Link / Meta validate / data validate 阶段形成闭环。

```rust
pub struct ForeignKeyMeta {
    pub name: String,
    pub desc: Option<String>,
    pub export: bool,
    pub export_when: ExportCondition,
    pub fields: Vec<String>,
    pub target_table: String,
    pub target_index: Option<String>,
    pub allow_empty: Option<bool>,
    pub linked_field: Option<String>,
}

pub struct FieldForeignKeyMeta {
    pub target_table: String,
    pub target_index: Option<String>,
    pub allow_empty: Option<bool>,
    pub linked_field: Option<String>,
}
```

设计要点：

- `fields` 是 source table 中参与引用的字段列表。
- `target_table` / `target_index` 描述被引用的目标表和唯一索引；`target_index` 为空时使用目标表 primary index。
- `allow_empty` 允许 optional FK 全空时跳过引用完整性检查；省略时按 source fields 的 `required` 派生，所有 source fields 都是 optional 时允许全空，否则不允许全空。
- `export` / `export_when` 控制该关系是否进入当前 target 的 CodeModel；validation 也默认只检查当前 target 可见关系。
- `linked_field` 是可选的生成代码 relation helper 语义名。未设置时 FK 只参与 link、validation 和 CodeModel 关系记录；设置后，codegen 为当前 target 可见 FK 生成 lazy getter、cache slot 和 link/link_all 预解析逻辑。
- Link 阶段解析为 `LinkedForeignKeyMeta<'meta>`，供 validation 执行引用完整性检查，并供 codegen 判断是否生成关联查询 helper。
- 当前 MVP 支持 CollectionTable 或 ObjectTable 作为 FK source；FK target 必须是 CollectionTable unique / primary index，ObjectTable 作为 target 暂不支持。
- 标量 list field 可作为 FK source，link 阶段会把 `LinkedField::List(Primitive/Enum)` 的元素类型作为 FK key 类型参与兼容性检查；该能力只允许单个 list source field 引用单字段 unique / primary index，不支持复合数组外键。
- `linked_field` 的 raw name 在 Meta 层只做轻量结构校验：非空、同一 source table 内不重复、不能与 source field / fragment raw name 直接冲突。目标语言关键字、大小写转换后冲突、与生成 helper 名冲突等由 Codegen naming validate 负责。

### 4.5.10 Target Tags 与导出条件

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

### 4.5.11 primitive 类型

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

### 4.5.12 `IndexMeta`

```rust
pub struct IndexMeta {
    pub name: String,
    pub desc: Option<String>,
    pub export: bool,
    pub export_when: ExportCondition,
    pub fields: Vec<String>,
    pub unique: bool,
    pub primary: bool,
    pub ordered: bool,
}
```

说明：

- Raw `fields` 仍保存字段名字符串，方便 serde 加载和错误展示。
- Link 后通过 `LinkedIndexMeta.fields: Vec<&FieldMeta>` 获得真实字段引用。
- `export = false` / `export_when` 控制 index 是否生成到目标代码；primary index 如果引用了当前 target 不导出的字段，应报错；非 primary index 引用了不导出的字段时跳过该 index。
- `ordered` 由 serde 默认 `false`，表示该索引只承诺 exact lookup；为 `true` 时表示生成代码还必须提供 lower/upper bound 与 range query。`ordered` 不影响 primary / foreign key / override identity 语义，FK target 仍只要求 target index 是 unique / primary。
- Meta validate 除了检查 key type 可 hash / equality 外，还要对 `ordered = true` 检查 key type 可稳定排序；struct/list 不能作为 ordered key。float key 若允许，应要求 finite，并可保留 warning 策略。
- 当前只支持 table 顶层简单字段名；后续如支持 `a.b[0].c`，应引入 `FieldPath` 和 `LinkedFieldPath<'meta>`。

## 4.6 Linked 数据结构

### 4.6.1 `LinkedField<'meta>`

```rust
pub enum LinkedField<'meta> {
    Primitive(PrimitiveFieldType),
    Enum(&'meta EnumMeta),
    Struct(&'meta StructMeta),
    List(Box<LinkedField<'meta>>),
}
```

这是 Link 字段类型的核心结果：

- primitive 保存 copyable enum。
- enum / struct 保存对 `MetaModel` 内部 owned meta 的借用引用。
- list 保存递归 linked element type；当前稳定输入只使用 primitive / enum element，未来可扩展 struct element 与 expanded path。
- 未解析成功时产生 `UnknownFieldType`，该字段不会进入 linked fields。

### 4.6.2 `LinkedFieldMeta<'meta>`

```rust
pub struct LinkedFieldMeta<'meta> {
    pub meta: &'meta FieldMeta,
    pub linked_field: LinkedField<'meta>,
}
```

`meta` 指向原始字段配置；`linked_field` 是解析后的字段类型。后续 validation/exporter 需要字段名、desc、default 等原始属性时从 `meta` 读取，需要类型时从 `linked_field` 读取。

### 4.6.3 `LinkedIndexMeta<'meta>`

```rust
pub struct LinkedIndexMeta<'meta> {
    pub meta: &'meta IndexMeta,
    pub fields: Vec<&'meta FieldMeta>,
}
```

`meta.fields` 保留原始字符串；`fields` 是解析到同一 table 字段列表中的真实字段引用。`ordered` 仍从 `meta.ordered` 读取，不在 linked 结构中重复存储。

### 4.6.4 `LinkedTableMeta<'meta>`

```rust
pub struct LinkedTableMeta<'meta> {
    pub meta: &'meta TableMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    pub indexes: Vec<LinkedIndexMeta<'meta>>,
    pub foreign_keys: Vec<LinkedForeignKeyMeta<'meta>>,
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

### 4.6.5 `LinkedForeignKeyMeta<'meta>`

```rust
pub struct LinkedForeignKeyMeta<'meta> {
    pub meta: LinkedForeignKeySource<'meta>,
    pub fields: Vec<&'meta FieldMeta>,
    pub target_table: &'meta TableMeta,
    pub target_index: &'meta IndexMeta,
    pub allow_empty: bool,
    pub linked_field: Option<&'meta str>,
}

pub enum LinkedForeignKeySource<'meta> {
    TableLevel(&'meta ForeignKeyMeta),
    FieldLevel {
        field: &'meta FieldMeta,
        foreign_key: &'meta FieldForeignKeyMeta,
        synthetic_name: String,
    },
}
```

说明：

- `fields` 是 source table 中真实字段引用，顺序与 raw `fields` 或字段级 source field 一致。
- `target_table` 必须指向 CollectionTable；`target_index` 是显式 `target_index` 或目标表 primary index 解析后的唯一索引。
- `allow_empty` 在 Link 阶段完成默认派生，避免 validation / codegen 重复推导。
- `linked_field` 保留 raw semantic name，供 CodeModel 构造 `CodeLinkedField`。Link 不做目标语言命名转换。
- 字段级 FK 可以通过 `LinkedForeignKeySource::FieldLevel` 保留原字段和 raw foreign_key 配置，同时生成稳定 synthetic relation name，例如 `field_name` 或 `field_name_fk`，用于诊断和 CodeModel 排序。

### 4.6.6 `LinkedStructMeta<'meta>`

```rust
pub struct LinkedStructMeta<'meta> {
    pub meta: &'meta StructMeta,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    field_index: HashMap<&'meta str, usize>,
}
```

struct 的字段 Link 规则与 table 相同。

### 4.6.7 `LinkedMetaModel<'meta>`

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

## 4.7 Link 流程

入口：

```rust
pub fn link_meta_model<'meta>(
    model: &'meta MetaModel,
) -> (Option<LinkedMetaModel<'meta>>, Vec<MetaDiagnostic>);

impl MetaModel {
    pub fn link(&self) -> Result<LinkedMetaModel<'_>, Vec<MetaDiagnostic>>;
}
```

`link_meta_model` 只收集类型、索引、fragment、foreign key 等引用解析阶段的 diagnostics；`MetaModel::link()` 面向外部调用方，会合并 `link_meta_model` 与 `validate_meta_model` 的结果，避免手工构造 raw model 时绕过结构校验。

流程：

1. 从 `model.enums` 构建临时 `HashMap<&str, &EnumMeta>`。
2. 从 `model.structs` 构建临时 `HashMap<&str, &StructMeta>`。
3. 遍历 tables：
   - 为当前 table 构建 field name -> `&FieldMeta` 的临时 HashMap。
   - 调用 `link_fields` 解析每个 field 的 `field_type`。
   - 调用 `link_indexes` 将 `IndexMeta.fields` 解析为 `Vec<&FieldMeta>`。
   - link table fragments，解析 inline / typed fragment payload。
   - link foreign keys，解析 source fields、target table、target index、`allow_empty` 默认值和可选 `linked_field`。
   - 根据 `TableMeta.primary_index` 或 `IndexMeta.primary` 解析当前 table 选中的 primary index。
   - 构造 `LinkedTableMeta<'meta>`。
4. 遍历 structs，按同样方式构造 `LinkedStructMeta<'meta>`。
5. 如果没有 Link diagnostics，返回 `Some(LinkedMetaModel<'meta>)`；否则返回 `None` 和 diagnostics。

字段类型解析顺序：

```text
PrimitiveFieldType::parse(field.field_type)
  -> enum_index.get(field.field_type)
  -> struct_index.get(field.field_type)
  -> list_element_type_name(field.field_type) recursive resolve
  -> UnknownFieldType
```

实现中先识别 `T[]` / `list<T>` 并递归解析 element type，形成 `LinkedField::List(Box<...>)`；非 list 再按 primitive / enum / struct 解析。

Foreign key link 规则：

- table-level FK 使用 `ForeignKeyMeta.fields` 查找 source table 字段；字段级 FK 使用所属 `FieldMeta` 作为唯一 source field。
- 字段级 FK 必须引用目标表单字段 unique / primary index；如果目标 index 是复合索引，应在 link/validate 阶段报错并提示改用 table-level FK。
- 省略 `target_index` 时，从 target table 的 primary index 解析；如果目标表没有 primary index，报 missing target index / invalid foreign key diagnostic。
- target table 必须是 CollectionTable；target index 必须 unique 或 primary。
- source field 类型与 target index field 类型兼容性可在 link 阶段做结构检查，也可在 data validation 编译阶段做；但 CodeModel 构建前必须保证不把不兼容 FK 暴露给 codegen。
- list FK 只允许单个 list source field 对单字段 target index，link 时使用 list element type 与 target field type 比较；复合 list FK 或多个 list source fields 必须报 unsupported/invalid diagnostic。
- `allow_empty = None` 时，字段级 FK 按 source field `required` 推导；table-level FK 按 source fields 的 `required` 组合推导，所有 source fields 都 optional 时允许全空，否则不允许全空。
- `linked_field` 不改变 link 的引用解析结果，只额外保留 helper name。raw name 为空、同表重复、与 source field/fragment raw name 冲突应由 Meta validate 报错；目标语言命名冲突由 Codegen validate 报错。

## 4.8 Loader 与配置片段边界

入口：

```rust
impl MetaModel {
    pub fn from_toml_str(input: &str) -> Result<Self>;
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self>;
    pub fn from_paths<I, P>(paths: I) -> Result<Self>;
    pub fn finish_loading(self) -> Result<Self>;
}

```

加载策略：

- `from_toml_str`：直接 TOML 反序列化为 `MetaModel`，随后执行 `finish_loading`。
- `from_path`：读取指定 Meta TOML 文件并执行 `finish_loading`。
- `from_paths`：合并多个显式传入的 Meta TOML 文件；不处理 imports。
- Project Config 的 `imports` 递归、去重、片段合并由 Project 模块负责；Meta loader 只负责把已给定的 Meta 内容加载成 `MetaModel` 并完成 link/validate。
- `merge_models`：按传入顺序合并 enums、structs、tables。
- `finish_loading` / `MetaModel::finish_loading()`：统一复用 `MetaModel::link()` 的 link + validate 诊断生命周期，排序并去重后返回；该方法只返回已完成校验的 owned raw model，不保存 borrowed linked view。

## 4.9 校验与诊断

### 4.9.1 `MetaDiagnostic`

```rust
pub struct MetaDiagnostic {
    pub code: MetaDiagnosticCode,
    pub message: String,
}
```

当前诊断不包含 source location；通过 `PartialOrd` / `Ord` 对 code 和 message 排序，保证输出稳定。

### 4.9.2 `MetaDiagnosticCode`

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

### 4.9.3 当前校验内容

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
- ObjectTable Step 1 不支持 indexes / primary_index / row_export / fragments / multi-source declarations；ObjectTable 可以声明 table-level `foreign_keys` 或 field-level `foreign_key` 作为 FK source。
- 任意 table 的 foreign key target 不能指向 ObjectTable。
- `linked_field` raw name 非空、在同一 source table 内唯一、不能与 source table common field name 或 fragment name 直接重复。
- 字段级 FK 的 `linked_field` 与 table-level FK 的 `linked_field` 使用同一 source table scope 检查重复；同一字段不能同时通过字段级 FK 和 table-level FK 生成同名 relation helper。
- `linked_field` 只做 Meta raw scope 检查；大小写折叠、case style 转换、目标语言关键字和与生成 runtime helper 冲突仍属于 Codegen validate。

`link_meta_model` 当前覆盖：

- field type 不是 primitive、enum、struct、list 时产生 `UnknownFieldType`。
- foreign key source field、target table、target index 和 primary-index fallback 的引用解析。
- foreign key `allow_empty` 默认派生和 list/composite FK 基础结构约束。

## 4.10 已 Link 与后续可 Link 的部分

### 4.10.1 当前已 Link

| 原始配置位置 | Raw 表达 | Linked 表达 | 当前收益 |
| --- | --- | --- | --- |
| table field `type` | `FieldMeta.field_type: String` | `LinkedFieldMeta.linked_field` | 支持 primitive / enum / struct / list，发现未知类型。 |
| struct field `type` | `FieldMeta.field_type: String` | `LinkedFieldMeta.linked_field` | struct 字段与 table 字段共享解析逻辑。 |
| table index fields | `IndexMeta.fields: Vec<String>` | `LinkedIndexMeta.fields: Vec<&FieldMeta>` | 索引字段引用不再需要下游重复查找。 |
| table primary index | `TableMeta.primary_index: Option<String>` + `IndexMeta.primary` | `LinkedTableMeta::primary_index() -> Option<&LinkedIndexMeta>` | 直接取得当前 table 选中的主索引。 |
| typed / inline fragment | `FragmentMeta.type_name` / `FragmentMeta.fields` | `LinkedFragmentPayload<'meta>` | fragment payload 可统一表现为 struct typed payload 或 inline fields。 |
| foreign key | `ForeignKeyMeta { fields, target_table, target_index, linked_field }` / `FieldForeignKeyMeta` | `LinkedForeignKeyMeta<'meta>` | 外键关系可直接用于引用完整性校验，并为 CodeModel 生成 `linked_field` relation helper 提供输入。 |
| table 查询 | `MetaModel.tables: Vec<TableMeta>` | `LinkedMetaModel.table_index` | 按名称快速查 linked table。 |
| struct 查询 | `MetaModel.structs: Vec<StructMeta>` | `LinkedMetaModel.struct_index` / `struct_meta_index` | 同时支持 linked struct 和 raw struct 查询。 |
| enum 查询 | `MetaModel.enums: Vec<EnumMeta>` | `LinkedMetaModel.enum_index` | enum 类型解析和查询加速。 |

### 4.10.2 建议后续 Link

| 候选位置 | Raw 表达 | 建议 Linked 表达 | 触发条件 |
| --- | --- | --- | --- |
| field path | `String` | `LinkedFieldPath<'meta>` | 支持 expanded value、嵌套 struct、数组 slot 后补充。 |
| source fragments | `SourceRef.fragments: Vec<String>` | `Vec<&FragmentMeta>` | source 到 fragment 声明的引用加速。 |
| validator 引用 | validator 名字符串 | `LinkedValidator<'meta>` 或 compiled handle | validator registry 落地后补充。 |
| default value | `toml::Value` | `LinkedDefaultValue` / typed value | 需要按 `LinkedField` 判断 TOML 值是否合法。 |
| export/filter 字段引用 | 字符串或表达式 | linked field refs / compiled predicate | export view 和 filter 表达式设计落地后补充。 |
| struct dependency graph | struct field type 字符串 | DAG / topological order | 代码生成需要 struct 输出顺序或循环检测时补充。 |
| source provider | `SourceRef.provider: String` | provider registry handle | source plugin/registry 落地后补充。 |

## 4.11 TOML 输入格式

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

单 source 简写：

```toml
[[tables]]
name = "items"
source = "items.csv"

[[tables.fields]]
name = "id"
type = "i64"
required = true
```

## 4.12 单元测试组织

Meta 模块测试集中在 `src/meta/tests.rs`，使用内联 TOML 字符串和临时目录构造多文件显式加载场景；ProjectConfig imports 递归测试由 exporter / CLI 相关测试覆盖。

当前测试重点：

1. `parses_basic_collection_table`
   - project copyright、collection kind、desc、source、primary index、primitive link。
2. `parses_basic_object_table_without_index`
   - object table 不需要 index，兼容 `source`。
3. `links_custom_enum_and_struct_types_after_loading_all_meta`
   - enum / struct 自定义类型在全部加载后解析为引用。
4. `supports_imported_meta_files_before_linking_field_types`
   - root/types 文件通过 `MetaModel::from_paths` 显式合并后，再成功 Link。
5. `accepts_canonical_and_legacy_primitive_field_types`
   - `int` / `float` 与 `i64` / `f64` 兼容。
6. `rejects_unknown_field_type_after_linking`
   - 未知类型产生 `UnknownFieldType`。
7. duplicate 系列
   - table / field 等重复命名产生对应 diagnostic。
8. index 系列
   - 缺失字段、primary mismatch、非 unique primary index。

计划补充：

- duplicate enum / struct / enum value 的显式测试。
- ProjectConfig import / import cycle 的去重测试。
- `LinkedIndexMeta.fields` 持有真实 `&FieldMeta` 的断言。
- `LinkedTableMeta::primary_index()` 在 `primary_index = "..."` 和 `primary = true` 两种写法下都能返回选中的 linked index。
- `LinkedStructMeta.field` 查询测试。
- `MetaModel::from_paths` 多入口合并测试。

## 4.13 与下游模块的接口约束

- Loader 返回 `MetaModel`，因为它是 owned 数据，便于调用方控制生命周期。
- 下游在需要类型解析或快速查询时，应在持有 `MetaModel` 的作用域内调用 `meta.link()` 得到 `LinkedMetaModel<'_>`。
- `MetaModel::from_toml_str` / `from_path` / `from_paths` 返回的是已完成 link + validate 检查的 raw model；raw model 本身不保存 borrowed links。
- `MetaModel::finish_loading()` 是给外部已构造 raw model 使用的统一收口入口，例如 Project 模块抽取出的 raw `MetaModel`；它不重新实现诊断合并逻辑，而是复用 `MetaModel::link()`。
- `link_meta_model()` 每次调用都会基于 raw model 构造 runtime borrowed view并只返回引用解析 diagnostics；它不再从 `meta` 模块入口 re-export，外部调用方应优先使用 `MetaModel::link()`。`MetaModel::link()` 会额外合并结构校验 diagnostics，调用方只能在 diagnostics 为空时消费 linked model；如果 diagnostics 非空，linked model 视为不可用。
- `ProjectConfig::raw_meta_model()` 只抽取 raw model，不做 Meta 校验；`build_target()` 对该 raw model 调用 `MetaModel::link()`，由 Meta 模块统一完成 link + validate 并返回当前执行阶段使用的 borrowed view。
- `LinkedMetaModel<'_>` 不应跨越 `MetaModel` 生命周期；通常应在 export/validate 函数栈内临时创建并传递引用。
- validation/exporter 不应再自行解析 `FieldMeta.field_type` 字符串；应读取 `LinkedFieldMeta.linked_field` 或 `primitive_type()`。
- 对 index 逻辑，应优先使用 `LinkedIndexMeta.fields`，避免重复做 field name 查找。

## 4.14 当前完成标准

Meta 模块当前阶段完成的判断标准：

- `MetaModel` 可从 TOML 字符串、路径、多路径加载。
- standalone Meta loader 支持显式多路径合并；ProjectConfig imports 能递归加载，重复 canonical path 只加载一次。
- 所有 Meta 文件加载完成后再执行 Link。
- `MetaModel` 保持纯配置；运行期 HashMap 索引只存在于 `LinkedMetaModel<'meta>` 及子结构。
- field type 从字符串 Link 为 primitive / enum / struct 引用。
- index field 从字符串 Link 为 `&FieldMeta` 引用。
- primary index 从 table/index 声明 Link 为 `LinkedTableMeta` 上可查询的 selected linked index。
- Link 和 validate diagnostics 会在加载完成前合并返回，避免下游使用不完整模型。
- 单元测试覆盖基础加载、primitive 兼容、自定义类型、显式多路径合并、未知类型、重复命名和 primary index 校验；ProjectConfig imports 由 exporter / CLI 相关测试覆盖。
