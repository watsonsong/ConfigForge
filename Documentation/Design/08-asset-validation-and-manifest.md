# 08. Asset 引用校验、补齐与 Asset Manifest 初步设计

本文从现有 Data Validation / Export CLI 设计中拆出，专门收敛 `asset` 字段、资源存在性校验、target-aware 资源策略、资源引用自动补齐和 Asset Manifest 聚合导出的用户可见行为。当前实现已完成 asset validation / completion 闭环，并将 `type = "asset_manifest"` 作为与 JSON、MessagePack、bin、POT、Lua 同级的 data export task 接入，支持 deterministic JSON 和 Unreal DataTable 兼容 CSV。Rust 模块边界和当前实现细节见 `Documentation/Implementation/01-module-outline.md`、`03-project-module-detail-design.md`、`04-meta-module-detail-design.md`、`06-validation-module-detail-design.md` 和 `08-exporter-module-detail-design.md`。

## 8.1 设计目标

游戏配置中大量字段本质是资源引用，例如图标、prefab、blueprint、actor class、音效、技能特效、UI 资源、关卡资源、Spine / Live2D / Niagara / VFX Graph 等。ConfigForge 应在导出前尽早发现资源引用错误，并把目标包需要的资源列表稳定输出给客户端打包流水线。

目标能力：

1. 在 Meta 中用 `type = "asset"` + `asset_kind` 表达资源引用语义，而不是把所有资源路径都当普通 `string`。
2. 用 Project Config 声明可变的资源搜索路径，用 Meta / schema TOML 声明稳定的 `asset_kind` 语义、扩展名、输入格式、大小写策略和 target policy。
3. 对当前 target 的 ExportView 做资源引用校验：被 target tags / `export_when` / `row_export` 排除的数据不校验、不参与补齐。
4. 在导出 runtime data 时可按 `asset_kind` 把短写法或编辑器复制引用规范化为 canonical ref，确保 JSON / MessagePack / bin 看到同一份补齐后的字符串值。
5. 输出稳定排序的 Asset Manifest，供 Unity Addressables、Unreal Asset Manager、自研打包器、CDN 预热或资源审计系统消费；该 exporter 与其他 data exporter 同级，通过 `[[targets.exports]] type = "asset_manifest"` 启用。
6. 保持 generated core 与 runtime data 低耦合：`asset` 在 runtime payload 中第一阶段仍是字符串值；资源存在性、补齐和 manifest 是导出前工具链能力，不把引擎资源系统绑定进 generated code。

非目标：

- 不直接扫描 Unity / Unreal / 自研引擎的完整依赖图；ConfigForge 只验证配置显式引用的资源，并输出引用清单。
- 不负责资源导入、压缩、打包、热更差分或 CDN 上传。
- 不在第一阶段解析引擎专有 GUID / meta 文件；可作为后续 provider 扩展。
- 不让 `string` 字段隐式进入 Asset Manifest；必须显式声明 `type = "asset"`。

## 8.2 当前状态

当前实现已完成 P0-1 与 Asset Manifest exporter：`asset` 字段必须声明 `asset_kind`，Meta 支持顶层 `[[asset_kinds]]` 和 `extends` 继承；Project Config 支持 `[assets]` / `[[assets.search_paths]]`；runtime build 会对当前 target ExportView 中可见的 asset 字段执行 resolver 校验，并在导出 JSON / MessagePack / bin 等 runtime data 前把可补齐引用写成 canonical export ref。已实现的 resolver 覆盖 local filesystem search path、relative / mounted / mounted_object、extension policy、wrapper stripping、object name 模板、存在性、sidecar、命名前后缀、prefix / pattern / regex、target-gated deny 和基础大小写检查。`type = "asset_manifest"` 已作为聚合 data exporter 接入，读取同一 canonicalized DataSet，输出 deterministic JSON 或 Unreal DataTable 兼容 CSV。当前 validation 诊断 MVP 统一使用 `asset.reference` code 并携带 table / field / row / target 定位；更细粒度 asset diagnostic code、resolution cache 和 provider 扩展仍作为后续阶段单独推进。

现有文档中的已确认语义仍保留：

- Asset existence 默认对 ExportView 执行。
- 被 target filter 排除的记录，其 asset 不阻塞该 target 导出，也不进入 asset manifest。
- POT、Asset Manifest 等聚合 data exporter 使用与 target runtime export 一致的过滤后数据视图。

因此本功能已形成当前闭环：它在不破坏现有 runtime data / generated API 的前提下，补齐游戏项目最常见的资源引用检查、导出值规范化和 manifest 输出能力。

## 8.3 用户配置模型

Asset 配置分两层：

1. **Project-owned search paths**：描述“去哪里找资源”。它随项目 checkout、CI workspace、Unity / Unreal 工程位置变化，因此属于 Project Config。
2. **Meta-owned asset kinds**：描述“这类字段允许引用什么资源、策划可以怎么填写、如何规范化、如何输出 manifest”。它是数据 schema 的一部分，应和 Meta 一起 review / 版本化。

这样做比单个 `assets.root` 更兼容：同一个项目可以同时引用 Unity 工程 `Assets/`、Unreal 工程 `Content/`、共享美术仓库、DLC 目录、平台专用资源目录或远程 catalog dump；同一种 `asset_kind` 也可以按顺序在多个 search path 中查找。

### Meta 字段声明

`asset` 字段必须声明 `asset_kind`。`asset_kind` 是稳定字符串 ID，用来选择 Meta / schema TOML 中对应的资源解析规则。

设计上，`asset_kind` 是一套可复用的“字段 validation + 导出前 post-modifier”规则模板：同一个全局 kind（例如 `unreal_asset`）可以定义 Unreal mounted object、wrapper stripping、`.uasset` 查找等基础规则；字段专用 kind（例如 `item_icon`、`skill_class`）可以通过 `extends` 继承全局 kind，再叠加更窄的 `base_dirs`、命名前后缀、正则和 target policy。字段仍然只引用一个最终 kind，resolver 使用继承合并后的 effective asset kind 执行校验和 canonical ref 补齐。

```toml
[[tables.fields]]
name = "icon"
type = "asset"
asset_kind = "ui_texture"
required = true

[[tables.fields]]
name = "skill_vfx"
type = "asset"
asset_kind = "vfx_prefab"
required = false
export_when = { any = ["client"] }
```

字段规则：

| 字段 | 含义 |
| --- | --- |
| `type = "asset"` | 字段是资源引用逻辑值。runtime 基础表示仍为 string。 |
| `asset_kind` | 必填。引用 Meta / schema TOML 中的 `asset_kinds[].name`。 |
| `required` / `default` | 继续遵守普通 scalar 的 missing/default/required 规则。optional empty 不校验存在性，也不进入 manifest。 |
| `export_when` | 字段不可见时不校验、不导出、不进入 manifest。 |

`asset` list 的目标态可以支持 `type = "asset[]"` 或 `list<asset>`，但 P0-1 建议先覆盖 scalar asset；列表资源引用作为 P0-2 或同阶段可选扩展，避免一次性扩大 parser / manifest aggregation 范围。

### Project Config：search paths

Project Config 只放环境相关的搜索路径和默认策略。推荐使用 `[[assets.search_paths]]`，而不是单个 `assets.root`：

```toml
[assets]
case_sensitive = true
require_existing = true

[[assets.search_paths]]
name = "unity_assets"
root = "../UnityProject/Assets"
kind = "filesystem"

[[assets.search_paths]]
name = "unreal_game"
root = "../UnrealProject/Content"
kind = "filesystem"
mount = "/Game"

[[assets.search_paths]]
name = "shared_art"
root = "../SharedArt"
kind = "filesystem"
```

建议字段：

| 字段 | 所在位置 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `assets.case_sensitive` | `[assets]` | 平台默认或 `true` | 全局大小写策略；asset kind 可覆盖。推荐 CI / shipping 使用 `true`。 |
| `assets.require_existing` | `[assets]` | `true` | 是否检查资源存在。设为 `false` 时仍可生成 manifest，但不做 filesystem existence 校验。 |
| `assets.search_paths[].name` | `[[assets.search_paths]]` | 必填 | search path ID，供 `asset_kind.search_paths` 引用。 |
| `assets.search_paths[].root` | `[[assets.search_paths]]` | 必填 | 本地根目录。相对路径按声明它的 Project Config 文件所在目录解析，import 片段按片段目录解析后合并。 |
| `assets.search_paths[].kind` | `[[assets.search_paths]]` | `filesystem` | P0-1 只要求 local filesystem；后续可扩展 `unity_asset_database`、`unreal_asset_registry`、`remote_catalog`。 |
| `assets.search_paths[].mount` | `[[assets.search_paths]]` | 无 | 虚拟挂载前缀。例如 Unreal `Content/` 对应 `/Game`，Godot project root 对应 `res://`。 |

兼容简写：小项目可以继续允许 `[assets] root = "..."`，实现层可把它规范化成一个默认 search path，例如 `name = "default"`。长期文档和示例推荐 `search_paths`，因为它能表达多工程、多 DLC、多平台资源目录。

### Meta / schema TOML：asset kinds

`asset_kind` 定义建议作为 Meta 的顶层配置归档，而不是 Project 环境配置。公共引擎规则（例如 `unreal_asset`）应放在可复用的公共 Meta 片段中，字段专用 kind 再通过 `extends` 继承并收窄命名或目录规则。示例：

```toml
[[asset_kinds]]
name = "ui_texture"
search_paths = ["unity_assets"]
base_dirs = ["UI/Textures"]
extensions = [".png", ".asset"]
case_sensitive = true
require_sidecars = ["{path}.meta"]
input = "path"
extension_policy = "required"

[[asset_kinds]]
name = "vfx_prefab"
search_paths = ["unity_assets", "shared_art"]
base_dirs = ["VFX"]
extensions = [".prefab"]
require_sidecars = ["{path}.meta"]
deny_path_prefixes = ["dev/", "test/"]
deny_when_tags = ["shipping"]
name_prefixes = ["VFX_"]

[[asset_kinds]]
name = "unreal_asset"
search_paths = ["unreal_game"]
extensions = [".uasset"]
input = "path"
path_style = "mounted_object"
strip_wrappers = ["BlueprintGeneratedClass'{value}'", "Class'{value}'"]
object_name = "{stem}_C"
allow_auto_complete = true

[[asset_kinds]]
name = "unreal_skill_class"
extends = "unreal_asset"
base_dirs = ["Skills"]
name_prefixes = ["GA_"]

[[asset_kinds]]
name = "item_icon"
extends = "unreal_asset"
base_dirs = ["UI/Icons"]
object_name = "{stem}"
name_prefixes = ["T_"]
name_suffixes = ["_Icon"]
```

建议字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 必填 | `asset_kind` ID，供 field `asset_kind` 引用。必须全局唯一。 |
| `extends` | 无 | 可选父 `asset_kind`。用于把引擎级通用规则（如 `unreal_asset`）与字段级定制规则（如 `item_icon`）组合；实现应拒绝未知父 kind、自继承和继承环。 |
| `search_paths` | `[]` | 允许查找的 Project search path ID。空数组表示使用所有 search paths，不推荐。 |
| `base_dirs` | `[""]` | kind 在 search path 下的候选子目录。icon、vfx、audio 等固定目录应写在这里，让策划只填短路径。 |
| `extensions` | `[]` | 允许的主资源后缀。Unity / filesystem 用真实后缀；Unreal 可用 `.uasset`；Cocos runtime path 可配置为 extensionless 输入但用此列表查找真实文件。 |
| `input` | `path` | 策划填写值的基础语法。P0-1 建议只保留通用 `path` / `string` 类，不把 `cocos`、`unreal` 作为核心枚举。引擎差异由下方 path style、wrapper、extension、object name 等规则组合表达。 |
| `path_style` | `relative` | path 解释方式。可选 `relative`、`mounted`、`mounted_object`：普通相对路径、带挂载前缀路径、带 package/object 名的路径。 |
| `manifest_path` | `search_path_relative` | Asset Manifest 输出路径口径的目标态选项。可选 `input`、`search_path_relative`、`engine_canonical`；当前 resolver / runtime export / Asset Manifest exporter 尚不消费该字段，manifest 使用导出前 canonicalize 后的 asset string。 |
| `extension_policy` | `required` | source input 的扩展名策略。可选 `required`、`forbidden`、`optional`。Cocos `resources.load` 风格可用 `forbidden` + `extensions` 查找真实文件。 |
| `strip_wrappers` | `[]` | 输入 wrapper 模板。用于接受编辑器复制引用，例如 `BlueprintGeneratedClass'{value}'`，解析时提取 `{value}` 再进入通用 path 校验。 |
| `object_separator` | `.` | `mounted_object` 中 package path 与 object name 的分隔符。Unreal 可用 `.`。 |
| `object_name` | 无 | object name 模板，例如 `{stem}` 或 `{stem}_C`。用于校验或自动补齐 package 内对象名，不限定 Unreal。 |
| `case_sensitive` | 继承全局 | 是否要求 source ref 大小写与真实文件一致。 |
| `require_existing` | 继承全局 | 是否检查主资源存在。 |
| `require_sidecars` | `[]` | 配对文件模板，例如 Unity 主资源要求 `{path}.meta` 同时存在。 |
| `allow_path_prefixes` | `[]` | 若非空，规范化后的 logical path 必须位于这些 prefix 下。 |
| `deny_path_prefixes` | `[]` | 命中任一 prefix 时非法。用于禁止 `dev/`、`test/`、`editor/` 等目录。 |
| `deny_when_tags` | `[]` | 当前 target tags 命中任一 tag 时启用 deny rule。常见值为 `shipping`。 |
| `name_prefixes` | `[]` | 文件名或对象名必须满足的前缀集合，例如 `Icon_`、`VFX_`、`BP_`。 |
| `name_suffixes` | `[]` | 文件名或对象名必须满足的后缀集合，例如 `_Icon`、`_Mat`、`_C`。 |
| `allow_patterns` | `[]` | 简化正则 allow list。匹配对象默认为规范化后的 logical ref；适合少量项目私有规则。 |
| `deny_patterns` | `[]` | 简化正则 deny list。匹配对象默认为规范化后的 logical ref；用于禁止复杂目录、命名或私有包规则。 |
| `regex_rules` | `[]` | 高级阶段化正则规则数组。用于兜底私有化引擎或难以结构化表达的特殊引用模式。 |
| `allow_auto_complete` | `false` | 是否允许从短路径 / 文件名补全 canonical ref。默认关闭，避免歧义。 |

`input` 不建议扩张成 `cocos_resource_path`、`unreal_soft_object_path` 这类引擎专用枚举。更好的模型是“少量通用基础语法 + 可组合规则”：

| 规则组合 | 可表达场景 | 例子 |
| --- | --- | --- |
| `input = "path"` + `path_style = "relative"` + `extension_policy = "required"` | 普通文件系统 / Unity 主资源。填写相对 `base_dirs` 的路径。 | `item_sword.png`、`Weapons/sword.prefab` |
| `input = "path"` + `path_style = "relative"` + `extension_policy = "forbidden"` | Cocos Creator `resources.load` 风格，策划填不带扩展名的 runtime path，校验时用 `extensions` 找真实文件。 | `ui/icon_sword` |
| `input = "path"` + `path_style = "mounted"` + search path `mount = "res://"` | Godot `res://` project resource path。 | `res://ui/icon_sword.png` |
| `input = "path"` + `path_style = "mounted_object"` + search path `mount = "/Game"` + `object_separator = "."` | Unreal soft object / class path。 | `/Game/UI/WBP_Item.WBP_Item_C` |
| `strip_wrappers = ["BlueprintGeneratedClass'{value}'"]` + `path_style = "mounted_object"` | Unreal 编辑器复制引用。 | `BlueprintGeneratedClass'/Game/UI/WBP_Item.WBP_Item_C'` |

### 高级正则兜底规则

`prefix`、`suffix`、`extensions`、`path_style`、`object_name` 等结构化规则应覆盖大多数公开引擎和常规项目约束。为了兼容不公开私有化引擎、项目私有包格式或特殊命名体系，`asset_kind` 可以额外提供正则规则作为高级 escape hatch，但正则只做校验，不参与核心路径解析、候选生成和自动补齐。

简单场景可使用 `allow_patterns` / `deny_patterns`：

```toml
[[asset_kinds]]
name = "private_ui_sprite"
search_paths = ["private_game_assets"]
base_dirs = ["ui/icons"]
extensions = [".asset"]
input = "path"
path_style = "mounted_object"
extension_policy = "forbidden"
object_separator = "#"
name_prefixes = ["Icon_"]
allow_patterns = ["^pkg://ui/icons/Icon_[A-Za-z0-9_]+#[A-Za-z][A-Za-z0-9_]*$"]
deny_patterns = ["(^|/)EditorOnly(/|$)"]
```

复杂场景使用阶段化 `regex_rules`：

```toml
[[asset_kinds.regex_rules]]
name = "private_engine_package_ref"
target = "logical_ref"
pattern = "^pkg://[a-z0-9_]+/[a-z0-9_/]+#[A-Za-z][A-Za-z0-9_]*$"
severity = "error"
message = "私有引擎资源引用必须符合 pkg://package/path#ObjectName 格式。"
```

建议字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 必填 | 正则规则 ID，用于诊断、日志和测试定位。 |
| `target` | `logical_ref` | 匹配对象。可选 `raw_input`、`logical_ref`、`search_path_relative`、`file_name`、`stem`、`object_name`、`export_ref`。 |
| `pattern` | 必填 | Rust `regex` 语义的正则表达式。Meta load/link 阶段预编译，失败直接报 Meta 配置错误。 |
| `severity` | `error` | `error` 或 `warning`。P0-1 可先只实现 `error`。 |
| `message` | 自动生成 | 失败时的用户可读说明。 |
| `when_tags` | `[]` | 非空时只在当前 target tags 命中时启用。 |
| `invert` | `false` | `true` 表示反向匹配，适合表达 deny rule。 |
| `case_sensitive` | 继承 kind | 正则大小写策略。 |

正则约束：

- 正则不应产生新的 canonical ref，也不应依赖 capture group 做路径重写；未来若需要转换能力，应单独设计 `rewrite_rules`。
- 自动补齐只能依赖结构化规则和唯一候选，不依赖正则猜测。
- 诊断应输出匹配目标、规则名和失败 message，便于项目自查私有规则。

路径解析推荐规则：

1. 先按 `asset_kind.input` 和 `path_style` 解析 source cell，必要时应用 `strip_wrappers`，得到 canonical logical ref。
2. 再按 `search_paths × base_dirs × extensions` 生成候选 filesystem path 或 engine virtual path。
3. 候选必须在对应 search path root 内，禁止 absolute path 和 `..` 逃逸。
4. 如果多个候选命中，除非 kind 配置了明确优先级，否则报 `AssetRefAmbiguous`。
5. 执行 prefix / suffix / allow / deny / regex 等语义规则；正则作用在解析后的阶段化值上。
6. 如果 `allow_auto_complete = true`，只允许在唯一候选时补全；补全结果作为当前 target 的 canonical export ref 写入 runtime data，诊断可提示用户建议写 canonical ref。
7. 后续 manifest 默认复用 canonical logical path，不输出本机绝对路径。

## 8.4 引擎资源规则映射

`asset_kind` 不直接绑定 Unity / Unreal / Godot / Cocos，但它要能配置出这些引擎的常见资源规则。

### Unity

Unity 的资源文件通常位于工程 `Assets/` 下。导入资源时 Unity 为主资源和文件夹生成配套 `.meta` 文件，`.meta` 中保存 GUID 和 import settings；丢失或错误移动 `.meta` 可能导致引用断裂。因此 Unity 风格 kind 应支持：

- 主资源后缀校验，例如 `.png`、`.prefab`、`.asset`、`.mat`、`.controller`。
- 配对 `.meta` 文件校验，例如 `require_sidecars = ["{path}.meta"]`，对 `Foo.png` 检查 `Foo.png.meta`。
- 固定目录，例如 UI icon 只允许 `Assets/UI/Textures`。
- 文件名前缀 / 后缀，例如 `Icon_`、`VFX_`。
- manifest 输出可选择相对 `Assets/` 的路径，便于后续转换为 Addressables group 或 bundle list。

示例：

```toml
[[asset_kinds]]
name = "unity_ui_texture"
search_paths = ["unity_assets"]
base_dirs = ["UI/Textures"]
extensions = [".png", ".asset"]
input = "path"
path_style = "relative"
extension_policy = "required"
require_sidecars = ["{path}.meta"]
name_prefixes = ["Icon_"]
```

### Unreal Engine

Unreal 用户常从编辑器复制资源引用。常见路径是虚拟挂载路径而不是文件系统路径，例如 `/Game/...`；对象路径通常包含 package path 和对象名，形式类似 `/Game/UI/WBP_Item.WBP_Item`。Blueprint class / generated class 引用常见对象名后缀 `_C`，复制引用时还可能带类型前缀和引号，例如 `BlueprintGeneratedClass'/Game/UI/WBP_Item.WBP_Item_C'`。

Unreal 风格不是通过 `input = "unreal"` 这类专用枚举实现，而是通过 mounted object path、wrapper stripping、object name 模板和 extension lookup 等通用规则组合实现。kind 应支持：

- search path mount 映射：`Content/` ↔ `/Game`。
- 接受 canonical soft object path：`/Game/Folder/Asset.Asset`。
- 接受 class path：`/Game/Folder/BP_Name.BP_Name_C`。
- 接受编辑器复制引用的 wrapper：`Texture2D'...'`、`BlueprintGeneratedClass'...'`，解析后校验内部路径。
- 文件存在性映射到 `.uasset`：`/Game/UI/WBP_Item.WBP_Item_C` 对应 `Content/UI/WBP_Item.uasset`。
- 对 class kind 要求对象名 `_C`，对 pure asset kind 不要求 `_C`。
- 可选自动补全：策划填 `UI/WBP_Item` 时，kind 在唯一命中时补成 `/Game/UI/WBP_Item.WBP_Item_C`。默认关闭，避免同名资源或 object name 不等于 file name 时误补。

示例：

```toml
[[asset_kinds]]
name = "unreal_widget_class"
search_paths = ["unreal_game"]
base_dirs = ["UI"]
extensions = [".uasset"]
input = "path"
path_style = "mounted_object"
strip_wrappers = ["BlueprintGeneratedClass'{value}'", "Class'{value}'"]
object_separator = "."
object_name = "{stem}_C"
allow_auto_complete = true
name_prefixes = ["WBP_"]
```

自动补齐规则必须保守：只有一个候选 `.uasset` 命中、package asset name 和 object name 规则都可确定时才补齐；否则报错并要求策划填写完整 Unreal 路径。这样既支持从编辑器复制粘贴，也支持项目为高频字段提供短写法。

### Godot

Godot 项目资源通常使用 `res://` 表示项目根目录，`user://` 表示用户可写数据目录。配置引用游戏随包资源时通常应使用 `res://`；`user://` 更适合存档、设置、日志或用户生成内容，不应默认进入 shipping asset manifest。

Godot 风格也可由通用 mounted path 表达，不需要独立 `godot` input：

- `input = "path"`，`path_style = "mounted"`。
- `res://` prefix 校验，并映射到 project root search path。
- 按 `.tscn`、`.tres`、图片、音频等后缀校验。
- 可配置禁止 `user://`，或只允许特定 tool target 使用。

示例：

```toml
[[asset_kinds]]
name = "godot_scene"
search_paths = ["godot_project"]
base_dirs = ["scenes"]
extensions = [".tscn"]
input = "path"
path_style = "mounted"
allow_path_prefixes = ["res://scenes/"]
```

### Cocos Creator

Cocos Creator 的动态加载常以 `assets/resources` 为根，`resources.load()` 传入相对 `resources` 的路径，并且通常不包含文件扩展名。图片等导入资源还可能有子资源路径，例如 `image/spriteFrame`、`image/texture`。

Cocos 风格可由 relative path + forbidden extension policy 表达，不需要独立 `cocos` input：

- `input = "path"`，`path_style = "relative"`，`extension_policy = "forbidden"`，策划填 runtime load path，不带后缀。
- `base_dirs = ["resources"]` 或 search path root 直接指向 `assets/resources`。
- 校验时用 `extensions` 找真实主资源，例如 `.prefab`、`.png`、`.mp3`。
- 支持子资源 suffix / subpath 规则，例如 SpriteFrame kind 要求 `/spriteFrame` 或根据主图片自动补 runtime path。

示例：

```toml
[[asset_kinds]]
name = "cocos_prefab"
search_paths = ["cocos_assets"]
base_dirs = ["resources"]
extensions = [".prefab"]
input = "path"
path_style = "relative"
extension_policy = "forbidden"
```

## 8.5 Target 校验与补齐策略

基础策略由 effective target tags 驱动。Resource validation 和 canonical ref 补齐绑定到 target 的 MetaAndData validation / export 阶段：只要当前 target 可见 asset 字段、Meta 声明了 matching kind，且 Project Config 提供了 kind 需要的 search path，就执行校验并在需要时补齐导出值。`asset_manifest` 是同级 data export task，消费 validation 通过后的 canonicalized DataSet，因此不会与 JSON / MessagePack / bin 看到不同的 asset string。

示例：

```toml
[[targets]]
name = "client_shipping"
tags = ["client", "shipping"]

[[targets.exports]]
type = "json"
output = "Generated/Data/ClientShipping"
```

在上面的 target 中，可见 asset 字段会被检查；如果某个 kind 开启 `allow_auto_complete = true`，runtime data exporter 和 asset manifest exporter 看到的是补齐后的 canonical string。Manifest 输出作为普通 `[[targets.exports]]` 聚合 exporter 添加：

```toml
[[targets]]
name = "client_shipping"
tags = ["client", "shipping"]

[[targets.exports]]
type = "asset_manifest"
output = "Generated/AssetManifest"
file_name = "client_assets.json"
format = "json"
deduplicate = true
include_referenced_by = true
```

因此 P0-1 的判断标准是：没有 manifest task 时，asset validation / completion 仍然生效；manifest task 只是后续额外消费已解析 asset refs 的一个 exporter。
当前实现仍保留该边界：没有 manifest task 时，asset validation / completion 仍然生效；声明 manifest task 时，它额外消费 canonical asset refs 生成清单。

## 8.6 校验语义

Asset 校验在 target ExportView 上执行。校验输入是当前 target 可见 table、row、field / field path 和解析后的 typed value。

### 字段级校验

对每个可见 `asset` 字段：

1. 按普通 scalar 规则处理 missing、empty、default、required。
2. 非空 asset ref 必须是 string-like value，且符合 asset ref syntax。
3. `asset_kind` 必须存在于 Meta `asset_kinds`，且引用的 search paths 必须存在于 Project Config。
4. ref path 必须符合 `asset_kind.input` 的格式；解析后的候选路径不能是 absolute path，不能通过 `..` 逃逸 search path root / base dir。
5. 如果 kind 配置了 `extensions`，`extension_policy = "required"` 时 ref 后缀必须命中；`extension_policy = "forbidden"` 时输入不得包含扩展名，并且必须能通过 `extensions` 唯一解析到主资源。
6. 如果 kind 配置了 `name_prefixes` / `name_suffixes`，文件名或对象名必须命中。
7. 如果 target tags 命中 kind 的禁止策略，不能引用禁止 prefix。
8. 如果启用 existence 校验，resolved filesystem path 必须存在。
9. 如果配置了 `require_sidecars`，主资源对应的配对文件也必须存在，例如 Unity `.meta`。
10. 如果配置了 `allow_patterns`、`deny_patterns` 或 `regex_rules`，按规则匹配对应阶段化字符串；失败时报 `AssetRegexRuleMismatch` 或 `AssetPathDeniedForTarget`。
11. 如果启用 case-sensitive 校验，source ref 的每个路径段大小写必须与真实文件系统项一致。
12. 如果 `allow_auto_complete = true`，且 source ref 是短路径、缺少 mount / object name 或来自 wrapper 剥离后的可补齐形式，则只在唯一候选命中且 object name 规则确定时生成 canonical export ref；无法唯一确定时报 `AssetRefAmbiguous` 或 `AssetRefInvalid`。

### Target-aware 规则

Asset 校验必须遵守当前 ExportView：

- 被 table `export_when` 排除的表不校验。
- 被 field `export = false` 或 `export_when` 排除的字段不校验。
- 被 `row_export` 排除的 collection row 不校验。
- ObjectTable 作为 singleton row 校验当前 target 可见字段。
- server target 默认不会因为 client-only asset 缺失失败；只有该 asset 字段在 server ExportView 中可见时才校验。

Shipping / region / platform 规则不建议写成硬编码 target 名称；应通过 target tags 和 kind policy 表达，例如 `deny_when_tags = ["shipping"]`、`deny_path_prefixes = ["dev/", "test/"]`，或后续 target-level asset policy。

### 大小写与跨平台问题

资源路径大小写是游戏项目常见事故，尤其是 Windows 编辑环境和 Linux/macOS/移动端构建环境不一致。P0-1 推荐：

- validation 的 path normalization 使用 lexical 规则，不能吞掉 case mismatch。
- `case_sensitive = true` 时，即使当前文件系统大小写不敏感，也应尽量通过逐段目录枚举确认真实路径段大小写。
- 如果平台无法可靠判断大小写，应输出 warning 或 fail-fast 取决于配置；shipping target 推荐 fail-fast。
- canonical export ref 默认使用 source 中经过规范化的 logical path；若校验阶段发现真实路径大小写不同，应报错而不是自动改写。后续 manifest 也应复用同一 canonical ref。

### 诊断

目标态建议稳定诊断 code：

| Code | 场景 |
| --- | --- |
| `AssetKindMissing` | `type = "asset"` 字段未声明 `asset_kind`。 |
| `AssetKindUnknown` | `asset_kind` 不存在于 Meta `asset_kinds`。 |
| `AssetSearchPathUnknown` | asset kind 引用的 search path 未在 Project Config 中声明。 |
| `AssetRefInvalid` | ref 为空、absolute、包含非法字符或 path traversal。 |
| `AssetExtensionNotAllowed` | 后缀不在 kind `extensions` 中。 |
| `AssetRefAmbiguous` | 短路径、自动补齐或 extensionless 输入命中多个候选资源。 |
| `AssetNamePatternMismatch` | 文件名或对象名不满足 kind 的 prefix / suffix 规则。 |
| `AssetRegexRuleMismatch` | asset ref 不满足 kind 的 allow pattern 或阶段化正则规则。 |
| `AssetPathDeniedForTarget` | 当前 target tags 下命中禁止 prefix。 |
| `AssetFileMissing` | resolved path 不存在。 |
| `AssetSidecarMissing` | 主资源存在，但 required sidecar 缺失，例如 Unity `.meta`。 |
| `AssetPathCaseMismatch` | ref 大小写与真实文件不一致。 |
| `AssetManifestConflict` | manifest 输出中同一 logical asset path 对应不同 kind 或 canonical path 冲突。 |

当前实现的 validation 诊断 MVP 统一使用 `asset.reference` code，并包含 target、table、field_path、source row 和 resolver 失败信息。下表中的细粒度 code 是后续诊断增强目标；增强时应继续包含 target、table、field_path、record_key、asset_kind、asset_ref、resolved_path、source location，以及相关 asset kind / search path 定义位置。

## 8.7 Asset Manifest 导出

Asset Manifest 是已实现的聚合 data exporter，读取当前 target ExportView 中所有可见且非空的 `asset` 字段。它与 JSON、MessagePack、bin、POT、Lua 等 export task 同级，使用 `[[targets.exports]] type = "asset_manifest"` 启用；它消费导出前已经 canonicalize 的 asset string，因此不会重新定义资源解析语义。

### Export task

当前 exporter type：

```toml
[[targets.exports]]
type = "asset_manifest"
output = "Generated/AssetManifest"
file_name = "client_assets.json"
format = "json"
deduplicate = true
include_referenced_by = true
```

字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `file_name` | `asset_manifest.json` | 输出文件名。 |
| `format` | `json` | 支持 `json` / `csv`。CSV 输出为 Unreal DataTable 可导入格式。 |
| `deduplicate` | `true` | 同一 `(kind, path)` 合并为一个 asset entry。 |
| `include_referenced_by` | `true` | 是否输出引用来源数组。关闭后只输出资源列表，适合打包器快速消费。 |
| `include_source_location` | `false` | 是否在 `referenced_by` 中附加 source path / row。默认关闭以保持 manifest 精简。 |

### JSON 输出结构

当前 JSON：

```json
{
  "version": 1,
  "target_tags": ["client", "shipping"],
  "assets": [
    {
      "kind": "ui_texture",
      "path": "UI/Textures/item_sword.png",
      "referenced_by": [
        {
          "table": "items",
          "row": "1001",
          "field": "icon"
        }
      ]
    }
  ]
}
```

字段语义：

| 字段 | 说明 |
| --- | --- |
| `version` | Manifest schema version。首版为 `1`。 |
| `target_tags` | 当前 target 的 tag 集合，按稳定顺序输出。 |
| `assets[].kind` | asset kind。 |
| `assets[].path` | canonical asset string，与 JSON / MessagePack / bin runtime export 中的 asset 字段值一致。 |
| `assets[].referenced_by[]` | 引用来源。用于排查资源为何进入包。 |
| `referenced_by.table` | table name。 |
| `referenced_by.row` | record key；CollectionTable 使用 primary index 或 source row fallback，ObjectTable 可用 `<object>`。 |
| `referenced_by.field` | field path，例如 `icon`、`effects[0].prefab`。 |

排序必须稳定：

1. `assets` 按 `(kind, path)` 排序。
2. `referenced_by` 按 table order、row order、field order 排序。
3. 相同引用去重。
4. 输出 UTF-8，JSON object key 顺序固定。

### CSV 输出兼容（Unreal DataTable）

当前实现不拆分 exporter type，而是使用统一 `type = "asset_manifest"`，通过 `format = "csv"` 输出 CSV。CSV 头部使用 Unreal DataTable 可导入的 `Name` 行名列，并保持稳定列顺序：

```csv
Name,AssetRef,AssetKind,ReferencedBy,Tables,Fields,RecordKeys,Sources
```

字段语义：

| 字段 | 说明 |
| --- | --- |
| `Name` | Unreal DataTable 行名，由 `(kind, path)` 生成 ASCII 安全稳定 ID；冲突时追加序号。 |
| `AssetRef` | canonical asset string，与 JSON manifest 的 `assets[].path` 一致。 |
| `AssetKind` | `asset_kind`。 |
| `ReferencedBy` | 展平引用来源，格式如 `items[id=1001].icon`，多个引用用 `|` 分隔；`include_referenced_by = false` 时为空。 |
| `Tables` | 去重后的表名集合，`|` 分隔。 |
| `Fields` | 去重后的 field path 集合，`|` 分隔。 |
| `RecordKeys` | 去重后的 record key 集合，`|` 分隔；ObjectTable 使用 `<object>`。 |
| `Sources` | 当 `include_source_location = true` 时输出 source path / row，多个值用 `|` 分隔。 |

CSV 默认同样 `deduplicate = true`，即同一 `(kind, path)` 合并为一个 DataTable row，并把 referenced_by 信息展平到字符串列。若设置 `deduplicate = false`，同一资源的多次引用会分别输出为多行，行名冲突时追加稳定序号。

## 8.8 与 runtime data / codegen 的关系

第一阶段不改变 runtime data 和 generated code 的基础形态；当前实现已按以下语义落地：

- `asset` 在 JSON / MessagePack / bin runtime payload 和 Asset Manifest 中仍按字符串输出，但当 `asset_kind` 允许补齐时，输出值应使用当前 target 解析得到的 canonical export ref，而不是原始 source cell。
- 各语言 generated row 字段类型仍为 string-like 类型。
- codegen 仍把 `asset` 映射为 string-like 类型，不默认生成引擎资源加载 API。
- 业务运行时如何把 string asset path 转成 Unity Addressable、Unreal SoftObjectPath、AssetId、GUID 或句柄，由项目侧 adapter 或手写扩展决定。

实现上已通过共享 `AssetResolver` 避免 validation 和 exporter 使用不同语义：validation 阶段用 resolver 产生 diagnostics，export 阶段复用同一解析逻辑把 raw asset value materialize 成 canonical string；Asset Manifest exporter 读取这份 canonicalized DataSet。后续可引入 target-level asset resolution cache，减少 validation 与 export 的重复解析成本。

未来扩展可以在 codegen 层增加可选 mapping，例如 C# / TypeScript 生成品牌化 `AssetRef<TKind>` wrapper，或 Unreal C++ 生成 `FSoftObjectPath` / `TSoftClassPtr` 友好 API。但这些都不应成为 P0-1 的阻塞项。

## 8.9 边界和开放问题

需要讨论确认的点：

1. **asset ref 是相对 kind base_dirs、search path root，还是 engine virtual root？** 本文建议由 `asset_kind.input` 决定：Unity / filesystem 高频字段可相对 `base_dirs`，Unreal / Godot 可填写 engine canonical path，Cocos 可填写 runtime extensionless path。
2. **是否允许 extension 自动补全？** 本文建议默认不补全；只有 `extension_policy = "forbidden"` 或 `allow_auto_complete = true` 时才允许，并且必须唯一命中。
3. **是否支持 Unreal 短路径自动补齐？** 可以支持，但必须保守：唯一候选、对象名规则可确定、class `_C` 规则明确时才补齐；否则要求填写完整 Unreal path 或编辑器复制引用。
4. **是否把不存在资源作为 warning？** 本文建议默认 error；只有 `assets.require_existing = false` 或 kind/target policy 显式关闭时跳过存在性校验。
5. **是否需要资源 provider 抽象？** P0-1 可先支持 local filesystem；Unity Asset Database、Unreal Asset Registry、Godot import data、Cocos asset database、远程 CDN catalog 可后续作为 provider 扩展。
6. **是否要校验 shipping 包禁止 dev/test 路径？** 本文建议通过 target tags + kind deny policy 实现，而不是硬编码 shipping target 名称。
7. **是否输出 resolved filesystem path？** 本文建议 runtime data 和后续 manifest 默认不输出本机绝对路径，避免不可复现；只在 diagnostics 或 debug mode 输出。
8. **正则是否能参与路径重写？** P0-1 不支持。正则仅用于校验，避免把 `asset_kind` 变成难以诊断的脚本系统。

## 8.10 分阶段落地建议

### P0-1a：Meta / Project 配置和基础校验

- Meta parser 接受 `asset_kind` 并要求 `type = "asset"` 字段必须声明。
- Project Config 接受 `[assets]` 和 `[[assets.search_paths]]`；Meta 接受 `[[asset_kinds]]`。
- Validation 在 ExportView 上检查 kind、path syntax、extension、sidecar、prefix/suffix、deny prefix、regex、existence 和 case sensitivity。
- 添加诊断 code 与标准 reference_game asset fixture。

### P0-1b：Asset 引用补齐与 runtime export 集成

- 增加可复用 asset resolver，支持 relative / mounted / mounted_object、wrapper stripping、object name 模板、extension lookup 和唯一候选补齐。
- JSON / MessagePack / bin runtime export 输出 canonical asset string；未开启补齐或无需补齐时保持原值语义。
- 标准工程中已有 `skill.skill_asset`、`classes.icon` 等 asset 字段，可先按 Unreal mounted object / class 模式改造测试，并在集成测试临时目录中伪造 `.uasset` 文件验证存在性和补齐。

### P0-2：Asset Manifest exporter

状态：已实现。

- 新增 `type = "asset_manifest"` 聚合 data exporter。
- 输出 deterministic JSON manifest，包含 target tags、kind、path、referenced_by。
- 支持 `format = "csv"` 输出 Unreal DataTable 可导入 CSV。
- 与 JSON / MessagePack / bin runtime export 共用同一次 target validation、ExportView 和 canonicalized asset refs。
- 覆盖 target-aware 行过滤：shipping 排除的 debug row 不进入 manifest。

### P0-3：工具链集成和扩展格式

- 增加 CSV manifest 或 engine-specific transform 示例。
- 补充 Unity Addressables / Unreal Asset Manager / 自研打包器的消费说明。
- 评估 asset provider 插件接口，例如 Unreal Asset Registry、Unity GUID database、远程 CDN catalog。
