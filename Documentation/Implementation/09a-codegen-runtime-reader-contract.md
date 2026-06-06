# 09a. Codegen Runtime Reader Contract

本文从 Codegen 模块详细设计中拆出，专门描述 generated code 与运行时 loader/reader 的契约。产品层语义以 [`../Design/06-export-cli.md`](../Design/06-export-cli.md) 为准。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 总览、IR、命名、冲突检测和测试总览。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、runtime loading 和 generated API 语义。
- [`13-runtime-binary-format-detail-design.md`](13-runtime-binary-format-detail-design.md) — `bin` runtime data schema revision 与 positional reader contract。

## 当前状态

- 当前内置 format-agnostic backend 的 runtime contract 覆盖范围见 [`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix)。
- Eager `GameData` 与 table-level `LazyGameData` 已实现 MVP。
- TypeScript 已提供 async 与 sync loading API；sync API 要求 loader 同步返回 reader。C# / Java 当前提供同步 `Load` / `load` 和 `LazyGameData` API。
- Generated code 使用逻辑 table name；真实 artifact path、后缀和格式由 loader / reader 映射。

## Lazy loading state machine

每个 generated table/object member 在 lazy facade 中拥有独立状态：

```text
Unloaded
  -> Loading      # 可选，仅多线程或 async Promise coalescing 需要显式表达
  -> Loaded(T)
  -> Failed(E)    # 可选；默认可以不缓存失败以允许重试
```

规则：

1. `Unloaded` 首次访问时调用对应 table 的现有 generated `load(loader)` / `Load(loader)` / `load(loader)`，复用同一套 reader parser、schema revision 校验与 index 构建逻辑。
2. 成功后缓存 `Loaded(T)`；后续访问直接返回同一 table/object 实例。
3. 加载过程中若 reader/decode/schema/index 构建失败，不能留下半初始化 table。默认策略是不缓存失败并返回错误；如果语言使用 Promise 共享并发加载，可以让 rejected Promise 只在本轮请求内共享，下一次访问重新创建 Promise。
4. Lazy facade 保留 `RuntimeDataLoader`，但不保留单次 table reader cursor。`RuntimeDataLoader.open(table.data_file)` 每次 table 首次加载时创建短生命周期 reader；reader 在 table 解析结束后释放。若接入方需要常驻 mmap/package/decode cache，应由 loader 自己管理。
5. `preload_all` / `PreloadAll` / `preloadAll` 按 CodeModel table 顺序触发所有 table/object accessor。它用于 fail-fast 和预热；执行成功后 lazy facade 的所有 table 都处于 Loaded。
6. `link_all` 在 lazy facade 上应先 preload 所有当前 target 可见 table，再执行 relation cache 预链接；否则可能因只加载 source table 而遗漏 target table。

## 与现有生成器模块的落地关系

- `repository.rs` 继续负责 table/object `load`、index 构建和 lazy accessor 渲染；lazy accessor 应复用现有 table `load` 函数，避免生成第二套 parser。
- `mod.rs` / facade 渲染增加 eager `GameData` 与 lazy `LazyGameData` 两套 facade。默认仍生成 eager；lazy 可无条件生成，也可由 codegen option 控制。为了 API 稳定，建议先无条件生成 lazy facade，除非代码体积成为问题。
- `symbols.rs` 需要把 lazy facade 类型名、`loadLazy` / `preloadAll` / async relation helper 等纳入冲突检测。
- 测试需要覆盖：eager load 不持有 loader；lazy 构造不调用 loader；首次访问某 table 只打开该 table；重复访问不重复打开；`preloadAll` 打开全部 table；lazy relation helper 会触发 target table 加载；TypeScript 并发访问共享同一个 Promise。

Reader 基础语义：

- `enter_field(name)` / `enterField(name)` 用于 Rust / TypeScript named-field layout 的底层 cursor API。C++ runtime contract 已收敛为 schema-driven 字段 API，不再暴露 `EnterField`。
- `enter_required_field(name)` / `EnterRequiredField(name)` / `enterRequiredField(name)` 是 generated parser 优先使用的 schema-driven 字段入口。Named reader 按字段名进入并执行 required/null 校验；binary reader 对 required 字段执行 no-op，因为字段由 generated schema 顺序隐式定位。
- `enter_optional_field(name)` / `EnterOptionalField(name)` / `enterOptionalField(name)` 返回字段是否 present。Named reader 默认按字段 missing/null 判断；binary reader 消费当前 record 的下一个 optional presence bit。present 后 generated parser 读取字段 payload，并通过 `exit_field` / `ExitField` / `exitField` 结束字段；binary reader 的 exit field 是 no-op。
- `read_enum_index(variants)` / `ReadEnumIndex(variants)` / `readEnumIndex(variants)` 统一 enum 读取。Named reader 将 string enum value 映射为 variants 下标；binary reader 读取 ordinal 并校验范围。Generated enum parser 只根据 index 构造目标语言 enum，不再分支判断 reader layout。
- `enter_index(i)` / `EnterIndex(i)` / `enterIndex(i)` 用于 payload array 或 positional record。越界是读取错误，应带上 index 上下文。进入成功后通过 `exit` / `ExitField` / `exitField` 返回上一层；C++ 使用 `ExitField` 作为统一回退 API。
- reader contract 采用可回退 cursor，而不是每次字段/数组访问返回新的 reader 对象。C++ adapter 因此避免 `std::make_unique` 子 reader 堆分配；TypeScript adapter 避免频繁临时对象；Rust adapter 避免 cloning `serde_json::Value` / MessagePack `Value` 子树。
- 分层命名约定：Meta / IR / generated helper 层使用 `List` 表示 schema 字段类型；runtime value / wire payload 中的 `Array` 只表示 JSON / MessagePack / binary 的物理容器 shape。`row_count` / `ReadRowCount()` / `readRowCount()` / `read_row_count()` 只表示 CollectionTable 顶层记录数；`list length` / `ReadListLength()` / `readListLength()` / `read_list_len()` 只表示 Meta list 字段元素数。二者必须是不同语义，互不 fallback。
- `len` / `length` 只对 payload array / object / map 有意义；对 scalar 调用应报错。Rust / TypeScript named reader 可在 `read_list_len` / `readListLength` 中复用底层 `len` / `length`，但 generated parser 不直接用通用长度 API 读取 list；C++ runtime contract 不暴露通用 `Size()`。兼容别名 `read_array_len` / `readArrayLength` / `ReadArrayLength` 仅保留给旧 adapter，语义委托给 list length，不得 fallback 到 row count。
- 最小 scalar API 包含 int、float/number、bool、string。`text`、`asset` 按 string 读取；enum 先按 string 读取，再由 generated parser 校验枚举值。
- 当 exporter 输出字符串 cell 时，推荐 generated parser helper 兼容 string-to-int、string-to-float、string-to-bool；reader 也可以直接在 `as_*` 中兼容字符串，但跨语言一致性以 generated parser 规则为准。
- missing 与 null 必须区分：missing 表示字段不存在；null 表示字段存在但值为空。required 字段 missing 或 null 都应由 generated parser 报错；optional field / optional fragment missing 时得到 `None` / `undefined`；null 是否允许作为 optional empty value 由生成器策略统一决定，第一阶段建议将 null 等价为 optional empty，但对 required 字段仍报错。
- 错误上下文由 generated parser 包装 table、file、row index、field path；reader 只需要报告底层类型/格式错误。

## 抽象加载器设计

### 为什么需要抽象加载器

生成代码不应该假设数据来自本地文件系统。不同接入环境可能是：

- Rust 服务端：从磁盘或对象存储读取 JSON；
- C++ 游戏客户端：从包内资源、虚拟文件系统或引擎 AssetManager 读取；
- TypeScript：从 HTTP、Node fs、浏览器 bundle 或 IndexedDB 读取。

因此当前生成代码依赖一个通用 loader / reader 抽象：loader 给定逻辑 data file path，负责打开对应 JSON / MessagePack / 引擎资源并返回格式无关的 reader；生成代码再用 reader 读取 payload envelope `{ meta, rows }`、逐字段解析并返回 typed rows。loader 接口不包含按表生成的 typed loader 方法，避免用户每新增表或字段都要修改 loader 实现。

### 语言无关接口

```text
RuntimeDataLoader
  open(path) -> RuntimeDataReader

RuntimeDataReader
  field(name) / index(i)
  begin_object / end_object
  begin_array / end_array / len
  read_scalar(type)
```

各 backend 暴露的 runtime loader / reader 具体命名以 [`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) 和对应 backend 专题文档为准；语义上都必须提供“打开逻辑 table path 并返回通用 reader cursor”的能力。接口方法不得按表名或字段名生成；表名、字段名、字段顺序只出现在生成的 parser 函数内部。

reader 需要支持两种读取模式：

1. named-field：生成代码调用 `field("id")`、`field("name")` 等方法，适合 JSON object、MessagePack map、引擎反射对象。
2. positional：生成代码按 CodeModel 中稳定字段顺序调用 `index(0)`、`index(1)` 或连续 `read_next_*`，适合未来紧密二进制布局。选择 positional layout 时，exporter 和 codegen 必须共享同一字段顺序；顺序定义应来自 linked Meta 中 target-visible fields、fragment fields 和 object fields 的稳定遍历结果。

### 数据格式适配

加载器实现只负责打开资源并提供通用 reader；生成代码负责解析业务对象：

```text
loader.open("items")
  -> read bytes/text
  -> decode to generic reader/value/cursor
  -> generated code reads `rows`
  -> generated code loops rows and reads fields `id`, `name`, ...
  -> generated code constructs Vec<ItemRow>
  -> ItemsTable::new / constructor
  -> indexes
```

这样让生成代码保持对文件系统和具体解析库无绑定，同时让用户 loader 保持稳定。代价是生成代码需要包含每种语言的 runtime parser helper（字段读取、类型转换、required/default/optional 处理、错误上下文），但这些 helper 可以由生成器稳定输出，不需要用户手写。

### 默认 loader

可以为每种语言生成一个可选的默认 loader / reader adapter 示例，但核心产物只依赖接口，不强制生成默认文件 loader：

| Language | 默认实现 |
| --- | --- |
| Rust | `FsJsonRuntimeDataLoader` + `JsonValueReader` 或 `FsMessagePackRuntimeDataLoader` + `MessagePackValueReader`，可选生成/复制 |
| C++ | `FileSystemDataFileLoader { root }` + JSON/MessagePack value reader，依赖可选生成 |
| TypeScript | `FetchDataFileLoader` / `NodeFsDataFileLoader` + JS value reader，二选一或都不默认生成 |
| Unreal C++ | Unreal module 内实现 `IConfigForgeRuntimeDataLoader` / `IConfigForgeRuntimeDataReader`，当前仓库只提供测试 shim，不提供生产 adapter |
| C# | `System.Text.Json` value reader + filesystem loader，当前仅在 standard_project harness 中作为示例 |
| Java | JDK-only / Jackson / Gson value reader + filesystem 或资源 loader，当前仅在 standard_project harness 中作为示例 |

当前实现：

- [`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) 中的 format-agnostic backend 当前已生成通用 loader / reader interface，并由生成代码完成 typed parsing。
- 集成测试 harness 直接复用 `runtime/rust/serde_json_reader/src/runtime_reader.rs` 与 `runtime/rust/serde_msgpack_reader/src/runtime_reader.rs`，确保同一份 generated Rust code 可通过不同格式 reader 读取等价数据；新增表或字段时无需在 harness 中维护自定义 reader，只需要更新断言覆盖代表性查询。
- 后续可按语言新增可选 adapter，而不是强制把文件系统或第三方解析库纳入生成代码。

默认 decoder 策略：

- 核心生成代码生成通用 reader interface，但不强制 runtime 依赖具体 JSON / MessagePack 包。
- JSON / MessagePack 字节到通用 reader/value 的解析放在业务侧 `RuntimeDataLoader` 实现或后续可选 adapter 中。
- 通用 reader/value 到 typed rows 的解析放在生成代码中。
- MessagePack decoder 不绑定具体 npm / C++ / Rust 包。

## Runtime data 文件约定

当前 exporter 输出形态是包含 `meta` 和 `rows` 的 payload。JSON 与 MessagePack 使用同一逻辑结构，只是编码方式不同。目标态中业务侧 `RuntimeDataLoader` 只需要把该 payload 暴露为通用 reader；生成代码读取时至少应支持当前 shape：

```json
{
  "meta": { "name": "items" },
  "rows": [
    { "id": "1", "name": "Sword" }
  ]
}
```

未来也可以支持更轻量的：

```json
[
  { "id": 1, "name": "Sword" }
]
```

第一阶段推荐导出 runtime data 时同步调整为 typed payload：

- int/float/bool 输出 number/bool；
- string/text/asset/enum 输出 string；
- struct 后续输出 object。

如果短期仍输出字符串 cell，生成代码必须在加载时做类型转换。这会导致各语言生成重复 parse helper，但不会要求业务 loader 按表/字段重复实现转换逻辑。

建议分两步：

1. Codegen 第一阶段先兼容当前 `{ meta, rows }`，由通用 reader 暴露 runtime value，并由生成代码把字段 parse 为目标类型。
2. Data typed export 落地后，生成代码的 parse helper 可以减少 string parsing；loader / reader 接口仍保持不变。

### Exporter 写出实现方案

runtime data export type 由 exporter registry、运行时 loader / reader 组合决定；当前默认 codegen 不消费真实 runtime data export task 结果，也不通过 `RuntimeDataFile.format` 切换生成逻辑。文件名后缀只描述 artifact path，不作为 reader layout 的唯一来源。当前 codegen 层不再生成 `RuntimeDataFormat` 类型。若未来 manifest 或 adapter 配置需要显式记录格式，可在 build/runtime 层维护枚举，例如：

```rust
pub enum RuntimeTableDataFormat {
    Json,
    MessagePack,
    Bin,
}
```

写出流程：

1. Project Config load 后先构造 `NormalizedTargetConfig`，完成 export task name 规范化，并拒绝 MetaOnly codegen export task 的 `data` 字段。
2. `build_target` 消费当前 target 的 `Vec<NormalizedExportTask>`，并按 exporter dependency contract 构造 target-local execution view。当前实现直接执行 MetaAndData / MetaOnly 两个分支，没有 MetaAndData export task 时跳过 data load / validation，进入 MetaOnly-only 流程；完整 node-level DAG executor 仍是后续演进方向。
3. 对同一 `(project.input, effective_tags)` 构造一次 `DataLoadPlan`，使用 `DataLoader::new(data_root)` 加载；Meta 中 source path 继续由 `data_root.join(source.path)` 解析。
4. 对共享 DataSet 执行 normalize、target validation，并派生 DataSet-dependent ExportView；同 target 下 JSON / MessagePack / Bin / gettext_pot / lua_table 等多个 MetaAndData export task 复用该结果。
5. 每个 MetaAndData export task 根据自身 `type` 从 `DataFormatRegistry` 选择 exporter，根据自身 `output` 写出一套数据产物。table exporter 写出每表 runtime data artifact；aggregate exporter 写出 POT / manifest / Lua 等聚合 artifact。
6. `Json` 使用 `serde_json::to_writer_pretty` 通过 shared `ArtifactSink` 写出 `<table>.json`。
7. `MessagePack` 使用 `rmp_serde::encode::write_named` 或等价 named-field 编码通过 shared `ArtifactSink` 写出 `<table>.msgpack`。
8. Runtime table data export task 在 exporter 内部生成 `RuntimeDataFile` metadata 用于保持规划顺序和未来 manifest 扩展；Build DAG 当前只保留 task 名称用于稳定校验，不再向上聚合 runtime result struct。聚合 exporter 通过 `export_to_sink()` 直接写入 shared sink。
9. MetaOnly codegen export task 不选择 runtime data export task，统一向 `CodeExportInput` 传入空 `runtime_data_files`，让 CodeModel 使用逻辑表名。
10. Artifact path 规划必须跨所有 export task 统一检查，不能只在 codegen 内检查 generated code 的重复路径。

依赖建议：

```toml
rmp-serde = "1"
```

如果后续引入 manifest，manifest 中也应以 `(target_name, export_name, table_name)` 作为 runtime data artifact key，并记录每个 artifact 的 `type`、相对 data artifact root 的 `path`、最终写入 path 和 `table_name`。

### Codegen 读取实现方案

CodeModel 构建时使用逻辑 table name 作为 runtime load path：

- 忽略 runtime data artifact 的真实 path，避免 codegen 绑定具体格式。
- 每张表默认使用 `<table>` 作为逻辑 data file path。
- 每个 `CodeTable` 保留 `data_file`，语言生成器用它生成常量。
- 生成代码不根据 runtime export type 切换 parser；reader contract 负责把 named-field 或 packed layout 映射到同一组 `EnterRequiredField` / `EnterOptionalField` / `ReadRowCount` / `EnterIndex` / scalar API。
- 多个 runtime export type 的选择不进入 CodeModel 决策。

生成代码的 load 流程统一为：

```text
Table::load(loader)
  -> reader = loader.open(DATA_FILE)
  -> rows_reader = reader.field("rows") 或 reader.index(rows_slot)
  -> rows = generated parser loops rows_reader and calls read_<table>_row
  -> Table::new / constructor(rows)
  -> build indexes
```

这样 JSON 与 MessagePack 的字节解析差异落在 loader / reader 实现里，表结构、字段名、字段顺序、类型转换、required/default/optional 处理落在生成代码里，repository、index、GameData facade 不需要关心具体解析库。

## Runtime data 与加载接口测试计划

数据格式与生成代码加载接口相关测试应覆盖：

1. `TargetDataFormat` / registry 解析：`json`、`messagepack`、可选别名 `msgpack`，以及未知格式报错。
2. Runtime data 写出：
   - JSON target 生成 `<table>.json`，内容可用 `serde_json` 读取；
   - MessagePack target 生成 `<table>.msgpack`，内容可用 `rmp-serde` 读取为同一 payload；
   - `RuntimeDataFile.path` 与实际文件后缀一致。
3. Project Config schema：接受 `[[targets.exports]]`；旧 `[[targets.data]]` / `[[targets.code]]` 不作为兼容目标；空 export task name、重复 name、非法字符都应报错；单 export task 省略 name 时规范化为 `default`，多 export task 省略 name 时默认使用 `type`。
4. 多 MetaAndData export task：同一 target 同时配置 JSON 与 MessagePack，两个 output 都生成对应文件，且相同 `(project.input, effective_tags)` 的数据加载/normalize/validation 可以复用。
5. CLI extra tags：不同 effective tags 分别加载、normalize、validation 并派生 DataSet-dependent ExportView，不能复用另一个 tags 组合的 ExportView。
6. MetaOnly codegen 格式无关：`data` 设置时必须报错；target 有多个 MetaAndData export task 时，codegen 仍只生成一份逻辑表名代码。
7. 路径语义：入口 `ConfigForge.toml` 中 `project.input` / export `output` 相对 project root；import 片段中的相对路径相对片段目录；Meta source path 相对 `project.input`；`RuntimeDataFile.path` 相对 data artifact root。
8. artifact 冲突：所有 export task 之间的输出路径冲突都应报错。当前实现的冲突 key 是不依赖文件存在性的 lexical path normalization，至少消除 `.` / `..` 造成的字符串差异；它不覆盖大小写不敏感卷上的 case alias，也不解析 symlink alias。中期可对已存在父目录 canonicalize，并在 macOS / Windows 或探测到大小写不敏感卷时引入 case-fold key；不存在路径必须保留 lexical fallback，不能要求输出目录预先存在。
9. CodeModel 默认文件名：始终使用逻辑表名，不包含 `.json` / `.msgpack` / `.bin` 后缀。
10. Rust 生成代码包含：
   - 通用 `RuntimeDataLoader` / `RuntimeDataReader`；
   - 每张表的 `DATA_FILE`；
   - row struct、table parser、table repository 和 index helper。
11. C++ 生成代码包含：
   - 通用 `IRuntimeDataLoader` / `IRuntimeDataReader`；
   - 每张表的 `DataFile`；
   - row struct、table parser、table repository 和 index helper。
12. TypeScript 生成代码包含：
   - 通用 `RuntimeDataLoader` / `RuntimeDataReader`；
   - 每张表的 `dataFile`；
   - row interface、table parser、table repository 和 index helper。
13. CLI integration：分别构建 JSON target、MessagePack target，以及同 target 多 MetaAndData + 多 MetaOnly export task 的标准工程，验证 runtime data 文件和 generated code 文件同时生成，并检查代码中引用逻辑表名且不随 runtime export type 改变。
