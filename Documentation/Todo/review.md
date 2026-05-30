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

- [ ] 统一 runtime reader 中 `list` / `array` / `len` / `row_count` / `length` 命名契约。
  - 当前 Meta/IR/codegen 类型层使用 `List`，runtime reader 物理容器多处使用 `Array`，Rust/TS 有通用 `len/length` fallback，C++ `ReadArrayLength()` fallback 到 `ReadRowCount()`。
  - 目标：文档明确分层命名：schema 类型层使用 `List`，runtime value/wire 容器若使用 `Array` 必须解释为 payload shape；row count 与 list length 必须是不同语义。
  - 验收：`Documentation/glossary.md`、reader contract 文档、Rust/TS/C++ runtime 接口和 generated helper 命名保持一致；如需破坏性改名，按 reader contract minor/major 版本规划兼容 alias。

- [x] 为 runtime readers 增加可配置 decode limits。
  - 已完成：Rust `RuntimeDecodeLimits`、TypeScript `RuntimeDecodeLimits` / `normalizeRuntimeDecodeLimits`、C++ `RuntimeDecodeLimits` 均提供默认上限和业务覆盖入口。
  - 已完成：binary / JSON / MessagePack adapter 在文件读取、`row_count`、array length、string bytes、nesting depth 入口统一做上界检查。
  - 验收：不可信 payload 在 reserve/分配前完成上界检查；Rust unit tests 覆盖 file / rows / array / string / nesting，C++ smoke tests 覆盖主要 limit 分支。

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
