# 02. 执行阶段设计

## 2.1 总体流程

一次 CLI 调用选择一个 Project Config 和一个 target，构建该 target 声明的产物。以下是语义依赖，具体分支与调度以 [Build 实现](../Implementation/10-build-module-detail-design.md)为准：

```text
CLI -> Project imports / merge -> selected target + effective tags
  -> builtin GeneratorRegistry / ValidatorRegistry
  -> raw Meta -> source plan + Meta link/validate
  -> all-task artifact path planning and reservation
  -> MetaOnly generation
  -> source read -> normalize -> target ExportView -> validation
  -> MetaAndData generation -> sink completion / commit -> diagnostics
```

Meta link 与 source read 可以重叠；全 target 规划是任何 generator 写出的前置屏障。MetaOnly generation 不等待 source read；它与数据分支的工作不代表新增 target 或重复读取 source。MetaOnly-only target 无需 project.input。

## 2.2 配置解析阶段

输入为 CLI --project / --target、Project 顶层 imports、target 配置和本次 --tag。一次执行只选择一个 target；工程输入与各 export task output 来自配置，CLI --report-output 仅影响附加报告。

相对 imports 以声明文件目录解析；Project input / output 等路径语义见 [配置设计](03-configuration.md)。配置错误使用现有诊断返回，不承诺所有解析错误都能批量报告。

## 2.3 内置能力注册与扩展边界

当前 build 使用 GeneratorRegistry 查询 artifact generator 的 type id、aliases 和 input requirements；ValidatorRegistry 编译显式 validator。Source provider 当前在 CSV/XLSX 枚举分发，不存在生产 SourceProviderRegistry 或 BuiltinTypeRegistry。

动态库插件、plugin manifest、CLI plugin search path、通用能力 override/版本协议尚未实现。它们属于未来扩展，不能出现在当前可运行配置或命令中。Data/Code Artifact Toolkit 是内部复用工具，不是另一套 build-facing registry。

## 2.4 Project imports 与 Meta 加载

Project Config 顶层 imports 递归导入同格式 TOML 片段，检测循环并稳定合并 Meta/targets；Meta 文件自身不声明 imports。Link 解析类型、index、FK 等引用并返回 linked view，raw MetaModel 保留 owned 声明。

当前只实现 TOML，不支持 XML，也不设计隐式 include 或未声明的 schema 扩展覆盖。具体重复声明与 merge 规则见 [Project 实现](../Implementation/03-project-module-detail-design.md)。

## 2.5 Source 加载阶段

本地 CSV/XLSX reader 读取物理表格并归一化为 raw DataTable；Lark / Google Sheet provider 尚未实现。

要求：

- `csv` 和 `xlsx` 是本地文件 provider，不再额外设计 `local_file` provider。
- provider 的输出不解释业务类型，只保留 row、column、raw cell、source location。
- Source 加载可以并行，但诊断输出必须稳定排序。
- 当前本地批次完成 join 后按 source 计划顺序选择错误，不承诺跨分支取消。
- 本地读取与 CPU 工作复用局部 Rayon pool；真实远端 provider 引入时再设计独立 I/O runtime 与限流。
- 计算密集型解析或校验任务可以使用线程池并行执行。
- 并发拆分既可以按任务类型拆分，也可以在合理粒度下按数据拆分，例如按 source、table、target 或 artifact 拆分。
- 并发执行不得改变语义结果、诊断排序和 manifest 排序。

## 2.6 Sheet Mapping 与 DataSet Materialization

加载阶段组合 raw 字符串 DataSet；normalize 应用默认值，Meta-aware Resolver 在验证视图中建立 typed resolved companion。

内存模型原则：ConfigForge 当前选择把 Project / Meta / DataSet 全量物化到内存，以换取更简单、可验证的 target filtering、validation 和同 target 多 generator 共享输入模型。后续除非有明确生产数据证明该模型不可接受，否则不设计 row paging、out-of-core DataSet、按 target 重读 source 或跨阶段数据流式游标；大表输出的内存控制放在 artifact 边界，通过 `ArtifactSink` / `ArtifactWriteStream` 直接写文件，避免把完整 artifact 内容先收集成 `String` / `Vec<u8>`。

职责：

- 解析 header 到 field path 的映射。
- 按 field type 解析 raw cell。
- 应用字段默认值。
- 组合 base `sources`。
- 按列表顺序应用 `override_sources`。
- 生成完整 `DataSet`。

明确规则：

- 校验面向所有 source 读取并组合后的最终数据视图，不要求每个 source 或 override source 单独满足 required。
- ObjectTable 的 override 以字段或 field path 为单位覆盖。
- CollectionTable 的 override 以整行 record 为单位覆盖，匹配依据是 `primary_index`。
- `override_sources` 可以声明 `export_when`；只有匹配当前 effective target tags 的 override source 才会被读取和应用。被过滤掉的 override source 不参与 source schema、required/default/type 或 index 校验。
- `override_sources` 后声明的 source 覆盖先声明的 override 结果。
- override row 默认必须匹配已有 base row；是否允许 insert 是未来策略，当前默认报错。
- Source sheet 允许存在空行和空列。
- 未映射到 field path 的行或列不进入 DataSet，可作为表格注释、中间计算或人工分隔使用。
- 如果某行/列部分映射到 field path，则只有映射字段参与解析；未映射单元格不产生字段值。

## 2.6.1 并发流水线约束

并发策略应服务于总体执行阶段，而不是改变阶段依赖关系。

推荐约束：

- Project resolution 和 raw Meta merge 是 source plan 的前置依赖；Meta link/validate 可与 source read 重叠。
- 多个互不依赖的 Meta 文件解析可以并行，但 import 图解析、循环检测和 merge/link 需要在稳定顺序下汇总。
- 多个 source 可以并行加载；同一 source 内部是否按 sheet/range 并行由 provider 决定。
- Sheet Mapping、type parse、DataSet materialization 可按 table/source 分片并行，但最终 DataSet 组合顺序必须确定。
- 当前一次 build 只处理一个 target；跨 target 共享 DataSet 尚未实现，不能忽略 tags 对 source selection 的影响。
- 多 artifact 可以并行生成；Artifact Writer 负责统一冲突检查、写入策略和最终提交。
- 所有阶段的 diagnostics 必须进入统一 collector，并按稳定 key 输出。

## 2.7 Target Tags 与 ExportView 阶段

Target 定义一组 `tags`，例如 `client`、`server`、`debug`、`shipping`、平台、地区、渠道或赛季。CLI 可以额外传入一次性 tags。ConfigForge 以 `target.tags ∪ CLI extra tags` 得到 effective target tags，并据此生成导出视图；tags 本身只是能力标签，工具不硬编码具体业务含义。

流程：

```text
已按 effective tags 选择并组合 base/override 的 DataSet
  -> apply table export_when
  -> apply field export_when
  -> apply row_export filters
  -> apply fragment filters
  -> ExportView
```

明确规则：

- ExportView 是目标实际导出的数据视图。
- 所有 target-aware 条件都使用 effective target tags，而不是仅使用 Project Config 中静态声明的 `target.tags`。
- 表、字段通过统一的 `export_when = { any = [...], all = [...], none = [...] }` 匹配 target tags。
- override source 通过同样的 `export_when` 匹配 effective target tags；例如 `materials_s1.csv` 可以只在 CLI 传入 `--tag s1` 时覆盖基础 `materials.csv` 中的 Oak 售价。
- `export = false` 是强制不导出；不再设计 `tool_only`、`validator_only`、`filter_field` 作为导出机制。
- collection 表可通过 `row_export` 指定数据列控制行级导出，常用于 `debug` / `shipping` 行裁剪。
- 被排除记录对应的 text、asset 等也默认不进入 POT / asset manifest 等聚合 data export 产物。
- 若某个聚合 data exporter 需要额外读取非 runtime 字段，必须由字段策略或 exporter policy 显式声明，但仍受 target tags 过滤影响。

## 2.8 Validation 阶段

ConfigForge 的功能性校验在 source 读取、组合、过滤后执行。普通 CLI 始终按选定 target 的 ExportView 校验；库另外提供 full DataSet 校验入口，不提供无 target 的 validate 子命令。

校验范围：

- required/default 结果；
- type parse 结果；
- enum 值；
- index unique / non-unique；
- primary index；
- foreign key；
- collection 级规则；
- asset / text 策略；
- target type mapping；
- output path conflict。

特别规则：

- 集合级检查默认只检查过滤后的目标数据视图。例如 shipping 过滤掉的测试行，不参与 shipping 集合唯一性、资源清单、国际化模板等检查。
- required 字段如果缺失但配置了 `default`，使用 default 后不报错。
- required 字段无值且无 default 时，针对当前验证视图报错。

## 2.9 Export Planning 与 Artifact Generation

导出前必须先规划产物：

- runtime data files；
- generated code；
- special artifacts；
- schemas；
- manifests。

要求：

- Meta link 后必须规划所有 export task 的完整输出路径，通过共享 ArtifactSink 统一检查并预留；输出路径冲突必须在任何 generator 写文件前发现，覆盖 MetaOnly / MetaAndData 跨分支及附属文件。
- 独立 artifact 可并行生成。
- 写文件应通过统一 Artifact Writer，支持临时文件和确定性 commit。
- 最终 manifest 按路径稳定排序。

## 2.10 诊断输出阶段

各阶段逐步使用 shared diagnostics；仍存在兼容错误桥接，不保证每个底层错误都有完整 table/cell 定位。

诊断按模块稳定 key 排序并合并；以下是定位维度，不要求所有模块强制使用同一 comparator：

1. severity；
2. source file / source id；
3. table name；
4. record key；
5. row index；
6. column index；
7. field path；
8. diagnostic code；
9. message。

CLI 当前以 --diagnostic-format 选择 human 或 JSON 失败诊断；jsonl / summary 不属于当前选项。人类阅读的文件报告通过 --report 或 validation_report export task 输出。
