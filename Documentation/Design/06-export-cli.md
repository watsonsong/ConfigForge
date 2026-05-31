# 06. 导出与 CLI 设计

## 6.1 Export Target

一个 target 描述一次逻辑导出。

字段：

| 字段 | 说明 |
| --- | --- |
| `name` | 稳定 target 名称。 |
| `exports` | 当前 target 下的导出任务列表。每个任务通过 `type` 选择 exporter，由 exporter registry 的 dependency contract 决定只依赖 Meta，还是同时依赖 Meta + Data。 |

Project-level `name` / `desc` / `copyright` 可被 target 的代码生成器、schema exporter 或 manifest exporter 用于生成文件头。Codegen export task 可以通过 `file_header` 模板覆盖默认文件头。

路径解析语义：

- CLI `build --project path/to/ConfigForge.toml` 指定的 Project Config 文件所在目录是 project root。
- `project.input` 与 `targets.exports.output` 的相对路径均相对声明它们的 Project Config 文件所在目录解析；入口文件中的相对路径因此等价于相对 project root。
- `imports` 引入的配置片段先按片段文件目录解析自身相对路径，再合并到总配置，避免片段移动到子目录后路径语义丢失。
- Meta 中 table `source`、`sources[].path`、`override_sources[].path` 不直接相对 `ConfigForge.toml`，而是相对 project-level `project.input` 目录查找。

### 6.1.1 Export Task

`[[targets.exports]]` 描述当前 target 要生成的产物、写到哪里，以及使用哪种 exporter。一个 target 可以声明多套 export task，例如同时导出 JSON 供工具链调试、MessagePack 或 `bin` 供客户端运行时加载，也可以导出 gettext POT、Lua table、Rust/C++/TypeScript 访问代码等产物：

```toml
[[targets]]
name = "client"

[[targets.exports]]
name = "json"
output = "Generated/Data/Json"
type = "json"

[[targets.exports]]
name = "messagepack"
output = "Generated/Data/MessagePack"
type = "messagepack"

[[targets.exports]]
name = "i18n_pot"
output = "Generated/I18n"
type = "gettext_pot"
domain = "game_config"
file_name = "game_config.pot"
source_reference = "source_location"
include_translator_comments = true
deduplicate = true
```

字段：

| 字段 | 说明 |
| --- | --- |
| `name` | 当前 target 内唯一的 export task 名称。单 export task 可省略并由实现填入默认名；多 export task 省略时默认使用 `type`。 |
| `output` | 产物输出目录。运行时表数据会在该目录下按表名生成文件；POT / manifest 这类聚合格式会在该目录下按格式选项规划一个或多个文件；codegen 会在该目录生成语言文件。 |
| `type` | exporter 类型 ID。运行时格式支持 `json`、`messagepack` 和 `bin`；聚合/内嵌数据格式支持 `gettext_pot` / alias `pot`、`lua_table` / alias `lua`；代码生成支持 `rust`、`cpp`、`typescript` 及其 alias。 |
| exporter 专属字段 | 可选。格式专属选项直接摊平在对应 `[[targets.exports]]` 下，例如 `type = "gettext_pot"` 的 `domain`、`file_name`、`source_reference`、`include_translator_comments`、`deduplicate`。 |

导出规则：

- 每张 table 输出一个 runtime data 文件。
- `json` 使用 `.json` 后缀，`messagepack` 使用 `.msgpack` 后缀，`bin` 使用 `.bin` 后缀。
- JSON / MessagePack 承载同一套 named payload：table meta 摘要和 normalized rows；字段名使用 Meta canonical name。`bin` 使用同一 target-aware export view，但 wire layout 不保存 name/tag，按 generated code 共享的 schema 顺序紧密排列。
- 同一 target 下多个 MetaAndData export task 共享同一个 `project.input` 与 effective tags，因此执行层只加载、归一化、校验一次，再分发给不同 exporter。
- 同一个 target 的 code generation 不绑定 runtime data export task。生成代码使用逻辑表名作为默认加载路径，具体文件后缀和格式由运行时传入的 loader / reader 实现决定。
- MetaOnly-only target 可以不设置 `project.input`；生成代码仍可通过同一套逻辑表名加载 API 在运行时读取任意受支持格式的数据。
- Project Config 使用 `[[targets.exports]]` 数组表表达所有导出任务；`type` 与 registry dependency contract 决定它是运行时表数据、国际化模板、资源清单、Lua 内嵌数据还是代码产物。

### 6.1.2 Code Generation Export Task

一个 export target 可以生成多个代码产物，例如同一次导出同时生成 Rust、C++ 和 TypeScript 访问代码：

```toml
[[targets.exports]]
type = "cpp"
output = "Generated/Cpp"
module_name = "GameConfig"
namespace = "game::config"
file_header = "// {{project.name}}\n// {{project.copyright}}\n// Generated at {{datetime}}"
keyword_conflict = "suffix_underscore"

[targets.exports.naming]
type = "pascal_case"
enum_value = "pascal_case"
field = "snake_case"
function = "pascal_case"
constant = "pascal_case"

[targets.exports.cpp]
namespace = "game::config"
```

通用要求：

- `type` 决定使用哪个代码生成器。
- `output` 是该代码生成 export task 的根目录，不与 runtime data output 混淆。
- `file_name` 可覆盖生成代码文件名；Rust / TypeScript 单文件输出会自动补齐默认扩展，C++ 会用同一 stem 生成 `.hpp` / `.cpp`。
- `data` 不支持设置。生成代码格式无关，只依赖 Meta 和 target tags；格式选择由接入方在运行时传入的 loader / reader 决定。
- `file_header` 是文本模板，支持 `{{project.name}}`、`{{project.desc}}`、`{{project.copyright}}`、`{{datetime}}`。
- `keyword_conflict` 控制 Meta 名称转换成目标语言标识符后碰到关键字时如何处理。

CLI 可以通过 `--tag <tag>` 为本次构建追加 extra tags。实际参与 table/field/index `export_when`、`row_export`、`override_sources[].export_when`、runtime export 和 codegen 的是 effective target tags：`target.tags ∪ CLI extra tags`。例如 `config-forge --target client_shipping --tag s1` 会在 `client_shipping` 原有 tags 上追加 `s1`，从而启用只在 S1 赛季生效的 `materials_s1.csv` override source。
- `naming` 控制 type、field、function、constant 等符号的大小写组织。
- 语言生成器可以声明额外配置，但必须放在该 export task 内对应语言子表中，不能污染其他语言任务；实现层应按语言使用 tagged enum / 多态配置承载这些差异。
- 生成代码必须暴露 runtime data 加载抽象接口，不直接绑定本地文件系统、引擎资源系统、网络或包内资源。

当前内置语言特定配置：

| 子表 | 字段 | 说明 |
| --- | --- | --- |
| `[targets.exports.rust]` | `derive_serde` | 为生成类型派生 serde 序列化/反序列化 trait。 |
| `[targets.exports.cpp]` | `namespace` | C++ namespace 覆盖；未设置时可使用通用 `namespace`。 |
| `[targets.exports.typescript]` | `readonly_fields` | interface 字段是否生成 `readonly`。 |

`[targets.exports.i18n]` 控制 `text` 翻译 helper 的调用形态：默认 `mode = "generic"`，保留显式 translator 参数；`mode = "global_function"` 时必须配置 `function`，生成代码直接调用该全局函数，例如 gettext/libintl 或项目自己的全局 i18n facade。`returns = "string"` 表示函数总是返回最终文案，`returns = "optional"` 表示函数可能 missing，helper 会 fallback 到 raw text。C++ 可用 `cpp_include` 注入 include，TypeScript 可用 `typescript_import` 注入 import。

`keyword_conflict` 策略：

| 策略 | 说明 |
| --- | --- |
| `suffix_underscore` | 自动追加 `_`，作为默认策略。 |
| `raw_identifier` | 使用目标语言原生关键字转义方式；语言不支持时应报错。 |
| `error` | 直接报错，要求用户修改 Meta 或命名配置。 |

`naming` 策略：

| 策略 | 说明 |
| --- | --- |
| `preserve` | 尽量保留原始名称，仅做非法字符处理。 |
| `pascal_case` | 类型名常用。 |
| `camel_case` | TypeScript 字段和函数常用。 |
| `snake_case` | Rust 字段和函数常用。 |
| `screaming_snake_case` | 常量或部分 enum value 常用。 |

代码生成器必须在写文件前完成：

1. 标识符合法性检查；
2. 关键字冲突处理；
3. 同作用域生成名冲突检查；
4. 输出路径冲突检查。

## 6.2 Data Export Format

当前第一阶段支持以下 runtime data export type：

| Type | 说明 |
| --- | --- |
| `json` | 默认格式，便于调试和通用接入。 |
| `messagepack` | 紧凑二进制格式，用于包体和加载性能更敏感的 runtime。 |
| `bin` | ConfigForge 私有紧密二进制格式；使用 positional typed layout、`.bin` 后缀、最小 magic/version header 和 generated code 的 expected schema revision，不改变 JSON / MessagePack named payload contract。 |

格式实现采用注册工厂模式：核心导出流程只根据 `[[targets.exports]].type` 从 `DataFormatRegistry` 中查找 exporter，不在流程中通过枚举 `match` 分发。新增导出格式应通过注册新的 exporter 实现扩展，避免修改核心导出编排逻辑。当前内置注册器提供 `json`、`messagepack` 与 `bin` 三种 runtime table data exporter，以及 `gettext_pot` / alias `pot`、`lua_table` / alias `lua` 聚合 exporter；`asset_manifest_csv` 等聚合格式仍为后续扩展。`bin` 的具体 wire format 见 `../Implementation/13-runtime-binary-format-detail-design.md`。

别名策略：

- 配置中使用规范名 `messagepack`。
- CLI 诊断使用 export type ID；生成代码不再包含格式枚举。
- 如需兼容 `msgpack` 简写，可作为解析别名，但文档和生成产物统一输出 `messagepack`。

当前内置聚合 / 内嵌数据 export type：

| Type | 说明 |
| --- | --- |
| `gettext_pot` / `pot` | Gettext POT 聚合 exporter；从当前 target 可见 `text` 字段抽取待翻译模板。 |
| `lua_table` / `lua` | Lua 内嵌数据 exporter；代码和数据合一，作为 MetaAndData export task 读取 source data，并生成 root facade + 每表一个 `.lua` 文件。 |

未来可扩展但当前不实现：

| Type | 说明 |
| --- | --- |
| `yaml` | 可选文本格式。 |
| `sqlite` | 可选数据库格式。 |
| `protobuf` | 未来设计，不作为当前实现要求。 |

不使用 `custom` 作为模糊格式。自定义格式必须声明明确 ID，例如 `studio_binary_v1`。

### Lua Table

`lua_table` 是 MetaAndData export type，而不是普通 codegen backend。它会同时消费 Meta 与当前 target 的已归一化、已校验数据，输出可被 Lua VM 直接 `require` 的数据+访问代码：

- 每张可见 table 生成一个 `.lua` 文件；
- 生成一个 root facade 文件，按访问情况 lazy `require` 对应 table 文件，也可提供 `preload_all()`；
- CollectionTable 每行默认以数组存储字段值，并用共享 metatable 支持 `row.field` 访问，以降低每行重复字符串 key 的内存成本；
- 为 CollectionTable 生成 primary / unique / non-unique index lookup，索引引用 row 对象，不复制 row 数据；
- ObjectTable 默认输出 keyed Lua table；
- 该格式依赖 `project.input`，不能用于未设置 source data root 的 MetaOnly-only target。

示例配置：

```toml
[[targets.exports]]
name = "lua"
output = "Generated/LuaConfig"
type = "lua_table"
module_name = "game.config"
root_file = "init.lua"
row_layout = "array_with_metatable"
lazy_require = true
```

详细实现设计见 [`../Implementation/09i-lua-embedded-data-generator.md`](../Implementation/09i-lua-embedded-data-generator.md)。

Protobuf 未来规则：

- 不使用 `stable_hash` 自动生成 field number 作为生产推荐策略。
- 如需要 Protobuf，Meta field 应支持显式 `field_number`。
- 删除/重命名字段需要 schema evolution 策略和 reserved number 设计。

### 6.2.1 生成代码的 Runtime Data 加载抽象

当前实现状态：Rust / C++ / TypeScript 生成代码使用 **Meta-agnostic reader loader 契约**，即接入方只实现通用 `open(path)` 并返回 `RuntimeDataReader` / `IRuntimeDataReader`；生成代码内部负责按 Meta 读取表、字段、fragment 和 object payload，并构造 typed rows / objects。生成代码 API 不包含按表生成的 `load_<table>_rows` / `Load<Table>Rows` / `load<Table>Rows` loader 方法。

生成代码必须把“如何取得配置文件”和“如何把通用序列化数据组装成当前 Meta 对应的 typed rows / typed objects”分层：

- **接入方实现的 RuntimeDataLoader 必须是 Meta-agnostic 的通用加载/读取协议**，不应包含按表、按字段生成的方法，也不应返回 `ItemRow`、`GameRulesRow` 等 Meta-specific 类型。
- **生成代码负责所有 Meta-specific 逻辑**：打开哪一个 table data file、读取 collection/object payload、按字段名或约定顺序读取字段、执行 runtime value 到目标语言类型的转换、构造 row/object、构建索引和 facade。

推荐抽象仍命名为 **RuntimeDataLoader**，但它表达的是“通用序列化 reader / cursor provider”，而不是“每张表的 typed row factory”。语言 API 形态可以按语言习惯调整，但语义应一致：

| 语言 | Runtime 加载接口语义 |
| --- | --- |
| Rust | `RuntimeDataLoader::open(path) -> Result<RuntimeDataReader, Error>`；reader 提供 begin/end object、begin/end array、按字段名读取 value、按序读取 value 等通用方法。 |
| C++ | `IRuntimeDataLoader::Open(path) -> std::unique_ptr<IRuntimeDataReader>`；reader 只暴露格式无关的 object / array / field / scalar 读取能力。 |
| TypeScript | `RuntimeDataLoader.open(path): Promise<RuntimeDataReader> | RuntimeDataReader`；reader 提供通用 JSON-like / MessagePack-like 读取 cursor。 |

通用 reader 至少需要覆盖两类 layout：

1. **Named-field layout**：例如 JSON object、MessagePack map。生成代码传入字段名，reader 定位并返回字段值；字段顺序不作为语义要求。
2. **Positional / packed layout**：例如当前 `bin` 使用的紧密二进制 record。生成代码按 Meta 中稳定的字段顺序调用 reader；exporter 与 codegen 必须使用同一字段顺序定义，reader 不需要知道字段名。

reader 的基础语义必须覆盖当前 Meta 类型：int、float、bool、string/text/asset、enum、object/array。enum 按 string 读取后由生成代码校验枚举值；`text` 和 `asset` 按 string 读取。字段缺失和字段存在但为 null 必须区分：required 字段缺失或 null 都由生成代码报错；optional field / optional fragment 缺失时视为 absent；null 是否等价 optional absent 由生成器策略统一定义，第一阶段建议只对 optional 视为 empty。当前 exporter 仍可能输出字符串 cell 时，string-to-int / string-to-bool / string-to-float 的兼容规则应由 generated parser helper 统一处理，避免每个业务 loader 重复实现。

因此业务 runtime 只需要长期维护一次通用 loader / reader 实现，可以选择：

- 用 JSON parser 实现通用 reader；
- 用 MessagePack decoder 实现通用 reader；
- 提供一个同时支持多种格式的 reader；
- 在引擎侧复用已有资源加载和反序列化 cursor。

格式选择由运行时传入的 loader / reader 实现决定。runtime data export task 只负责导出具体格式文件及后缀；生成代码保留逻辑 table name，并通过同一套通用读取路径工作。若 target 同时导出 JSON、MessagePack 与 bin，同一份生成代码可以分别配合 JSON loader、MessagePack loader 或 binary loader 加载对应格式。无论 Meta 新增多少表和字段，接入方的 RuntimeDataLoader 接口都不应变化；变化只体现在重新生成的解析代码中。

#### 6.2.1.1 Table-level Lazy Loading

当前 `GameData.load(loader)` 语义是 **eager load**：一次打开并解析当前 target 可见的所有 table/object，构造完整 repository / object facade 后返回；返回后 generated `GameData` 不再持有 loader，业务可以立即释放文件句柄、资源包 reader 或网络 client。这仍是默认模式，适合服务器启动阶段、编辑器校验、热更包预载等希望确定性发现所有数据/版本错误的场景。

当前已实现 **table-level lazy load** 模式：生成代码提供一个懒加载 facade，只在业务首次访问某张 table/object 时才调用 `RuntimeDataLoader.open(logical_table_name)` 并执行该 table 的 generated parser / index 构建。为了支持这种模式，懒加载 facade 必须持有 loader 或 reader provider；也就是说接入方传入的 loader 需要在懒加载 facade 生命周期内保持可用。单次 `open()` 返回的 `RuntimeDataReader` / cursor 不建议跨 table 长期持有：它应在该 table 完成解析后释放；如果某 runtime 需要常驻解码上下文或资源包句柄，应把它放在 loader 内部，而不是让 generated table 持有半打开的 cursor。

两种模式必须共存，且 API 语义明确区分：

| 模式 | 入口 | Loader 生命周期 | Table 加载时机 | 适用场景 |
| --- | --- | --- | --- | --- |
| Eager | `GameData.load(...)` / `GameData::load(...)` / `GameData::Load(...)` | 只在 load 调用期间借用；返回后可释放 | 所有可见 table/object 立即加载 | 启动预载、测试、校验、希望 fail-fast |
| Lazy | `GameData.loadLazy(...)` / `LazyGameData::new(...)` / `GameData::LoadLazy(...)` | facade 持有或借用；必须活到 lazy facade 结束或显式 preload 完成 | 首次访问某 table/object 时加载并缓存 | 大型配置、低内存客户端、按玩法模块访问 |

Lazy facade 的核心规则：

1. **按 table 原子加载**：一个 table/object 只会从 unloaded -> loaded 迁移一次。加载成功后缓存 typed table/object；后续访问不再调用 loader。加载失败时默认不缓存半成品，允许调用方修复外部条件后重试；若语言 runtime 更适合缓存失败，也必须记录为 failed state 并在文档中说明。
2. **索引随 table 构建**：CollectionTable 第一次加载完成时立即构建全部 declared indexes；后续 lookup 与 eager 模式相同，不做 per-query 解析。
3. **ObjectTable 同样懒加载**：ObjectTable source artifact 虽然只产生 singleton object，但仍是一张逻辑 table；第一次访问该 object 字段时打开对应 data file。
4. **并发访问策略语言化**：当前生成器不承诺统一强线程安全；除 TypeScript Promise coalescing 外，Rust/C++ lazy cache 的并发语义应按生成代码和业务封装确认，后续可通过语言 option 增加更强同步策略。即使两个线程同时触发同一 table 加载，也不能暴露两个不同 table 实例给同一个 facade。
5. **生命周期可释放**：eager 模式不保留 loader；lazy 模式保留 loader。若业务希望“先 lazy facade 构造、随后一次性加载全部表并释放 loader”，可提供 `loadAll()` / `preloadAll()` + `releaseLoader()` 或直接调用 eager load。当前保留 eager load 作为释放 loader 的主路径；lazy facade 支持 preload all 以便 fail-fast，但仍按语言实现决定是否显式释放 loader。
6. **错误发现时机改变**：lazy 模式把 table decode/schema revision/index duplicate 等错误推迟到首次访问该 table；文档和 API 命名必须让业务理解这一点。需要 fail-fast 的场景应调用 eager load 或 lazy facade 的 `preloadAll()`。

Foreign key relation helper 与 lazy load 的交互：

- 若 source row 所在 table 已加载，但 relation target table 尚未加载，relation helper 首次解析时必须先触发 target table 加载，再执行 target index lookup。
- `link_all` / `LinkAll` / `linkAll` 在 lazy facade 上应等价于 `preloadAll()` 后对所有可见 relation 执行预链接；它会加载所有可见 table，因此不再是按需行为。
- TypeScript 的 lazy table access 通常是 async，因此 lazy relation helper 也需要 async 形态，例如 `await row.defaultClassLazy(data)` 或由 `LazyGameData.resolveDefaultClass(row)` 承载；现有 eager relation helper 保持同步。
- Rust / C++ 可以在 accessor 中同步打开并解析 table，因此 lazy relation helper 可以保持同步 `Result` / exception-return 形态；但最好通过 generated data-access trait / facade interface 让 eager 与 lazy 共享 relation resolution 代码。

推荐语言 API 草案：

| 语言 | Lazy facade | Table accessor | Eager 兼容 |
| --- | --- | --- | --- |
| Rust | `LazyGameData<L: RuntimeDataLoader>` 持有 `L` 或 `&L`；每个 table 用 `OnceCell<Table>` 缓存 | `fn items(&self) -> RuntimeDataResult<&ItemsTable>`；`fn preload_all(&self) -> RuntimeDataResult<()>` | `GameData::load(&loader)` 保持一次性加载；也可让 `GameData` 与 `LazyGameData` 共同实现 generated `GameDataAccess` trait |
| C++ | `LazyGameData` 持有 `std::shared_ptr<IRuntimeDataLoader>` 或明确生命周期的引用 wrapper；table cache 用 `std::optional<Table>` | `const ItemsTable& Items() const`，失败时 throw 或返回 `RuntimeDataResult<std::reference_wrapper<const ItemsTable>>`；`PreloadAll()` | `GameData::Load(loader)` 保持 eager；`LazyGameData::PreloadAll()` 后可选 `ReleaseLoader()` |
| TypeScript | `LazyGameData` 持有 `RuntimeDataLoader`，每个 table cache 保存 `Promise<Table>` 或 loaded table | `items(): Promise<ItemsTable>` / `gameRules(): Promise<GameRulesRow>`；并发访问复用同一个 Promise | `await GameData.load(loader)` 保持 eager；`await LazyGameData.loadAll(loader)` 可返回 eager `GameData` 或加载完的 lazy facade |

命名上应避免让 lazy facade 的 async / fallible accessor 伪装成 eager field。现有 eager `data.items.getById(...)` 可继续保留；lazy TypeScript 不应生成 `data.items` 这种看似同步的属性，而应生成 `await data.items()` 或 `await data.getItems()`。

#### 6.2.1.2 生成的 Foreign Key Relation Helper

当 foreign key 声明了 `linked_field` 时，代码生成器应在不改变 runtime data shape 的前提下生成关联访问 helper。原始 source field 仍作为普通字段暴露，例如 `default_class_id` 仍是整数；`linked_field = "default_class"` 只额外生成一个只读 relation getter，用于从当前 row/object 的 FK key 自动跳转到目标 CollectionTable 的唯一索引行。

生成 API 的核心语义：

1. **懒解析**：首次访问 helper 时，根据 source fields 当前值组装 key，并调用目标 table 的 unique / primary index lookup。
2. **缓存 row index**：lookup 成功后，只把目标行在 target table rows 中的下标缓存到 source row/object 自身；不缓存目标对象引用、指针或闭包，避免 Rust 生命周期、自引用、C++ 指针失效和 TypeScript 循环引用问题。
3. **再次访问不查索引**：后续访问 helper 时，直接使用缓存 row index 从 target table rows 中取只读 row/object，不再执行 hash/map lookup。
4. **空关系可缓存**：`allow_empty = true` 且 source key 全空时，helper 返回空值并缓存“已解析为空”，避免每次访问都重新判断和查找。
5. **运行时防御**：若非空 key 查不到目标行，说明 data/code 版本不匹配、数据被绕过 validation 或手工篡改；helper 应返回错误 / throw，而不是静默当作 optional empty。

缓存状态必须至少区分三种：

| 状态 | 语义 |
| --- | --- |
| unresolved | 尚未访问，也未被 `link` 预解析。 |
| resolved empty | 合法空关系，例如 optional FK 全空。 |
| resolved index | 已解析到目标 table row index。 |

推荐语言 API 形态：

| 语言 | 单值 FK relation | 可空 FK relation | list FK relation |
| --- | --- | --- | --- |
| Rust | `row.default_class(&data) -> Result<&ClassRow, RuntimeDataError>` | `Result<Option<&ClassRow>, RuntimeDataError>` | `Result<Vec<&ClassRow>, RuntimeDataError>` 或 iterator facade |
| C++ | `row.GetDefaultClass(data) -> const ClassRow&` 或 error-return 版本 | `const ClassRow*` / `std::optional<std::reference_wrapper<const ClassRow>>` | `std::vector<const ClassRow*>` |
| TypeScript | `row.defaultClass(data): ClassRow` 或 `throws` | `ClassRow | undefined` | `readonly ClassRow[]` |

Rust 需要内部可变性才能让 `&self` getter 填充 cache；单值 cache 应使用 `Cell<LinkCache>` 这类 inline copy 状态，避免每次访问产生 borrow tracking；list cache 因需要保存 `Vec<usize>`，可使用 `RefCell<LinkListCache>` 延迟分配 row-index vector。为了业务访问便利，推荐生成只读 getter + 内部 cache，例如：

```rust
pub struct GameRulesRow {
    pub default_class_id: i64,
    default_class_link: std::cell::Cell<LinkCache>,
}

impl GameRulesRow {
    pub fn default_class<'a>(&self, data: &'a GameData) -> Result<&'a ClassRow, RuntimeDataError>;
}
```

C++ row 可以把 cache 字段声明为 `mutable`，保证 relation getter 是 `const` 方法：

```cpp
struct GameRulesRow {
    std::int64_t default_class_id;
    mutable LinkCache default_class_link;

    const ClassRow& GetDefaultClass(const GameData& data) const;
};
```

TypeScript 可以在 row class 或 row wrapper 中保存私有 cache 字段；若当前 TS 生成的是 plain interface，应为启用 `linked_field` 的 row 生成 class / wrapper，或把 helper 放在 table facade 中并用 side-table cache。为了让 cache “缓存到自己身上”，推荐 row class：

```ts
export class GameRulesRow {
  readonly defaultClassId: number;
  private defaultClassLink: LinkCache = LINK_UNRESOLVED;

  defaultClass(data: GameData): ClassRow {
    // first access resolves by ClassTable index; later access reads cached row index
  }
}
```

TypeScript 若继续使用 plain object row，应避免为每个 row 分配 relation closure 或 cache object：relation method 可以复用模块级函数引用，单值 cache 用 number sentinel（例如 unresolved = -1、empty = -2、row index >= 0），list cache 仅在首次解析时分配 `number[]`。

每个包含 `linked_field` 的 table repository 应生成 `link(data)`，整个 DataSet facade 应生成 `linkAll()` / `link_all()` / `LinkAll()`：

| 层级 | API 语义 |
| --- | --- |
| Row/Object | 可选生成 `link(data)`，只预解析当前 row/object 的所有 `linked_field`。 |
| Table repository | `link(data)` 遍历本 table 所有 rows 或 singleton object，预解析本 table 可见的所有 `linked_field`。 |
| DataSet facade | `link_all()` / `LinkAll()` / `linkAll()` 按确定性 table 顺序调用每个 table 的 `link`。 |

`link_all` 面向游戏服务器、长生命周期客户端等“启动时加载全部配置，运行中高频访问”的场景；调用后 relation getter 不应再执行索引查询，只做 cache 状态检查和 row index 取值。`link` / `link_all` 必须幂等，多次调用不能改变结果。若 source table 与 target table 之间存在环状 FK，缓存 row index 而不是引用可以避免对象图循环；`link_all` 只填充 index cache，不需要按拓扑排序构造对象。

可见性规则与 CodeModel 一致：只有当前 target 可见的 FK 才能生成 helper 和 cache 字段。若 FK 不可见、source fields 不可见、target table 不可见或 target index 不可见，当前 target 的生成代码中不得出现该 `linked_field` API。`linked_field` 不进入 JSON / MessagePack / bin payload，也不影响 binary schema revision；它只影响 generated code 的类型、helper、cache 与 link API。

#### 6.2.1.3 TypeScript Sync Loading

TypeScript generated code 同时支持 async 与 sync loader 场景：

- `RuntimeDataLoader.open(path)` 可以返回 `RuntimeDataReader` 或 `Promise<RuntimeDataReader>`。
- `GameData.load(loader)` 总是 async，适合文件系统、fetch、资源包异步初始化等场景。
- `GameData.loadSync(loader)` 要求 loader 同步返回 reader；如果返回 Promise，generated helper 必须抛错，避免业务在数据尚未加载完成时继续执行。
- `LazyGameData` 同时提供 async table accessor 与 `xxxSync()` accessor。
- `preloadAllSync()` / `linkAllSync()` 是同步 loader 场景的便捷 API；如果业务 loader 可能异步，应使用 async 版本。

#### 6.2.1.4 生成符号冲突范围

用户可见 generated names 都参与冲突检查，包括字段、fragment、index lookup、relation helper、text helper、`GameData`、`LazyGameData`、`GameDataAccess` 等。`linked_field` 不仅不能和原始字段重名，也不能和目标语言生成后的 helper 名称冲突。`keyword_conflict` 只处理保留字或关键字冲突，不保证所有业务语义命名冲突都能自动修复。

#### 6.2.1.5 Binary schema revision

`bin` schema revision 由 exporter 与 codegen 共享 table shape 计算。影响 revision 的是 target-visible table kind、fields、fragments 与 wire layout；`linked_field` relation helper、text helper、注释和输出路径不进入 runtime payload，也不改变 binary schema revision。Reader 提供 revision 时 generated code 必须校验；若某格式不提供 revision，则按对应 reader contract 跳过或由格式 reader 处理。

### 6.2.2 标准 RuntimeDataReader 实现包

ConfigForge 应提供一组标准化的 RuntimeDataReader / RuntimeDataLoader adapter，覆盖主流目标语言与 runtime data export type 的组合。目标不是让生成代码强依赖这些 adapter，而是提供可直接复用、可拷贝修改、可作为测试 harness 基础实现的标准参考实现。

设计原则：

1. **生成代码核心仍保持无第三方 runtime 依赖**：Rust / C++ / TypeScript generated code 只声明 reader / loader 契约和 generated parser，不直接依赖 `serde_json`、`rmp-serde`、`RapidJSON`、`msgpack-c`、npm MessagePack 包等。
2. **标准 adapter 按官方支持矩阵拆分**：stable runtime data export type 应覆盖官方 target language；experimental type 可以先只覆盖部分语言。每个 adapter 单元只负责“某语言中把某格式的通用 value/cursor 暴露为 RuntimeDataReader”，transport loader 作为可组合层，不进入语言 × 格式矩阵。
3. **资源加载与格式解码分层**：`RuntimeDataLoader` 的文件系统、HTTP、引擎资源系统等 transport 逻辑应与 JSON / MessagePack reader 解码逻辑分开。标准包可以提供常用组合（如 Node fs + JSON、Rust fs + MessagePack），但底层 reader 应可被自定义 loader 复用。
4. **依赖显式可选**：不同语言/格式 adapter 允许依赖不同三方库，但依赖必须限制在对应 adapter 包/模块内，并通过 Cargo feature、npm optional dependency、CMake option / package manager 选项等机制显式启用。
5. **测试与项目复用同源**：集成测试中的 harness 应优先使用这些标准 adapter 或其 fixture 包装，而不是在 harness 中长期内嵌一份独立 JSON / MessagePack reader 逻辑；这样测试能持续验证真实接入方会使用的 reader contract。

标准 adapter 必须声明 reader contract 版本与支持范围。当前 contract 版本为 `1.0`，统一规定 ConfigForge Runtime Value、missing/null、scalar conversion 和错误上下文语义；Rust / TypeScript / C++ adapter 应通过共享 compliance suite 验证这些行为一致。Rust 当前 adapter 由于 trait 由 generated crate 声明，短期定位为可复制/可 include 的 `copyable_template`；等稳定 `config_forge_runtime` crate 落地后再升级为 standalone package。

首批格式矩阵建议：

| 语言 | JSON adapter | MessagePack adapter | 依赖策略 |
| --- | --- | --- | --- |
| Rust | `serde_json` reader module；`serde_json_reader` | `rmp-serde` reader module；`serde_msgpack_reader` | Cargo features：`json`、`messagepack`、`fs`；当前为 `copyable_template`，各格式 reader 自带 value adapter 与 fs loader。 |
| TypeScript | 原生 `JSON.parse` value reader | `@msgpack/msgpack` 解码后的 value reader | npm 包拆分与 peer dependency；Node fs / fetch loader 与格式 reader 分离。 |
| C++ | 生产推荐 `RapidJSON` reader；`rapidjson_reader` 为生产推荐 JSON adapter | 推荐 `msgpack-cxx` reader；`msgpackc_reader` 解码到 msgpack object/value 后适配 | CMake options：`CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER`、`CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER`；通过 `find_package` / package manager / 显式 `FetchContent` 引入三方库，不 vendored 到 generated code。 |

标准 adapter 的产物形式按语言习惯决定：

reader 目录和包名应优先表达“具体三方读取库”，而不是只表达文件格式。默认规则是 `<third_party_library>_reader`，例如 `rapidjson_reader`、`msgpackc_reader`、`serde_json_reader`；如果三方库名称本身缺少明确文件格式，再使用 `<third_party_library>_<format>_reader`。这样做是因为同一种语言下同一格式通常有多个三方库可选，目标用户也可能已经在项目中使用另一套 JSON / MessagePack 库。Rust 中 `rmp` 本身是 Rust MessagePack 的缩写，结合当前实现依赖 `rmp-serde` / serde value，因此目录命名为 `serde_msgpack_reader`。

- **Rust**：由于当前 `RuntimeDataReader` trait 由 generated crate 声明，外部 crate 无法在不依赖用户 generated crate 的情况下“一次性”为 adapter 类型实现该 trait。短期应提供可复制/可 include 的 Rust reader module，放入 generated crate 或业务 crate 后实现本地 trait；具体 value reader 应放在对应格式 reader 中，例如 `serde_json_reader` 的 `JsonValueReader` 和 `serde_msgpack_reader` 的 `MessagePackValueReader`，避免把 `serde_json::Value` 伪装成通用 core。中期可评估把 reader trait 抽到稳定 runtime support crate，让 generated code 选择 `use config_forge_runtime::RuntimeDataReader`，从而使 adapter crate 可以直接实现 trait。
- **TypeScript**：TypeScript 接口是结构化类型，标准 npm 包可以直接导出符合 `RuntimeDataReader` 形状的 classes/functions；generated code 无需 import adapter，业务侧把 adapter instance 传给 `GameData.load` 即可。
- **C++**：`IRuntimeDataReader` 是 nominal virtual interface，adapter 需要与 generated header 一起编译。短期可提供 header/source template 或 header-only adapter，要求业务工程配置 generated header include path；中期可评估拆出稳定 runtime interface header，使 adapter 库不依赖某次 generated header。

推荐仓库目录为顶层 `runtime/`，例如：

```text
runtime/
  compliance/            # 跨语言 reader contract payload / expected cases
  rust/
    serde_json_reader/    # serde_json reader + 可选 fs loader
    serde_msgpack_reader/ # rmp-serde reader + 可选 fs loader
  typescript/
    reader_core/          # shared reader helpers / error helpers
    json_reader/          # JSON.parse value reader
    messagepack_reader/   # @msgpack/msgpack value reader
    loaders/              # node-fs、fetch 等 transport loader
  cpp/
    rapidjson_reader/     # RapidJSON reader；生产推荐 JSON adapter
    msgpackc_reader/      # msgpack-cxx reader；生产推荐 MessagePack adapter
    loaders/              # filesystem / engine-resource 示例 loader
    CMakeLists.txt        # adapter targets、options、find_package、tests
```

`runtime/` 下的实现属于“标准 runtime adapter 源码/模板”，不是每次 codegen 的输出文件。后续可以通过 codegen option 选择是否把某个 adapter 拷贝到目标输出目录，也可以由游戏项目直接依赖该目录对应语言的 package/crate/library。

C++ adapter 需要有独立工程化入口。推荐支持 `add_subdirectory(runtime/cpp)` 和安装后的 `find_package(ConfigForgeRuntime CONFIG)` 两种接入方式，并暴露 `config_forge_runtime::rapidjson_reader`、`config_forge_runtime::msgpackc_reader`、`config_forge_runtime::filesystem_loader` 等 CMake targets。当前短期方案不提供单独的 `core` target，adapter 直接依赖 generated header 中的 `IRuntimeDataReader` / `IRuntimeDataLoader` contract；后续如拆出稳定 runtime interface header，再重新评估 stable interface target。生产项目显式打开 RapidJSON / msgpack-cxx 选项；测试环境如果缺少 RapidJSON 或 msgpack-cxx headers 可跳过对应 adapter smoke。详细 CMake option、target、三方依赖解析、C++ ownership、runtime_reader option 校验、emit 语义和 CI 矩阵见 [`../Implementation/09e-codegen-runtime-adapters.md`](../Implementation/09e-codegen-runtime-adapters.md) 与 [`../Tech/cpp-runtime-dependencies.md`](../Tech/cpp-runtime-dependencies.md)。

## 6.3 非运行时数据导出格式

POT、Asset Manifest 等辅助产物不再引入独立的 `targets.special_exporters` 配置层，而是明确作为 `[[targets.exports]]` 的一种 `type`。这样 TOML 结构保持简单：`target.exports` 统一表示“当前 target 要生成的产物”，由 exporter dependency contract 区分 MetaOnly 与 MetaAndData。

非运行时 data exporter 与 runtime data exporter 的定位不同：runtime data 面向游戏运行时加载；POT、资源清单、审计索引等面向本地化、资源打包、工具链或文档流程。它们共享同一个 Meta / Data / ExportView 语义，但非运行时 data exporter 可以读取被 runtime payload 隐藏、同时又被 exporter policy 显式允许的辅助字段，例如 text translator comment 或 asset bundle tag。无论是否读取辅助字段，都必须遵守 target tags、table/field visibility、`row_export` 和 artifact path conflict 规则。

目标态配置继续使用 target 内的 `[[targets.exports]]` 数组表。示例：

```toml
[[targets]]
name = "client_i18n"
tags = ["client"]

[[targets.exports]]
name = "pot"
output = "Generated/I18n"
type = "gettext_pot"
domain = "game_config"
file_name = "game_config.pot"
source_reference = "source_location"
include_translator_comments = true
deduplicate = true
```

通用字段沿用 `targets.exports`：

| 字段 | 说明 |
| --- | --- |
| `name` | 当前 target 内唯一的 export task 名称，用于诊断和 artifact key。 |
| `output` | 数据产物输出目录。聚合格式如果只产生一个文件，应通过格式选项或默认规则确定文件名。 |
| `type` | exporter 类型 ID，例如 `gettext_pot`、`asset_manifest_csv`。禁止使用模糊 `custom`。 |
| exporter 专属字段 | 直接摊平在当前 `[[targets.exports]]` 下，例如 `domain`、`file_name`、`source_reference`。 |

所有 export task 产物，无论是 runtime table data、POT / manifest 还是 codegen，都必须一起进入 artifact plan，最终由 Artifact Writer 统一检查路径冲突并写入。当前实现的冲突检查是 lexical path level：会折叠 `.` / `..`，但不会解析 symlink，也不会按大小写不敏感文件系统折叠 path key；平台增强策略见 Implementation / Tech 文档。

### gettext POT

`gettext_pot` 是面向 `text` 字段的国际化模板导出器。`text` 在 Meta 中表示“国际化文本逻辑值”，source cell 第一阶段默认存储待翻译原文；POT exporter 从当前 target 的 ExportView 中抽取所有可见 `text` 值，生成 gettext 兼容的 `.pot` 模板，供后续 `msginit` / 翻译平台 / PO 编辑器生成各语言 `.po`。

输入：ExportView 中未被过滤的 `text` 字段，以及 exporter policy 明确允许的 text 辅助元数据。

输出：`.pot`。

应包含：

- `msgid`：默认等于 source cell 原文。空字符串默认不导出；required 校验仍由 validation 处理。
- optional `msgctxt`：用于区分相同原文在不同 UI 语境下的翻译。
- source reference：gettext `#:` 行，定位到源表、row、column 或 logical record。
- table / field / record key：可作为 extracted comment 或 reference 附加信息，便于翻译系统回写和排查。
- translator comments：gettext `#.` 行，用于说明语气、变量含义、UI 场景、长度限制等。
- flags：例如后续支持插值占位符时可写 `#, c-format` / 自定义 flag；第一阶段默认不生成格式 flag。

规则：

- source reference 默认来自 source path 和 row 坐标；当前 CSV resolver 保留 source path / row，尚不保留列坐标，因此首版输出形如 `data/items.csv:2`。`source_reference = "record_key"` 当前不生成 gettext `#:` 行，而是将 record key 写入 translator comment（例如 `record=id=2001`）；`both` 当前只合并 source location，record key 仍作为 translator comment 输出。若 ObjectTable key-value layout 无自然列坐标，则使用 logical field path 或 record key comment 作为补充定位信息。
- `msgid` 默认使用原文驱动模式，即 source cell 文本本身。未来可配置 key-driven 模式，此时 `text` cell 可表示稳定 key，而默认语言原文来自伴随列/外部 catalog。
- `msgctxt` 默认可由 `table.field_path` 派生；也可由 Meta 字段参数、target policy 或 source 中显式 context 列覆盖。不同 context 下相同 msgid 必须产生不同 POT entry。
- translator comments 可来自字段 `desc`、未来 text 字段声明、source 中显式映射的额外注释列/行、以及 exporter 生成的 table/field/record 描述；如果多个来源同时存在，应按 exporter policy 稳定合并并去重。
- 被 ExportView 过滤掉的 row / field / fragment 不产生 POT 条目，也不产生 translator comments。`export = false` 的字段默认不进入 runtime data；若未来字段策略显式允许 `type = "gettext_pot"` 读取某个辅助字段，POT exporter 可读取它，但仍必须遵守 target tags 与 row filter。
- dedup 规则必须稳定：`deduplicate = true` 时，相同 `(msgctxt, msgid)` 合并为一个 entry，source references 和 translator comments 按 table order、row order、field order 稳定追加；如果相同 `(msgctxt, msgid)` 的 comments 冲突，默认合并而不报错，严格模式可升级为 warning / error。`deduplicate = false` 时按抽取顺序原样输出 entries，调用方需自行处理 gettext 工具对重复 key 的行为。
- 输出编码固定 UTF-8；字符串必须按 gettext 规范转义双引号、反斜杠、换行和控制字符。多行 msgid 应使用标准多段字符串表示。
- header entry `msgid ""` 必须稳定生成，包含 `Project-Id-Version`、`Content-Type: text/plain; charset=UTF-8`、`Content-Transfer-Encoding: 8bit` 等基础字段；首版为 deterministic 输出，不写 `POT-Creation-Date`。若未来增加 generated_at/build timestamp，应遵循 deterministic / generated_at 策略。

推荐输出示例：

```po
msgid ""
msgstr ""
"Project-Id-Version: demo\\n"
"Content-Type: text/plain; charset=UTF-8\\n"

#. 物品在背包和详情界面展示的名称。
#. table=items field=name record=id=2001
#: data/items.csv:2:3
msgctxt "items.name"
msgid "Iron Ore"
msgstr ""
```

未来国际化扩展路径：

1. **原文驱动 text（当前目标）**：`text` cell 是默认语言原文，POT 使用它作为 `msgid`。runtime data 仍保留字符串，生成代码可提供翻译访问 helper。
2. **key-driven text**：字段增加 `text_key` / `text_context` 策略，runtime data 可存稳定 key，POT 从 key + default text 生成；适合原文频繁改动但不希望打断已有翻译的项目。
3. **富文本 / 带参数文本**：新增 `rich_text`、`format_text` 或 text option，记录占位符 schema、plural category、gender/select 等规则；POT exporter 生成 flags 和 placeholder comments，validation 检查占位符一致性。
4. **复数与变体**：支持 gettext plural `msgid_plural`、ICU MessageFormat 或引擎自定义 catalog。此阶段不应把所有能力塞入 primitive `text`，而应引入明确的 field type 或 field option。
5. **多 catalog/domain**：按 target、table、模块或字段策略输出多个 POT/domain，artifact plan 必须保证多文件路径稳定且不冲突。

生成代码影响：

- 第一阶段 `text` 在 runtime payload 中仍按 `string` 输出，生成代码字段类型保持 `String` / `std::string` / `string`，不破坏现有用户。
- 对 `text` 字段，生成器额外生成翻译 helper。默认 `generic` 模式下，例如 Rust row/struct 方法 `display_name_text(&translator)`、C++ row/struct 方法 `DisplayNameText(translator)`、TypeScript 顶层 helper `itemsDisplayNameText(row, translator)` / `structDisplayNameText(value, translator)`。`global_function` 模式下，helper 不再接受 translator 参数，而是直接调用配置的全局函数。helper 默认传入原始字符串和 `table.field` / `struct.field` context，由业务侧 translator 或全局函数返回当前语言文本。
- helper 不应把翻译结果缓存到 row 中，除非 translator 明确提供语言版本号；否则切换语言后会读到旧文本。推荐 translator 自己缓存。
- helper 的 lookup contract 应保持语言无关并与当前生成代码一致：输入为 `(msgctxt, msgid)`，返回 translated string 或可选 translated string。`msgctxt` 默认来自 `table.field` / `struct.field`，`msgid` 为 raw text；generic translator 或 `global_function` 返回 missing 时，应回退 raw text。table、field path、record key 等 source metadata 可作为未来扩展，不属于当前 helper contract。
- 若未来采用 key-driven text，生成代码可以把 raw field 视作 key，并通过同一 helper 访问 translated text；API 名称保持稳定，减少业务迁移成本。

### Asset Manifest CSV

Asset manifest 同样作为 `[[targets.exports]] type = "asset_manifest_csv"` 的聚合数据导出格式，而不是独立配置层。

输入：ExportView 中未被过滤的 asset 字段。

输出：CSV 或其他 manifest 格式。

推荐列：

```csv
asset_ref,asset_kind,table,field_path,record_key,source,tag
```

默认行为：

- 不包含被 context filter 排除的数据。
- 可配置是否去重。

## 6.4 Code Generation Comments

Meta 中的 `desc` 是代码生成注释的主要来源。Code generator 应按目标语言注释风格把 `desc` 输出到生成代码中。

推荐映射：

| Meta 层级 | 生成代码位置 |
| --- | --- |
| table desc | 生成的表类型、repository、访问类或模块注释。 |
| field desc | 生成的属性、字段、getter 或 schema 字段注释。 |
| enum desc | 生成的 enum 类型注释。 |
| enum value desc | 生成的 enum member 注释。 |
| named struct desc | 生成的 struct/interface/class 注释。 |
| struct field desc | 生成的成员字段注释。 |
| fragment desc | 生成的 fragment 类型或访问 API 注释。 |

要求：

- 代码生成器必须对 `desc` 做目标语言安全转义，避免破坏注释块或生成非法代码。
- 多行 `desc` 应保留语义，可按目标语言格式拆成多行注释。
- 如果目标语言不支持某种注释形式，也应尽量生成普通注释。
- `desc` 不应改变 runtime data 内容，只影响生成代码注释和辅助文档。

## 6.5 Artifact Model

Artifact 内容类型：

| Content | 说明 |
| --- | --- |
| Text | UTF-8 文本文件，例如 generated code、POT、CSV manifest。 |
| Binary | 二进制文件，例如 MessagePack runtime data、未来二进制 schema。 |

要求：

- 导出前发现路径冲突。
- 路径冲突检查当前使用 lexical-normalized artifact path；两个 export task 如果规划出同 output + 同 type + 同 table 文件名，必须报错。大小写不敏感 alias 与 symlink alias 属于中期平台增强范围。
- 产物写入通过 Artifact Writer。
- Artifact Writer 支持 `direct` 和 `temp-commit` 写入策略；CLI 默认 `direct`，可通过 `--write-mode temp-commit` 选择临时提交写入。
- 并行生成不影响 artifact write result 顺序。
- `temp-commit` 模式先完成所有临时文件写入，再按稳定顺序提交到最终路径；临时文件写入失败时不提交最终产物并清理已创建的临时文件，提交阶段失败可能保留已经 rename 成功的最终文件。
- 未来 summary manifest 可作为 `targets.exports` 的聚合导出格式产物；记录 runtime data artifact 时，应以 `(target_name, export_name, table_name)` 作为稳定 key，并额外记录 `type`、相对 data artifact root 的 `path` 和最终写入 path，避免同一 table 的多格式产物互相覆盖。POT / manifest 这类聚合 export task 应使用 `(target_name, export_name, type, artifact_name)` 作为稳定 key。

写入策略：

| 策略 | 说明 |
| --- | --- |
| `temp-commit` | 先写入同目录临时文件；所有临时文件成功写入后，按稳定顺序 rename 到最终路径。 |
| `direct` | 直接写入最终路径，适合本地调试或项目明确选择该行为的场景。 |

无论使用哪种策略，都必须在写入前完成 export plan 和路径冲突检查。

## 6.6 CLI 总体形式

当前 CLI 只保留构建一个 target 所需的最小参数：Project Config 路径和 target 名称。Meta、Data、输出目录和代码生成语言都从 target 配置中读取，不在命令行重复声明。

```bash
config-forge --project ConfigForge.toml --target client
```

参数：

| 参数 | 说明 |
| --- | --- |
| `--project <FILE>` | Project Config 路径。 |
| `--target <NAME>` | 要构建的 target 名称。 |
| `--dry-run` | 执行完整构建计划和 artifact 路径冲突检查，输出 artifact plan，但不创建目录、不写出文件。 |
| `--write-mode <direct\|temp-commit>` | 非 dry-run 构建使用的 artifact 写入模式；默认 `direct`。如果同时传入 `--dry-run`，以 dry-run 行为为准。 |
| `--artifact-memory-limit <SIZE>` | artifact 写入 pipeline 的固定内存预算，例如 `512MiB`。与 `--artifact-memory-percent` 互斥。 |
| `--artifact-memory-percent <PERCENT>` | 按启动时可用内存百分比计算 artifact pipeline 预算；默认 25%，范围 `1..=80`，并 clamp 到 `[64MiB, 2GiB]`。 |
| `--trace-tasks <FILE>` | 将本次 build 的轻量 tracing / metrics 以 JSON 写入指定文件，包含阶段名、开始时间、耗时、状态和执行资源参数；不改变 stdout 的稳定构建输出。 |

执行行为：

1. 解析 Project Config，并递归加载其顶层 `imports` 指向的工程配置片段。
2. 按 `--target` 找到目标 target。
3. 合并所有配置片段中的 `enums`、`structs`、`tables` 并完成 link/validate。
4. 根据 `[[targets.exports]]` 与 exporter dependency contract 构造 target-local execution view；当前实现按 MetaAndData / MetaOnly 两个分支执行，而不是完整通用 DAG 调度器。MetaAndData 分支读取 `project.input`、归一化、校验并运行数据相关 exporter，MetaOnly 分支运行格式无关 codegen。
5. 同 `(project.input, effective_tags)` 的 MetaAndData export task 复用加载、归一化和校验结果。
6. 汇总所有 export artifacts，按 export task 声明顺序稳定排序，并统一完成路径归一化和冲突检查。
7. 按 `--dry-run` / `--write-mode` 写出 artifact，或在 dry-run 模式下只返回写入计划；artifact writer 会按 memory budget 对 writer queue 中“已接收但未写出”的 artifact bytes 做限流。当前实现仍保留 `Vec<Artifact>` 收集边界，后续 streaming generator 会进一步降低生成阶段峰值。
8. 如果传入 `--trace-tasks`，在构建完成后写出 trace JSON，用于分析 Project load、input preparation、exporter、artifact write 等阶段耗时。
9. 未声明的任务不产生对应输出。例如 MetaOnly-only target 不读取 Data。

## 6.7 退出码

| Exit Code | 含义 |
| --- | --- |
| `0` | 成功，无 error。 |
| `1` | 构建、校验或导出失败。 |
| `2` | CLI 参数错误。 |
| `3` | Project/Meta 配置解析失败。 |
| `4` | Source 加载失败。 |

如果存在多个错误，退出码取最主要失败类别，但诊断中应保留全部错误。
