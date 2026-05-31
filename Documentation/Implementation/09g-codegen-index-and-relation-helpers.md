# 09g. Codegen Index and Relation Helpers

本文从 Codegen 模块详细设计中拆出，专门描述 CollectionTable index lookup、Foreign Key `linked_field` relation helper、row-index cache 与 `link` / `link_all` API。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 模块总览和专题索引。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、runtime loading 和 generated API 语义。
- [`01-module-outline.md`](01-module-outline.md) — Rust 模块边界与依赖方向。

## 9.9 索引辅助查找函数设计

### 9.9.1 索引来源

索引来自 `TableMeta.indexes`，Link 后为 `LinkedTableMeta.indexes`。每个 index 包含：

- `name`；
- `fields`；
- `unique`；
- `primary`。

`FieldMeta.unique = true` 可视为单字段 unique index，建议在 CodeModel 中统一转成 synthetic index，命名为 `by_<field>`。

### 9.9.2 API 规则

| 索引类型 | Rust | C++ | TypeScript |
| --- | --- | --- | --- |
| unique single | `get_by_id(id) -> Option<&Row>` | `GetById(id) -> const Row*` | `getById(id): Row | undefined` |
| non-unique single | `find_by_type(type) -> Vec<&Row>` | `FindByType(type) -> vector<const Row*>` | `findByType(type): readonly Row[]` |
| unique compound | `get_by_a_b(a, b) -> Option<&Row>` | `GetByAB(a, b) -> const Row*` | `getByAB(a, b): Row | undefined` |
| non-unique compound | `find_by_a_b(a, b) -> Vec<&Row>` | `FindByAB(a, b) -> vector<const Row*>` | `findByAB(a, b): readonly Row[]` |

### 9.9.3 索引构建规则

1. Repository 构造时一次性遍历 loaded rows 构建所有索引。
2. unique index 如果发现重复，加载阶段返回错误。理论上导出前 validation 已经检查，但 generated runtime 仍应防御。
3. non-unique index 保存 row index 列表，保持原始数据顺序。
4. bool/int/float/string/enum 可作为 key。
5. float 作为索引 key 第一阶段允许但不推荐；可在 Meta validate 或 Codegen validate 中给 warning。
6. struct/text/asset 是否允许作为 key 需要策略。第一阶段建议：
   - `text` / `asset` 作为字符串 key 允许；
   - struct key 不支持。

### 9.9.3a Source-order row access policy（规划）

CollectionTable repository 需要把“内部为构建索引而遍历 rows”和“对业务公开 source-order rows 数组”分开建模：

- `preserve_source_order = true`：生成 public source-order API，例如 Rust `len()` / `row_at(index)` / `rows()`，C++ `Size()` / `RowAt(index)` / `Rows()`，TypeScript `length` / `rowAt(index)` / `rows()`；顺序为当前 target visible rows 的稳定顺序。
- `preserve_source_order = false` 且存在 index：默认只生成 index lookup API，不生成 public `rows` API，也不承诺 source order。这样普通 keyed 配置表不会因为实现细节暴露可遍历数组。
- 无 index 的 CollectionTable 若需要 codegen 访问，应显式声明 `preserve_source_order = true`。随机角色姓氏/名字表属于这一类：一列 `string value`，无 index，按 row order 随机访问。
- `preserve_source_order` 不使 row ordinal 成为外键目标；FK target 仍必须是 explicit unique / primary index。

实现迁移建议：

1. 先收敛 public API：只有 `preserve_source_order = true` 生成 rows/row_at/len；indexed table 的 `rows()` 从文档和新生成 API 中移除或改为 opt-in。
2. 再优化内部 storage：`preserve_source_order` 不要求或禁止某种 owner storage。对于多索引表，推荐 private row arena / `rows_` / `Vec<Row>` 拥有行数据，各 index 存 row index / row handle，避免每个索引复制行；对于只有单个唯一索引且无 relation cache 的表，生成器未来可以选择 index-owned row storage。无论哪种策略，未 opt-in 时都不暴露 source-order API。
3. relation cache 当前以 row index 为核心；当 target table 不 preserve source order 时，需要把 cache 从“public source-order ordinal”抽象为 `CodeRowHandle`（ordinal / primary key / internal stable id）。当前实现可以使用 private row index 作为内部 handle，但不得把它作为 public source-order API 暴露。

### 9.9.4 Primary index

如果存在 primary index，额外生成主访问函数：

| Language | API |
| --- | --- |
| Rust | `get(primary_key)` 或 `get_by_primary(...)` |
| C++ | `Get(...)` 或 `GetByPrimary(...)` |
| TypeScript | `get(...)` 或 `getByPrimary(...)` |

第一阶段建议生成 `get_by_<primary_index_name>`，避免主索引字段变化导致 `get(...)` 语义不清。后续可配置是否生成短别名 `get(...)`。

## 9.10 Foreign Key Relation Helper 与 Link API

### 9.10.1 关系解析流程

带 `linked_field` 的 helper 使用目标 table 现有 unique / primary index，不额外构建专用 relation map。生成器应复用 `CodeForeignKey.target_table` 和 `target_index` 找到目标 repository 的 lookup 方法，并根据 `CodeForeignKey.fields` 读取 source key。

单值 FK helper 伪流程：

```text
get_relation(source_row, data):
  cache = source_row.<relation_cache>
  if cache == Empty:
    return empty optional result
  if cache == RowIndex(i):
    return data.<target_table>.row_at(i) or mismatch error

  key = build_key(source_row.<source_fields>)
  if key is all-empty and allow_empty:
    cache = Empty
    return empty optional result
  if key is partial-empty:
    return runtime data error

  index = data.<target_table>.index_of_<target_index>(key)
  if index not found:
    return runtime data error
  cache = RowIndex(index)
  return data.<target_table>.row_at(index)
```

为了避免重复查询，目标 repository 除了现有 `get_by_* -> row` API，还应生成内部 row handle lookup。当前实现可表现为 `index_of_by_* -> Option<usize>` / `IndexOfBy*` / `indexOfBy*`，但规划上不应把该 `usize` 等同于 public source-order ordinal；在 indexed-only table 不保留 source-order rows 后，它应演进为 `CodeRowHandle`。该 API 可以是 private/package-private，只服务 relation cache；公开 API 仍返回 row 引用或对象。对于复合 key，内部 index lookup 使用与公开 lookup 相同的 key 构造逻辑。

### 9.10.2 `link` / `link_all` 实现

Table-level `link` 不需要特殊拓扑顺序，因为它只把 key 解析成 row index，不会持有目标 row 引用，也不会递归访问目标 row 的 relation helper。DataSet `link_all` 按 CodeModel table 顺序执行即可，保持生成结果确定性。

```text
GameData.link_all():
  for table in model.tables:
    table.link(self)
```

ObjectTable source 的 FK 按 singleton object 处理；CollectionTable source 遍历全部 rows。若某 table 没有任何 `linked_field`，可以不生成 table `link`，DataSet `link_all` 跳过它；也可以生成 no-op 方法以稳定 API，具体由语言生成器决定。推荐只有存在 relation helper 时生成 table `link`，DataSet `link_all` 总是生成，便于业务启动阶段统一调用。

`link` / `link_all` 需要满足：

- 幂等：已解析 cache 再次 link 时不重新查索引，或重新查出的结果必须一致；推荐直接复用 getter 的 cache fast path。
- 错误原子性不强制：若中途失败，部分 earlier rows 可能已缓存。由于失败表示数据/版本问题，调用方应丢弃整个 `GameData`；不要求回滚 cache。
- 线程安全由生成器语言策略决定。Rust 若使用 `Cell` / `RefCell`，默认不是 `Sync`；如服务器需要多线程共享，可后续提供 `thread_safe_relation_cache` option，把 cache 改为 `OnceCell` / `Mutex` / eager-only link 模式。当前默认优化单线程或启动期预 link 场景。

### 9.10.3 Runtime data 与 schema 影响

`linked_field` 只影响 generated code，不影响 runtime payload：

- JSON / MessagePack 中不输出 `linked_field` 字段。
- `bin` schema revision 不因为新增、删除或重命名 `linked_field` 而变化；但 generated code API 会变化，因此 code artifact 仍会更新。
- Reader contract 不需要新增方法；relation helper 使用已加载到内存中的 typed rows 和 indexes，不读取底层 reader。
- Validation 仍是引用完整性的权威来源；generated helper 的 defensive error 是运行时版本错配保护，不替代 export 前 validation。

### 9.10.4 测试策略

实现阶段应新增语言无关和语言专属测试：

1. IR 测试：`linked_field` 正确进入 `CodeForeignKey.linked_field`，未声明时为 `None`；target filtering 后不可见 FK 不进入 relation helper。
2. 命名测试：`linked_field` 与字段、fragment、lookup method、关键字发生目标语言命名冲突时报错。
3. 生成快照/片段测试：Rust/C++/TypeScript 均生成原始 FK 字段、cache slot、relation getter、table link 和 DataSet link_all。
4. Runtime harness：构造 `game_rules.default_class_id -> classes.by_id`，首次访问返回目标 row；第二次访问命中 cache。可通过 instrumented index lookup counter 或暴露测试 hook 验证第二次不查 map。
5. Optional/list/composite FK：覆盖 empty optional cache、list 顺序保持、复合 key lookup。
6. 错误路径：绕过 validation 构造缺失目标 row 的 runtime data，helper/link_all 返回错误 / throw，且不把缺失当作合法 empty。
