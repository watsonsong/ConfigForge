# 06. Validation 模块详细设计

相关文档：[校验语义](../Design/05-data-validation.md)、[内置 validators](../Design/09-builtin-validators.md)、[诊断](02-diagnostic-module-detail-design.md)、[FK 实现](11-foreign-key-detail-design.md)、[Designer Report](../Design/10-designer-validation-report.md)。

## 6.1 职责与当前结构

validation 依赖 meta、data、diagnostic 和 asset resolver，不依赖 generator、artifact toolkit 或 build。负责 normalized data、target/full view、基础规则、显式 validator、报告模型与渲染；build 负责调度和失败后报告写入。

| 文件 | 当前职责 |
| --- | --- |
| [entry.rs](../../src/validation/entry.rs) | full/target 入口、串并行 report 收集、FK/asset/registry 顺序和 error gate。 |
| [context.rs](../../src/validation/context.rs) | ValidationContext / ExportValidationView / ExportTableView / VisibleRow。 |
| [defaults.rs](../../src/validation/defaults.rs) | table-local default 与 normalize，pool 版本稳定合并。 |
| [table.rs](../../src/validation/table.rs)、[struct_value.rs](../../src/validation/struct_value.rs) | required/type/enum、struct/fragment、unique/index 与 source schema。 |
| [foreign_key.rs](../../src/validation/foreign_key.rs)、[key_policy.rs](../../src/validation/key_policy.rs) | 引用完整性、raw/resolved key 策略。 |
| [asset.rs](../../src/validation/asset.rs) | 可见 asset 校验与来源。 |
| [registry.rs](../../src/validation/registry.rs)、[builtins/](../../src/validation/builtins/) | factory、compiled plan、typed config 和规则执行。 |
| [diagnostic.rs](../../src/validation/diagnostic.rs) | ValidationIssue / ValidationReport / ValidationOutcome。 |
| [report/](../../src/validation/report/) | Designer report model、enrichment 与四种格式 render。 |

## 6.2 入口与执行顺序

`validate_data_set*` 接受 raw data 并 normalize；`validate_normalized_data_set*` 接受已 normalized data。入口以 full/target、report、assets、pool 区分；签名以 [mod.rs](../../src/validation/mod.rs) 为准。

Target 主路径：

1. 建立 ExportValidationView，按 tags/export_when/row_export 过滤 table、field、row，构建 resolved companion。
2. 收集各表基础报告，pool 版本稳定合并。
3. 在同一 target view 上执行 FK，有 assets 配置时继续 asset validation。
4. 编译并执行 registry validators，合并 report。
5. 稳定排序，再执行 warning/error gate。

`validate_normalized_data_set_for_target_with_assets_outcome_and_pool` 返回携带 blocking report 的 ValidationOutcome；build 可以先导出报告，再由 `into_result` 阻断数据产物。兼容的 Result 入口直接执行 error gate。前置配置/source/resolver 错误不保证形成完整报告。

Full 入口用于库调用的完整数据集校验；普通 CLI 按选定 target 构建，没有独立 validate 子命令。

## 6.3 Raw / Resolved 视图

ValidationContext 持有 linked Meta、可选 owned tags、raw DataSet 借用和 ExportValidationView。Meta 与 Data 生命周期分开，避免绑定 owned source 数据的生命周期。

ExportTableView 保留 raw table，并缓存与 visible rows 对齐的 ResolvedTable。主要 accessor：

- visible_raw_row / raw_cell：显式 raw policy、来源或兼容 fallback。
- visible_resolved_row / resolved_value：默认的值读取路径。
- resolved_path_value：fragment / nested struct path。
- field_source：关联 resolved trace 和 raw cell location。

VisibleRow.row_index 指向原始 DataTable 下标，source row/sheet/column 取自 raw provenance，不是过滤后的行号。typed parse 复用 data::typed_value，不在 validator、struct 或 FK element 校验中维护另一套转换规则。

## 6.4 Registry 编译与执行

ValidatorMeta 使用公共 kind、name、desc、severity、message、export_when，其余 flatten TOML 参数由 factory 编译为 typed config。

- ValidatorFactory 提供 kind、scope、compile；CompiledValidator 使用 ValidationContext 和 issue collector，不重复解析 TOML。
- CompiledValidationPlan::compile 遍历 table 与 top-level field 显式配置；未知 kind、非法参数或 scope 错误显式拒绝。
- Field 使用 `[[tables.fields.validators]]`，Record/Table 使用 `[[tables.validators]]`。DataSet scope 是内部扩展形状，尚无通用 dataset validator 配置入口。
- Registry register 当前按 kind 替换 factory，不能把未来插件的冲突拒绝策略写成当前行为。

当前注册 range、regex、string_length、required_when、compare、sum、monotonic，以及 color/vector factory。具体参数、默认值和 support field 见 [规则设计](../Design/09-builtin-validators.md)与 builtins 实现。

required/type/enum/unique/FK 当前由基础流程或专门模块执行，不是从便捷字段生成的 registry implicit validators。`kind = "unique"`、`kind = "foreign_key"` 不是当前有效的显式配置。

## 6.5 Target-aware 规则

- table/field/index 可见性遵守同一 effective tags；collection 行由 row_export 过滤。
- field validator 检查可见字段；record/table validator 可显式读取 hidden support fields，不使其导出。
- fragment 只检查 active 且 target 可见的字段；typed fragment 复用标准 struct。
- struct child validators 的完整推广、whole-struct default 和通用跨表 aggregate 尚不支持。
- FK 是 Meta 关系；ObjectTable 可作 source，target 必须是 CollectionTable unique/primary index。

ObjectTable 不支持 index、primary_index、row_export 或 fragments；其 CSV/XLSX split merge、exact path override 和 singleton 校验见 [Meta 语义](../Design/04-meta-model.md)与 [Data 实现](05-data-module-detail-design.md)。

## 6.6 诊断与报告

基础 producer 使用 catalog 的 CF_DATA_* 等 stable code；业务 validator 使用专属 validator.* code。旧 code 的 deprecated metadata 用于兼容。新 producer 提供 current/expected/hint 和 source path/sheet/row/column；规则见 [Diagnostic 设计](02-diagnostic-module-detail-design.md)。

ValidationReport 是原始 issue 集合；DesignerValidationReport::from_validation_report_with_schema 是 generator 主入口，负责 enrichment、summary/grouping 和报告模型，四种 renderer 共用。from_validation_report 可不带 schema enrichment；from_diagnostic_report 仅为通用 adapter/测试入口。

warning 不阻断构建，error 阻断数据产物。失败时报告写入边界见 [Build 实现](10-build-module-detail-design.md)。

## 6.7 测试与扩展边界

[Validation tests](../../src/validation/tests.rs)、[report tests](../../src/validation/report/tests.rs)与 [CLI integration](../../tests/cli_integration.rs)覆盖基础正常/异常路径、struct/list/fragment、target filtering、validator compile、hidden support fields、group/order、数值边界、稳定排序、CSV/XLSX 定位与失败报告。

是否把基础检查迁入 registry 按实际复用需求评估，不作为尚未完成的用户功能。跨表 normalize、nested validator、新 scope 或远端资源引入前，先更新产品语义与 Build 依赖。测试/coverage 以 [代码规范](../code-style.md)为准。
