# 08. Asset 引用校验与 Asset Manifest 初步设计

本文从现有 Data Validation / Export CLI 设计中拆出，专门收敛 `asset` 字段、资源存在性校验、target-aware 资源策略和 Asset Manifest 聚合导出的用户可见行为。当前文档是初步设计，用于讨论产品语义和配置口径；具体 Rust 模块拆分、缓存结构和 exporter 实现细节应后续进入 `Documentation/Implementation/`。

## 8.1 设计目标

游戏配置中大量字段本质是资源引用，例如图标、prefab、blueprint、actor class、音效、技能特效、UI 资源、关卡资源、Spine / Live2D / Niagara / VFX Graph 等。ConfigForge 应在导出前尽早发现资源引用错误，并把目标包需要的资源列表稳定输出给客户端打包流水线。

目标能力：

1. 在 Meta 中用 `type = "asset"` + `asset_kind` 表达资源引用语义，而不是把所有资源路径都当普通 `string`。
2. 在 Project Config 中声明 asset root、kind、扩展名、大小写策略和 target policy。
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

### Meta 字段声明

`asset` 字段必须声明 `asset_kind`。`asset_kind` 是稳定字符串 ID，用来选择 Project Config 中对应的资源解析规则。

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
| `asset_kind` | 必填。引用 Project Config 中的 `assets.kinds[].name`。 |
| `required` / `default` | 继续遵守普通 scalar 的 missing/default/required 规则。optional empty 不校验存在性，也不进入 manifest。 |
| `export_when` | 字段不可见时不校验、不导出、不进入 manifest。 |

`asset` list 的目标态可以支持 `type = "asset[]"` 或 `list<asset>`，但 P0-1 建议先覆盖 scalar asset；列表资源引用作为 P0-2 或同阶段可选扩展，避免一次性扩大 parser / manifest aggregation 范围。

### Project Config：asset root 与 kinds

Project Config 新增 `[assets]` 与 `[[assets.kinds]]`：

```toml
[assets]
root = "../GameProject/Assets"

[[assets.kinds]]
name = "ui_texture"
root = "UI/Textures"
extensions = [".png", ".asset"]
case_sensitive = true

[[assets.kinds]]
name = "vfx_prefab"
root = "VFX"
extensions = [".prefab"]
case_sensitive = true
deny_path_prefixes = ["dev/", "test/"]
deny_when_tags = ["shipping"]
```

建议字段：

| 字段 | 所在位置 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `assets.root` | `[assets]` | 无 | 全局资源根目录。相对路径按声明它的 Project Config 文件所在目录解析，import 片段按片段目录解析后合并。 |
| `assets.case_sensitive` | `[assets]` | 平台默认或 `true` | 全局大小写策略；kind 可覆盖。推荐 CI / shipping 使用 `true`。 |
| `assets.require_existing` | `[assets]` | `true` | 是否检查资源存在。设为 `false` 时仍可生成 manifest，但不做 filesystem existence 校验。 |
| `name` | `[[assets.kinds]]` | 必填 | `asset_kind` ID。必须全局唯一。 |
| `root` | `[[assets.kinds]]` | `""` | kind 相对 `assets.root` 的子目录。 |
| `extensions` | `[[assets.kinds]]` | `[]` | 允许的后缀。空数组表示不限制扩展名。比较默认大小写敏感。 |
| `case_sensitive` | `[[assets.kinds]]` | 继承全局 | 是否要求 source 中引用路径大小写与真实文件一致。 |
| `allow_path_prefixes` | `[[assets.kinds]]` | `[]` | 若非空，资源路径必须位于这些 logical prefix 下。 |
| `deny_path_prefixes` | `[[assets.kinds]]` | `[]` | 命中任一 prefix 时非法。用于禁止 `dev/`、`test/`、`editor/` 等目录。 |
| `deny_when_tags` | `[[assets.kinds]]` | `[]` | 当前 target tags 命中任一 tag 时启用 deny rule。常见值为 `shipping`。 |

路径解析推荐规则：

1. Source cell 中的 asset ref 是 logical asset path，不应写绝对路径。
2. 禁止 `..` 逃逸 kind root；禁止 absolute path；禁止空路径作为有效资源。
3. `assets.root` + kind `root` + source path 组成候选 filesystem path。
4. 如果 source path 已包含 kind root 前缀，P0-1 建议仍按“相对 kind root”解释，避免同一 kind 内出现两种写法；是否允许 absolute logical path 作为后续讨论项。

### Target 或 export task 策略

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

P0-1 建议先把 resource validation 绑定到 target 的 MetaAndData validation 阶段，只要当前 target 可见 asset 字段且 Project Config 声明了 matching kind，就执行校验；manifest 是否输出由 `[[targets.exports]] type = "asset_manifest"` 决定。

## 8.4 校验语义

Asset 校验在 target ExportView 上执行。校验输入是当前 target 可见 table、row、field / field path 和解析后的 typed value。

### 字段级校验

对每个可见 `asset` 字段：

1. 按普通 scalar 规则处理 missing、empty、default、required。
2. 非空 asset ref 必须是 string-like value，且符合 asset ref syntax。
3. `asset_kind` 必须存在于 `assets.kinds`。
4. ref path 必须是相对 logical path，不能是 absolute path，不能通过 `..` 逃逸 kind root。
5. 如果 kind 配置了 `extensions`，ref 后缀必须命中。
6. 如果 target tags 命中 kind 的禁止策略，不能引用禁止 prefix。
7. 如果启用 existence 校验，resolved filesystem path 必须存在。
8. 如果启用 case-sensitive 校验，source ref 的每个路径段大小写必须与真实文件系统项一致。

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
| `AssetKindUnknown` | `asset_kind` 不存在于 Project Config。 |
| `AssetRefInvalid` | ref 为空、absolute、包含非法字符或 path traversal。 |
| `AssetExtensionNotAllowed` | 后缀不在 kind `extensions` 中。 |
| `AssetPathDeniedForTarget` | 当前 target tags 下命中禁止 prefix。 |
| `AssetFileMissing` | resolved path 不存在。 |
| `AssetPathCaseMismatch` | ref 大小写与真实文件不一致。 |
| `AssetManifestConflict` | manifest 输出中同一 logical asset path 对应不同 kind 或 canonical path 冲突。 |

诊断应包含：target、table、field_path、record_key、asset_kind、asset_ref、resolved_path、source location，以及相关 Project Config 中 kind 定义位置。

## 8.5 Asset Manifest 导出

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
| `assets[].path` | canonical logical path。首版建议输出相对 `assets.root` 的路径，即 kind root + ref path，便于外部打包器统一消费。 |
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

## 8.6 与 runtime data / codegen 的关系

第一阶段不改变 runtime data 和 generated code 的基础形态：

- `asset` 在 JSON / MessagePack / bin runtime payload 中仍按字符串输出。
- 各语言 generated row 字段类型仍为 string-like 类型。
- codegen 可以在注释中保留 `asset_kind`，但不默认生成引擎资源加载 API。
- 业务运行时如何把 string asset path 转成 Unity Addressable、Unreal SoftObjectPath、AssetId、GUID 或句柄，由项目侧 adapter 或手写扩展决定。

未来扩展可以在 codegen 层增加可选 mapping，例如 C# / TypeScript 生成品牌化 `AssetRef<TKind>` wrapper，或 Unreal C++ 生成 `FSoftObjectPath` / `TSoftClassPtr` 友好 API。但这些都不应成为 P0-1 的阻塞项。

## 8.7 边界和开放问题

需要讨论确认的点：

1. **asset ref 是相对 kind root 还是相对 assets.root？** 本文建议 source cell 相对 kind root，manifest 输出相对 assets.root 的 canonical path。
2. **是否允许 extension 自动补全？** 本文建议不自动补全，避免同名不同扩展资源造成歧义。
3. **是否支持多候选扩展查找？** 本文建议只校验 source 明确写出的文件名；多候选可作为后续可选策略。
4. **是否把不存在资源作为 warning？** 本文建议默认 error；只有 `assets.require_existing = false` 或 kind/target policy 显式关闭时跳过存在性校验。
5. **是否需要资源 provider 抽象？** P0-1 可先支持 local filesystem；Unity GUID、Unreal Asset Registry、远程 asset catalog 可后续作为 provider 扩展。
6. **是否要校验 shipping 包禁止 dev/test 路径？** 本文建议通过 target tags + kind deny policy 实现，而不是硬编码 shipping target 名称。
7. **是否输出 resolved filesystem path？** 本文建议 manifest 默认不输出本机绝对路径，避免不可复现；只在 diagnostics 或 debug mode 输出。

## 8.8 分阶段落地建议

### P0-1a：Meta / Project 配置和基础校验

- Meta parser 接受 `asset_kind` 并要求 `type = "asset"` 字段必须声明。
- Project Config 接受 `[assets]` 和 `[[assets.kinds]]`。
- Validation 在 ExportView 上检查 kind、path syntax、extension、deny prefix、existence 和 case sensitivity。
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

