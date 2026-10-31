# 05. Data 模块详细设计

相关文档：[Source 配置](../Design/03-configuration.md#34-source-declaration)、[数据视图与校验](../Design/05-data-validation.md)、[Struct / Expanded Value](12-struct-expanded-value-detail-design.md)。本文描述当前实现，不维护已经完成的迁移步骤。

## 5.1 职责与边界

`data` 负责 source 计划、CSV/XLSX 读取、多源组合、来源追踪和 Meta-aware resolved value。依赖 `meta`、`diagnostic`，不依赖 `validation`、`generator` 或 `build`。Reader 解释物理表格；Resolver 解释 linked Meta、类型、default、fragment 与 target visibility。

```text
raw Meta + effective target tags
  -> SourceReadPlan
  -> DataLoader -> RawSourceData / DataSet
  -> validation::normalize_data_set_with_pool
  -> ExportValidationView + DataResolver::resolve_table_rows
  -> validation / data-backed generators
```

Source read 可与 Meta link 并行；normalize 和 resolve 等两者完成。调度屏障、失败顺序和共享范围见 [Build 实现](10-build-module-detail-design.md)。

## 5.2 当前模块结构

| 文件 | 职责 |
| --- | --- |
| [mod.rs](../../src/data/mod.rs) | 私有模块声明和选定 API re-export。 |
| [plan.rs](../../src/data/plan.rs) | owned source 计划、provider、layout 和兼容 DataLoadPlan。 |
| [loader.rs](../../src/data/loader.rs) | 路径检查、reader 分发、多 source / override 合并与稳定顺序。 |
| [csv.rs](../../src/data/csv.rs) | CSV record/header 与 ObjectTable key/value 读取。 |
| [xlsx.rs](../../src/data/xlsx.rs) | calamine、sheet 选择、稀疏物理坐标、公式缓存与 workbook cache。 |
| [model.rs](../../src/data/model.rs) | Raw DataSet / DataTable / DataRow 与 row/cell 来源。 |
| [resolved.rs](../../src/data/resolved.rs) | ResolvedTable / ResolvedValue / DataValue 和值来源。 |
| [resolver.rs](../../src/data/resolver.rs) | scalar、struct、fragment、list、color/vector 的结构化解析。 |
| [typed_value.rs](../../src/data/typed_value.rs)、[value_conversion.rs](../../src/data/value_conversion.rs) | 共享 typed parse 与 JSON/TOML/cell 转换。 |
| [provider.rs](../../src/data/provider.rs) | 保留的 SourceProvider / blocking shell 兼容接口；不用于生产 build。 |

## 5.3 加载计划

`SourceReadPlan` 从 raw Meta 或 tables + effective tags 构造，不借用 linked Meta。每个 `DataSourcePlan` 保存 table、provider、path、可选 sheet、header/data 行号、layout、fragments、base/override role 和覆盖匹配字段。

- `DataProvider` 仅 `Csv` / `Xlsx`；字符串 source 始终是 CSV 简写，XLSX 必须显式指定 provider。
- source path 受 data root containment 检查；行号为 1-based。
- 不可见 table 和 inactive override 在计划阶段排除，不访问其文件。
- `DataLoadPlan` 包装 SourceReadPlan，兼容仍返回 DataSet 的 `load*` 入口。
- 通用 CollectionTable split merge、base source `export_when`、override insert/upsert、fragment join 和远端 provider 不支持。

## 5.4 Raw 与 Resolved 模型

Raw 数据保留源文本、诊断位置与显式 key policy：

- `DataRow = BTreeMap<String, String>`，field path 保持扁平。
- DataTable 包含 rows、等长 row_sources / cell_sources；ObjectTable 的 key/value 多行合成一条 logical row。
- RowSourceInfo 保存 source index/path/sheet/物理 row/fragments/role 和可选 replaces；CellSourceInfo 保存每个 field path 的物理行列与 column name。
- RawSourceData 包装 owned DataSet，不持有 Meta 引用。

Resolved 数据提供共享类型解释：

- ResolvedTable 保存与所选 raw rows 对齐的 structured rows 和 row sources。
- ResolvedValue 包含 DataValue 与 ValueSourceTrace；DataValue 支持 typed scalar、struct、array、color、vector，scalar 支持 string/int/float/bool/enum。
- ValueSourceTrace 保存 field path、显式值/default/空 object 的种类和可选 source index/path/row/raw value；sheet 和精确列由 raw provenance 补齐，不在此重复存储。
- 生产按表解析，在 ExportValidationView 缓存 resolved companion。整份 ResolvedDataSet 与 dataset-level resolver 仅作 `#[cfg(test)]` 辅助，不是生产 public API。

Validation 和 generator 优先读 resolved values；FK、unique/index、record key 遵守 [key policy](../../src/validation/key_policy.rs)，不能隐式改成 typed key canonicalization。

## 5.5 CSV/XLSX reader

CSV 保持既有 record/header 语义。CollectionTable 默认 header 第 1 行、data 第 3 行起；ObjectTable 默认 `key,value` / `key,title,value`，data 第 2 行起，title 仅供人工说明。起点、空行列与参数约束见 [Source 配置](../Design/03-configuration.md#34-source-declaration)。

XLSX 使用 calamine，不依赖 Excel，不生成临时 CSV：

- 多 sheet 必须指定名称，唯一 sheet 可省略；保留真实 sheet 和物理行列，偏移起点不重编号。
- 跳过空 header 列和整行空值；文本保留前导零，bool/有限数字转为与区域无关的文本，不使用显示格式。
- 日期/时长要求业务文本；Excel error 与不支持的值报错。
- 公式只读缓存，不计算；缺缓存/缓存错误报错，缓存空字符串合法。

DataLoader 每次 `read_sources*` 创建 workbook cache，独立 `load_source` 使用一次性 cache。同一物理工作簿按 canonical path 复用打开结果并用单独 Mutex 保护，不同工作簿可并行。cache 不跨 build。三方库选择和版本约束只在 [开发环境](../Tech/development-environment.md#xlsx-读取库)维护。

## 5.6 多源与覆盖

CSV/XLSX 在 DataTable 边界汇合，共用合并规则：

| 场景 | 当前行为 |
| --- | --- |
| CollectionTable 多 base | 仅 polymorphic variant append；每个 source 提供完整 record，按声明顺序追加，合并后校验全局主索引。 |
| CollectionTable override | 按 primary index 匹配既有 row，整行替换；不 insert/upsert/join。 |
| ObjectTable 多 base | 互斥 field-path split merge 为 singleton object。 |
| ObjectTable override | exact field path patch 已存在字段，保留其他字段。 |

后声明 override 覆盖前者。结果保留最终 cell 来源和替换 row trace，不能仅保存整个逻辑表的一个文件名。

## 5.7 来源与诊断

DataTable 的 `enrich_location` / `enrich_error_location` 补齐 shared DiagnosticLocation；嵌套 path 优先匹配最长 source path，ObjectTable 定位到每个 field 的 value cell。source_sheet 独立于文件路径，JSON 缺省省略。

CSV 语法/I/O 使用 ParseCsv；XLSX、source schema、resolver 使用现有 shared diagnostic code，不引入另一套 DataDiagnostic。稳定 code 以 [catalog](../../src/diagnostic/code.rs)和 [Diagnostic 设计](02-diagnostic-module-detail-design.md)为准。

## 5.8 并发与兼容边界

本地读取、normalize 和 CPU 工作复用 build 局部 Rayon pool，由 jobs 控制；结果按 plan 顺序合并，完成时间不改变 override 或错误顺序。

SourceProvider、LocalCsvSourceProvider、AsyncSourceReadShell 和 RemoteProviderPolicy 保留兼容；shell 是 blocking scoped-thread 实现，未接入生产 build，也未执行远端 retry/backoff/rate-limit。扩展触发条件见 [任务系统](../Tech/task-system-design.md)。

## 5.9 测试边界

- [Data tests](../../src/data/tests.rs)：计划、CSV、ObjectTable、merge/override、resolver、typed value 与错误来源。
- [XLSX tests](../../src/data/xlsx/tests.rs)：sheet、稀疏坐标、值/公式、混合 merge/override、构建间刷新。
- [Validation tests](../../src/validation/tests.rs)：required/default、可见性、struct/list/fragment、index/FK 和诊断。
- [CLI integration](../../tests/cli_integration.rs)、[XLSX integration](../../tests/cli_integration/xlsx.rs)：真实工程、写入模式、串并行等价、报告与定位贯通。

命令、profile 和 coverage 要求见 [测试矩阵](../Tech/test-ci-matrix.md)、[代码规范](../code-style.md)，不保留单次测试数字。
