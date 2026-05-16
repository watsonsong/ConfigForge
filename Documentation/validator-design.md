# Validator Design

ConfigForge 的核心价值之一是“在导出前发现错误”。Validator 系统负责把 Meta、Source、归一化后的数据、索引、外键、导出配置等输入组合起来，执行确定性的正确性检查，并输出可定位、可批量修复的诊断信息。

本文档设计 Validator 的代码结构、配置模型、执行阶段，以及值得实现的检查项清单。它是设计文档，不要求当前代码已经实现。

## 设计目标

- 支持内置 Validator，例如 required、unique index、foreign key、struct cycle、source conflict。
- 支持通过 Meta 或独立配置声明额外 Validator，例如数值范围、字符串格式、跨字段关系。
- 支持不同作用域的检查：字段级、对象/行级、集合级、多集合级、全局工程级。
- 支持确定性执行顺序和稳定诊断输出。
- 支持一次运行收集尽可能多的错误，而不是遇到第一个错误就停止。
- 支持未来扩展自定义 Validator 或插件 Validator。
- 支持按阶段串行、阶段内并行的 Validator 执行模型。
- 保持 Validator 与 Source Provider、Exporter 解耦；Validator 操作的是归一化数据与 Meta 模型。

实现层面的插件注册、并行执行、异步边界和诊断报告要求见 `Documentation/implementation-requirements.md`。

## 核心概念

| Concept | Suggested Symbol | Meaning |
| --- | --- | --- |
| Validator | `Validator` | 一个可执行的检查单元。 |
| Validator Kind | `ValidatorKind` | Validator 类型，例如 `required`、`range`、`foreign_key`。 |
| Validation Scope | `ValidationScope` | Validator 的作用域，例如 field、record、collection、multi_collection、project。 |
| Validation Phase | `ValidationPhase` | 执行阶段，例如 meta、source、field、record、collection、project、export。 |
| Validation Context | `ValidationContext` | Validator 执行时可读的 Meta、数据、索引、source location、配置。 |
| Validation Diagnostic | `ValidationDiagnostic` | 一个错误/警告/信息级诊断。 |
| Validation Registry | `ValidatorRegistry` | 内置和扩展 Validator 的注册表。 |
| Validation Plan | `ValidationPlan` | 针对一个项目解析出来的 Validator 执行计划。 |
| Validation Severity | `ValidationSeverity` | `error`、`warning`、`info`。 |

## Validator 作用域

Validator 应按作用域建模，而不是全部塞进一个“表级约束”概念中。

### Field Validator

字段级 Validator 检查一个字段值本身。

适用示例：

- required；
- type parse；
- numeric min/max；
- string length；
- regex；
- enum value；
- asset exists；
- non-empty array；
- array length；
- nested struct required child field。

输入通常是：table、record/object、field path、typed value、raw value、source location、field meta。

### Record Validator

对象/行级 Validator 检查一个逻辑对象内部多个字段之间的关系。ObjectTable 的顶层对象和 CollectionTable 的每一行 record 都可以作为 record scope。

适用示例：

- `start_time <= end_time`；
- `min_level <= max_level`；
- 如果 `type = "weapon"`，则 `weapon_stats` 必填；
- `price = 0` 时 `sellable = false`；
- 两个字段不能同时为空或不能同时存在；
- fragment presence 与字段值一致。

输入通常是：table、record key、typed record、field lookup、source locations。

### Collection Validator

集合级 Validator 检查一个表最终逻辑集合中的多个对象。它在 source merge、override apply、字段解析之后执行。

适用示例：

- unique index；
- dense integer index；
- ordered index key monotonicity；
- table row count min/max；
- ID range；
- 同一 group 内排序值唯一；
- 权重总和等于 10000；
- 概率配置不超过 1.0；
- 同一等级段不能重叠；
- 树/图结构无环。

输入通常是：table、所有 records、已构建或待构建 indexes、collection-level config。

### Multi-Collection Validator

多集合 Validator 检查多个表之间的关系。

适用示例：

- foreign key；
- composite foreign key；
- 引用目标需要拥有某个 fragment；
- 跨表 ID 命名空间不冲突；
- 任务链引用的任务存在且无环；
- 掉落表引用的 item 与 reward 表一致；
- 多张 region/version 表之间覆盖规则一致。

输入通常是：多个 table 的 records、indexes、primary index、foreign-key config。

### Project / Export Validator

工程级或导出级 Validator 检查 Meta、配置、导出目标或全局约束。

适用示例：

- Meta 名称唯一；
- enum / named struct 引用存在；
- named struct 无循环；
- source provider declaration 合法；
- export type mapping 引用有效；
- 目标导出路径不冲突；
- 所有导出目标都有安全默认类型映射。

## 执行阶段

推荐把 Validator 分阶段执行，避免某些检查依赖尚未准备好的数据。

1. **Meta Validation**：检查 Meta 自身结构，例如 table/field/enum/struct 名称、struct cycle、index 定义。
2. **Source Declaration Validation**：检查 source provider、path/uri/sheet/range/auth_ref 等声明。
3. **Source Normalization Validation**：检查 header mapping、field path、重复列、JSON cell 解析。
4. **Field Validation**：对 typed value 执行 required、type、enum、range、string、asset 等检查。
5. **Record Validation**：对 ObjectTable 顶层对象或 CollectionTable 每行执行跨字段检查。
6. **Source Composition Validation**：检查 base source split conflict、override conflict、override missing target。
7. **Collection Validation**：检查 unique index、dense index、集合统计、组内唯一、概率/权重等。
8. **Multi-Collection Validation**：检查 foreign key、跨表一致性、跨表图结构等。
9. **Export Validation**：检查 export target、type mapping、输出路径、生成器能力。

阶段之间可以短路，也可以降级。例如 Meta 解析失败时不应继续读数据；但某个字段 range 失败不应阻止同表其他字段继续检查。

阶段之间必须保持确定性顺序；阶段内部可以把独立任务并行化。例如 Field Validation 可以按表/记录/字段分片，Record Validation 可以按记录分片，Collection Validation 可以按表分片。并行任务应写入线程本地 `DiagnosticCollector`，再按稳定排序规则合并。

## 并行执行与诊断收集

Validator Runner 应先构建 `ValidationPlan`，再执行任务。计划阶段负责决定哪些 validator 可以并行，哪些 validator 必须串行。

推荐规则：

- Validator 默认应是只读输入、无共享可变状态；
- 需要外部服务、全局缓存或有顺序依赖的 Validator 必须声明执行约束；
- 同一 `ValidationPhase` 内的独立任务可以并行；
- 不同 `ValidationPhase` 不应乱序执行；
- 每个任务产出的 diagnostics 先进入局部 collector；
- 阶段结束后统一合并、排序、去重和输出；
- 严重错误可以阻止后续阶段，但不应阻止当前阶段中已可安全执行的其他任务。

推荐诊断排序 key：severity、source file/source id、table、row、column、field path、diagnostic code、message。

## 推荐代码结构

未来 Rust 实现可以把 Validator 拆成接口、上下文、注册表、执行器、内置实现和配置解析几层。

```text
src/
  validation/
    mod.rs
    context.rs
    diagnostic.rs
    plan.rs
    registry.rs
    runner.rs
    scope.rs
    collector.rs
    reporter.rs
    validators/
      mod.rs
      meta.rs
      source.rs
      field.rs
      record.rs
      collection.rs
      multi_collection.rs
      export.rs
```

概念性 Rust 草图：

```rust
enum ValidationScope {
    Field,
    Record,
    Collection,
    MultiCollection,
    Project,
    Export,
}

enum ValidationPhase {
    Meta,
    SourceDeclaration,
    SourceNormalization,
    Field,
    Record,
    SourceComposition,
    Collection,
    MultiCollection,
    Export,
}

enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

struct ValidationDiagnostic {
    severity: ValidationSeverity,
    code: String,
    message: String,
    table: Option<String>,
    field_path: Option<FieldPath>,
    source_location: Option<SourceLocation>,
    related_locations: Vec<SourceLocation>,
}

trait Validator {
    fn id(&self) -> &'static str;
    fn scope(&self) -> ValidationScope;
    fn phase(&self) -> ValidationPhase;
    fn validate(&self, ctx: &ValidationContext, out: &mut Vec<ValidationDiagnostic>);
}
```

如果不同 scope 的输入差异很大，实际实现可以拆成多个 trait：

```rust
trait FieldValidator {
    fn validate_field(&self, ctx: &FieldValidationContext, out: &mut Vec<ValidationDiagnostic>);
}

trait RecordValidator {
    fn validate_record(&self, ctx: &RecordValidationContext, out: &mut Vec<ValidationDiagnostic>);
}

trait CollectionValidator {
    fn validate_collection(&self, ctx: &CollectionValidationContext, out: &mut Vec<ValidationDiagnostic>);
}

trait MultiCollectionValidator {
    fn validate_project(&self, ctx: &ProjectValidationContext, out: &mut Vec<ValidationDiagnostic>);
}
```

推荐实际实现采用“分 scope trait + 统一 runner”的方式，因为字段级和多集合级 Validator 的输入差异很大，强行统一一个 `validate()` 容易导致 context 过大。

## 内置 Validator 与配置 Validator

### 内置 Validator

内置 Validator 来自 Meta 语义本身，即使用户不额外配置也必须执行。

示例：

- field required；
- primitive type parse；
- builtin value parse, such as color/vector/rotation/datetime；
- enum value exists；
- named struct reference exists；
- named struct cycle detection；
- array element type validation；
- index field exists；
- unique index duplicate check；
- dense integer index no holes；
- primary_index points to unique index；
- foreign key target exists；
- base source unique conflict；
- override source duplicate key conflict。

### 配置 Validator

配置 Validator 由 Meta 或独立 validation 配置声明。它们表达项目/业务规则。

配置可以挂在不同层级：

- field validators：声明在字段上；
- record validators：声明在 table 上；
- collection validators：声明在 table 上；
- multi_collection validators：声明在 MetaModel 或 validation config 上；
- export validators：声明在 export target 上。

TOML 草案：

```toml
[[tables.fields]]
name = "level"
type = "int"
required = true

[[tables.fields.validators]]
kind = "range"
min = 1
max = 100
severity = "error"

[[tables.validators]]
scope = "record"
kind = "compare"
left = "min_level"
op = "<="
right = "max_level"

[[tables.validators]]
scope = "collection"
kind = "group_unique"
group_by = ["chapter_id"]
fields = ["stage_index"]

[[validators]]
scope = "multi_collection"
kind = "foreign_key"
source_table = "rewards"
source_fields = ["item_id"]
target_table = "items"
target_index = "by_id"
```

XML 草案：

```xml
<field name="level" type="int" required="true">
  <validator kind="range" min="1" max="100" severity="error" />
</field>

<table name="stages" kind="collection">
  <validator scope="record" kind="compare" left="min_level" op="&lt;=" right="max_level" />
  <validator scope="collection" kind="group_unique" groupBy="chapter_id" fields="stage_index" />
</table>

<validator scope="multi_collection" kind="foreign_key" sourceTable="rewards" sourceFields="item_id" targetTable="items" targetIndex="by_id" />
```

## Validator 配置原则

- `required`、`unique index`、`foreign key` 这类 Meta 语义可以有简写字段，也可以归一化成内置 Validator。
- 业务 Validator 尽量声明为数据，而不是硬编码在导出流程中。
- Validator 配置应有稳定 `kind`，便于错误码、文档、插件注册。
- Validator 参数应可被 schema 校验，避免拼错参数到运行时才发现。
- 默认严重级别是 `error`，可显式降为 `warning`。
- 所有 Validator 都应输出稳定错误码，例如 `field.required_missing`、`index.unique_duplicate`。
- 错误信息必须包含 table、field path、row/cell/source location；跨对象错误应包含 related locations。

## 值得实现的检查项清单

### Meta / Schema 检查

| Validator | Scope | Value |
| --- | --- | --- |
| table name unique | project | 避免导出产物和引用歧义。 |
| field name unique | table / struct | 避免 field path 歧义。 |
| enum variant unique | enum | 避免枚举解析歧义。 |
| named struct exists | project | 防止 struct 引用悬空。 |
| named struct no cycle | project | 防止无限嵌套。 |
| index field exists | table | 防止索引引用不存在字段。 |
| primary_index valid | table | 保证 primary_index 指向唯一索引。 |
| collection access strategy | table | 保证 CollectionTable 至少有索引或 `preserve_source_order = true`。 |
| fragment name unique | table | 保证 fragment 查询 API 稳定。 |
| validator config schema | project | 提前发现 Validator 配置参数错误。 |

### Source / Layout 检查

| Validator | Scope | Value |
| --- | --- | --- |
| source provider known | source | 避免无法加载 source。 |
| source locator exists | source | 检查 path/uri/sheet/range。 |
| header path exists | table | 避免表头拼错导致静默丢数据。 |
| duplicate header path | table | 避免一个字段被多个列写入。 |
| required header exists | table | 在读数据前发现缺列。 |
| JSON cell shape | field | 确保 JSON 与 FieldType 匹配。 |
| expanded array index contiguous | field / table | 避免数组展开漏槽位。 |
| unknown column policy | table | 控制是否允许备注列/临时列。 |

### Field 检查

| Validator | Scope | Value |
| --- | --- | --- |
| required | field | 必填字段不能为空。 |
| type parse | field | 保证 int/float/bool 等类型正确。 |
| builtin value parse | field | 保证 color/vector/rotation/datetime/range 等内置值类型格式正确。 |
| color channel range | field | 保证颜色通道在 byte 或 normalized 范围内。 |
| vector dimension | field | 保证 vector2/3/4 组件数量正确。 |
| rotation unit/order | field | 保证 rotation 单位和欧拉顺序配置合法。 |
| datetime pattern parse | field | 保证 datetime/date/time 按配置 pattern 可解析。 |
| enum value | field | 保证有限状态正确。 |
| range min/max | field | 等级、数量、概率、权重等常见约束。 |
| string length | field | 控制 ID、名称、描述长度。 |
| regex pattern | field | 校验 code、资源路径、命名规范。 |
| allowed values | field | 轻量枚举或临时白名单。 |
| array length | field | 控制奖励、消耗、条件数量。 |
| no duplicate array element | field | 防止同一奖励/条件重复配置。 |
| asset exists | field | 防止运行时资源丢失。 |
| localized text key valid | field | 防止文本缺本地化 key。 |

### Record / Object 检查

| Validator | Scope | Value |
| --- | --- | --- |
| field compare | record | `min <= max`、`start <= end`。 |
| conditional required | record | 某字段满足条件时另一个字段必填。 |
| mutually exclusive fields | record | 两个字段不能同时填写。 |
| at least one of fields | record | 多个字段至少填一个。 |
| derived consistency | record | 例如 `total = a + b + c`。 |
| fragment required by discriminator | record | 多态记录中 type 与 fragment 一致。 |
| time window valid | record | 活动、任务、商店开放时间合法。 |

### Collection 检查

| Validator | Scope | Value |
| --- | --- | --- |
| unique index | collection | 防止查找歧义。 |
| composite unique | collection | 防止组合键重复。 |
| dense integer index | collection | 支持数组式运行时存储。 |
| group unique | collection | 同 group 内序号/code 唯一。 |
| ordered monotonic | collection | 等级表、经验表、阶段表有序。 |
| no range overlap | collection | 等级段、时间段、区间配置不重叠。 |
| range coverage | collection | 区间覆盖完整，无缺口。 |
| weight sum | collection | 掉落权重或概率和符合预期。 |
| tree no cycle | collection | 任务链、科技树、章节树不能成环。 |
| parent exists | collection | 同表父子关系引用存在。 |
| source split conflict | collection / object | base sources 不允许重复逻辑单位。 |
| override target exists | collection / object | override 默认不能覆盖不存在对象/字段。 |

### Multi-Collection 检查

| Validator | Scope | Value |
| --- | --- | --- |
| foreign key exists | multi_collection | 防止引用不存在记录。 |
| composite foreign key | multi_collection | 支持组合索引引用。 |
| foreign key required fragment | multi_collection | 引用多态表时要求目标拥有某 fragment。 |
| cross-table ID namespace | multi_collection | 多表共享 ID 空间时防冲突。 |
| cross-table graph no cycle | multi_collection | 跨表任务/科技/剧情链路无环。 |
| reward item consistency | multi_collection | reward 表引用 items、currencies 等合法目标。 |
| version/layer consistency | multi_collection | 多版本 source layer 引用关系一致。 |

### Export 检查

| Validator | Scope | Value |
| --- | --- | --- |
| export target valid | export | target language/engine/data_format 支持。 |
| type mapping exists | export | 所有逻辑类型有安全目标表示。 |
| target type import valid | export | 外部目标类型 import/include 可生成。 |
| output path conflict | export | 避免多个 target 覆盖同一文件。 |
| generated name conflict | export | 避免表名、字段名、enum 名生成后冲突。 |

## 诊断设计

Validator 诊断应可定位、可排序、可批量处理。

建议字段：

- `severity`：error/warning/info；
- `code`：稳定机器可读错误码；
- `message`：人类可读说明；
- `table`、`field_path`、`record_key`；
- `source_location`：文件/sheet/row/column/cell；
- `related_locations`：重复值、外键目标、覆盖来源等相关位置；
- `hint`：可选修复建议；
- `validator_id`：触发该诊断的 Validator。

排序建议：

1. severity；
2. table name；
3. source path；
4. row/column；
5. field path；
6. diagnostic code。

## 未来扩展

- 支持 Validator 插件，通过 registry 注册自定义 `kind`。
- 支持表达式型 Validator，例如 `expr = "min_level <= max_level"`。
- 支持 warning budget 或 CI 阈值。
- 支持只验证受影响 source/table 的增量校验。
- 支持按 target 选择 export validators。
- 支持把诊断输出为 JSON，供 IDE、CI、表格批注工具消费。
