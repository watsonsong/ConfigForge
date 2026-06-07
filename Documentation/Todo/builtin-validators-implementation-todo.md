# Built-in Validators 实现 TODO

本文把 [`Design/09-builtin-validators.md`](../Design/09-builtin-validators.md) 中的 P1-2 游戏常用内置 validators 设计拆成可独立开发、验证和提交的实现步骤。当前实现基线是：validator registry、`ValidatorMeta`、field/table validator TOML 承载、结构化 `ValidationReport`、target-aware `ExportValidationView` 已存在；内置 registry 目前只注册了 field-scope `range`。

## 总体原则

- 每个 Step 必须能独立编译、独立测试、独立提交。
- 新增 validator 必须通过 `ValidatorFactory -> CompiledValidator` 路径接入，不把规则散落到 validation 主流程。
- 每个 validator 的 TOML 参数必须在 compile 阶段转成 typed config，并拒绝 unknown parameter。
- 数据检查默认运行在当前 target ExportView 上；被 table / row / field filter 排除的数据默认不参与。
- record / table validator 如需读取 hidden support fields，只能读取参数中显式引用的字段，且不改变 runtime export 行为。
- 诊断必须保留 stable code、table、field / group、row/source row、validator kind/name、target tags 等可测试信息。
- 并行 validation 不得改变诊断顺序；新增 table/group validator 必须有 deterministic 排序策略。

## [x] Step 1：抽出 validator 参数与字段类型共享 helper

目标：减少后续 built-in validators 重复实现参数解析、字段查找、target tags 判断和诊断构造。

范围：

- 在 `src/validation/builtins/` 下新增共享 helper 模块，例如 `common.rs`。
- 提供公共参数读取工具：required/optional string、bool、integer、number、string array、enum string。
- 提供 unknown parameter 检查工具，错误信息统一为 `validator <kind> has unknown parameter <name>`。
- 提供 field lookup / table lookup helper，统一报 unknown table / field。
- 提供 scalar field kind 分类 helper：numeric、string-like、comparable、stable-key-capable。
- 将 `range` 内部的 `optional_number` / `optional_bool` / unknown param 检查迁移到共享 helper，保持行为不变。

验收：

- 现有 `range` 测试全部通过。
- 新增 helper 单元测试覆盖：缺必填参数、类型错误、unknown parameter、字段类型分类。
- 不新增任何新 validator kind。

建议提交：`refactor(validation): share builtin validator helpers`

## [x] Step 2：实现 `regex` field validator

目标：支持游戏配置中 key / name / asset ref / text key 的正则命名规则。

范围：

- 新增 `src/validation/builtins/regex.rs` 并在 `builtins/mod.rs`、`ValidatorRegistry::with_builtin_validators()` 注册。
- scope：`Field`。
- 参数：`pattern`（必填）、`case_sensitive = true`、`allow_empty`、`match_mode = "full" | "search"`。
- 支持字段类型：`string`、`text`、`asset`、enum raw string。
- `full` 模式必须等价整串匹配；不能依赖用户自己写 `^...$`。
- compile 阶段预编译 regex；invalid regex 作为配置错误。
- 诊断 code：`validator.regex_mismatch`。

验收：

- 覆盖合法/非法 regex、full/search 差异、case_sensitive、allow_empty。
- 覆盖 string/text/asset/enum 支持，int/float/bool/list/struct 拒绝。
- 覆盖 target field hidden 时 validator 跳过。

建议提交：`feat(validation): add regex validator`

## [x] Step 3：实现 `string_length` field validator

目标：支持 UI 文本、昵称、道具名、描述字段长度限制。

范围：

- 新增 `src/validation/builtins/string_length.rs` 并注册。
- scope：`Field`。
- 参数：`min`、`max`、`exact`、`allow_empty`、`unit = "unicode_scalar" | "utf8_bytes"`、`trim_before_count = false`。
- 约束：`exact` 与 `min` / `max` 互斥；至少提供 `min` / `max` / `exact` 之一；`min <= max`。
- 支持字段类型：`string`、`text`、`asset`。
- 诊断 code：`validator.string_too_short`、`validator.string_too_long`、`validator.string_length_mismatch`。

验收：

- 覆盖 `min` / `max` / `exact` 和互斥配置错误。
- 覆盖 `unicode_scalar` 与 `utf8_bytes`，包含中文、emoji、组合字符样例。
- 覆盖 `trim_before_count` 与 `allow_empty`。
- 覆盖非 string-like 类型拒绝。

建议提交：`feat(validation): add string length validator`

## [ ] Step 4：补齐并统一 `range` 行为与文档测试

目标：在新增 field validators 后，把已有 `range` 的行为、诊断和 helper 使用统一到 P1-2 口径。

范围：

- 继续支持现有参数：`min`、`max`、`exclusive_min`、`exclusive_max`、`allow_empty`。
- 确认所有参数解析走 Step 1 的共享 helper。
- 诊断 code 维持现有 `field.range_violation`，或在兼容策略明确后迁移为 `validator.range_violation`；若迁移，必须更新所有测试和文档。
- 检查 warning severity、自定义 message、export_when、target hidden field 行为与 regex/string_length 一致。

验收：

- 现有 range 测试继续通过。
- 新增对统一 helper 错误文案、自定义 message、warning 和 export_when 的回归测试。
- 如修改诊断 code，必须同步 Design / Implementation 文档。

建议提交：`refactor(validation): align range validator behavior`

## [x] Step 5：实现 record validator 基础设施与共享 `when` 条件模型

目标：为 `required_when` 和后续 record-level validators 提供 row 内条件判断、support field 读取和稳定诊断定位基础。

范围：

- 在 `src/validation/builtins/` 下新增 condition helper，例如 `condition.rs`。
- 支持叶子条件：`field` + `equals` / `not_equals` / `in` / `not_in` / `is_empty` / `not_empty`。
- 支持组合条件：`all` / `any` / `none`，允许递归嵌套。
- compile 阶段检查：未知字段、未知参数、叶子与组合混用、多个比较操作混用、空组合、字面值类型兼容。
- run 阶段读取当前 visible row 的 raw/normalized value；允许显式引用 hidden support field。
- 增加 record validator 运行 helper：按 target visible rows 遍历、定位 source row、格式化 record key。

验收：

- 只引入基础设施，不新增用户可见 validator kind 也可以独立提交。
- 单元测试覆盖所有叶子操作、组合操作、hidden support field、target row filter。
- 错误信息能明确指出 validator、table、field path 和参数位置。

建议提交：`feat(validation): add record condition helpers`

## [x] Step 6：实现 `required_when` record validator

目标：支持“某条件满足时字段必填”的常见配置规则。

范围：

- 新增 `src/validation/builtins/required_when.rs` 并注册。
- scope：`Record`，声明在 `[[tables.validators]]`。
- 参数：`field`（必填）、`when`（必填）、`treat_empty_string_as_missing = true`。
- `when` 复用 Step 5 条件模型。
- 规则 active 时，目标 `field` 必须存在且非空；若目标 field 在当前 target hidden 但 validator active，应作为配置错误或 compile/validate 阶段明确报错。
- 诊断 code：`validator.required_when_missing`。

验收：

- 覆盖简单条件、all/any/none、hidden support field。
- 覆盖 field hidden + validator active 的错误。
- 覆盖 validator export_when 与 table/row filter 组合。

建议提交：`feat(validation): add required when validator`

## [x] Step 7：实现 `compare` record validator

目标：支持 row 内上下界、时间窗口、等级区间等字段间比较。

范围：

- 新增 `src/validation/builtins/compare.rs` 并注册。
- scope：`Record`，声明在 `[[tables.validators]]`。
- 参数：`left`、`op`、`right`、`skip_if_either_empty = true`。
- 首版只支持 field-vs-field，不支持 field-vs-literal。
- 支持类型：`int`、`float`、`string`、`text`、`asset`、enum；两侧必须类型兼容。
- `<` / `<=` / `>` / `>=` 只允许用于可稳定排序类型。
- 诊断 code：`validator.compare_failed`、`validator.compare_operand_missing`。

验收：

- 覆盖数值比较、字符串/enum 比较、所有 op。
- 覆盖空值跳过 / 不跳过。
- 覆盖类型不兼容、hidden support field、target row filter。

建议提交：`feat(validation): add compare validator`

## [x] Step 8：实现 table validator 分组、排序和数值聚合基础设施

目标：为 `sum` 与 `monotonic` 提供可复用的 group_by / order_by / deterministic diagnostics 能力。

范围：

- 新增 table validator helper，例如 `table_common.rs`。
- 支持读取 visible rows，并允许显式引用 hidden support fields 作为 `group_by` / `order_by`。
- 支持 stable group key 构造：空 group_by 表示整表；多字段 group_by 按声明顺序拼 key。
- 支持 stable scalar sort key 构造，用于 `order_by`。
- 支持 numeric parse helper，统一 int/float、finite float、empty 行为。
- 明确 group diagnostic 的定位策略：group-level issue 可定位 table + validator name + group key，row-level issue 额外定位 source row。

验收：

- 单元测试覆盖 empty group_by、多字段 group_by、hidden support field、row_export 过滤。
- 单元测试覆盖 order_by stable sort、重复 order key 检测所需数据结构。
- 不新增用户可见 validator kind 也可以独立提交。

建议提交：`feat(validation): add table validator helpers`

## [x] Step 9：实现 `sum` table validator

目标：支持掉落池、抽卡池、随机权重、概率分布的总和校验。

范围：

- 新增 `src/validation/builtins/sum.rs` 并注册。
- scope：`Table`，声明在 `[[tables.validators]]`。
- 参数：`field`、`group_by = []`、`equals` / `min` / `max`、`epsilon = 0.0`、`skip_empty = false`。
- 约束：`equals` 与 `min` / `max` 互斥；至少提供 `equals` 或 `min` / `max` 之一；`epsilon >= 0`。
- `field` 必须为 int/float；group_by 字段必须是 stable-key-capable scalar。
- 诊断 code：`validator.sum_mismatch`、`validator.sum_too_small`、`validator.sum_too_large`。

验收：

- 覆盖整表一组、多字段 group_by、hidden support field。
- 覆盖 int 精确比较、float + epsilon。
- 覆盖 skip_empty、empty 报错、row_export 后 sum 变化。
- 诊断包含 group key、expected、actual。

建议提交：`feat(validation): add sum validator`

## [ ] Step 10：实现 `monotonic` table validator

目标：支持等级经验、成长曲线、关卡难度等有序曲线校验。

范围：

- 新增 `src/validation/builtins/monotonic.rs` 并注册。
- scope：`Table`，声明在 `[[tables.validators]]`。
- 参数：`field`、`group_by = []`、`order_by`、`direction = "increasing" | "decreasing"`、`strict = true`、`skip_equal_order_key = false`。
- 支持 `field`：int、float、string、text、asset、enum；`order_by` 必须是 stable-sort-capable scalar。
- 重复 `order_by` 且 `skip_equal_order_key = false` 时报告 `validator.monotonic_duplicate_order_key`。
- 单调违反时报告 `validator.monotonic_violation`，并包含当前 row 与前一 row 的 order/value。

验收：

- 覆盖 increasing/decreasing、strict/non-strict。
- 覆盖 duplicate order key、skip_equal_order_key。
- 覆盖 group_by、hidden support field、target row filter 后序列变化。
- 诊断顺序在并行验证下稳定。

建议提交：`feat(validation): add monotonic validator`

## [ ] Step 11：标准工程样例、文档同步与回归验证

目标：把新增 validators 的推荐用法落到标准工程和文档，便于后续 QA 与用户学习。

范围：

- 在标准工程 Meta 中挑选低风险样例：例如 ID/key 用 `regex`，展示名用 `string_length`，等级经验用 `monotonic`，掉落/权重 fixture 如存在则用 `sum`。
- 更新 [`Design/09-builtin-validators.md`](../Design/09-builtin-validators.md) 的当前状态，标注哪些已实现。
- 更新 [`Design/README.md`](../Design/README.md) 状态矩阵中 validators 的实现边界。
- 更新 [`Implementation/06-validation-module-detail-design.md`](../Implementation/06-validation-module-detail-design.md) 的目录结构、builtins 列表和测试边界。
- 如新增 helper 对 public re-export 有影响，更新 `src/validation/mod.rs` 和相关文档。

验收：

- `cargo fmt` 通过。
- `cargo test` 通过，至少覆盖 unit tests、CLI integration tests、标准工程构建。
- 文档不再把已实现 validator 描述为计划中能力。

建议提交：`docs(validation): document builtin validators completion`

## 推荐落地节奏

建议按以下 commit 序列推进：

1. Step 1：共享 helper；
2. Step 2：`regex`；
3. Step 3：`string_length`；
4. Step 4：`range` 对齐；
5. Step 5：record condition infrastructure；
6. Step 6：`required_when`；
7. Step 7：`compare`；
8. Step 8：table helper；
9. Step 9：`sum`；
10. Step 10：`monotonic`；
11. Step 11：标准工程与文档收口。

如果需要更快产生用户价值，可以先完成 Step 1～4 发布 field validators，再单独排期 Step 5～7 和 Step 8～10。
