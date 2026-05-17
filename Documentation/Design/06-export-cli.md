# 06. 导出与 CLI 设计

## 6.1 Export Target

一个 target 描述一次逻辑导出。

字段：

| 字段 | 说明 |
| --- | --- |
| `name` | 稳定 target 名称。 |
| `data` | 可选 runtime data 导出任务。存在时才读取 source data，并由 `format` 决定写出格式。 |
| `code` | 当前 target 下的一个或多个代码生成任务。每个任务有独立语言、输出目录、文件头和命名策略。 |

Project-level `name` / `desc` / `copyright` 可被 target 的代码生成器、schema exporter 或 manifest exporter 用于生成文件头。Target code task 可以通过 `file_header` 模板覆盖默认文件头。

### 6.1.1 Runtime Data Export Task

`[targets.data]` 描述当前 target 是否导出 runtime data、从哪里读取 source data、写到哪里，以及使用哪种 runtime data format：

```toml
[[targets]]
name = "client"

[targets.data]
input = "Data"
output = "Generated/Data"
format = "messagepack"
```

字段：

| 字段 | 说明 |
| --- | --- |
| `input` | Source data 根目录。只有声明 `[targets.data]` 时才读取。 |
| `output` | Runtime data 输出目录。 |
| `format` | Runtime data 格式。当前第一阶段支持 `json` 和 `messagepack`，默认 `json`。 |

导出规则：

- 每张 table 输出一个 runtime data 文件。
- `json` 使用 `.json` 后缀，`messagepack` 使用 `.msgpack` 后缀。
- 两种格式承载同一套逻辑 payload：table meta 摘要和 normalized rows；字段名使用 Meta canonical name。
- 同一个 target 的 code generation 必须感知该 target 的 runtime data format，生成的默认数据文件名、加载接口和格式常量必须与 `[targets.data].format` 一致。
- code-only target 可以不声明 `[targets.data]`，但如果仍生成读取 runtime data 的代码，默认按 `json` 规划文件名；正式项目推荐 data 与 code 在同一个 target 中同时声明，避免格式不一致。

### 6.1.2 Code Generation Task

一个 export target 可以生成多个代码产物，例如同一次导出同时生成 Rust、C++ 和 TypeScript 访问代码：

```toml
[[targets.code]]
language = "cpp"
output = "Generated/Cpp"
module_name = "GameConfig"
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

生成代码必须把“如何取得配置文件”和“如何解析配置格式”接口化，避免把 runtime 绑定到 exporter 所在机器的文件系统或某个具体引擎 API。

推荐抽象为一层：**RuntimeDataLoader**。生成代码只声明每张表所需的 typed row 加载方法，具体 runtime 自行决定如何读取文件、如何识别格式、如何解码成 row 列表。

语言 API 形态可以按语言习惯调整，但语义应一致：

| 语言 | Runtime 加载接口 |
| --- | --- |
| Rust | `RuntimeDataLoader::load_<table>_rows(path) -> Result<Vec<TableRow>, String>` |
| C++ | `IRuntimeDataLoader::Load<Table>Rows(path) -> std::vector<TableRow>` |
| TypeScript | `RuntimeDataLoader.load<Table>Rows(path) -> Promise<TableRow[]> | TableRow[]` |

生成代码中的 repository / `GameData::load` 不直接打开文件，也不感知数据格式，而是接收 runtime loader。业务 runtime 可以选择：

- 提供 JSON decoder；
- 提供 MessagePack decoder；
- 提供一个同时支持多种格式的 loader；
- 在引擎侧复用已有资源加载和反序列化系统。

格式选择由 target 的 `[targets.data].format` 决定，并影响导出的 table data file 后缀；生成代码只保留 data file path，不包含格式常量。

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
- 产物写入通过 Artifact Writer。
- Artifact Writer 支持 `temp_commit` 和 `direct` 写入策略；推荐默认 `temp_commit`。
- 并行生成不影响 manifest 顺序。
- 失败 target 不应提交部分产物。

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
4. 如果 target 声明 `[targets.data]`，再读取 Data source、归一化、校验并写出 runtime data。
5. 如果 target 声明 `[[targets.code]]`，按每个 code task 的语言和输出目录生成代码。
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
