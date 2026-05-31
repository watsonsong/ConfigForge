# 09b. Codegen Rust Backend

本文从 Codegen 模块详细设计中拆出，专门描述 Rust backend 的生成内容、API 形态和 lazy loading 落地方式。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 总览、IR、命名、冲突检测和测试总览。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、runtime loading 和 generated API 语义。
- [`13-runtime-binary-format-detail-design.md`](13-runtime-binary-format-detail-design.md) — `bin` runtime data schema revision 与 positional reader contract。

## 当前状态

Rust backend 已实现 MVP：生成 enum、struct、row/object、repository、`GameData`、`LazyGameData`、index lookup、`linked_field` relation helper、`link_all` 与 `text` translation helper。

## Rust 生成内容

当前实现采用单文件输出：`generated.rs`。以下多文件布局仍是后续拆分方向：

```text
generated/rust/
  mod.rs
  types.rs          # enum / struct / row types
  loader.rs         # RuntimeDataLoader trait
  tables.rs         # table repository + index helpers
```

当前单文件模式：`generated.rs`。

核心形态：

```rust
pub trait RuntimeDataReader {
    // 通用 reader cursor；实现可以包装 JSON value、MessagePack value、二进制 reader 或引擎序列化对象。
    // enter_* 在当前 reader 内部移动 cursor，exit 回到上一层，避免每次字段/数组访问创建子 reader。
    fn enter_field(&self, name: &'static str) -> RuntimeDataResult<bool>;
    fn enter_index(&self, index: usize) -> RuntimeDataResult<()>;
    fn exit(&self) -> RuntimeDataResult<()>;
    fn len(&self) -> Result<usize, String>;
    fn as_i64(&self) -> Result<i64, String>;
    fn as_f64(&self) -> Result<f64, String>;
    fn as_bool(&self) -> Result<bool, String>;
    fn as_string(&self) -> Result<String, String>;
    fn enter_required_field(&self, name: &'static str) -> RuntimeDataResult<()>;
    fn enter_optional_field(&self, name: &'static str) -> RuntimeDataResult<bool>;
    fn exit_field(&self) -> RuntimeDataResult<()>;
    fn read_enum_index(&self, variants: &[&str]) -> RuntimeDataResult<usize>;
}

pub trait RuntimeDataLoader {
    type Reader: RuntimeDataReader;
    fn open(&self, path: &str) -> Result<Self::Reader, String>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ItemRow {
    pub id: i64,
    pub name: String,
    pub rarity: ItemRarity,
}

pub struct ItemsTable {
    rows: Vec<ItemRow>,
    by_id: std::collections::HashMap<i64, usize>,
    by_rarity: std::collections::HashMap<ItemRarity, Vec<usize>>,
}

impl ItemsTable {
    pub const DATA_FILE: &'static str = "items";

    pub fn load(loader: &impl RuntimeDataLoader) -> Result<Self, String>;
    fn read_row(reader: &impl RuntimeDataReader) -> Result<ItemRow, String>;
    pub fn rows(&self) -> &[ItemRow];
    pub fn get_by_id(&self, id: i64) -> Option<&ItemRow>;
    pub fn find_by_rarity(&self, rarity: ItemRarity) -> Vec<&ItemRow>;
}

pub struct GameData {
    pub items: ItemsTable,
}

impl GameData {
    pub fn load(loader: &impl RuntimeDataLoader) -> Result<Self, String>;
}
```

Rust 约束：

- `unique = true` 的索引生成 `get_by_* -> Option<&Row>`。
- 非 unique 索引生成 `find_by_* -> Vec<&Row>` 或迭代器。第一阶段用 `Vec<&Row>` 简单实现。
- 复合索引生成 key struct 或 tuple。公开 API 推荐展开参数：`get_by_region_id(region: String, id: i64)`。
- 若生成 serde 代码，需要生成 `serde` feature 说明或由项目配置决定是否带 derive。
- 生成代码只定义通用 `RuntimeDataLoader` / `RuntimeDataReader` trait，不强制生成 JSON 或 MessagePack 的具体 decoder；业务 runtime 可以用 `serde_json`、`rmp-serde` 或项目自研库实现 reader。`ItemsTable::load` 打开 `DATA_FILE` 后由生成代码读取 `rows` 数组、逐行调用 `read_row` 并构建索引。

Rust `linked_field` 生成规则：

- Row/Object struct 保留原始 FK 字段，例如 `pub default_class_id: i64`；同时为每个 `linked_field` 生成非序列化 cache 字段。当前实现单值/可空 FK 使用 `Cell<LinkCache>`，list FK 使用 `RefCell<LinkListCache>`。`Cell<LinkCache>` 是 row 内 inline 状态，不发生堆分配；list FK 仅在首次解析时为 row-index list 分配 `Vec<usize>`。如果 row derive serde，需要为 cache 提供 `#[serde(skip)]` 或构造时初始化，避免把 runtime cache 写入业务序列化结果。
- Getter 推荐使用 `&self` + `&GameData`，返回 `Result<&TargetRow, RuntimeDataError>` / `Result<Option<&TargetRow>, RuntimeDataError>`。内部通过 `Cell<LinkCache>` 支持首次访问写 cache；list FK 使用 `RefCell<LinkListCache>`，后续访问借用已缓存 row indexes，不 clone 缓存 vector。
- Table 生成 `pub fn link(&self, data: &GameData) -> Result<(), RuntimeDataError>`，遍历 `rows` 或 singleton object 调用每个 relation getter 以填充 cache。
- DataSet facade 生成 `pub fn link_all(&self) -> Result<(), RuntimeDataError>`，按 CodeModel table 顺序调用各 table 的 `link`。只要全部 cache 存 row index，FK 环不会导致 borrow cycle；getter 返回值生命周期仍绑定到 `&GameData`。
- 如果目标 row 已被缓存为 index，但后续通过 index 取 row 时越界，应返回 data/code mismatch 错误；正常生成代码不提供可变 rows API，因此越界只可能来自版本不一致或内部 bug。

Rust `text` 翻译 helper 生成规则：

- 对每个 `CodeTypeRef::Text` 字段，在 row/object `impl` 中生成 helper；普通字段仍保留 `pub <field>: String`。
- 默认 `targets.code.i18n.mode = "generic"` 时生成轻量 translator trait，或允许用户类型实现等价 lookup trait：

```rust
pub trait TextTranslator {
    fn translate(&self, msgctxt: Option<&str>, msgid: &str) -> Option<String>;
}

impl ItemRow {
    pub fn display_name_text(&self, translator: &impl TextTranslator) -> String {
        translator
            .translate(Some("items.display_name"), &self.display_name)
            .unwrap_or_else(|| self.display_name.clone())
    }
}
```

当 `targets.code.i18n.mode = "global_function"` 时，不生成 `TextTranslator` trait，helper 直接调用配置的全局函数，签名不再带 translator 参数：

```rust
// ConfigForge.toml:
// [targets.code.i18n]
// mode = "global_function"
// function = "crate::i18n::translate"

impl ItemRow {
    pub fn display_name_text(&self) -> String {
        crate::i18n::translate(Some("items.display_name"), &self.display_name).into()
    }
}
```

`returns = "string"` 表示全局函数总是返回最终文案；`returns = "optional"` 表示返回 `Option<String>`，helper 会 fallback 到 raw text。

- 如果未来 translator 需要返回错误，生成器可通过 option 切换为 `RuntimeDataResult<String>`；默认 helper 采用 fallback，不引入新的错误路径。
- helper 返回 owned `String`，避免翻译值生命周期绑定到 translator 内部缓存实现。若业务要求零拷贝，可后续增加 `Cow<'a, str>` option。
- table / field path / record key 等 source metadata 当前不进入 helper contract；后续如 translator 需要诊断/统计信息，可在兼容 option 下扩展 `TextSource`，但不参与默认 gettext lookup key。

## Rust LazyGameData 生成方案

新增类型建议命名为 `LazyGameData<L>`，与 eager `GameData` 并存：

```rust
pub struct LazyGameData<L: RuntimeDataLoader> {
    loader: L,
    items: std::cell::OnceCell<ItemsTable>,
    game_rules: std::cell::OnceCell<GameRulesRow>,
}

impl<L: RuntimeDataLoader> LazyGameData<L> {
    pub fn new(loader: L) -> Self;
    pub fn items(&self) -> RuntimeDataResult<&ItemsTable>;
    pub fn game_rules(&self) -> RuntimeDataResult<&GameRulesRow>;
    pub fn preload_all(&self) -> RuntimeDataResult<()>;
    pub fn link_all(&self) -> RuntimeDataResult<()>;
}
```

设计细节：

- `L` 可以是具体 loader、`&'a Loader` 或业务自定义 wrapper；为了支持借用 loader，可生成或文档要求为 `&T where T: RuntimeDataLoader` 提供 blanket impl。
- 单线程默认使用 `std::cell::OnceCell<T>`，避免每次访问都经过 `RefCell<Option<T>>` borrow。若需要多线程共享，后续 codegen option 可切到 `std::sync::OnceLock<T>`，但当前 relation cache 使用 `Cell` / `RefCell`，整体仍默认非 `Sync`。
- `OnceCell` 不直接缓存 error。accessor 伪代码：若 cell empty，则调用 `T::load(&self.loader)?`，成功后 `set`，再返回 `get().unwrap()`；失败直接返回，不改变 cell。
- 为了让 `linked_field` relation helper 同时支持 eager 与 lazy，目标态应抽出 generated `GameDataAccess` trait：

```rust
pub trait GameDataAccess {
    fn items(&self) -> RuntimeDataResult<&ItemsTable>;
    fn classes(&self) -> RuntimeDataResult<&ClassesTable>;
}
```

  eager `GameData` 的实现只是返回已加载字段；lazy `LazyGameData<L>` 的实现会按需加载。relation getter 后续可从 `fn default_class(&self, data: &GameData)` 演进为 `fn default_class<'a>(&self, data: &'a impl GameDataAccess) -> RuntimeDataResult<&'a ClassesRow>`，从而让 target table 未加载时自动触发加载。
- `GameData::load(loader)` 保持 eager，不持有 loader。若用户希望先 lazy 构造再全部加载并释放 loader，Rust 可以继续推荐直接调用 eager；也可后续提供 `LazyGameData::into_eager(self) -> RuntimeDataResult<GameData>`，该方法 preload 后移动出所有 table 并丢弃 loader。
