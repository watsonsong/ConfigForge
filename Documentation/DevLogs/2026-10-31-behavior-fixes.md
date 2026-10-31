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
