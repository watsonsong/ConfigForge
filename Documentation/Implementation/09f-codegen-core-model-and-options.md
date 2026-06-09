# 09f. Codegen Core Model and Options

本文从 code artifact toolkit 详细设计中拆出，专门描述核心输入输出模型、`CodeExportTarget` / `CodeExportInput`、测试专用 `GeneratedFile`、CodeModel IR、命名策略、类型映射和通用 writer。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — code artifact toolkit 总览和专题索引。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、runtime loading 和 generated API 语义。
- [`01-module-outline.md`](01-module-outline.md) — Rust 模块边界与依赖方向。

## 9.4 核心输入输出模型

### 9.4.1 语言标识与生成器注册表

目标语言使用字符串标识，不使用中心化枚举。这样新增一种语言时不需要修改 `CodeExportLanguage` 一类的核心 enum，也不需要在 build 层增加新的 match 分支。

设计边界：`CodeGenerator` 表达的是 code artifact 能力（语言标识、命名、格式化、文件头、关键字冲突、CodeWriter / CodeModel 渲染），而不是 “只依赖 Meta” 这一输入依赖分类。当前内置 Rust/C++/C#/Java/TypeScript 等 format-agnostic codegen 的 requirements 都是 MetaOnly；但这只是这些 generator 的调度属性。Lua table 这类 data-backed code-like artifact 证明输入依赖不应成为继承体系：它需要 Meta + Data，却仍应复用 code artifact options / writer helper。长期应把当前 `CodegenOptions` 中可复用的部分沉淀为 `CodeArtifactOptions` facet，供普通 language codegen 和 mixed generator 共同使用。

```rust
pub(crate) trait CodeGenerator: Send + Sync {
    fn language(&self) -> &'static str;
    fn default_module_name(&self) -> &'static str;
    fn default_naming(&self) -> NamingConfig;
    #[cfg(test)]
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>>;
    fn generate_to_sink(&self, input: &CodeExportInput<'_>, sink: &ArtifactSink) -> Result<()>;
}
```

注册表职责：

- `CodeGenerator::language()` 返回规范语言名，例如 `rust`、`cpp`、`typescript`。
- alias 是 build-facing `ArtifactGenerator` descriptor 的职责，例如 C++ 支持 `c++`，TypeScript 支持 `ts`；`CodeGenerator` 不再承载 alias / requirements。
- 每个语言模块维护自己的 `LANGUAGE` 常量，例如 `src/generator/rust/mod.rs` 中的 `LANGUAGE = "rust"`。核心 `src/code_artifact/mod.rs` 不定义 `pub mod languages`，避免新增语言时回到核心模块增加常量。
- `CodeExportTarget::from_task` 接收已查找到的 `&dyn CodeGenerator`，由 generator 提供默认模块名和默认命名规则。
- `src/build/` 只通过 `GeneratorRegistry` 查找 build-facing artifact generator；找不到时报告“未注册生成器”，不感知具体语言集合。
- 内置语言的集中注册点是 `GeneratorRegistry::with_builtin_generators()`。当前 `CodeGenerator` 是 crate 内部 code artifact renderer facet，不作为稳定外部 generator plugin API 暴露；外部用户通过 Project Config 选择内置语言，而不是实现该 trait。

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
| `options` | 从当前 `targets.exports` 任务解析出的通用和语言专属选项，包括文件头、命名策略、关键字冲突策略、语言扩展参数等。 |

`CodeExportTarget` 不应直接由 CLI 硬编码构造；正式流程应来自 Project Config 的 `[[targets.exports]] type = "rust" / "cpp" / "typescript"`。生成代码必须保持格式无关：CodeModel 只依赖 Meta 和 target tags，table data file 常量使用逻辑表名（例如 `items`），实际解析 JSON / MessagePack / bin 由运行时传入的 loader / reader 实现决定。

### 9.4.2.1 Data export format 与 codegen 关系

解析规则：

- Project Config 中 `[[targets.exports]] type = "json"` 选择 JSON exporter。
- Project Config 中 `[[targets.exports]] type = "messagepack"` 选择 MessagePack exporter。
- Project Config 中 `[[targets.exports]] type = "bin"` 选择私有紧密二进制 exporter。
- Project Config 中 `[[targets.exports]] type = "gettext_pot"` / `"asset_manifest"` 等聚合格式仍属于 MetaAndData export task，但不产生 generated runtime loader 直接消费的 table data。Asset Manifest 已实现为聚合 exporter；CSV 输出通过 `type = "asset_manifest"` + `format = "csv"` 表达。
- Project Config 中 `[[targets.exports]] type = "lua_table"` 属于 data-backed mixed generator：它会直接生成 Lua 数据+访问代码合一产物，必须依赖当前 target 的 resolved data；不要把它接入当前只服务 format-agnostic language backend 的 `CodeGenerator` trait。长期应让它通过 code artifact facet 复用文件头、命名、格式化等通用代码生成选项。
- 可兼容解析 `"msgpack"` 为 MessagePack，但错误信息、文档、生成代码和 manifest 中统一使用规范名 `messagepack`。
- Project Config 使用 `[[targets.exports]]` 数组表表达所有导出任务；format-agnostic codegen 只关心 Meta 和 target tags，不绑定任意 MetaAndData export task。更抽象的 artifact generator 模型会通过 requirements 声明是否需要 Data，而不是把 `CodeGenerator` 固定继承到 MetaOnly 分支。

Project Config 层模型（`TargetConfig` 是 project-level 聚合结构；build 会按 exporter contract 把统一 export task 适配为 exporter / codegen 模块内部配置）：

```rust
pub struct TargetConfig {
    pub name: String,
    pub tags: Vec<String>,
    pub exports: Vec<TargetExportConfig>,
}

pub struct TargetExportConfig {
    pub name: Option<String>,
    pub type_id: String,
    pub output: PathBuf,
    pub options: BTreeMap<String, toml::Value>,
}
```

`TargetExportConfig.name` 在 raw config 中允许 `Option<String>`，仅用于支持“单 export task 可省略名称”的用户体验。parse 后应立即构造 normalized model；后续 validate / build / manifest / codegen 不再直接消费 raw target：

```rust
pub struct NormalizedTargetConfig {
    pub name: String,
    pub tags: Vec<String>,
    pub exports: Vec<NormalizedExportTask>,
}

pub struct NormalizedExportTask {
    pub name: String,
    pub type_id: String,
    pub requirements: GeneratorRequirements,
    pub output: PathBuf,
    pub options: BTreeMap<String, toml::Value>,
}
```

归一化与校验规则：

- target 内 export task 为 0 个时合法但不会生成产物。
- target 内 export task 为 1 个时，`name` 可省略；省略时规范化名称固定为 `default`。
- target 内 export task 多于 1 个时，省略 `name` 时默认使用 `type`，显式或默认名称必须在 target 内唯一。
- export task name 大小写敏感；只允许 ASCII 字母、数字、`_`、`-`，且必须以字母或数字开头和结尾。
- normalized model 中 `NormalizedExportTask.name` 使用非空 `String`；runtime export branch output 也以 export task name 作为稳定校验键。
- 当前 format-agnostic codegen export task 的 `data` 必须为空；若配置 `data = "..."`，归一化阶段报错。生成代码格式无关，不绑定任意 MetaAndData export task。
- 无论 target 有 0 个、1 个还是多个 MetaAndData export task，当前 format-agnostic codegen task 都只接收 Meta、target tags 和语言配置。长期如果出现 data-backed code-like generator，应作为普通 artifact generator 声明 `needs_data = true`，同时复用 code artifact facet，而不是复用当前 `CodeExportInput` 的 MetaOnly 假设。
- `project.input` 与 `targets.exports.output` 相对路径在加载 Project Config 片段时按片段文件所在目录归一化；入口 `ConfigForge.toml` 的相对路径等价于 project root 相对路径。
- DataLoader 的 `data_root` 使用 `project.input`，Meta `source` / `sources[].path` 再相对该 data root join。

文件规划规则：

| Format | 文件后缀 | exporter 写出方式 | 生成代码读取方式 |
| --- | --- | --- | --- |
| `Json` | `.json` | `serde_json::to_writer_pretty` 通过 shared sink 写出 payload envelope。 | `RuntimeDataLoader` 根据 `DATA_FILE` / `DataFile` / `dataFile` 打开 JSON payload 为通用 reader；生成代码读取 rows 并构造 typed rows。 |
| `MessagePack` | `.msgpack` | `rmp-serde` writer API 通过 shared sink 写出同一 payload envelope 的 MessagePack bytes。 | `RuntimeDataLoader` 根据 `DATA_FILE` / `DataFile` / `dataFile` 打开 MessagePack payload 为通用 reader；生成代码读取 rows 并构造 typed rows。 |
| `Bin` | `.bin` | 写入 `magic = "CFGB"`、format version、header size、`u64 schema_revision` 后，按 target-aware schema 顺序紧密排列 record，不保存 tag/name。 | generated code 携带每张表的 expected revision，binary reader 校验 magic/version/header/revision 后按 positional layout 顺序读取。 |

`RuntimeDataFile` 中保存的 `path` 必须是已带正确后缀的相对文件名，例如 `items.json`、`items.msgpack` 或 `items.bin`。该路径相对当前 data artifact root / runtime loader root，不相对 code output 或 project root。CodeModel 不使用这些真实 artifact path 生成加载常量；每张表统一使用逻辑表名。`bin` 的 schema revision 由 CodeModel 基于 Meta 和 target tags 计算，用于运行时 reader 暴露 revision 时进行可选校验。

当一个 target 同时导出多套 runtime data export task 时，Build DAG 只校验每个 task output 的 `export_name` 与声明顺序匹配，不再向 codegen 聚合 runtime metadata。MetaOnly codegen export task 在构造每个 `CodeExportInput` 时传入空 `runtime_data_files`。同一 table 同时出现 `items.json`、`items.msgpack` 和 `items.bin` 时也不会影响 CodeModel；格式选择由运行时 loader / reader 完成。未来 manifest 可以记录所有 runtime data export task 的真实 artifact path，但不进入 codegen 决策。

### 9.4.2.2 `CodeArtifactOptions` / `CodegenOptions`

```rust
pub struct CodeArtifactOptions {
    pub file_header: Option<String>,
    pub keyword_conflict: KeywordConflictPolicy,
    pub i18n: I18nOptions,
    pub naming: NamingConfig,
    pub formatting: CodeWriterConfig,
    pub language_options: HashMap<String, toml::Value>,
}

pub type CodegenOptions = CodeArtifactOptions;

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

当前 `CodeArtifactOptions` 是 code-like artifact 复用的基础配置结构；`CodegenOptions` 作为 type alias 继续保持 language codegen API 兼容。它只表达代码文本生成能力，不表达 MetaOnly / MetaAndData 调度分类：

- `file_header` 与 `{{project.*}}` / `{{datetime}}` 模板渲染；
- `keyword_conflict` 与标识符冲突策略；
- `i18n` helper 调用形态；
- `naming` 与 `formatting`；
- generator-specific option bucket，例如 `[targets.exports.lua_table]`、`[targets.exports.typescript]` 或其他 code-like generator 的专属子表。

这样 Lua 这类 requirements 包含 Data 的 code-like generator 可以复用相同配置解析和 writer 行为，而不需要复制一套与 `CodegenOptions` 平行的基础选项。当前 `LuaTableExporter` 位于 `src/generator/lua/`，作为 direct data-backed mixed `ArtifactGenerator` 注册；`CodeExportTarget.language`、`CodeGenerator::language()`、CodeModel 构造等仍属于 format-agnostic language codegen renderer facet，不应强加给 schema doc、runtime table data exporter、asset manifest 或 validation report。

配置来源：

```toml
[[targets.exports]]
type = "rust"
output = "Generated/Rust"
module_name = "game_config"
file_header = "// {{project.name}}\n// Generated at {{datetime}}"
keyword_conflict = "suffix_underscore"

[targets.exports.naming]
type = "pascal_case"
enum_value = "pascal_case"
field = "snake_case"
function = "snake_case"
constant = "screaming_snake_case"

[targets.exports.formatting]
indent_style = "spaces"
indent_width = 4
trailing_newline = true
trailing_blank_line = false

[targets.exports.rust]
derive_serde = true
```

设计要求：

- `CodegenOptions` 拆成通用配置和语言特定配置两层：`file_header`、`keyword_conflict`、`naming` 为通用层，`language_options` 保存 TOML 中按语言名展开的扩展配置。
- `formatting` 是代码生成的跨语言通用格式配置，用于驱动共享 `CodeWriter`：`indent_style = "spaces" | "tab"` 控制缩进 token，`indent_width` 控制 spaces 模式下每级缩进宽度，`trailing_newline` 控制文件末尾是否至少保留一个换行，`trailing_blank_line` 控制文件结束是否保留一个空行（即内容以两个换行结束）。
- 每个语言生成器通过 `CodeGenerator::default_formatting()` 声明语言默认格式：Rust / C++ 默认为 4 空格，TypeScript 默认为 2 空格；TOML 中 `[targets.exports.formatting]` 只覆盖显式声明的字段。
- 语言配置不使用 tagged enum；`CodegenTaskConfig` 通过 `#[serde(flatten)]` 把 `[rust]`、`[cpp]`、`[typescript]` 等表保存在 `HashMap<String, toml::Value>` 中。
- 每个语言生成器只能读取自己的扩展配置，例如 `RustCodegenOptions` 定义在 `src/generator/rust/mod.rs`，并由 Rust generator 通过 `CodegenOptions::language_options::<RustCodegenOptions>(LANGUAGE)` 解析。
- 新增语言时应新增 generator、语言扩展 option struct 和该语言自己的解析逻辑；不修改 IR，不增加中心化 `LanguageCodegenOptions` enum，也不在核心模块增加语言专属 helper。
- 如果配置中出现当前语言不支持的 `CaseStyle` 或 `KeywordConflictPolicy`，应在 code artifact validate 阶段报错。

### 9.4.2.3 `CodeWriter` / `StreamingCodeWriter`

代码生成器应逐步从直接 `String::push_str(&format!(...))` 迁移到共享 writer。`CodeWriter` 和 `StreamingCodeWriter` 位于 `src/code_artifact/writer.rs`，属于语言无关基础设施，只负责文本组织和格式策略，不感知 Meta、IR、目标语言 AST 或 artifact 写入。

目标 API：

```rust
pub struct CodeWriter { /* ... */ }
pub struct StreamingCodeWriter<W: std::io::Write> { /* ... */ }

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

impl<W: std::io::Write> StreamingCodeWriter<W> {
    pub fn new(output: W, config: CodeWriterConfig) -> Self;
    pub fn raw(&mut self, text: impl AsRef<str>) -> std::io::Result<()>;
    pub fn raw_fmt(&mut self, args: std::fmt::Arguments<'_>) -> std::io::Result<()>;
    pub fn write(&mut self, text: impl AsRef<str>) -> std::io::Result<()>;
    pub fn line(&mut self, text: impl AsRef<str>) -> std::io::Result<()>;
    pub fn line_fmt(&mut self, args: std::fmt::Arguments<'_>) -> std::io::Result<()>;
    pub fn blank_line(&mut self) -> std::io::Result<()>;
    pub fn indent(&mut self);
    pub fn dedent(&mut self);
    pub fn indented<R>(&mut self, write_body: impl FnOnce(&mut Self) -> R) -> R;
    pub fn finish(self) -> std::io::Result<W>;
}
```

职责边界：

- `line(...)` 在当前缩进层级写入一行并追加换行；`write(...)` 支持写入多行文本并在每个非空新行开始处补齐当前缩进；`raw(...)` 仅追加已有文本，不自动缩进，用于迁移阶段包裹既有生成片段或写入已格式化的大块 runtime helper。
- `indent()` / `dedent()` 管理块级缩进；`indented(...)` 用于保证局部块结束后恢复缩进层级。
- `finish()` 是唯一做文件尾规范化的位置：当 `trailing_blank_line = true` 时文件内容以 `\n\n` 结束；否则当 `trailing_newline = true` 时以单个 `\n` 结束；当 `trailing_newline = false` 且未要求 trailing blank line 时移除末尾换行。
- `StreamingCodeWriter` 的方法返回 `std::io::Result<()>` 并把错误原样交给调用方；当底层 writer 是 `ArtifactWriteStream` 时，由 artifact 边界统一映射为 `ConfigForgeError::WriteFile`。streaming writer 只缓存尾部换行数量，用于在 `finish()` 时实现 `trailing_newline = false` / `trailing_blank_line = true`，不缓存完整文件内容。
- 表达式层仍可返回 `String`，例如目标语言类型表达式、reader helper 表达式和 index key 表达式；语句/块/文件层应使用 `CodeWriter`，避免在高层继续扩散 `push_str(&format!(...))`。
- 生产生成入口是 sink-first `generate_to_sink()`，应使用 `StreamingCodeWriter<ArtifactWriteStream>` / `CheckedStreamingCodeWriter` 直接写入 shared `ArtifactSink`；`generate()` 仅在 `#[cfg(test)]` 下保留，可继续使用 `CodeWriter` 做 String-backed snapshot，并与 sink-backed 输出做等价性校验。

### 9.4.2.4 文件头模板

`file_header` 是可选模板。模板渲染上下文来自 Project Config 和执行上下文：

| 变量 | 来源 |
| --- | --- |
| `{{project.name}}` | `ProjectConfig.project.name`。 |
| `{{project.desc}}` | `ProjectConfig.project.desc`，未配置为空。 |
| `{{project.copyright}}` | `ProjectConfig.project.copyright`，未配置为空。 |
| `{{datetime}}` | 当前生成时间，由 exporter 注入。 |

文件头渲染应发生在语言生成器写入每个 artifact 内容前。语言生成器可以决定是否为每行追加注释前缀，但模板本身应允许用户完整控制文本。

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

### 9.4.4 `GeneratedFile`（测试专用）

```rust
pub struct GeneratedFile {
    pub path: PathBuf,
    pub content: String,
}
```

`GeneratedFile` 只在 `#[cfg(test)]` 下导出，用于 generator 单元测试中的 String-backed snapshot 和 `generate_to_sink()` 等价性校验。生产 codegen 不再返回该结构，也不通过 `Vec<GeneratedFile>` materialize 完整生成内容；所有实际 artifact 统一由 `generate_to_sink()` 写入 `ArtifactSink`。

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

pub struct CodeIndex {
    pub name: String,
    pub unique: bool,
    pub primary: bool,
    pub ordered: bool,
    pub fields: Vec<CodeField>,
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
8. 保留 `CodeIndex.ordered`，让语言生成器区分 unordered exact lookup index 与 ordered range-capable index。
9. 后续应保留 `preserve_source_order` / repository storage policy：区分只承诺 index lookup 的 indexed table，和显式要求 source-order 数组访问的 table。

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
- 依据 table access policy 决定是否渲染 public row sequence API；`preserve_source_order = true` 才生成 row count / row_at / rows，indexed table 默认只生成 lookup；
- 依据 `CodeIndex.ordered` 选择索引 storage 与 API：unordered index 生成 exact lookup；ordered index 除 exact lookup 外还生成 lower_bound / upper_bound / range 查询，并优先使用一次构建后排序数组而不是在线树结构；
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

代码生成必须先解决命名合法性问题，不允许直接使用 Meta 名称。命名策略由 `targets.exports.naming` 配置，并由每个语言生成器解释。

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

### 9.7.1.1 `text` 与国际化生成策略

`text` 在 runtime 基础表示上仍映射为字符串，与 `string` 使用同一套 reader scalar API 和 runtime payload shape；区别在语义层：`text` 是可被 `type = "gettext_pot"` 聚合 exporter 抽取、校验并在 generated code 中附加翻译访问 helper 的字段。生成器不得把普通 `string` 隐式视为可翻译文本，也不得因为启用 POT exporter 改变 runtime data 中 `text` 字段的原始值。

第一阶段采用原文驱动模型：source cell 保存默认语言原文，runtime data 保存同一原文，POT exporter 以该原文作为 `msgid`。因此生成代码需要同时支持两类访问：

- **raw text**：原字段访问保持不变，例如 Rust `row.display_name: String`、C++ `row.display_name`、TypeScript `row.displayName: string`。这保证现有业务代码不因开启 i18n 而破坏，并提供翻译缺失时的天然 fallback。
- **translated text helper**：对 `CodeTypeRef::Text` 字段额外生成可选 helper。默认 `generic` 模式由调用方传入 translator/lookup 对象，把 `(msgctxt, msgid)` 映射为目标语言文本；`global_function` 模式直接调用 `targets.exports.i18n.function`，不再要求调用方传 translator。

推荐的语言无关 lookup contract：

```text
TextTranslator
  translate(msgctxt?, msgid) -> translated string or missing

global_function
  function(msgctxt?, msgid) -> translated string or missing/final string
```

其中：

- `msgid` 来自 raw text 字段值；
- `msgctxt` 来自字段级配置、POT exporter policy 或未来 key-driven 模型的 text context；当前默认是 `table.field` / `struct.field`；
- table、field_path、record_key 等 source metadata 可作为未来扩展，当前已实现 helper contract 不传入 `TextSource`；
- `generic` translator 缺失、返回 missing 或返回空 optional 时，generated helper 默认 fallback 到 raw text；`global_function` 可通过 `returns = "string"` 表示函数总是返回最终文案，或通过 `returns = "optional"` 表示 missing 后 fallback。若业务希望严格模式，应由 translator / global function 自身抛错 / 返回 error，或由后续 codegen option 控制。

推荐 API 形态：

| Language | Raw access | Translated helper |
| --- | --- | --- |
| Rust | `row.display_name: String` | `generic`: `row.display_name_text(&translator) -> String`；`global_function`: `row.display_name_text() -> String` |
| C++ | `row.display_name` | `generic`: `row.DisplayNameText(translator)`；`global_function`: `row.DisplayNameText()` |
| TypeScript | `row.displayName: string` | `generic`: `itemsDisplayNameText(row, translator): string`；`global_function`: `itemsDisplayNameText(row): string` |

命名规则：

- helper 名称由字段生成名追加稳定后缀派生，例如 Rust `_text`、C++ `Text`。TypeScript interface 不能声明方法实现，首版生成顶层函数，名称由 owner + field + `Text` 派生，例如 `itemsNameText`。
- helper 与普通字段、fragment getter、relation helper、index function、lazy helper 同处一个 row/object API scope（或 TypeScript 顶层 export scope）时，必须纳入目标语言冲突检查。当前实现已覆盖已生成 helper 的基础冲突路径；更细粒度的 symbol conflict fixture 仍需后续补齐。
- translator helper 是 generated code API，不是 runtime payload 字段；它不参与 JSON / MessagePack / `bin` schema revision，不影响 index key、FK、relation cache 或 artifact manifest。

缓存策略：默认不在 row/object 内缓存翻译结果。原因是翻译结果依赖语言、domain、运行时 locale、fallback chain 和 translator 实例，row 级缓存容易跨 locale 污染，也会增加生成代码状态复杂度。需要高性能翻译时，应由业务提供带缓存的 translator，缓存 key 至少包含 `(domain, context, msgid, locale)`；generated helper 只负责传入稳定 lookup 信息。

未来 key-driven / rich text / plural 扩展：

- key-driven 模型中，raw cell 可改为稳定 key，默认文案来自 companion column 或外部 catalog。此时 helper contract 应优先传 key，并可继续把 default text 作为 fallback；POT exporter 的 `msgid` 生成规则同步切换。
- rich_text 应进入新的 `CodeTypeRef::RichText` 或 struct-like value，不复用 `string`；helper 返回目标语言的 rich text AST、markup string 或由语言 option 指定的类型。
- format_text / plural / gender / select 应显式建模占位符和变量类型，translator contract 需要接受参数 map，并在 validation 阶段校验 source 与 translation placeholder 一致。普通 `text` helper 不隐式解析 `{name}` 或 `%s`。

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
