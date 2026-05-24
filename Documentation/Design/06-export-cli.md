# 06. 导出与 CLI 设计

## 6.1 Export Target

一个 target 描述一次逻辑导出。

字段：

| 字段 | 说明 |
| --- | --- |
| `name` | 稳定 target 名称。 |
| `data` | 可选 runtime data 导出任务列表。存在时才读取 source data，并由每个任务的 `format` 决定写出格式。 |
| `code` | 当前 target 下的一个或多个代码生成任务。每个任务有独立语言、输出目录、文件头和命名策略。 |

Project-level `name` / `desc` / `copyright` 可被 target 的代码生成器、schema exporter 或 manifest exporter 用于生成文件头。Target code task 可以通过 `file_header` 模板覆盖默认文件头。

路径解析语义：

- CLI `build --project path/to/ConfigForge.toml` 指定的 Project Config 文件所在目录是 project root。
- `targets.data.input`、`targets.data.output`、`targets.code.output` 的相对路径均相对声明它们的 Project Config 文件所在目录解析；入口文件中的相对路径因此等价于相对 project root。
- `imports` 引入的配置片段先按片段文件目录解析自身相对路径，再合并到总配置，避免片段移动到子目录后路径语义丢失。
- Meta 中 table `source`、`sources[].path`、`override_sources[].path` 不直接相对 `ConfigForge.toml`，而是相对当前 target data task 的 `input` 目录查找。

### 6.1.1 Runtime Data Export Task

`[[targets.data]]` 描述当前 target 是否导出 runtime data、从哪里读取 source data、写到哪里，以及使用哪种 runtime data format。一个 target 可以声明多套 data task，例如同时导出 JSON 供工具链调试、MessagePack 供客户端运行时加载：

```toml
[[targets]]
name = "client"

[[targets.data]]
name = "json"
input = "Data"
output = "Generated/Data/Json"
format = "json"

[[targets.data]]
name = "messagepack"
input = "Data"
output = "Generated/Data/MessagePack"
format = "messagepack"
```

字段：

| 字段 | 说明 |
| --- | --- |
| `name` | 当前 target 内唯一的 data task 名称。单 data task 可省略并由实现填入默认名；多 data task 必填。 |
| `input` | Source data 根目录。Meta source path 以该目录为搜索基础；只有声明 `targets.data` 时才读取。 |
| `output` | Runtime data 输出目录。 |
| `format` | Runtime data 格式。当前第一阶段支持 `json` 和 `messagepack`，默认 `json`。 |

导出规则：

- 每张 table 输出一个 runtime data 文件。
- `json` 使用 `.json` 后缀，`messagepack` 使用 `.msgpack` 后缀。
- 两种格式承载同一套逻辑 payload：table meta 摘要和 normalized rows；字段名使用 Meta canonical name。
- 同一 target 下多个 data task 如果 `input` 相同，执行层应只加载、归一化、校验一次，再分发给不同格式 exporter。
- 同一个 target 的 code generation 必须感知它引用的 runtime data task，生成的默认数据文件名必须与对应 data task 的 `format` 和真实 artifact 路径一致。
- code-only target 可以不声明 `targets.data`，但如果仍生成读取 runtime data 的代码，默认按 `json` 规划文件名；正式项目推荐 data 与 code 在同一个 target 中同时声明，避免格式不一致。
- 项目当前仍处于开发阶段，不保留旧 `[targets.data]` table 形态兼容；正式配置统一使用 `[[targets.data]]` 数组表。

### 6.1.2 Code Generation Task

一个 export target 可以生成多个代码产物，例如同一次导出同时生成 Rust、C++ 和 TypeScript 访问代码：

```toml
[[targets.code]]
language = "cpp"
output = "Generated/Cpp"
module_name = "GameConfig"
data = "messagepack"
namespace = "game::config"
file_header = "// {{project.name}}\n// {{project.copyright}}\n// Generated at {{datetime}}"
keyword_conflict = "suffix_underscore"

[targets.code.naming]
type = "pascal_case"
enum_value = "pascal_case"
field = "snake_case"
function = "pascal_case"
constant = "pascal_case"

[targets.code.cpp]
namespace = "game::config"
```

通用要求：

- `language` 决定使用哪个代码生成器。
- `output` 是该代码生成任务的根目录，不与 target runtime data output 混淆。
- `data` 指向当前 target 内的 runtime data task 名称。只有一个 data task 时可省略；多个 data task 时必须显式指定，避免默认文件名引用不确定。
- `file_header` 是文本模板，支持 `{{project.name}}`、`{{project.desc}}`、`{{project.copyright}}`、`{{datetime}}`。
- `keyword_conflict` 控制 Meta 名称转换成目标语言标识符后碰到关键字时如何处理。
- `naming` 控制 type、field、function、constant 等符号的大小写组织。
- 语言生成器可以声明额外配置，但必须放在该 code task 内对应语言子表中，不能污染其他语言任务；实现层应按语言使用 tagged enum / 多态配置承载这些差异。
- 生成代码必须暴露 runtime data 加载抽象接口，不直接绑定本地文件系统、引擎资源系统、网络或包内资源。

当前内置语言特定配置：

| 子表 | 字段 | 说明 |
| --- | --- | --- |
| `[targets.code.rust]` | `derive_serde` | 为生成类型派生 serde 序列化/反序列化 trait。 |
| `[targets.code.cpp]` | `namespace` | C++ namespace 覆盖；未设置时可使用通用 `namespace`。 |
| `[targets.code.typescript]` | `readonly_fields` | interface 字段是否生成 `readonly`。 |

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

## 6.2 Runtime Data Format

当前第一阶段支持：

| Format | 说明 |
| --- | --- |
| `json` | 默认格式，便于调试和通用接入。 |
| `messagepack` | 紧凑二进制格式，用于包体和加载性能更敏感的 runtime。 |

格式实现采用注册工厂模式：核心导出流程只根据 `format` 字符串从 `DataFormatRegistry` 中查找 `DataFormatExporter`，不在流程中通过枚举 `match` 分发。新增导出格式应通过注册新的 exporter 实现扩展，避免修改核心导出编排逻辑。内置注册器提供 `json` 与 `messagepack`。

别名策略：

- 配置中使用规范名 `messagepack`。
- CLI 诊断使用格式 ID；生成代码不再包含格式枚举。
- 如需兼容 `msgpack` 简写，可作为解析别名，但文档和生成产物统一输出 `messagepack`。

未来可扩展但当前不实现：

| Format | 说明 |
| --- | --- |
| `lua_table` | Lua 特殊模式，代码和数据可合一。 |
| `yaml` | 可选文本格式。 |
| `sqlite` | 可选数据库格式。 |
| `protobuf` | 未来设计，不作为当前实现要求。 |

不使用 `custom` 作为模糊格式。自定义格式必须声明明确 ID，例如 `studio_binary_v1`。

Protobuf 未来规则：

- 不使用 `stable_hash` 自动生成 field number 作为生产推荐策略。
- 如需要 Protobuf，Meta field 应支持显式 `field_number`。
- 删除/重命名字段需要 schema evolution 策略和 reserved number 设计。

### 6.2.1 生成代码的 Runtime Data 加载抽象

当前实现状态：Rust / C++ / TypeScript 生成代码已落地 **Meta-agnostic reader loader 契约**，即接入方只实现通用 `open(path)` 并返回 `RuntimeDataReader` / `IRuntimeDataReader`；生成代码内部负责按 Meta 读取表、字段、fragment 和 object payload，并构造 typed rows / objects。上一阶段按表生成 `load_<table>_rows` / `Load<Table>Rows` / `load<Table>Rows` 等方法的 typed loader 过渡契约已废弃，不再是当前生成代码 API。

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
2. **Positional / packed layout**：例如未来紧密排布的数组、二进制 record。生成代码按 Meta 中稳定的字段顺序调用 reader；exporter 与 codegen 必须使用同一字段顺序定义，reader 不需要知道字段名。

reader 的基础语义必须覆盖当前 Meta 类型：int、float、bool、string/text/asset、enum、object/array。enum 按 string 读取后由生成代码校验枚举值；`text` 和 `asset` 按 string 读取。字段缺失和字段存在但为 null 必须区分：required 字段缺失或 null 都由生成代码报错；optional field / optional fragment 缺失时视为 absent；null 是否等价 optional absent 由生成器策略统一定义，第一阶段建议只对 optional 视为 empty。当前 exporter 仍可能输出字符串 cell 时，string-to-int / string-to-bool / string-to-float 的兼容规则应由 generated parser helper 统一处理，避免每个业务 loader 重复实现。

因此业务 runtime 只需要长期维护一次通用 loader / reader 实现，可以选择：

- 用 JSON parser 实现通用 reader；
- 用 MessagePack decoder 实现通用 reader；
- 提供一个同时支持多种格式的 reader；
- 在引擎侧复用已有资源加载和反序列化 cursor。

格式选择由 code task 引用的 `targets.data` 任务决定，并影响导出的 table data file 后缀；生成代码保留 data file path，并按 data task format 生成对应的通用读取路径。若 target 同时导出 JSON 与 MessagePack，可以分别配置 TypeScript code task 引用 `data = "json"`、C++ code task 引用 `data = "messagepack"`。无论 Meta 新增多少表和字段，接入方的 RuntimeDataLoader 接口都不应变化；变化只体现在重新生成的解析代码中。

### 6.2.2 标准 RuntimeDataReader 实现包

ConfigForge 应提供一组标准化的 RuntimeDataReader / RuntimeDataLoader adapter，覆盖主流目标语言与 runtime data format 的组合。目标不是让生成代码强依赖这些 adapter，而是提供可直接复用、可拷贝修改、可作为测试 harness 基础实现的标准参考实现。

设计原则：

1. **生成代码核心仍保持无第三方 runtime 依赖**：Rust / C++ / TypeScript generated code 只声明 reader / loader 契约和 generated parser，不直接依赖 `serde_json`、`rmp-serde`、`RapidJSON`、`msgpack-c`、npm MessagePack 包等。
2. **标准 adapter 按官方支持矩阵拆分**：stable runtime data format 应覆盖官方 target language；experimental format 可以先只覆盖部分语言。每个 adapter 单元只负责“某语言中把某格式的通用 value/cursor 暴露为 RuntimeDataReader”，transport loader 作为可组合层，不进入语言 × 格式矩阵。
3. **资源加载与格式解码分层**：`RuntimeDataLoader` 的文件系统、HTTP、引擎资源系统等 transport 逻辑应与 JSON / MessagePack reader 解码逻辑分开。标准包可以提供常用组合（如 Node fs + JSON、Rust fs + MessagePack），但底层 reader 应可被自定义 loader 复用。
4. **依赖显式可选**：不同语言/格式 adapter 允许依赖不同三方库，但依赖必须限制在对应 adapter 包/模块内，并通过 Cargo feature、npm optional dependency、CMake option / package manager 选项等机制显式启用。
5. **测试与项目复用同源**：集成测试中的 harness 应优先使用这些标准 adapter 或其 fixture 包装，而不是在 harness 中长期内嵌一份独立 JSON / MessagePack reader 逻辑；这样测试能持续验证真实接入方会使用的 reader contract。

标准 adapter 必须声明 reader contract 版本与支持范围。当前 contract 版本为 `1.0`，统一规定 ConfigForge Runtime Value、missing/null、scalar conversion 和错误上下文语义；Rust / TypeScript / C++ adapter 应通过共享 compliance suite 验证这些行为一致。Rust 当前 adapter 由于 trait 由 generated crate 声明，短期定位为可复制/可 include 的 `copyable_template`；等稳定 `config_forge_runtime` crate 落地后再升级为 standalone package。

首批格式矩阵建议：

| 语言 | JSON adapter | MessagePack adapter | 依赖策略 |
| --- | --- | --- | --- |
| Rust | `serde_json::Value` reader template | `rmp_serde` template；中期收敛到 ConfigForge Runtime Value | Cargo features：`json`、`messagepack`、`fs`；当前为 `copyable_template`，默认只启用 core。 |
| TypeScript | 原生 `JSON.parse` value reader | `@msgpack/msgpack` 解码后的 value reader | npm 包拆分与 peer dependency；Node fs / fetch loader 与格式 reader 分离。 |
| C++ | 生产推荐 `RapidJSON` reader；`json_builtin` 仅作为无依赖 test fallback | 推荐 `msgpack-c` reader，解码到 msgpack object/value 后适配 | CMake options：`DATAEXPORT_RUNTIME_WITH_JSON_RAPIDJSON`、`DATAEXPORT_RUNTIME_WITH_MESSAGEPACK`；通过 `find_package` / package manager / 显式 `FetchContent` 引入三方库，不 vendored 到 generated code。 |

标准 adapter 的产物形式按语言习惯决定：

- **Rust**：由于当前 `RuntimeDataReader` trait 由 generated crate 声明，外部 crate 无法在不依赖用户 generated crate 的情况下“一次性”为 adapter 类型实现该 trait。短期应提供可复制/可 include 的 Rust module template，放入 generated crate 或业务 crate 后实现本地 trait；中期可评估把 reader trait 抽到稳定 runtime support crate，让 generated code 选择 `use config_forge_runtime::RuntimeDataReader`，从而使 adapter crate 可以直接实现 trait。
- **TypeScript**：TypeScript 接口是结构化类型，标准 npm 包可以直接导出符合 `RuntimeDataReader` 形状的 classes/functions；generated code 无需 import adapter，业务侧把 adapter instance 传给 `GameData.load` 即可。
- **C++**：`IRuntimeDataReader` 是 nominal virtual interface，adapter 需要与 generated header 一起编译。短期可提供 header/source template 或 header-only adapter，要求业务工程配置 generated header include path；中期可评估拆出稳定 runtime interface header，使 adapter 库不依赖某次 generated header。

推荐仓库目录为顶层 `runtime_readers/`，例如：

```text
runtime_readers/
  compliance/            # 跨语言 reader contract payload / expected cases
  rust/
    core/                 # trait glue/template、错误工具、测试 fixture glue
    json_serde/           # serde_json reader + 可选 fs loader
    messagepack_rmp/      # rmp-serde reader + 可选 fs loader
  typescript/
    core/                 # shared reader helpers / error helpers
    json/                 # JSON.parse value reader
    messagepack/          # @msgpack/msgpack value reader
    loaders/              # node-fs、fetch 等 transport loader
  cpp/
    core/                 # adapter glue、错误工具、CMake helpers
    json_builtin/         # 无依赖 minimal fallback；测试/示例用途
    json_rapidjson/       # RapidJSON reader；生产推荐 JSON adapter
    messagepack_msgpack_cpp/ # msgpack-c reader；生产推荐 MessagePack adapter
    loaders/              # filesystem / engine-resource 示例 loader
    CMakeLists.txt        # adapter targets、options、find_package、tests
```

`runtime_readers/` 下的实现属于“标准 runtime adapter 源码/模板”，不是每次 codegen 的输出文件。后续可以通过 codegen option 选择是否把某个 adapter 拷贝到目标输出目录，也可以由游戏项目直接依赖该目录对应语言的 package/crate/library。

C++ adapter 需要有独立工程化入口。推荐支持 `add_subdirectory(runtime_readers/cpp)` 和安装后的 `find_package(ConfigForgeRuntimeReaders CONFIG)` 两种接入方式，并暴露 `config_forge_runtime::core`、`config_forge_runtime::json_builtin_test_fallback`、`config_forge_runtime::json_rapidjson`、`config_forge_runtime::messagepack_msgpack_cpp`、`config_forge_runtime::filesystem_loader` 等 CMake targets。普通业务构建默认不启用 builtin fallback，测试构建可显式启用；生产项目显式打开 RapidJSON / msgpack-c 选项。详细 CMake option、target、三方依赖解析、C++ ownership、runtime_reader option 校验、emit 语义和 CI 矩阵见 `Documentation/Implementation/08-codegen-module-detail-design.md` 的“标准 RuntimeDataReader adapter 工程结构”。

## 6.3 Special Exporter

Special Exporter 当前属于 Planned / Not implemented。当前 `TargetConfig` 尚不支持 `special_exporters` 字段，正式可运行配置不应声明该字段；以下 POT 和 Asset Manifest 是目标态设计，后续应纳入统一 ArtifactPlan 后再实现。

### gettext POT

输入：ExportView 中未被过滤的 text 字段。

输出：`.pot`。

应包含：

- msgid；
- optional msgctxt；
- source reference；
- table / field / record key；
- translator comments。

规则：

- source reference 默认来自 source 表名和 row/column 坐标。
- translator comments 可来自 text 字段声明、`desc`，或 source 中显式映射的额外注释列/行；如果多个来源同时存在，应按 exporter policy 稳定合并。
- 被 ExportView 过滤掉的 text 不产生 POT 条目，也不产生 translator comments。

### Asset Manifest CSV

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
- `desc` 不应改变 runtime data 内容，只影响生成代码、辅助文档和 inspect 输出。

## 6.5 Artifact Model

Artifact 类型：

| Kind | 说明 |
| --- | --- |
| RuntimeData | 运行时数据。 |
| RuntimeSchema | schema，例如未来 proto。 |
| GeneratedCode | 生成代码。 |
| LocalizationTemplate | POT。 |
| AssetManifest | 资源清单。 |
| Manifest | 本次导出的产物列表。 |

要求：

- 导出前发现路径冲突。
- 路径冲突检查使用规范化绝对 artifact path；两个 data task 如果规划出同 output + 同 format + 同 table 文件名，必须报错。
- 产物写入通过 Artifact Writer。
- Artifact Writer 支持 `temp_commit` 和 `direct` 写入策略；推荐默认 `temp_commit`。
- 并行生成不影响 manifest 顺序。
- 失败 target 不应提交部分产物。
- 未来 manifest 记录 runtime data artifact 时，应以 `(target_name, data_name, table_name)` 作为稳定 key，并额外记录 `format`、相对 data artifact root 的 `path` 和最终写入 path，避免同一 table 的多格式产物互相覆盖。

写入策略：

| 策略 | 说明 |
| --- | --- |
| `temp_commit` | 先写入临时文件或临时目录，所有 artifact 成功后再确定性提交到最终路径。 |
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

执行行为：

1. 解析 Project Config，并递归加载其顶层 `imports` 指向的工程配置片段。
2. 按 `--target` 找到目标 target。
3. 合并所有配置片段中的 `enums`、`structs`、`tables` 并完成 link/validate。
4. 如果 target 声明 `[[targets.data]]`，再按 data task 读取 Data source、归一化、校验并写出 runtime data；同 input 的多格式 data task 可复用加载结果，不同 input 必须分别派生 DataSet-dependent ExportView。
5. 如果 target 声明 `[[targets.code]]`，按每个 code task 的语言、输出目录和绑定 data task 生成代码。
6. 未声明的任务不读取对应输入、不产生对应输出。例如 code-only target 不读取 Data。

## 6.7 退出码

| Exit Code | 含义 |
| --- | --- |
| `0` | 成功，无 error。 |
| `1` | 构建、校验或导出失败。 |
| `2` | CLI 参数错误。 |
| `3` | Project/Meta 配置解析失败。 |
| `4` | Source 加载失败。 |

如果存在多个错误，退出码取最主要失败类别，但诊断中应保留全部错误。
