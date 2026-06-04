# 09c. Codegen C++ Backend

本文从 Codegen 模块详细设计中拆出，专门描述 C++ backend 的 header/source 结构、namespace、ownership 和 lazy loading 落地方式。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 总览、IR、命名、冲突检测和测试总览。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、runtime loading 和 generated API 语义。
- [`13-runtime-binary-format-detail-design.md`](13-runtime-binary-format-detail-design.md) — `bin` runtime data schema revision 与 positional reader contract。

## 当前状态

C++ backend 已实现 MVP：生成 header/source、enum、struct、row/object、repository、`GameData`、`LazyGameData`、index lookup、`linked_field` relation helper、`LinkAll` 与 `text` translation helper。当前 backend 已覆盖 `generate_to_sink()`，production build 分别通过 `ArtifactSink` 打开 `generated.hpp` 和 `generated.cpp` 并使用 streaming writer 写出；`generate()` 仅在 `#[cfg(test)]` 下保留给单元测试做 String-backed 快照和 header/source streaming equivalence 校验。

## C++ 生成内容

当前实现采用单文件头输出：`generated.hpp`。以下多文件布局仍是后续拆分方向：

```text
generated/cpp/
  config_forge_types.hpp
  config_forge_loader.hpp
  config_forge_tables.hpp
  config_forge_tables.cpp
```

核心形态：

```cpp
class IRuntimeDataReader {
public:
    virtual ~IRuntimeDataReader() = default;
    virtual void EnterIndex(std::size_t index) = 0;
    virtual std::int64_t AsInt64() const = 0;
    virtual double AsDouble() const = 0;
    virtual bool AsBool() const = 0;
    virtual std::string AsString() const = 0;
    virtual void EnterRequiredField(const std::string& name) = 0;
    virtual bool EnterOptionalField(const std::string& name) = 0;
    virtual void ExitField() = 0;
    virtual std::size_t ReadEnumIndex(const std::vector<std::string>& variants);
    virtual std::size_t ReadRowCount() = 0;
    virtual std::size_t ReadListLength();
};

class IRuntimeDataLoader {
public:
    virtual ~IRuntimeDataLoader() = default;
    virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;
};

struct ItemRow {
    std::int64_t id;
    std::string name;
    ItemRarity rarity;
};

class ItemsTable {
public:
    static constexpr const char* DataFile = "items";

    static ItemsTable Load(IRuntimeDataLoader& loader);
    static ItemRow ReadRow(const IRuntimeDataReader& reader);
    const std::vector<ItemRow>& Rows() const;
    const ItemRow* GetById(std::int64_t id) const;
    std::vector<const ItemRow*> FindByRarity(ItemRarity rarity) const;

private:
    std::vector<ItemRow> rows_;
    std::unordered_map<std::int64_t, std::size_t> by_id_;
    std::unordered_map<ItemRarity, std::vector<std::size_t>> by_rarity_;
};

class GameData {
public:
    static GameData Load(IRuntimeDataLoader& loader);
    const ItemsTable& Items() const;
private:
    ItemsTable items_;
};
```

C++ 约束：

- 第一阶段目标 C++17。
- 运行时 JSON / MessagePack 解析库不强绑定。生成代码只定义通用 `IRuntimeDataLoader` / `IRuntimeDataReader`，由业务实现；标准 runtime support 层可提供基于 RapidJSON、msgpack-c 或引擎序列化系统的可选 adapter。按表读取、字段转换和 typed row 构造由生成的 `Load` / `ReadRow` 实现完成。
- `Rows()` / `Size()` 等 public source-order API 应成为 `preserve_source_order = true` 的 opt-in 行为；普通 indexed table 默认只生成 `GetBy*` / `FindBy*`。底层 `std::vector<Row> rows_` 可以继续作为 private owner storage，多个 index 保存 `std::size_t` row handle，以避免行数据被每个索引复制；未 opt-in 时不得暴露 source-order API。
- unordered index 应优先使用 `std::unordered_map`；单字段和复合 key 都应走 hash-based storage，复合 key 需要生成 `struct Key`、`operator==` 和 hash，不能因为复合 key 缺标准库 hash 而退回 `std::map`。当前 `class_levels.by_class_level` 之所以生成 `std::map`，只是现有实现以“复合 key 无 hash”为条件选择了树形 map；这不是目标设计。
- `ordered = true` 的 index 不使用 `std::map` 作为默认结构，而使用 `std::vector<std::pair<Key, std::size_t>>` 排序数组：Load 阶段收集全部 `(key, row_index)`，按 key 字典序稳定排序；exact lookup 通过 `std::lower_bound` 找 equal range；范围查询通过 `lower_bound(lower)` 到 `lower_bound(upper)` 的半开区间实现。生成代码应额外提供 `LowerBoundBy*`、`UpperBoundBy*`、`RangeBy*` 或等价 range view API。
- enum class 需要生成 hash 或转换为底层整数用于 unordered_map。

C++ `linked_field` 生成规则：

- Row/Object struct 保留原始 FK 字段，并生成 `mutable LinkCache <name>_link_`，使 relation getter 可以是 `const` 方法。
- 必填单值 FK 可以生成 `const TargetRow& GetDefaultClass(const GameData& data) const`；为了与运行时错误模型一致，也可生成 error-return 版本，例如 `RuntimeDataResult<std::reference_wrapper<const TargetRow>>`。当前文档不强制异常或 Result 方案，但同一生成器内必须统一。
- optional FK 推荐返回 `const TargetRow*` 或 `std::optional<std::reference_wrapper<const TargetRow>>`；只有合法全空 key 返回空指针 / nullopt，非空 key lookup 失败必须报错。
- list FK 返回 `std::vector<const TargetRow*>` 或轻量 relation view；cache 保存 `std::vector<std::size_t>`，不保存 pointers。
- Table 生成 `void Link(const GameData& data) const` 或 error-return 版本；DataSet 生成 `LinkAll()`，按确定性顺序调用所有 table/object 的 Link。

C++ `text` 翻译 helper 生成规则：

- Row/Object struct 保留 `std::string` raw 字段；对每个 text 字段生成 const helper method。
- 默认 `targets.exports.i18n.mode = "generic"` 时生成抽象 translator interface 或文档化 duck-typed template。若保持 ABI 简单，可使用纯虚接口：

```cpp
class ITextTranslator {
public:
    virtual ~ITextTranslator() = default;
    virtual std::optional<std::string> Translate(
        std::optional<std::string_view> context,
        std::string_view msgid) const = 0;
};

struct ItemRow {
    std::string display_name;
    std::string DisplayNameText(const ITextTranslator& translator) const;
};
```

当 `targets.exports.i18n.mode = "global_function"` 时，helper 直接调用配置的全局函数，不再接受 translator 参数；`cpp_include` 可注入声明所在头文件：

```cpp
// ConfigForge.toml:
// [targets.exports.i18n]
// mode = "global_function"
// function = "GlobalTranslate"
// cpp_include = "\"i18n.hpp\""

struct ItemRow {
    std::string display_name;
    std::string DisplayNameText() const {
        return GlobalTranslate("items.display_name", display_name.c_str());
    }
};
```

`returns = "string"` 表示全局函数总是返回最终文案；`returns = "optional"` 表示返回 `std::optional<std::string>`，helper 会 fallback 到 raw text。

- helper 默认返回 `std::string`，translation missing 时复制 raw 字段作为 fallback。若项目采用异常或 Result 错误模型，translator 可自行抛错；生成器不把翻译缺失视为 runtime data error。
- 不在 row 内保存 locale 或 translated cache；跨 locale cache 由 `ITextTranslator` 实现维护。

## C++ LazyGameData 生成方案

新增 `LazyGameData`，与现有 `GameData` 并存：

```cpp
class LazyGameData {
public:
    explicit LazyGameData(std::shared_ptr<IRuntimeDataLoader> loader);
    const ItemsTable& Items() const;
    const GameRulesRow& GameRules() const;
    void PreloadAll() const;
    void LinkAll() const;

private:
    std::shared_ptr<IRuntimeDataLoader> loader_;
    mutable std::optional<ItemsTable> items_;
    mutable std::optional<GameRulesRow> game_rules_;
};
```

设计细节：

- 默认推荐 `std::shared_ptr<IRuntimeDataLoader>`，因为 lazy facade 必须延长 loader 生命周期；若业务不希望共享所有权，可后续补充 `LazyGameDataRef` 或构造函数接收 `IRuntimeDataLoader&` 并明确调用方负责生命周期。
- 每个 accessor 检查 `std::optional<T>`；未加载时调用现有 `T::Load(*loader_)` 并 `emplace`。异常模型沿用当前 C++ generated code：decode/schema/index 失败抛异常；若后续引入 `RuntimeDataResult<T>`，lazy accessor 应返回 `Result<std::reference_wrapper<const T>>`。
- 多线程第一阶段不承诺；如果需要，后续可为每个 table 加 `std::once_flag` + error propagation 或 mutex。默认文档要求同一个 `LazyGameData` 不并发首次访问同一 table。
- `GameData::Load(loader)` 保持 eager，不保存 loader。`LazyGameData::PreloadAll()` 加载全部表；可选 `ReleaseLoader()` 只有在所有 table 都已 Loaded 后才允许清空 `loader_`，否则后续访问 unloaded table 应报错。
- 为了让 linked relation helper 可同时接受 eager 与 lazy，后续可生成轻量 access interface（例如 `IGameDataAccess` 或模板参数），其中 `Items()` / `Classes()` 返回 const table reference 或抛错。eager `GameData` 与 `LazyGameData` 都实现同名 accessor。
