# ObjectTable Step 1 详细设计

本文档描述 ObjectTable 的第一阶段实现方案。目标是先支持最常见的“常量配置表”场景：CSV 使用 `key,value` 行式布局，每一行提供一个字段值，最终导出为单个 object。

本阶段只做设计，不在本文档中假设代码已经完成。

## 1. 目标

ObjectTable Step 1 支持：

1. `kind = "object"` 的 table 使用 CSV `key,value` 行式 source。
2. CSV 第一行为 header；数据行第一列是 top-level field key，第二列是 raw value。
3. 一个 ObjectTable source 归一化为一个逻辑 object row。
4. validation 复用现有 required/default/type/enum/struct JSON cell 规则。
5. runtime data export 输出单个 object，而不是 collection records array。
6. codegen 能把 ObjectTable 表达为目标语言中的单个配置对象或 singleton table 数据。

示例 CSV：

```csv
key,value
max_level,100
start_gold,500
economy,"{""gold_rate"":""1.2"",""drop_rate"":""0.8""}"
start_item,"{""id"":""1"",""count"":""10""}"
```

示例 Meta：

```toml
[[tables]]
name = "game_config"
kind = "object"
source = "game_config.csv"

[[tables.fields]]
name = "max_level"
type = "int"
required = true

[[tables.fields]]
name = "start_gold"
type = "int"
default = 0

[[tables.fields]]
name = "economy"
type = "EconomyConfig"

[[tables.fields]]
name = "start_item"
type = "RewardItem"
```

目标 runtime JSON。注意：Step 1 不改变当前 runtime scalar value contract，scalar 仍保持 string；typed decode 由运行时代码或生成 loader 完成。

```json
{
  "max_level": "100",
  "start_gold": "500",
  "economy": { "gold_rate": "1.2", "drop_rate": "0.8" },
  "start_item": { "id": "1", "count": "10" }
}
```

## 2. 非目标

Step 1 不实现：

- 多 source ObjectTable 合并。
- `override_sources`。
- ObjectTable foreign key source 或 target。
- ObjectTable `row_export`。
- ObjectTable fragments / variant source mode。
- ObjectTable index / primary index。
- 后续格子自动 expand 为 array 元素。
- `economy.gold_rate` 这类 field path 展开。
- 独立子表式 array/detail table。
- array/map schema 的完整类型系统。
- runtime scalar typed export；如果未来要把 scalar 从 string 改为 typed int/float/bool，应作为全局 runtime export contract change，同时影响 CollectionTable 和 ObjectTable。

复杂对象在 Step 1 中优先使用 named struct JSON cell。数组、map 和 struct field path rows 可以作为后续 Step 2 设计；struct source layout 自动判定与 expanded value 的后续实现方案见 `08-struct-expanded-value-detail-design.md`。

## 3. Source Layout

### 3.1 默认布局

Step 1 对 `kind = "object"` + CSV source 使用固定布局：

```csv
key,value
```

规则：

- header 必须恰好两列。
- 第一列名 trim 后必须是 `key`，第二列名 trim 后必须是 `value`；比较大小写敏感，错误信息保留原始列名。
- 数据行中 `key` 不能为空。
- 数据行中 `key` 必须匹配 ObjectTable 的 top-level field name。
- 数据行中 `key` 在同一个 source 内不能重复。
- `value` 可以为空；空值后续由 required/default/type validation 判断。
- 除 `key` / `value` 外的列必须报错，避免用户误以为后续格子 expand 已生效。

### 3.2 为什么不立即加入布局配置

可选设计是增加：

```toml
[tables.object_layout]
kind = "key_value_rows"
key_column = "key"
value_column = "value"
```

Step 1 暂不建议引入该配置，原因：

- 当前只有一种 ObjectTable source layout，配置会增加 schema 面积。
- 固定 `key,value` 能让 CSV 规范更简单，也便于测试和错误提示。
- 如果未来确实需要自定义列名或多 value columns，可在 Step 2 以向后兼容方式增加 `object_layout`。

## 4. Data Model 归一化

当前 `DataTable` 是 row-oriented：`Vec<DataRow>`。Step 1 可继续复用该内部模型，但 ObjectTable source loader 应将 `key,value` rows 转换为一个 logical row：

```text
CSV rows:
  key=max_level, value=100
  key=start_gold, value=500

DataTable rows:
  [ { "max_level": "100", "start_gold": "500" } ]
```

如果 CSV 只有 header、没有任何数据行，也必须产出一个 empty logical row：

```text
DataTable rows:
  [ {} ]
```

这不是为了表达“有一条真实 record”，而是为了让 required/default/type validation 在 ObjectTable 的 singleton object 上执行。没有 required 字段时 empty object 可以导出为 `{}`；存在 required 字段且没有 default 时必须失败。

这样做的好处：

- required/default/type/enum validation 可以最大程度复用现有 per-row 逻辑。
- runtime export 可以在 table kind 分支上选择 object 输出形态。
- codegen IR 仍能从同一 `LinkedTableMeta` + `DataTable` 输入构建。

source row number 诊断要求：

- loader 级错误必须包含 CSV row number，例如 empty key、duplicate key、unknown key。
- type/enum/required 等 validation 级错误 Step 1 至少应包含 table、field、source path；如果实现成本可控，推荐增加 ObjectTable 专用 field source map，把 field key 映射到原始 CSV row number。
- field source map 可作为 ObjectTable 专用 normalized source info，避免污染 CollectionTable 的 row-level source model。

## 5. Validation 规则

### 5.1 Meta validation

ObjectTable Step 1 应在 Meta validation 阶段限制：

- ObjectTable 不能声明 indexes / primary_index。
- ObjectTable 不能声明 `row_export`。
- ObjectTable 不能声明 fragments。
- ObjectTable 不能声明 table-level `foreign_keys`。
- ObjectTable field 不能声明 field-level `foreign_key`。
- 任何其他 table 的 FK target 不能指向 ObjectTable；Meta validation/link 阶段需要扫描所有 table 的 `resolved_foreign_keys()` 并在 target table kind 为 Object 时直接报错。
- ObjectTable Step 1 只支持 legacy/simple `source = "...csv"`；不能声明 `sources` 或 `override_sources`。

如果这些能力未来实现，应在对应阶段放开。

### 5.2 Data validation

ObjectTable 归一化为单 logical row 后：

- required/default/type/enum 使用现有字段规则。
- struct `json_cell` 使用现有 JSON cell 校验。
- field validators 默认可以用于 ObjectTable 字段，只要 validator scope 是 field。
- collection/index/unique/FK/row_export/fragment validation 跳过 ObjectTable。
- table-level `export/export_when` 生效；不匹配当前 target 时不生成该 artifact。
- field-level `export/export_when` 生效；不可见字段从 object 中移除。
- table 可见但所有字段被过滤时导出 `{}`。

### 5.3 Unknown keys

Step 1 建议对 CSV 中未知 key 报错，而不是忽略。

原因：

- ObjectTable 常用于全局配置，拼写错误通常应该阻断导出。
- 未知 key 如果被忽略，会造成配置作者误以为某项设置生效。

错误示例：

```text
table `game_config` object source row 4 references unknown field key `max_lv`
```

Step 1 中错误文案可以先使用 `field key`；当后续支持 field path rows 后，再升级为 `field path`。

## 6. Runtime Export

CollectionTable 当前导出 rows array。ObjectTable Step 1 应导出单个 object。

建议 JSON shape：

```json
{
  "name": "game_config",
  "rows": [ ... ]
}
```

不适合 ObjectTable，因为这会把常量表伪装成 collection。

建议 runtime artifact 中每个 ObjectTable payload 为：

```json
{
  "max_level": "100",
  "start_gold": "500"
}
```

如果当前 exporter envelope 必须统一为 table artifact，可以让 artifact 文件仍叫 `game_config.json`，但文件内容是 object，而不是 `{ rows: [...] }` 或 records array。

需要同步调整 MessagePack exporter，使 JSON 与 MessagePack 的逻辑 shape 一致。

实现上不能只修改 `build_exported_table_payload` 的内部内容，因为当前 payload 类型是 collection-oriented。建议引入共享 payload 类型，确保 JSON 与 MessagePack 使用同一 builder：

```rust
#[derive(Serialize)]
#[serde(untagged)]
pub enum ExportedTablePayload {
    Collection(ExportedCollectionTable),
    Object(BTreeMap<String, ExportedValue>),
}
```

也可以拆分为 `build_exported_collection_payload` 与 `build_exported_object_payload`，再由 `serialize_table` 根据 `TableKind` 分派。无论采用哪种形式，都应保持 ObjectTable 与 CollectionTable 的 scalar value contract 一致：Step 1 仍使用 `ExportedValue::Scalar(String)`。

## 7. Codegen IR 与生成器

Step 1 的 codegen 可以分两层推进：

1. IR 层区分 table kind：`CodeTable.kind = Object | Collection`。
2. 生成器对 ObjectTable 生成单对象访问入口。

建议生成语义：

- Rust：`GameData::game_config() -> &GameConfig` 或 public field `game_config: GameConfig`。
- TypeScript：`data.gameConfig` 或 `getGameConfig()`。
- C++：`GetGameConfig()` 返回 const reference。

如果短期不扩展生成器 API，也必须确保 ObjectTable 不被错误生成为 collection repository。

## 8. 实现步骤建议

### Step 1.1 Meta 限制

- 在 `meta/validate.rs` 中补充 ObjectTable Step 1 unsupported feature checks。
- 增加测试：ObjectTable 声明 index、primary_index、row_export、table-level foreign_keys、field-level foreign_key、fragments、sources、override_sources 均报错。
- 增加测试：CollectionTable FK target 指向 ObjectTable 时直接报错。

### Step 1.2 DataLoadPlan / CSV object loader

- DataLoadPlan 根据 `TableKind::Object` 标记 source load mode。
- CSV reader 增加 object key/value 读取路径，或在 DataLoader 中将普通 CSV rows 转换为 single logical row。
- 校验 header 必须严格为两列 `key,value`。
- 校验 duplicate key / empty key / unknown key。
- only header 必须产出 one empty logical row。

### Step 1.3 Validation 复用与跳过规则

- required/default/type/enum/field validator 复用现有逻辑。
- index / unique / FK / row_export / fragment validation 对 ObjectTable 跳过或在 Meta 阶段禁止。
- single empty logical row 下 required/default 必须正常执行。
- 增加 ObjectTable required/default/type/unknown key 测试。

### Step 1.4 Runtime export shape

- 增加 payload enum 或 collection/object builder 分派，根据 `TableKind::Object` 输出单 object。
- JSON / MessagePack 测试确认 ObjectTable artifact shape。
- only-header ObjectTable：无 required 字段时允许输出 `{}`；有 default 字段时输出 default；有 required 且无 default 字段时失败。

### Step 1.5 Codegen

- CodeModel 加 table kind。
- 生成器避免为 ObjectTable 生成 collection index repository。
- 增加 Rust/TypeScript/C++ smoke tests。

## 9. 测试计划

Meta tests：

- parse ObjectTable with fields。
- reject ObjectTable indexes / primary_index。
- reject ObjectTable row_export。
- reject ObjectTable fragments。
- reject ObjectTable foreign keys。
- reject ObjectTable field-level foreign keys。
- reject CollectionTable FK target to ObjectTable。
- reject ObjectTable `sources` and `override_sources` in Step 1。

Data tests：

- read `key,value` CSV into one logical row。
- reject missing `key` / `value` header。
- reject extra columns。
- reject empty key。
- reject duplicate key。
- reject unknown key。
- only header produces one empty logical row。

Validation tests：

- required field passes when key exists。
- required field fails when key missing。
- default fills missing field。
- only header + required field fails。
- only header + default field fills default。
- only header + no required/default exports empty object。
- primitive / enum type validation works。
- struct `json_cell` works。

Export tests：

- JSON exports object shape。
- MessagePack exports same logical shape。
- target field filtering removes hidden object fields。
- table export_when mismatch skips ObjectTable artifact。
- all fields filtered exports `{}` when table itself is visible。

Codegen tests：

- ObjectTable appears in CodeModel with object kind。
- no index/repository helpers are generated for ObjectTable。
- generated loader exposes singleton object access.

CLI integration tests：

- Add an object table fixture or extend the standard fixture with `game_config.csv`。
- CLI build target emits `game_config.json` with object shape。
- JSON and MessagePack targets preserve the same logical shape。
- target field filtering works end-to-end。

## 10. Open Questions

- 是否允许自定义 key/value column name？Step 1 暂不支持。
- 是否允许 comments rows，例如 key 为空或以 `#` 开头？Step 1 建议不支持，避免和空 key 错误冲突。
- field path rows 是否在 Step 1 支持 nested struct expand？Step 1 先只允许 top-level field name；后续由 source layout 自动判定决定 `economy` 使用 JSON cell 还是 `economy.gold_rate` 等 child paths，具体实现方案见 `08-struct-expanded-value-detail-design.md`。
- ObjectTable source row number 是否需要 field-level source map？建议作为诊断增强后续实现。
- ObjectTable codegen API 是 public field 还是 accessor method？需要结合三种语言生成风格统一决定。
