# 整体设计评估（2026-10-31）

这是 diagnostics 修复后的源码审查快照，不替代当前模块设计。后续结构方向见 [Implementation 大纲](../Implementation/01-module-outline.md#17-后续结构简化方向)。本次未修改 Rust 源码，也未将审查建议视为已完成实现。

## 总体判断

核心分层合理：Project / Meta / Data / Validation / Generator / Artifact 的依赖单向；统一 generator 支持 mixed artifact；CodeModel 隔离语言后端；共享 sink 集中路径安全；局部 Rayon pool 避免第二套生产执行器。主要复杂度来自重复准备、同类语义的多个消费者独立遍历，以及未退出的兼容入口。

## 1. 优先修复：普通 struct 内 Asset 被遗漏

源码证据：

- [validation/asset.rs](../../src/validation/asset.rs)：普通 table field 直接进入只接收 primitive Asset 的 helper；递归 helper 主要由 typed fragment 路径调用。
- [build/runtime.rs](../../src/build/runtime.rs)：canonicalize_asset_refs 以同样方式遍历，普通 struct child 未进入补齐。
- [asset_manifest/extract.rs](../../src/generator/asset_manifest/extract.rs)：Manifest 也有同类分支。

用临时目录构造合法 Project、CSV 和真实的 `UI/Icon.png`。同一 table 同时包含 primitive Asset `direct` 与普通 struct `Visual` 的 Asset child `visual.icon`。两者使用相同 kind（base_dirs = ["UI"]、extensions = [".png"]、relative path、required extension）。CSV 使用 expanded header `direct,visual.icon`，两值均为 `Icon.png`。

运行当前 debug CLI 后构建成功，JSON 中 direct 是 `UI/Icon.png`，visual.icon 仍是 `Icon.png`；Manifest 只记录 direct。再把 visual.icon 改为不存在的 `Missing.png`，构建仍成功且 diagnostics 为空，错误路径被导出。已直接复现 expanded 普通 struct；JSON cell、typed/inline fragment 和 default/override 的组合属于修复时必须扩展的测试维度，不能声称本次已逐一复现。

应先修正值遍历覆盖，再复用遍历逻辑。遍历时显式携带字段 Meta、field path、resolved value、source trace 和 target visibility；让校验、补齐、Manifest 消费同一规则。不要为每个消费者复制另一套递归。

## 2. 优先修复：未知配置键被静默忽略

Project / Meta 的多数 serde 结构没有 deny_unknown_fields，TargetConfig 已严格拒绝；generator 的专属 parser 与 codegen 的动态 language_options 处理也不一致。临时 Project 验证了两组拼写错误：field 的 `requird = true`、Rust export 的 `ruts = { derive_serde = true }`。两次 dry-run 均成功且 diagnostics 为空，配置没有达到调用者的意图。

应建立一致边界：封闭 schema 拒绝未知键；export 的动态 options 先消费共用选项，再仅接受当前 generator 允许的扩展区域；validator params 保持注册式解释并检查未知项。不能一概对 flatten 类型加 serde 属性，也不应为了容纳拼写错误引入全局宽松模式。回归应包含未知外层键、未知语言子键、合法 alias、imports 和 validator 扩展。

## 3. 低风险简化：删除由输入重新构造的 task output

[runtime.rs](../../src/build/runtime.rs) 的 RuntimeExportTaskOutput 只保存 export_name。实际 generator 执行结果是 Result<()>，成功后 stable_task_outputs 又从输入 export_tasks 构造名称列表；[pipeline.rs](../../src/build/pipeline.rs) 随后检查数量和名称是否与同一输入匹配。这不能验证实际产物，真正的产物完整性由 sink 预留和 finish 负责。

可让该层只返回真实 diagnostics / 业务 metadata，删除名称列表和配对校验。保持 indexed parallel collect 的错误顺序，不删除 sink 的计划与实际写出检查。多层 pipeline context 可合并重复转交，但不应变成贯穿所有模块的万能 context。

## 4. 收益较高：复用 resolved data 与 Asset 解析结果

[validation/context.rs](../../src/validation/context.rs) 已构造 ResolvedTable，validation 返回后 view 被释放；[payload.rs](../../src/data_artifact/payload.rs)、binary、POT、Asset Manifest 又各自调用 DataResolver。JSON + MessagePack + binary 等同一 target 多格式会重复解析。build 的 canonicalize_asset_refs 即使没有 asset kind 也 clone 整个 DataSet；有 Asset 时又新建 resolver 并重新执行检查。

最小切入点是移除无修改时的全量复制，并在当前 build 生命周期中保留可共享的 target 解析结果。验证时仍需 raw 数据、hidden support field、raw index/FK key 和错误收集；不能直接用仅保留导出字段的 payload 替代 validation 输入。逐步让 exporter 读取共享结果，保持现有输出与原始来源。需要多表 / 大表 / 多格式测量，不能仅凭源码给出提速比例。

## 5. 收敛 validation 的接口与失败状态

[entry.rs](../../src/validation/entry.rs) 已有 14 个 pub 函数组合 raw/normalized、full/target、assets、report/outcome、pool；未来增加开关会继续组合增长。现有 wrapper 较薄，不必立即删除公开 API，但内部应使用单一请求 / scope 与统一 outcome，旧入口转发。

[context.rs](../../src/validation/context.rs) 的串行 for_target 与并行 target_table_view 重复构造；resolved_table_for 用 `.ok()` 把解析失败变成 None，随后 [table.rs](../../src/validation/table.rs) 通过 raw fallback 和可能的二次 resolve 补救。这不是“错误一定丢失”，但状态含义不明确且维护两条路径。应显式保留 resolve outcome，逐步共享 row/value 校验；保留继续收集多错误的行为。

## 6. 隔离兼容调度器和空转配置

[provider.rs](../../src/data/provider.rs) 保留 AsyncSourceReadShell、资源 lane、RemoteProviderPolicy，生产 CSV/XLSX build 使用 Rayon。该 shell 实际使用 blocking scoped threads，policy 无 retry/rate-limit 执行消费者；[ExecutionOptions](../../src/build/options.rs) 中 io/network 也不是独立生产调度限流。它们属于额外公开兼容表面，不能当成已完成远端能力。

先审计库调用方，再选择兼容模块、弃用或下一次破坏性版本移除。不要为了让这些类型“有用”提前接入 async runtime。新远端 provider 出现时依据真实 transport 重新设计。

## 7. Codegen 的准备与组织

[input.rs](../../src/generator/input.rs) 在 planning / generation 重建 CodegenTaskConfig 和 CodeExportTarget，language_options 多次反序列化。可在任务局部准备 typed options 并共享给路径规划与渲染；不要引入全局注册器缓存或过度泛型。

[code_artifact/mod.rs](../../src/code_artifact/mod.rs) 的 for_language/defaults 保留另一套语言默认值，主要由测试使用；生产默认值来自具体 CodeGenerator。测试应尽量复用正式配置准备，避免两套默认值一起变更而掩盖漂移。CodeExportInput.runtime_data_files 生产总为空，build_code_model 对该参数已不使用，但 runtime manifest 推荐仍有测试消费者；应先拆清需求，不能直接删除所有 RuntimeDataFile。

Java / C# 的 mod.rs 集中 options、runtime、reader、repository、render，明显长于其他后端的分文件组织。按现有后端职责拆分即可，跨语言差异继续留在具体后端；不建议建立通用代码模板 DSL。

## 不建议做的重构

- 不为单 target 构建引入通用 DAG executor、全局缓存、插件运行时或多 crate workspace。
- 不把所有基础校验强制迁入 registry，仅为接口形式统一。
- 不合并 MetaModel、CodeModel、ResolvedValue 和 DesignerReport；它们分别服务配置、代码生成、数据语义和展示。
- 不把重复的安全检查当成无效计算：路径规划屏障、create 时的文件系统检查和 finish 完整性仍有不同职责。
- 不为了减少行数直接删除公开兼容 API，或把共享 diagnostics 和 enrichment 全部塞回 message。

## 验证与记录

本次使用现有 debug CLI 在系统临时目录复现两组 Asset 输入和两组未知配置输入；未修改仓库 fixture。文档变更运行 docs:build / docs:check，刷新 Design HTML。没有修改 Rust，因此不重复运行上一轮完整测试，也不将静态审查结论当成性能测量结果。
