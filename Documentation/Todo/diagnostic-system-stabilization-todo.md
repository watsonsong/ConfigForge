# Diagnostics System Stabilization TODO

本文记录 ConfigForge 诊断 / 错误处理从当前状态进入 `Stable` 前仍需要收敛的实际事项。当前基础协议、report、CLI JSON、Generator Registry、Artifact path safety、核心 Data validation 已有较完整结构化诊断；但错误处理整体还不能标 Stable，因为仓库仍存在大量 `ConfigForgeError::Validation(String)` fallback，且部分高频用户错误仍依赖自然语言 message。

## 当前结论（2026-06-10）

**暂不删除本 TODO，也不把 Diagnostics 标为 Stable。**

原因：当前 `src/**/*.rs` 中仍有 `ConfigForgeError::Validation(` **443 处 / 78 文件**。这些不全是用户可见错误，但仍覆盖 asset resolver、data loading、data resolver、typed value、data artifact、code artifact、generator 私有 option、project/build config、builtin validators 等路径。只要高频 CLI / build 失败仍可能只返回裸字符串，机器消费方就不能完全依赖 stable code / source trace / current / expected。

已经完成且不再作为 TODO 追踪的内容：

- `Diagnostic` / `ValidationIssue` enrichment 字段：`current_value`、`expected`、`explanation`、`domain`、`owner`。
- `DiagnosticEntry` default methods 与 `Diagnostic::from_entry()` enrichment bridge。
- `DiagnosticLocation` source column：`source_column_number`、`source_column_name`、`column_index`。
- `Diagnostic` builder-style API 与 `ConfigForgeError::from_diagnostic(s)` / `from_diagnostic_report()` helpers。
- Diagnostic code catalog / metadata 与第一批 stable code。
- CLI `--diagnostic-format json`。
- Designer Validation Report HTML / Markdown / JSON / CSV enrichment 字段贯通。
- Generator Registry unknown / ambiguous / health check 与 prepared input 缺失结构化诊断。
- Artifact path escape / collision / case-insensitive / symlink / hardlink alias 结构化诊断。
- 核心 table validation 的 type / required / duplicate key / FK / asset reference 结构化诊断。

以下内容已从本 TODO 移除，不再作为 Diagnostics Stable 的阻塞项：

- CI annotation / SARIF 输出格式：属于 CI 集成增强，不是错误处理稳定性的前置条件。
- 多源 sheet/provider 精确定位：属于未来 source provider 能力，不阻塞当前 CSV-first 诊断稳定。
- build `validation_report` 的 `include_warnings` / `include_errors` / `group_by`：属于 Designer Report 功能增强，不是底层错误协议稳定前置条件。
- 强制引入 diagnostic enum：当前继续保留 `String` 契约，以支持外部 generator / validator 自定义 code。

## Stable 判定标准

达到 Stable 前，应同时满足：

1. 用户可通过 CLI / build 触发的主要业务错误都有 stable diagnostic code。
2. 高价值错误至少包含足够的 `source` / `target` / `location` / `current_value` / `expected` / `hint`。
3. 新增 ConfigForge-produced 业务错误默认不再写裸 `ConfigForgeError::Validation(format!(...))`。
4. 测试断言优先依赖 code / severity / location / current / expected，而不是自然语言 message。
5. 设计状态矩阵才能从 `Implemented MVP` 调整为 `Stable`。

## 实际剩余 TODO

### P0：建立 legacy 收敛护栏

当前 helper 已经足够顺手，但没有机制阻止继续新增裸字符串错误。

- [ ] 增加 legacy 债务统计测试或脚本，记录 `ConfigForgeError::Validation(` 当前基线；新增使用点必须显式更新 allowlist / debt count。
- [ ] 在文档 / 代码注释中明确：新的 ConfigForge-produced 业务校验错误必须使用 `Diagnostic` / `DiagnosticReport`，`Validation(String)` 只作为第三方错误包装或尚未迁移 fallback。
- [ ] 将迁移目标从“全仓库一次性清零”改为“用户可见路径优先 + debt count 单调下降”，避免噪声重构。

### P0：Project / Build 配置错误结构化

这些错误会直接阻断 CLI build / validate，是 Stable 前最值得优先迁移的入口层错误。

- [ ] 迁移 target 不存在、重复 target / export task name、非法 task name、非法 resource flag、非法 input/output 配置等 build / project config 错误。
- [ ] 为 build / project config 错误补充 `source = "build.*"` 或 `project.*`、`target`、`current_value`、`expected`、`hint`。
- [ ] 为上述路径增加 CLI JSON integration tests，断言 stable code 和 target / option context。

### P0：Data loading / source reading 错误结构化

Data loading 仍有大量用户高频错误使用裸字符串：CSV ragged row、empty file、duplicate header、source missing、source escape、override row invalid、unsupported source feature 等。

- [ ] 为 CSV/source loading 定义并接入 stable code，例如 `CF_DATA_SOURCE_READ_FAILED`、`CF_DATA_SOURCE_SCHEMA_INVALID`、`CF_DATA_SOURCE_PATH_ESCAPE`、`CF_DATA_DUPLICATE_HEADER`（命名以实际实现为准）。
- [ ] 迁移 CSV parse / ragged / duplicate header / empty file 等错误，填充 `source_path`、`source_row_number`、`source_column_name`、`current_value`、`expected`。
- [ ] 迁移 data load plan / source path escape / missing source / unsupported provider feature 错误，填充 source path 和 expected source policy。
- [ ] 增加 CLI JSON 或 unit tests 覆盖上述高频路径。

### P1：Data resolver / typed value / enum 细分诊断

核心 table validation 已覆盖 type / required / duplicate / FK，但 enum、struct/list、typed value 细节仍有 legacy fallback 或被折叠到通用 type error。

- [ ] 将 enum 错误拆为 `CF_DATA_INVALID_ENUM` producer，填充当前枚举字符串 / ordinal 和候选值。
- [ ] 迁移 struct JSON cell、expanded struct/list、recursive/nested path、list index 连续性等 data resolver 错误，保留 source cell trace。
- [ ] 迁移 typed value / default value conversion 的用户可见错误，填充当前值和期望类型。
- [ ] 补充 report / CLI JSON 字段断言，避免新路径只落 message。

### P1：Asset resolver 细粒度结构化

Validation 层 asset reference 已输出 `CF_DATA_INVALID_ASSET_REF`，但 `src/asset.rs` 内部仍有大量规则失败以裸字符串表达。

- [ ] 将 unknown asset kind、unsupported input mode、invalid relative path / escape、mount mismatch、missing file、missing sidecar、prefix/suffix/pattern/object name/case-sensitive path 等 resolver 错误迁移为结构化 diagnostic。
- [ ] 根据需要拆分 asset diagnostic code；若暂不拆分，至少确保统一 `CF_DATA_INVALID_ASSET_REF` 能保留 `current_value`、`expected`、`hint`。
- [ ] 增加 resolver unit tests 与 CLI JSON 测试，断言 code / current / expected，而不是只匹配 message。

### P1：Builtin validators 从 legacy code 渐进迁移

当前 `validator.*` / `field.range_violation` 已进入 catalog metadata，但仍属于 legacy 命名体系。

- [ ] 保持现有 `validator.*` 对外兼容的同时，为内置 validators 评估是否需要 `CF_VALIDATION_*` stable code；若继续使用 legacy code，则文档中明确这些 code 已被视为稳定外部契约。
- [ ] 优先迁移 range / regex / string_length / required_when / compare / sum / monotonic 的 config error 与 runtime violation，补齐 `current_value`、`expected`、`hint`。
- [ ] 更新 tests，使 validator tests 不只断言自然语言 message。

### P1：Generator / artifact 私有 option 与 code/data artifact 错误

Generator registry 与 artifact writer 已结构化，但具体 generator、code artifact、data artifact toolkit 仍有裸字符串错误。

- [ ] 迁移 generator 私有 option 解析错误到 `CF_GENERATOR_INVALID_OPTION` / `CF_GENERATOR_UNSUPPORTED_OPTION`，补充 task / target / option context。
- [ ] 迁移 code artifact model / naming / unsupported type / file name 错误，补充 generator source、table / field / type context。
- [ ] 迁移 data artifact runtime payload / binary encoder / registry 错误，补充 table / field / row / expected runtime type context。
- [ ] 为每类新增至少一个 producer test 或 CLI JSON test。

### P2：Stable 收口

- [ ] 当 P0 / P1 的用户可见路径迁移完成后，重新统计 `ConfigForgeError::Validation(`，只允许测试、第三方错误包装和明确 legacy allowlist 留存。
- [ ] 更新 `Documentation/Design/README.md` diagnostics 状态，从 `Implemented MVP` 调整为 `Stable`。
- [ ] 删除本 TODO 文件，或将剩余非阻塞增强迁移到对应 feature TODO / issue。
