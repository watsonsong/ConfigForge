# 任务系统与并发执行技术设计

本文是当前资源控制、并行执行约束和扩展决策的权威说明。当前实现使用单 target 的阶段式 branch pipeline 和局部 Rayon CPU pool，不包含通用 DAG executor 或异步 I/O runtime。

## 1. 文档边界与当前状态

| 主题 | 权威文档 |
| --- | --- |
| 产品执行阶段与输入依赖 | [执行流程](../Design/02-execution-pipeline.md)、[导出与 CLI](../Design/06-export-cli.md) |
| 当前模块、分支拓扑、规划屏障、错误顺序、trace 字段 | [Build 实现](../Implementation/10-build-module-detail-design.md) |
| SourceReadPlan、CSV、RawSourceData、DataSet | [Data 实现](../Implementation/05-data-module-detail-design.md) |
| report 收集、target view、validator 与 error gate | [Validation 实现](../Implementation/06-validation-module-detail-design.md) |
| sink 状态、stream 生命周期与提交 | [Artifact 实现](../Implementation/07-artifact-module-detail-design.md) |
| 大小写、symlink / junction、路径别名与 root containment | [平台文件系统语义](platform-filesystem-semantics.md) |
| 环境依赖、profile 与缺失工具策略 | [开发环境](development-environment.md)、[测试与 CI 矩阵](test-ci-matrix.md) |

本文不再复制这些模块的接口草案或阶段迁移步骤。早期 executor / async / 分阶段改造讨论保留在 Git 历史；本轮收敛依据见 [phase-0008](../DevLogs/phase-0008.md)。

当前实现边界：

- 一次 build 处理一个 target；同一 target 的 data-backed generators 共享一次 source read、normalize 和 validation。
- 生产本地 CSV 通过 DataLoader 在同一个局部 Rayon pool 中读取，后续 CPU 工作复用该 pool。
- MetaOnly 分支与 source read 可以重叠执行；任何 generator 写出前，所有 export task 必须完成全 target 路径规划与预留。
- generator 统一使用 ArtifactGenerator，按 needs_data 分组；validation_report 可在 validation error 时通过 report-only 输入写出。
- Meta 与 DataSet 当前驻留内存；产物使用 streaming 写出，不代表输入已支持分页或 out-of-core 处理。

## 2. CLI 资源控制

实际默认值、覆盖和校验以 [options.rs](../../src/build/options.rs) 为准。

| 参数 | 默认值 | 当前作用与合法范围 |
| --- | --- | --- |
| --jobs | available_parallelism 限制到 1..=4；查询失败取 1 | 局部 CPU pool worker 数；显式值须在 1..=128 |
| --serial | false | 使用单 worker，覆盖显式 --jobs；归一化后执行数值校验 |
| --trace-tasks | 不输出 | 写出阶段级 JSON trace；具体字段与失败行为见 Build 实现 |

CLI 只暴露实际生效的 worker 控制。--io-concurrency / --network-concurrency 已删除，旧调用会报未知参数。公开 Rust ExecutionOptions::from_cli 仍保留两个可选资源参数；默认值先按默认 jobs 计算，再应用库调用的显式覆盖。--serial 仍将三个库资源字段归一化为 1，以保持兼容。

BuildTrace 保留 io_concurrency / network_concurrency 字段，普通 CLI 调用使用库默认值，--serial 时为 1。本地 CSV 并发实际由 jobs 控制。当前不能依据 trace 中的 io_concurrency / network_concurrency 推断存在独立 I/O semaphore 或 network lane。

## 3. 并行执行与资源约束

### 3.1 局部 CPU pool

每次 build 创建并复用局部 Rayon pool，不更改全局 Rayon pool，也不为每个 export task 或 table 创建新 pool。

CPU 工作主要以 source、table、export task 为粒度：包括本地读取、normalize、target view、table validation、路径规划与生成。任务由局部 pool 调度；嵌套并行继续使用同一个 pool，不把每行、每字段默认变成独立任务。

生产 CSV 路径不为每个 source 创建 OS thread。未来若引入网络 I/O，应使用独立、受限的 I/O 执行边界，避免 CPU worker 阻塞等待远端请求或在 CPU worker 内创建额外 runtime。

是否改变并发粒度应由小表、多表、大表的耗时与峰值内存测量决定。不能只以任务数量更多作为优化依据。

### 3.2 所有权与重入

- LinkedMetaModel 借用原始 Meta；scoped 执行必须让借用数据活得足够久，无需为了并行复制整份 Meta。
- SourceReadPlan 是可独立读取的 owned source 计划，在 raw Meta 与 effective tags 可用时构造；不依赖 linked Meta。
- DataSet 与 linked view 在执行阶段共享只读借用，workers 返回局部结果与 report 后稳定合并。
- generator / table format 实现需要 Send + Sync、可重入；不得在实例上保存某次 build 的可变业务状态。
- 多 target 共享 DataSet 尚未实现。若将来加入缓存，必须区分 project input、effective tags 及 source override selection，不能只按 input path 复用。

### 3.3 Provider 兼容边界

LocalCsvSourceProvider 委托 DataLoader，共用 CSV layout 和 root/path 检查。公开的 SourceProvider、AsyncSourceReadShell、RemoteProviderPolicy 入口继续保留。

AsyncSourceReadShell 当前是 blocking scoped-thread 兼容 shell，未接入生产 build；名称不表示已运行 async runtime。RemoteProviderPolicy 当前只定义配置与默认值，尚无 retry / backoff / rate-limit 执行消费者。

新增远端 provider 时，先确定真实 transport、认证、请求配额、超时和取消需求，再选择 runtime 与资源控制实现；不提前维护另一套未执行的 provider / executor API。

## 4. 确定性与失败策略

并行执行改变完成时间，不改变对外可观察顺序：

- source 合并按计划顺序，normalize 按原 DataSet table 顺序合并。
- indexed Rayon iterator 的 collect 保留输入顺序，不额外包装 index 后重复排序；来自无序集合或完成队列的结果仍须稳定排序。
- 路径规划按 export task 声明顺序合并，ArtifactSink entries 按 normalized path 输出。
- validation 收集完整 report 后按稳定 key 排序，再决定 warning / error gate，不能退化为遇到第一个错误即返回。
- LinkMeta / 全局规划 / MetaOnly generation 所在左支与 source read join 后，先返回左支错误，再返回 source 错误。具体规则与测试由 Build 实现维护。

当前没有通用 cancellation token 或任务级重试。已启动的本地 source read 会在 join 时完成，即使另一分支失败。未来协作取消必须说明哪些任务可以停止、哪些诊断需要保留、临时产物如何清理，再决定接口。

## 5. 产物与内存

全 target 路径预留是执行屏障，不能为提前写出某一类产物而绕过。sink 在 reserve_paths 与 create 两处检查路径，分别保护规划阶段和实际打开流时的状态；两处检查不作为重复计算删除。

路径 key 已使用文件系统语义处理别名，并非仅 lexical normalization。平台限制与剩余竞争窗口见平台文件系统语义文档，本文不再维护另一套路径规则。

生产生成器直接写 ArtifactSink / ArtifactWriteStream；避免先收集整个文件或所有产物的 String / Vec<u8>。小型表达式、结构化 payload 与生成器的业务聚合仍可使用内存模型。

三种写入模式共享规划与序列化：

- dry-run 执行完整业务流程并统计 bytes，不写最终文件。
- direct 直接写最终文件，后续失败可能留下已经写出的内容。
- temp-commit 先写临时文件，再逐文件提交；提交失败可能留下已成功提交的文件，并非全工程事务。

不能为了减少分层而移除 stream finish、错误状态记录、临时文件清理或严格计划完成检查。

## 6. 观测与验证

当前 BuildTrace 记录阶段名称、相对开始时间、耗时、成功/失败与 detail；不提供 node-level critical path、resource wait、per-generator span 或 cancellation propagation。

并发与准备流程修改至少验证：

1. jobs 1 / serial 与多 worker 的文件内容、路径集合、report 顺序一致。
2. MetaOnly target 无需 project.input；mixed target 的 MetaOnly generation 不等待 source read。
3. 全内置 generator 在三种写入模式下共享完整计划，跨分支冲突在写文件前失败。
4. source layout、target tags、override selection、validation warning / error、report-only 失败导出保持原语义。
5. stream 失败与 drop、temp-commit 清理、sink 未完成计划均被拒绝。

命令与 profile 仅由测试与 CI 矩阵维护。涉及语言后端时运行对应 runtime harness；缺少可选工具必须显式 skip，full / nightly 的必需工具缺失按该矩阵失败。没有运行的语言或平台不能表述为已验证。

调整并发、引入缓存或改变内存模型前，再记录大表耗时与峰值内存；普通入口去重不以新增通用 benchmark / executor 框架为前提。

## 7. 扩展触发条件

| 需求 | 重新评估的设计 |
| --- | --- |
| 真实远端 Sheet provider | async runtime、独立 I/O / network 配额、timeout、retry、rate-limit 与协作取消 |
| 同次构建多个 target 并共享准备结果 | 包含 tags / source selection 的依赖 key、缓存生命周期与失效 |
| 阶段式 pipeline 无法表达的依赖或调度 | 小型任务图模型、失败传播与调度成本；按证据决定是否需要 DAG executor |
| 大输入导致可测的内存瓶颈 | 分块读取、分页 DataSet 或磁盘中间表示，以及 FK / index 的跨块语义 |
| 阶段级 trace 无法定位实测瓶颈 | 增加所需 span / metrics，避免提前承诺未采集的数据 |

这些能力当前均未实现。没有相应需求时，继续使用现有 branch pipeline、统一 generator 和局部 pool。
