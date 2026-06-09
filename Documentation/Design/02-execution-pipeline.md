# 02. 执行阶段设计

## 2.1 总体流程

ConfigForge 的一次执行以 Project 配置、Meta 文件、Source 数据和 Target 配置为输入，输出诊断、运行时数据、代码和辅助产物。

推荐总体流程：

```text
CLI Arguments
  -> Project Configuration Resolution
  -> Plugin Registry Initialization
  -> Meta Discovery / Import / Parse / Merge
  -> Source Declaration Resolution
  -> Source Loading
  -> Sheet Normalization and Field Mapping
  -> Full DataSet Materialization
  -> Target Tags Resolution
  -> ExportView Filtering
  -> Validation on Target View
  -> Export Planning
  -> Artifact Generation
  -> Manifest / Diagnostics Output
```

## 2.2 配置解析阶段

输入：

- CLI 参数；
- project config；
- environment variables；
- target selection；
- optional plugin config。

输出：

- resolved project root；
- selected meta roots；
- selected target list；
- selected target tags；
- output policy；
- diagnostic output policy。

要求：

- CLI 参数优先级高于 project 默认配置。
- 所有相对路径都应相对 project root 或声明文件所在目录解析，并在诊断中保留原始写法。
- 配置解析错误应尽量批量报告。

## 2.3 插件注册阶段

输入：

- project plugin manifest；
- CLI 指定的 plugin search path；
- 内置 provider/exporter/validator。

输出：

- SourceProviderRegistry；
- GeneratorRegistry（build-facing artifact generator facade；按 export type id 查询 generator requirements 与执行入口）；
- Data artifact toolkit facet（内部能力，用于复用 runtime table payload、data serialization helper 或 aggregate artifact helper；不作为 Build-facing registry）；
- Code artifact toolkit facet（内部能力，用于复用 CodeModel / writer helper / renderer；不作为 Build-facing registry）；
- ValidatorRegistry；
- BuiltinTypeRegistry。

要求：

- 内置能力先注册。
- 插件按配置声明顺序注册。
- 能力 ID 冲突默认报错；显式 `override = true` 才允许覆盖。
- 插件加载失败应包含路径、插件 ID、版本、API 版本和底层错误。

## 2.4 Meta 加载、Import 与合并阶段

Meta 支持多文件配置。一个 TOML 或 XML Meta 文件可以 import/include 其他 Meta 文件。

流程：

```text
Root Meta Files
  -> Resolve imports recursively
  -> Parse each document
  -> Merge declarations deterministically
  -> Link references
  -> Produce MetaModel
```

要求：

- import 路径默认相对当前 Meta 文件所在目录。
- import 图必须检测循环引用。
- 多文件中 table、enum、named struct、target、validator 等命名冲突必须报错，除非声明了明确的扩展/覆盖语义。
- 合并顺序必须稳定，不依赖文件系统遍历顺序。
- TOML 和 XML 都应映射到同一个 canonical MetaModel。

## 2.5 Source 加载阶段

Source Provider 负责把 CSV、XLSX、Lark Sheet、Google Sheet 等外部数据读取为统一的二维 Sheet。

要求：

- `csv` 和 `xlsx` 是本地文件 provider，不再额外设计 `local_file` provider。
- provider 的输出不解释业务类型，只保留 row、column、raw cell、source location。
- Source 加载可以并行，但诊断输出必须稳定排序。
- 一个 source 加载失败不应阻止其他互不依赖 source 的加载。
- Source Provider 涉及文件或网络 I/O 时应优先使用异步接口或操作系统 I/O 机制，避免为大量 I/O 等待创建阻塞线程。
- 计算密集型解析或校验任务可以使用线程池并行执行。
- 并发拆分既可以按任务类型拆分，也可以在合理粒度下按数据拆分，例如按 source、table、target 或 artifact 拆分。
- 并发执行不得改变语义结果、诊断排序和 manifest 排序。

## 2.6 Sheet Mapping 与 DataSet Materialization

该阶段根据 Meta 把二维 Sheet 转换为 typed logical data。

内存模型原则：ConfigForge 当前选择把 Project / Meta / DataSet 全量物化到内存，以换取更简单、可验证的 target filtering、validation、multi-target 共享和 artifact generator 输入模型。后续除非有明确生产数据证明该模型不可接受，否则不设计 row paging、out-of-core DataSet、按 target 重读 source 或跨阶段数据流式游标；大表输出的内存控制放在 artifact 边界，通过 `ArtifactSink` / `ArtifactWriteStream` 直接写文件，避免把完整 artifact 内容先收集成 `String` / `Vec<u8>`。

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

- Project Configuration Resolution、Plugin Registry Initialization、Meta merge/link 是后续阶段的前置依赖。
- 多个互不依赖的 Meta 文件解析可以并行，但 import 图解析、循环检测和 merge/link 需要在稳定顺序下汇总。
- 多个 source 可以并行加载；同一 source 内部是否按 sheet/range 并行由 provider 决定。
- Sheet Mapping、type parse、DataSet materialization 可按 table/source 分片并行，但最终 DataSet 组合顺序必须确定。
- 多 target 的 ExportView、Validation 和 Export Planning 可以在共享不可变 Full DataSet 的前提下并行。
- 多 artifact 可以并行生成；Artifact Writer 负责统一冲突检查、写入策略和最终提交。
- 所有阶段的 diagnostics 必须进入统一 collector，并按稳定 key 输出。

## 2.7 Target Tags 与 ExportView 阶段

Target 定义一组 `tags`，例如 `client`、`server`、`debug`、`shipping`、平台、地区、渠道或赛季。CLI 可以额外传入一次性 tags。ConfigForge 以 `target.tags ∪ CLI extra tags` 得到 effective target tags，并据此生成导出视图；tags 本身只是能力标签，工具不硬编码具体业务含义。

流程：

```text
Full DataSet
  -> apply table export_when
  -> select override_sources by export_when
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

ConfigForge 的功能性校验在 source 读取、组合、过滤后执行。对于 target 相关命令，校验对象是该 target 的 ExportView；对于未指定 target 的 validate 命令，默认校验完整 DataSet 或由 CLI 指定 context 派生的验证视图。

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

- 输出路径冲突必须在写文件前发现。
- 独立 artifact 可并行生成。
- 写文件应通过统一 Artifact Writer，支持临时文件和确定性 commit。
- 最终 manifest 按路径稳定排序。

## 2.10 诊断输出阶段

所有阶段应输出结构化诊断。

诊断排序 key：

1. severity；
2. source file / source id；
3. table name；
4. record key；
5. row index；
6. column index；
7. field path；
8. diagnostic code；
9. message。

CLI 应支持 human、json、jsonl、summary 等输出格式。
