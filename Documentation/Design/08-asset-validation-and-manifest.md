# 08. Asset 引用校验与 Asset Manifest 初步设计

本文从现有 Data Validation / Export CLI 设计中拆出，专门收敛 `asset` 字段、资源存在性校验、target-aware 资源策略和 Asset Manifest 聚合导出的用户可见行为。当前文档是初步设计，用于讨论产品语义和配置口径；具体 Rust 模块拆分、缓存结构和 exporter 实现细节应后续进入 `Documentation/Implementation/`。

## 8.1 设计目标

游戏配置中大量字段本质是资源引用，例如图标、prefab、blueprint、actor class、音效、技能特效、UI 资源、关卡资源、Spine / Live2D / Niagara / VFX Graph 等。ConfigForge 应在导出前尽早发现资源引用错误，并把目标包需要的资源列表稳定输出给客户端打包流水线。

目标能力：

1. 在 Meta 中用 `type = "asset"` + `asset_kind` 表达资源引用语义，而不是把所有资源路径都当普通 `string`。
2. 用 Project Config 声明可变的资源搜索路径，用 Meta / schema TOML 声明稳定的 `asset_kind` 语义、扩展名、输入格式、大小写策略和 target policy。
3. 对当前 target 的 ExportView 做资源引用校验：被 target tags / `export_when` / `row_export` 排除的数据不校验、不进入 manifest。
4. 输出稳定排序的 Asset Manifest，供 Unity Addressables、Unreal Asset Manager、自研打包器、CDN 预热或资源审计系统消费。
5. 保持 generated core 与 runtime data 低耦合：`asset` 在 runtime payload 中第一阶段仍是字符串值；资源存在性和 manifest 是导出前工具链能力，不把引擎资源系统绑定进 generated code。

非目标：

- 不直接扫描 Unity / Unreal / 自研引擎的完整依赖图；ConfigForge 只验证配置显式引用的资源，并输出引用清单。
- 不负责资源导入、压缩、打包、热更差分或 CDN 上传。
- 不在第一阶段解析引擎专有 GUID / meta 文件；可作为后续 provider 扩展。
- 不让 `string` 字段隐式进入 Asset Manifest；必须显式声明 `type = "asset"`。

## 8.2 当前状态

当前实现中 `asset` 已作为 primitive field type 出现在 Meta 设计中，但运行时仍按 string-like scalar 解析和导出。资源引用语法、`asset_kind`、文件存在性校验和 Asset Manifest exporter 尚未实现。

现有文档中的已确认语义仍保留：

- Asset existence 默认对 ExportView 执行。
- 被 target filter 排除的记录，其 asset 不阻塞该 target 导出，也不进入 asset manifest。
- POT、Asset Manifest 等聚合 data exporter 使用与 target runtime export 一致的过滤后数据视图。

因此本功能可以作为 P0-1 落地：它在不破坏现有 runtime data / generated API 的前提下，补齐游戏项目最常见的资源引用检查闭环。

## 8.3 用户配置模型

Asset 配置分两层：

1. **Project-owned search paths**：描述“去哪里找资源”。它随项目 checkout、CI workspace、Unity / Unreal 工程位置变化，因此属于 Project Config。
2. **Meta-owned asset kinds**：描述“这类字段允许引用什么资源、策划可以怎么填写、如何规范化、如何输出 manifest”。它是数据 schema 的一部分，应和 Meta 一起 review / 版本化。

这样做比单个 `assets.root` 更兼容：同一个项目可以同时引用 Unity 工程 `Assets/`、Unreal 工程 `Content/`、共享美术仓库、DLC 目录、平台专用资源目录或远程 catalog dump；同一种 `asset_kind` 也可以按顺序在多个 search path 中查找。

### Meta 字段声明

`asset` 字段必须声明 `asset_kind`。`asset_kind` 是稳定字符串 ID，用来选择 Meta / schema TOML 中对应的资源解析规则。

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

`asset_kind` 定义建议作为 Meta 的顶层配置归档，而不是 Project 环境配置。示例：

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
manifest_path = "search_path_relative"

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
name = "unreal_blueprint_class"
search_paths = ["unreal_game"]
base_dirs = ["Characters", "UI"]
extensions = [".uasset"]
input = "path"
path_style = "mounted_object"
strip_wrappers = ["BlueprintGeneratedClass'{value}'", "Class'{value}'"]
object_name = "{stem}_C"
allow_auto_complete = true
```

建议字段：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 必填 | `asset_kind` ID，供 field `asset_kind` 引用。必须全局唯一。 |
| `search_paths` | `[]` | 允许查找的 Project search path ID。空数组表示使用所有 search paths，不推荐。 |
| `base_dirs` | `[""]` | kind 在 search path 下的候选子目录。icon、vfx、audio 等固定目录应写在这里，让策划只填短路径。 |
| `extensions` | `[]` | 允许的主资源后缀。Unity / filesystem 用真实后缀；Unreal 可用 `.uasset`；Cocos runtime path 可配置为 extensionless 输入但用此列表查找真实文件。 |
| `input` | `path` | 策划填写值的基础语法。P0-1 建议只保留通用 `path` / `string` 类，不把 `cocos`、`unreal` 作为核心枚举。引擎差异由下方 path style、wrapper、extension、object name 等规则组合表达。 |
| `path_style` | `relative` | path 解释方式。可选 `relative`、`mounted`、`mounted_object`：普通相对路径、带挂载前缀路径、带 package/object 名的路径。 |
| `manifest_path` | `search_path_relative` | manifest 输出路径口径。可选 `input`、`search_path_relative`、`engine_canonical`。 |
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
| `allow_auto_complete` | `false` | 是否允许从短路径 / 文件名补全 canonical ref。默认关闭，避免歧义。 |

`input` 不建议扩张成 `cocos_resource_path`、`unreal_soft_object_path` 这类引擎专用枚举。更好的模型是“少量通用基础语法 + 可组合规则”：

| 规则组合 | 可表达场景 | 例子 |
| --- | --- | --- |
| `input = "path"` + `path_style = "relative"` + `extension_policy = "required"` | 普通文件系统 / Unity 主资源。填写相对 `base_dirs` 的路径。 | `item_sword.png`、`Weapons/sword.prefab` |
| `input = "path"` + `path_style = "relative"` + `extension_policy = "forbidden"` | Cocos Creator `resources.load` 风格，策划填不带扩展名的 runtime path，校验时用 `extensions` 找真实文件。 | `ui/icon_sword` |
| `input = "path"` + `path_style = "mounted"` + search path `mount = "res://"` | Godot `res://` project resource path。 | `res://ui/icon_sword.png` |
| `input = "path"` + `path_style = "mounted_object"` + search path `mount = "/Game"` + `object_separator = "."` | Unreal soft object / class path。 | `/Game/UI/WBP_Item.WBP_Item_C` |
| `strip_wrappers = ["BlueprintGeneratedClass'{value}'"]` + `path_style = "mounted_object"` | Unreal 编辑器复制引用。 | `BlueprintGeneratedClass'/Game/UI/WBP_Item.WBP_Item_C'` |

路径解析推荐规则：

1. 先按 `asset_kind.input` 和 `path_style` 解析 source cell，必要时应用 `strip_wrappers`，得到 canonical logical ref。
2. 再按 `search_paths × base_dirs × extensions` 生成候选 filesystem path 或 engine virtual path。
3. 候选必须在对应 search path root 内，禁止 absolute path 和 `..` 逃逸。
4. 如果多个候选命中，除非 kind 配置了明确优先级，否则报 `AssetRefAmbiguous`。
5. 如果 `allow_auto_complete = true`，只允许在唯一候选时补全；补全结果进入 manifest，诊断可提示用户建议写 canonical ref。
6. manifest 默认输出 canonical logical path，不输出本机绝对路径。

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
manifest_path = "input"
```

## 8.5 Target 或 export task 策略

基础策略由 effective target tags 驱动。目标态可在 target 或 asset manifest export task 上增加更细粒度策略：

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

P0-1 建议先把 resource validation 绑定到 target 的 MetaAndData validation 阶段，只要当前 target 可见 asset 字段、Meta 声明了 matching kind，且 Project Config 提供了 kind 需要的 search path，就执行校验；manifest 是否输出由 `[[targets.exports]] type = "asset_manifest"` 决定。

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
10. 如果启用 case-sensitive 校验，source ref 的每个路径段大小写必须与真实文件系统项一致。

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
- manifest 中输出 canonical logical path，默认使用 source 中经过规范化的相对 path；若校验阶段发现真实路径大小写不同，应报错而不是自动改写。

### 诊断

建议稳定诊断 code：

| Code | 场景 |
| --- | --- |
| `AssetKindMissing` | `type = "asset"` 字段未声明 `asset_kind`。 |
| `AssetKindUnknown` | `asset_kind` 不存在于 Meta `asset_kinds`。 |
| `AssetSearchPathUnknown` | asset kind 引用的 search path 未在 Project Config 中声明。 |
| `AssetRefInvalid` | ref 为空、absolute、包含非法字符或 path traversal。 |
| `AssetExtensionNotAllowed` | 后缀不在 kind `extensions` 中。 |
| `AssetRefAmbiguous` | 短路径、自动补齐或 extensionless 输入命中多个候选资源。 |
| `AssetNamePatternMismatch` | 文件名或对象名不满足 kind 的 prefix / suffix 规则。 |
| `AssetPathDeniedForTarget` | 当前 target tags 下命中禁止 prefix。 |
| `AssetFileMissing` | resolved path 不存在。 |
| `AssetSidecarMissing` | 主资源存在，但 required sidecar 缺失，例如 Unity `.meta`。 |
| `AssetPathCaseMismatch` | ref 大小写与真实文件不一致。 |
| `AssetManifestConflict` | manifest 输出中同一 logical asset path 对应不同 kind 或 canonical path 冲突。 |

诊断应包含：target、table、field_path、record_key、asset_kind、asset_ref、resolved_path、source location，以及相关 asset kind / search path 定义位置。

## 8.7 Asset Manifest 导出

Asset Manifest 是聚合 data exporter，读取当前 target ExportView 中所有可见且非空的 `asset` 字段。

### Export task

建议新增 exporter type：

```toml
[[targets.exports]]
type = "asset_manifest"
output = "Generated/AssetManifest"
file_name = "client_assets.json"
format = "json"
deduplicate = true
include_referenced_by = true
```

字段建议：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `file_name` | `asset_manifest.json` | 输出文件名。 |
| `format` | `json` | 首版建议支持 `json`；CSV 可作为兼容工具链的后续格式或 alias。 |
| `deduplicate` | `true` | 同一 `(kind, path)` 合并为一个 asset entry。 |
| `include_referenced_by` | `true` | 是否输出引用来源数组。关闭后只输出资源列表，适合打包器快速消费。 |
| `include_source_location` | `false` | 是否在 `referenced_by` 中附加 source path / row。默认关闭以保持 manifest 精简。 |

### JSON 输出结构

推荐首版 JSON：

```json
{
  "version": 1,
  "target": "client_shipping",
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
| `target` | target name。 |
| `assets[].kind` | asset kind。 |
| `assets[].path` | canonical logical path。首版建议由 `asset_kind.manifest_path` 决定，可输出 source input、search path relative path 或 engine canonical path，便于外部打包器统一消费。 |
| `assets[].referenced_by[]` | 引用来源。用于排查资源为何进入包。 |
| `referenced_by.table` | table name。 |
| `referenced_by.row` | record key；CollectionTable 使用 primary index 或 source row fallback，ObjectTable 可用 `<object>`。 |
| `referenced_by.field` | field path，例如 `icon`、`effects[0].prefab`。 |

排序必须稳定：

1. `assets` 按 `(kind, path)` 排序。
2. `referenced_by` 按 table order、row order、field order 排序。
3. 相同引用去重。
4. 输出 UTF-8，JSON object key 顺序固定。

### CSV 输出兼容

现有 `06-export-cli.md` 曾提到 `asset_manifest_csv`。为避免过早分裂 exporter，建议产品层使用统一 `type = "asset_manifest"`，通过 `format = "csv"` 输出 CSV：

```csv
asset_ref,asset_kind,table,field_path,record_key,source,tag
```

其中 `asset_ref` 对应 JSON manifest 的 canonical `path`。CSV 不适合表达多引用合并结构，因此 `format = "csv"` 可以默认 `deduplicate = false`，每个引用一行；若 `deduplicate = true`，需要把 referenced_by 展平或省略，需在实现前再确认。

## 8.8 与 runtime data / codegen 的关系

第一阶段不改变 runtime data 和 generated code 的基础形态：

- `asset` 在 JSON / MessagePack / bin runtime payload 中仍按字符串输出。
- 各语言 generated row 字段类型仍为 string-like 类型。
- codegen 可以在注释中保留 `asset_kind`，但不默认生成引擎资源加载 API。
- 业务运行时如何把 string asset path 转成 Unity Addressable、Unreal SoftObjectPath、AssetId、GUID 或句柄，由项目侧 adapter 或手写扩展决定。

未来扩展可以在 codegen 层增加可选 mapping，例如 C# / TypeScript 生成品牌化 `AssetRef<TKind>` wrapper，或 Unreal C++ 生成 `FSoftObjectPath` / `TSoftClassPtr` 友好 API。但这些都不应成为 P0-1 的阻塞项。

## 8.9 边界和开放问题

需要讨论确认的点：

1. **asset ref 是相对 kind base_dirs、search path root，还是 engine virtual root？** 本文建议由 `asset_kind.input` 决定：Unity / filesystem 高频字段可相对 `base_dirs`，Unreal / Godot 可填写 engine canonical path，Cocos 可填写 runtime extensionless path。
2. **是否允许 extension 自动补全？** 本文建议默认不补全；只有 `extension_policy = "forbidden"` 或 `allow_auto_complete = true` 时才允许，并且必须唯一命中。
3. **是否支持 Unreal 短路径自动补齐？** 可以支持，但必须保守：唯一候选、对象名规则可确定、class `_C` 规则明确时才补齐；否则要求填写完整 Unreal path 或编辑器复制引用。
4. **是否把不存在资源作为 warning？** 本文建议默认 error；只有 `assets.require_existing = false` 或 kind/target policy 显式关闭时跳过存在性校验。
5. **是否需要资源 provider 抽象？** P0-1 可先支持 local filesystem；Unity Asset Database、Unreal Asset Registry、Godot import data、Cocos asset database、远程 CDN catalog 可后续作为 provider 扩展。
6. **是否要校验 shipping 包禁止 dev/test 路径？** 本文建议通过 target tags + kind deny policy 实现，而不是硬编码 shipping target 名称。
7. **是否输出 resolved filesystem path？** 本文建议 manifest 默认不输出本机绝对路径，避免不可复现；只在 diagnostics 或 debug mode 输出。

## 8.10 分阶段落地建议

### P0-1a：Meta / Project 配置和基础校验

- Meta parser 接受 `asset_kind` 并要求 `type = "asset"` 字段必须声明。
- Project Config 接受 `[assets]` 和 `[[assets.search_paths]]`；Meta 接受 `[[asset_kinds]]`。
- Validation 在 ExportView 上检查 kind、path syntax、extension、sidecar、prefix/suffix、deny prefix、existence 和 case sensitivity。
- 添加诊断 code 与标准 reference_game asset fixture。

### P0-1b：Asset Manifest JSON exporter

- 新增 `type = "asset_manifest"` 聚合 data exporter。
- 输出 deterministic JSON manifest，包含 target、kind、path、referenced_by。
- 与 JSON / MessagePack / bin runtime export 共用同一次 target validation 和 ExportView。
- 覆盖 target-aware 行过滤：shipping 排除的 debug row 不进入 manifest。

### P0-1c：工具链集成和扩展格式

- 增加 CSV manifest 或 engine-specific transform 示例。
- 补充 Unity Addressables / Unreal Asset Manager / 自研打包器的消费说明。
- 评估 asset provider 插件接口，例如 Unreal Asset Registry、Unity GUID database、远程 CDN catalog。
