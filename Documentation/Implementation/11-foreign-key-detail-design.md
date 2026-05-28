# 11. Foreign Key 专项实现详细设计

本文档描述 ConfigForge 外键（Foreign Key / FK）能力的实现方案。产品语义以 `Documentation/Design/04-meta-model.md` 与 `Documentation/Design/05-data-validation.md` 为准；本文档聚焦 Rust 类型、模块边界、Link/Validation/Codegen 接入点、阶段拆分和测试计划。

## 11.1 背景与目标

当前项目已经具备：

- table / field / enum / struct / index 的 raw meta 与 linked meta；
- target-aware `ExportValidationView`；
- required / default / type / enum / unique 基础校验；
- 注册式 validator 框架与 `ValidationReport`；
- runtime data export 与 Rust / TypeScript / C++ codegen。

外键能力用于表达“当前表字段引用另一张表的唯一 key”。典型示例：

```toml
[[tables]]
name = "monsters"
source = "monsters.csv"

[[tables.fields]]
name = "id"
type = "int"

[[tables.fields]]
name = "drop_item_id"
type = "int"

[[tables.foreign_keys]]
name = "drop_item"
fields = ["drop_item_id"]
target_table = "items"
target_index = "by_id"
allow_empty = true
```

实现目标：

1. 在 Meta 中把 foreign key 建模为一等关系数据，而不是普通 `ValidatorMeta`。
2. 在 Link 阶段解析 source fields、target table、target index，并复用现有 field/index 类型信息。
3. 在 Validation 阶段自动执行 referential integrity 校验。
4. 与 target tags、`export_when`、`row_export` 和 ExportView 语义保持一致。
5. 为后续 codegen 生成关联查询 helper 预留 IR 和类型结构。

## 11.2 非目标与阶段边界

Phase 1 只实现外键关系的 Meta / Link / Validation 闭环，暂不一次性实现所有关系型能力。

Phase 1 支持：

- 单字段 FK；
- 复合 FK；
- 指向 target table 的 primary index；
- 指向 target table 的 explicit unique index；
- `allow_empty`；
- `export` / `export_when`；
- target-aware validation；
- source / target table 均为 collection table 的常规关系校验。

Phase 1 不支持：

- fragment field 作为 FK source field 或 target index field；
- struct child path，例如 `reward.item_id`；
- array / map 内元素 FK；
- 指向非 unique index 的一对多关系；
- cross-target full-dataset FK 模式；
- ObjectTable 作为 FK source table 或 target table；
- 自动 codegen helper；
- reverse relation / back reference；
- cascade、override、join 等数据库式关系操作。

## 11.3 配置模型

### 11.3.1 字段级单键语法糖

单字段外键是最常见场景，Phase 1 应直接支持字段级语法糖，避免用户为了一个 `item_id -> items.by_id` 关系额外编写 table-level `[[tables.foreign_keys]]`。

推荐写法：

```toml
[[tables.fields]]
name = "item_id"
type = "int"
required = true
foreign_key = { target_table = "items", target_index = "by_id" }
```

完整写法：

```toml
[[tables.fields]]
name = "drop_item_id"
type = "int"
required = false
foreign_key = {
  name = "drop_item",
  desc = "引用掉落物品。",
  target_table = "items",
  target_index = "by_id",
  allow_empty = true,
  export = true,
  export_when = { any = ["client"] }
}
```

字段级语法糖规则：

- 只表达**单 source field** 外键，source field 固定为当前字段。
- `target_index` 可省略；省略时引用 target table primary index。
- 目标 index 必须是一字段 unique / primary index；如果目标 index 是复合 index，应报 Meta validation error，并提示改用 table-level `[[tables.foreign_keys]]`。
- `name` 可省略；省略时使用当前字段名作为 FK name，例如字段 `item_id` 生成关系名 `item_id`。如果该名称与 table-level FK 或其他字段级 FK 冲突，应报重复 FK name。
- `allow_empty` 可省略；省略时按当前字段 `required` 派生：`required = true` 等价 `allow_empty = false`，`required = false` 等价 `allow_empty = true`。
- `desc`、`export`、`export_when` 与 table-level FK 语义一致。

字段级语法糖与 table-level FK 最终进入同一套 linked/validation/codegen 逻辑。实现上推荐在 helper 方法或 link/validate 阶段把字段级声明展开为规范化 FK 视图，而不是在多个模块分别处理两套逻辑。

### 11.3.2 Table-level 外键配置

复合外键或需要显式列出多个 source fields 时，使用 table-level 配置：

```toml
[[tables.foreign_keys]]
name = "item"
desc = "引用物品表。"
fields = ["item_id"]
target_table = "items"
target_index = "by_id"
allow_empty = true
export = true
export_when = { any = ["client"] }
```

复合外键：

```toml
[[tables.foreign_keys]]
name = "item_in_region"
fields = ["item_id", "region"]
target_table = "items"
target_index = "by_id_region"
```

table-level 配置也可用于单字段 FK，但不推荐作为常规写法。推荐约定：

- 单字段 FK：优先写在 `[[tables.fields]]` 下，配置更贴近字段定义。
- 多字段 FK：使用 `[[tables.foreign_keys]]`，因为它天然属于整张表的关系约束。

如果 `target_index` 省略，则默认引用 target table 的 primary index：

```toml
[[tables.foreign_keys]]
name = "item"
fields = ["item_id"]
target_table = "items"
```

## 11.4 Meta Raw Model

在 `src/meta/model/raw.rs` 增加，并经 `src/meta/model.rs` re-export：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FieldForeignKeyMeta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub export_when: ExportCondition,
    pub target_table: String,
    #[serde(default)]
    pub target_index: Option<String>,
    #[serde(default)]
    pub allow_empty: Option<bool>,
}
```

在 `FieldMeta` 中增加：

```rust
#[serde(default)]
pub foreign_key: Option<FieldForeignKeyMeta>,
```

table-level 规范外键配置：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ForeignKeyMeta {
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default = "default_export")]
    pub export: bool,
    #[serde(default)]
    pub export_when: ExportCondition,
    #[serde(default)]
    pub fields: Vec<String>,
    pub target_table: String,
    #[serde(default)]
    pub target_index: Option<String>,
    #[serde(default)]
    pub allow_empty: Option<bool>,
}
```

在 `TableMeta` 中增加：

```rust
#[serde(default)]
pub foreign_keys: Vec<ForeignKeyMeta>,
```

字段级声明不直接存入 `TableMeta.foreign_keys`，以便保留用户原始配置位置。需要遍历“表级 FK + 字段级 FK 展开结果”的模块应通过统一 iterator/helper 获取。

建议新增内部规范化视图：

```rust
pub struct ResolvedForeignKeyMeta<'meta> {
    pub name: &'meta str,
    pub desc: Option<&'meta str>,
    pub export: bool,
    pub export_when: &'meta ExportCondition,
    pub fields: Vec<&'meta str>,
    pub target_table: &'meta str,
    pub target_index: Option<&'meta str>,
    pub allow_empty: bool,
    pub declared_at: ForeignKeyDeclarationSite<'meta>,
}

pub enum ForeignKeyDeclarationSite<'meta> {
    Table(&'meta ForeignKeyMeta),
    Field(&'meta FieldMeta, &'meta FieldForeignKeyMeta),
}
```

该视图可以只作为 link/validate 内部 helper，不一定进入 public API。核心目标是让后续逻辑处理统一的 `fields / target_table / target_index / allow_empty`，并在诊断中保留声明来源。

`name` 必须借用 raw meta 中已有字符串：table-level FK 借用 `ForeignKeyMeta.name`，field-level 显式 `name` 借用 `FieldForeignKeyMeta.name`，field-level 默认 `name` 借用 `FieldMeta.name`。Phase 1 不为默认 name 构造 owned string，以便 `foreign_key_index: HashMap<&'meta str, usize>` 与现有 table/index lookup 生命周期策略一致。

字段语义：

| 字段 | 说明 |
| --- | --- |
| `name` | table scope 内唯一的关系名，用于诊断、文档和未来 codegen helper 命名。 |
| `desc` | 关系说明，未来可用于文档 / inspect / 代码注释。 |
| `export` | 是否进入 runtime-facing relation model；默认 `true`。Phase 1 validation 也按该字段过滤 target relation。 |
| `export_when` | target tags 条件；不匹配时当前 target 不校验该 FK。 |
| `fields` | source table 中参与 FK key 的 common field 名称列表。 |
| `target_table` | 被引用 table 名称。 |
| `target_index` | 被引用 index 名称；为空时使用 target table primary index。 |
| `allow_empty` | source key 全空时是否跳过 referential integrity 校验。 |

字段级 `FieldForeignKeyMeta.allow_empty` 是 `Option<bool>`：

- `Some(value)`：使用显式配置；
- `None`：按当前字段 `required` 派生，即 `!field.required`。

table-level `ForeignKeyMeta.allow_empty` 也是 `Option<bool>`：

- `Some(value)`：使用显式配置；
- `None`：按 source fields 的 `required` 派生；所有 source fields 都是 `required = false` 时默认允许全空，否则默认不允许全空；
- 复合 FK 部分为空始终报 partial key error，不受 `allow_empty` 影响。

`ForeignKeyMeta` 增加方法：

```rust
impl ForeignKeyMeta {
    pub fn is_exported_for(&self, target: &TargetTags) -> bool {
        self.export && self.export_when.matches(target)
    }
}
```

## 11.5 Linked Model

### 11.5.1 类型定义

在 `src/meta/model/linked.rs` 增加，并经 `src/meta/model.rs` re-export：

```rust
pub struct LinkedForeignKeyMeta<'meta> {
    pub name: &'meta str,
    pub desc: Option<&'meta str>,
    pub export: bool,
    pub export_when: &'meta ExportCondition,
    pub allow_empty: bool,
    pub declared_at: ForeignKeyDeclarationSite<'meta>,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    pub target_table: &'meta LinkedTableMeta<'meta>,
    pub target_index: &'meta LinkedIndexMeta<'meta>,
}
```

但当前 `LinkedMetaModel<'meta>` 内部持有 `Vec<LinkedTableMeta<'meta>>`，直接让 `LinkedForeignKeyMeta` 引用 `LinkedTableMeta` 会形成 self-referential 结构风险。因此 Phase 1 推荐采用 index-based linked 表达：

```rust
pub struct LinkedForeignKeyMeta<'meta> {
    pub name: &'meta str,
    pub desc: Option<&'meta str>,
    pub export: bool,
    pub export_when: &'meta ExportCondition,
    pub allow_empty: bool,
    pub declared_at: ForeignKeyDeclarationSite<'meta>,
    pub fields: Vec<LinkedFieldMeta<'meta>>,
    pub target_table_name: &'meta str,
    pub target_index_name: &'meta str,
}
```

执行时通过 `LinkedMetaModel::table(target_table_name)` 和 `LinkedTableMeta::index(target_index_name)` 查询目标。这样避免 linked model 自引用，同时与当前 table/index 查询方式一致。

与规范化视图一致，`LinkedForeignKeyMeta.name` 使用 `&'meta str` 而不是 `Cow<'meta, str>`。字段级默认 name 必须直接借用 `FieldMeta.name`，不要生成临时 `String`，否则无法安全放入 `HashMap<&'meta str, usize>`。

`LinkedTableMeta<'meta>` 增加：

```rust
pub foreign_keys: Vec<LinkedForeignKeyMeta<'meta>>,
foreign_key_index: HashMap<&'meta str, usize>,
```

以及查询方法：

```rust
pub fn foreign_key(&self, name: &str) -> Option<&LinkedForeignKeyMeta<'meta>>;
```

### 11.5.2 Link 流程

在 `src/meta/link.rs` 中：

1. table fields 链接完成后，收集 table-level `table.foreign_keys` 与 field-level `field.foreign_key` 展开结果。
2. 对每个 FK：
   - source `fields` 通过当前 table 的 linked field lookup 解析；
   - 字段级 FK 的 source `fields` 固定为当前字段名；
   - 字段级 FK 的 `name` 为空时派生为当前字段名，并直接借用 `FieldMeta.name`；
   - 字段级 FK 的 `allow_empty` 为空时派生为 `!field.required`；
   - table-level FK 的 `allow_empty` 为空时，根据已解析 source fields 派生：所有 source fields 都是 optional 则为 `true`，否则为 `false`；
   - `target_table` 暂先保存名称，真正存在性可在 validate 中确认，也可在 link 中通过 `model.table()` fail-fast；
   - `target_index` 为空时解析为目标表 primary index 名称；
   - explicit `target_index` 保存原始名称。
3. 将 linked foreign keys 传入 `LinkedTableMeta::new(...)`。

是否在 Link 阶段直接失败：

- 当前项目 Link 对未知 field type / index field 等会返回 `Result`；
- 因此外键 target table/index/source field 也建议在 Link 阶段 fail-fast；
- Meta validate 仍保留面向用户的多诊断检查，便于一次性报告多个结构错误。

## 11.6 Meta Validation

在 `src/meta/validate.rs` 增加 `validate_foreign_keys()`。

检查规则：

1. FK name 非空。字段级 FK 未配置 `name` 时，派生 name 使用字段名。
2. FK names 在 table scope 内唯一，table-level 与 field-level FK 共同参与去重。
3. `fields` 非空。字段级 FK 展开后固定为单字段。
4. `fields` 内字段名不重复。
5. source fields 必须存在于当前 table common fields。
6. source fields 不允许使用 dotted fragment path；Phase 1 不支持 fragment field FK。
7. `target_table` 非空且存在。
8. source table 和 target table 都必须是 CollectionTable；ObjectTable 作为 source/target 一律报 unsupported。
9. `target_index`：
   - 为空时，target table 必须存在 primary index；若不存在，应提示“请在 target table 配置 primary index，或显式指定 target_index”；
   - 非空时，target table 必须存在该 index。
10. target index 必须 unique 或 primary。
11. source fields 数量必须等于 target index fields 数量。
12. source field type 与 target index field type 必须兼容。
13. `export_when` tag 合法。
14. 字段级 FK 展开后 target index fields 数量必须为 1；若目标是复合 primary/unique index，应提示“字段级 FK 只能引用单字段 index；请改用 table-level `[[tables.foreign_keys]]` 并列出多个 source fields”。

类型兼容规则 Phase 1：

| source / target | 兼容规则 |
| --- | --- |
| primitive | 必须是同一种 `PrimitiveFieldType`。 |
| enum | 必须引用同一个 enum 类型名。 |
| struct | 不支持作为 FK key。 |
| fragment field | 不支持。 |

错误分类建议：

- 缺字段 / 缺 target table / 缺 target index：`MissingFieldReference` 或新增更具体 code；
- 非 unique target：`InvalidIndexDeclaration` 或新增 `InvalidForeignKeyDeclaration`；
- 类型不兼容：新增 `InvalidForeignKeyDeclaration` 更清晰。

建议新增 `MetaDiagnosticCode::InvalidForeignKeyDeclaration`，避免把所有外键错误混入 index / field 诊断。

## 11.7 Validation 执行设计

外键不是用户显式 validator，但 Validation 阶段可把 linked FK 当作 implicit dataset validator 执行。

### 11.7.1 接入位置

当前已在 `src/validation/foreign_key.rs` 中实现 hardcoded FK 校验，并由 `src/validation/entry.rs` 的 full / target-aware validation 入口调用；这种方式与当前 required/type/unique 基础规则保持一致：

```rust
pub fn validate_normalized_data_set_with_report(...) -> Result<ValidationReport> {
    ...
    validate_foreign_keys(meta, &context)?;
    ...
}
```

或在 `validate_normalized_data_set*` 内，在基础 table validation 后、registry validators 前执行。

后续迁移路径：

- 新增 `src/validation/builtins/referential_integrity.rs`；
- 将 FK 关系编译为 implicit `CompiledValidator`；
- 统一进入 `CompiledValidationPlan`。

Phase 1 为降低改动范围，可以先 hardcoded，文档和命名上保留“implicit validator”的概念。

Phase 1 的 hardcoded FK 校验暂不进入 `ValidationReport` 聚合：发现第一个 FK 错误时直接返回 `ConfigForgeError::Validation(String)`，与当前 required/type/unique 基础校验保持一致。后续迁移为 implicit `CompiledValidator` 后，再产出结构化 `ValidationIssue` 并进入 `validate_*_with_report` 的聚合结果。

### 11.7.2 Full validation 与 target validation

Full validation：

- 使用全量 `DataSet`；
- source table 所有 rows 参与；
- target table 所有 rows 参与；
- FK `export` / `export_when` 不过滤，除非未来定义 full validation 也按 target 执行。

Target validation：

- 使用 `ExportValidationView::for_target(...)`；
- FK 本身 `is_exported_for(target_tags)` 为 false 时跳过；
- source table 当前 target 不可见时，该 table 不在 view 中，因此无需执行其 FK；
- FK 自身可见但 target table 当前 target 不可见时，返回 validation error；
- FK 自身可见但 target index 当前 target 不可见时，返回 validation error；
- FK 自身可见但 source fields 或 target index fields 当前 target 不可见时，返回 validation error；
- source table 只有 visible rows 参与；
- target key set 只从 target table 当前 target visible rows 中构建，被 `row_export` 排除的 target row 不可被引用。

### 11.7.3 Key 构造

定义内部 key：

```rust
type ForeignKeyValue = Vec<String>;
```

构造规则：

1. 按 `fields` / `target_index.fields` 顺序取值。
2. 每个 cell 使用 normalize 后的 `DataTable` raw string cell 值。
3. `trim().is_empty()` 视为空。
4. 单字段 FK：
   - 全空：按 `allow_empty` 处理；
   - 非空：查 target key set。
5. 复合 FK：
   - 全空：`allow_empty = true` 时跳过，否则报 required-like FK error；
   - 部分为空：总是报 partial key error；
   - 全非空：查 target key set。

target key set：

```rust
let mut target_keys = HashSet<Vec<String>>;
```

target index 已保证 unique，因此如果目标数据本身重复，会由现有 unique index validation 先报错；FK 校验不需要重复诊断 target duplicate。

Phase 1 不做 primitive typed canonicalization，FK resolution 使用与 unique index 相同的 raw cell key 语义。例如 int 字段中 raw cell `"01"` 与 `"1"` 是两个不同 key，即使两者都能通过 int 类型校验。若未来引入 typed key canonicalization，必须同步升级 unique index、codegen index 与 FK，否则同一 key 在不同模块会出现不一致语义。

### 11.7.4 诊断信息

缺引用错误：

```text
table `monsters` row 3 foreign key `drop_item` references missing `items.by_id` key `["1001"]`
```

部分空 key：

```text
table `shops` row 5 foreign key `item_in_region` has partial empty key `["1001", ""]`
```

不允许空 key：

```text
table `monsters` row 3 foreign key `drop_item` is empty but `allow_empty = false`
```

target-aware 字段不可见错误：

```text
table `monsters` foreign key `drop_item` references non-exported source field `drop_item_id` for current target
```

target table / index 不可见错误：

```text
table `monsters` foreign key `drop_item` references target table `items` which is not exported for current target
table `monsters` foreign key `drop_item` references target index `items.by_id` which is not exported for current target
```

字段级 FK 默认 target index 诊断：

```text
field `monsters.drop_item_id` foreign key omits `target_index`, but target table `items` has no primary index; configure a primary index or specify `target_index`
field `monsters.drop_item_id` foreign key references composite index `items.by_id_region`; field-level foreign key only supports single-field index, use [[tables.foreign_keys]] for composite keys
```

row number 优先使用 `RowSourceInfo.source_row_number`，与当前 fragment / ExportView 行号策略一致。

### 11.7.5 与现有 validation 顺序

推荐顺序：

1. normalize defaults；
2. table-level基础校验：required / type / enum / fragment / unique；
3. foreign key referential integrity；
4. registry validators。

理由：

- FK key 应使用 default 后的值；
- source/target key 类型必须先通过 type validation；
- target unique index 冲突应优先报告；
- 业务 validator 可以依赖外键已经通过的前提。

## 11.8 Codegen 与 IR 预留

Phase 1 不生成语言 helper，但建议在 CodeModel 中预留结构，便于后续演进。

```rust
pub struct CodeForeignKey {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub target_table: String,
    pub target_index: String,
    pub allow_empty: bool,
}
```

如果 Phase 1 想进一步降低范围，也可以暂不改 CodeModel，只保证 Meta / Validation 闭环。推荐策略：

- Phase 1a：只做 Meta + Validation，不动 codegen；
- Phase 1b：CodeModel 暴露 `CodeForeignKey`，但语言生成器不使用；其中 `allow_empty` 表示 helper 返回值是否天然 optional；
- Phase 2：生成关联查询 helper。

未来语言 helper 形态：

| 语言 | 可能生成 API |
| --- | --- |
| Rust | `pub fn drop_item<'a>(&self, row: &MonsterRow, data: &'a GameData) -> Option<&'a ItemsRow>` |
| TypeScript | `dropItem(row: MonstersRow, data: GameData): ItemsRow | undefined` |
| C++ | `const ItemsRow* GetDropItem(const MonstersRow& row, const GameData& data) const` |

具体 API 需要结合 repository ownership、lifetime 和 GameData 聚合结构另行设计。

## 11.9 与 Fragment 的关系

Phase 1 FK 只允许 common fields：

- source `fields` 不能写 `fragment.field`；
- target index 不能引用 fragment fields；
- fragment field 作为 FK key 的能力留到 `FieldPath` / nested validation scope 成熟后实现。

原因：

- fragment 字段只对部分 rows active，外键 cardinality 和 target visibility 更复杂；
- 当前 index 也限制为 common fields；
- 先保持 FK 与现有 unique/primary index 一致，减少语义分歧。

如果未来支持 fragment FK，需要补充：

- `FieldPath` parser；
- active fragment 判断；
- fragment target visibility 与 relation visibility；
- codegen 中 optional fragment relation helper 的返回策略。

## 11.10 实施步骤

建议拆成小提交：

### Step 1：Meta raw model

- 新增 `ForeignKeyMeta`。
- 新增 `FieldForeignKeyMeta`。
- `TableMeta` 增加 `foreign_keys`。
- `FieldMeta` 增加 `foreign_key`。
- public re-export 增加 `ForeignKeyMeta` / `FieldForeignKeyMeta`。
- TOML parse 测试覆盖字段级语法糖、table-level 单字段、复合、默认 `allow_empty`、默认 `target_index`。

### Step 2：Meta validation

- 新增 `validate_foreign_keys()`。
- 新增或复用 diagnostic code。
- 测试：重复 FK name、字段级派生 name 与 table-level name 冲突、空 fields、未知 source field、unknown target table/index、target index 非 unique、target table 无 primary index 但省略 `target_index`、字段数量不一致、字段级 FK 指向复合 index、类型不兼容、fragment path 被拒绝、ObjectTable 作为 source/target 被拒绝。

### Step 3：Link

- 新增 `LinkedForeignKeyMeta`。
- `LinkedTableMeta` 增加 `foreign_keys` 与 lookup。
- `link.rs` 解析 source fields / target names / target index name。
- 测试 linked lookup。

### Step 4：Validation

- 在 full dataset validation 中执行 FK。
- 在 target validation 中基于 `ExportValidationView` 执行 FK。
- 测试：有效引用、缺引用、allow_empty、部分空复合 key、target row 被 `row_export` 排除、FK `export_when` 跳过、FK 可见但 target table/index 不可见时报错。

### Step 5：文档与 fixture

- 更新 `Documentation/Design/04-meta-model.md` 当前状态。
- 更新 `Documentation/Design/05-data-validation.md` FK validation 语义。
- 在 standard project fixture 中增加一个小型 FK 示例。

## 11.11 测试计划

### Meta tests

1. parse 字段级 `foreign_key = { ... }`。
2. parse `[[tables.foreign_keys]]`。
3. `target_index` 为空时允许配置进入 raw meta。
4. 字段级 FK 未配置 `name` 时派生字段名。
5. 字段级 FK 未配置 `allow_empty` 时按 `required` 派生。
6. table-level FK 未配置 `allow_empty` 时按 source fields `required` 派生。
7. 重复 FK name 报错，包括字段级派生 name 与 table-level name 冲突。
8. source field 不存在报错。
9. source field 写 `stats.item_id` 报错。
10. source/target table 为 ObjectTable 报 unsupported。
11. target table 不存在报错。
12. 省略 `target_index` 但 target table 无 primary index 时报错。
13. target index 不存在报错。
14. target index 非 unique 报错。
15. 字段级 FK 指向复合 target index 报错，并提示改用 table-level FK。
16. source/target field 数量不一致报错。
17. source/target field 类型不兼容报错。

### Validation tests

1. 单字段 FK 引用存在，通过。
2. 单字段 FK 引用缺失，失败。
3. 字段级 FK 引用存在，通过。
4. 字段级 FK 对 `required = false` 字段默认允许空值。
5. 字段级 FK 对 `required = true` 字段默认不允许空值。
6. table-level 单字段 FK 对 optional source field 默认允许空值。
7. table-level 复合 FK 全部 source fields optional 时默认允许全空。
8. table-level 复合 FK 任一 source field required 时默认不允许全空。
9. `allow_empty = true` 且 key 为空，通过。
10. `allow_empty = false` 且 key 为空，失败。
11. 复合 FK 全非空且存在，通过。
12. 复合 FK 部分为空，失败。
13. 复合 FK 缺引用，失败。
14. raw string key 语义：`"01"` 不匹配 `"1"`。
15. target validation 中 source row 被 `row_export` 排除，不校验该 row。
16. target validation 中 target row 被 `row_export` 排除，source 引用该 row 失败。
17. FK `export_when` 不匹配当前 target，跳过。
18. FK 可见但 target table `export_when` 不匹配，报 validation error。
19. FK 可见但 target index `export_when` 不匹配，报 validation error。
20. FK `export_when` 不匹配时，即使 target table/index 不可见也跳过。

### Integration tests

1. CLI export 前发现 FK 缺引用并失败。
2. 修复数据后 JSON / MessagePack export 成功。
3. target tags 下 FK 与 row_export 行为一致。

## 11.12 风险与开放问题

1. **Full validation 是否受 `export` / `export_when` 影响**  
   建议 full validation 检查全部 FK；target validation 才按 target 过滤。若 CLI 的默认 validate 已经绑定 target，则以 target 语义为准。

2. **ObjectTable 外键语义**  
   Phase 1 明确禁止 ObjectTable 参与 FK：source table 与 target table 都必须是 CollectionTable。等 ObjectTable 的 index / singleton access / runtime payload 语义稳定后，再单独设计 ObjectTable FK。

3. **Codegen helper 的 ownership / lifetime**  
   不在 Phase 1 解决，避免阻塞 validation 能力落地。

4. **诊断结构化程度**  
   当前很多校验仍桥接到 `ConfigForgeError::Validation(String)`。Phase 1 可保持一致；后续可把 FK 错误迁移到结构化 `ValidationIssueLocation`。

5. **target visibility 下字段不可见时的行为**  
   推荐严格报错：一个当前 target 可见的 FK，不应引用当前 target 不可见的 target table、target index、source field 或 target index field。否则 codegen / runtime relation helper 会缺少必要目标或字段。

## 11.13 完成标准

外键 Phase 1 完成时应满足：

- `ForeignKeyMeta` 可从 TOML 加载。
- `FieldForeignKeyMeta` 字段级语法糖可从 TOML 加载，并能规范化为单字段 FK。
- Meta validation 能稳定报告所有结构错误。
- Link 后可从 `LinkedTableMeta` 查询外键关系。
- Full validation 能发现缺引用、空 key、partial composite key。
- Target validation 与 ExportView / row_export / export_when 一致。
- 不支持 fragment/nested field FK 时有明确错误。
- `cargo fmt`、`cargo test`、`cargo clippy --all-targets --all-features` 通过。
