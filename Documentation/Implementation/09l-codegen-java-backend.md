# 09l. Codegen Java Backend

本文描述规划中的 Java codegen backend 的实现设计、生成 API 形态、运行时 reader contract、测试策略和与现有 CodeModel / runtime data exporter 的复用边界。分步实现清单放在 [`../Todo/codegen-java-backend-todo.md`](../Todo/codegen-java-backend-todo.md)；本文作为稳定设计文档维护。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 总览、模块边界和专题索引。
- [`09a-codegen-runtime-reader-contract.md`](09a-codegen-runtime-reader-contract.md) — 语言无关 runtime reader / loader contract、逻辑 table name 与 schema revision 协作。
- [`09f-codegen-core-model-and-options.md`](09f-codegen-core-model-and-options.md) — CodeModel IR、命名、类型映射和通用 writer。
- [`09g-codegen-index-and-relation-helpers.md`](09g-codegen-index-and-relation-helpers.md) — index lookup、foreign-key relation helper 和 row-index cache。
- [`09h-codegen-generator-api-and-tests.md`](09h-codegen-generator-api-and-tests.md) — 新语言生成器接入步骤、diagnostic 和测试组织。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见导出 target、runtime loading 和 generated API 语义。

## 当前状态

Java backend 尚未实现。目标是新增独立 codegen type：`java`，内部生成器命名为 `JavaCodeGenerator`，输出 JDK-only `.java` 源码文件，可被普通 `javac` / Maven / Gradle 工程编译，不绑定 Android、Spring、libGDX 或特定游戏引擎。Android / 服务器 / 自研引擎接入通过业务侧 runtime loader adapter 完成；生成核心代码只依赖 JDK 标准库。

第一阶段采用单文件输出 `GeneratedGameConfig.java`。由于 Java 每个文件只能有一个 public top-level type，生成代码采用 **public root class + nested public static types** 形态：root class 承载 runtime contract、enum、row/value type、table repository、`GameData`、`LazyGameData`、compound key 和 helper。业务侧可通过 `GeneratedGameConfig.GameData.load(loader)`、`GeneratedGameConfig.ItemsRow` 等访问生成 API。

功能目标追齐 Rust / C++ / TypeScript / C# format-agnostic backend MVP：enum、value struct / row class、ObjectTable、CollectionTable repository、eager `GameData`、`LazyGameData`、unique / non-unique / ordered index、compound key、`linked_field` relation helper、`text` helper，以及 format-agnostic runtime reader contract。

## 设计目标与非目标

### 目标

- 复用现有 `CodeModel`、target tags、runtime data file 逻辑 table name 和 shared `ArtifactSink` 写入路径。
- 默认生成 idiomatic Java 代码，第一阶段面向 Java 11+ 语法子集，避免 record / sealed class 等较新语法，便于 Android / 老项目接入。
- 不把 JSON / MessagePack / binary 解码库写死进生成代码；生成代码只依赖 `ConfigForgeRuntimeDataReader` / `ConfigForgeRuntimeDataLoader`。
- 通过只读 repository API 暴露配置，避免生成代码在加载后被业务误改导致 index / relation cache 不一致。
- 与其他 backend 保持相同语义：validation 在导出前完成，运行时 helper 只做 data/code mismatch 的防御性错误处理。

### 非目标

- 不在第一阶段生成 Android `AssetManager`、Resources、Gson、Jackson、Fastjson、Spring Resource 或 libGDX 绑定。
- 不在第一阶段生成 Maven / Gradle project 模板、annotation processor 或 Java source generator；集成测试可临时创建最小 `javac` harness。
- 不生成异步 API 作为默认入口；Java 第一阶段使用同步 loader。异步 I/O 可在后续通过 `CompletableFuture` profile 扩展。
- 不引入 `javax.annotation` / JetBrains / AndroidX nullability annotation 依赖；optional 字段使用 boxed primitive 或 nullable reference，缺失语义由生成文档和 helper 行为约定。
- 不暴露可变 rows collection；如果业务需要 source-order 访问，遵循 `preserve_source_order = true` opt-in 策略。

## Codegen options

建议在 `[targets.exports.java]` 下支持：

| Option | 默认值 | 说明 |
| --- | --- | --- |
| `package` | 通用 `target.namespace` 转换或 `configforge.generated` | Java package。可由 language option 覆盖通用 namespace。 |
| `root_class_name` | `GeneratedGameConfig` 或由 `file_name` stem 推导 | 唯一 public top-level class 名，必须与 `.java` 文件名一致。 |
| `game_data_class_name` | `GameData` | nested eager facade 名称。 |
| `lazy_class_name` | `LazyGameData` | nested lazy facade 名称。 |
| `access_modifier` | `public` | root class 访问级别；第一阶段允许 `public` / package-private（空字符串或 `package`）。 |
| `emit_final` | `true` | row/value/table/facade 是否默认生成 `final`。 |
| `file_name` | 通用 file name | 输出文件名，默认 `GeneratedGameConfig.java`；若设置 `.java` stem，默认同步为 `root_class_name`。 |

无效 package、非法 identifier、非法 access modifier、`file_name` stem 与 `root_class_name` 不一致应在 `JavaCodeGenerator.generate_to_sink()` 前通过 validation diagnostic 报错。

## 命名规则

- root / nested class、enum 默认 PascalCase：`GeneratedGameConfig`、`ItemsRow`、`ItemsTable`、`GameData`。
- enum value 默认 UPPER_SNAKE_CASE：`COMMON`、`RARE`，并在 enum reader 中按 Meta canonical value 映射。
- public method 默认 lowerCamelCase：`load`、`getById`、`findByRarity`、`preloadAll`。
- field / private cache 默认 lowerCamelCase：`rows`、`byId`、`loader`；避免 Java 生态中不常见的 `_field`。
- constant 默认 UPPER_SNAKE_CASE：`DATA_FILE`。
- `CodeModel` 的 `snake_case` table / field / index name 统一通过 `naming::make_identifier` 转为 Java identifier，并使用 Java keyword set 做冲突处理。

## 类型映射

| Meta / CodeTypeRef | Java 类型 |
| --- | --- |
| `int` | `long` |
| `float` | `double` |
| `bool` | `boolean` |
| `string` / `text` / `asset` | `String` |
| enum | generated nested enum type |
| struct | generated nested final static class |
| `list<int>` | `long[]` |
| `list<float>` | `double[]` |
| `list<bool>` | `boolean[]` |
| `list<T>` for reference / enum / struct | `T[]` |
| optional primitive | boxed type：`Long` / `Double` / `Boolean` |
| optional reference type | nullable `String` / generated type / array |

第一阶段所有 generated row / struct 使用 nested `public static final class`，字段 private final，暴露 getter：

```java
public static final class ItemsRow {
    private final long id;
    private final String name;
    private final ItemRarity rarity;

    private ItemsRow(long id, String name, ItemRarity rarity) {
        this.id = id;
        this.name = name;
        this.rarity = rarity;
    }

    public long getId() { return id; }
    public String getName() { return name; }
    public ItemRarity getRarity() { return rarity; }
}
```

使用 class + private constructor 的原因：

- 避免 Java record 对 Java 16+ 的要求。
- optional primitive 可自然表达为 boxed nullable。
- relation cache 可作为 private mutable field 放在 row object 内，不影响 public data shape。

## Runtime reader contract

Java generated code 定义轻量同步接口：

```java
public interface ConfigForgeRuntimeDataReader {
    void enterRequiredField(String name);
    boolean enterOptionalField(String name);
    void exitField();
    int readRowCount();
    void enterRow(int index);
    void exitRow();
    int readListLength();
    void enterListElement(int index);
    void exitListElement();
    long readInt();
    double readFloat();
    boolean readBool();
    String readString();
}

public interface ConfigForgeRuntimeDataLoader {
    ConfigForgeRuntimeDataReader open(String path);
}

public static final class ConfigForgeRuntimeException extends RuntimeException {
    public ConfigForgeRuntimeException(String message) { super(message); }
    public ConfigForgeRuntimeException(String message, Throwable cause) { super(message, cause); }
}
```

错误模型使用 unchecked exception：

- Java 调用方常规使用 `RuntimeException` 处理 decode / schema / data-code mismatch，避免每个 generated API 强制声明 `throws`。
- 避免为每个 generated API 引入 `Result<T>` 或 checked exception 导致业务调用噪音过高。
- 如后续需要 checked profile，可增加 `checked_exceptions = true` option，但不作为 MVP。

## 生成内容

第一阶段单文件输出：

```text
GeneratedGameConfig.java
```

建议文件结构：

1. 文件头。
2. `package ...;`。
3. `import java.util.*;` 等最小 JDK import；global i18n 可追加 `java_import`。
4. `public final class GeneratedGameConfig` open。
5. runtime reader / loader / exception contract。
6. enum / struct / row type。
7. compound index key type。
8. repository table type。
9. `GameDataAccess`、`GameData`、`LazyGameData`。
10. root class close。

核心 API 示例：

```java
public final class GeneratedGameConfig {
    public interface GameDataAccess {
        ItemsTable getItems();
        GameRulesRow getGameRules();
    }

    public static final class ItemsTable {
        public static final String DATA_FILE = "items";

        private final ItemsRow[] rows;
        private final Map<Long, Integer> byId;
        private final Map<ItemRarity, int[]> byRarity;

        private ItemsTable(ItemsRow[] rows, Map<Long, Integer> byId, Map<ItemRarity, int[]> byRarity) { ... }

        public static ItemsTable load(ConfigForgeRuntimeDataLoader loader) { ... }
        private static ItemsTable fromRows(ItemsRow[] rows) { ... }
        public int size() { return rows.length; }
        public ItemsRow rowAt(int index) { ... }
        public ItemsRow getById(long id) { ... }
        public List<ItemsRow> findByRarity(ItemRarity rarity) { ... }
    }

    public static final class GameData implements GameDataAccess {
        public ItemsTable getItems() { ... }
        public GameRulesRow getGameRules() { ... }
        public static GameData load(ConfigForgeRuntimeDataLoader loader) { ... }
    }
}
```

## Index 生成规则

- unique unordered：`Map<TKey, Integer>`，value 为 row index。
- non-unique unordered：构建阶段先用 `Map<TKey, List<Integer>>`，完成后冻结为 `Map<TKey, int[]>`。
- ordered index：生成 per-index entry class，例如 `ItemsByLevelEntry`，字段为 key + row index；按 key 排序；lookup 使用 generated binary search helper，避免 stream / lambda 分配。
- primitive key 使用 boxed map key：`Long` / `Double` / `Boolean`。`double` key 与其他 backend 一样只用于可比较/可 hash 的 validated value；NaN / non-finite 已在 validation 阶段拒绝。
- compound key：生成 `private static final class <Table><Index>Key implements Comparable<...>`，实现 `equals`、`hashCode`、`compareTo`。
- duplicate unique key 在 `fromRows` 阶段抛 `ConfigForgeRuntimeException`，错误信息包含 table / index 名。
- public non-unique / range 返回 `List<Row>`。第一阶段可复制为 `ArrayList<Row>` / `Collections.unmodifiableList(...)`，后续可优化为轻量 readonly view。

## Relation helper

为让 eager 与 lazy 共享 relation helper，生成公共 access interface：

```java
public interface GameDataAccess {
    ItemsTable getItems();
    ClassesTable getClasses();
}
```

Row relation helper 规则：

- 必填单值 FK：`ClassesRow getDefaultClass(GameDataAccess data)`，找不到目标时抛 `ConfigForgeRuntimeException`。
- optional FK：`ClassesRow getDefaultClass(GameDataAccess data)`，source key 全空时返回 `null`；非空 key 找不到仍抛错。
- list FK：`List<ClassesRow> getStarterClasses(GameDataAccess data)`，保持 source list 顺序。
- cache 保存 target row index：单值用 `int` sentinel，list 用 nullable `int[]`。不缓存目标 row 引用，避免生命周期和 mutable repository 问题。
- `Table.link(GameDataAccess data)` 遍历 rows 预解析；`GameData.linkAll()` 按 CodeModel table 顺序调用。

## Text helper

默认 generic translator：

```java
public interface TextTranslator {
    String translate(String context, String msgid);
}

public String nameText(TextTranslator translator) {
    String translated = translator.translate("items.name", name);
    return translated != null ? translated : name;
}
```

`targets.exports.i18n.mode = "global_function"` 时可通过 Java option 增加 `java_import` / `function` 生成直接调用：

```toml
[targets.exports.i18n]
mode = "global_function"
function = "GameI18n.translate"
returns = "optional"
java_import = "game.runtime.i18n.GameI18n"
```

若 `returns = "string"`，helper 直接返回全局函数结果；若 `returns = "optional"`，`null` 时 fallback raw text。第一阶段不生成 `Optional<String>`，避免热路径分配和 API 噪音。

## LazyGameData

第一阶段生成同步 lazy facade：

```java
public static final class LazyGameData implements GameDataAccess {
    private final ConfigForgeRuntimeDataLoader loader;
    private ItemsTable items;

    public ItemsTable getItems() {
        if (items != null) {
            return items;
        }
        ItemsTable loaded = ItemsTable.load(loader);
        items = loaded;
        return loaded;
    }

    public GameData preloadAll() { ... }
}
```

设计约束：

- Lazy cache 不缓存异常；加载失败时字段保持 `null`，下次访问可重试。
- `preloadAll()` 生成完整 eager `GameData`，并调用 `linkAll()`。
- Relation helper 接受 `GameDataAccess`，因此同一 row helper 可用于 eager `GameData` 和 lazy `LazyGameData`。
- 暂不生成 async lazy，避免同步/异步 API 混淆。后续可新增 `async = true` profile，生成 `CompletableFuture<ItemsTable> getItemsAsync()`。

## 文件组织与 Rust 模块结构

新增源码建议：

```text
src/codegen/java/
  mod.rs        # JavaCodeGenerator、options、render orchestration
  types.rs      # JavaNames、type mapping、key expr / params
  symbols.rs    # generated symbol collision validation
  tests.rs      # unit tests
```

接入点：

- `src/codegen/mod.rs` 注册 `JavaCodeGenerator` 到 `builtin_generators()`。
- `CodegenOptions::for_language("java")` 返回 Java 默认命名：type PascalCase、field/function camelCase、constant screaming snake case。
- `CodeExportTarget` 使用默认 `GeneratedGameConfig.java`，支持 `file_name` 覆盖；`file_name` stem 必须与 root public class 一致。

## 测试策略

单元测试：

- language 注册和 option 解析。
- 默认生成 `.java` 文件名和 file header。
- enum / struct / row / ObjectTable / CollectionTable snapshot。
- 类型映射覆盖 primitive、list、optional、enum、struct、text、asset。
- unique / non-unique / ordered / compound index source 断言。
- relation helper、`GameDataAccess`、`linkAll`、`LazyGameData` source 断言。
- `text` generic / global_function helper source 断言。
- symbol collision：Java keyword、getter/helper/index/relation/cache 冲突。
- `generate_to_sink()` 与测试专用 `generate()` 输出等价。

集成 / harness：

- 临时 project：`type = "java"` 能生成 `.java`。
- 若本机有 `javac` / `java`，临时创建最小 harness：编译 generated code + generic in-memory reader，运行加载 / index / relation / text helper / lazy smoke。
- standard_project 后续可加入 `out/client_code/java/GeneratedGameConfig.java`，长期回归先使用 in-memory value reader 验证 generated parser 和 repository API；真实 JSON / MessagePack reader adapter 可在 `runtime/java/` 落地后替换 harness。
- 没有 JDK 时不失败，只跳过 Java compiler harness；Rust unit 和内容断言仍必须通过。

## 后续扩展

- Android profile：控制 package、nullable annotation、collection 返回类型和资源加载约定，但 generated core 仍不直接绑定 Android API。
- Async profile：生成 `CompletableFuture<GameData> loadAsync(...)` 和 async lazy accessor。
- Multi-file output：按 `Runtime.java`、`Types.java`、`Tables.java`、`GameData.java` 拆分，并在 artifact writer 中输出目录树；需要重新设计 public top-level type 与 package-private helper 边界。
- Runtime adapter：在 `runtime/java/json_reader`、`runtime/java/messagepack_reader` 中提供 JDK / Jackson / Gson / MessagePack 可选 adapter，而不是写死进 generated core。
- Nullability annotation profile：可选生成 JetBrains / AndroidX / JSpecify annotation，但必须显式配置依赖。
