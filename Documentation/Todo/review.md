# 项目 Review TODO

本文记录本轮整体 Review 中不适合在一次小修内完成的设计、复用、性能与命名一致性改进项。小型问题已优先直接修复；以下事项需要单独设计、分阶段迁移或影响 reader/codegen contract。

## P0：语义一致性与重复逻辑收敛

- [x] 统一 validation / resolver / FK 的 typed value 解析主路径。
  - 已完成：抽出 `src/data/typed_value.rs`，集中提供 `LinkedField -> ScalarValue`、list JSON cell / JSON array 到 `Vec<ResolvedValue>` 的转换；`DataResolver`、`validation::struct_value`、`validation::foreign_key` 已改为复用这条路径。
  - 已完成：struct expanded child default、struct JSON child default、list default、list FK 元素转换统一走 shared helper，避免 default、empty、JSON array、enum/list element conversion 规则漂移。
  - 后续：range 等 validator 仍运行在 raw/normalized `DataRow` 上；迁移到 `validate_resolved_*` 后可让更多 validator 直接消费 `ResolvedValue/DataValue`。

- [x] 将 FK target key set 建模为 single/composite 两条路径并缓存构建结果。
  - 已完成：`src/validation/foreign_key.rs` 引入 `ForeignKeyTargetKeyCache`，按 `(target_table, target_index, target_view_scope)` 缓存 target key set，full / target-aware validation 分别使用独立 scope。
  - 已完成：单字段 key 使用 `HashSet<String>`，复合 key 使用 `HashSet<Vec<String>>`；list FK 元素 lookup 直接走 single-key set，不再为每个元素构造临时 `Vec<String>`。
  - 验收：单元测试覆盖同一 target index 缓存复用、single/composite 分流和 target view visible rows 作用域。

## P1：runtime/codegen contract 与安全边界

- [x] 统一 runtime reader 中 `list` / `array` / `len` / `row_count` / `length` 命名契约。
  - 原问题：Meta/IR/codegen 类型层使用 `List`，runtime reader 物理容器多处使用 `Array`，Rust/TS 有通用 `len/length` fallback，C++ `ReadArrayLength()` fallback 到 `ReadRowCount()`。
  - 已完成：`Documentation/glossary.md` 与 reader contract 文档明确分层命名：schema 类型层使用 `List`，runtime value/wire 容器中的 `Array` 只表示 payload shape；row count 与 list length 是不同语义，不互相 fallback。
  - 已完成：Rust/TypeScript/C++ reader contract 新增 `read_list_len` / `readListLength` / `ReadListLength`，generated list helper 优先使用 list length；旧 `read_array_len` / `readArrayLength` / `ReadArrayLength` 保留为兼容 alias 并委托 list length。

- [x] 为 runtime readers 增加可配置 decode limits。
  - 已完成：Rust `RuntimeDecodeLimits`、TypeScript `RuntimeDecodeLimits` / `normalizeRuntimeDecodeLimits`、C++ `RuntimeDecodeLimits` 均提供默认上限和业务覆盖入口。
  - 已完成：binary / JSON / MessagePack adapter 在文件读取、`row_count`、list length、string bytes、nesting depth 入口统一做上界检查。
  - 验收：不可信 payload 在 reserve/分配前完成上界检查；Rust unit tests 覆盖 file / rows / list / string / nesting，C++ smoke tests 覆盖主要 limit 分支。

## P2：内部模型和 exporter 复用

- [x] 重新评估 `DataValue::Array(Vec<ResolvedValue>)` 的内存模型。
  - 已完成：`DataValue::Array` 改为 `Vec<ArrayElementValue>`，每个元素只保存 `DataValue` 与可选 raw value，不再常驻完整 `ResolvedValue`、field path string 与 source trace。
  - 已完成：list parser 仍在转换失败时按需构造 `field[index]` 诊断路径；payload/binary exporter 与 list FK 校验改为消费轻量元素模型。
  - 验收：单元测试覆盖 `ArrayElementValue` 小于 `ResolvedValue`、raw value/export fallback 与 list element 解析行为。

- [x] 抽象 binary record optional bitmap 写入逻辑。
  - 已完成：抽出 `PresenceSlot` 与 `write_optional_bitmap`，统一 optional slot 过滤、bitmap byte 初始化和 presence bit 写入。
  - 已完成：table record、struct record、fragment record 都复用同一 bitmap writer；table record 仍保持字段 optional slot 在前、fragment slot 在后的既有 layout 顺序。
  - 验收：单元测试覆盖 required slot 不占 bit、optional slot bit packing 与跨 byte bitmap；既有 binary schema revision/layout 签名逻辑保持不变。

- [ ] 引入 resolver 上下文对象减少参数蔓延。
  - 当前 resolver helper 反复传递 `table_name`、`row_number`、`row_source`、`field_path`、`target_tags`。
  - 目标：引入只读 `ResolveContext` / `ValueContext`，集中错误上下文、source trace 与 target tags。
  - 验收：新增 map/list expanded path、source trace 或诊断字段时不需要继续扩大多层函数签名。
