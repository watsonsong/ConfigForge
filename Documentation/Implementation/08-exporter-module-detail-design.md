# 08. Exporter 模块详细设计

本文档描述 `src/exporter/` 的 runtime data export 实现边界。Exporter 模块负责把已经加载、归一化并通过 validation 的数据导出为 runtime data artifacts；它与 Codegen 平行，不再承载工程构建主流程。

## 8.1 设计目标

Exporter 模块需要达成：

1. 定义 target data task 配置 `TargetDataConfig`。
2. 定义 runtime data format exporter trait 和 registry。
3. 内置 JSON 与 MessagePack 两种 data format，并为 `bin` 紧密二进制格式预留实现入口。
4. 对 CollectionTable 与 ObjectTable 生成正确 runtime payload shape。
5. 复用 DataResolver 解释 struct JSON cell / expanded value / target visibility，避免 exporter 自己重复实现 source layout 解析。
6. 为未来二进制格式、自定义格式 exporter 和 artifact writer 留出扩展点。

非目标：

- 不执行数据加载计划；DataLoader 属于 `data` 模块，由 `build` 编排调用。
- 不执行 validation；validation 属于 `validation` 模块，由 `build` 在导出前调用。
- 不生成代码；codegen 只消费 exporter 返回的 `RuntimeDataFile` 描述。

## 8.2 模块结构

当前实现按职责拆分为：

```text
src/exporter/
  mod.rs              # 对外 API 与 re-export
  config.rs           # TargetDataConfig
  model.rs            # ExportedTable / ExportedTablePayload / ExportedValue
  registry.rs         # DataFormatRegistry / DataFormatExporter / builtin_exporters
  runtime.rs          # runtime artifact plan、并行导出编排、artifact 写入入口
  payload.rs          # CollectionTable / ObjectTable 共享 payload builder
  json.rs             # JsonDataExporter
  messagepack.rs      # MessagePackDataExporter
  binary.rs           # BinaryDataExporter（planned）
  tests.rs            # exporter 核心行为测试
```

`mod.rs` 仅保留模块声明和公共 re-export，确保既有消费者仍可通过 `crate::exporter::*`
访问稳定 API；新增格式应优先落在独立格式模块中，核心 registry / runtime / payload 不内联具体格式逻辑。

## 8.3 配置与 registry

```rust
pub struct TargetDataConfig {
    pub name: Option<String>,
    pub input: PathBuf,
    pub output: PathBuf,
    pub format: String,
}
```

语义：

- `name`：可选 data task 名称。多个 data task 时必须显式命名；单个未命名 task 由 build 归一化为 `default`。
- `input`：数据根目录。
- `output`：runtime data 输出目录。
- `format`：格式名称，默认 `json`；当前内置支持 `json`、`messagepack`，并接受 `msgpack` alias；planned 支持 `bin` 紧密二进制格式，详见 `13-runtime-binary-format-detail-design.md`。

```rust
pub trait DataFormatExporter: Send + Sync {
    fn name(&self) -> &'static str;
    fn aliases(&self) -> &'static [&'static str];
    fn extension(&self) -> &'static str;
    fn plan_table_artifact(&self, table_meta: &LinkedTableMeta<'_>) -> Result<RuntimeArtifactPlan>;
    fn serialize_table(...) -> Result<Vec<u8>>;
}
```

`DataFormatRegistry` 保存 exporter 列表并按 name / alias 查询。Exporter implementation 必须是 thread-safe 和 reentrant，因为 `build` 会在 CPU worker pool 中并行序列化多个 table artifact。

内置格式通过 `builtin_exporters()` 返回 `Vec<Box<dyn DataFormatExporter>>` 并注册到 `DataFormatRegistry`。新增导出格式应优先新增 `src/exporter/<format>.rs` 并实现 `DataFormatExporter`，格式名、alias、扩展名和序列化逻辑由格式模块自己维护；核心模块只负责 registry、调度和共享 payload，不应内联具体格式序列化实现。`bin` 属于私有 positional wire layout，不应直接 serde 序列化 `ExportedTablePayload`，而应与 JSON / MessagePack 共享 target-aware resolved export view 后分别渲染为 named 或 positional layout。

## 8.4 Runtime payload contract

当前共享 payload enum：

```rust
#[serde(untagged)]
pub enum ExportedTablePayload {
    Collection(ExportedTable),
    Object(BTreeMap<String, ExportedValue>),
}
```

### 8.4.1 CollectionTable

CollectionTable 输出包含 table meta envelope 和 rows：

```json
{
  "meta": { "name": "items" },
  "rows": [
    { "id": "1001", "name": "Potion" }
  ]
}
```

导出规则：

- 按 target tags 过滤 table / field。
- 按 `row_export` 过滤 rows。
- variant source row 只导出该 source 激活的 fragments。
- struct value 输出为 nested object。

### 8.4.2 ObjectTable

ObjectTable Step 1 已并入 Exporter 模块语义：ObjectTable 不输出 rows array，而输出单个 object。

```json
{
  "max_level": "100",
  "start_gold": "500",
  "economy": { "gold_rate": "1.2", "drop_rate": "0.8" }
}
```

导出规则：

- ObjectTable 必须已经归一化为一个 logical row。
- table 可见但所有字段被 target 过滤时输出 `{}`。
- JSON 与 MessagePack 必须通过同一 payload builder，确保逻辑 shape 一致；planned `bin` 必须复用同一份 resolved export view，但 wire layout 按 schema 顺序紧密排列，不保存字段名。
- JSON / MessagePack 的 scalar runtime contract 当前仍为 string-compatible；如果未来改为 typed scalar，必须作为 named payload 的全局 runtime data contract change 同时影响 CollectionTable。planned `bin` 是 schema-aware typed binary encoding，按 Meta type 写 int/float/bool/enum bytes，不改变 JSON / MessagePack 的 named payload contract。

ObjectTable source layout、校验限制和 codegen 表达分别记录在 Data / Validation / Codegen 模块文档，不再保留单独的 Step 1 过程文档。

## 8.5 DataResolver 使用原则

Exporter 不应直接解释 raw `DataRow` 中的 JSON 字符串或 dotted path。当前实现通过：

```text
DataResolver::resolve_table_rows / resolve_table
  -> ResolvedValue
  -> ExportedValue
```

这样可保证：

- validation 和 exporter 对 struct JSON cell / expanded value 的理解逐步收敛。
- default、target-hidden child、empty struct occurrence 等语义不分叉。
- 后续 array/map 支持只需要扩展 `DataValue -> ExportedValue` 转换。

## 8.6 Artifact path 约束

Exporter 负责在单个 data task 内检查 runtime artifact path 冲突：

- `plan_table_artifact()` 给出每张 table 的相对输出路径。
- `ensure_unique_runtime_artifact_paths()` 拒绝同一个 exporter 计划出的重复路径。
- 跨 data task 的输出路径冲突由 build 在目标级别提前检查，因为多个 data task 可能写入不同或相同 output root。

## 8.7 测试边界

Exporter 模块测试应覆盖：

- registry 能解析内置 exporter 和 alias。
- 自定义 exporter plugin 可以注册和执行。
- runtime data artifact path 冲突在写文件前失败。
- CollectionTable 输出 rows envelope。
- ObjectTable 输出单 object shape，包括 only-header、default、target field filtering、all fields filtered。
- JSON 与 MessagePack 对 struct / ObjectTable 使用同一逻辑 payload。
- struct expanded value 和 JSON cell 都能导出为 nested object。
