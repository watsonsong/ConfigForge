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

## Unreal C++ backend

Unreal Engine 环境不由标准 `cpp` backend 增加一个“大开关”来兼容，而是使用独立 export type：`type = "cpp_unreal"`，内部实现命名为 `CppUnrealCodeGenerator`，输出目录仍由当前 `[[targets.exports]]` 的 `output` 指定。原因：

- 标准 C++ backend 的目标是引擎无关 C++17，public API 使用 `std::vector`、`std::unordered_map`、`std::string`、`std::unique_ptr`、异常等标准库约定。
- Unreal 项目倾向于使用 `TArray`、`TMap`、`FString`、`TUniquePtr` / `TSharedPtr`、`check` / `ensure` / `UE_LOG`、`TEXT()` 字面量和 PascalCase + 类型前缀命名；同一个 public API 混用两套习惯会降低接入体验。
- Blueprint 反射还要求 `USTRUCT` / `UCLASS` / `UFUNCTION` / `UPROPERTY` / `GENERATED_BODY()` 等 UnrealHeaderTool 可解析形态，这会显著影响文件布局、include、类型命名和可暴露 API，不适合塞进标准 C++ emitter 的局部分支。

### Unreal 习惯和生成约束

Unreal backend 需要遵守以下生成约束：

- **容器与字符串**：`std::vector<T>` 映射为 `TArray<T>`；unordered index 的唯一 key 映射为 `TMap<Key, int32>` 或 `TMap<Key, int64>` row handle；非唯一 key 映射为 `TMap<Key, TArray<int32>>`。配置文本字段优先使用 `FString`；如果未来区分本地化 display text，可额外支持 `FText`；name-like 轻量标识符可由选项映射到 `FName`，但默认不自动把 Meta string 改成 `FName`。
- **有序索引**：继续使用排序数组策略，不使用 `TMap` 的排序迭代语义承载 range query。存储形态为 `TArray<TPair<Key, int32>>`；Load 后调用 `StableSort`；`FindBy*`、`LowerBoundBy*`、`UpperBoundBy*`、`RangeBy*` 使用手写二分或 `Algo::LowerBoundBy` / `Algo::UpperBoundBy`。等价 key 的相对顺序必须保持 source row order。
- **复合 key**：生成 `F<Table><Index>Key` value struct，提供 `operator==`、`operator<` 和非成员 `GetTypeHash(const FKey&)`，使其可作为 `TMap` key。hash combine 使用 Unreal 提供的 hash combine helper 或显式组合各字段 `GetTypeHash`。
- **错误模型**：Unreal 模块通常关闭或避免依赖 C++ exceptions。`cpp_unreal` 不应默认生成 `throw std::runtime_error`。第一阶段 containers 模式推荐生成 `FConfigForgeResult<T>` / `FConfigForgeStatus` 或 `bool + OutError` 形态；不可恢复的内部不变量可用 `check`，可诊断但不中断的集成问题可用 `ensure` / `UE_LOG`。所有字符串字面量使用 `TEXT("...")`。
- **命名**：类型使用 Unreal 前缀：普通 value struct 使用 `F`，enum 使用 `E`，Blueprint function library 使用 `U`；bool 字段生成 `b` 前缀；函数、类型、属性默认 PascalCase。内部私有字段不沿用标准 C++ backend 的 `rows_` snake_case，而使用 `Rows` / `ById` 或明确 `Private` 后缀以避开 UPROPERTY 和方法名冲突。
- **文件布局**：containers 模式可生成普通 `.h` / `.cpp`；Blueprint 模式必须生成 UnrealHeaderTool 可处理的 header，包含 `#pragma once`、模块 API 宏、`.generated.h` 必须位于 include 列表末尾。生成器需要配置 `api_macro`，例如 `GAMECONFIG_API`，用于 `UCLASS` / `USTRUCT` 导出。
- **反射限制**：Blueprint public API 不暴露 `std::*`、裸 row pointer、模板返回值、`std::optional`、`std::reference_wrapper` 或复杂迭代器；用 `bool` 返回成功状态，结果通过 `UPARAM(ref)` / out 参数返回，或返回 Blueprint 支持的 `USTRUCT` / `TArray<USTRUCT>`。

### 支持级别

`cpp_unreal` 至少提供两个支持级别：

```toml
[[targets.exports]]
type = "cpp_unreal"
output = "Source/GameConfig/Generated"
module_name = "GameConfig"

[targets.exports.cpp_unreal]
support = "containers" # 或 "blueprint"
api_macro = "GAMECONFIG_API"
namespace = "GameConfig"
```

#### Level 1: `support = "containers"`

目标是让 C++ gameplay code 在 Unreal module 内自然使用生成代码，但不要求 Blueprint 可见：

- 生成普通 C++ `struct` / `class` / repository / `GameData` / `LazyGameData`，不加 `USTRUCT` / `UCLASS`。
- 类型和容器使用 Unreal API：`TArray`、`TMap`、`TPair`、`FString`、`TOptional`、`TUniquePtr` / `TSharedPtr`、`int32` / `int64`。
- reader/loader contract 改成 Unreal 风格，例如 `TUniquePtr<IConfigForgeRuntimeDataReader> Open(const FString& Path, FConfigForgeError& OutError)`；生成代码仍只依赖逻辑 table name，不绑定 Asset Manager、Pak、本地文件或网络。
- relation helper 和 text helper 复用标准 backend 的语义，但返回 Unreal 类型；例如 text helper 默认返回 `FString`，可选对接 `FText` translator。
- 索引 API 可以返回 `const FItemRow*` 给 C++ 使用；但这些指针不出现在 Blueprint 层。

#### Level 2: `support = "blueprint"`

目标是在 containers 模式基础上增加 Blueprint 可见 facade：

- row / object / fragment value type 生成 `USTRUCT(BlueprintType)`，首行 `GENERATED_BODY()`；字段生成 `UPROPERTY(BlueprintReadOnly, Category="ConfigForge")`。
- enum 生成 `UENUM(BlueprintType)`，枚举值按 Unreal 习惯生成 `EItemRarity::Common` 这类强类型枚举；必要时为 display name 添加 metadata。
- table repository 本体仍可保持非 UObject C++ 类型，避免大量配置行进入 GC 管理；Blueprint 层通过 `UBlueprintFunctionLibrary` 或 `UObject` facade 暴露静态/实例函数。
- lookup 函数生成 Blueprint 友好形态，例如：

```cpp
UCLASS()
class GAMECONFIG_API UGameConfigBlueprintLibrary : public UBlueprintFunctionLibrary
{
    GENERATED_BODY()

public:
    UFUNCTION(BlueprintPure, Category="ConfigForge")
    static bool GetItemsById(const FItemsTable& Table, int64 Id, FItemsRow& OutRow);

    UFUNCTION(BlueprintPure, Category="ConfigForge")
    static void FindItemsByRarity(const FItemsTable& Table, EItemRarity Rarity, TArray<FItemsRow>& OutRows);
};
```

- 当前 Blueprint facade 只包装已经加载好的 repository 查询，调用方显式传入 `F<Table>Table`，不会隐式触发 I/O，因此使用 `BlueprintPure`。unique 查询返回 `bool` 并通过 `OutRow` 复制结果；non-unique / ordered range 查询通过 `TArray<F<Row>>` out 参数复制结果。可能进行加载 / I/O 的 Blueprint API 若未来加入，应使用 `BlueprintCallable`，让执行顺序显式。
- 当前不为 `LazyGameData` 生成 Blueprint 加载入口；C++ 层仍可用 `FLazyGameData`。若未来暴露 lazy/eager 加载给 Blueprint，应明确初始化入口，例如 `LoadGameConfig` / `PreloadAll`，不在纯 getter 中隐式打开 runtime data。
- 若暴露全局 singleton，需要由项目 adapter 决定生命周期；生成代码不直接创建 `UGameInstanceSubsystem`，但可以提供可被 subsystem 包装的 facade 类。

### 与现有 C++ backend 的复用边界

`cpp_unreal` 应复用核心语义而不是复用标准 C++ 文本模板：

- **复用**：`CodeExportInput`、`CodeModel`、target tags 过滤后的表/字段/index/foreign key IR、命名冲突检查框架、索引与 relation helper 的语言无关规划、runtime reader 的 Meta-agnostic 分层原则、sink-based streaming 输出基础设施。
- **抽象成共享策略**：index 构建流程、ordered range API 的语义、复合 key 需要 equality / ordering / hash 的判定、`GameData` / `LazyGameData` eager/lazy 结构，可以提取为 backend-neutral helper 或 emitter plan。
- **不直接复用**：`CppNames` 的 `std::*` 类型映射、include 列表、异常生成、snake_case private 字段、`std::optional` / `std::unique_ptr` / `std::shared_ptr` 形态、标准 C++ 的 reader interface 和所有 Blueprint 相关输出。
- **测试复用**：继续使用同一 Meta fixture 验证导出可见性、索引、foreign key、text helper；新增 Unreal 专属 snapshot 断言 `TArray` / `TMap` / `GetTypeHash` / `USTRUCT(BlueprintType)` / `.generated.h` include 顺序 / 无 `std::` 和无 `throw`。
