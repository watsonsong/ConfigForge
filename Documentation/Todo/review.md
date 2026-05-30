# 项目 Review TODO

本文记录本轮整体 Review 中不适合在一次小修内完成的设计、复用、性能与命名一致性改进项。小型问题已优先直接修复；以下事项需要单独设计、分阶段迁移或影响 reader/codegen contract。

## P0：语义一致性与重复逻辑收敛

- [ ] 统一 validation / resolver / FK 的 typed value 解析主路径。
  - 当前 `DataResolver`、`validation::struct_value`、`validation::foreign_key` 分别解析 scalar/list JSON cell，导致 default、empty、JSON array、element scalar conversion 等规则容易漂移。
  - 目标：先抽出共享 `LinkedField -> ScalarValue` 与 `JSON array -> Vec<ScalarValue>` helper；中期让 FK、required/type/enum/range validation 消费同一份 `ResolvedValue/DataValue`。
  - 验收：list default、list FK、struct child default 和 scalar conversion 在 validation/export/codegen runtime fixture 中只有一套规则来源。

- [ ] 将 FK target key set 建模为 single/composite 两条路径并缓存构建结果。
  - 当前每个 FK 都会重建 target key set，且单字段/list FK 仍使用 `HashSet<Vec<String>>`。
  - 目标：按 `(target_table, target_index, target_view_scope)` 缓存 target key set；单字段 key 使用 `HashSet<String>` 快路径，复合 key 保留 `HashSet<Vec<String>>`。
  - 验收：多个 FK 指向同一 target index 时只扫描一次 target table；list FK 元素 lookup 不再为每个元素构造临时 `Vec<String>`。

## P1：runtime/codegen contract 与安全边界

- [ ] 统一 runtime reader 中 `list` / `array` / `len` / `row_count` / `length` 命名契约。
  - 当前 Meta/IR/codegen 类型层使用 `List`，runtime reader 物理容器多处使用 `Array`，Rust/TS 有通用 `len/length` fallback，C++ `ReadArrayLength()` fallback 到 `ReadRowCount()`。
  - 目标：文档明确分层命名：schema 类型层使用 `List`，runtime value/wire 容器若使用 `Array` 必须解释为 payload shape；row count 与 list length 必须是不同语义。
  - 验收：`Documentation/glossary.md`、reader contract 文档、Rust/TS/C++ runtime 接口和 generated helper 命名保持一致；如需破坏性改名，按 reader contract minor/major 版本规划兼容 alias。

- [ ] 为 runtime readers 增加可配置 decode limits。
  - 当前 list/array length、row count、string bytes、nesting depth 等来自外部 payload，缺少统一 limits API。
  - 目标：覆盖 `max_file_bytes`、`max_rows`、`max_array_length`、`max_string_bytes`、`max_nesting_depth`，并在 binary/json/messagepack adapter 中保持一致诊断。
  - 验收：不可信 payload 在 reserve/分配前完成上界检查，并有跨语言 compliance fixtures。

## P2：内部模型和 exporter 复用

- [ ] 重新评估 `DataValue::Array(Vec<ResolvedValue>)` 的内存模型。
  - 当前每个 scalar list element 都保存完整 `ResolvedValue`、field path string 与 source trace；大数组会产生较多小对象与字符串。
  - 目标：考虑 `ScalarArray(Vec<ScalarValue>)` 或 `Array(Vec<DataValue>)` 的轻量表达，诊断路径按需构造。
  - 验收：大数组导出不为每个元素常驻保存重复 path/source 包装，同时保持元素级诊断能力。

- [ ] 抽象 binary record optional bitmap 写入逻辑。
  - 当前 table record、struct record、fragment record 各自处理 optional slot count、bitmap 初始化和 presence bit 写入。
  - 目标：抽出 `RecordSlot` / `PresenceSlot` 或 `write_optional_bitmap`，统一 target visibility、optional presence 与 slot 顺序。
  - 验收：新增 optional 规则时只需修改一处 bitmap writer，并保持 binary schema revision/layout signature 稳定。

- [ ] 引入 resolver 上下文对象减少参数蔓延。
  - 当前 resolver helper 反复传递 `table_name`、`row_number`、`row_source`、`field_path`、`target_tags`。
  - 目标：引入只读 `ResolveContext` / `ValueContext`，集中错误上下文、source trace 与 target tags。
  - 验收：新增 map/list expanded path、source trace 或诊断字段时不需要继续扩大多层函数签名。
