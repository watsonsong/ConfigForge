# 09d. Codegen TypeScript Backend

本文从 Codegen 模块详细设计中拆出，专门描述 TypeScript backend 的 generated API、async/sync loading、lazy loading 和 relation helper。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 总览、IR、命名、冲突检测和测试总览。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、runtime loading 和 generated API 语义。
- [`13-runtime-binary-format-detail-design.md`](13-runtime-binary-format-detail-design.md) — `bin` runtime data schema revision 与 positional reader contract。

## 当前状态

TypeScript backend 已实现 MVP：生成 interface/class、repository、`GameData`、`LazyGameData`、index lookup、`linked_field` relation helper、`linkAll`、`text` translation helper，并提供 `loadSync`、lazy sync accessor、`preloadAllSync` 与 `linkAllSync`。

## TypeScript sync loading contract

- `RuntimeDataLoader.open(path)` 可以返回 `RuntimeDataReader` 或 `Promise<RuntimeDataReader>`。
- `GameData.load(loader)` 总是 async，兼容同步和异步 loader。
- `GameData.loadSync(loader)` 要求 loader 同步返回 reader；如果返回 Promise，generated runtime helper 必须抛错，避免业务误以为同步加载已完成。
- `LazyGameData` 同时提供 async table accessor 与 `xxxSync()` accessor。
- `preloadAllSync()` / `linkAllSync()` 只适用于同步 loader 场景。

## TypeScript 生成内容

当前实现采用单文件输出：`generated.ts`。以下多文件布局仍是后续拆分方向：

```text
generated/ts/
  types.ts
  loader.ts
  tables.ts
  index.ts
```

核心形态：

```ts
export interface RuntimeDataReader {
  enterField(name: string): boolean;
  enterIndex(index: number): void;
  exit(): void;
  length(): number;
  asNumber(): number;
  asBool(): boolean;
  asString(): string;
  enterRequiredField?(name: string): void;
  enterOptionalField?(name: string): boolean;
  exitField?(): void;
  readEnumIndex?(variants: readonly string[]): number;
  readRowCount?(): number;
  readListLength?(): number;
}

export interface RuntimeDataLoader {
  open(path: string): Promise<RuntimeDataReader> | RuntimeDataReader;
}

export type ItemRarity = "common" | "rare";

export interface ItemRow {
  id: number;
  name: string;
  rarity: ItemRarity;
}

export class ItemsTable {
  static readonly dataFile = "items";

  private readonly rows_: ItemRow[];
  private readonly byId: Map<number, ItemRow>;
  private readonly byRarity: Map<ItemRarity, ItemRow[]>;

  static async load(loader: RuntimeDataLoader): Promise<ItemsTable>;
  private static readRow(reader: RuntimeDataReader): ItemRow;
  rows(): readonly ItemRow[];
  getById(id: number): ItemRow | undefined;
  findByRarity(rarity: ItemRarity): readonly ItemRow[];
}

export class GameData {
  readonly items: ItemsTable;
  static async load(loader: RuntimeDataLoader): Promise<GameData>;
}
```

TypeScript 约束：

- 第一阶段生成 ESM。
- `RuntimeDataLoader.open` 同时允许同步值和 `Promise`，生成代码内部统一 `await`。
- JSON / MessagePack 底层解码由 `RuntimeDataLoader` / `RuntimeDataReader` 实现负责；生成核心代码不绑定 MessagePack npm 包。
- runtime data shape `{ meta, rows }` 的导航、字段 runtime type guard、primitive 转换、typed rows 构造由生成代码负责；loader 实现不随 Meta 新增表或字段而变化。

TypeScript `linked_field` 生成规则：

当前实现状态：TypeScript 已生成 relation method、row-index cache、table-level `link(data)` 和 `GameData.linkAll()`，与 Rust/C++ linked helper 语义保持一致。

- Row/Object 仍以 interface + plain object 输出；一旦启用 `linked_field`，该 row/object interface 会额外包含 relation method、cache slot 和 `link(data)` method。reader 在构造 object literal 时初始化 cache，并把 relation/link method 指向模块级函数，避免为每个 row 分配闭包。
- 原始 FK 字段保持普通属性，例如 `defaultClassId: number`；helper 使用命名后的 method，例如 `defaultClass(data: GameData): ClassesRow`。由于解析需要传入 `GameData`，不生成 ES getter 属性。
- cache 字段由 `linked_field` 派生，例如 `defaultClassLink`。单值/可空 FK 使用 `number` sentinel：`LINK_UNRESOLVED = -1`、`LINK_EMPTY = -2`、`rowIndex >= 0`；list FK 使用 `number[] | undefined`，仅在首次解析时分配数组。cache 只保存 row index，不缓存目标 row 对象引用。
- optional FK 返回 `TargetRow | undefined`；list FK 返回 `readonly TargetRow[]`，内部按缓存 row indexes 映射到目标 repository `rowAt` 结果，并返回新数组，不暴露可变内部缓存。
- 目标 unique index 额外生成私有语义的 `indexOfBy*` helper，relation method 首次访问或 `link(data)` 预解析时用它填充 cache。
- Table 生成 `link(data: GameData): void` 遍历 rows；ObjectTable row 生成 `link(data)` method；DataSet 生成 `linkAll(): void`。若 lookup 失败，当前生成代码抛出 `Error`，错误信息包含 source relation 与 target index。

TypeScript `text` 翻译 helper 生成规则：

- Interface 中保留 raw 字段，例如 `displayName: string`；helper 可以作为 row object method 生成，也可以生成模块级函数。为了与现有 relation helper 的 plain object 策略一致，推荐在需要 helper 时给 object literal 绑定模块级函数引用，避免每条 row 创建闭包。
- 默认 `targets.code.i18n.mode = "generic"` 时生成 translator function type：

```ts
export interface TextTranslator {
  translate(context: string | undefined, msgid: string): string | undefined;
}

export interface ItemRow {
  displayName: string;
  displayNameText(translator: TextTranslator): string;
}
```

当 `targets.code.i18n.mode = "global_function"` 时，helper 直接调用配置的全局函数，不再接受 translator 参数；`typescript_import` 可注入 import：

```ts
// ConfigForge.toml:
// [targets.code.i18n]
// mode = "global_function"
// function = "globalTranslate"
// typescript_import = "import { globalTranslate } from './i18n'"

export function itemsDisplayNameText(value: { displayName: string }): string {
  return globalTranslate("items.display_name", value.displayName);
}
```

`returns = "string"` 表示全局函数总是返回最终文案；`returns = "optional"` 表示返回 `string | undefined`，helper 会 fallback 到 raw text。

- helper 默认同步返回 string。异步远程翻译不应混入基础 row helper；业务应在 translator 内预加载 catalog，或未来通过单独 option 生成 `displayNameTextAsync`。
- helper fallback 到 raw text，不抛出翻译缺失错误；若 translator 抛错，错误按普通业务异常传播。
- helper method、relation method、`link(data)` 和 cache slot 同属 row/object object literal scope；命名冲突必须在生成前发现。

## TypeScript LazyGameData 生成方案

TypeScript 的 loader 是 async-friendly，lazy accessor 必须显式 async，不能伪装成同步 property：

```ts
export class LazyGameData {
  private readonly loader: RuntimeDataLoader;
  private itemsPromise?: Promise<ItemsTable>;
  private gameRulesPromise?: Promise<GameRulesRow>;

  static loadLazy(loader: RuntimeDataLoader): LazyGameData;
  items(): Promise<ItemsTable>;
  gameRules(): Promise<GameRulesRow>;
  preloadAll(): Promise<void>;
  linkAll(): Promise<void>;
}
```

设计细节：

- 每个 table cache 保存 `Promise<T>` 而不是只保存 `T`，这样并发两次 `await data.items()` 会共享同一次 load。Promise resolve 后自然缓存 loaded table。
- 如果 Promise reject，默认应清空对应 cache，使下一次访问可以重试：

```ts
if (!this.itemsPromise) {
  this.itemsPromise = ItemsTable.load(this.loader).catch((error) => {
    this.itemsPromise = undefined;
    throw error;
  });
}
return this.itemsPromise;
```

- `GameData.load(loader)` 保持 eager，并继续返回字段属性同步可访问的 `GameData`。
- lazy facade 不生成 `readonly items: ItemsTable` 属性，避免业务误以为访问是同步零成本；使用 `await data.items()` / `await data.gameRules()` 或 `getItems()` 命名。
- Eager relation helper 当前是同步 `row.defaultClass(data): ClassesRow`。Lazy relation helper 需要 async 形态，推荐生成额外 helper，例如 `row.defaultClassAsync(data: LazyGameData): Promise<ClassesRow>`，或在 `LazyGameData` 上生成 `resolveDefaultClass(row): Promise<ClassesRow>`。不应把现有同步 helper 改成可能返回 Promise 的 union 类型。
- `preloadAll()` 使用 `Promise.all` 或确定性顺序均可；若需要稳定错误顺序，推荐按 CodeModel table 顺序逐个 await。`linkAll()` 应 `await preloadAll()` 后再调用 eager-style link 逻辑填充 relation cache。
