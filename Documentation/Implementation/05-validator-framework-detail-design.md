# 05. 注册式 Validator 框架详细设计

本文档描述 ConfigForge 后续 validator 框架的实现设计。目标是在保留现有 required / type / enum / unique / target-aware validation 行为的基础上，引入可注册、可扩展、可配置的 validator 机制，使内置校验和未来插件校验都能通过统一管线执行，而不是继续把规则硬编码在 `src/validation.rs` 中。

需要特别区分：**validator 是检查规则，foreign key 是数据关系模型**。外键关系不仅用于导出前校验，还会影响 runtime data/codegen，例如生成自动查找关联记录的 helper。因此外键不应只作为 `kind = "foreign_key"` 的普通 validator 配置存在；它应成为 Meta 中的一等关系数据，再由 validation 阶段从外键关系隐式生成 referential-integrity validator。

产品语义仍以 `Documentation/Design/05-data-validation.md` 为准；本文档聚焦 Rust 代码结构、Meta 配置承载方式、registry 编译流程、迁移路径和测试计划。

## 5.1 设计目标

### 5.1.1 必须解决的问题

1. **避免校验逻辑继续硬编码扩散**
   - 当前 `src/validation.rs` 直接写死 required、type、enum、unique index 等规则。
   - 新增 range、regex、长度、概率和跨字段规则时，如果继续堆函数，会导致模块难以维护。

2. **支持 validator 专属配置字段**
   - 例如 range validator 需要 `min`、`max`、`exclusive_min`、`exclusive_max`。
   - regex / length validator 需要 `pattern`、`min_length`、`max_length` 等专属字段。
   - 这些配置应能出现在 Meta 中，但不能让 `FieldMeta` / `TableMeta` 为每一种 validator 都新增固定字段。

3. **统一内置与扩展 validator**
   - 内置 validator：required、type、enum、range、unique、referential_integrity。
   - 未来扩展 validator：asset existence、text key、weight sum、probability、业务自定义规则。
   - 所有 validator 通过同一个 registry 注册、编译和执行。

4. **与 Target Tags / ExportView 语义一致**
   - target-specific validation 默认运行在当前 target 的 ExportView 上。
   - 被 `row_export` 排除的行不参与 collection-level validator。
   - 不导出的字段默认不参与 field-level validator，但 filter support field 仍可被 row_export 或特殊 validator 读取。

5. **给出清晰迁移路径**
   - 第一阶段不能一次重写所有 validation。
   - 应先引入 registry 和新 validator 配置，再逐步把现有 hardcoded 规则迁移进去。

### 5.1.2 非目标

- 第一阶段不实现动态加载外部二进制插件。
- 第一阶段不要求把所有诊断模型替换成结构化 `DiagnosticReport`；可以继续桥接到 `ConfigForgeError::Validation(String)`。
- 第一阶段不实现表达式语言。跨字段/跨表复杂规则先通过注册 validator 的 typed config 表达。

## 5.2 Meta 配置设计

### 5.2.1 通用 validator 配置形态

Validator 配置采用“固定公共字段 + validator 专属参数 flatten”的结构。用户在 TOML 中写：

```toml
[[tables.fields.validators]]
kind = "range"
min = 1
max = 999
message = "max_stack must be in [1, 999]"
```

对应 Rust raw meta：

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

serde 设计：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ValidatorMeta {
    pub kind: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub severity: ValidationSeverity,
    #[serde(default)]
    pub export_when: ExportCondition,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(flatten)]
    pub params: BTreeMap<String, toml::Value>,
}
```

说明：

- `kind` 是 registry lookup key，例如 `range`、`unique`、`weight_sum`。
- `name` 是可选稳定标识，便于诊断、禁用或覆盖。
- `severity` 第一阶段即进入 raw meta，但执行层必须配套 `ValidationReport` / `ValidationIssue` collector；CLI 根据是否存在 error 决定退出码，warning 只输出不阻塞构建。
- `export_when` 控制 validator 是否在当前 target tags 下执行。
- `params` 保存 validator 专属字段，避免 `FieldMeta` 为每个 validator 增加硬编码字段。
- 公共字段名属于 reserved keys：`kind`、`name`、`desc`、`severity`、`export_when`、`message`，不得被 validator 专属参数复用。
- 每个 `ValidatorFactory` 在 compile 阶段必须检查必填参数、参数类型和未知参数；未知参数必须诊断为配置错误，避免拼写错误被静默忽略。例如 `kind = "range"` 中出现 `minn = 1` 应提示 unknown parameter `minn`，并尽量给出 `min` 的建议。

### 5.2.2 Field-level validators

字段级 validator 放在字段下：

```toml
[[tables.fields]]
name = "max_stack"
type = "int"
required = true

[[tables.fields.validators]]
kind = "range"
min = 1
max = 999
```

建议 `FieldMeta` 增加：

```rust
pub struct FieldMeta {
    // existing fields...
    #[serde(default)]
    pub validators: Vec<ValidatorMeta>,
}
```

Range validator 配置字段：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `min` | integer / float，可选 | 最小值。 |
| `max` | integer / float，可选 | 最大值。 |
| `exclusive_min` | bool，默认 false | 是否不包含最小值。 |
| `exclusive_max` | bool，默认 false | 是否不包含最大值。 |
| `allow_empty` | bool，默认跟随 required | 空值是否跳过 range 检查。 |

Range validator 不直接在 `FieldMeta` 上增加 `min` / `max` 字段，原因：

- range 是 validator 专属参数，不是所有字段都有的通用属性。
- 未来新增 regex、length、asset_exists 不需要继续修改 `FieldMeta` 的顶层字段。
- registry factory 可以把 `params` 编译成 typed `RangeConfig`，既保持 meta 扩展性，又保持执行阶段类型安全。

### 5.2.3 Table-level validators

表级 validator 放在 table 下：

```toml
[[tables.validators]]
kind = "unique"
fields = ["id", "region"]
```

建议 `TableMeta` 增加：

```rust
pub struct TableMeta {
    // existing fields...
    #[serde(default)]
    pub validators: Vec<ValidatorMeta>,
}
```

Table-level validator 默认输入当前 table 的 ExportView rows。若某 validator 确实需要 Full DataSet，应显式配置：

```toml
[[tables.validators]]
kind = "weight_sum"
fields = ["weight"]
input = "full_dataset"
```

第一阶段建议只实现默认 `input = "export_view"`，保留 `input` 字段为未来扩展。

Record-level validator 也声明在 `[[tables.validators]]` 下，由 registry factory 的 `scope()` 决定按单行执行还是按整表执行。例如 begin/end 时间关系、条件 required、多字段互斥、多字段至少填一个都属于 `ValidatorScope::Record`。因此 `TableMeta.validators` 同时承载 record-level 与 table-level validator 的 raw 配置；compile 阶段根据 factory scope 生成不同执行策略。

### 5.2.4 便捷字段与 implicit validators

注册式 validator 是扩展机制，不意味着所有内置常用规则都必须手写 `validators` 数组。对于高频、稳定、语义清晰的内置规则，应继续提供便捷字段，然后在 validation compile 阶段转换成 implicit validator。

现有 `IndexMeta { unique = true }` 和 `FieldMeta { unique = true }` 必须保留。

迁移策略：

- `IndexMeta.unique = true` 在 link/compile 阶段自动生成一个 implicit `unique` validator。
- `FieldMeta.unique = true` 自动生成 synthetic single-field unique validator。
- 用户也可以显式写 `[[tables.validators]] kind = "unique"`。
- 最终执行阶段只看到 compiled validators，不关心来源是 implicit 还是 explicit。

便捷字段原则：

| 配置形态 | 是否推荐保留 | 原因 |
| --- | --- | --- |
| `required = true` | 是 | 字段基础语义，影响 default/type validation 和用户理解。 |
| `unique = true` | 是 | 高频单字段唯一性，写成 validator 太啰嗦；后台可转 implicit unique validator。 |
| `IndexMeta.unique = true` | 是 | index 的查询语义和唯一性是一体的，codegen 也依赖它决定 get/find API。 |
| `validators = [{ kind = "range", ... }]` | 是 | range 参数较多，适合注册式配置；也可未来增加 `range = { ... }` 作为语法糖。 |
| `validators = [{ kind = "foreign_key", ... }]` | 否 | 外键是关系数据，会影响 codegen，不应只作为普通 validator。 |

这样既保留用户书写便利，又让执行层遵守开闭原则：便捷字段只在 compile 阶段生成 implicit validator，不把具体检查逻辑散落在 validation pipeline 中。

### 5.2.5 Foreign key 是关系数据，不是普通 validator

外键关系应成为 `TableMeta` 上的一等 Meta 数据。推荐配置：

```toml
[[tables.foreign_keys]]
name = "item"
fields = ["item_id"]
target_table = "items"
target_index = "by_id"
allow_empty = true
```

复合外键：

```toml
[[tables.foreign_keys]]
name = "item_in_region"
fields = ["item_id", "region"]
target_table = "items"
target_index = "by_id_region"
```

建议 `TableMeta` 增加：

```rust
pub struct TableMeta {
    // existing fields...
    #[serde(default)]
    pub foreign_keys: Vec<ForeignKeyMeta>,
}

pub struct ForeignKeyMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub export_when: ExportCondition,
    pub fields: Vec<String>,
    pub target_table: String,
    #[serde(default)]
    pub target_index: Option<String>,
    #[serde(default)]
    pub allow_empty: bool,
}
```

如果 `target_index` 为空，则默认使用 target table 的 primary index。单字段便捷写法可以作为后续语法糖：

```toml
[[tables.fields]]
name = "item_id"
type = "int"
foreign_key = { name = "item", target_table = "items", target_index = "by_id" }
```

该便捷写法在 load/compile 阶段展开成 `TableMeta.foreign_keys` 中的一条关系。

外键关系的消费者：

| 消费者 | 用途 |
| --- | --- |
| Meta link | 解析 source fields、target table、target index，形成 `LinkedForeignKeyMeta<'meta>`。 |
| Validation | 从 linked foreign key 隐式生成 referential-integrity validator。 |
| CodeModel | 把可导出的外键关系转成 `CodeForeignKey`。 |
| 语言生成器 | 生成自动查找关联记录的 helper，例如 `item(row)` / `GetItem(row)` / `getItem(row)`。 |
| 文档/inspect | 展示表间关系图。 |

外键 link/compile 期应检查：

- source fields 存在；
- target table 存在；
- target index 存在；
- target index 是 unique；
- source fields 数量与 target index fields 数量一致；
- source/target key 类型兼容。

validation 执行期检查：

- optional FK 全空时跳过；
- composite FK 部分为空时报错；
- 非空 FK 必须能在 target ExportView index 中找到。

外键关系默认按 `input = "export_view"` 语义执行：source table、target table、source fields、target index fields 都必须在当前 target 的 ExportView 中可见，target 中被 `row_export` 排除的行默认不可引用。若业务确实需要校验完整数据集中的关系，后续可显式支持 `input = "full_dataset"`；此时诊断必须标明当前 target 与 input scope，避免用户误解被过滤行是否可引用。若 foreign key 自身的 `export_when` 不匹配当前 target，则该关系不参与当前 target 的 referential-integrity validation，也不进入当前 target 的 CodeModel。

codegen 规则：

- 只有 `foreign_key.export && foreign_key.export_when.matches(target_tags)` 的关系进入 CodeModel。
- source table、target table、source fields、target index fields 都必须在当前 target CodeModel 中可见，否则：
  - 若外键是当前 target 可导出的关系但字段/表不可见，应报 codegen validation error；
  - 若外键被 `export_when` 排除，则跳过。
- target index 必须唯一，生成代码才能返回单条关联记录；非唯一关系应建模为另一类 relation，例如 `many_to_one = false` / `relation = "many"`，第一阶段不支持。

## 5.3 代码模块结构

建议把当前扁平 `src/validation.rs` 逐步拆成目录模块：

```text
src/validation/
  mod.rs              # 对外 API：validate_data_set_for_target 等
  context.rs          # ValidationContext、ExportValidationView
  diagnostic.rs       # ValidationDiagnostic / ValidationIssue，桥接 ConfigForgeError
  registry.rs         # ValidatorRegistry、ValidatorFactory 注册与 lookup
  meta.rs             # ValidatorMeta、ValidationSeverity、ValidatorInputScope（也可放 meta/model.rs）
  compile.rs          # Raw ValidatorMeta -> CompiledValidator
  builtins/
    mod.rs
    required.rs       # 可第二阶段迁移
    type_check.rs     # 可第二阶段迁移
    enum_value.rs     # 可第二阶段迁移
    range.rs
    unique.rs
    referential_integrity.rs # 由 ForeignKeyMeta 隐式生成的关系完整性校验
```

短期也可以先保持 `src/validation.rs`，新增内部子模块文件，避免一次性大迁移：

```text
src/validation.rs
src/validation/registry.rs
src/validation/builtins/range.rs
src/validation/builtins/unique.rs
src/validation/builtins/referential_integrity.rs
```

Rust 支持 `src/validation.rs` 与 `src/validation/` 子目录共存；后续再把 `validation.rs` 改为 `validation/mod.rs`。

## 5.4 核心 trait 与类型

### 5.4.1 Validator scope

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatorScope {
    Field,
    Record,
    Table,
    DataSet,
}
```

Scope 说明：

| Scope | 输入 | 示例 |
| --- | --- | --- |
| `Field` | 单字段所有可见值 | required、type、enum、range、regex、length。 |
| `Record` | 单行可见字段 | begin/end 时间关系、条件 required。 |
| `Table` | 单表可见 rows | unique、权重总和、最少行数。 |
| `DataSet` | 多表视图 | 外键关系完整性、跨表一致性、资源 manifest 检查。 |

### 5.4.2 ValidatorFactory

Factory 负责把 raw `ValidatorMeta.params` 编译成类型安全的 executable validator。

```rust
pub trait ValidatorFactory: Send + Sync {
    fn kind(&self) -> &'static str;
    fn scope(&self) -> ValidatorScope;

    fn compile<'meta>(
        &self,
        meta: &'meta ValidatorMeta,
        location: ValidatorLocation<'meta>,
        linked_meta: &'meta LinkedMetaModel<'meta>,
    ) -> Result<Box<dyn CompiledValidator<'meta> + 'meta>>;
}
```

`ValidatorLocation` 描述 validator 声明所在位置：

```rust
pub enum ValidatorLocation<'meta> {
    Field {
        table: &'meta LinkedTableMeta<'meta>,
        field: &'meta LinkedFieldMeta<'meta>,
    },
    Table {
        table: &'meta LinkedTableMeta<'meta>,
    },
    Record {
        table: &'meta LinkedTableMeta<'meta>,
    },
    DataSet,
}
```

`Record` 与 `Table` 都来自 `[[tables.validators]]`，区别只在执行粒度：`Record` 对每个可见 row 分别执行，`Table` 对当前 ExportView 的整表 rows 执行。

### 5.4.3 CompiledValidator

CompiledValidator 负责执行。为了兼容不同 scope，第一阶段可用一个统一 context，并由 validator 自己读取需要的数据：

```rust
pub trait CompiledValidator<'meta>: Send + Sync {
    fn kind(&self) -> &'static str;
    fn scope(&self) -> ValidatorScope;
    fn validate<'data>(&self, ctx: &ValidationContext<'meta, 'data>, issues: &mut Vec<ValidationIssue>);
}
```

`ValidationContext`：

```rust
pub struct ValidationContext<'meta, 'data> {
    pub linked_meta: &'meta LinkedMetaModel<'meta>,
    pub target_tags: Option<TargetTags>,
    pub full_data_set: &'data DataSet,
    pub export_view: ExportValidationView<'meta, 'data>,
}
```

这里刻意区分 `'meta` 与 `'data`：`LinkedMetaModel<'meta>` 借用自 `MetaModel`，而 `DataSet` 是数据加载后的 owned 数据，不应被迫与 meta 共享同一生命周期。`TargetTags` 可以由 context 持有 owned value；如果实现希望避免 clone，也可增加独立 `'ctx` 生命周期并使用 `Option<&'ctx TargetTags>`，但不能把它绑定到 `'meta`。

`ExportValidationView` 第一阶段可以是轻量借用视图，不必复制所有数据：

```rust
pub struct ExportValidationView<'meta, 'data> {
    pub tables: Vec<ExportTableView<'meta, 'data>>,
}

pub struct ExportTableView<'meta, 'data> {
    pub meta: &'meta LinkedTableMeta<'meta>,
    pub table: &'data DataTable,
    pub visible_fields: BTreeSet<&'meta str>,
    pub visible_rows: Vec<VisibleRow>,
}

pub struct VisibleRow {
    pub row_index: usize,              // index in DataTable.rows
    pub source_row_number: Option<usize>,
}
```

`VisibleRow.row_index` 始终指向原始 `DataTable.rows` 下标，诊断中的 row 也应优先使用原始 source row number，而不是过滤后视图中的第 N 行。短期实现如果还没有 source location，可以先只填 `row_index`，但语义必须保持“原始数据行”而非“可见行序号”。

当前已经有 `validate_normalized_data_set_for_target` 的 target-aware 逻辑，后续可以把其中的 table/field/row 过滤收敛到 `ExportValidationView`。

### 5.4.4 ValidatorRegistry

```rust
pub struct ValidatorRegistry {
    factories: BTreeMap<String, Box<dyn ValidatorFactory>>,
}

impl ValidatorRegistry {
    pub fn new() -> Self;
    pub fn with_builtin_validators() -> Self;
    pub fn register(&mut self, factory: impl ValidatorFactory + 'static);
    pub fn get(&self, kind: &str) -> Option<&dyn ValidatorFactory>;
    pub fn require(&self, kind: &str) -> Result<&dyn ValidatorFactory>;
}
```

内置注册：

```rust
pub fn builtin_validator_registry() -> ValidatorRegistry {
    let mut registry = ValidatorRegistry::new();
    registry.register(RangeValidatorFactory);
    registry.register(UniqueValidatorFactory);
    registry.register(ReferentialIntegrityValidatorFactory);
    registry
}
```

## 5.5 编译流程

Validator 不应在每行执行时反复解析 TOML 参数或查找字段引用。推荐编译流程：

```text
MetaModel load
  -> meta structural validate
  -> LinkedMetaModel
  -> ValidatorRegistry compile
       - explicit validators from FieldMeta/TableMeta
       - implicit validators from required/type/enum/index unique/field unique
       - implicit referential-integrity validators from ForeignKeyMeta
       - resolve field/table/index refs
       - parse params into typed config
  -> CompiledValidationPlan
  -> validate ExportView / Full DataSet
```

```rust
pub struct CompiledValidationPlan<'meta> {
    validators: Vec<Box<dyn CompiledValidator<'meta> + 'meta>>,
}

impl<'meta> CompiledValidationPlan<'meta> {
    pub fn run<'data>(&self, ctx: &ValidationContext<'meta, 'data>) -> ValidationReport;
}
```

Validator identity 用于 implicit / explicit 去重和稳定诊断：

```rust
pub struct ValidatorIdentity {
    pub kind: String,
    pub scope: ValidatorScope,
    pub table: Option<String>,
    pub fields: Vec<String>,
    pub target_condition: ExportCondition,
    pub source: ValidatorSource,
}

pub enum ValidatorSource {
    Explicit,
    ImplicitIndex,
    ImplicitField,
    ImplicitForeignKey,
    ImplicitBuiltin,
}
```

去重策略：explicit 优先于 implicit；`kind + scope + table + fields + target_condition` 相同的 implicit/explicit validator 合并为一条执行计划；若两个 explicit validator identity 相同但参数不一致，应报 duplicate validator config，而不是重复执行并输出重复诊断。

编译期错误属于 Meta/Validation config 错误，例如：

- unknown validator kind；
- range validator 同时缺失 `min` 和 `max`；
- range validator 应用于非 numeric 字段；
- foreign key relation 的 target table/index 不存在；
- validator config 参数类型错误；
- validator config 出现未知参数或重复冲突配置。

执行期错误属于数据错误，例如：

- `max_stack = 1000` 超出 `max = 999`；
- unique key 重复；
- 外键关系指向不存在的 target row。

## 5.6 内置 validator 设计

### 5.6.1 RangeValidator

适用范围：field-level。

支持字段类型：`int`、`float`。未来可扩展 decimal。

Typed config：

```rust
pub struct RangeValidatorConfig {
    pub min: Option<NumericBound>,
    pub max: Option<NumericBound>,
    pub exclusive_min: bool,
    pub exclusive_max: bool,
    pub allow_empty: bool,
}

pub enum NumericBound {
    Int(i64),
    Float(f64),
}
```

执行规则：

- 空值：如果 field 非 required 且 `allow_empty = true`，跳过。
- `int` 字段用 `i64` 比较；`float` 字段用 `f64` 比较。
- `min > max` 是配置错误，应在 compile 阶段报错。
- 对 `bool/string/text/asset/enum/struct` 使用 range 是配置错误。

诊断示例：

```text
validation failed: table `items` row 12 field `max_stack` value `1000` exceeds max `999`
```

### 5.6.2 UniqueValidator

适用范围：table-level。

来源：

- explicit `[[tables.validators]] kind = "unique"`；
- implicit `IndexMeta.unique = true`；
- implicit `FieldMeta.unique = true`。

Typed config：

```rust
pub struct UniqueValidatorConfig<'meta> {
    pub name: String,
    pub fields: Vec<&'meta LinkedFieldMeta<'meta>>,
}
```

执行规则：

- 输入 rows 是当前 target ExportView rows。
- key 按字段顺序拼接。
- duplicate 报错时显示当前 row 与首次出现 row 的位置。
- `IndexMeta.unique = true` 仍同时保留 index/codegen 语义：CodeModel 继续根据 unique index 生成 `get_by_* -> Option<Row>`，而 validation compile 额外生成同名 implicit `UniqueValidator`。
- `FieldMeta.unique = true` 是用户便捷写法：CodeModel 可继续生成 synthetic `by_<field>` index，validation compile 同步生成 synthetic `UniqueValidator`，二者共享同一命名策略，避免“校验唯一但运行时代码没有查询 API”的语义断裂。

### 5.6.3 ReferentialIntegrityValidator

适用范围：dataset-level。它不是用户直接手写的普通 validator，而是由 `ForeignKeyMeta` 关系数据隐式生成，用于检查外键引用完整性。

Typed config：

```rust
pub struct ReferentialIntegrityValidatorConfig<'meta> {
    pub relation_name: String,
    pub source_table: &'meta LinkedTableMeta<'meta>,
    pub source_fields: Vec<&'meta LinkedFieldMeta<'meta>>,
    pub target_table: &'meta LinkedTableMeta<'meta>,
    pub target_index: &'meta LinkedIndexMeta<'meta>,
    pub allow_empty: bool,
}
```

执行规则：

- 先从 target ExportView rows 构建 target key set。
- 遍历 source ExportView rows。
- source fields 全空且 `allow_empty = true` 时跳过。
- source fields 部分为空时报错。
- 非空 key 不存在时报错。

### 5.6.4 Required / Type / Enum 的迁移

Required、type、enum 当前已存在 hardcoded 实现。建议迁移顺序：

1. 第一阶段保留现有 hardcoded required/type/enum，新增 range/unique registry，并预留外键关系到 referential-integrity validator 的编译入口。
2. 第二阶段把 required/type/enum 也转成 implicit validators。
3. 第三阶段 `validate_normalized_table_for_target` 只负责构建 context 和运行 plan，不再直接检查字段。

这样可以降低一次性重构风险。

## 5.7 与 Target Tags / ExportView 的关系

Validator 执行时必须明确输入视图：

| validator | 默认输入 | 说明 |
| --- | --- | --- |
| required/type/enum/range | 当前 target visible fields + visible rows | 不导出的字段不校验。 |
| unique | 当前 target visible rows | `row_export` 排除行不参与 duplicate 检查。 |
| referential_integrity（由 foreign key 关系生成） | 当前 target source/target visible rows | target 中被过滤的记录默认不可引用。 |
| asset/text | 当前 target visible rows | 被过滤行不阻塞当前 target。 |

Validator 自身也支持 `export_when`：

```toml
[[tables.fields.validators]]
kind = "range"
min = 1
max = 100
export_when = { any = ["client"] }
```

执行时同时满足：

```text
validator.meta.export_when.matches(target_tags)
&& validator 所在 table/field 在当前 view 可见
```

Validation 入口必须显式表达当前视图模式，避免 CLI / CI / QA 在未指定 target 时得到不稳定结果：

```rust
pub enum ValidationMode {
    FullDataSet,
    Target { target_name: String, tags: TargetTags },
    AllTargets,
}
```

CLI 规则：

- `build/export --target client`：必须使用 `ValidationMode::Target { target_name: "client", ... }`，validation 与 runtime data/codegen 使用同一个 ExportView。
- `validate --target client`：检查 client ExportView。
- `validate --all-targets`：对项目配置中的所有 target 分别派生 ExportView 并执行，诊断中标明 target 名称。
- `validate` 不带 target：如果项目未定义 targets，可执行 `FullDataSet`；如果项目定义了 targets，应要求用户显式指定 `--target` 或 `--all-targets`，避免 target-only 规则被跳过后产生“假通过”。

## 5.8 诊断模型

短期可以继续输出 `ConfigForgeError::Validation(String)`，但 validator 内部建议先生成结构化 issue：

```rust
pub struct ValidationIssue {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub table: Option<String>,
    pub field: Option<String>,
    pub row_index: Option<usize>,
    pub source_row_number: Option<usize>,
    pub validator: String,
    pub target: Option<String>,
    pub hint: Option<String>,
}

pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn has_error(&self) -> bool;
}
```

桥接策略：

- 第一阶段内部就使用 `ValidationReport` 收集 error / warning；CLI 根据 `has_error()` 决定退出码，warning 只输出不阻塞构建。
- 为兼容现有 `Result<()>` API，可在边界层把 report 中的 error issue 转成字符串并复用 `ConfigForgeError::Validation`。
- 中期引入统一 diagnostics 后，直接输出结构化 report。

错误码建议：

| Validator | code |
| --- | --- |
| required | `field.required_missing` |
| type | `field.type_mismatch` |
| enum | `field.enum_unknown_value` |
| range | `field.range_violation` |
| unique | `table.unique_duplicate` |
| referential_integrity | `dataset.foreign_key_missing` |

## 5.9 修改路径

### Phase 1：引入 registry 与 range validator

目标：证明框架可用，同时避免大规模迁移。

修改点：

1. `src/meta/model.rs`
   - 新增 `ValidatorMeta`、`ValidationSeverity`。
   - `FieldMeta` 增加 `validators: Vec<ValidatorMeta>`。
   - `TableMeta` 增加 `validators: Vec<ValidatorMeta>`。
   - 当前代码尚不接受 validator TOML；这些字段属于 Phase 1 新增能力，文档示例在实现前仅表示目标结构。

2. `src/meta/validate.rs`
   - 校验 validator `kind` 非空、tag 合法、公共字段合法。
   - 不在 Meta validate 阶段解析 registry；unknown kind 由 validation compile 阶段根据 registry 报错。

3. `src/validation/registry.rs`
   - 新增 `ValidatorRegistry`、`ValidatorFactory`、`CompiledValidator`。
   - 新增 `ValidationMode`、`ValidationReport` / `ValidationIssue` collector，明确 warning 不影响退出码。

4. `src/validation/builtins/range.rs`
   - 实现 range factory 和 compiled validator。

5. `src/validation.rs`
   - 在现有 target-aware validation 中插入 `CompiledValidationPlan::run`。
   - 第一阶段 required/type/enum/unique 可继续保留 hardcoded；range 走 registry。

6. 测试
   - range min/max 成功与失败。
   - range 参数类型错误。
   - range 未知参数报错，例如 `minn`。
   - range 应用于 string 字段报配置错误。
   - target tags 下不导出的字段不执行 range。

### Phase 2：迁移 unique validator

目标：把当前 `validate_field_uniqueness`、`validate_index_uniqueness` 迁移到 registry。

修改点：

1. `UniqueValidatorFactory` 支持 explicit unique config。
2. compile 阶段从 `FieldMeta.unique` / `IndexMeta.unique` 生成 implicit unique validators。
3. 为 explicit / implicit unique 生成稳定 `ValidatorIdentity` 并执行去重；explicit 优先于 implicit，冲突配置时报 duplicate validator config。
4. 删除或废弃 hardcoded uniqueness 函数。
5. 测试保持现有 unique 行为不回退，并覆盖 row_export 后 unique 不冲突、implicit/explicit 重复不产生重复诊断。

### Phase 3：实现 foreign key 关系数据与 referential-integrity validator

目标：支持跨表关系建模、跨表一致性校验，并让 codegen 能基于外键关系生成自动查找关联记录的 helper。

修改点：

1. `TableMeta` 增加 `foreign_keys: Vec<ForeignKeyMeta>`。
2. Link 阶段解析为 `LinkedForeignKeyMeta<'meta>`，解析 source fields、target table、target index。
3. Validation compile 阶段从 `LinkedForeignKeyMeta` 隐式生成 `ReferentialIntegrityValidator`。
4. CodeModel 增加 `CodeForeignKey`，语言生成器基于它生成关联查找 helper。
5. 执行阶段在 target ExportView 上构建 target key set。
6. 测试：
   - target row 存在通过；
   - target row 缺失报错；
   - optional FK 全空跳过；
   - composite FK 部分为空报错；
   - target tags 过滤 target row 后 FK 报错。
   - generated code 包含外键关联 helper，且 source/target 被 target tags 过滤时行为正确。

### Phase 4：迁移 required/type/enum 并引入结构化诊断

目标：`src/validation.rs` 不再硬编码具体规则，只负责上下文、视图和执行顺序。

修改点：

1. Required/Type/Enum 变为 implicit validators。
2. `ValidationIssue` 成为 validation 模块内部标准输出。
3. CLI 层统一格式化 validation diagnostics。

## 5.10 执行顺序

推荐顺序：

1. Field-level parse/type/default 相关规则。
2. Field-level value validators，例如 range、regex、length。
3. Record-level validators。
4. Table-level validators，例如 unique、row count、weight sum。
5. DataSet-level validators，例如 foreign key、cross-table consistency。

原因：

- FK / unique 依赖字段值已完成基础 parse/type 检查。
- 后续如果支持 fail-fast，可以在前置基础错误过多时跳过昂贵的 dataset-level validator。

第一阶段可以继续发现第一个错误即返回；中期应收集多个 `ValidationIssue`，统一排序后输出。

## 5.11 测试计划

### 单元测试

| 测试位置 | 覆盖内容 |
| --- | --- |
| `src/meta/tests.rs` | validator meta 解析、公共字段默认值、无效公共字段诊断。 |
| `src/validation/builtins/range.rs` | range 参数解析、边界比较、错误信息。 |
| `src/validation/builtins/unique.rs` | explicit / implicit unique、复合 key、row_export 后过滤。 |
| `src/validation/builtins/referential_integrity.rs` | 外键关系生成的完整性校验、target 查找、optional、复合 key、target ExportView 语义。 |
| `src/validation/tests.rs` 或 `src/validation.rs` | registry compile/run 流程。 |

### 集成测试

在 `tests/cli_integration.rs` 增加：

1. `range_validator_blocks_invalid_data_before_export`。
2. `range_validator_respects_target_tags`。
3. `foreign_key_relation_validates_target_export_view`。
4. `explicit_unique_validator_matches_index_unique_behavior`。

### 回归要求

- 现有 required/default/type/enum/unique 测试必须继续通过。
- target tags validation 行为不得回退。
- `cargo fmt`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test` 通过。

## 5.12 推荐第一步实现切片

最小可交付切片：

1. 增加 `ValidatorMeta` 和 `FieldMeta.validators`。
2. 实现 `ValidatorRegistry` / `RangeValidatorFactory`。
3. 在 target-aware validation 中执行 field-level explicit validators。
4. 支持 TOML：

```toml
[[tables.fields]]
name = "level"
type = "int"

[[tables.fields.validators]]
kind = "range"
min = 1
max = 100
```

5. 补充单元测试和 CLI 测试。

这个切片能验证“validator 专属配置字段 + registry + target-aware execution”的核心闭环，同时不会立即改动现有 unique 逻辑，也不会阻塞后续把外键作为一等关系数据落地，风险最低。
