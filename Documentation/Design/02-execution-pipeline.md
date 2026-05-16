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
  -> Target Context Resolution
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
- selected context tags；
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
- DataExporterRegistry；
- CodeGeneratorRegistry；
- ValidatorRegistry；
- BuiltinTypeRegistry；
- SpecialExporterRegistry。

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

## 2.6 Sheet Mapping 与 DataSet Materialization

该阶段根据 Meta 把二维 Sheet 转换为 typed logical data。

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
- `override_sources` 后声明的 source 覆盖先声明的 override 结果。
- override row 默认必须匹配已有 base row；是否允许 insert 是未来策略，当前默认报错。

## 2.7 Target Context 与 ExportView 阶段

Target 定义默认 tags，CLI 也可以额外指定 tags。ConfigForge 根据 target/context 生成导出视图。

流程：

```text
Full DataSet
  -> apply record filters
  -> apply field filters / export policy
  -> apply fragment filters
  -> ExportView
```

明确规则：

- ExportView 是目标实际导出的数据视图。
- shipping 等上下文排除的记录，不进入常规 runtime data。
- 被排除记录对应的 text、asset 等也默认不进入 special exporter 产物。
- `tool_only`、`export = false` 字段默认不进入常规 runtime data。
- 若 special exporter 需要使用非常规字段，必须通过 `special_exports` 或 exporter policy 显式允许，但仍受 target/context 过滤影响。

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

