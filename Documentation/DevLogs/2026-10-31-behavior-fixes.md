# 架构评估后的行为修复（2026-10-31）

本记录接续 [架构评估](2026-10-31-architecture-review.md)，只记录修复证据与验证快照；当前契约见 Design / Implementation。

## 第一步：嵌套 Asset 的一致处理

- `asset::asset_field_paths` 仅依赖 Meta，统一普通 struct、active typed / inline fragment 的字段路径枚举。target 模式检查祖先与 child 可见性，full 模式不做导出过滤。
- validation 与 Manifest 读取 resolved scalar，包含 JSON occurrence 与 default；build 在导出副本中按原 expanded / JSON 布局写回 canonical ref，不混用布局，也不改写无关 raw key。
- Manifest 的可选 source location 优先使用字段 cell trace，修正 ObjectTable override 后仍报告原 row source 的问题。
- 存在性测试明确设置 `require_existing = true`。前次审查中“Missing.png 构建成功”本身不足以证明存在性校验缺失，因为该选项默认关闭；已确认的缺陷是 nested 字段未参与三条消费路径。本次用开启存在性检查的失败测试验证修复，不改变默认策略。

验证：

- 新增 3 个单元测试与 5 个 CLI 测试，覆盖 expanded / root JSON / nested JSON、默认值、CollectionTable / ObjectTable、typed / inline fragment、target child / ancestor / row 过滤、raw `001` record key、无 Manifest 时的校验，以及 override 成功 / 失败来源。
- `cargo fmt`、`cargo lint`、`cargo test --locked` 通过；local profile 共 707 个测试（573 lib、84 CLI、5 architecture、45 其他 integration）。可选语言工具遵守既有 local skip 策略，不等同于 full CI 环境验证。
- 安装并运行 `cargo-llvm-cov 0.9.1`；针对 Asset 相关 lib / CLI tests 生成 HTML 并检查未覆盖行。`asset/fields.rs` 行覆盖 97.53%，`build/asset_refs.rs` 93.14%，`asset_manifest/extract.rs` 96.77%。剩余主要是配置已验证后的防御分支、非 Asset / 不可见 table 与空值路径；这些数字仅代表本次定向测试。
- Design HTML 已刷新；docs build/check 与 diff check 通过。

下一步仍应独立修复未知配置键。Asset 遍历收敛没有改变配置解析边界，因此不需要更换该项方案，也不应把 build 解析缓存等较大重构混入行为修复。

## 第二步：拒绝未知配置字段与未消费的选项

- Project / Meta 的封闭配置、target 条件与语言 options 使用 `deny_unknown_fields`；保持 `TargetExportConfig.options`、`ValidatorMeta.params` 等明确动态区域，不对 flatten map 添加封闭结构属性。
- codegen 消费共用参数后，只接受当前 generator 的 canonical language table；未知外层键按稳定顺序报错。别名 `c++` / `ts` / `cs` 仍配对应 `cpp` / `typescript` / `csharp` 表。
- JSON / MessagePack / bin 规划与生成拒绝额外选项。POT / Lua / Asset Manifest 检查未知子键，并拒绝 nested table 旁被旧实现忽略的其他选项；flat / nested 的合法单一形式保持支持。
- validator 参数仍由注册 factory 解释；自定义 factory 的扩展参数可以加载，内置 validator 的拼写错误仍由该 validator 报错。
- 严格解析暴露一个旧外键测试中已被忽略的 `when_field` / `cases`；删除这两个废弃字段，保留原测试的 linked_field / fragment 名称冲突断言。

验证：

- 新增 3 个单元测试与 4 个 CLI 测试，包含 standalone Meta、import 文件定位、`requird` / `ruts`、共用及语言子配置、全部内置 generator 的未知参数、合法别名与扩展参数。
- `cargo fmt --check`、`cargo lint`、`cargo test --locked` 通过；local profile 共 714 个测试（576 lib、88 CLI、5 architecture、45 其他 integration）。
- 对全部 lib / CLI tests 运行 LLVM coverage 并生成 / 检查 HTML。`code_artifact/mod.rs` 行覆盖 92.35%，`data_artifact/config.rs` 94.74%，`generator/input.rs` 89.68%，`meta/model/raw.rs` 93.84%，`project/mod.rs` 97.92%。本次新增的未知键拒绝和 nested sibling 拒绝分支均被执行；未覆盖行主要属于既有的 i18n snippet 错误、默认构造与 prepared input 防御分支，不作为此次行为修复扩展范围。
- 配置 Design、Project / Meta / codegen / data toolkit Implementation 已同步，Design HTML 已刷新；docs build/check 与 diff check 通过。

两项行为修复完成后，其余评估建议仍是独立的结构 / 性能优化。本轮不引入 cache、validation API 重构或兼容接口删除，避免扩大行为变更范围。
