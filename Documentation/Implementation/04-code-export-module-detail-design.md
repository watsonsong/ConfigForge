# 04. Code Export 模块详细设计

本文档描述 ConfigForge 的代码导出任务设计。目标是在已加载、已链接并通过校验的 Meta 与 Data 基础上，生成 Rust、C++ 和 TypeScript 三种语言的运行时代码，覆盖枚举、结构体、数据表、索引辅助查找函数，并提供抽象的数据文件加载器接口，让接入方可以从本地文件、包内资源、网络或引擎资源系统中加载实际数据。

产品层语义仍以 `Documentation/Design/` 为准；本文档聚焦程序实现模块、生成代码形态、跨语言约束、运行时加载抽象和测试策略。

## 4.1 设计目标

第一阶段 Code Export 需要达成：

1. 基于 `LinkedMetaModel<'meta>` 生成完整类型代码，而不是只为每张表生成简单 stub。
2. 支持三种目标语言：Rust、C++、TypeScript。
3. 生成所有 Meta 中声明的：
   - enum；
   - named struct；
   - table row type；
   - table repository / collection type；
   - index key type 和辅助查找函数。
4. 生成一个统一的 DataSet / Database facade，便于业务侧一次加载后按表访问。
5. 支持 JSON 与 MessagePack 两种 runtime data format，并让生成代码能够按 target format 读取对应文件。
6. 支持抽象数据文件加载器和抽象 runtime data decoder，生成代码不绑定具体文件系统、引擎资源系统或反序列化库。
6. 保持生成代码确定性：同一份 Meta 生成稳定文件名、声明顺序和 API 名称。
7. 在生成前发现命名冲突、保留字、目标语言类型映射缺失、输出路径冲突等问题。

第一阶段不实现：

- 生成代码的插件化动态加载；
- JSON / MessagePack 之外的复杂二进制数据格式；
- 跨语言运行时库发布；
- 多 target 差异化字段过滤；
- struct/json cell 的完整解析；
- foreign key 自动跳转接口；
- 增量代码生成。

这些能力应在生成器 API 上预留扩展点，但不阻塞第一阶段落地。

## 4.2 与现有流程的关系

当前 exporter 已经完成基础流程：

```text
MetaModel::from_path
  -> DataLoadPlan::from_meta_model
  -> MetaModel::link
  -> DataLoader::load
  -> normalize_data_set
  -> validate_normalized_data_set
  -> write runtime data / simple Rust stub
```

Code Export 应替换当前 `exporter.rs` 中临时的 `write_table_rust_stub`，并把生成代码的职责从 `exporter` 编排层下沉到独立模块。

建议新流程：

```text
Meta Load / Link
Data Load / Normalize / Validate
  -> ExportInput
  -> CodeExportPlan::build
  -> CodeGeneratorRegistry dispatch
  -> ArtifactPlan
  -> ArtifactWriter
```

关键约束：

- Code Export 只依赖已 linked 的 Meta，不直接读取 TOML。
- Code Export 不重新读取 CSV，不直接依赖 `DataLoader`。
- Runtime data 文件和 generated code 文件都应进入同一套 Artifact Plan，统一检查路径冲突。
- Data 文件格式第一阶段支持 JSON 与 MessagePack；加载抽象约束生成代码如何读取 bytes，decoder 抽象约束如何把 bytes 解析为 typed rows，不强制读取来源或第三方 runtime 库。

## 4.3 模块结构建议

建议新增 `src/codegen/` 目录模块：

```text
src/codegen/
  mod.rs              # 对外 API 与 re-export
  model.rs            # CodeExportTarget / CodeExportInput / Artifact 等核心模型
  ir.rs               # Language-neutral CodeModel IR
  naming.rs           # 目标语言命名规范、保留字处理、冲突检测
  type_mapping.rs     # Meta type -> target language type 映射
  index.rs            # 索引 API 规划、Key 类型规划
  runtime.rs          # 抽象 loader / repository API 的语言无关描述
  rust.rs             # RustCodeGenerator
  cpp.rs              # CppCodeGenerator
  typescript.rs       # TypeScriptCodeGenerator
  tests.rs            # codegen 单元测试
```

`src/exporter.rs` 保留为执行编排层，负责：

- 加载 Meta 和 Data；
- 归一化与校验；
- 构造 `CodeExportInput`；
- 调用对应 `CodeGenerator`；
- 写出 Artifact。

后续可以再拆出 `src/artifact/`：

```text
src/artifact/
  mod.rs
  model.rs            # Artifact / ArtifactKind / ArtifactPath
  writer.rs           # direct / temp_commit 写入策略
  manifest.rs         # manifest 生成
```

第一阶段如果不想一次性扩大范围，可以先让 `CodeGenerator` 返回 `Vec<GeneratedFile>`，由 exporter 顺序写入；但类型命名应为未来 Artifact Writer 预留。

## 4.4 核心输入输出模型

### 4.4.1 `CodeExportLanguage`

```rust
pub enum CodeExportLanguage {
    Rust,
    Cpp,
    TypeScript,
}
```

### 4.4.2 `CodeExportTarget`

```rust
pub struct CodeExportTarget {
    pub language: CodeExportLanguage,
    pub module_name: String,
    pub namespace: Option<String>,
    pub out_dir: PathBuf,
    pub options: CodegenOptions,
}
```

字段说明：

| 字段 | 说明 |
| --- | --- |
| `language` | 目标语言。 |
| `module_name` | 生成模块名，例如 Rust module、TS namespace/package、C++ 文件前缀。 |
| `namespace` | C++ namespace / Rust module path / TS namespace 的可选覆盖。 |
| `out_dir` | 生成代码输出目录。 |
| `options` | 从 `targets.code` 解析出的通用和语言专属选项，包括文件头、命名策略、关键字冲突策略、语言扩展参数等。 |

`CodeExportTarget` 不应直接由 CLI 硬编码构造；正式流程应来自 Project Config 的 `[[targets.code]]`。runtime data 的具体格式由 `[targets.data].format` 选择，并通过 `RuntimeDataFile.path` 的真实后缀传递给 CodeModel；code-only target 没有 data artifact 时当前默认推导 `<table>.json`。

### 4.4.2.1 Runtime data format

解析规则：

- Project Config 中 `[targets.data].format = "json"` 选择 JSON exporter。
- Project Config 中 `[targets.data].format = "messagepack"` 选择 MessagePack exporter。
- 可兼容解析 `"msgpack"` 为 MessagePack，但错误信息、文档、生成代码和 manifest 中统一使用规范名 `messagepack`。

文件规划规则：

| Format | 文件后缀 | exporter 写出方式 | 生成代码读取方式 |
| --- | --- | --- | --- |
| `Json` | `.json` | `serde_json::to_vec_pretty` 或等价 typed JSON。 | loader 读取 bytes，decoder 按 JSON 解码。 |
| `MessagePack` | `.msgpack` | `rmp-serde` 写出同一 payload 的 MessagePack bytes。 | loader 读取 bytes，decoder 按 MessagePack 解码。 |

`RuntimeDataFile` 中保存的 `path` 必须是已带正确后缀的相对文件名，例如 `items.json` 或 `items.msgpack`。CodeModel 优先使用 runtime data export 阶段返回的真实路径；当 target 没有 data export artifact 时，才默认推导 `<table>.json`。

### 4.4.2.2 `CodegenOptions`

```rust
pub struct CodegenOptions {
    pub file_header: Option<String>,
    pub keyword_conflict: KeywordConflictPolicy,
    pub naming: NamingConfig,
    pub language: LanguageCodegenOptions,
}

pub enum LanguageCodegenOptions {
    Rust(RustCodegenOptions),
    Cpp(CppCodegenOptions),
    TypeScript(TypeScriptCodegenOptions),
}

pub struct RustCodegenOptions {
    pub derive_serde: bool,
}

pub struct CppCodegenOptions {
    pub namespace: Option<String>,
}

pub struct TypeScriptCodegenOptions {
    pub readonly_fields: bool,
}

pub enum KeywordConflictPolicy {
    SuffixUnderscore,
    RawIdentifier,
    Error,
}

pub struct NamingConfig {
    pub type_name: CaseStyle,
    pub enum_value: CaseStyle,
    pub field: CaseStyle,
    pub function: CaseStyle,
    pub constant: CaseStyle,
}

pub enum CaseStyle {
    Preserve,
    PascalCase,
    CamelCase,
    SnakeCase,
    ScreamingSnakeCase,
}
```

配置来源：

```toml
[[targets.code]]
language = "rust"
output = "Generated/Rust"
module_name = "game_config"
file_header = "// {{project.name}}\n// Generated at {{datetime}}"
keyword_conflict = "suffix_underscore"

[targets.code.naming]
type = "pascal_case"
enum_value = "pascal_case"
field = "snake_case"
function = "snake_case"
constant = "screaming_snake_case"

[targets.code.rust]
derive_serde = true
```

设计要求：

- `CodegenOptions` 拆成通用配置和语言特定配置两层：`file_header`、`keyword_conflict`、`naming` 为通用层，`language` 为语言配置的 tagged enum。
- 每个语言生成器只能读取自己的扩展配置，例如 `RustCodegenOptions`、`CppCodegenOptions`、`TypeScriptCodegenOptions`。
- 新增语言时应新增 generator 和语言扩展解析逻辑，不修改 IR。
- 如果配置中出现当前语言不支持的 `CaseStyle` 或 `KeywordConflictPolicy`，应在 codegen validate 阶段报错。

### 4.4.2.3 文件头模板

`file_header` 是可选模板。模板渲染上下文来自 Project Config 和执行上下文：

| 变量 | 来源 |
| --- | --- |
| `{{project.name}}` | `ProjectConfig.project.name`。 |
| `{{project.desc}}` | `ProjectConfig.project.desc`，未配置为空。 |
| `{{project.copyright}}` | `ProjectConfig.project.copyright`，未配置为空。 |
| `{{datetime}}` | 当前生成时间，由 exporter 注入。 |

文件头渲染应发生在语言生成器写入每个 `GeneratedFile` 内容前。语言生成器可以决定是否为每行追加注释前缀，但模板本身应允许用户完整控制文本。

### 4.4.3 `CodeExportInput`

```rust
pub struct CodeExportInput<'meta> {
    pub meta: &'meta LinkedMetaModel<'meta>,
    pub target_tags: TargetTags,
    pub project: CodeExportProject,
    pub target: CodeExportTarget,
    pub runtime_data_files: Vec<RuntimeDataFile>,
    pub generated_at: String,
}
```

`project` 来自 Project Config，用于渲染文件头模板；`target_tags` 来自当前 target 的 `tags`，用于构造目标相关的 CodeModel；`runtime_data_files` 来自数据导出阶段的 Artifact 规划，用于生成加载函数中的默认资源名。例如 JSON target 中 `items.json` 对应 `ItemsTable::DATA_FILE`，MessagePack target 中 `items.msgpack` 对应同一个常量。`generated_at` 由 exporter 注入，用于渲染文件头中的 `{{datetime}}`。

### 4.4.4 `GeneratedFile`

```rust
pub struct GeneratedFile {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub content: String,
}
```

第一阶段只生成文本代码文件。后续如果生成二进制 schema 或 manifest，可扩展为 `GeneratedContent::Text | Bytes`。

## 4.5 语言无关 CodeModel IR

为了避免 Rust/C++/TypeScript 三套生成器重复理解 Meta，建议先把 `LinkedMetaModel<'meta>` 转换为语言无关 IR：

```rust
pub struct CodeModel {
    pub enums: Vec<CodeEnum>,
    pub structs: Vec<CodeStruct>,
    pub tables: Vec<CodeTable>,
}

pub struct CodeEnum {
    pub name: String,
    pub desc: Option<String>,
    pub values: Vec<CodeEnumValue>,
}

pub struct CodeStruct {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
}

pub struct CodeTable {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub indexes: Vec<CodeIndex>,
    pub primary_index: Option<String>,
    pub data_file: String,
}
```

IR 职责：

1. 保留 Meta 中的语义名称和描述信息。
2. 根据 `target_tags` 过滤不应导出的表、字段和索引，例如 `export = false`、`export_when = { any = ["client"] }`。
3. 保留 table、field、index、enum、struct 的语言无关关系。
4. 保留 runtime data file 信息。
5. 把 `LinkedField` 转换为目标语言无关的 `CodeTypeRef`。

IR 不负责：

- 目标语言标识符生成；
- table row type 名称；
- repository type 名称；
- index key type 名称；
- lookup function 名称；
- 关键字冲突处理。

这些都属于语言生成器和 `CodegenOptions.naming` 的职责。这样新增语言时不需要修改 `ir.rs`。

```rust
pub enum CodeTypeRef {
    Int,
    Float,
    Bool,
    String,
    Text,
    Asset,
    Enum { name: String },
    Struct { name: String },
}
```

目标语言生成器只负责：

- 根据 `CodegenOptions.naming` 把 Meta name 转换成目标语言标识符；
- 根据 `KeywordConflictPolicy` 处理关键字冲突；
- 把 `CodeTypeRef` 映射为语言类型；
- 渲染语法；
- 渲染 loader / repository / index API；
- 处理 import/include/use。

## 4.6 命名策略与冲突检测

代码生成必须先解决命名合法性问题，不允许直接使用 Meta 名称。命名策略由 `targets.code.naming` 配置，并由每个语言生成器解释。

### 4.6.1 通用规则

1. Meta 名称可保持较宽松；代码名称必须符合目标语言标识符规则。
2. 非 ASCII 字符第一阶段不直接生成标识符，应转义或报错。推荐先报错，后续再支持 transliteration。
3. 非字母数字下划线字符按单词分隔处理。
4. 数字开头的名称加前缀，例如 `field_123` / `Table123`。
5. 保留字按 `keyword_conflict` 处理：追加 `_`、raw identifier 或报错。
6. 同一作用域内 code name 冲突必须报错，而不是静默覆盖。

### 4.6.1.1 命名配置解析

生成器应把通用 `NamingConfig` 映射为自己的命名函数。例如 Rust 默认：

| 符号 | 默认策略 |
| --- | --- |
| type | `pascal_case` |
| enum_value | `pascal_case` |
| field | `snake_case` |
| function | `snake_case` |
| constant | `screaming_snake_case` |

TypeScript 默认：

| 符号 | 默认策略 |
| --- | --- |
| type | `pascal_case` |
| enum_value | `preserve`，用于字符串 union literal |
| field | `camel_case` |
| function | `camel_case` |
| constant | `camel_case` |

C++ 默认：

| 符号 | 默认策略 |
| --- | --- |
| type | `pascal_case` |
| enum_value | `pascal_case` |
| field | `snake_case` |
| function | `pascal_case` |
| constant | `pascal_case` |

命名转换应是 generator-local 的能力；`CodeModel` 只提供原始语义名称。

### 4.6.2 建议命名映射

| Meta 对象 | Rust | C++ | TypeScript |
| --- | --- | --- | --- |
| enum type | `PascalCase` | `PascalCase` | `PascalCase` |
| enum value | `PascalCase` | `PascalCase` | `PascalCase` |
| struct type | `PascalCase` | `PascalCase` | `PascalCase` |
| table row type | `PascalCase` + `Row` | `PascalCase` + `Row` | `PascalCase` + `Row` |
| repository type | `PascalCase` + `Table` | `PascalCase` + `Table` | `PascalCase` + `Table` |
| field | `snake_case` | `snake_case` | `camelCase` |
| index key type | `PascalCase` + index + `Key` | 同 Rust | 同 Rust |
| lookup function | `find_by_xxx` / `get_by_xxx` | `FindByXxx` / `GetByXxx` | `findByXxx` / `getByXxx` |

### 4.6.3 冲突场景

必须检查：

- enum / struct / table row 类型重名；
- 同一 struct/table 内字段生成名重名；
- enum value 生成名重名；
- table repository 内 index function 重名；
- 生成文件路径重名；
- 生成名称与 runtime helper 类型重名，例如 `DataLoader`、`GameData`。

## 4.7 类型映射

### 4.7.1 Primitive 映射

| Meta type | Rust | C++ | TypeScript |
| --- | --- | --- | --- |
| `int` / `i64` | `i64` | `std::int64_t` | `number` |
| `float` / `f64` | `f64` | `double` | `number` |
| `bool` | `bool` | `bool` | `boolean` |
| `string` | `String` | `std::string` | `string` |
| `text` | `String` | `std::string` | `string` |
| `asset` | `String` 或 `AssetRef` | `std::string` 或 `AssetRef` | `string` 或 `AssetRef` |

第一阶段建议 `asset` 先映射为字符串，后续通过 target type mapping 支持自定义 `AssetRef`。

### 4.7.2 Enum 映射

- Rust：`enum ItemRarity { Common, Rare }`，建议 derive `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`。
- C++：`enum class ItemRarity { Common, Rare };`，生成 `FromString` / `ToString`。
- TypeScript：推荐生成字符串 union 或 enum。第一阶段建议字符串 union，更贴合 JSON：

```ts
export type ItemRarity = "common" | "rare";
```

如果使用 TS `enum`，需要考虑运行时代码体积和 JSON 字符串转换。

### 4.7.3 Struct 映射

- Rust：`struct Reward { ... }`。
- C++：`struct Reward { ... };`。
- TypeScript：`export interface Reward { ... }`。

当前 Data validation 对 struct value 仍是 reserved；Code Export 可以先生成类型声明，但运行时 JSON parse 支持应和 Data 阶段能力对齐。第一阶段建议：

- 如果 Meta 中存在 struct 类型字段，生成 struct 类型声明；
- table row 中的 struct 字段 parse / load 标记为 unsupported 或要求 runtime data 已经是对象结构；
- 在导出前由 Validation 阶段继续阻断 struct 数据值，避免生成代码承诺已可运行。

## 4.8 生成内容

### 4.8.1 Rust 生成内容

建议文件：

```text
generated/rust/
  mod.rs
  types.rs          # enum / struct / row types
  loader.rs         # DataFileLoader trait / RuntimeDataDecoder trait
  tables.rs         # table repository + index helpers
```

可选单文件模式：`generated.rs`。

核心形态：

```rust
pub enum RuntimeDataFormat {
    Json,
    MessagePack,
}

pub trait DataFileLoader {
    fn load_bytes(&self, path: &str) -> Result<Vec<u8>, LoadError>;
}

pub trait RuntimeDataDecoder {
    fn decode_items_rows(
        &self,
        format: RuntimeDataFormat,
        path: &str,
        bytes: &[u8],
    ) -> Result<Vec<ItemRow>, LoadError>;
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
    pub const DATA_FILE: &'static str = "items.msgpack";
    pub const DATA_FORMAT: RuntimeDataFormat = RuntimeDataFormat::MessagePack;

    pub fn load(
        loader: &impl DataFileLoader,
        decoder: &impl RuntimeDataDecoder,
    ) -> Result<Self, LoadError>;
    pub fn rows(&self) -> &[ItemRow];
    pub fn get_by_id(&self, id: i64) -> Option<&ItemRow>;
    pub fn find_by_rarity(&self, rarity: ItemRarity) -> Vec<&ItemRow>;
}

pub struct GameData {
    pub items: ItemsTable,
}

impl GameData {
    pub fn load(
        loader: &impl DataFileLoader,
        decoder: &impl RuntimeDataDecoder,
    ) -> Result<Self, LoadError>;
}
```

Rust 约束：

- `unique = true` 的索引生成 `get_by_* -> Option<&Row>`。
- 非 unique 索引生成 `find_by_* -> Vec<&Row>` 或迭代器。第一阶段用 `Vec<&Row>` 简单实现。
- 复合索引生成 key struct 或 tuple。公开 API 推荐展开参数：`get_by_region_id(region: String, id: i64)`。
- 若生成 serde 代码，需要生成 `serde` feature 说明或由项目配置决定是否带 derive。
- 生成代码只定义 loader / decoder trait，不强制生成 JSON 或 MessagePack 的具体 decoder；业务 runtime 可以用 `serde_json`、`rmp-serde` 或项目自研库实现。

### 4.8.2 C++ 生成内容

建议文件：

```text
generated/cpp/
  config_forge_types.hpp
  config_forge_loader.hpp
  config_forge_tables.hpp
  config_forge_tables.cpp
```

核心形态：

```cpp
class IDataFileLoader {
public:
    virtual ~IDataFileLoader() = default;
    virtual std::vector<std::uint8_t> LoadBytes(const std::string& path) = 0;
};

enum class RuntimeDataFormat {
    Json,
    MessagePack,
};

class IRuntimeDataDecoder {
public:
    virtual ~IRuntimeDataDecoder() = default;
    virtual std::vector<ItemRow> DecodeItemsRows(
        RuntimeDataFormat format,
        const std::string& path,
        const std::vector<std::uint8_t>& bytes) = 0;
};

struct ItemRow {
    std::int64_t id;
    std::string name;
    ItemRarity rarity;
};

class ItemsTable {
public:
    static constexpr const char* DataFile = "items.msgpack";
    static constexpr RuntimeDataFormat DataFormat = RuntimeDataFormat::MessagePack;

    static ItemsTable Load(IDataFileLoader& loader, IRuntimeDataDecoder& decoder);
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
    static GameData Load(IDataFileLoader& loader, IRuntimeDataDecoder& decoder);
    const ItemsTable& Items() const;
private:
    ItemsTable items_;
};
```

C++ 约束：

- 第一阶段目标 C++17。
- 运行时 JSON / MessagePack 解析库不强绑定。生成代码只定义 `IRuntimeDataDecoder`，由业务实现；可在后续提供基于 `nlohmann::json`、`msgpack-c` 或引擎序列化系统的可选 adapter。
- 复合 key 需要生成 `struct Key`、`operator==` 和 hash。
- enum class 需要生成 hash 或转换为底层整数用于 unordered_map。

### 4.8.3 TypeScript 生成内容

建议文件：

```text
generated/ts/
  types.ts
  loader.ts
  tables.ts
  index.ts
```

核心形态：

```ts
export interface DataFileLoader {
  loadBytes(path: string): Promise<Uint8Array | ArrayBuffer | string> | Uint8Array | ArrayBuffer | string;
}

export type RuntimeDataFormat = "json" | "messagepack";

export interface RuntimeDataDecoder {
  decodeItemsRows(format: RuntimeDataFormat, path: string, data: Uint8Array | ArrayBuffer | string): ItemRow[];
}

export type ItemRarity = "common" | "rare";

export interface ItemRow {
  id: number;
  name: string;
  rarity: ItemRarity;
}

export class ItemsTable {
  static readonly dataFile = "items.msgpack";
  static readonly dataFormat: RuntimeDataFormat = "messagepack";

  private readonly rows_: ItemRow[];
  private readonly byId: Map<number, ItemRow>;
  private readonly byRarity: Map<ItemRarity, ItemRow[]>;

  static async load(loader: DataFileLoader, decoder: RuntimeDataDecoder): Promise<ItemsTable>;
  rows(): readonly ItemRow[];
  getById(id: number): ItemRow | undefined;
  findByRarity(rarity: ItemRarity): readonly ItemRow[];
}

export class GameData {
  readonly items: ItemsTable;
  static async load(loader: DataFileLoader, decoder: RuntimeDataDecoder): Promise<GameData>;
}
```

TypeScript 约束：

- 第一阶段生成 ESM。
- `DataFileLoader.loadBytes` 同时允许同步值和 `Promise`，生成代码内部统一 `await`。
- JSON / MessagePack 解码通过 `RuntimeDataDecoder` 注入；可提供可选 `JsonRuntimeDataDecoder` 示例，但生成核心代码不绑定 MessagePack npm 包。
- 对 runtime data shape 做轻量检查：表文件必须能解析为数组或 `{ rows: [...] }`。是否对每个字段做严格 runtime type guard 可通过选项控制。

## 4.9 索引辅助查找函数设计

### 4.9.1 索引来源

索引来自 `TableMeta.indexes`，Link 后为 `LinkedTableMeta.indexes`。每个 index 包含：

- `name`；
- `fields`；
- `unique`；
- `primary`。

`FieldMeta.unique = true` 可视为单字段 unique index，建议在 CodeModel 中统一转成 synthetic index，命名为 `by_<field>`。

### 4.9.2 API 规则

| 索引类型 | Rust | C++ | TypeScript |
| --- | --- | --- | --- |
| unique single | `get_by_id(id) -> Option<&Row>` | `GetById(id) -> const Row*` | `getById(id): Row | undefined` |
| non-unique single | `find_by_type(type) -> Vec<&Row>` | `FindByType(type) -> vector<const Row*>` | `findByType(type): readonly Row[]` |
| unique compound | `get_by_a_b(a, b) -> Option<&Row>` | `GetByAB(a, b) -> const Row*` | `getByAB(a, b): Row | undefined` |
| non-unique compound | `find_by_a_b(a, b) -> Vec<&Row>` | `FindByAB(a, b) -> vector<const Row*>` | `findByAB(a, b): readonly Row[]` |

### 4.9.3 索引构建规则

1. Repository 构造时一次性遍历 rows 构建所有索引。
2. unique index 如果发现重复，加载阶段返回错误。理论上导出前 validation 已经检查，但 generated runtime 仍应防御。
3. non-unique index 保存 row index 列表，保持原始数据顺序。
4. bool/int/float/string/enum 可作为 key。
5. float 作为索引 key 第一阶段允许但不推荐；可在 Meta validate 或 Codegen validate 中给 warning。
6. struct/text/asset 是否允许作为 key 需要策略。第一阶段建议：
   - `text` / `asset` 作为字符串 key 允许；
   - struct key 不支持。

### 4.9.4 Primary index

如果存在 primary index，额外生成主访问函数：

| Language | API |
| --- | --- |
| Rust | `get(primary_key)` 或 `get_by_primary(...)` |
| C++ | `Get(...)` 或 `GetByPrimary(...)` |
| TypeScript | `get(...)` 或 `getByPrimary(...)` |

第一阶段建议生成 `get_by_<primary_index_name>`，避免主索引字段变化导致 `get(...)` 语义不清。后续可配置是否生成短别名 `get(...)`。

## 4.10 抽象加载器设计

### 4.10.1 为什么需要抽象加载器

生成代码不应该假设数据来自本地文件系统。不同接入环境可能是：

- Rust 服务端：从磁盘或对象存储读取 JSON；
- C++ 游戏客户端：从包内资源、虚拟文件系统或引擎 AssetManager 读取；
- TypeScript：从 HTTP、Node fs、浏览器 bundle 或 IndexedDB 读取。

因此生成代码依赖两个抽象：loader 给定逻辑 data file path 返回 bytes；decoder 根据 target format 把 bytes 转成 typed rows。

### 4.10.2 语言无关接口

```text
DataFileLoader
  load_bytes(path) -> bytes

RuntimeDataDecoder
  decode_<table>_rows(format, path, bytes) -> Vec<Row>
```

第一阶段 JSON 和 MessagePack 都走 `load_bytes`，避免 JSON 使用 text、MessagePack 使用 bytes 导致生成 API 分裂。JSON decoder 可以自行把 bytes 按 UTF-8 解释为文本。

### 4.10.3 数据格式适配

加载器只负责读取原始内容，不负责解析业务对象。解析由 runtime adapter / decoder 完成：

```text
loader.load_bytes("items.msgpack")
  -> bytes
  -> runtime decoder by RuntimeDataFormat::MessagePack
  -> Vec<ItemRow>
  -> ItemsTable::from_rows
  -> indexes
```

这样可以把“从哪里加载”和“如何解释数据”分开。

### 4.10.4 默认 loader

可以为每种语言生成一个可选的默认本地文件 loader：

| Language | 默认实现 |
| --- | --- |
| Rust | `FsDataFileLoader { root: PathBuf }` |
| C++ | `FileSystemDataFileLoader { root }`，可选生成 |
| TypeScript | `FetchDataFileLoader` / `NodeFsDataFileLoader`，二选一或都不默认生成 |

第一阶段建议：

- Rust 生成 `FsDataFileLoader`，便于测试。
- C++ 只生成接口，不绑定文件 API。
- TypeScript 只生成接口和一个简单 `fetch` loader 示例，避免 Node/browser 环境分歧。

默认 decoder 策略：

- 核心生成代码只生成 decoder interface，不强制 runtime 依赖。
- Rust 可选生成 `SerdeRuntimeDataDecoder`，需要调用方启用/提供 `serde_json` 与 `rmp-serde`。
- C++ 可选生成 adapter 示例，不默认依赖 `nlohmann::json` 或 `msgpack-c`。
- TypeScript 可选生成 JSON decoder；MessagePack decoder 由 runtime 注入，以避免绑定具体 npm 包。

## 4.11 Runtime data 文件约定

当前 exporter 输出形态是包含 `meta` 和 `rows` 的 payload。JSON 与 MessagePack 使用同一逻辑结构，只是编码方式不同。生成代码 / decoder 读取时建议支持两种 shape：

```json
{
  "meta": { "name": "items" },
  "rows": [
    { "id": "1", "name": "Sword" }
  ]
}
```

以及未来更轻量的：

```json
[
  { "id": 1, "name": "Sword" }
]
```

第一阶段推荐导出 runtime data 时同步调整为 typed payload：

- int/float/bool 输出 number/bool；
- string/text/asset/enum 输出 string；
- struct 后续输出 object。

如果短期仍输出字符串 cell，decoder 必须在加载时做类型转换。这会导致三种语言重复实现 parse 逻辑，不推荐长期保留。

建议分两步：

1. Codegen 第一阶段先兼容当前 `{ meta, rows }`，并由 decoder 把字段从 runtime value parse 为目标类型。
2. Data typed export 落地后，decoder 可以直接读取 typed JSON / MessagePack。

## 4.11.1 Exporter 写出实现方案

`TargetDataFormat` 与 codegen 层 `RuntimeDataFormat` 保持一一对应：

```rust
pub enum TargetDataFormat {
    Json,
    MessagePack,
}
```

写出流程：

1. `build_target` 根据 `[targets.data].format` 选择 runtime data format，默认 `Json`。
2. `write_runtime_data` 遍历 linked tables，为每张表构造统一 `ExportedTable { meta, rows }` payload。
3. `Json` 使用 `serde_json::to_vec_pretty` 写出 `<table>.json`。
4. `MessagePack` 使用 `rmp_serde::to_vec_named` 或等价 named-field 编码写出 `<table>.msgpack`。
5. 返回给 codegen 的 `RuntimeDataFile { table_name, path }` 必须使用真实写出的文件名。
6. `write_generated_code` 把 runtime data export 返回的 `RuntimeDataFile` 传入 `CodeExportInput`，让 CodeModel 使用真实文件名。

依赖建议：

```toml
rmp-serde = "1"
```

如果后续引入 manifest，manifest 中也应记录每个 runtime data artifact 的 `format`、`path`、`table_name`。

## 4.11.2 Codegen 读取实现方案

CodeModel 构建时需要使用 `RuntimeDataFile`：

- 若 `runtime_data_files` 中存在 table 对应 artifact，使用 artifact path。
- 若不存在，则默认推导 `<table>.json`。
- 每个 `CodeTable` 保留 `data_file`，语言生成器用它生成常量。

生成代码的 load 流程统一为：

```text
Table::load(loader, decoder)
  -> bytes = loader.load_bytes(DATA_FILE)
  -> rows = decoder.decode_<table>_rows(DATA_FORMAT, DATA_FILE, bytes)
  -> Table::from_rows(rows)
```

这样 JSON 与 MessagePack 的差异完全落在 decoder 实现里，repository、index、GameData facade 不需要关心具体解析库。

## 4.12 生成器接口

```rust
pub trait CodeGenerator {
    fn language(&self) -> CodeExportLanguage;
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>>;
}
```

内置生成器：

```rust
pub struct RustCodeGenerator;
pub struct CppCodeGenerator;
pub struct TypeScriptCodeGenerator;
```

生成过程：

```text
CodeExportInput
  -> build CodeModel
  -> validate CodeModel
  -> render language files
  -> validate file paths
  -> Vec<GeneratedFile>
```

`validate CodeModel` 应检查：

- 目标语言命名合法；
- 命名冲突；
- unsupported field type；
- unsupported index key type；
- unsupported runtime data format；
- 输出路径冲突；
- 目标语言需要的 runtime dependency 是否已声明。

## 4.15 测试计划

数据格式与生成代码加载接口相关测试应覆盖：

1. `TargetDataFormat` 解析：`json`、`messagepack`、可选别名 `msgpack`，以及未知格式报错。
2. Runtime data 写出：
   - JSON target 生成 `<table>.json`，内容可用 `serde_json` 读取；
   - MessagePack target 生成 `<table>.msgpack`，内容可用 `rmp-serde` 读取为同一 payload；
   - `RuntimeDataFile.path` 与实际文件后缀一致。
3. Codegen 输入传递：`CodeExportInput.runtime_data_files` 使用 data export 阶段返回的真实路径。
4. CodeModel 默认文件名：没有 runtime data artifact 时，默认推导 `.json`。
5. Rust 生成代码包含：
   - `RuntimeDataLoader`；
   - 每张表的 `DATA_FILE`；
   - row struct、table repository 和 index helper。
6. C++ 生成代码包含：
   - `IRuntimeDataLoader`；
   - 每张表的 `DataFile`；
   - row struct、table repository 和 index helper。
7. TypeScript 生成代码包含：
   - `RuntimeDataLoader`；
   - 每张表的 `dataFile`；
   - row interface、table repository 和 index helper。
8. CLI integration：分别构建 JSON target 与 MessagePack target，验证 runtime data 文件和 generated code 文件同时生成，并检查代码中引用的默认文件名与格式一致。

## 4.13 文件组织策略

### 4.13.1 多文件模式，推荐默认

优点：便于阅读和差异比较。

```text
types.*
loader.*
tables.*
index.*
```

### 4.13.2 单文件模式

适合小项目或直接拷贝接入：

```text
config_forge_generated.rs
config_forge_generated.hpp
config_forge_generated.ts
```

第一阶段可以只实现单文件模式以降低复杂度，但 IR 和 GeneratedFile 应支持多文件。

## 4.14 错误与诊断

第一阶段可以继续使用 `ConfigForgeError::Validation(String)`，但建议新增 codegen diagnostic 模型：

```rust
pub struct CodegenDiagnostic {
    pub code: CodegenDiagnosticCode,
    pub message: String,
    pub language: Option<CodeExportLanguage>,
    pub meta_object: Option<String>,
}
```

建议错误码：

| Code | 场景 |
| --- | --- |
| `InvalidIdentifier` | 生成名称不合法且无法自动修正。 |
| `ReservedKeyword` | 命中目标语言保留字且策略不允许自动后缀。 |
| `NameConflict` | 同作用域生成名冲突。 |
| `UnsupportedFieldType` | 目标语言不支持字段类型。 |
| `UnsupportedIndexKey` | 索引字段类型不能作为 key。 |
| `OutputPathConflict` | 多个 artifact 指向同一路径。 |
| `RuntimeDependencyMissing` | 生成代码要求的依赖未配置。 |

## 4.15 测试计划

### 4.15.1 CodeModel 单元测试

1. enum / struct / table 都能进入 `CodeModel`。
2. primitive / enum / struct 字段映射为正确 `CodeTypeRef`。
3. table / field / index 的 `export = false` 与 `export_when` 按 `target_tags` 过滤；primary index 引用当前 target 不导出的字段时报错，非 primary index 引用不导出字段时跳过。
4. primary index、unique index、non-unique index 都能生成 `CodeIndex`。
5. 命名 sanitize 后稳定且可预测。
6. 命名冲突能报错。

### 4.15.2 语言生成器 snapshot 测试

对同一个小 Meta fixture 生成三种语言代码，断言关键片段：

- enum 声明；
- struct/interface 声明；
- row type；
- loader interface；
- repository type；
- unique / non-unique lookup function；
- GameData facade。

推荐先使用字符串 contains 测试，后续引入 snapshot 工具。

### 4.15.3 生成代码编译测试

第一阶段建议至少覆盖 Rust 生成代码编译：

1. 生成 Rust 代码到 temp dir。
2. 创建临时 Cargo project 或 test module。
3. `cargo check` 验证生成代码语法正确。

C++ / TypeScript 可后续增加：

- C++：`clang++ -std=c++17 -fsyntax-only`。
- TypeScript：`tsc --noEmit`。

### 4.15.4 集成测试

在 `tests/fixtures/standard_project/` 上运行：

1. export 生成 runtime data。
2. export 生成 Rust/C++/TS code。
3. 检查输出文件存在。
4. 检查关键 API 名称存在。
5. Rust 生成代码可选执行 compile check。

## 4.16 第一阶段落地顺序

推荐分阶段实现：

### Phase 1：CodeModel 与 Rust 生成器

1. 新增 `src/codegen/`。
2. 实现 CodeModel IR。
3. 实现 naming / type mapping。
4. 替换现有 `write_table_rust_stub`。
5. 生成 Rust enum / struct / row / table / index helper / loader trait。
6. 增加单元测试和标准 fixture 回归。

### Phase 2：TypeScript 生成器

1. 生成 `types.ts` / `tables.ts` / `loader.ts`。
2. 支持 async loader。
3. 支持 Map-based indexes。
4. 增加 snapshot/contains 测试。

### Phase 3：C++ 生成器

1. 生成 header-only 版本，降低 `.cpp` 链接复杂度。
2. 生成 loader interface。
3. 生成 enum conversion 和 index hash。
4. 后续再拆 `.hpp` / `.cpp`。

### Phase 4：Artifact Writer 和 manifest

1. 把 runtime data 和 generated code 统一为 Artifact。
2. 做路径冲突检查。
3. 支持 direct / temp_commit。
4. 输出 manifest。

## 4.17 第一阶段完成标准

完成时应满足：

- Rust、C++、TypeScript 三种语言至少能生成类型和表访问代码。
- 所有 enum、struct、table row type 都有生成结果。
- 所有 table index 都有对应辅助查找函数。
- 每种语言都有抽象 loader 接口。
- 生成代码不直接依赖 ConfigForge CLI 内部类型。
- 命名冲突和非法标识符不会静默生成错误代码。
- `export_project` 不再使用临时 `write_table_rust_stub`。
- `cargo fmt`、`cargo clippy --all-targets --all-features`、`cargo test` 通过。
- 标准工程 fixture 覆盖 runtime data + code export 输出。
