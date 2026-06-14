# TODO: Data / Validation 统一消费 resolved view

状态：Planned / Not implemented

## 判断结论

这项工作不适合单次修复，应该按阶段推进。

原因不是 `DataResolver` 能力不足，而是当前 Validation 的核心数据面仍是 raw
`DataSet` / `DataRow`：`ValidationContext` 保存 raw `DataSet` 与
`ExportValidationView`，而 `ExportTableView` 保存 raw `DataTable`、visible fields 和
visible rows（`src/validation/context.rs:15`, `src/validation/context.rs:197`）。
`DataResolver` 在 table validation 中已经被调用，但当前主要作为“可解析性检查”，
结果没有进入后续 validation 数据流（`src/validation/table.rs:44`,
`src/validation/table.rs:178`）。

如果一次性替换所有 raw access，会同时影响 table/type/required/index、struct/list
expanded value、built-in validators、FK、asset validation、report enrichment、asset
manifest / POT / binary generator 的 row filter 与 record key 语义；其中 FK 还明确保留
raw key string 以避免 typed canonicalization 带来的 target-dependent 行为差异
（`src/validation/foreign_key.rs:799`）。因此需要先建立 resolved validation view 和
source trace 桥接，再逐类迁移规则。

## 当前状态摘要

- DataResolver 已提供 typed/default-aware/target-aware resolved rows：`ResolvedTable`、
  `ResolvedRow`、`ResolvedValue`、`DataValue`、`ValueSourceTrace` 位于
  `src/data/resolved.rs:24`；`DataResolver::resolve_table_rows` 支持按 visible row index
  resolve target view（`src/data/resolver.rs:85`）。
- Runtime data payload 已主要消费 resolved values：JSON / MessagePack / Lua 等通过
  `build_exported_table_payload` 进入 `DataResolver::resolve_table_rows`
  （`src/data_artifact/payload.rs:44`）。
- Validation target view 的 row visibility 仍通过 raw row cell 计算：
  `row.get(field).map(String::as_str)`（`src/validation/context.rs:90`）。
- Table validation 的 required、field value、fragment、unique / index 逻辑仍直接遍历
  raw rows（例如 `src/validation/table.rs:506`, `src/validation/table.rs:630`,
  `src/validation/table.rs:684`, `src/validation/table.rs:847`）。
- Struct/list expanded validation 仍按 raw field path 递归解析
  （`src/validation/struct_value.rs:117`, `src/validation/struct_value.rs:221`,
  `src/validation/struct_value.rs:698`）。
- Built-in validators 仍通过 `ExportTableView.table.rows` 与 common raw accessor 取值
  （`src/validation/builtins/common.rs:343`）。
- Asset validation 与 Asset Manifest 仍读 raw asset refs / raw record keys
  （`src/validation/asset.rs:71`, `src/generator/asset_manifest/extract.rs:25`,
  `src/generator/asset_manifest/extract.rs:188`）。

## 目标原则

1. Validation 主流程统一拥有 target-aware resolved view；raw row 只作为兼容、source
   trace、row_export predicate 或明确要求 raw 语义的规则输入。
2. raw / resolved 两条路径的诊断位置一致，尤其要保持 cell source 的 source path、row、
   column 信息。
3. ObjectTable key-level cell source、override source trace、struct/list expanded source trace
   通过同一套 resolved source lookup 暴露给 validation、report 和 generator。
4. Asset / report / runtime export 不再重复解释 raw cell；需要 raw key 语义的地方必须显式
   命名并记录 policy。

## [x] Step 1：引入 resolved validation view，先不改变规则语义

目标：让 ValidationContext 同时携带 raw view 与 resolved view，建立后续迁移的稳定数据面。

范围：

- 在 validation context 层新增 target-aware resolved table/row view，例如
  `ResolvedExportValidationView` 或给 `ExportTableView` 增加 resolved companion。
- `ExportTableView` 保留 raw `DataTable` 与 `visible_rows`，用于 row source、cell source 和
  需要 raw string policy 的规则。
- target view 构建阶段调用 `DataResolver::resolve_table_rows`，把结果缓存到 view 中；避免
  `validate_normalized_export_table_with_report` 再 resolve 后丢弃结果。
- 提供统一 accessors：
  - `visible_raw_row(row)`；
  - `visible_resolved_row(row)`；
  - `field_source(row, field_path)`；
  - `resolved_value(row, field_name)`；
  - `raw_cell(row, field_path)`（命名必须显式表示 raw 语义）。
- 为 resolved trace 补齐或桥接 `DataTable::cell_source(row_index, field_path)` 的 column 信息，
  不直接把 diagnostic location 降级为只有 row。

验收：

- 不改变现有 validation 报错集合和 artifact 输出。
- 新增/更新单元测试覆盖：resolved view row count、target-visible fields、source trace 与 raw
  cell source 对齐。
- 运行 `cargo test validation` 与 `cargo test --test cli_integration validation`。

## [x] Step 2a：迁移 target table 标量字段 type/required/default-aware validation

目标：先让 target validation 的普通字段校验消费 Step 1 缓存的 resolved value，避免 target
table view 在基础字段校验阶段再次解释 raw cell。

范围：

- `ExportTableView` 的 `resolved_value` / `raw_cell` accessor 成为 target field validation 的入口。
- `validate_field_values_view` 对非 struct 字段优先信任 resolved value：
  - resolved value 存在时，type/default/expanded vector 等解释已经由 DataResolver 完成；
  - raw value 仍用于保持既有 missing-required diagnostic 的 current value；
  - resolver 不可用时保留旧 raw path fallback。
- `validate_normalized_table_view_with_report` 不再对已有 resolved table 的 view 重复运行 resolver。

验收：

- `cargo test validation::` 通过。
- `cargo test --test cli_integration validation` 通过。
- 后续 Step 2b 继续迁移 full table path、struct/list occurrence helpers 与 raw helper 收敛。

## [x] Step 2b：迁移基础 full table / struct-list validation

目标：把已经由 DataResolver 统一解释的类型、默认值、struct/list/vector/color 基础校验迁到
resolved value，减少 validation 重复解析 raw cell。

范围：

- `src/validation/table.rs` 中：
  - type validation 使用 `ResolvedValue.value`；
  - required / effective value 判定使用 resolved source kind 与 value emptiness；
  - vector/color/list/struct 的基础 shape 校验优先消费 resolved value；
  - raw helper `field_has_source` / `field_has_effective_value` 收敛到 resolved accessors。
- `src/validation/struct_value.rs` 中：
  - struct JSON cell 与 expanded child cell 的解释不再独立扫描 raw path；
  - list element / struct child diagnostics 使用 resolved nested source trace；
  - 保留必要的冲突检查，但以 resolver 输出或 resolver diagnostics 为依据。

验收：

- default、expanded vector、expanded struct、expanded list、ObjectTable override source 的错误位置不回退。
- 标准 fixture 与 validation 集成测试通过：`cargo test --test cli_integration validation standard_project`。
- 新增针对 default vs explicit cell source、expanded list element source 的回归测试。

## [x] Step 3：迁移 built-in validators 到 resolved-aware accessors

目标：让 built-in validators 不再直接从 `DataRow` 解释值，而是通过统一 accessor 获取 typed
resolved value 或显式 raw string。

范围：

- 在 `src/validation/builtins/common.rs` 增加 resolved-aware value API：
  - typed scalar / number / bool / string export；
  - raw string accessor 仅用于明确 raw policy 的 validator；
  - unified source location helper。
- 优先迁移 numeric / typed validators：range、color、vector、sum、monotonic、compare。
- 对 regex、string_length、required_when、condition 明确 policy：
  - 默认使用 scalar export string；
  - 如确需 raw string，配置/代码命名体现 raw semantics。
- group key、order key、aggregate key 不直接读 raw row，改为 policy-driven key builder。

验收：

- validators 对默认值、enum canonical value、expanded struct/list 子字段行为有测试覆盖。
- 旧 raw-only case 不发生 silent behavior change；需要变更的行为在 Design/Implementation 中补充说明。
- 运行 `cargo test validation::builtins` 与相关 CLI validation tests。

## [x] Step 4：单独处理 FK、index、record key 的 key policy

目标：把“数据值验证”与“key 字符串语义”分开，避免将 FK/index key 机械迁移成 typed canonical
value 导致兼容性问题。

范围：

- 为 FK/index/record key 增加明确 policy：
  - `RawCellKey`：保持当前 raw string 行为；
  - `ScalarExportKey`：使用 resolved scalar export string；
  - `TypedCanonicalKey`：仅在经过设计确认后用于新能力。
- `src/validation/foreign_key.rs` 保留当前 FK raw key 默认行为，但通过命名和 helper 表达
  “raw by design”，减少未来误改。
- index uniqueness、group_by、asset manifest record key、POT record key 迁到同一 key builder。
- 对 ObjectTable key-level cell source 和 override source trace 建立统一 lookup。

验收：

- FK 现有行为完全兼容；新增测试证明 raw `"01"` 与 typed int `1` 的 key policy 不被误合并。
- index / record key diagnostics 仍指向正确 cell。
- 运行 `cargo test validation::foreign_key`、`cargo test --test cli_integration target_filtering fixture_exports`。

## [ ] Step 5：收敛 asset validation、report、manifest/POT/binary 的 raw 剩余点

目标：让 data-backed 产物与 report enrichment 共享 resolved view / source trace，不再各自重复解释 raw
cell。

范围：

- `src/validation/asset.rs` 使用 resolved value 读取 asset ref，source location 从统一 accessor 获取。
- `src/generator/asset_manifest/extract.rs` 使用 resolved row / resolved asset values；record key 走
  Step 4 的 key builder。
- binary/POT 的 visible row filter 和 record key 从 shared helper 获取；payload 已 resolved 的部分保持不变。
- validation report enrichment 不直接回查 raw row；通过 ValidationIssue location / source trace 展示。

验收：

- asset missing、manifest CSV/JSON、POT text extraction 的 source / record key 稳定。
- 运行 `cargo test --test cli_integration validation fixture_exports standard_project`。

## [ ] Step 6：文档提升与清理

目标：阶段完成后删除短期 TODO，把稳定设计提升到 canonical docs。

范围：

- 更新 `Documentation/Design/05-data-validation.md`：记录 resolved validation view、raw key policy、diagnostic source trace policy。
- 更新 `Documentation/Implementation/05-data-module-detail-design.md` 和
  `Documentation/Implementation/06-validation-module-detail-design.md`：记录 DataResolver / ValidationContext
  的最终边界。
- 删除或归档本文档，避免 TODO 成为唯一设计来源。

验收：

- 所有 Step checkbox 已按实际提交状态更新。
- canonical docs 能独立说明当前实现。
- `cargo test` 通过后提交文档清理。
