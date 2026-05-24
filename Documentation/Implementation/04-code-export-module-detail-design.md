# 04. Code Export 模块详细设计

本文档描述 ConfigForge 的代码导出任务设计。目标是在已加载、已链接并通过校验的 Meta 与 Data 基础上，生成 Rust、C++ 和 TypeScript 三种语言的运行时代码，覆盖枚举、结构体、数据表、索引辅助查找函数，并提供抽象的数据文件加载器接口，让接入方可以从本地文件、包内资源、网络或引擎资源系统中加载实际数据。加载器接口必须与具体 Meta、表和字段解耦；所有按表/字段解析和 typed row/object 构造职责属于生成代码。

产品层语义仍以 `Documentation/Design/` 为准；本文档聚焦程序实现模块、生成代码形态、跨语言约束、运行时加载抽象和测试策略。

## 4.1 设计目标

当前已落地第一阶段 Code Export 需要达成：

1. 基于 `LinkedMetaModel<'meta>` 生成完整类型代码，而不是只为每张表生成简单 stub。
2. 支持三种目标语言：Rust、C++、TypeScript。
3. 生成所有 Meta 中声明的：
   - enum；
   - named struct；
   - table row type；
   - table repository / collection type；
   - index key type 和辅助查找函数。
4. 生成一个统一的 DataSet / Database facade，便于业务侧一次加载后按表访问。
5. 支持 JSON 与 MessagePack 两种 runtime data format；生成代码通过 runtime data export 阶段返回的真实文件名暴露每张表的默认数据文件路径。
6. 生成抽象 typed `RuntimeDataLoader`，完成当前可运行闭环；该 typed loader 是过渡契约。
7. 保持生成代码确定性：同一份 Meta 生成稳定文件名、声明顺序和 API 名称。
8. 在生成前发现命名冲突、保留字、目标语言类型映射缺失、输出路径冲突等问题。

下一阶段 loader contract 迁移目标：生成抽象 `RuntimeDataLoader` / `RuntimeDataReader`，由接入方负责打开资源并提供通用 object / array / scalar 读取能力；生成代码负责读取 payload envelope `{ meta, rows }`、按 Meta 字段映射到 typed rows / typed objects。目标态 loader 不能按表生成方法，也不能返回具体 `TableRow` 类型。迁移完成前，测试和接入说明需要明确区分当前 typed loader 过渡契约与目标态 reader loader 契约。

第一阶段不实现：

- 生成代码的插件化动态加载；
- JSON / MessagePack 之外的复杂二进制数据格式；
- 跨语言运行时库发布；
- struct/json cell 的完整解析；
- foreign key 自动跳转接口；
- 增量代码生成。

这些能力应在生成器 API 上预留扩展点，但不阻塞第一阶段落地。

## 4.2 与现有流程的关系

当前 exporter 已经完成的基础流程：

```text
ProjectConfig::from_path
  -> MetaModel::link
  -> DataLoadPlan::from_tables(target-visible tables)
  -> DataLoader::load
  -> normalize_data_set
  -> validate_normalized_data_set_for_target
  -> export runtime data (JSON / MessagePack)
  -> CodeGeneratorRegistry dispatch (Rust / C++ / TypeScript)
  -> write generated files
```

Code Export 已从 `exporter` 编排层下沉到独立 `src/codegen/` 模块；`exporter.rs` 负责构造输入、分发生成器、检查输出路径冲突并写出 artifact。

目标态流程仍可继续演进为：

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
- Data 文件格式第一阶段支持 JSON 与 MessagePack；目标态生成代码只约束通用 `RuntimeDataLoader` / reader 协议。loader 实现负责打开具体文件并暴露格式无关的读取 cursor，生成代码本身不绑定文件系统、引擎资源系统、JSON 库或 MessagePack 库，但负责所有 table / field 级解析逻辑。

## 4.3 模块结构建议

当前 `src/codegen/` 目录模块：

```text
src/codegen/
  mod.rs              # 对外 API、re-export、CodeExportTarget / CodeExportInput / Artifact 等核心模型
  ir.rs               # Language-neutral CodeModel IR
  naming.rs           # 目标语言命名规范、保留字处理、冲突检测
  rust.rs             # RustCodeGenerator
  cpp.rs              # CppCodeGenerator
  typescript.rs       # TypeScriptCodeGenerator
  tests.rs            # codegen 单元测试
```

`type_mapping.rs`、`index.rs`、`runtime.rs` 当前未拆分为独立文件；对应逻辑仍分别位于 IR 与各语言生成器内部，后续可在模块继续膨胀时再拆出。

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

`CodeExportTarget` 不应直接由 CLI 硬编码构造；正式流程应来自 Project Config 的 `[[targets.code]]`。runtime data 的具体格式由 code task 引用的 `[[targets.data]]` 任务选择，并通过 `RuntimeDataFile.path` 的真实后缀传递给 CodeModel；code-only target 没有 data artifact 时当前默认推导 `<table>.json`。

### 4.4.2.1 Runtime data format

解析规则：

- Project Config 中 `[[targets.data]].format = "json"` 选择 JSON exporter。
- Project Config 中 `[[targets.data]].format = "messagepack"` 选择 MessagePack exporter。
- 可兼容解析 `"msgpack"` 为 MessagePack，但错误信息、文档、生成代码和 manifest 中统一使用规范名 `messagepack`。
- 项目当前仍处于开发阶段，不保留旧 `[targets.data]` table 形态兼容；Project Config 只接受 `[[targets.data]]` 数组表。

Project Config 层建议模型：

```rust
pub struct TargetConfig {
    pub name: String,
    pub tags: Vec<String>,
    pub data: Vec<TargetDataConfig>,
    pub code: Vec<CodegenTaskConfig>,
}

pub struct TargetDataConfig {
    pub name: Option<String>,
    pub input: PathBuf,
    pub output: PathBuf,
    pub format: String,
}

pub struct CodegenTaskConfig {
    pub language: CodeExportLanguage,
    pub output: PathBuf,
    pub module_name: Option<String>,
    pub data: Option<String>,
    // file_header / naming / language-specific options ...
}
```

`TargetDataConfig.name` 在 raw config 中允许 `Option<String>`，仅用于支持“单 data task 可省略名称”的用户体验。parse 后应立即构造 normalized model；后续 validate / build / manifest / codegen 不再直接消费 raw target：

```rust
pub struct NormalizedTargetConfig {
    pub name: String,
    pub tags: Vec<String>,
    pub data: Vec<NormalizedTargetDataConfig>,
    pub code: Vec<NormalizedCodegenTaskConfig>,
}

pub struct NormalizedTargetDataConfig {
    pub name: String,
    pub input: PathBuf,
    pub output: PathBuf,
    pub format: String,
}

pub struct NormalizedCodegenTaskConfig {
    pub task: CodegenTaskConfig,
    pub selected_data_name: Option<String>,
}
```

归一化与校验规则：

- target 内 data task 为 0 个时，code-only target 合法。
- target 内 data task 为 1 个时，`name` 可省略；省略时规范化名称固定为 `default`。
- target 内 data task 多于 1 个时，每个 data task 必须有非空 `name`，且名称在 target 内唯一。
- data task name 大小写敏感；只允许 ASCII 字母、数字、`_`、`-`，且必须以字母或数字开头和结尾。
- normalized model 中 `NormalizedTargetDataConfig.name` 和 `RuntimeConfigForgeResult.data_name` 都使用非空 `String`。
- code task 的 `data` 为空时：若 target 有 0 个 data task，`selected_data_name = None`，传入空 `runtime_data_files` 并让 CodeModel 默认推导 `.json`；若只有 1 个 data task，自动填入 `selected_data_name`；若有多个 data task，配置校验报错。
- code task 的 `data` 非空时，必须能在当前 target 的 data task 中找到同名任务。
- `input`、`output`、`code.output` 相对路径在加载 Project Config 片段时按片段文件所在目录归一化；入口 `ConfigForge.toml` 的相对路径等价于 project root 相对路径。
- DataLoader 的 `data_root` 使用 `TargetDataConfig.input`，Meta `source` / `sources[].path` 再相对该 data root join。

文件规划规则：

| Format | 文件后缀 | exporter 写出方式 | 生成代码读取方式 |
| --- | --- | --- | --- |
| `Json` | `.json` | `serde_json::to_vec_pretty` 写出 payload envelope。 | `RuntimeDataLoader` 根据 `DATA_FILE` / `DataFile` / `dataFile` 打开 JSON payload 为通用 reader；生成代码读取 rows 并构造 typed rows。 |
| `MessagePack` | `.msgpack` | `rmp-serde` 写出同一 payload envelope 的 MessagePack bytes。 | `RuntimeDataLoader` 根据 `DATA_FILE` / `DataFile` / `dataFile` 打开 MessagePack payload 为通用 reader；生成代码读取 rows 并构造 typed rows。 |

`RuntimeDataFile` 中保存的 `path` 必须是已带正确后缀的相对文件名，例如 `items.json` 或 `items.msgpack`。该路径相对当前 data artifact root / runtime loader root，不相对 code output 或 project root。CodeModel 优先使用 runtime data export 阶段返回的真实路径；当 target 没有 data export artifact 时，才默认推导 `<table>.json`。

当一个 target 同时导出多套 data task 时，`build_target` 应按 data task 分组返回：

```rust
pub struct RuntimeConfigForgeResult {
    pub data_name: String,
    pub input: PathBuf,
    pub output: PathBuf,
    pub format: String,
    pub files: Vec<RuntimeDataFile>,
}
```

`write_generated_code` 在构造每个 `CodeExportInput` 时只传入该 code task 的 `selected_data_name` 对应的 `RuntimeConfigForgeResult.files`，避免同一 table 同时出现 `items.json` 和 `items.msgpack` 时 CodeModel 无法确定默认文件名。未来 manifest 可以记录所有 data task；codegen 输入只保留当前 code task 所需的视图。

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
    pub fragments: Vec<CodeFragment>,
    pub indexes: Vec<CodeIndex>,
    pub foreign_keys: Vec<CodeForeignKey>,
    pub primary_index: Option<String>,
    pub data_file: String,
}

pub struct CodeFragment {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
}

pub struct CodeForeignKey {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub target_table: String,
    pub target_index: String,
    pub allow_empty: bool,
}
```

IR 职责：

1. 保留 Meta 中的语义名称和描述信息。
2. 根据 `target_tags` 过滤不应导出的表、字段和索引，例如 `export = false`、`export_when = { any = ["client"] }`。
3. 保留 table、field、index、foreign key、enum、struct 的语言无关关系。
4. 保留 runtime data file 信息。
5. 把 `LinkedField` 转换为目标语言无关的 `CodeTypeRef`。
6. 对 polymorphic table 保留 fragment 类型、table-fragment 关联和当前 target 可见的 fragment fields。

IR 不负责：

- 目标语言标识符生成；
- table row type 名称；
- fragment type 名称；
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
- 渲染 foreign key 关联 helper，例如 Rust `fn item<'a>(&self, data: &'a GameData, row: &QuestRow) -> Option<&'a ItemsRow>`、C++ `GetItem`、TypeScript `getItem`；
- 处理 import/include/use。

Foreign key 对 CodeModel 的要求：

- 外键不是普通 validator，而是 Meta 关系数据。validation 只负责保证关系完整性；codegen 需要读取同一份 `LinkedForeignKeyMeta` 生成关联查询 API。
- 只有当前 target 可导出的 foreign key 进入 `CodeForeignKey`。
- source table、target table、target index、source fields、target index fields 都必须在当前 target CodeModel 可见。
- target index 必须 unique，第一阶段只生成 many-to-one helper；一对多反向索引可在后续 relation 设计中扩展。

### 4.5.1 Polymorphic Fragment CodeModel

variant source mode 落地后，CodeModel 需要表达三件事：

1. 每个 fragment 的字段类型，用于生成 fragment struct/interface。
2. 每个 polymorphic table 可能拥有的 fragments，用于 row type 和访问器生成。
3. 当前 target tags 下可见的 fragment/field 集合，用于避免生成无法出现在 runtime payload 中的 API。

构建规则：

- `CodeFragment` 来自 `LinkedFragmentMeta` 或等价 linked 结构；名称在 table scope 内唯一。
- 如果不同 table 下有同名 fragment，生成器必须通过 table name 参与命名，例如 Rust `ItemsStatsFragment` / TS `ItemsStatsFragment`，避免全局类型冲突。
- `CodeTable.fields` 只包含 common fields。
- `CodeTable.fragments` 只包含当前 target 可见 fragments；fragment 内字段再按 field `export_when` 过滤。
- `CodeFragment` 是 table-local IR，不再在 `CodeModel` 顶层保存一份全局 fragments；如果生成器需要全局遍历 fragment，可以从 `CodeModel.tables[*].fragments` flatten，避免 `CodeFragment` / `CodeTableFragment` 双结构重复导致不一致。
- primary/unique index 第一阶段只允许使用 common fields，因此 `CodeIndex.fields` 不需要支持 fragment path。
- runtime data 中不额外输出 `fragments` 数组；fragment nested object 是否存在就是 row 的 fragment presence。生成器不需要为 `fragments` metadata 生成字段。

推荐生成 API：

| 语言 | Row 表达 | Fragment 访问 |
| --- | --- | --- |
| Rust | row struct 包含 common fields、每个 fragment `Option<FragmentType>` | `fn has_stats(&self) -> bool`、`fn stats(&self) -> Option<&ItemsStatsFragment>` |
| C++ | row struct 包含 common fields、每个 fragment `std::optional<FragmentType>` | `HasStats()`、`Stats()` 返回 pointer/optional reference |
| TypeScript | row interface 包含 common fields、每个 fragment 可选属性 | `hasStats(row)` 或 row helper class 的 `hasStats()` |

`has_*` 的语义直接来自对应 optional fragment 字段是否存在，而不是额外的 fragment name list。若 runtime payload 中出现空 object，例如 `"stats": {}`，loader 仍应构造 `Some(ItemsStatsFragment { ... })` / `std::optional` / 非 `undefined` 对象，以保留 presence。

第一阶段不要求生成 discriminated union。原因是一个 source 可以声明多个 fragments，例如 weapon 同时拥有 `stats` 和 `skills`，更接近 composition 而不是单一 enum variant。后续如果产品约束“一条 record 必须且只能属于一个 kind”，可以在此基础上额外生成 union facade。

runtime loader 只负责把底层 JSON / MessagePack / 引擎资源暴露为通用 reader。生成代码负责识别 fragment object 是否存在、读取 fragment 内字段、执行 string -> typed primitive 转换并构造 typed rows；fragment fields 与普通字段遵循同一 generated parser 规则。

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
    // 字段缺失返回 Ok(None)，读取/格式错误返回 Err。
    fn field(&self, name: &str) -> Result<Option<Self>, String>
    where
        Self: Sized;
    fn index(&self, index: usize) -> Result<Self, String>
    where
        Self: Sized;
    fn len(&self) -> Result<usize, String>;
    fn as_i64(&self) -> Result<i64, String>;
    fn as_f64(&self) -> Result<f64, String>;
    fn as_bool(&self) -> Result<bool, String>;
    fn as_string(&self) -> Result<String, String>;
    fn is_null(&self) -> bool;
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
    pub const DATA_FILE: &'static str = "items.msgpack";

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

### 4.8.2 C++ 生成内容

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
    virtual bool HasField(const std::string& name) const = 0;
    virtual std::unique_ptr<IRuntimeDataReader> Field(const std::string& name) const = 0;
    virtual std::unique_ptr<IRuntimeDataReader> Index(std::size_t index) const = 0;
    virtual std::size_t Size() const = 0;
    virtual std::int64_t AsInt64() const = 0;
    virtual double AsDouble() const = 0;
    virtual bool AsBool() const = 0;
    virtual std::string AsString() const = 0;
    virtual bool IsNull() const = 0;
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
    static constexpr const char* DataFile = "items.msgpack";

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
- 运行时 JSON / MessagePack 解析库不强绑定。生成代码只定义通用 `IRuntimeDataLoader` / `IRuntimeDataReader`，由业务实现；可在后续提供基于 `nlohmann::json`、`msgpack-c` 或引擎序列化系统的可选 adapter。按表读取、字段转换和 typed row 构造由生成的 `Load` / `ReadRow` 实现完成。
- 复合 key 需要生成 `struct Key`、`operator==` 和 hash。
- enum class 需要生成 hash 或转换为底层整数用于 unordered_map。

### 4.8.3 TypeScript 生成内容

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
  field(name: string): RuntimeDataReader | undefined;
  index(index: number): RuntimeDataReader;
  length(): number;
  asNumber(): number;
  asBool(): boolean;
  asString(): string;
  isNull(): boolean;
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
  static readonly dataFile = "items.msgpack";

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

Reader 基础语义：

- `field(name)` / `Field(name)` / `field(name)` 用于 named-field layout。字段不存在时应返回 `None` / `undefined` / 可检测的 missing 状态，而不是伪造空值；读取过程本身出错才返回 error / throw。
- `index(i)` 用于 array 或 positional record。越界是读取错误，应带上 index 上下文。
- `len` / `Size` / `length` 只对 array / object / map 有意义；对 scalar 调用应报错。
- 最小 scalar API 包含 int、float/number、bool、string。`text`、`asset` 按 string 读取；enum 先按 string 读取，再由 generated parser 校验枚举值。
- 在当前 exporter 仍可能输出字符串 cell 的过渡期，推荐 generated parser helper 兼容 string-to-int、string-to-float、string-to-bool；reader 也可以直接在 `as_*` 中兼容字符串，但跨语言一致性以 generated parser 规则为准。
- missing 与 null 必须区分：missing 表示字段不存在；null 表示字段存在但值为空。required 字段 missing 或 null 都应由 generated parser 报错；optional field / optional fragment missing 时得到 `None` / `undefined`；null 是否允许作为 optional empty value 由生成器策略统一决定，第一阶段建议将 null 等价为 optional empty，但对 required 字段仍报错。
- 错误上下文由 generated parser 包装 table、file、row index、field path；reader 只需要报告底层类型/格式错误。

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

因此目标态生成代码依赖一个通用 loader / reader 抽象：loader 给定逻辑 data file path，负责打开对应 JSON / MessagePack / 引擎资源并返回格式无关的 reader；生成代码再用 reader 读取 payload envelope `{ meta, rows }`、逐字段解析并返回 typed rows。当前已落地实现仍是按表生成的 typed loader 方法，这是过渡形态；后续实现必须迁移到 Meta-agnostic loader，避免用户每新增表或字段都要修改 loader 实现。

### 4.10.2 语言无关接口

```text
RuntimeDataLoader
  open(path) -> RuntimeDataReader

RuntimeDataReader
  field(name) / index(i)
  begin_object / end_object
  begin_array / end_array / len
  read_scalar(type)
```

Rust / C++ / TypeScript 生成代码分别暴露同名语义的 `RuntimeDataLoader` / `IRuntimeDataLoader` / `RuntimeDataLoader`，以及配套 `RuntimeDataReader` / `IRuntimeDataReader`。接口方法不得按表名或字段名生成；表名、字段名、字段顺序只出现在生成的 parser 函数内部。

reader 需要支持两种读取模式：

1. named-field：生成代码调用 `field("id")`、`field("name")` 等方法，适合 JSON object、MessagePack map、引擎反射对象。
2. positional：生成代码按 CodeModel 中稳定字段顺序调用 `index(0)`、`index(1)` 或连续 `read_next_*`，适合未来紧密二进制布局。选择 positional layout 时，exporter 和 codegen 必须共享同一字段顺序；顺序定义应来自 linked Meta 中 target-visible fields、fragment fields 和 object fields 的稳定遍历结果。

### 4.10.3 数据格式适配

加载器实现只负责打开资源并提供通用 reader；生成代码负责解析业务对象：

```text
loader.open("items.msgpack")
  -> read bytes/text
  -> decode to generic reader/value/cursor
  -> generated code reads `rows`
  -> generated code loops rows and reads fields `id`, `name`, ...
  -> generated code constructs Vec<ItemRow>
  -> ItemsTable::new / constructor
  -> indexes
```

这样让生成代码保持对文件系统和具体解析库无绑定，同时让用户 loader 保持稳定。代价是生成代码需要包含每种语言的 runtime parser helper（字段读取、类型转换、required/default/optional 处理、错误上下文），但这些 helper 可以由生成器稳定输出，不需要用户手写。

### 4.10.4 默认 loader

可以为每种语言生成一个可选的默认 loader / reader adapter 示例，但核心产物只依赖接口，不强制生成默认文件 loader：

| Language | 默认实现 |
| --- | --- |
| Rust | `FsRuntimeDataLoader { root: PathBuf }` + `SerdeValueReader`，可选生成 |
| C++ | `FileSystemDataFileLoader { root }` + JSON/MessagePack value reader，依赖可选生成 |
| TypeScript | `FetchDataFileLoader` / `NodeFsDataFileLoader` + JS value reader，二选一或都不默认生成 |

当前实现：

- Rust / C++ / TypeScript 当前已落地实现仍生成 typed loader interface，不绑定文件 API；这是过渡状态。
- 目标态应改为生成通用 loader / reader interface，并由生成代码完成 typed parsing。
- 集成测试 harness 应随目标态调整：harness 实现一次通用 reader，新增表或字段时无需改 harness 中的 loader 接口，只需要更新断言覆盖代表性查询。
- 后续可按语言新增可选 adapter，而不是强制把文件系统或第三方解析库纳入生成代码。

默认 decoder 策略：

- 核心生成代码生成通用 reader interface，但不强制 runtime 依赖具体 JSON / MessagePack 包。
- JSON / MessagePack 字节到通用 reader/value 的解析放在业务侧 `RuntimeDataLoader` 实现或后续可选 adapter 中。
- 通用 reader/value 到 typed rows 的解析放在生成代码中。
- MessagePack decoder 不绑定具体 npm / C++ / Rust 包。

### 4.10.5 从 typed loader 迁移到通用 reader loader

迁移应按可验证的小步推进，避免一次性重写三种语言导致测试真空：

1. 生成三种语言的 `RuntimeDataReader` trait/interface，并保留 typed loader 过渡接口直到对应语言 parser 测试通过。
2. 为 table row、object table、named struct、fragment 生成 `read_row` / `ReadRow` / `readRow` 和 `read_object` parser helper。
3. 将 table/object `load` 从 `loader.load_<table>_rows(DATA_FILE)` 切换为 `loader.open(DATA_FILE)`，由生成代码读取 `rows` 或 object payload。
4. 先用 TypeScript JS value reader 做 spike，验证 bool、float、enum、optional fragment、missing field、default field 和 string cell 兼容策略。
5. 再迁移 Rust reader，验证 associated type、owned/borrowed cursor、`as_string` 生命周期和 MessagePack value reader 方案。
6. 最后迁移 C++ reader，初期可接受 `unique_ptr` cursor 简化实现；如果性能压力明显，再优化为非分配 value/view cursor。
7. 更新 codegen 单测和 standard_project harness；所有语言都迁移后，删除 typed loader 断言和 typed harness 过渡说明。

迁移验收标准：生成代码不再出现按表的 loader 方法；新增表、字段、fragment 或 object table 字段时，用户侧 loader / reader 接口和 harness loader 实现不需要改动，只需要重新生成代码并更新业务断言。

## 4.11 Runtime data 文件约定

当前 exporter 输出形态是包含 `meta` 和 `rows` 的 payload。JSON 与 MessagePack 使用同一逻辑结构，只是编码方式不同。目标态中业务侧 `RuntimeDataLoader` 只需要把该 payload 暴露为通用 reader；生成代码读取时至少应支持当前 shape：

```json
{
  "meta": { "name": "items" },
  "rows": [
    { "id": "1", "name": "Sword" }
  ]
}
```

未来也可以支持更轻量的：

```json
[
  { "id": 1, "name": "Sword" }
]
```

第一阶段推荐导出 runtime data 时同步调整为 typed payload：

- int/float/bool 输出 number/bool；
- string/text/asset/enum 输出 string；
- struct 后续输出 object。

如果短期仍输出字符串 cell，生成代码必须在加载时做类型转换。这会导致三种语言生成重复 parse helper，但不会要求业务 loader 按表/字段重复实现转换逻辑。

建议分两步：

1. Codegen 第一阶段先兼容当前 `{ meta, rows }`，由通用 reader 暴露 runtime value，并由生成代码把字段 parse 为目标类型。
2. Data typed export 落地后，生成代码的 parse helper 可以减少 string parsing；loader / reader 接口仍保持不变。

## 4.11.1 Exporter 写出实现方案

runtime data format 由 data export registry 选择，并通过 `RuntimeDataFile.path` 的文件名后缀传给 codegen；当前 codegen 层不再生成 `RuntimeDataFormat` 类型。目标态可用枚举描述如下：

```rust
pub enum TargetDataFormat {
    Json,
    MessagePack,
}
```

写出流程：

1. Project Config load 后先构造 `NormalizedTargetConfig`，完成 data task name 规范化、code task `selected_data_name` 自动绑定和引用校验。
2. `build_target` 消费当前 target 的 `Vec<NormalizedTargetDataConfig>`。没有 data task 时跳过 runtime data 导出，进入 code-only 流程。
3. 按 `NormalizedTargetDataConfig.input` 分组。每个不同 input 构造一次 `DataLoadPlan`，使用 `DataLoader::new(data_root)` 加载；Meta 中 source path 继续由 `data_root.join(source.path)` 解析。
4. 对每个不同 input 的 DataSet 执行 normalize、target validation，并派生 DataSet-dependent ExportView；同 input 的 JSON / MessagePack 等多个 data task 复用该结果。不同 input 即使属于同一 target，也不能复用另一个 input 的 DataSet / validation view / concrete ExportView。
5. 每个 data task 根据自身 `format` 从 `DataFormatRegistry` 选择 exporter，根据自身 `output` 写出一套 runtime data artifact。
6. `Json` 使用 `serde_json::to_vec_pretty` 写出 `<table>.json`。
7. `MessagePack` 使用 `rmp_serde::to_vec_named` 或等价 named-field 编码写出 `<table>.msgpack`。
8. 每个 data task 返回独立的 `RuntimeConfigForgeResult { data_name, input, output, format, files }`，其中 `RuntimeDataFile { table_name, path }` 必须使用真实写出的文件名。
9. `write_generated_code` 根据每个 `NormalizedCodegenTaskConfig.selected_data_name` 选择对应 `RuntimeConfigForgeResult.files` 传入 `CodeExportInput`，让 CodeModel 使用当前 code task 绑定的数据文件名。
10. Artifact path 规划必须跨所有 data task、code task 和未来 special exporter 统一检查，不能只在 codegen 内检查 generated code 的重复路径。

依赖建议：

```toml
rmp-serde = "1"
```

如果后续引入 manifest，manifest 中也应以 `(target_name, data_name, table_name)` 作为 runtime data artifact key，并记录每个 artifact 的 `format`、相对 data artifact root 的 `path`、最终写入 path 和 `table_name`。

## 4.11.2 Codegen 读取实现方案

CodeModel 构建时需要使用 `RuntimeDataFile`：

- 若 `runtime_data_files` 中存在 table 对应 artifact，使用 artifact path。
- 若不存在，则默认推导 `<table>.json`。
- 每个 `CodeTable` 保留 `data_file`，语言生成器用它生成常量。
- 同一个 `CodeExportInput.runtime_data_files` 中同一 table 只能出现一个 artifact；多个 data format 的选择在 exporter 编排层通过 `NormalizedCodegenTaskConfig.selected_data_name` 完成，不进入 CodeModel 决策。

生成代码的 load 流程统一为：

```text
Table::load(loader)
  -> reader = loader.open(DATA_FILE)
  -> rows_reader = reader.field("rows") 或 reader.index(rows_slot)
  -> rows = generated parser loops rows_reader and calls read_<table>_row
  -> Table::new / constructor(rows)
  -> build indexes
```

这样 JSON 与 MessagePack 的字节解析差异落在 loader / reader 实现里，表结构、字段名、字段顺序、类型转换、required/default/optional 处理落在生成代码里，repository、index、GameData facade 不需要关心具体解析库。

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

1. `TargetDataFormat` / registry 解析：`json`、`messagepack`、可选别名 `msgpack`，以及未知格式报错。
2. Runtime data 写出：
   - JSON target 生成 `<table>.json`，内容可用 `serde_json` 读取；
   - MessagePack target 生成 `<table>.msgpack`，内容可用 `rmp-serde` 读取为同一 payload；
   - `RuntimeDataFile.path` 与实际文件后缀一致。
3. Project Config schema：接受 `[[targets.data]]`；旧 `[targets.data]` table 形态不作为兼容目标；空 data task name、多 data task 缺 name、非法字符、重复 name 都应报错；单 data task 省略 name 时规范化为 `default`。
4. 多 data task：同一 target 同时配置 JSON 与 MessagePack，两个 output 都生成对应文件，且同 input 的数据加载/normalize/validation 可以复用。
5. 多 input：不同 input 分别加载、normalize、validation 并派生 DataSet-dependent ExportView，不能复用另一个 input 的 ExportView。
6. code task 绑定：target 只有一个 data task 时 `CodegenTaskConfig.data` 可省略并在 normalized model 中填入 `selected_data_name`；多个 data task 且省略 `data` 时报错；显式 `data = "json"` / `data = "messagepack"` 时 `CodeExportInput.runtime_data_files` 使用对应真实路径；显式引用不存在 data task 时报错。
7. 路径语义：入口 `ConfigForge.toml` 中 data input/output/code output 相对 project root；import 片段中的相对路径相对片段目录；Meta source path 相对 data input；`RuntimeDataFile.path` 相对 data artifact root。
8. artifact 冲突：data task 之间、data task 与 code task 之间、多个 code task 之间的输出路径冲突都应报错。执行冲突检查前应做不依赖文件存在性的 lexical path normalization，至少消除 `.` / `..` 造成的字符串差异；对已存在路径可视需要再做 canonicalize，但不能要求输出目录预先存在。
9. CodeModel 默认文件名：没有 runtime data artifact 时，默认推导 `.json`。
10. Rust 生成代码包含：
   - 通用 `RuntimeDataLoader` / `RuntimeDataReader`；
   - 每张表的 `DATA_FILE`；
   - row struct、table parser、table repository 和 index helper。
11. C++ 生成代码包含：
   - 通用 `IRuntimeDataLoader` / `IRuntimeDataReader`；
   - 每张表的 `DataFile`；
   - row struct、table parser、table repository 和 index helper。
12. TypeScript 生成代码包含：
   - 通用 `RuntimeDataLoader` / `RuntimeDataReader`；
   - 每张表的 `dataFile`；
   - row interface、table parser、table repository 和 index helper。
13. CLI integration：分别构建 JSON target、MessagePack target，以及同 target 多 data + 多 code 的标准工程，验证 runtime data 文件和 generated code 文件同时生成，并检查代码中引用的默认文件名与绑定 data task 一致。

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
7. polymorphic table 的 common fields 与 fragments 能进入 `CodeModel`，fragment fields 按 target tags 过滤。
8. 同名 fragment 出现在不同 table 时，生成类型名应包含 table scope，避免冲突。
9. fragment field 不能作为 index key；Meta/CodeModel 层应给出明确错误或在 Meta validation 阶段提前拒绝。

### 4.15.2 语言生成器 snapshot 测试

对同一个小 Meta fixture 生成三种语言代码，断言关键片段：

- enum 声明；
- struct/interface 声明；
- fragment struct/interface 声明；
- row type；
- row 中的 optional fragment 字段；
- `has_fragment` / `get_fragment` 风格访问器；
- 通用 loader / reader interface；
- generated row/object parser helper；
- repository type；
- unique / non-unique lookup function；
- GameData facade。

推荐先使用字符串 contains 测试，后续引入 snapshot 工具。

### 4.15.3 生成代码编译与运行测试

生成代码测试不应只停留在“文件存在”和字符串片段断言。集成测试需要把 runtime data artifact 与 generated code artifact 串起来，验证生成代码能被目标语言工具链接受，并能通过 generated loader interface 读取真实导出的数据。

当前测试分两层描述：

- **Current regression harness**：在 typed loader 过渡期，harness 可以实现当前生成代码要求的按表 typed loader 方法，但必须读取真实 runtime data artifact，并把真实 JSON / MessagePack payload 映射为 generated rows / objects，不能退回纯 mock 数据。
- **Target reader harness**：通用 reader loader 落地后，harness 改为只实现一次 `open + RuntimeDataReader`；typed rows / typed object 映射必须由生成代码完成，harness 不再按表手写 generated rows。

通用流程：

1. 在临时目录或被 `.gitignore` 忽略的 fixture `out/` 目录中执行 export，生成 runtime data 与对应语言代码。
2. 为每种目标语言创建测试 harness。当前过渡期 harness 实现 typed loader 并读取真实 runtime data；目标态 harness 实现通用 `RuntimeDataLoader` / `IRuntimeDataLoader` 与 reader 接口，并把 JSON / MessagePack payload 暴露为通用 reader。
3. 编译或类型检查 generated code 与 harness。
4. 运行 harness 测试 `GameData` / facade 的加载入口。
5. 断言代表性查询函数正常工作。测试不需要覆盖每一个字段和每一个 index，但必须覆盖当前功能类型：
   - collection table 加载；
   - object table 加载；
   - 单字段 unique index 查询；
   - 多字段联合 unique index 查询；
   - 目标语言对默认 data file 常量的引用。
6. 如果未来标准工程新增 non-unique index、foreign-key helper、special exporter manifest 等生成代码能力，应在同一类 generated-code runtime test 中补充一个代表性断言，而不是为所有表重复写同构断言。

测试 harness 的 loader 是测试代码，不是产品默认 loader。生成代码仍保持对文件系统、JSON/MessagePack 库和引擎资源系统无依赖；真实项目可以按自己的资源系统实现 loader。目标态下新增表或字段时，harness / 用户侧 loader 接口不应变化。

每种语言的 harness 代码应作为 fixture 的一部分归档进代码仓库，例如 `tests/fixtures/standard_project/harness/{rust,cpp,typescript}/`。集成测试运行时将已归档的 harness 与本次 export 产生的 generated code / runtime data 组合到临时目录或编译命令中：harness 本身稳定纳入 review，generated artifacts 仍位于被忽略的 `out/` 或临时目录中，避免把本次生成结果当作源文件提交。

### 4.15.4 分语言 generated-code runtime test 方案

标准工程当前按 target 拆分：client 生成 JSON + TypeScript/C++，server 生成 MessagePack + Rust。因此 generated-code runtime test 可以复用标准工程配置，分别验证三种语言。

| 语言 | 输入产物 | 工具链 | Harness 做法 | 必测断言 |
| --- | --- | --- | --- | --- |
| Rust | `out/server_code/rust/generated.rs` + `out/server_data/*.msgpack` | `cargo test` | 当前：实现 typed `RuntimeDataLoader`，用 `rmp-serde` 读取真实 MessagePack payload 并映射 generated rows / objects。目标：实现通用 `RuntimeDataLoader` / reader，不手写 generated rows / objects 映射。 | `GameData::load` 成功；`items.get_by_id(id)` 命中；`class_levels.get_by_class_level(class_id, level)` 命中联合索引；`game_rules` object 字段可访问。 |
| TypeScript | `out/client_code/typescript/generated.ts` + `out/client_data/*.json` | `tsc` + `node` | 当前：实现 typed `RuntimeDataLoader`，用 Node `fs` 读取真实 JSON payload 并映射 generated row interface/object。目标：实现通用 reader，由 generated parser 映射 row interface/object。 | `GameData.load` 成功；`items.getById(id)` 命中；`classLevels.getByClassLevel(classId, level)` 命中联合索引；object table 可访问。 |
| C++ | `out/client_code/cpp/generated.hpp` / `generated.cpp` + `out/client_data/*.json` | `c++ -std=c++17` | 当前：实现 typed `IRuntimeDataLoader`，使用轻量 JSON 读取/映射逻辑构造 generated rows。目标：实现通用 `IRuntimeDataLoader` / reader，不能按表手写 typed rows。 | `GameData::Load` 成功；`Items().GetById(id)` 命中；`ClassLevels().GetByClassLevel(class_id, level)` 命中联合索引；object table 可访问。 |

归档后的 harness 不应直接依赖仓库中某一次已生成的 `out/` 文件路径。推荐做法是：集成测试先执行 export，再把对应语言的 generated code 复制到临时 harness 工作目录（或通过 include path/link path 引用），最后执行目标语言测试命令。这样能确保测试的是“当前实现刚生成的代码和数据”，而不是历史残留 artifact。

工具链缺失时的处理原则：

- CI 环境应安装 Rust/Cargo、C++17 compiler、Node.js 和 TypeScript，使三类 runtime test 都能执行。
- 本地开发环境如果缺少 C++ compiler、Node 或 `tsc`，对应测试可以显式 skip，但测试输出必须说明 skip 原因，不能静默通过。
- Rust 生成代码测试不应 skip，因为项目本身依赖 Rust toolchain。

### 4.15.5 CLI 集成测试

在 `tests/fixtures/standard_project/` 上运行：

1. export 生成 runtime data。
2. export 生成 Rust/C++/TS code。
3. 检查输出文件存在。
4. 检查关键 API 名称存在。
5. 运行分语言 generated-code runtime test，确认生成代码能通过 loader 加载真实导出数据并执行代表性查询。

集成测试写出的 artifact 应位于 fixture-local `out/` 目录或临时目录。fixture-local `out/` 必须通过 `.gitignore` 忽略；新增 fixture 如果也写出导出结果，应遵循 `tests/fixtures/**/out/` 约定，避免误提交生成产物。

## 4.16 第一阶段落地状态

当前第一阶段主体已落地，历史分阶段计划更新为实现状态：

### Phase 1：CodeModel 与 Rust 生成器

状态：已完成。

1. 已新增 `src/codegen/`。
2. 已实现 CodeModel IR。
3. 已实现 naming / type mapping 和生成符号冲突检测。
4. 已移除临时 Rust stub 路径，改由 `CodeGeneratorRegistry` 分发。
5. 已生成 Rust enum / struct / row / table / index helper / typed loader trait；通用 reader loader 仍待替换。
6. 已增加单元测试和标准 fixture 回归。

### Phase 2：TypeScript 生成器

状态：已完成核心单文件生成。

1. 当前输出 `generated.ts`，包含 type / table / loader / GameData facade。
2. 已支持 async typed loader；通用 reader loader 仍待替换。
3. 已支持 Map-based indexes，并为 unique index 增加重复 key 防御。
4. 当前测试以语义 contains 断言为主，snapshot/golden file 可后续补充。

### Phase 3：C++ 生成器

状态：已完成核心 header-only 生成。

1. 当前输出 `generated.hpp`。
2. 已生成 typed loader interface；通用 reader loader 仍待替换。
3. 已生成 enum、row、repository 和 index helper。
4. 后续可再拆 `.hpp` / `.cpp` 或补充更完整 enum conversion。

### Phase 4：Artifact Writer 和 manifest

状态：部分完成。

1. runtime data 与 generated code 都以 `GeneratedFile`/artifact 形态参与写出。
2. 已做输出路径冲突检查。
3. 当前写入策略为 direct write；`temp_commit` 未实现。
4. manifest 未实现。

## 4.17 第一阶段完成标准

完成时应满足：

- Rust、C++、TypeScript 三种语言至少能生成类型和表访问代码。（已满足）
- 所有 enum、struct、table row type 都有生成结果。（已满足；struct 数据值 validation 仍 reserved）
- 当前 target 可见的 table index 都有对应辅助查找函数。（已满足）
- 每种语言都有抽象 loader 接口。（当前以 typed loader 过渡满足；目标态需改为 Meta-agnostic 通用 reader loader）
- 生成代码不直接依赖 ConfigForge CLI 内部类型。
- 命名冲突和非法标识符不会静默生成错误代码。
- exporter 不再使用临时 Rust stub。
- `cargo fmt`、`cargo clippy --all-targets --all-features`、`cargo test` 通过。
- 标准工程 fixture 覆盖 runtime data + code export 输出。
