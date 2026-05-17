# 06. 导出与 CLI 设计

## 6.1 Export Target

一个 target 描述一次逻辑导出。

字段：

| 字段 | 说明 |
| --- | --- |
| `name` | 稳定 target 名称。 |
| `language` | 代码生成语言，例如 `typescript`、`lua`、`csharp`、`cpp`。 |
| `engine` | 可选引擎，例如 `unity`、`unreal`。 |
| `data_format` | runtime data format，例如 `json`、`lua_table`。 |
| `tags` | target 默认导出上下文 tag。 |
| `output` | 输出目录或 artifact 路径。 |
| `type_mappings` | logical type 到 target type 的映射。 |
| `special_exporters` | POT、asset manifest 等辅助产物。 |
| `code` | 当前 target 下的一个或多个代码生成任务。每个任务有独立语言、输出目录、文件头和命名策略。 |

Project-level `name` / `desc` / `copyright` 可被 target 的代码生成器、schema exporter 或 manifest exporter 用于生成文件头。Target code task 可以通过 `file_header` 模板覆盖默认文件头。

### 6.1.1 Code Generation Task

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

当前推荐：

| Format | 说明 |
| --- | --- |
| `json` | 默认格式，便于调试和通用接入。 |
| `lua_table` | Lua 特殊模式，代码和数据可合一。 |
| `yaml` | 可选文本格式。 |
| `msgpack` | 可选紧凑二进制格式。 |
| `sqlite` | 可选数据库格式。 |
| `protobuf` | 未来设计，不作为当前实现要求。 |

不使用 `custom` 作为模糊格式。自定义格式必须声明明确 ID，例如 `studio_binary_v1`。

Protobuf 未来规则：

- 不使用 `stable_hash` 自动生成 field number 作为生产推荐策略。
- 如需要 Protobuf，Meta field 应支持显式 `field_number`。
- 删除/重命名字段需要 schema evolution 策略和 reserved number 设计。

## 6.3 Special Exporter

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

命令：

```bash
config-forge <command> [options]
```

全局参数：

| 参数 | 说明 |
| --- | --- |
| `--project <FILE>` | Project Config 路径，默认查找 `ConfigForge.toml`。 |
| `--root <DIR>` | 覆盖 project root。 |
| `--meta <FILE>` | 追加或覆盖 Meta root 文件。可重复。 |
| `--target <NAME>` | 选择 target。可重复。 |
| `--tag <TAG>` | 追加导出上下文 tag。可重复。 |
| `--context <A,B>` | 一次指定多个 tag。 |
| `--out <DIR>` | 覆盖输出目录。 |
| `--diagnostic-format <human|json|jsonl|summary>` | 诊断格式。 |
| `--fail-on-warning` | warning 也导致失败。 |
| `--plugin-path <DIR>` | 追加插件搜索路径。可重复。 |
| `--no-color` | 禁用终端颜色。 |
| `--verbose` | 输出更多调试信息。 |

## 6.7 `validate` 命令

用途：只读取、组合、过滤和校验，不写正式导出产物。

```bash
config-forge validate --project ConfigForge.toml
config-forge validate --project ConfigForge.toml --target client_lua_shipping
config-forge validate --meta Meta/items.toml --tag shipping --tag global
```

行为：

- 未指定 target/context 时，默认校验 Full DataSet。
- 指定 target/context 时，校验对应 ExportView。
- 可输出 human/json/jsonl 诊断。
- 不生成 runtime data，但可选择 `--dry-run-export-plan` 输出导出计划。

## 6.8 `export` 命令

用途：校验通过后生成运行时数据、代码和特殊产物。

```bash
config-forge export --project ConfigForge.toml --target client_lua_shipping
config-forge export --project ConfigForge.toml --target client_lua_shipping --out Build/Generated
```

行为：

1. 解析配置。
2. 加载插件。
3. 解析 Meta 和 source。
4. 生成 target ExportView。
5. 校验 ExportView。
6. 规划 artifact。
7. 写入产物和 manifest。

如果校验失败，不应写入最终产物。

## 6.9 `inspect` 命令

用途：帮助用户理解项目和 Meta。

子命令建议：

```bash
config-forge inspect meta --project ConfigForge.toml
config-forge inspect table items --project ConfigForge.toml
config-forge inspect target client_lua_shipping --project ConfigForge.toml
config-forge inspect sources --project ConfigForge.toml
```

输出：

- 合并后的 MetaModel 摘要；
- 表字段、索引、外键；
- table、field、enum、struct 等定义的 `desc`；
- source resolution；
- target tags 和 exporter；
- filter 后 record 数量统计。

## 6.10 `debug` 命令

用途：为工具开发和 CI 排障提供信息。

```bash
config-forge debug plugins --project ConfigForge.toml
config-forge debug export-plan --project ConfigForge.toml --target client_lua_shipping
config-forge debug diagnostics-schema
```

## 6.11 退出码

| Exit Code | 含义 |
| --- | --- |
| `0` | 成功，无 error。 |
| `1` | 校验或导出失败。 |
| `2` | CLI 参数错误。 |
| `3` | Project/Meta 配置解析失败。 |
| `4` | Source 加载失败。 |
| `5` | 插件加载失败。 |

如果存在多个错误，退出码取最主要失败类别，但诊断中应保留全部错误。
