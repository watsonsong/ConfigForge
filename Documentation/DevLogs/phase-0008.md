# Phase 0008：逐项架构简化与验证

## 时间与范围

2026-10-30。依据 [phase-0007](phase-0007.md) 的评估，按用户要求逐项重新判断、修改、验证与提交；本记录描述这次实施结果，不覆盖旧评估快照。

## 提交与决策

| Commit | 修复 | 开始前的重新判断 |
| --- | --- | --- |
| f0e0817 | 恢复 Windows / 跨语言测试基线，处理路径与换行、Java UTF-8、TypeScript lint | 完整测试存在既有失败，先建立可用基线，再评估重构结果 |
| 76ac9dd | syn 架构 guard 与跨平台核心 integration CI | 字符串截断不能证明依赖边界；使用测试专用 AST visitor，无需完整编译器分析 |
| 2f9e21d | CSV provider 委托带路径检查的 DataLoader | 公开 provider / shell 兼容性需要保留，生产本地读取继续使用 Rayon |
| 020778b | Validation 共用 report 收集与 full / target 内核 | 保留外部入口和语义差异，只合并已确认相同的内部流程 |
| 6cff348 | Generator 统一 planning / generate 与有效输入状态 | 全 generator 产物规划已覆盖，直接使用 common planning 与 typed execution input 即可，无需另一套 trait |
| 01a448d | 单 target 计划仅保留两类任务 indices | 当前无 multi-target cache 消费者，移除单元素 Vec、恒定字段和未消费 key |
| 23568a7 | 共享代码文件头与 streaming 写出 | 语言模型有实际差异，只提取相同的写入操作，不引入模板 DSL |
| 376d36a | SchemaDoc 仅按可见表名规划路径；共享 runtime table plan | 路径 helper 已能消除主要重复，无需通用 prepared-result cache；文件系统复查继续保留 |
| 8c49196 | 移除 AggregateDataExporter 重复 trait | 统一 generator 已足够；旧 aggregate trait 无共享算法且不是公开扩展入口，保留逐表 RuntimeTableFormat |
| ca0cf79 | 收敛任务技术文档、修正文档与源码偏差 | 前述简化已落地，删除另一套未来 API 草案，保留约束、扩展触发条件与历史证据 |
| 后续排序清理提交 | 去掉 source read、MetaOnly 与 Data generator indexed collect 后的重复排序 | 文档收敛后回查发现三处遗漏；输入有序且 indexed collect 保序，跨分支重新合并与无序结果排序仍保留 |

每一项均在前一项测试通过并提交之后开始，分别运行 cargo fmt、完整 cargo test 与 cargo lint。未把所有实现合并为一次提交。

## 保留的功能与性能边界

- 保留全部内置 generator、type ID / aliases、格式选项和公开 API。
- 保留 MetaOnly 与 source read 重叠、局部 Rayon pool、target tags / source override selection、稳定 report 与错误优先级。
- 保留全 target 规划屏障、filesystem-aware 路径检查、streaming、dry-run、direct、temp-commit 和失败报告导出。
- 移除的内存产物入口仅属于内部旧 facade；Lua 内容测试仍有测试专用入口。
- 未引入通用 DAG、远端 runtime、跨 target cache 或全量产物内存缓冲。没有执行性能基准，不把代码减少表述为已测得的速度提升。

## 文档权威与历史迁移

当前资源策略由 [任务系统技术设计](../Tech/task-system-design.md) 描述；拓扑由 [Build 实现](../Implementation/10-build-module-detail-design.md) 描述；路径语义由 [平台文件系统语义](../Tech/platform-filesystem-semantics.md) 描述。

任务系统旧文档的 Phase 0–12、TaskKey / ExecutionContext / async provider API 草案和迁移建议，可在 Git 基线 e50f8b8 的 Documentation/Tech/task-system-design.md 查阅。Build 的“阶段 A 实施步骤”在同一基线的 Implementation/10 文档中保留。本轮不复制旧全文，避免再次形成第二套权威设计。

修正了以下现状偏差：

- Artifact 已拆分为 model / path / writer，不再是“建议新增目录”。
- Build 的 pipeline / artifact_plan / plan / trace 已拆分，不再由 mod.rs 承担全部职责。
- 路径 key 已感知文件系统别名，不再写作仅 lexical 且计划未来实现。
- io / network concurrency 参数尚未控制生产调度；AsyncSourceReadShell 是未接入主流程的 blocking 兼容 shell。
- 文档使用当前 RuntimeTableFormat 与具体 generator helpers，移除不存在的聚合 facet 接口草案。

## 验证快照与限制

文档清理与后续排序清理各自的 cargo fmt、完整 cargo test、cargo lint 与 cargo clippy --all-targets -- -D warnings 均通过；相对链接与 git diff --check 也通过。

排序清理复用现有 source 乱序完成 / 首个计划错误测试、target 分支顺序测试与 CLI 串并行一致性测试，不为标准库和 Rayon 的保序契约新增镜像测试。

最终测试集包含 555 项 lib 单元测试、4 项架构测试、64 项 CLI integration，以及 runtime compliance / metadata 等测试。相对上一项减少的一项 lib 测试只验证已删除 aggregate trait 的默认 legacy fallback；POT / Lua / Asset Manifest 内容、sink 和全 generator 写入模式测试继续保留。

本地 Rust 与 Java runtime harness 可运行；缺少的可选 C++ / TypeScript / Lua 等环境按 local profile 显式 skip。不能据此声称全部语言和 GitHub runner 均实际验证。CI workflow 已补入口，但本轮未 push 或等待远端 workflow。

未安装 cargo-llvm-cov，本轮没有覆盖率百分比结论；通过定向回归与现有集成测试验证受影响行为。
