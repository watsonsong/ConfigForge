# 09. Codegen 模块详细设计

本文档描述 ConfigForge 的代码导出任务设计。目标是在已加载、已链接并通过校验的 Meta 与 Data 基础上，生成 Rust、C++ 和 TypeScript 三种语言的运行时代码，覆盖枚举、结构体、数据表、索引辅助查找函数，并提供抽象的数据文件加载器接口，让接入方可以从本地文件、包内资源、网络或引擎资源系统中加载实际数据。加载器接口必须与具体 Meta、表和字段解耦；所有按表/字段解析和 typed row/object 构造职责属于生成代码。

产品层语义仍以 `Documentation/Design/` 为准；本文档聚焦程序实现模块、生成代码形态、跨语言约束、运行时加载抽象和测试策略。

## 9.1 设计目标

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
5. 支持 JSON、MessagePack 与 `bin` 紧密二进制 runtime data format。生成代码通过 runtime data export 阶段返回的真实文件名暴露每张表的默认数据文件路径；`bin` 额外携带每表 schema revision 供 generated reader 校验。
6. 生成抽象 `RuntimeDataLoader` / `RuntimeDataReader`，由接入方 loader 只负责打开数据文件并提供通用 reader，生成代码负责所有表/字段解析和 typed row/object 构造。
7. 保持生成代码确定性：同一份 Meta 生成稳定文件名、声明顺序和 API 名称。
8. 在生成前发现命名冲突、保留字、目标语言类型映射缺失、输出路径冲突等问题。

当前 loader contract 使用 Meta-agnostic reader loader：生成抽象 `RuntimeDataLoader` / `RuntimeDataReader`，由接入方负责打开资源并提供通用 object / array / scalar 读取能力；生成代码负责读取 payload envelope `{ meta, rows }`、按 Meta 字段映射到 typed rows / typed objects。loader 不能按表生成方法，也不能返回具体 `TableRow` 类型。

第一阶段不实现：

- 生成代码的插件化动态加载；
- `bin` v1 之外的复杂二进制数据格式、schema evolution 和随机访问；
- 跨语言运行时库发布；
- struct/json cell 的完整解析；
   - 未声明 `linked_field` 的 foreign key 自动跳转接口；
- 增量代码生成。

这些能力应在生成器 API 上预留扩展点，但不阻塞第一阶段落地。

## 9.2 与现有流程的关系

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

Code Export 位于独立 `src/codegen/` 模块；当前 `src/build/` 负责构造输入、分发生成器、检查输出路径冲突并写出 artifact。

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
- Data 文件格式第一阶段支持 JSON、MessagePack 与 `bin`。`bin` 使用 positional packed layout，并通过 exporter 与 CodeModel 共享的 schema revision 保证 generated reader 与数据文件布局一致。目标态生成代码只约束通用 `RuntimeDataLoader` / reader 协议。loader 实现负责打开具体文件并暴露格式无关的读取 cursor，生成代码本身不绑定文件系统、引擎资源系统、JSON 库、MessagePack 库或 binary decoder，但负责所有 table / field 级解析逻辑。

## 9.3 模块结构建议

当前 `src/codegen/` 目录模块：

```text
src/codegen/
  mod.rs              # 对外 API、re-export、CodeExportTarget / CodeExportInput / Artifact 等核心模型
  ir.rs               # Language-neutral CodeModel IR
  naming.rs           # 目标语言命名规范、保留字处理、冲突检测
  rust/
    mod.rs            # RustCodeGenerator 与生成编排
    runtime.rs        # Rust runtime loader/reader contract 渲染
    types.rs          # Rust enum/struct/table row/fragment 类型渲染
    reader.rs         # Rust table/object payload reader 渲染
    repository.rs     # Rust repository、index、database facade 渲染
    symbols.rs        # RustNames 与符号冲突检查
    tests.rs          # Rust 生成器测试
  cpp/
    mod.rs            # CppCodeGenerator 与 header/source 编排
    runtime.rs        # C++ runtime loader/reader contract 渲染
    types.rs          # C++ enum/struct/table row/fragment 类型渲染
    reader.rs         # C++ table/object payload reader 渲染
    repository.rs     # C++ repository、index、database facade 渲染
    symbols.rs        # CppNames 与符号冲突检查
    tests.rs          # C++ 生成器测试
  typescript/
    mod.rs            # TypeScriptCodeGenerator 与生成编排
    runtime.rs        # TypeScript runtime loader/reader contract 渲染
    types.rs          # TypeScript enum/interface/fragment 类型渲染
    reader.rs         # TypeScript table/object payload reader 渲染
    repository.rs     # TypeScript repository、index、database facade 渲染
    symbols.rs        # TypeScriptNames 与符号冲突检查
    tests.rs          # TypeScript 生成器测试
  writer.rs           # CodeWriter / indentation / trailing newline helper
  tests.rs            # IR、配置、命名等语言无关公共测试
```

Codegen 模块遵循开闭原则：核心模块只定义语言无关模型、生成器 trait、注册表查找和公共配置结构；每种目标语言作为一等公民独立放在自己的目录中，语言标识常量、语言专属 options、类型映射、运行时 reader contract 渲染、命名默认值和测试都在该目录内维护。新增语言时应优先通过新增 `src/codegen/<language>/`、实现 `CodeGenerator` 并注册到 builtin registry 完成，不回到 IR 或 build 编排层增加语言分支，也不在核心 `mod.rs` 中新增语言常量或语言 options。

每个语言目录的 `mod.rs` 只负责实现 `CodeGenerator`、构造 `CodeModel`、调用语言内各渲染模块并返回 `GeneratedFile`；实际渲染逻辑按 runtime contract、类型声明、reader、repository 和符号解析拆分。类型映射当前随 `types.rs` / `reader.rs` 保持在语言目录内；若单一语言继续膨胀，可继续在该语言目录内拆出 `type_mapping.rs`、`index.rs` 等，不应把语言专属逻辑回流到核心 `src/codegen/mod.rs`。

`src/build/` 保留为执行编排层，负责：

- 加载 Meta 和 Data；
- 归一化与校验；
- 构造 `CodeExportInput`；
- 调用对应 `CodeGenerator`；
- 写出 Artifact。

Artifact Writer 已拆为独立 `src/artifact/` 模块，Codegen 后续应通过 Artifact 模块统一写出：

```text
src/artifact/
  mod.rs
  model.rs            # Artifact / ArtifactContent / ArtifactPath
  writer.rs           # dry-run / direct / temp-commit 写入策略
```

`CodeGenerator` 可以继续返回 `Vec<GeneratedFile>`，但 Build 写文件必须统一转换为 `artifact::Artifact` 并交给 Artifact Writer；Codegen 自身不直接写文件。

### 9.3.1 ObjectTable codegen 边界

ObjectTable Step 1 的 codegen 语义并入 Codegen 模块：

- `CodeTable.kind` 区分 `Object` / `Collection`。
- ObjectTable 生成目标语言中的 singleton object 类型和访问入口。
- ObjectTable 不生成 collection repository，不生成 index lookup helpers。
- ObjectTable 可以携带 `CodeForeignKey` 关系元数据，用于生成从 singleton object 字段跳转到 CollectionTable repository 的 helper；当前 Rust/C++/TypeScript 生成器已渲染 `linked_field` helper/cache/link。
- Rust / TypeScript / C++ 都应避免把 ObjectTable 误表达为 rows collection。
- Runtime data file path 仍由 build 传入的 `RuntimeDataFile` 决定；codegen 不关心 ObjectTable 的 CSV `key,value` 物理布局。

建议 API 形态：

- Rust：`GameData::game_config() -> &GameConfig` 或 public field `game_config: GameConfig`。
- TypeScript：`data.gameConfig` 或 `getGameConfig()`。
- C++：`GetGameConfig()` 返回 const reference。

## 9.4 核心输入输出模型

### 9.4.1 语言标识与生成器注册表

目标语言使用字符串标识，不使用中心化枚举。这样新增一种语言时不需要修改 `CodeExportLanguage` 一类的核心 enum，也不需要在 build 层增加新的 match 分支。

```rust
pub trait CodeGenerator: Send + Sync {
    fn language(&self) -> &'static str;
    fn language_aliases(&self) -> &'static [&'static str] { &[] }
    fn supports_language(&self, language: &str) -> bool;
    fn default_module_name(&self) -> &'static str;
    fn default_naming(&self) -> NamingConfig;
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>>;
}

pub fn builtin_generators() -> Vec<Box<dyn CodeGenerator>>;
pub fn find_generator<'a>(
    generators: &'a [Box<dyn CodeGenerator>],
    language: &str,
) -> Option<&'a dyn CodeGenerator>;
```

注册表职责：

- `CodeGenerator::language()` 返回规范语言名，例如 `rust`、`cpp`、`typescript`。
- `language_aliases()` 提供兼容别名，例如 C++ 支持 `c++`，TypeScript 支持 `ts`。
- 每个语言模块维护自己的 `LANGUAGE` 常量，例如 `src/codegen/rust/mod.rs` 中的 `LANGUAGE = "rust"`。核心 `src/codegen/mod.rs` 不定义 `pub mod languages`，避免新增语言时回到核心模块增加常量。
- `CodeExportTarget::from_task` 接收已查找到的 `&dyn CodeGenerator`，由 generator 提供默认模块名和默认命名规则。
- `src/build/` 只通过 `find_generator(context.generators, task.language)` 查找生成器；找不到时报告“未注册生成器”，不感知具体语言集合。
- 内置语言的唯一集中注册点是 `builtin_generators()`。未来插件式生成器可以替换或扩展传入 build 上下文的 registry，而不改 IR。

### 9.4.2 `CodeExportTarget`

```rust
pub struct CodeExportTarget {
    pub language: String,
    pub out_dir: PathBuf,
    pub file_name: Option<String>,
    pub module_name: String,
    pub namespace: Option<String>,
    pub options: CodegenOptions,
}
```

字段说明：

| 字段 | 说明 |
| --- | --- |
| `language` | 目标语言字符串标识，保存 registry 中 generator 的规范语言名。 |
| `out_dir` | 生成代码输出目录。 |
| `file_name` | 可选生成代码文件名。Rust / TypeScript 使用该名称补齐默认扩展（`.rs` / `.ts`）；C++ 使用该名称或默认 `generated.hpp` 作为头文件名，并用同一 stem 生成 `.cpp` 源文件。该字段只能是单个文件名，不能包含路径分隔符。 |
| `module_name` | 生成模块名，例如 Rust module、TS namespace/package、C++ 文件前缀。 |
| `namespace` | C++ namespace / Rust module path / TS namespace 的可选覆盖。 |
| `options` | 从 `targets.code` 解析出的通用和语言专属选项，包括文件头、命名策略、关键字冲突策略、语言扩展参数等。 |

`CodeExportTarget` 不应直接由 CLI 硬编码构造；正式流程应来自 Project Config 的 `[[targets.code]]`。生成代码必须保持格式无关：CodeModel 只依赖 Meta 和 target tags，table data file 常量使用逻辑表名（例如 `items`），实际解析 JSON / MessagePack / bin 由运行时传入的 loader / reader 实现决定。

### 9.4.2.1 Runtime data format

解析规则：

- Project Config 中 `[[targets.data]].format = "json"` 选择 JSON exporter。
- Project Config 中 `[[targets.data]].format = "messagepack"` 选择 MessagePack exporter。
- Project Config 中 `[[targets.data]].format = "bin"` 选择私有紧密二进制 exporter。
- 可兼容解析 `"msgpack"` 为 MessagePack，但错误信息、文档、生成代码和 manifest 中统一使用规范名 `messagepack`。
- Project Config 使用 `[[targets.data]]` 数组表表达 runtime data 导出任务。

Project Config 层建议模型（`TargetConfig` 是 project-level 聚合结构；`TargetDataConfig` 由 exporter 模块定义，`CodegenTaskConfig` 由 codegen 模块定义）：

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
    pub language: String,
    pub output: PathBuf,
    pub file_name: Option<String>,
    pub module_name: Option<String>,
    pub data: Option<String>,
    pub language_options: HashMap<String, toml::Value>,
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
}
```

归一化与校验规则：

- target 内 data task 为 0 个时，code-only target 合法。
- target 内 data task 为 1 个时，`name` 可省略；省略时规范化名称固定为 `default`。
- target 内 data task 多于 1 个时，每个 data task 必须有非空 `name`，且名称在 target 内唯一。
- data task name 大小写敏感；只允许 ASCII 字母、数字、`_`、`-`，且必须以字母或数字开头和结尾。
- normalized model 中 `NormalizedTargetDataConfig.name` 和 `RuntimeConfigForgeResult.data_name` 都使用非空 `String`。
- code task 的 `data` 必须为空；若配置 `data = "..."`，归一化阶段报错。生成代码格式无关，不绑定任意 data task。
- 无论 target 有 0 个、1 个还是多个 data task，code task 都只接收 Meta、target tags 和语言配置。
- `input`、`output`、`code.output` 相对路径在加载 Project Config 片段时按片段文件所在目录归一化；入口 `ConfigForge.toml` 的相对路径等价于 project root 相对路径。
- DataLoader 的 `data_root` 使用 `TargetDataConfig.input`，Meta `source` / `sources[].path` 再相对该 data root join。

文件规划规则：

| Format | 文件后缀 | exporter 写出方式 | 生成代码读取方式 |
| --- | --- | --- | --- |
| `Json` | `.json` | `serde_json::to_vec_pretty` 写出 payload envelope。 | `RuntimeDataLoader` 根据 `DATA_FILE` / `DataFile` / `dataFile` 打开 JSON payload 为通用 reader；生成代码读取 rows 并构造 typed rows。 |
| `MessagePack` | `.msgpack` | `rmp-serde` 写出同一 payload envelope 的 MessagePack bytes。 | `RuntimeDataLoader` 根据 `DATA_FILE` / `DataFile` / `dataFile` 打开 MessagePack payload 为通用 reader；生成代码读取 rows 并构造 typed rows。 |
| `Bin` | `.bin` | 写入 `magic = "CFGB"`、format version、header size、`u64 schema_revision` 后，按 target-aware schema 顺序紧密排列 record，不保存 tag/name。 | generated code 携带每张表的 expected revision，binary reader 校验 magic/version/header/revision 后按 positional layout 顺序读取。 |

`RuntimeDataFile` 中保存的 `path` 必须是已带正确后缀的相对文件名，例如 `items.json`、`items.msgpack` 或 `items.bin`。该路径相对当前 data artifact root / runtime loader root，不相对 code output 或 project root。CodeModel 不使用这些真实 artifact path 生成加载常量；每张表统一使用逻辑表名。`bin` 的 schema revision 由 CodeModel 基于 Meta 和 target tags 计算，用于运行时 reader 暴露 revision 时进行可选校验。

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

`write_generated_code` 在构造每个 `CodeExportInput` 时传入空 `runtime_data_files`。同一 table 同时出现 `items.json`、`items.msgpack` 和 `items.bin` 时也不会影响 CodeModel；格式选择由运行时 loader / reader 完成。未来 manifest 可以记录所有 data task 的真实 artifact path，但不进入 codegen 决策。

### 9.4.2.2 `CodegenOptions`

```rust
pub struct CodegenOptions {
    pub file_header: Option<String>,
    pub keyword_conflict: KeywordConflictPolicy,
    pub naming: NamingConfig,
    pub formatting: CodeWriterConfig,
    pub language_options: HashMap<String, toml::Value>,
}

pub struct CodeWriterConfig {
    pub indent_style: IndentStyle,
    pub indent_width: usize,
    pub trailing_newline: bool,
    pub trailing_blank_line: bool,
}

pub enum IndentStyle {
    Spaces,
    Tab,
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

[targets.code.formatting]
indent_style = "spaces"
indent_width = 4
trailing_newline = true
trailing_blank_line = false

[targets.code.rust]
derive_serde = true
```

设计要求：

- `CodegenOptions` 拆成通用配置和语言特定配置两层：`file_header`、`keyword_conflict`、`naming` 为通用层，`language_options` 保存 TOML 中按语言名展开的扩展配置。
- `formatting` 是代码生成的跨语言通用格式配置，用于驱动共享 `CodeWriter`：`indent_style = "spaces" | "tab"` 控制缩进 token，`indent_width` 控制 spaces 模式下每级缩进宽度，`trailing_newline` 控制文件末尾是否至少保留一个换行，`trailing_blank_line` 控制文件结束是否保留一个空行（即内容以两个换行结束）。
- 每个语言生成器通过 `CodeGenerator::default_formatting()` 声明语言默认格式：Rust / C++ 默认为 4 空格，TypeScript 默认为 2 空格；TOML 中 `[targets.code.formatting]` 只覆盖显式声明的字段。
- 语言配置不使用 tagged enum；`CodegenTaskConfig` 通过 `#[serde(flatten)]` 把 `[rust]`、`[cpp]`、`[typescript]` 等表保存在 `HashMap<String, toml::Value>` 中。
- 每个语言生成器只能读取自己的扩展配置，例如 `RustCodegenOptions` 定义在 `src/codegen/rust/mod.rs`，并由 Rust generator 通过 `CodegenOptions::language_options::<RustCodegenOptions>(LANGUAGE)` 解析。
- 新增语言时应新增 generator、语言扩展 option struct 和该语言自己的解析逻辑；不修改 IR，不增加中心化 `LanguageCodegenOptions` enum，也不在核心模块增加语言专属 helper。
- 如果配置中出现当前语言不支持的 `CaseStyle` 或 `KeywordConflictPolicy`，应在 codegen validate 阶段报错。

### 9.4.2.3 `CodeWriter`

代码生成器应逐步从直接 `String::push_str(&format!(...))` 迁移到共享 `CodeWriter`。`CodeWriter` 位于 `src/codegen/writer.rs`，属于语言无关基础设施，只负责文本组织和格式策略，不感知 Meta、IR、目标语言 AST 或 artifact 写入。

目标 API：

```rust
pub struct CodeWriter { /* ... */ }

impl CodeWriter {
    pub fn new(config: CodeWriterConfig) -> Self;
    pub fn with_content(content: impl Into<String>, config: CodeWriterConfig) -> Self;
    pub fn raw(&mut self, text: impl AsRef<str>);
    pub fn write(&mut self, text: impl AsRef<str>);
    pub fn line(&mut self, text: impl AsRef<str>);
    pub fn blank_line(&mut self);
    pub fn indent(&mut self);
    pub fn dedent(&mut self);
    pub fn indented<R>(&mut self, write_body: impl FnOnce(&mut Self) -> R) -> R;
    pub fn finish(self) -> String;
}
```

职责边界：

- `line(...)` 在当前缩进层级写入一行并追加换行；`write(...)` 支持写入多行文本并在每个非空新行开始处补齐当前缩进；`raw(...)` 仅追加已有文本，不自动缩进，用于迁移阶段包裹既有生成片段或写入已格式化的大块 runtime helper。
- `indent()` / `dedent()` 管理块级缩进；`indented(...)` 用于保证局部块结束后恢复缩进层级。
- `finish()` 是唯一做文件尾规范化的位置：当 `trailing_blank_line = true` 时文件内容以 `\n\n` 结束；否则当 `trailing_newline = true` 时以单个 `\n` 结束；当 `trailing_newline = false` 且未要求 trailing blank line 时移除末尾换行。
- 表达式层仍可返回 `String`，例如目标语言类型表达式、reader helper 表达式和 index key 表达式；语句/块/文件层应使用 `CodeWriter`，避免在高层继续扩散 `push_str(&format!(...))`。
- 迁移允许分阶段进行：先使用 `CodeWriter::with_content(...).finish()` 统一文件尾策略和配置解析，再将 TypeScript、Rust、C++ 的 statement/block 生成函数逐步改为接收 `&mut CodeWriter`。

### 9.4.2.4 文件头模板

`file_header` 是可选模板。模板渲染上下文来自 Project Config 和执行上下文：

| 变量 | 来源 |
| --- | --- |
| `{{project.name}}` | `ProjectConfig.project.name`。 |
| `{{project.desc}}` | `ProjectConfig.project.desc`，未配置为空。 |
| `{{project.copyright}}` | `ProjectConfig.project.copyright`，未配置为空。 |
| `{{datetime}}` | 当前生成时间，由 exporter 注入。 |

文件头渲染应发生在语言生成器写入每个 `GeneratedFile` 内容前。语言生成器可以决定是否为每行追加注释前缀，但模板本身应允许用户完整控制文本。

### 9.4.3 `CodeExportInput`

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

`project` 来自 Project Config，用于渲染文件头模板；`target_tags` 来自 Build 归一化后的 effective target tags（Project target tags 与 CLI extra tags 的并集），用于构造目标相关的 CodeModel；`runtime_data_files` 当前由 build 传入空列表，保留为未来 manifest / 兼容扩展入口，不参与默认资源名决策。生成代码中的默认资源名始终是逻辑表名，例如 `ItemsTable::DATA_FILE = "items"`。`generated_at` 由 exporter 注入，用于渲染文件头中的 `{{datetime}}`。

### 9.4.4 `GeneratedFile`

```rust
pub struct GeneratedFile {
    pub path: PathBuf,
    pub content: String,
}
```

第一阶段只生成文本代码文件。后续如果生成二进制 schema 或 manifest，可扩展为 `GeneratedContent::Text | Bytes`。

## 9.5 语言无关 CodeModel IR

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
    pub kind: CodeTableKind,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub fragments: Vec<CodeFragment>,
    pub indexes: Vec<CodeIndex>,
    pub foreign_keys: Vec<CodeForeignKey>,
    pub primary_index: Option<String>,
    pub data_file: String,
    pub data_format: String,
    pub schema_revision: Option<u64>,
}

pub enum CodeTableKind {
    Object,
    Collection,
}

pub struct CodeFragment {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub payload: CodeFragmentPayload,
}

pub enum CodeFragmentPayload {
    InlineFields,
    Struct { name: String },
}

pub struct CodeForeignKey {
    pub name: String,
    pub desc: Option<String>,
    pub fields: Vec<CodeField>,
    pub target_table: String,
    pub target_index: String,
    pub allow_empty: bool,
    pub linked_field: Option<CodeLinkedField>,
}

pub struct CodeLinkedField {
    pub name: String,
    pub cardinality: CodeRelationCardinality,
}

pub enum CodeRelationCardinality {
    One,
    OptionalOne,
    Many,
}
```

IR 职责：

1. 保留 Meta 中的语义名称和描述信息。
2. 根据 `target_tags` 过滤不应导出的表、字段和索引，例如 `export = false`、`export_when = { any = ["client"] }`。
3. 保留 table、field、index、foreign key、enum、struct 的语言无关关系。
4. 保留 runtime data file 信息、format 和 `bin` schema revision。
5. 把 `LinkedField` 转换为目标语言无关的 `CodeTypeRef`。
6. 对 polymorphic table 保留 fragment 类型、table-fragment 关联和当前 target 可见的 fragment fields。
7. 对声明了 `linked_field` 的 foreign key 保留生成 relation helper 所需的语言无关信息，包括 helper 语义名、source fields、target table/index、empty 策略和单值/list cardinality。

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
- 基于带 `linked_field` 的 `CodeForeignKey` 渲染 foreign key relation helper、cache slot 和 `link` / `link_all` API；未声明 `linked_field` 的 FK 只保留关系元数据，不生成访问 helper；
- 处理 import/include/use。

Foreign key 对 CodeModel 的要求：

- 外键不是普通 validator，而是 Meta 关系数据。validation 负责保证关系完整性；CodeModel 读取同一份 `LinkedForeignKeyMeta` 并暴露 `CodeForeignKey`，为语言生成器生成关联查询 API 提供输入。
- 只有当前 target 可导出的 foreign key 进入 `CodeForeignKey`；CollectionTable 与 ObjectTable source 使用同一套 IR，target table 仍必须是 CollectionTable index。
- source table、target table、target index、source fields、target index fields 都必须在当前 target CodeModel 可见。
- target index 必须 unique，当前 IR 仅表达 many-to-one 关系；语言级 helper 和一对多反向索引可在后续 relation 设计中扩展。
- `linked_field` 在 IR 中保持 raw semantic name，不提前转换为 Rust/C++/TypeScript 标识符；最终 helper 名、cache field 名和冲突检查由各语言 `symbols.rs` / naming 模块处理。
- `CodeRelationCardinality::One` 表示非 list、非空 FK；`OptionalOne` 表示允许全空的 FK；`Many` 表示当前已支持的单 list source field FK，getter 返回与 source list 顺序一致的 target rows。若 list FK 也允许空 list，空 list 返回空集合，不等价于 optional one 的 empty relation。
- `CodeForeignKey.fields` 必须按 Meta 中声明的 source field 顺序保存，复合 key helper 按此顺序组装 key；目标 index fields 的顺序由 `target_index` 对应 `CodeIndex.fields` 决定，构建索引和 lookup 必须使用同一 key order。

### 9.5.1 Foreign Key Relation CodeModel

`linked_field` 功能要求 CodeModel 不只知道“有一个 FK”，还要知道生成 helper 与 cache 的最小必要信息。IR 构建步骤：

1. 从 `LinkedTableMeta.foreign_keys` 遍历当前 table 的 FK。
2. 应用 target-aware 过滤：FK 自身 `export/export_when`、source table、source fields、target table、target index、target index fields 都必须对当前 target 可见。
3. 构造 `CodeForeignKey`；`linked_field` 未声明时 `CodeLinkedField = None`。
4. 若 `linked_field` 存在，按 source field 类型判断 cardinality：
   - 单个 list source field -> `Many`；
   - `allow_empty = true` 或默认派生允许全空 -> `OptionalOne`；
   - 其他非 list FK -> `One`。
5. CodeModel 层只做目标语言无关的结构表达；helper 是否需要生成 row class、内部可变 cache、mutable 字段或 side table cache，由语言生成器决定。

命名与符号检查：

- IR 允许 `linked_field.name` 与目标语言关键字、helper 前缀或大小写策略暂时无关；语言生成器必须把它纳入 row/object API scope 的符号表。
- 同一 table 内，field getter、fragment getter、relation getter、row method、index helper、constructor helper 经命名转换后不能冲突。例如 TS 中 `default_class` 和 `defaultClass` 可能映射到同一个 `defaultClass`，必须报错。
- cache slot 是实现细节，但仍需避免与用户可见字段冲突。推荐使用保留前缀 / 后缀，例如 Rust `__cfg_link_default_class`、C++ `default_class_link_`、TS `#defaultClassLink` 或 private `defaultClassLink_`，并统一进入内部符号检查。

缓存 IR 不需要直接建模为字段，因为它不来自 Meta，也不参与 runtime data 读取、序列化、binary schema revision 或 artifact manifest。语言生成器可在渲染 row/object type 时根据 `CodeForeignKey.linked_field` 派生 cache slot。

推荐共享运行时 cache enum 语义：

```text
LinkCache
  Unresolved
  Empty
  RowIndex(usize)

LinkListCache
  Unresolved
  RowIndexes(Vec<usize>)
```

`OptionalOne` 使用 `LinkCache::Empty` 表达合法空关系；`One` 不应产生 Empty，若解析不到目标行应返回错误；`Many` 使用 `RowIndexes([])` 表达空列表，元素级缺失仍是错误。

### 9.5.2 Polymorphic Fragment CodeModel

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

## 9.6 命名策略与冲突检测

代码生成必须先解决命名合法性问题，不允许直接使用 Meta 名称。命名策略由 `targets.code.naming` 配置，并由每个语言生成器解释。

### 9.6.1 通用规则

1. Meta 名称可保持较宽松；代码名称必须符合目标语言标识符规则。
2. 非 ASCII 字符第一阶段不直接生成标识符，应转义或报错。推荐先报错，后续再支持 transliteration。
3. 非字母数字下划线字符按单词分隔处理。
4. 数字开头的名称加前缀，例如 `field_123` / `Table123`。
5. 保留字按 `keyword_conflict` 处理：追加 `_`、raw identifier 或报错。
6. 同一作用域内 code name 冲突必须报错，而不是静默覆盖。

### 9.6.1.1 命名配置解析

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

### 9.6.2 建议命名映射

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

### 9.6.3 冲突场景

必须检查：

- enum / struct / table row 类型重名；
- 同一 struct/table 内字段生成名重名；
- enum value 生成名重名；
- table repository 内 index function 重名；
- 生成文件路径重名；
- 生成名称与 runtime helper 类型重名，例如 `DataLoader`、`GameData`。

## 9.7 类型映射

### 9.7.1 Primitive 映射

| Meta type | Rust | C++ | TypeScript |
| --- | --- | --- | --- |
| `int` / `i64` | `i64` | `std::int64_t` | `number` |
| `float` / `f64` | `f64` | `double` | `number` |
| `bool` | `bool` | `bool` | `boolean` |
| `string` | `String` | `std::string` | `string` |
| `text` | `String` | `std::string` | `string` |
| `asset` | `String` 或 `AssetRef` | `std::string` 或 `AssetRef` | `string` 或 `AssetRef` |

第一阶段建议 `asset` 先映射为字符串，后续通过 target type mapping 支持自定义 `AssetRef`。

### 9.7.2 Enum 映射

- Rust：`enum ItemRarity { Common, Rare }`，建议 derive `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`。
- C++：`enum class ItemRarity { Common, Rare };`，生成 `FromString` / `ToString`。
- TypeScript：推荐生成字符串 union 或 enum。第一阶段建议字符串 union，更贴合 JSON：

```ts
export type ItemRarity = "common" | "rare";
```

如果使用 TS `enum`，需要考虑运行时代码体积和 JSON 字符串转换。

### 9.7.3 Struct 映射

- Rust：`struct Reward { ... }`。
- C++：`struct Reward { ... };`。
- TypeScript：`export interface Reward { ... }`。

当前 Data validation 对 struct value 仍是 reserved；Code Export 可以先生成类型声明，但运行时 JSON parse 支持应和 Data 阶段能力对齐。第一阶段建议：

- 如果 Meta 中存在 struct 类型字段，生成 struct 类型声明；
- table row 中的 struct 字段 parse / load 标记为 unsupported 或要求 runtime data 已经是对象结构；
- 在导出前由 Validation 阶段继续阻断 struct 数据值，避免生成代码承诺已可运行。

## 9.8 生成内容

### 9.8.1 Rust 生成内容

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

### 9.8.2 C++ 生成内容

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
- 复合 key 需要生成 `struct Key`、`operator==` 和 hash。
- enum class 需要生成 hash 或转换为底层整数用于 unordered_map。

C++ `linked_field` 生成规则：

- Row/Object struct 保留原始 FK 字段，并生成 `mutable LinkCache <name>_link_`，使 relation getter 可以是 `const` 方法。
- 必填单值 FK 可以生成 `const TargetRow& GetDefaultClass(const GameData& data) const`；为了与运行时错误模型一致，也可生成 error-return 版本，例如 `RuntimeDataResult<std::reference_wrapper<const TargetRow>>`。当前文档不强制异常或 Result 方案，但同一生成器内必须统一。
- optional FK 推荐返回 `const TargetRow*` 或 `std::optional<std::reference_wrapper<const TargetRow>>`；只有合法全空 key 返回空指针 / nullopt，非空 key lookup 失败必须报错。
- list FK 返回 `std::vector<const TargetRow*>` 或轻量 relation view；cache 保存 `std::vector<std::size_t>`，不保存 pointers。
- Table 生成 `void Link(const GameData& data) const` 或 error-return 版本；DataSet 生成 `LinkAll()`，按确定性顺序调用所有 table/object 的 Link。

### 9.8.3 TypeScript 生成内容

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

### 9.8.4 Table-level lazy load 生成设计（目标态）

当前实现状态：尚未实现。现有 `GameData::load` / `GameData::Load` / `GameData.load` 是 eager load，会立即加载所有当前 target 可见 table/object，并在返回后不再持有 loader。目标态是在保持 eager API 不破坏的前提下，新增 lazy facade；lazy facade 持有 loader 或 reader provider，按 table/object 首次访问加载并缓存 typed repository / object。

#### 9.8.4.1 语言无关状态机

每个 generated table/object member 在 lazy facade 中拥有独立状态：

```text
Unloaded
  -> Loading      # 可选，仅多线程或 async Promise coalescing 需要显式表达
  -> Loaded(T)
  -> Failed(E)    # 可选；默认可以不缓存失败以允许重试
```

规则：

1. `Unloaded` 首次访问时调用对应 table 的现有 generated `load(loader)` / `Load(loader)` / `load(loader)`，复用同一套 reader parser、schema revision 校验与 index 构建逻辑。
2. 成功后缓存 `Loaded(T)`；后续访问直接返回同一 table/object 实例。
3. 加载过程中若 reader/decode/schema/index 构建失败，不能留下半初始化 table。默认策略是不缓存失败并返回错误；如果语言使用 Promise 共享并发加载，可以让 rejected Promise 只在本轮请求内共享，下一次访问重新创建 Promise。
4. Lazy facade 保留 `RuntimeDataLoader`，但不保留单次 table reader cursor。`RuntimeDataLoader.open(table.data_file)` 每次 table 首次加载时创建短生命周期 reader；reader 在 table 解析结束后释放。若接入方需要常驻 mmap/package/decode cache，应由 loader 自己管理。
5. `preload_all` / `PreloadAll` / `preloadAll` 按 CodeModel table 顺序触发所有 table/object accessor。它用于 fail-fast 和预热；执行成功后 lazy facade 的所有 table 都处于 Loaded。
6. `link_all` 在 lazy facade 上应先 preload 所有当前 target 可见 table，再执行 relation cache 预链接；否则可能因只加载 source table 而遗漏 target table。

#### 9.8.4.2 Rust 生成方案

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

#### 9.8.4.3 C++ 生成方案

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

#### 9.8.4.4 TypeScript 生成方案

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

#### 9.8.4.5 与现有生成器模块的落地关系

- `repository.rs` 继续负责 table/object `load`、index 构建和 lazy accessor 渲染；lazy accessor 应复用现有 table `load` 函数，避免生成第二套 parser。
- `mod.rs` / facade 渲染增加 eager `GameData` 与 lazy `LazyGameData` 两套 facade。默认仍生成 eager；lazy 可无条件生成，也可由 codegen option 控制。为了 API 稳定，建议先无条件生成 lazy facade，除非代码体积成为问题。
- `symbols.rs` 需要把 lazy facade 类型名、`loadLazy` / `preloadAll` / async relation helper 等纳入冲突检测。
- 测试需要覆盖：eager load 不持有 loader；lazy 构造不调用 loader；首次访问某 table 只打开该 table；重复访问不重复打开；`preloadAll` 打开全部 table；lazy relation helper 会触发 target table 加载；TypeScript 并发访问共享同一个 Promise。

Reader 基础语义：

- `enter_field(name)` / `enterField(name)` 用于 Rust / TypeScript named-field layout 的底层 cursor API。C++ runtime contract 已收敛为 schema-driven 字段 API，不再暴露 `EnterField`。
- `enter_required_field(name)` / `EnterRequiredField(name)` / `enterRequiredField(name)` 是 generated parser 优先使用的 schema-driven 字段入口。Named reader 按字段名进入并执行 required/null 校验；binary reader 对 required 字段执行 no-op，因为字段由 generated schema 顺序隐式定位。
- `enter_optional_field(name)` / `EnterOptionalField(name)` / `enterOptionalField(name)` 返回字段是否 present。Named reader 默认按字段 missing/null 判断；binary reader 消费当前 record 的下一个 optional presence bit。present 后 generated parser 读取字段 payload，并通过 `exit_field` / `ExitField` / `exitField` 结束字段；binary reader 的 exit field 是 no-op。
- `read_enum_index(variants)` / `ReadEnumIndex(variants)` / `readEnumIndex(variants)` 统一 enum 读取。Named reader 将 string enum value 映射为 variants 下标；binary reader 读取 ordinal 并校验范围。Generated enum parser 只根据 index 构造目标语言 enum，不再分支判断 reader layout。
- `enter_index(i)` / `EnterIndex(i)` / `enterIndex(i)` 用于 payload array 或 positional record。越界是读取错误，应带上 index 上下文。进入成功后通过 `exit` / `ExitField` / `exitField` 返回上一层；C++ 使用 `ExitField` 作为统一回退 API。
- reader contract 采用可回退 cursor，而不是每次字段/数组访问返回新的 reader 对象。C++ adapter 因此避免 `std::make_unique` 子 reader 堆分配；TypeScript adapter 避免频繁临时对象；Rust adapter 避免 cloning `serde_json::Value` / MessagePack `Value` 子树。
- 分层命名约定：Meta / IR / generated helper 层使用 `List` 表示 schema 字段类型；runtime value / wire payload 中的 `Array` 只表示 JSON / MessagePack / binary 的物理容器 shape。`row_count` / `ReadRowCount()` / `readRowCount()` / `read_row_count()` 只表示 CollectionTable 顶层记录数；`list length` / `ReadListLength()` / `readListLength()` / `read_list_len()` 只表示 Meta list 字段元素数。二者必须是不同语义，互不 fallback。
- `len` / `length` 只对 payload array / object / map 有意义；对 scalar 调用应报错。Rust / TypeScript named reader 可在 `read_list_len` / `readListLength` 中复用底层 `len` / `length`，但 generated parser 不直接用通用长度 API 读取 list；C++ runtime contract 不暴露通用 `Size()`。兼容别名 `read_array_len` / `readArrayLength` / `ReadArrayLength` 仅保留给旧 adapter，语义委托给 list length，不得 fallback 到 row count。
- 最小 scalar API 包含 int、float/number、bool、string。`text`、`asset` 按 string 读取；enum 先按 string 读取，再由 generated parser 校验枚举值。
- 当 exporter 输出字符串 cell 时，推荐 generated parser helper 兼容 string-to-int、string-to-float、string-to-bool；reader 也可以直接在 `as_*` 中兼容字符串，但跨语言一致性以 generated parser 规则为准。
- missing 与 null 必须区分：missing 表示字段不存在；null 表示字段存在但值为空。required 字段 missing 或 null 都应由 generated parser 报错；optional field / optional fragment missing 时得到 `None` / `undefined`；null 是否允许作为 optional empty value 由生成器策略统一决定，第一阶段建议将 null 等价为 optional empty，但对 required 字段仍报错。
- 错误上下文由 generated parser 包装 table、file、row index、field path；reader 只需要报告底层类型/格式错误。

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

1. Repository 构造时一次性遍历 rows 构建所有索引。
2. unique index 如果发现重复，加载阶段返回错误。理论上导出前 validation 已经检查，但 generated runtime 仍应防御。
3. non-unique index 保存 row index 列表，保持原始数据顺序。
4. bool/int/float/string/enum 可作为 key。
5. float 作为索引 key 第一阶段允许但不推荐；可在 Meta validate 或 Codegen validate 中给 warning。
6. struct/text/asset 是否允许作为 key 需要策略。第一阶段建议：
   - `text` / `asset` 作为字符串 key 允许；
   - struct key 不支持。

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

为了避免重复查询，目标 repository 除了现有 `get_by_* -> row` API，还应生成内部 `index_of_by_* -> Option<usize>` / `IndexOfBy*` / `indexOfBy*`。该 API 可以是 private/package-private，只服务 relation cache；公开 API 仍返回 row 引用或对象。对于复合 key，内部 index lookup 使用与公开 lookup 相同的 key 构造逻辑。

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

## 9.11 抽象加载器设计

### 9.11.1 为什么需要抽象加载器

生成代码不应该假设数据来自本地文件系统。不同接入环境可能是：

- Rust 服务端：从磁盘或对象存储读取 JSON；
- C++ 游戏客户端：从包内资源、虚拟文件系统或引擎 AssetManager 读取；
- TypeScript：从 HTTP、Node fs、浏览器 bundle 或 IndexedDB 读取。

因此当前生成代码依赖一个通用 loader / reader 抽象：loader 给定逻辑 data file path，负责打开对应 JSON / MessagePack / 引擎资源并返回格式无关的 reader；生成代码再用 reader 读取 payload envelope `{ meta, rows }`、逐字段解析并返回 typed rows。loader 接口不包含按表生成的 typed loader 方法，避免用户每新增表或字段都要修改 loader 实现。

### 9.11.2 语言无关接口

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

### 9.11.3 数据格式适配

加载器实现只负责打开资源并提供通用 reader；生成代码负责解析业务对象：

```text
loader.open("items")
  -> read bytes/text
  -> decode to generic reader/value/cursor
  -> generated code reads `rows`
  -> generated code loops rows and reads fields `id`, `name`, ...
  -> generated code constructs Vec<ItemRow>
  -> ItemsTable::new / constructor
  -> indexes
```

这样让生成代码保持对文件系统和具体解析库无绑定，同时让用户 loader 保持稳定。代价是生成代码需要包含每种语言的 runtime parser helper（字段读取、类型转换、required/default/optional 处理、错误上下文），但这些 helper 可以由生成器稳定输出，不需要用户手写。

### 9.11.4 默认 loader

可以为每种语言生成一个可选的默认 loader / reader adapter 示例，但核心产物只依赖接口，不强制生成默认文件 loader：

| Language | 默认实现 |
| --- | --- |
| Rust | `FsJsonRuntimeDataLoader` + `JsonValueReader` 或 `FsMessagePackRuntimeDataLoader` + `MessagePackValueReader`，可选生成/复制 |
| C++ | `FileSystemDataFileLoader { root }` + JSON/MessagePack value reader，依赖可选生成 |
| TypeScript | `FetchDataFileLoader` / `NodeFsDataFileLoader` + JS value reader，二选一或都不默认生成 |

当前实现：

- Rust / C++ / TypeScript 当前已生成通用 loader / reader interface，并由生成代码完成 typed parsing。
- 集成测试 harness 直接复用 `runtime/rust/serde_json_reader/src/runtime_reader.rs` 与 `runtime/rust/serde_msgpack_reader/src/runtime_reader.rs`，确保同一份 generated Rust code 可通过不同格式 reader 读取等价数据；新增表或字段时无需在 harness 中维护自定义 reader，只需要更新断言覆盖代表性查询。
- 后续可按语言新增可选 adapter，而不是强制把文件系统或第三方解析库纳入生成代码。

默认 decoder 策略：

- 核心生成代码生成通用 reader interface，但不强制 runtime 依赖具体 JSON / MessagePack 包。
- JSON / MessagePack 字节到通用 reader/value 的解析放在业务侧 `RuntimeDataLoader` 实现或后续可选 adapter 中。
- 通用 reader/value 到 typed rows 的解析放在生成代码中。
- MessagePack decoder 不绑定具体 npm / C++ / Rust 包。

### 9.11.5 标准 runtime helper 工程结构

生成代码只声明 reader / loader contract，这解决了 Meta-specific 逻辑与业务资源系统耦合的问题，但实际游戏项目仍需要可运行的 JSON / MessagePack 等格式 reader。项目应提供标准 RuntimeDataReader adapter 源码/模板，作为 generated code 之外的 runtime support 层。

#### 9.11.5.1 分层边界

标准 adapter 分三层：

1. **Contract glue**：把某语言的 concrete value/cursor 类型适配成 generated code 期望的 `RuntimeDataReader` / `IRuntimeDataReader` / `RuntimeDataReader` 形状。
2. **Format reader**：把 JSON、MessagePack 等具体格式解码为 value/cursor，并实现 field/index/len/scalar/null 语义。
3. **Transport loader**：从文件系统、HTTP、bundle、引擎资源系统等位置读取 bytes/text，再交给 format reader。Transport 可选，业务项目可以只复用 format reader 并接入自己的资源系统。

核心 codegen 不 import adapter，也不把 adapter 的第三方依赖写进 generated code。adapter 是可选 runtime support：可以被业务项目直接依赖，也可以由 codegen option 拷贝到目标目录，还可以作为测试 harness 的基础实现。

#### 9.11.5.2 Reader contract 可执行规范

标准 adapter 不能只对齐接口形状，还必须对齐 reader contract 版本、value model、missing/null、scalar conversion 和错误上下文。当前 contract 版本定义为 `1.0`；后续所有 generated code 与 adapter 都应声明支持的 contract version。

建议 generated code 输出常量：

```rust
pub const CONFIG_FORGE_READER_CONTRACT_VERSION: &str = "1.0";
```

```ts
export const CONFIG_FORGE_READER_CONTRACT_VERSION = "1.0" as const;
```

```cpp
inline constexpr const char* CONFIG_FORGE_READER_CONTRACT_VERSION = "1.0";
```

adapter 也声明支持范围，例如 `SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0"`。当 codegen `emit = "copy"` 时，应把 adapter 支持范围写入复制文件头或 adapter manifest；当 `emit = "reference"` 时，应在生成 manifest / 诊断中说明所需 adapter 包或 CMake target 的 contract range。

Contract version bump 规则：

| 变化类型 | 版本级别 | 示例 |
| --- | --- | --- |
| reader / loader method signature 变更 | major | `field` 返回类型变化、loader open 参数变化。 |
| scalar conversion 或 missing/null 破坏性语义变更 | major | bool 不再接受 `"1"` / `"0"`。 |
| 新增可选方法或非破坏性能力 | minor | 新增 path context helper，但旧 adapter 仍可工作。 |
| 错误信息更清晰、内部性能优化 | patch | 增加 source path 到错误 message。 |

Contract 层定义的是 **ConfigForge Runtime Value**，不是 JSON Value。当前 Rust implementation 可以短期复用 `serde_json::Value` 降低实现成本，但 contract 应按下列 value model 描述：

| Runtime Value | JSON | MessagePack | Contract 语义 |
| --- | --- | --- | --- |
| object/map | object | map with string keys | `field(name)` 按字段名查找；非 string map key 暂不支持，应报错。 |
| array | array | array | `index(i)` 按 0-based 下标读取；越界报错。 |
| int / uint | number 或 numeric string | signed/unsigned integer 或 numeric string | generated parser 根据字段类型读取并检查范围；超出目标类型报错。 |
| float | finite number 或 numeric string | finite float / integer 或 numeric string | 非 finite 值必须报错。 |
| bool | bool 或 string | bool 或 string | string 仅接受大小写敏感的 `"true"` / `"false"` / `"1"` / `"0"`。 |
| string | string | string | 不做 number-to-string / bool-to-string 自动转换。 |
| null | null | nil | explicit null 在字段入口处按 missing 处理；scalar conversion 不接受 null。 |
| binary/ext | 不适用 | bin/ext | 当前 contract 不支持，adapter 必须明确报错，不能隐式转 string。 |

统一 scalar conversion policy：

| 读取路径 | 接受输入 | 禁止输入 | 额外要求 |
| --- | --- | --- | --- |
| int | integer value、可解析为整数的 string | float 小数、bool、object/array/null、溢出值 | 错误包含 path、expected `int`、actual type/value。 |
| float | finite number、可解析为 finite number 的 string | NaN、Infinity、bool、object/array/null | 错误包含 path、expected `float`、actual type/value。 |
| bool | bool、`"true"`、`"false"`、`"1"`、`"0"` | 大小写变体、number 1/0、其他 string、object/array/null | 保持与 ConfigForge validation/export 当前 bool string 语义一致。 |
| string | string | number/bool/object/array/null | 不自动 stringify。 |

missing/null 语义：

- `field(name)` 找不到字段或字段值为 null/nil 表示 missing：Rust/TS 返回 `None` / `undefined`，C++ 返回 `nullptr`。
- explicit null 不再依赖独立 reader null-check API；由 `EnterOptionalField` / `enter_optional_field` / `enterOptionalField` 的 bool 返回值统一表达 optional presence。
- `index(i)` 越界必须报错，不返回 null reader。
- `length()` / `len()` 对 payload array 返回元素数；对 object/map 返回字段数；对 scalar/null 报错。C++ adapter 不再暴露通用 `Size()`，generated collection 读取使用 `ReadRowCount()`；generated list 读取使用 `ReadListLength()` / `readListLength()` / `read_list_len()`。schema-aware binary reader 额外提供 list length / positional list element 读取能力，使生成代码能在 `bin` 中读取 length prefix 后顺序读取元素；named JSON / MessagePack reader 可在 list length API 内部复用 `length + index(i)`。

错误上下文要求：adapter 抛出的错误应包含 logical data path、reader path（field/index 链）、expected type、actual type/value。transport loader 还应包含 resolved resource path 或 URL。

#### 9.11.5.3 工程化目标与当前缺口

`runtime/` 的目标不是只服务 standard_project harness，而是成为游戏项目可直接引用的 runtime support 层。当前仓库已经有首批源码：

| 当前目录 | 定位 | 当前状态 |
| --- | --- | --- |
| `runtime/rust/serde_json_reader` | Rust JSON reader module | 已有基于 `serde_json` 的 `JsonValueReader` 与 fs loader。 |
| `runtime/rust/serde_msgpack_reader` | Rust MessagePack reader module | 已有基于 `rmp-serde` 的 `MessagePackValueReader` 与 fs loader，并被 Rust integration harness 使用。 |
| `runtime/typescript/json_reader` | TypeScript JSON reader + Node fs loader | 已有 JSON reader，使用结构化类型，不依赖 generated.ts。 |
| `runtime/typescript/messagepack_reader` | TypeScript MessagePack value reader | 已有基于 `@msgpack/msgpack` peer dependency 的 reader，复用 shared TypeScript core。 |
| `runtime/cpp/rapidjson_reader` | C++ RapidJSON reader adapter | 已有 header-only RapidJSON adapter，可用于测试与生产接入。 |
| `runtime/cpp/msgpackc_reader` | C++ msgpack-cxx reader adapter | 已有 header-only msgpack-cxx adapter，可用于 MessagePack runtime data 接入。 |

主要缺口：

1. 各语言 adapter 缺少统一 compatibility / test matrix 描述，无法明确哪些 adapter 是生产推荐、哪些只是 custom/example。
2. C++ stable interface header 仍待补齐，当前 C++ adapter 仍通过宏绑定 generated header 中的 `IRuntimeDataReader` / `IRuntimeDataLoader`。

因此 runtime loader 工程化按“两层稳定 + 多 adapter 可选”的方向推进：

- **稳定契约层**：定义 reader/loader contract 版本、payload shape、错误语义、scalar 兼容策略。生成代码只依赖该契约，不依赖具体 adapter。
- **可选 adapter 层**：每个语言/格式组合是独立工程单元，明确三方依赖、构建方式、测试入口和生产推荐等级。

#### 9.11.5.4 目录布局

推荐在仓库顶层维护 `runtime/` 作为标准 adapter 源码/模板目录：

```text
runtime/
  compliance/
    payloads/             # 跨语言共享 payload cases
    expected/             # scalar / error case expectations
  rust/
    serde_json_reader/
      src/runtime_reader.rs # JsonValueReader、JSON fs loader、generated trait glue
      Cargo.toml          # features: fs
    serde_msgpack_reader/
      src/runtime_reader.rs # MessagePackValueReader、MessagePack fs loader、generated trait glue
      Cargo.toml          # features: fs
  typescript/
    reader_core/
      src/                # shared reader helpers、error helpers
      package.json
    json_reader/
      src/                # JSON.parse value reader
      package.json
    messagepack_reader/
      src/                # @msgpack/msgpack based value reader
      package.json
    loaders/
      node-fs/            # Node fs transport loader
      fetch/              # browser/fetch transport loader
  cpp/
    rapidjson_reader/
      include/config_forge/runtime/rapidjson_reader/ # RapidJSON-backed header-only reader adapter
      CMakeLists.txt
    msgpackc_reader/
      include/config_forge/runtime/msgpackc_reader/ # msgpack-cxx-backed header-only reader adapter
      CMakeLists.txt
    loaders/
      filesystem/         # filesystem transport loader example
      CMakeLists.txt
    cmake/
      ConfigForgeRuntimeConfig.cmake.in
    CMakeLists.txt        # options、find_package、adapter targets、tests
```

`runtime/` 不是每次 export 的输出目录；它是 ConfigForge 源码仓库中维护的标准 runtime adapter 集合。未来如增加 `yaml`、`sqlite`、自定义二进制等 data format，应按同样结构补充对应语言子目录。

#### 9.11.5.5 官方支持矩阵、推荐等级与依赖策略

对 stable runtime data format，应覆盖官方 target language；对 experimental format 可以先只支持部分语言。Transport loader 不进入语言 × 格式矩阵，而是与 format reader 组合。每个 adapter 单元必须明确：

- 支持的 generated reader contract 版本；
- 支持的 data payload shape（当前至少 `{ meta, rows }` collection envelope 与 object table root object）；
- 依赖的三方库与启用方式；
- 是否提供 transport loader；
- 是否用于 CI / integration harness；
- 支持等级：`stable`、`preview`、`custom/example`；
- 形态：`standalone_package`、`copyable_template`、`generated_header_bound`。

首批矩阵：

| 语言 | 格式 | adapter 单元 | 支持等级 | 形态 | 依赖与启用方式 | 备注 |
| --- | --- | --- | --- | --- | --- | --- |
| Rust | JSON | `runtime/rust/serde_json_reader` | `stable` | `copyable_template` | `serde_json`；Cargo feature `json` / `fs` | 当前 trait 由 generated crate 声明，短期是生产可用模块；`JsonValueReader` 留在 JSON reader 内，stable runtime crate 落地后再升级为 standalone package。 |
| Rust | MessagePack | `runtime/rust/serde_msgpack_reader` | `stable` | `copyable_template` | `rmp-serde` + `serde_json::Value` 作为中间 value；Cargo feature `messagepack` / `fs` | `MessagePackValueReader` 留在 MessagePack reader 内，避免测试 harness 自定义 reader；中期可收敛到真正的 ConfigForge Runtime Value。 |
| TypeScript | JSON | `runtime/typescript/json_reader` | `stable` | `standalone_package` | 原生 `JSON.parse`；独立 npm package | 不依赖 generated.ts，Node fs/fetch loader 分离。 |
| TypeScript | MessagePack | `runtime/typescript/messagepack_reader` | `preview -> stable` | `standalone_package` | `@msgpack/msgpack` 作为 peer dependency；独立 npm package | 解码后的 unknown value 复用 core reader 语义。 |
| C++ | JSON | `runtime/cpp/rapidjson_reader` | `preview -> stable` | `generated_header_bound`，中期 `external_interface` | CMake option + `find_package(RapidJSON CONFIG)` / package manager | 用 RapidJSON 完整处理转义、Unicode、数字和 parse error；替代生产场景的 builtin parser。 |
| C++ | MessagePack | `runtime/cpp/msgpackc_reader` | `preview -> stable` | `generated_header_bound`，中期 `external_interface` | CMake option + `find_package(msgpack-cxx CONFIG)` / package manager / `FetchContent` 固定 `cpp-6.1.1` | 不手写二进制 parser；解码到 msgpack object/value 后适配 reader。 |

依赖策略：

- Rust 使用 Cargo feature 控制，如 `json`、`messagepack`、`fs`，默认 feature 尽量为空或只包含 core。
- TypeScript 使用拆包和 peer dependency：`@config-forge/runtime-reader-json` 不依赖 MessagePack；`@config-forge/runtime-reader-messagepack` peer-depend on `@msgpack/msgpack`，避免只用 JSON 的项目被迫安装 MessagePack 包。
- C++ 使用 CMake options / package manager `find_package` 控制第三方库启用，不 vendored 到 generated code。允许在 examples/CI 中用 `FetchContent` 或包管理器（vcpkg、Conan、系统包）下载三方库，但必须是显式 opt-in。
- adapter 版本应与 generated reader contract 版本兼容；当 reader interface 发生 breaking change，adapter 需要同步升级并在包名、feature、manifest 或文档中标明兼容范围。

#### 9.11.5.6 C++ CMake 工程化方案

C++ runtime helpers 采用 CMake target 管理，目标是让游戏项目能通过 `add_subdirectory(runtime/cpp)` 或安装后的 `find_package(ConfigForgeRuntime CONFIG)` 引入 adapter。

##### CMake options

```cmake
option(CONFIG_FORGE_RUNTIME_BUILD_TESTS "Build ConfigForge runtime reader tests" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER "Enable RapidJSON reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER "Enable msgpack-cxx MessagePack reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_FETCH_DEPS "Allow FetchContent for missing third-party deps" OFF)
```

普通业务构建默认不启用 `rapidjson_reader`，避免最容易链接的 target 被误用到生产。测试构建可以在 `CONFIG_FORGE_RUNTIME_BUILD_TESTS=ON` 时显式打开 `CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER=ON`，用于 RapidJSON reader smoke test。生产项目推荐显式启用 RapidJSON 或 MessagePack adapter：

```cmake
add_subdirectory(path/to/ConfigForge/runtime/cpp)

target_link_libraries(game_config_runtime PRIVATE
  config_forge_runtime::rapidjson_reader
  config_forge_runtime::filesystem_loader
)

target_include_directories(game_config_runtime PRIVATE
  ${CMAKE_CURRENT_BINARY_DIR}/Generated/Cpp
)
```

##### CMake targets

| target | 类型 | 说明 |
| --- | --- | --- |
| `config_forge_runtime::rapidjson_reader` | static/interface | RapidJSON based reader，生产推荐 JSON adapter。 |
| `config_forge_runtime::msgpackc_reader` | static/interface | msgpack-cxx based reader，生产推荐 MessagePack adapter。 |
| `config_forge_runtime::filesystem_loader` | static/interface | 文件系统 transport，可组合 JSON 或 MessagePack reader。 |

当前 C++ adapter 不提供单独的 `config_forge_runtime::core` target。短期以 generated header 中的 `IRuntimeDataReader` / `IRuntimeDataLoader` 作为唯一 C++ contract，adapter 通过宏绑定到该 generated interface，避免同时存在 generated interface 与 runtime core interface 两套 nominal virtual 类型。后续如拆出稳定 `config_forge_runtime.hpp`，再重新引入独立 stable interface target。

##### 三方依赖解析

- `rapidjson_reader` 优先 `find_package(RapidJSON CONFIG QUIET)`；找不到时，如果 `CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON`，可用 `FetchContent` 拉取 RapidJSON；否则给出明确 CMake error，提示通过 vcpkg/Conan/system package 安装。
- `msgpackc_reader` 优先 `find_package(msgpack-cxx CONFIG QUIET)` 或项目约定的 `find_package(msgpack CONFIG QUIET)`；找不到时，如果 `CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON`，可用 `FetchContent` 拉取 `msgpack/msgpack-c` 的 `cpp-6.1.1` tag；否则给出明确 CMake error。
- 三方库只链接到 adapter target，不出现在 generated code target 的 public API 中。业务可替换为引擎内置 JSON / MessagePack 实现，只要实现同一 `IRuntimeDataReader` contract。

##### generated header / stable interface 绑定

短期保持当前模式：C++ adapter include `generated.hpp`，并通过宏指定 generated interface 类型：

```cpp
#define CONFIG_FORGE_RUNTIME_CPP_READER my::game::config::IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER my::game::config::IRuntimeDataLoader
#include "config_forge/runtime/rapidjson_reader/runtime_loader.hpp"
```

Phase 1.5 优先评估拆出稳定 `config_forge_runtime.hpp`：generated C++ 提供 `interface = "inline" | "external"` 选项。默认 `inline` 保持当前 generated header 自带 interface；production adapter 推荐 `external`，由 generated code 和 adapter 同时 include 稳定 header。这样 adapter library 不再依赖某次 generated header，CMake target 可以更像普通第三方库被复用。

##### C++ reader 语义要求

生产级 C++ adapter 必须覆盖：

- JSON string escape / Unicode / number parse error 的完整库语义，不手写简化 parser；
- MessagePack map / array / integer / float / bool / string / nil；
- `field` 将 missing 与 explicit null 都映射为字段不存在，由 `EnterOptionalField` 的 bool 返回值表达 optional presence；
- reader cursor 必须 owning 或共享持有 root document/value，不能返回指向临时 DOM 节点的悬垂指针；RapidJSON cursor 通常应通过 `std::shared_ptr<Document>` 或等价 owner 维持 root 生命周期；
- field / index cursor 都必须满足同样 ownership 规则；
- `Index` 越界应抛出 `std::runtime_error` 或 ConfigForge runtime 定义的派生异常；C++ `ReadRowCount()` 对 array 返回元素数，对非 array 抛错；
- bool string 兼容 `"true"` / `"false"` / `"1"` / `"0"`，与 ConfigForge validation/export 语义保持一致；
- float 必须有限，非 finite 值报错；
- 错误信息包含 path / field / index 上下文，便于定位运行时数据问题。

##### filesystem loader 安全边界

标准 filesystem loader 只处理 logical data file path 到本地文件的安全映射：

- 禁止 absolute path；
- normalize / lexical clean 后路径必须仍在 data root 内，禁止 `..` path traversal；
- 错误信息同时包含 logical path 和 resolved path；
- 不对大小写敏感文件系统差异做隐式兼容；
- 不负责格式选择，格式 decode 由注入的 JSON / MessagePack decoder 或 adapter target 决定。

#### 9.11.5.7 TypeScript MessagePack 工程化方案

TypeScript adapter 使用结构化类型，不需要 import generated module。包结构推荐：

```text
runtime/typescript/
  core/                 # RuntimeDataReaderLike、ValueReader、shared scalar conversion
  json/                 # JsonValueReader、decodeJsonValue
  messagepack/          # MessagePackValueReader、decodeMessagePackValue
  loaders/node-fs/      # FsRuntimeDataLoader，可注入 decoder
  loaders/fetch/        # FetchRuntimeDataLoader，可注入 decoder
```

推荐 npm 包边界：

| package | 依赖 | 导出 |
| --- | --- | --- |
| `@config-forge/runtime-reader-core` | 无 | `RuntimeDataReaderLike`、shared `ValueReader`、scalar conversion helpers。 |
| `@config-forge/runtime-reader-json` | `core` | `JsonValueReader`、`decodeJson`。 |
| `@config-forge/runtime-reader-messagepack` | `core` + peer `@msgpack/msgpack` | `MessagePackValueReader`、`decodeMessagePack`。 |
| `@config-forge/runtime-loader-node-fs` | `core` | Node fs loader；decoder 由 JSON/MessagePack 包注入。 |

Loader / decoder 边界允许同步或异步，保持与 generated `RuntimeDataLoader.open` 的 `Promise<RuntimeDataReader> | RuntimeDataReader` 契约一致：

```ts
export type RuntimeReaderDecoder =
  (bytes: Uint8Array, path: string) => RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
```

`FetchRuntimeDataLoader` 必然 async；Node fs 推荐提供 async `FsRuntimeDataLoader`，同时可以提供测试用 `SyncFsRuntimeDataLoader`，但两者都应通过同一个 decoder 类型组合 JSON / MessagePack reader。

示例：

```ts
import { decode } from "@msgpack/msgpack";
import { MessagePackValueReader } from "@config-forge/runtime-reader-messagepack";
import { FsRuntimeDataLoader } from "@config-forge/runtime-loader-node-fs";
import { GameData } from "./generated";

const loader = new FsRuntimeDataLoader(dataDir, bytes => new MessagePackValueReader(decode(bytes)));
const data = await GameData.load(loader);
```

MessagePack adapter 的 value 语义应与 JSON adapter 对齐：object/map 支持 `field(name)`，array 支持 `index/length`，scalar 支持 string-to-number / string-to-bool 兼容策略，`null/undefined` 在字段入口处按 missing 处理。

#### 9.11.5.8 各语言 contract 约束

不同语言的 reader interface 绑定方式不同，adapter 形态也不同：

- **Rust**：当前 trait 由 generated crate 声明，外部 crate 无法不依赖用户 generated crate 就直接实现该 trait。短期方案是提供可复制 reader module，例如 `runtime/rust/serde_json_reader/src/runtime_reader.rs`，由 codegen option 或用户复制进 generated crate/业务 crate 后实现本地 trait；中期方案是拆出稳定 `config_forge_runtime` support crate，让 generated code 可配置为 `use config_forge_runtime::{RuntimeDataReader, RuntimeDataLoader}`，adapter crate 再直接实现这些 trait。
- **TypeScript**：接口是结构化类型，adapter package 只要导出同形状 class 即可直接传给 `GameData.load`。因此 TypeScript adapter 可以最早做成标准 npm workspace 包。
- **C++**：`IRuntimeDataReader` 是 nominal virtual interface，adapter 需要看到 generated header 或稳定 runtime interface header。短期方案是 adapter header/source 通过 include path 引用 generated header；中期方案是拆出稳定 `config_forge_runtime.hpp`，generated code 和 adapter 都依赖该 header，降低每次生成代码对 adapter 编译的耦合。

#### 9.11.5.9 与 codegen / 测试集成

Codegen 后续可增加 runtime adapter 相关选项，但默认仍不强制输出 adapter：

```toml
[targets.code.runtime_reader]
adapter = "rust/serde_msgpack_reader"   # 或 "typescript/messagepack_reader", "cpp/rapidjson_reader", "cpp/msgpackc_reader"
emit = "copy"                      # copy / reference / none
loader = "fs"                      # fs / fetch / none / custom
```

配置校验规则：

- `adapter` 的 language 必须与 code target language 匹配，例如 TypeScript target 不能选择 `cpp/rapidjson_reader`。
- `adapter` 的 format 不与 code task 绑定；同一份生成代码可以配合不同格式 adapter 使用。若未来显式配置 adapter，校验只需确认该 adapter 支持目标语言和 reader contract。
- `loader` 必须被所选语言和 adapter 支持，例如 `fetch` 只对提供 fetch transport 的 TypeScript/browser adapter 合法。
- `emit = "reference"` 时，必须能解析到对应 package/crate/CMake target；无法解析时应在 config 阶段报错或生成明确 diagnostic，不应延迟到编译/运行时。
- `emit = "copy"` 时，应记录 adapter source version 和 supported contract range；默认不覆盖用户已修改文件，除非显式配置 `overwrite = true` 或输出到全新 generated adapter 目录。

`emit` 语义：

| emit | 行为 | 适用场景 |
| --- | --- | --- |
| `none` | 只生成核心 generated code；不输出 adapter 文件、不输出依赖接入提示。 | 用户完全自研 loader，或引擎已有资源系统。 |
| `copy` | 复制 adapter source/template 到目标输出目录；同时复制必要 package/Cargo/CMake 片段或 manifest。复制内容必须带 adapter version / contract range 注释。 | 快速接入、示例工程、没有包管理体系的游戏项目。 |
| `reference` | 不复制源码；生成依赖接入提示或 manifest，例如 Rust path/crate dependency、TypeScript npm package、C++ CMake target。 | 正式项目通过包管理或 submodule 引入 runtime support。 |

`copy` 模式应避免覆盖用户手改：adapter 输出目录建议独立于 generated core code，例如 `Generated/Runtime/<adapter>/`；若目标文件已存在且 adapter version 不一致，默认报 diagnostic，提示用户清理或显式允许覆盖。

建议推进顺序：

1. 将 C++ JSON adapter 收敛为 `rapidjson_reader`，移除无依赖 builtin fallback，测试与生产都通过 RapidJSON 路径验证。
2. 补 TypeScript MessagePack adapter 与 shared TypeScript core，形成 TS JSON / MessagePack 对称结构。
3. 补 C++ 顶层 CMake 工程、`rapidjson_reader` stable adapter、`msgpackc_reader` stable adapter；在此之前优先评估 Phase 1.5 stable interface header。
4. 集成测试 harness 改为引用或复制这些 adapter，避免 harness 内长期维护独立 reader 实现。
5. 增加 adapter 自测：每个 adapter 至少用同一份 canonical payload 验证 field/index/len/scalar/null、missing vs null、string-to-number/bool、finite float、escaped string。
6. 再考虑是否提供 codegen option 自动复制 adapter 到目标工程输出目录。

#### 9.11.5.10 Adapter compliance suite 与 CI / 测试矩阵

canonical payload 应升级为正式 compliance suite，而不是只停留在矩阵描述。推荐目录：

```text
runtime/compliance/
  payloads/
    collection_envelope.json
    object_table.json
    scalar_compat.json
    missing_vs_null.json
    nested_struct.json
    fragment.json
    escaped_string.json
  expected/
    scalar_results.toml
    error_cases.toml
```

Rust / TypeScript / C++ adapter 都必须跑同一套 compliance cases。覆盖范围包括 missing vs null、bool `true/false/1/0`、numeric string、NaN/Infinity、int overflow、escaped string、array 越界、MessagePack 非 string map key、binary/ext unsupported 等场景。MessagePack cases 可以由同一份 logical fixture 生成 `.msgpack` payload，避免 JSON 与 MessagePack fixture 漂移。

标准 runtime helpers 的 CI 至少覆盖：

| 层级 | Rust | TypeScript | C++ |
| --- | --- | --- | --- |
| adapter unit | `cargo test --manifest-path runtime/rust/<adapter>/Cargo.toml` | `npm test` / `tsc -p` per package | `cmake -S runtime/cpp -B build -DCONFIG_FORGE_RUNTIME_BUILD_TESTS=ON ... && cmake --build build && ctest` |
| format matrix | JSON + MessagePack | JSON + MessagePack | RapidJSON JSON smoke、RapidJSON JSON、msgpack-c MessagePack |
| canonical payload | collection envelope、object table、nested struct、fragment object、missing/null、bool `1/0`、finite float、escaped string | 同左 | 同左 |
| integration harness | standard_project Rust MessagePack + JSON compile check | standard_project TypeScript JSON，后续增加 MessagePack fixture | standard_project C++ RapidJSON JSON smoke；后续增加 RapidJSON JSON 与 MessagePack compile/run |

CI 中三方依赖策略：

- 默认快速 CI 只跑无三方或已缓存依赖：Rust、TS JSON、C++ builtin。
- 完整 CI / nightly 跑 production adapter：TS MessagePack、C++ RapidJSON、C++ msgpack-c。
- 如果三方包不可用，测试应明确 skip 或 fail-fast 给出安装提示，不能静默退回 builtin parser 造成覆盖误判。

### 9.11.6 reader loader 落地状态

迁移已完成：

1. Rust / C++ / TypeScript 均生成 `RuntimeDataReader` trait/interface 和通用 `RuntimeDataLoader.open` / `IRuntimeDataLoader::Open`。
2. table row、object table、named struct、fragment 均由生成代码输出 reader parser helper。
3. table/object `load` 已从 `loader.load_<table>_rows(DATA_FILE)` 切换为 `loader.open(DATA_FILE)`，由生成代码读取 `rows` 或 object payload。
4. codegen 单测和 standard_project harness 已改为通用 reader loader，不再按表手写 typed rows / objects。

当前验收标准：生成代码不再出现按表的 loader 方法；新增表、字段、fragment 或 object table 字段时，用户侧 loader / reader 接口和 harness loader 实现不需要改动，只需要重新生成代码并更新业务断言。

## 9.12 Runtime data 文件约定

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

### 9.12.1 Exporter 写出实现方案

runtime data format 由 data export registry 选择，并通过 `RuntimeDataFile.format` 显式传给 codegen；文件名后缀只描述 artifact path，不作为 reader layout 的唯一来源。当前 codegen 层不再生成 `RuntimeDataFormat` 类型。目标态可用枚举描述如下：

```rust
pub enum TargetDataFormat {
    Json,
    MessagePack,
    Bin,
}
```

写出流程：

1. Project Config load 后先构造 `NormalizedTargetConfig`，完成 data task name 规范化，并拒绝 code task 的 `data` 字段。
2. `build_target` 消费当前 target 的 `Vec<NormalizedTargetDataConfig>`。没有 data task 时跳过 runtime data 导出，进入 code-only 流程。
3. 按 `NormalizedTargetDataConfig.input` 分组。每个不同 input 构造一次 `DataLoadPlan`，使用 `DataLoader::new(data_root)` 加载；Meta 中 source path 继续由 `data_root.join(source.path)` 解析。
4. 对每个不同 input 的 DataSet 执行 normalize、target validation，并派生 DataSet-dependent ExportView；同 input 的 JSON / MessagePack / Bin 等多个 data task 复用该结果。不同 input 即使属于同一 target，也不能复用另一个 input 的 DataSet / validation view / concrete ExportView。
5. 每个 data task 根据自身 `format` 从 `DataFormatRegistry` 选择 exporter，根据自身 `output` 写出一套 runtime data artifact。
6. `Json` 使用 `serde_json::to_vec_pretty` 写出 `<table>.json`。
7. `MessagePack` 使用 `rmp_serde::to_vec_named` 或等价 named-field 编码写出 `<table>.msgpack`。
8. 每个 data task 返回独立的 `RuntimeConfigForgeResult { data_name, input, output, format, files }`，其中 `RuntimeDataFile { table_name, path, format }` 必须使用真实写出的文件名和规范格式名。
9. `write_generated_code` 不选择 data task，统一向 `CodeExportInput` 传入空 `runtime_data_files`，让 CodeModel 使用逻辑表名。
10. Artifact path 规划必须跨所有 data task、code task 和未来 special exporter 统一检查，不能只在 codegen 内检查 generated code 的重复路径。

依赖建议：

```toml
rmp-serde = "1"
```

如果后续引入 manifest，manifest 中也应以 `(target_name, data_name, table_name)` 作为 runtime data artifact key，并记录每个 artifact 的 `format`、相对 data artifact root 的 `path`、最终写入 path 和 `table_name`。

### 9.12.2 Codegen 读取实现方案

CodeModel 构建时使用逻辑 table name 作为 runtime load path：

- 忽略 runtime data artifact 的真实 path，避免 codegen 绑定具体格式。
- 每张表默认使用 `<table>` 作为逻辑 data file path。
- 每个 `CodeTable` 保留 `data_file`，语言生成器用它生成常量。
- 生成代码不根据 data format 切换 parser；reader contract 负责把 named-field 或 packed layout 映射到同一组 `EnterRequiredField` / `EnterOptionalField` / `ReadRowCount` / `EnterIndex` / scalar API。
- 多个 data format 的选择不进入 CodeModel 决策。

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

## 9.13 生成器接口

```rust
pub trait CodeGenerator: Send + Sync {
    fn language(&self) -> &'static str;
    fn language_aliases(&self) -> &'static [&'static str] { &[] }
    fn supports_language(&self, language: &str) -> bool;
    fn default_module_name(&self) -> &'static str;
    fn default_naming(&self) -> NamingConfig;
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
CodegenTaskConfig.language
  -> find_generator(registry, language)
  -> CodeExportTarget::from_task(task, generator)
  -> CodeExportInput
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

扩展新语言的步骤：

1. 新增 `src/codegen/<language>/mod.rs`，在模块内定义 `LANGUAGE`、语言专属 options、实现 `CodeGenerator`，返回规范语言名、可选 alias、默认命名规则和生成逻辑。
2. 新增 `src/codegen/<language>/tests.rs`，覆盖该语言的 loader/reader contract、row/object/parser、index 和命名冲突行为。
3. 如有语言专属配置，新增 option struct，并通过 `CodegenOptions::language_options<T>("<language>")` 解析。
4. 将 generator 加入 `builtin_generators()`；若未来使用外部插件 registry，则由调用方传入扩展后的 registry。
5. 不修改 `CodeModel`、`src/build/` 的分发逻辑，也不新增中心化语言 enum。

## 9.14 Runtime data 与加载接口测试计划

数据格式与生成代码加载接口相关测试应覆盖：

1. `TargetDataFormat` / registry 解析：`json`、`messagepack`、可选别名 `msgpack`，以及未知格式报错。
2. Runtime data 写出：
   - JSON target 生成 `<table>.json`，内容可用 `serde_json` 读取；
   - MessagePack target 生成 `<table>.msgpack`，内容可用 `rmp-serde` 读取为同一 payload；
   - `RuntimeDataFile.path` 与实际文件后缀一致。
3. Project Config schema：接受 `[[targets.data]]`；旧 `[targets.data]` table 形态不作为兼容目标；空 data task name、多 data task 缺 name、非法字符、重复 name 都应报错；单 data task 省略 name 时规范化为 `default`。
4. 多 data task：同一 target 同时配置 JSON 与 MessagePack，两个 output 都生成对应文件，且同 input 的数据加载/normalize/validation 可以复用。
5. 多 input：不同 input 分别加载、normalize、validation 并派生 DataSet-dependent ExportView，不能复用另一个 input 的 ExportView。
6. code task 格式无关：`CodegenTaskConfig.data` 设置时必须报错；target 有多个 data task 时，codegen 仍只生成一份逻辑表名代码。
7. 路径语义：入口 `ConfigForge.toml` 中 data input/output/code output 相对 project root；import 片段中的相对路径相对片段目录；Meta source path 相对 data input；`RuntimeDataFile.path` 相对 data artifact root。
8. artifact 冲突：data task 之间、data task 与 code task 之间、多个 code task 之间的输出路径冲突都应报错。执行冲突检查前应做不依赖文件存在性的 lexical path normalization，至少消除 `.` / `..` 造成的字符串差异；对已存在路径可视需要再做 canonicalize，但不能要求输出目录预先存在。
9. CodeModel 默认文件名：始终使用逻辑表名，不包含 `.json` / `.msgpack` / `.bin` 后缀。
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
13. CLI integration：分别构建 JSON target、MessagePack target，以及同 target 多 data + 多 code 的标准工程，验证 runtime data 文件和 generated code 文件同时生成，并检查代码中引用逻辑表名且不随 data format 改变。

## 9.15 文件组织策略

### 9.15.1 多文件模式，推荐默认

优点：便于阅读和差异比较。

```text
types.*
loader.*
tables.*
index.*
```

### 9.15.2 单文件模式

适合小项目或直接拷贝接入：

```text
config_forge_generated.rs
config_forge_generated.hpp
config_forge_generated.ts
```

第一阶段可以只实现单文件模式以降低复杂度，但 IR 和 GeneratedFile 应支持多文件。

## 9.16 错误与诊断

第一阶段可以继续使用 `ConfigForgeError::Validation(String)`，但建议新增 codegen diagnostic 模型：

```rust
pub struct CodegenDiagnostic {
    pub code: CodegenDiagnosticCode,
    pub message: String,
    pub language: Option<String>,
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

## 9.17 测试计划

### 9.17.1 CodeModel 单元测试

1. enum / struct / table 都能进入 `CodeModel`。
2. primitive / enum / struct 字段映射为正确 `CodeTypeRef`。
3. table / field / index 的 `export = false` 与 `export_when` 按 `target_tags` 过滤；primary index 引用当前 target 不导出的字段时报错，非 primary index 引用不导出字段时跳过。
4. primary index、unique index、non-unique index 都能生成 `CodeIndex`。
5. 命名 sanitize 后稳定且可预测。
6. 命名冲突能报错。
7. polymorphic table 的 common fields 与 fragments 能进入 `CodeModel`，fragment fields 按 target tags 过滤。
8. 同名 fragment 出现在不同 table 时，生成类型名应包含 table scope，避免冲突。
9. fragment field 不能作为 index key；Meta/CodeModel 层应给出明确错误或在 Meta validation 阶段提前拒绝。

### 9.17.2 语言生成器 snapshot 测试

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

### 9.17.3 生成代码编译与运行测试

生成代码测试不应只停留在“文件存在”和字符串片段断言。集成测试需要把 runtime data artifact 与 generated code artifact 串起来，验证生成代码能被目标语言工具链接受，并能通过 generated loader interface 读取真实导出的数据。

当前测试 harness 只实现一次 `open + RuntimeDataReader` / `IRuntimeDataReader`；typed rows / typed object 映射必须由生成代码完成，harness 不再按表手写 generated rows。

通用流程：

1. 在临时目录或被 `.gitignore` 忽略的 fixture `out/` 目录中执行 export，生成 runtime data 与对应语言代码。
2. 为每种目标语言创建测试 harness。harness 实现通用 `RuntimeDataLoader` / `IRuntimeDataLoader` 与 reader 接口，并把 JSON / MessagePack payload 暴露为通用 reader。
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

### 9.17.4 分语言 generated-code runtime test 方案

标准工程当前按 target 拆分：client 生成 JSON + TypeScript/C++，server 生成 MessagePack + Rust。因此 generated-code runtime test 可以复用标准工程配置，分别验证三种语言。

| 语言 | 输入产物 | 工具链 | Harness 做法 | 必测断言 |
| --- | --- | --- | --- | --- |
| Rust | `out/server_code/rust/generated.rs` + `out/server_data/*.msgpack` | `cargo test` | 实现通用 `RuntimeDataLoader` / reader，用 `rmp-serde` 读取真实 MessagePack payload，并把 payload 暴露给 generated parser；不手写 generated rows / objects 映射。 | `GameData::load` 成功；`items.get_by_id(id)` 命中；`class_levels.get_by_class_level(class_id, level)` 命中联合索引；`game_rules` object 字段可访问。 |
| TypeScript | `out/client_code/typescript/generated.ts` + `out/client_data/*.json` | `tsc` + `node` | 实现通用 `RuntimeDataLoader` / reader，用 Node `fs` 读取真实 JSON payload，并由 generated parser 映射 row interface/object。 | `GameData.load` 成功；`items.getById(id)` 命中；`classLevels.getByClassLevel(classId, level)` 命中联合索引；object table 可访问。 |
| C++ | `out/client_code/cpp/generated.hpp` / `generated.cpp` + `out/client_data/*.json` | `c++ -std=c++17` | 实现通用 `IRuntimeDataLoader` / reader，使用轻量 JSON reader 暴露 payload；不能按表手写 typed rows。 | `GameData::Load` 成功；`Items().GetById(id)` 命中；`ClassLevels().GetByClassLevel(class_id, level)` 命中联合索引；object table 可访问。 |

归档后的 harness 不应直接依赖仓库中某一次已生成的 `out/` 文件路径。推荐做法是：集成测试先执行 export，再把对应语言的 generated code 复制到临时 harness 工作目录（或通过 include path/link path 引用），最后执行目标语言测试命令。这样能确保测试的是“当前实现刚生成的代码和数据”，而不是历史残留 artifact。

工具链缺失时的处理原则：

- CI 环境应安装 Rust/Cargo、C++17 compiler、Node.js 和 TypeScript，使三类 runtime test 都能执行。
- 本地开发环境如果缺少 C++ compiler、Node 或 `tsc`，对应测试可以显式 skip，但测试输出必须说明 skip 原因，不能静默通过。
- Rust 生成代码测试不应 skip，因为项目本身依赖 Rust toolchain。

### 9.17.5 CLI 集成测试

在 `tests/fixtures/standard_project/` 上运行：

1. export 生成 runtime data。
2. export 生成 Rust/C++/TS code。
3. 检查输出文件存在。
4. 检查关键 API 名称存在。
5. 运行分语言 generated-code runtime test，确认生成代码能通过 loader 加载真实导出数据并执行代表性查询。

集成测试写出的 artifact 应位于 fixture-local `out/` 目录或临时目录。fixture-local `out/` 必须通过 `.gitignore` 忽略；新增 fixture 如果也写出导出结果，应遵循 `tests/fixtures/**/out/` 约定，避免误提交生成产物。

## 9.18 第一阶段落地状态

当前第一阶段主体已落地，历史分阶段计划更新为实现状态：

### Phase 1：CodeModel 与 Rust 生成器

状态：已完成。

1. 已新增 `src/codegen/`。
2. 已实现 CodeModel IR。
3. 已实现 naming / type mapping 和生成符号冲突检测。
4. 已移除临时 Rust stub 路径，改由 `CodeGeneratorRegistry` 分发。
5. 已生成 Rust enum / struct / row / table / index helper / `RuntimeDataReader` / `RuntimeDataLoader::open` 和 generated parser helper。
6. 已增加单元测试和标准 fixture 回归。

### Phase 2：TypeScript 生成器

状态：已完成核心单文件生成。

1. 当前输出 `generated.ts`，包含 type / table / loader / GameData facade。
2. 已支持 async `RuntimeDataLoader.open` 和 generated reader parser。
3. 已支持 Map-based indexes，并为 unique index 增加重复 key 防御。
4. 当前测试以语义 contains 断言为主，snapshot/golden file 可后续补充。

### Phase 3：C++ 生成器

状态：已完成核心 header-only 生成。

1. 当前输出 `generated.hpp`。
2. 已生成 `IRuntimeDataReader` / `IRuntimeDataLoader::Open` 和 generated reader parser。
3. 已生成 enum、row、repository 和 index helper。
4. 后续可再拆 `.hpp` / `.cpp` 或补充更完整 enum conversion。

### Phase 4：Artifact Writer

状态：已完成当前阶段。

1. runtime data 与 generated code 都以 `GeneratedFile`/artifact 形态参与写出。
2. 已做输出路径冲突检查。
3. CLI 支持 dry-run、direct write 和 `temp-commit` 写入模式；`--dry-run` 优先于 `--write-mode`。
4. summary manifest 可作为 special exporter 产物独立实现。

## 9.19 第一阶段完成标准

完成时应满足：

- Rust、C++、TypeScript 三种语言至少能生成类型和表访问代码。（已满足）
- 所有 enum、struct、table row type 都有生成结果。（已满足；struct 数据值 validation 仍 reserved）
- 当前 target 可见的 table index 都有对应辅助查找函数。（已满足）
- 每种语言都有 Meta-agnostic 通用 reader loader 接口。（已满足）
- 生成代码不直接依赖 ConfigForge CLI 内部类型。
- 命名冲突和非法标识符不会静默生成错误代码。
- exporter 不再使用临时 Rust stub。
- `cargo fmt`、`cargo clippy --all-targets --all-features`、`cargo test` 通过。
- 标准工程 fixture 覆盖 runtime data + code export 输出。
