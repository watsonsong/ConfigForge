# Asset Kind 校验与导出补齐 TODO

本文跟踪 P0-1 `asset_kind`、资源引用校验和导出补齐的开发计划。Asset Manifest exporter 暂不纳入本轮实现；后续在校验 / 补齐闭环稳定后单独推进。

## 目标范围

- Meta 支持 `asset` 字段声明 `asset_kind`，并支持顶层 `[[asset_kinds]]`。
- Project Config 支持 `[assets]` 和 `[[assets.search_paths]]`，用于描述本地资源搜索根与 mount 映射。
- Validation 对当前 target ExportView 中可见 asset 字段执行资源引用校验。
- Runtime data export 输出经过 asset resolver 补齐后的 canonical asset string。
- 标准工程先按 Unreal mounted object / class 模式覆盖集成测试，通过测试临时目录伪造 `.uasset` 文件完成存在性校验。

## 非目标

- 本轮不实现 `type = "asset_manifest"` exporter。
- 本轮不扫描 Unity / Unreal / Godot / Cocos 的完整依赖图。
- 本轮不解析 Unity `.meta` 内容、Unreal Asset Registry 或私有引擎数据库。
- 正则规则只做校验，不做 capture rewrite 或路径转换。

## Step 1：Meta schema 接入 asset_kind

状态：待开始

工作项：

1. 在 raw Meta model 中为 field 增加 `asset_kind` 可选字段。
2. 在 linked field model 中保存 `asset_kind`，确保 fragment field、struct field 和 table field 都能透传。
3. 增加顶层 `[[asset_kinds]]` raw / linked model，字段覆盖 P0-1 所需子集：
   - `name`
   - `search_paths`
   - `base_dirs`
   - `extensions`
   - `input`
   - `path_style`
   - `extension_policy`
   - `strip_wrappers`
   - `object_separator`
   - `object_name`
   - `case_sensitive`
   - `require_existing`
   - `require_sidecars`
   - `allow_path_prefixes`
   - `deny_path_prefixes`
   - `deny_when_tags`
   - `name_prefixes`
   - `name_suffixes`
   - `allow_patterns`
   - `deny_patterns`
   - `regex_rules`
   - `allow_auto_complete`
4. Meta validate 增加规则：
   - `type = "asset"` 必须声明 `asset_kind`。
   - 非 asset 字段暂不允许声明 `asset_kind`。
   - `asset_kinds[].name` 全局唯一。
   - regex pattern 在 Meta 阶段预编译并报告配置错误。

建议测试：

- `asset` 字段缺少 `asset_kind` 报错。
- 非 asset 字段声明 `asset_kind` 报错。
- 重复 `asset_kinds[].name` 报错。
- 非法 regex pattern 报错。
- 现有没有 asset kind 的测试需按预期更新，或在兼容阶段明确只 warning；本轮建议直接转为 error。

## Step 2：Project Config 接入 assets.search_paths

状态：待开始

工作项：

1. 在 Project Config schema 中增加 `[assets]`：
   - `case_sensitive`
   - `require_existing`
   - `root` 兼容简写，可规范化为默认 search path。
2. 增加 `[[assets.search_paths]]`：
   - `name`
   - `root`
   - `kind = "filesystem"`
   - `mount`
3. import 合并和路径规范化阶段处理 search path root：
   - 相对路径按声明文件所在目录解析。
   - search path name 全局唯一。
   - P0-1 只接受 `kind = "filesystem"`，其他 kind 明确报 unsupported。
4. link / validate 阶段检查 `asset_kind.search_paths` 引用的 Project search path 存在。

建议测试：

- 多个 search path 合并成功。
- 重复 search path name 报错。
- 未知 search path 引用报 `AssetSearchPathUnknown`。
- 相对 root 在临时工程中解析到正确绝对路径。

## Step 3：实现 asset resolver 基础模型

状态：待开始

工作项：

1. 新增可复用 asset resolver 服务，输入包括：
   - linked meta asset kind
   - project asset search paths
   - target tags
   - raw source asset string
   - table / field / record context
2. 输出 `ResolvedAssetRef`：
   - raw input
   - canonical export ref
   - logical ref
   - search path id
   - search path relative path
   - resolved filesystem path
   - optional object name
   - completion applied flag
3. 支持通用解析：
   - `input = "path"`
   - `path_style = "relative"`
   - `path_style = "mounted"`
   - `path_style = "mounted_object"`
   - `strip_wrappers`
   - `object_separator`
   - `object_name = "{stem}"` / `"{stem}_C"`
4. 支持 extension policy：
   - `required`
   - `forbidden`
   - `optional`
5. 支持候选生成：
   - `search_paths × base_dirs × extensions`
   - 禁止 absolute path。
   - 禁止 `..` 逃逸。
   - 多候选时报 `AssetRefAmbiguous`。

建议测试：

- relative path + extension required。
- Unreal `/Game/UI/WBP_Item.WBP_Item_C` 映射到 `Content/UI/WBP_Item.uasset`。
- Unreal `BlueprintGeneratedClass'...'` wrapper 剥离。
- `UI/WBP_Item` 在唯一候选时补齐为 `/Game/UI/WBP_Item.WBP_Item_C`。
- 同名多候选补齐时报 ambiguous。

## Step 4：资源存在性、sidecar、命名和正则校验

状态：待开始

工作项：

1. `require_existing = true` 时检查主资源存在。
2. `require_sidecars` 按 `{path}` 模板检查配对文件。
3. 实现 `name_prefixes` / `name_suffixes`。
4. 实现 `allow_path_prefixes` / `deny_path_prefixes`。
5. 实现 `deny_when_tags`。
6. 实现 `allow_patterns` / `deny_patterns`。
7. 实现 `regex_rules` 的阶段化 target：
   - P0-1 至少支持 `raw_input`、`logical_ref`、`search_path_relative`、`file_name`、`stem`、`object_name`、`export_ref`。
8. 实现大小写检查：
   - P0-1 可以先做真实路径逐段枚举。
   - 无法可靠判断的平台场景按配置 warning / error；shipping 推荐 error。

建议测试：

- 缺少主资源报 `AssetFileMissing`。
- 缺少 sidecar 报 `AssetSidecarMissing`。
- 文件名不满足 prefix / suffix 报 `AssetNamePatternMismatch`。
- regex 不匹配报 `AssetRegexRuleMismatch`。
- shipping target 命中 deny prefix 报 `AssetPathDeniedForTarget`。

## Step 5：Validation pipeline 接入 ExportView

状态：待开始

工作项：

1. 在 target-aware validation 中遍历 ExportView 可见 table / row / field。
2. 仅对可见且非空的 asset 字段执行 resolver。
3. 保持普通 required / default / scalar type 校验先执行，asset resolver 在 string-like 值确定后执行。
4. 诊断包含：
   - target
   - table
   - field path
   - record key
   - asset kind
   - raw asset ref
   - canonical ref
   - resolved path
   - source location
5. server target 不应校验 client-only asset 字段。

建议测试：

- client target 校验 client asset。
- server target 跳过 `export_when = { any = ["client"] }` 的 asset 字段。
- row_export 排除的记录中 asset 缺失不阻塞当前 target。

## Step 6：Runtime export 使用 canonical asset string

状态：待开始

工作项：

1. 在 DataResolver 或与其等价的 materialization 边界接入 asset resolver。
2. JSON / MessagePack / bin exporter 对 asset scalar 输出 canonical export ref。
3. 未开启补齐或无需补齐时保持原 source ref 规范化语义。
4. 避免 validation 与 export 使用不同解析结果：
   - 可先复用同一 resolver 重算；
   - 后续再引入 target-level asset resolution cache。

建议测试：

- source 填 `Skills/GA_Slash`，JSON 输出 `/Game/Skills/GA_Slash.GA_Slash_C`。
- MessagePack / bin smoke 至少确认导出不失败；如已有 reader harness 可进一步断言值。
- wrapper 输入导出为去 wrapper 后 canonical path。

## Step 7：标准工程 Unreal 模式集成测试

状态：待开始

工作项：

1. 更新 `tests/fixtures/standard_project` 的 asset 字段：
   - `items.skill.skill_asset` 使用 `asset_kind = "unreal_skill_class"`。
   - `classes.icon` 可使用 `asset_kind = "unreal_ui_texture"` 或先保持单一 Unreal kind 覆盖核心路径。
2. 在标准工程或测试临时目录中配置 `[assets]` 和 `[[assets.search_paths]]`：
   - `root` 指向临时伪造 `Content/`。
   - `mount = "/Game"`。
3. 集成测试运行前在 tempdir 创建伪造文件：
   - `Content/Skills/GA_Slash.uasset`
   - `Content/Skills/GA_PowerShot.uasset`
   - `Content/Skills/GA_Fireball.uasset`
   - 如覆盖 classes icon，再创建对应 UI `.uasset`。
4. 数据表可先把现有值从 `assets/skills/slash.asset` 改为短写法或 Unreal canonical：
   - 短写法：`Skills/GA_Slash`
   - canonical：`/Game/Skills/GA_Slash.GA_Slash_C`
   - wrapper：`BlueprintGeneratedClass'/Game/Skills/GA_Slash.GA_Slash_C'`
5. 断言 client JSON 输出 canonical Unreal path。
6. 断言 server target 不包含 client-only skill asset，也不因为缺失 client asset 失败。

建议测试：

- standard_project client 构建通过，`items.json` 中 `skill.skill_asset` 是 canonical Unreal class path。
- standard_project server 构建通过且不输出 client-only asset 字段。
- 删除一个伪造 `.uasset` 后的负例测试能报 `AssetFileMissing`。

## Step 8：诊断、文档和回归收敛

状态：待开始

工作项：

1. 补齐诊断 code 文档和测试断言。
2. 更新 Design / Implementation 文档中的实现状态。
3. 运行格式化和测试：
   - `cargo fmt`
   - `cargo test`
   - 相关 CLI integration tests
4. 每个开发 step 完成并验证后单独提交 git。
5. 每次提交后根据实现反馈评估后续 step 是否需要调整。

## 后续：Asset Manifest exporter

状态：暂缓

后续单独拆分：

1. 新增 `type = "asset_manifest"` 聚合 data exporter。
2. 复用 asset resolver / cache，避免重新解析。
3. 输出 deterministic JSON。
4. 可选支持 `format = "csv"`。
5. 覆盖 target-aware 行过滤、deduplicate 和 referenced_by 排序。
