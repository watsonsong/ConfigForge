# 09k. Codegen C# Backend

本文描述已实现的 C# codegen backend 的生成 API 形态、运行时 reader contract、测试策略和与现有 CodeModel / runtime data exporter 的复用边界；本文作为稳定设计文档维护。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 总览、模块边界和专题索引。
- [`09a-codegen-runtime-reader-contract.md`](09a-codegen-runtime-reader-contract.md) — 语言无关 runtime reader / loader contract、逻辑 table name 与 schema revision 协作。
- [`09f-codegen-core-model-and-options.md`](09f-codegen-core-model-and-options.md) — CodeModel IR、命名、类型映射和通用 writer。
- [`09g-codegen-index-and-relation-helpers.md`](09g-codegen-index-and-relation-helpers.md) — index lookup、foreign-key relation helper 和 row-index cache。
- [`09h-codegen-generator-api-and-tests.md`](09h-codegen-generator-api-and-tests.md) — 新语言生成器接入步骤、diagnostic 和测试组织。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见导出 target、runtime loading 和 generated API 语义。

## 当前状态

C# backend 已实现为独立 codegen type：`csharp`（alias：`cs`），内部生成器命名为 `CSharpCodeGenerator`，输出 `.cs` 源码文件，可被普通 .NET SDK / C# compiler 编译，不绑定 Unity 或特定游戏引擎。Unity / Godot / 自研引擎接入通过业务侧 runtime loader adapter 完成；生成核心代码只依赖 .NET BCL。

第一阶段采用单文件输出 `generated.cs`，覆盖和 Rust / C++ / TypeScript 同等的核心能力：enum、class / row class、ObjectTable、CollectionTable repository、eager `GameData`、`LazyGameData`、unique / non-unique / ordered index、compound key、`linked_field` relation helper、`text` helper，以及 format-agnostic runtime reader contract。

## 设计目标与非目标

### 目标

- 复用现有 `CodeModel`、target tags、runtime data file 逻辑 table name 和 shared `ArtifactSink` 写入路径。
- 默认生成 idiomatic C# 代码，面向 .NET 8 / .NET Standard 2.1 兼容语法子集；如需 Unity 可后续增加 profile option。
- 不把 JSON / MessagePack / binary 解码库写死进生成代码；生成代码只依赖 `IConfigForgeRuntimeDataReader` / `IConfigForgeRuntimeDataLoader`。
- 通过只读 repository API 暴露配置，避免生成代码在加载后被业务误改导致 index / relation cache 不一致。
- 与其他 backend 保持相同语义：validation 在导出前完成，运行时 helper 只做 data/code mismatch 的防御性错误处理。

### 非目标

- 不在第一阶段生成 Unity `ScriptableObject`、`Addressables`、`Resources.Load` 或 `JsonUtility` 绑定。
- 不在第一阶段生成 NuGet package、`.csproj` 模板或 source generator；集成测试可临时创建最小 `.csproj` / `dotnet` harness。
- 不生成异步 API 作为默认入口；C# 第一阶段使用同步 loader。异步 I/O 可在后续通过 `LoadAsync` / `LazyGameData` async profile 扩展。
- 不暴露可变 rows collection；如果业务需要 source-order 访问，遵循 `preserve_source_order = true` opt-in 策略。

## Codegen options

`[targets.exports.csharp]` 支持：

| Option | 默认值 | 说明 |
| --- | --- | --- |
| `namespace` | 通用 `target.namespace` 或 `ConfigForge.Generated` | C# namespace。可由 language option 覆盖通用 namespace。 |
| `nullable` | `true` | 是否输出 `#nullable enable` 并使用 nullable reference annotations。 |
| `access_modifier` | `public` | 顶层 generated type 的访问级别；允许 `public` / `internal`。 |
| `class_name` | `GameData` | root eager facade 名称。 |
| `lazy_class_name` | `LazyGameData` | lazy facade 名称。 |
| `emit_partial` | `true` | 顶层 row/table/facade 是否生成 `partial`，便于业务扩展。 |
| `file_name` | 通用 file name | 输出文件名，默认 `generated.cs`。 |

无效 namespace、非法 identifier、非法 access modifier 应在 `CSharpCodeGenerator.generate_to_sink()` 前通过 validation diagnostic 报错。

## 命名规则

- 类型、属性、方法默认 PascalCase：`ItemsRow`、`ItemsTable`、`GameData.Load`。
- enum 使用 PascalCase type + PascalCase value：`ItemRarity.Common`。
- 私有字段使用 `_camelCase`：`_rows`、`_byId`、`_loader`。
- bool 属性不强制加 `Is` / `Has` 前缀，按 Meta 字段名 PascalCase 输出；只有冲突检测发现非法/重复时按全局 `keyword_conflict` 策略处理。
- `CodeModel` 的 `snake_case` table / field name 统一通过 `naming::make_identifier` 转为 C# identifier，并使用 C# keyword set 做冲突处理。
- `namespace` 既可来自通用 target namespace，也可来自 `[targets.exports.csharp] namespace`。C# backend 不需要像 Unreal Blueprint 一样禁用 namespace。

## 类型映射

| Meta / CodeTypeRef | C# 类型 |
| --- | --- |
| `int` | `long` |
| `float` | `double` |
| `bool` | `bool` |
| `string` / `text` / `asset` | `string` |
| enum | generated enum type |
| struct | generated value class / readonly record struct（第一阶段推荐 class 以避免 nested optional/copy 复杂性） |
| `list<T>` | `T[]` |
| optional value type | `T?` |
| optional reference type | `T?` with `#nullable enable` |

第一阶段建议所有 generated row / struct 使用 sealed partial class，字段生成 get-only auto property：

```csharp
public sealed partial class ItemsRow
{
    public long Id { get; init; }
    public string Name { get; init; } = string.Empty;
    public ItemRarity Rarity { get; init; }
}
```

使用 class + init-only property 的原因：

- 对 Unity / AOT 场景比 record 的 compiler-generated 语义更可控。
- optional reference type 可自然表达为 `string?`。
- relation cache 可作为 private mutable field 放在 row object 内，不影响 public data shape。

## Runtime reader contract

C# generated code 定义轻量同步接口：

```csharp
public interface IConfigForgeRuntimeDataReader
{
    void EnterRequiredField(string name);
    bool EnterOptionalField(string name);
    void ExitField();
    ulong? SchemaRevision();
    int ReadRowCount();
    void EnterRow(int index);
    void ExitRow();
    int ReadListLength();
    void EnterListElement(int index);
    void ExitListElement();
    void BeginRecord(int optionalSlotCount);
    void EndRecord();
    int ReadEnumIndex(IReadOnlyList<string> variants);
    void EnsureExhausted();
    long ReadInt();
    double ReadFloat();
    bool ReadBool();
    string ReadString();
}

public interface IConfigForgeRuntimeDataLoader
{
    IConfigForgeRuntimeDataReader Open(string path);
}
```

`BeginRecord` / `EndRecord`、`SchemaRevision`、`ReadEnumIndex` 与 `EnsureExhausted` 是 generated parser 直接调用的 schema-driven API：JSON / MessagePack reader 可在这些方法中保持轻量 no-op 或 string enum 映射，binary reader 则用它们消费 positional record、校验 schema revision、读取 enum ordinal 并确认 payload 已消费完毕。`ReadNext*`、optional bitmap 等低层 helper 不进入 generated inline contract，只保留在 concrete reader 或薄 adapter 内。

错误模型使用异常还是 Result 需要和 C# 生态习惯权衡。第一阶段推荐生成 `ConfigForgeRuntimeException : Exception` 并在 generated reader / repository 中抛出：

- C# 调用方常规使用 exception 处理 decode / schema / data-code mismatch。
- 避免为每个 generated API 引入自定义 `Result<T>` 包装导致业务调用噪音过高。
- 如后续需要无异常 profile，可增加 `error_model = "result"` option，但不作为 MVP。

## 生成内容

第一阶段单文件输出：

```text
generated.cs
```

建议文件结构：

1. 文件头和 `#nullable enable`。
2. `using System; using System.Collections.Generic;` 等最小 using；global i18n 可追加 `csharp_using`。
3. namespace open。
4. runtime reader / loader / exception contract。
5. enum / struct / row type。
6. compound index key type。
7. repository table type。
8. `IGameDataAccess`、`GameData`、`LazyGameData`。
9. namespace close。

核心 API 示例：

```csharp
public partial class ItemsTable
{
    public const string DataFile = "items";

    private readonly ItemsRow[] _rows;
    private readonly Dictionary<long, int> _byId;
    private readonly Dictionary<ItemRarity, int[]> _byRarity;

    private ItemsTable(ItemsRow[] rows, Dictionary<long, int> byId, Dictionary<ItemRarity, int[]> byRarity)
    {
        _rows = rows;
        _byId = byId;
        _byRarity = byRarity;
    }

    public static ItemsTable Load(IConfigForgeRuntimeDataLoader loader) { ... }
    private static ItemsTable FromRows(ItemsRow[] rows) { ... }
    public int Count => _rows.Length;
    public ItemsRow? RowAt(int index) { ... }
    public ItemsRow? GetById(long id) => _byId.TryGetValue(id, out var index) ? _rows[index] : null;
    public IReadOnlyList<ItemsRow> FindByRarity(ItemRarity rarity) { ... }
}

public partial class GameData : IGameDataAccess
{
    public ItemsTable Items { get; }
    public GameRulesRow GameRules { get; }

    public static GameData Load(IConfigForgeRuntimeDataLoader loader) { ... }
}
```

## Index 生成规则

- unique unordered：`Dictionary<TKey, int>`，value 为 row index。
- non-unique unordered：构建阶段先用 `Dictionary<TKey, List<int>>`，完成后冻结为 `Dictionary<TKey, int[]>`。
- ordered index：`KeyValuePair<TKey, int>[]` 或自定义 entry struct，按 key 排序；lookup 使用 generated binary search helper，避免依赖 LINQ 分配。
- compound key：生成 `readonly struct <Table><Index>Key : IEquatable<...>, IComparable<...>`，实现 `Equals`、`GetHashCode`、`CompareTo`。
- duplicate unique key 在 `FromRows` 阶段抛 `ConfigForgeRuntimeException`，错误信息包含 table / index 名。
- public non-unique / range 返回 `IReadOnlyList<Row>`。第一阶段可复制为 `Row[]`，后续可优化为轻量 readonly view。

## Relation helper

为让 eager 与 lazy 共享 relation helper，生成公共 access interface：

```csharp
public interface IGameDataAccess
{
    ItemsTable Items { get; }
    ClassesTable Classes { get; }
}
```

若 lazy accessor 可能失败且使用异常模型，则 property getter 可以按需加载并抛异常；接口保持同步 property 形态。

Row relation helper 规则：

- 必填单值 FK：`ClassesRow GetDefaultClass(IGameDataAccess data)`，找不到目标时抛 `ConfigForgeRuntimeException`。
- optional FK：`ClassesRow? GetDefaultClass(IGameDataAccess data)`，source key 全空时返回 `null`；非空 key 找不到仍抛错。
- list FK：`IReadOnlyList<ClassesRow> GetStarterClasses(IGameDataAccess data)`，保持 source list 顺序。
- cache 保存 target row index：单值用 `int` sentinel，list 用 `int[]?`。不缓存目标 row 引用，避免生命周期和 mutable repository 问题。
- `Table.Link(IGameDataAccess data)` 遍历 rows 预解析；`GameData.LinkAll()` 按 CodeModel table 顺序调用。

## Text helper

默认 generic translator：

```csharp
public interface ITextTranslator
{
    string? Translate(string? context, string msgid);
}

public string NameText(ITextTranslator translator)
{
    return translator.Translate("items.name", Name) ?? Name;
}
```

`targets.exports.i18n.mode = "global_function"` 时可通过 C# option 增加 `csharp_using` / `function` 生成直接调用：

```toml
[targets.exports.i18n]
mode = "global_function"
function = "GameI18n.Translate"
returns = "optional"
csharp_using = "Game.Runtime.I18n"
```

若 `returns = "string"`，helper 直接返回全局函数结果；若 `returns = "optional"`，`null` 时 fallback raw text。

## LazyGameData

第一阶段生成同步 lazy facade：

```csharp
public sealed partial class LazyGameData : IGameDataAccess
{
    private readonly IConfigForgeRuntimeDataLoader _loader;
    private ItemsTable? _items;

    public ItemsTable Items { get { ... } }
    public GameData PreloadAll() { ... }
}
```

设计约束：

- Lazy cache 不缓存异常；加载失败时字段保持 `null`，下次访问可重试。
- `PreloadAll()` 生成完整 eager `GameData`，并调用 `LinkAll()`。
- Relation helper 接受 `IGameDataAccess`，因此同一 row helper 可用于 eager `GameData` 和 lazy `LazyGameData`。
- 暂不生成 async lazy，避免同步/异步 API 混淆。后续可新增 `async = true` profile，生成 `Task<ItemsTable> GetItemsAsync()`。

## 文件组织与 Rust 模块结构

新增源码建议：

```text
src/codegen/csharp/
  mod.rs        # CSharpCodeGenerator、options、render orchestration
  types.rs      # CSharpNames、type mapping、key expr / params
  symbols.rs    # generated symbol collision validation
  tests.rs      # unit tests
```

接入点：

- `src/codegen/mod.rs` 注册 `CSharpCodeGenerator` 到 `builtin_generators()`。
- `CodegenOptions::for_language("csharp")` 返回 C# 默认命名：type/field/function/constant PascalCase。
- `CodeExportTarget` 使用默认 `generated.cs`，支持 `file_name` 覆盖。

## 测试策略

单元测试：

- language 注册和 option 解析。
- 默认生成 `.cs` 文件名和 file header。
- enum / struct / row / ObjectTable / CollectionTable snapshot。
- 类型映射覆盖 primitive、list、optional、enum、struct、text、asset。
- unique / non-unique / ordered / compound index source 断言。
- relation helper、`IGameDataAccess`、`LinkAll`、`LazyGameData` source 断言。
- `text` generic / global_function helper source 断言。
- symbol collision：C# keyword、property/helper/index/relation/cache 冲突。
- `generate_to_sink()` 与测试专用 `generate()` 输出等价。

集成 / harness：

- 临时 project：`type = "csharp"` 能生成 `.cs`。
- 若本机有 `dotnet`，临时创建最小 console project：`dotnet new console` 或直接写 `.csproj`，编译 generated code + mock reader，运行加载 / index / relation / text helper smoke。
- standard_project 已加入 `out/client_code/csharp/reference_game_config.cs`，长期回归使用临时 `.csproj` + `System.Text.Json` reader harness 覆盖 JSON runtime data、index、relation、text helper、lazy cache 和 `PreloadAll()`。
- 没有 `dotnet` 时不失败，只跳过 compiler harness；Rust unit 和内容断言仍必须通过。

## 后续扩展

- Unity profile：`unity = true` 时可控制 namespace、nullable、`readonly struct`、`List<T>` 兼容性，以及 AOT-friendly dictionary initialization。
- Async profile：生成 `Task<GameData> LoadAsync(...)` 和 async lazy accessor。
- Result error model：生成 `ConfigForgeResult<T>`，用于不希望 exception 穿透 gameplay hot path 的项目。
- Multi-file output：按 `Runtime.cs`、`Types.cs`、`Tables.cs`、`GameData.cs` 拆分，并在 artifact writer 中输出目录树。
- Source generator / analyzer：可后续独立项目化，不进入 MVP。
