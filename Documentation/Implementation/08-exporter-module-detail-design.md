# 08. Exporter 模块详细设计

本文档描述 `src/exporter/` 的 data export 实现边界。Exporter 模块负责把已经加载、归一化并通过 validation 的数据导出为数据产物，包括运行时表数据、gettext POT、asset manifest 等；它与 Codegen 平行，不再承载工程构建主流程。

## 8.1 设计目标

Exporter 模块需要达成：

1. 定义 exporter 内部配置 `TargetDataConfig`，由 build 从统一 `NormalizedExportTask` 适配得到。
2. 定义 export type exporter trait 和 registry。
3. 内置 JSON、MessagePack 与 `bin` 三种 runtime data export type；`bin` 使用 schema-aware positional typed layout；通过同一统一 export task `type` 支持已实现的 gettext POT、Lua 和 Asset Manifest 聚合 export type。
4. 对 CollectionTable 与 ObjectTable 生成正确 runtime payload shape。
5. 复用 DataResolver 解释 struct JSON cell / expanded value / target visibility，避免 exporter 自己重复实现 source layout 解析。
6. 为未来二进制格式、自定义 export type、聚合数据产物和 artifact writer 留出扩展点。

非目标：

- 不执行数据加载计划；DataLoader 属于 `data` 模块，由 `build` 编排调用。
- 不执行 validation；validation 属于 `validation` 模块，由 `build` 在导出前调用。
- 不生成代码；当前 codegen 默认不消费 exporter 真实写出的 runtime data artifact path / format，而是使用逻辑 table name 作为 runtime load key。
- 不执行翻译或读取外部 `.po` / 文本库；gettext POT exporter 只生成待翻译模板，运行时翻译由业务 translator / localization runtime 处理。

## 8.2 模块结构

当前实现按职责拆分为：

```text
src/exporter/
  mod.rs              # 对外 API 与 re-export
  config.rs           # TargetDataConfig
  model.rs            # ExportedTable / ExportedTablePayload / ExportedValue
  registry.rs         # DataFormatRegistry / DataFormatExporter / AggregateDataExporter / builtin_exporters
  runtime.rs          # runtime table data artifact plan、并行导出编排、artifact 写入入口
  payload.rs          # CollectionTable / ObjectTable 共享 payload builder
  json.rs             # JsonDataExporter
  messagepack.rs      # MessagePackDataExporter
  binary.rs           # BinaryDataExporter：CFGB header + schema revision + positional typed payload
  aggregate.rs        # 后续可拆出的聚合 data exporter 共享 context / artifact plan helper
  gettext_pot.rs      # 已实现：type = "gettext_pot" / alias "pot"
  asset_manifest.rs   # 已实现：type = "asset_manifest" / alias "assets"，JSON + Unreal DataTable CSV
  tests.rs            # exporter 核心行为测试
```

`mod.rs` 仅保留模块声明和公共 re-export，确保既有消费者仍可通过 `crate::exporter::*`
访问稳定 API；新增格式应优先落在独立格式模块中，核心 registry / runtime / payload 不内联具体格式逻辑。

`TargetDataConfig.format` 是 exporter 内部使用的规范 export type，来自 Project TOML 的 `[[targets.exports]].type`。运行时表数据 export type（JSON、MessagePack、`bin`）通常是“一张 table 一个 artifact”；聚合 export type（POT、Lua、asset manifest、summary manifest）可能跨所有 table 汇总成一个或多个文件、读取非 runtime 辅助字段，artifact path 规划也不是 `table -> extension` 的简单映射。因此实现上在同一个 `DataFormatRegistry` 下维护 `DataFormatExporter`（运行时表格式）与 `AggregateDataExporter`（聚合数据产物）两类能力，但 TOML 层不再引入 `targets.special_exporters`。

## 8.3 配置与 registry

```rust
pub struct TargetDataConfig {
    pub name: Option<String>,
    pub output: PathBuf,
    pub format: String,
    pub options: BTreeMap<String, toml::Value>,
}
```

语义：

- `name`：可选 export task 名称。单个未命名 task 由 build 归一化为 `default`；多个未命名 task 默认使用 `type`。
- `output`：数据产物输出目录。运行时表数据按 table 规划文件；POT / manifest 等聚合格式按格式选项规划文件。
- `format`：exporter 内部字段，由 `[[targets.exports]].type` 归一化而来；当前内置支持 runtime data 的 `json`、`messagepack`、`bin`，并接受 `msgpack` 与 `binary` alias；当前内置聚合 export type 支持 `gettext_pot` / alias `pot`、`lua_table` / alias `lua`、`asset_manifest` / alias `assets`。Asset Manifest 的 `format = "csv"` 是其专属选项，不拆分为 `asset_manifest_csv` exporter type；`bin` 紧密二进制格式详见 `13-runtime-binary-format-detail-design.md`。
- `options`：export type 专属配置，由 `[[targets.exports]]` 下除通用字段外的 flattened 字段归一化得到，例如 `type = "gettext_pot"` 下的 `domain`、`file_name`、`source_reference`，或 `type = "lua_table"` 下的 `module_name`、`root_file`、`row_layout`。

```rust
pub trait DataFormatExporter: Send + Sync {
    fn name(&self) -> &'static str;
    fn aliases(&self) -> &'static [&'static str];
    fn dependency(&self) -> ExportDependency { ExportDependency::MetaAndData }
    fn extension(&self) -> &'static str;
    fn plan_table_artifact(&self, table_meta: &LinkedTableMeta<'_>) -> Result<RuntimeArtifactPlan>;
    fn serialize_table_to_stream(..., stream: &mut ArtifactWriteStream) -> Result<()>;
    fn serialize_table(...) -> Result<Vec<u8>>; // legacy fallback only
}

pub trait AggregateDataExporter: Send + Sync {
    fn name(&self) -> &'static str;
    fn aliases(&self) -> &'static [&'static str] { &[] }
    fn dependency(&self) -> ExportDependency { ExportDependency::MetaAndData }
    fn plan_artifacts(
        &self,
        out_dir: &Path,
        task: &TargetDataConfig,
        linked_meta: &LinkedMetaModel<'_>,
        target_tags: &TargetTags,
    ) -> Result<Vec<PathBuf>>;
    fn export_to_sink(
        &self,
        out_dir: &Path,
        task: &TargetDataConfig,
        linked_meta: &LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
        sink: &ArtifactSink,
    ) -> Result<()>;
    fn export(...) -> Result<Vec<Artifact>>; // legacy fallback only
}
```

`DataFormatRegistry` 保存 table exporter 与 aggregate exporter 列表并按 name / alias 查询，同时暴露 dependency contract 查询。当前 data-backed exporter 的 dependency 均为 `MetaAndData`；没有单独的 `DataOnly` 模式，因为缺少 Meta 时无法可靠完成类型解释、target 过滤、索引语义和 validation。Exporter implementation 必须是 thread-safe 和 reentrant，因为 `build` 会在 CPU worker pool 中并行规划多个 export task、并行序列化多个 table artifact，并在 validation gate 通过后并行执行多个 data-backed exporter；aggregate exporter 也不得在实例内保存本次 build 的可变状态。table exporter 的主路径是 `serialize_table_to_stream()`；`serialize_table() -> Vec<u8>` 只作为少量测试和兼容调用的 legacy fallback。aggregate exporter 的主路径是 required `export_to_sink()`；`export() -> Vec<Artifact>` 只作为 legacy fallback。新增 exporter 应优先实现 streaming writer / sink writer。

内置格式通过 `builtin_exporters()` / aggregate registry 注册到 `DataFormatRegistry`，当前 table exporter 包括 `JsonDataExporter`、`MessagePackDataExporter` 和 `BinaryDataExporter`；aggregate exporter 包括已实现的 `GettextPotExporter`、`LuaTableExporter` 与 `AssetManifestExporter`。新增导出格式应优先新增 `src/exporter/<format>.rs` 并实现对应 exporter trait，格式名、alias、dependency、扩展名和序列化逻辑由格式模块自己维护；核心模块只负责 registry、调度和共享 payload，不应内联具体格式序列化实现。JSON 使用 `serde_json::to_writer_pretty()`，MessagePack 使用 `rmp_serde::encode::write_named()`，`bin` 的 encoder 可直接写 `ArtifactWriteStream`，不再为最终 artifact 先构建完整 `Vec<u8>`；POT、Lua 和 Asset Manifest 聚合导出器也通过 `export_to_sink()` 直接写入 shared sink，不再经 `Vec<Artifact>` 回到 build。`bin` 属于私有 positional wire layout，不直接 serde 序列化 `ExportedTablePayload`，而是与 JSON / MessagePack 共享 target-aware resolved export view 后渲染为 positional layout。

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
- list value 输出为 array；JSON / MessagePack 中 scalar list element 仍输出 string-compatible value，`bin` 中按元素类型写入 typed value。
- `bin` record 的 optional bitmap 由统一的 `PresenceSlot -> write_optional_bitmap` 路径写入：table record 先按可见字段顺序记录 optional field slots，再按可见 fragment 顺序记录 fragment slots；struct / fragment record 只记录可见 optional field slots。required field 不占用 bitmap bit，presence bit 使用 little-endian bit order（第 0 个 optional slot 对应 byte bit 0）。

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
- JSON 与 MessagePack 必须通过同一 payload builder，确保逻辑 shape 一致；`bin` 复用同一份 resolved export view，但 wire layout 按 schema 顺序紧密排列，不保存字段名。
- JSON / MessagePack 的 scalar runtime contract 当前仍为 string-compatible；如果未来改为 typed scalar，必须作为 named payload 的全局 runtime data contract change 同时影响 CollectionTable。`bin` 是 schema-aware typed binary encoding，按 Meta type 写 int/float/bool/enum bytes，不改变 JSON / MessagePack 的 named payload contract。
- ObjectTable 中的 list 字段输出为 array，例如 `"starter_item_ids": ["2001", "2002"]`。

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
- array/list JSON cell 支持已通过 `DataValue::Array(Vec<ArrayElementValue>) -> ExportedValue::Array` 转换接入；scalar list element 只常驻 element value 与 raw value，元素级诊断路径由 parser 按需构造。后续 map 或 expanded array path 仍应继续复用该转换边界。

## 8.6 Artifact path 约束

Exporter 负责在单个 export task 内检查 artifact path 冲突：

- `plan_table_artifact()` 给出每张 table 的相对输出路径。
- `ensure_unique_runtime_artifact_paths()` 拒绝同一个 exporter 计划出的重复路径。
- 聚合 exporter 通过 `plan_artifacts()` 给出一个或多个相对输出路径，例如 POT 单文件或多 domain 多文件。
- 跨 export task 的输出路径冲突由 build 在目标级别提前检查，因为多个 export task 可能写入不同或相同 output root；该 planning step 会在 CPU pool 中按 export task 并行执行，再按 export task 声明顺序稳定合并 path key。

POT 这类单文件聚合 data exporter 的 artifact key 建议使用 `(target_name, data_name, format)`；未来多 domain POT 可扩展为 `(target_name, data_name, format, domain)`。

## 8.7 聚合 Data Exporter 设计

当前在 `DataFormatRegistry` 下分别注册运行时表格式 exporter 与 aggregate exporter，而不是新增 TOML 层的 `special_exporters`：

```rust
pub struct DataFormatRegistry {
    exporters: Vec<Box<dyn DataFormatExporter>>,
    aggregate_exporters: Vec<Box<dyn AggregateDataExporter>>,
}
```

实现边界：

1. Build 层负责解析统一 export task 配置、按 `format` 查询 registry，并把 output dir、`TargetDataConfig`、linked Meta、target tags 与 raw/normalized data snapshot 传给 exporter，最后合并 artifact plan。
2. 聚合 exporter 默认读取 target ExportView，而不是 Full DataSet。若需要读取 `export = false` 辅助字段，必须通过 Meta 字段策略或 exporter policy 明确允许。
3. 聚合 exporter 通过 required `export_to_sink()` 直接写入 shared `ArtifactSink`；`export() -> Vec<Artifact>` 只作为测试 / 兼容 helper，不回到 Build 主路径。
4. 聚合 exporter 不绕过 validation；若需要 text placeholder / context 诊断，应在 validation 增加规则或由 exporter 产出 warning/error diagnostic，但不能静默修复数据。
5. 聚合 exporter 与 runtime data exporter 共享 DataResolver / resolved value 模型，避免再次解释 CSV raw cell。
6. 需要 project name/desc/copyright 等 artifact header 信息时，应优先复用共享的 `ArtifactProjectInfo`，不依赖 codegen 模块的 `CodeExportProject`，避免 `exporter -> codegen` 依赖倒置。

### 8.7.1 GettextPotExporter (`type = "gettext_pot"`)

`GettextPotExporter` 当前模块为 `src/exporter/gettext_pot.rs`，通过 `[[targets.exports]] type = "gettext_pot"` 或 alias `pot` 启用。

输入模型建议：

```rust
pub struct TextExtractionEntry {
    pub msgid: String,
    pub msgctxt: Option<String>,
    pub table_name: String,
    pub field_path: String,
    pub record_key: Option<String>,
    pub source_reference: Option<SourceReference>,
    pub translator_comments: Vec<String>,
}
```

抽取流程：

1. 遍历 target ExportView 中所有可见 table。
2. 对 CollectionTable 按导出后的 row order 遍历；对 ObjectTable 使用 singleton logical row。
3. 首版遍历可见 top-level field，筛选 Meta primitive type 为 `text` 的字段；不把 `string` 自动纳入 POT。fragment / struct child text 深度抽取保留为后续扩展。
4. 从 resolved scalar value 读取 raw text 作为 `msgid`；optional empty text 不产出 entry。
5. 构造 `msgctxt`：默认 `table.field_path`，如果未来 Meta/source/target policy 提供 context，则使用显式 context。
6. 构造 source reference：当前优先使用 DataResolver 保留的 source path / row；CSV 列坐标尚未保留时不输出列号。`source_reference = "record_key"` 当前不生成 gettext `#:` 行，而是将 primary index record key 写入 translator comment（例如 `record=id=2001`）；`both` 当前只合并 source location，record key 仍作为 translator comment 输出。
7. 合并 translator comments：field `desc`、未来 text option、source comment column/row、自动生成的 `table/field/record` 描述。
8. 当 `deduplicate = true` 时，以 `(msgctxt, msgid)` 做稳定分组，合并 references/comments；当 `deduplicate = false` 时，按抽取顺序原样渲染 entries。
9. 渲染 gettext `.pot` 文本 artifact。

POT 渲染要求：

- 输出 UTF-8 text artifact。
- header entry 固定在首位；首版不写 `POT-Creation-Date`，保证测试可确定。未来若接入 `generated_at`，应继续保证 deterministic 测试路径可控。
- escape 函数覆盖 `\\`、`"`、`\n`、`\t`、`\r` 与其他控制字符。
- comments 顺序稳定：translator comments (`#.`) -> extracted metadata comments -> source references (`#:`) -> flags (`#,`) -> `msgctxt` -> `msgid` -> `msgstr ""`。
- 同一 `(msgctxt, msgid)` 的多个 source reference 按 table order / row order / field order 排序并去重。

配置选项建议：

| 选项 | 默认 | 说明 |
| --- | --- | --- |
| `domain` | `messages` | POT header 的 domain / Project-Id-Version 来源，未设置时使用 `messages`。 |
| `file_name` | `<domain>.pot` | 单文件 POT 输出文件名，相对 `TargetDataConfig.output`。 |
| `context` | `table_field` | `none` / `table_field` / `field`。显式 context 列属于未来扩展。 |
| `source_reference` | `source_location` | `source_location` / `record_key` / `both` / `none`。当前 `record_key` 不写 `#:`，record key 始终作为 translator comment 输出；`both` 只合并 source location。 |
| `include_translator_comments` | `true` | 是否写入 field desc 和显式 comment。 |
| `deduplicate` | `true` | 是否按 `(msgctxt, msgid)` 合并。关闭时按抽取顺序原样输出，调用方需自行处理 gettext 工具对重复 key 的行为。 |
| `strict_placeholder` | `false` | 未来启用 placeholder 诊断时，是否将不一致升级为 error。 |

未来扩展：

- `TextExtractionEntry` 不应绑定 gettext；后续可复用同一抽取模型输出 CSV、XLIFF、Android strings、Unity localization table 或自定义翻译平台 JSON。
- 新增 `I18nTextKind` 或 field option 后，extractor 根据 kind 生成普通 msgid、plural、ICU message 或 rich-text segments。
- 外部 `.po` / localized catalog 导入不属于 exporter；如果未来需要“导入翻译并生成 runtime localization bundle”，应作为新的 localization build task 或 data provider，而不是 POT exporter 的职责。

### 8.7.2 AssetManifestExporter (`type = "asset_manifest"`)

`AssetManifestExporter` 当前模块为 `src/exporter/asset_manifest.rs`，通过 `[[targets.exports]] type = "asset_manifest"` 或 alias `assets` 启用。它是与 JSON、MessagePack、`bin`、POT、Lua 同级的聚合 data exporter，读取 validation / export 补齐后同一份 target-aware `DataSet`，只抽取当前 target ExportView 中可见且非空的 primitive `asset` 字段。

配置选项：

| 选项 | 默认 | 说明 |
| --- | --- | --- |
| `file_name` | `asset_manifest.json` 或 `asset_manifest.csv` | 单文件 manifest 输出名；未设置时随 `format` 选择默认扩展名。 |
| `format` | `json` | 支持 `json` 与 `csv`。CSV 为 Unreal DataTable 可导入格式。 |
| `csv_mode` | `unreal_datatable` | 仅 `format = "csv"` 生效。`unreal_datatable` 输出完整 Unreal DataTable 兼容列；`simple` 只输出去重后的 `AssetRef` 一列。 |
| `deduplicate` | `true` | 是否把相同 `(kind, path)` 合并为一个 entry。 |
| `include_referenced_by` | `true` | 是否输出 / 展平引用来源。 |
| `include_source_location` | `false` | 是否附带 source path / row。 |

抽取流程：

1. 遍历当前 target 可见 table；CollectionTable 按导出后 row 过滤语义处理，ObjectTable 使用 singleton logical row。
2. 遍历可见 top-level field、active fragment field 和 fragment struct child field，筛选 `PrimitiveFieldType::Asset` 且声明了 `asset_kind` 的字段。
3. 从 canonicalized `DataSet` 读取非空 asset string，不重新解析资源路径。
4. CollectionTable 使用 primary index field 生成 record key，缺少 primary index 时回退 source row；ObjectTable 使用 `<object>`。
5. 默认按 `(kind, path)` 去重并用 `BTreeMap` / `BTreeSet` 保持 deterministic output。
6. JSON 输出固定 `version`、`target_tags`、`assets`；默认 CSV 输出固定列 `Name,AssetRef,AssetKind,ReferencedBy,Tables,Fields,RecordKeys,Sources`，其中 `Name` 是 ASCII-safe Unreal row name；`csv_mode = "simple"` 时只输出 `AssetRef`，并按 canonical asset string 去重排序。

## 8.8 测试边界

Exporter 模块测试应覆盖：

- registry 能解析内置 exporter 和 alias。
- 自定义 exporter plugin 可以注册和执行。
- runtime data artifact path 冲突在写文件前失败。
- CollectionTable 输出 rows envelope。
- ObjectTable 输出单 object shape，包括 only-header、default、target field filtering、all fields filtered。
- JSON 与 MessagePack 对 struct / ObjectTable 使用同一逻辑 payload。
- `bin` 写出 CFGB header、schema revision 与 target-aware positional payload。
- runtime table export 直接通过 `sink.create(path)` + `serialize_table_to_stream()` + `stream.finish()` 写出；custom exporter 测试覆盖 legacy `serialize_table()` fallback 不参与主路径。
- aggregate export 直接通过 required `export_to_sink()` 写出；custom aggregate 测试覆盖 legacy `export() -> Vec<Artifact>` fallback 不参与主路径，POT / Lua / Asset Manifest 测试覆盖直接落盘内容。
- struct expanded value 和 JSON cell 都能导出为 nested object。
- 聚合 data exporter 测试覆盖 POT 抽取、去重、context、source reference、translator comment、target filter、ObjectTable、artifact path conflict；Asset Manifest 测试覆盖 JSON deterministic output、Unreal DataTable CSV header / row name、simple CSV 资源去重清单、默认文件名、target-aware row filter 和 registry alias；fragment/struct child text 深度抽取仍属于后续扩展测试。
