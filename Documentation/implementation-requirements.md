# Implementation Technical Requirements

ConfigForge 的实现需要服务于长期扩展：不同团队会接入不同 Source、不同运行时格式、不同校验规则和不同引擎工具链。因此实现层面应从一开始把 **插件化**、**并行化**、**异步 I/O** 和 **可调试性** 作为架构约束，而不是导出流程稳定之后的附加优化。

本文档是设计文档，不要求当前代码已经实现所有能力。

## 设计目标

- 支持通过动态库插件扩展 Source Provider、Data Exporter、Code Generator、Validator 和 Builtin Value Type。
- 支持多线程加载和链接 Meta 文件，在保持确定性输出的前提下提升大型项目处理速度。
- 支持按阶段并行执行 Validator，并批量收集诊断信息。
- 支持并行导出相互独立的产物，例如不同 target、不同表、不同 runtime data file。
- 支持异步 Source Provider API，尤其是 Lark Sheet、Google Sheet、HTTP、内部平台等网络数据源。
- 支持异步文件读写，避免大文件导出阻塞整个执行器。
- 支持结构化、稳定、可机器读取的错误和诊断输出。
- 尽量收集所有可继续发现的问题，而不是遇到第一个错误就停止。

## 非目标

- 本文档不绑定具体 async runtime。实现可以选择 `tokio`、`async-std` 或轻量自定义 runtime，但 public domain trait 应避免过度泄露 runtime 细节。
- 本文档不要求所有插件接口在第一版同时稳定。第一阶段可以先稳定内置 registry，再逐步开放动态库 ABI。
- 本文档不要求所有 Validator 都能并行执行。依赖全局状态或外部服务的 Validator 可以显式声明串行执行。
- 本文档不定义完整安全沙箱。动态库插件默认视为受信任代码；如果未来需要第三方不可信插件，应另行设计隔离进程或 WASM 沙箱。

## 插件化

### 插件扩展点

推荐至少预留以下插件扩展点：

| Plugin Kind | Suggested Trait / Symbol | Purpose |
| --- | --- | --- |
| Source Provider Plugin | `SourceProviderPlugin`, `SourceProvider` | 读取 CSV/XLSX/Lark/Google/internal sheet 等数据源并归一化为 `SourceSheet`。 |
| Data Exporter Plugin | `DataExporterPlugin`, `DataExporter` | 输出 JSON/YAML/MessagePack/Protobuf/SQLite/custom binary 等运行时数据。 |
| Code Generator Plugin | `CodeGeneratorPlugin`, `CodeGenerator` | 为 TypeScript/Lua/C#/C++ 等目标生成类型和访问 API。 |
| Validator Plugin | `ValidatorPlugin`, `ValidatorFactory` | 注册业务 Validator 或项目级规则。 |
| Builtin Type Plugin | `BuiltinTypePlugin`, `BuiltinValueTypeProvider` | 扩展可单元格编写的逻辑值类型及解析/校验规则。 |
| Special Exporter Plugin | `SpecialExporterPlugin`, `SpecialExporter` | 输出 POT、asset manifest、审计报告等辅助产物。 |

### 动态库加载

动态库插件用于让项目或团队在不修改 ConfigForge 主程序的情况下扩展能力。

推荐约束：

- 支持平台默认动态库后缀：macOS `.dylib`、Linux `.so`、Windows `.dll`。
- 插件应声明 `PluginManifest`，包含插件 ID、版本、支持的 ConfigForge API 版本、能力列表和可选配置 schema。
- CLI 启动时从显式配置的 plugin search paths 加载插件，不应隐式扫描不受控目录。
- 插件 ID 必须稳定且唯一，例如 `com.game.lark_sheet`、`studio.asset_manifest`。
- 插件加载失败应产生结构化诊断，说明路径、平台、符号名、版本约束和底层错误。
- 插件注册只应发生在启动/规划阶段，执行阶段不应动态改变 registry。

概念性 Rust 草图：

```rust
struct PluginManifest {
    id: String,
    version: String,
    api_version: String,
    capabilities: Vec<PluginCapability>,
}

enum PluginCapability {
    SourceProvider { provider: String },
    DataExporter { format: String },
    CodeGenerator { language: String },
    Validator { kind: String },
    BuiltinType { type_name: String },
    SpecialExporter { kind: String },
}

trait PluginRegistrar {
    fn register_source_provider(&mut self, provider: Box<dyn SourceProviderFactory>);
    fn register_data_exporter(&mut self, exporter: Box<dyn DataExporterFactory>);
    fn register_code_generator(&mut self, generator: Box<dyn CodeGeneratorFactory>);
    fn register_validator(&mut self, validator: Box<dyn ValidatorFactory>);
    fn register_builtin_type(&mut self, ty: Box<dyn BuiltinTypeProvider>);
}
```

Rust trait object 不能直接作为跨动态库稳定 ABI。实际实现应在边界层使用 C ABI、`abi_stable`、进程间协议或其他明确 ABI 策略，并在 ConfigForge 内部转换为安全的 Rust trait object。

### Registry 设计

所有内置能力和插件能力都应注册到统一 registry，但 registry 可以按领域拆分：

- `SourceProviderRegistry`
- `DataExporterRegistry`
- `CodeGeneratorRegistry`
- `ValidatorRegistry`
- `BuiltinTypeRegistry`
- `SpecialExporterRegistry`

Registry 解析规则应确定性：

1. 内置能力先注册；
2. 插件按配置文件中的声明顺序注册；
3. 若插件能力与已有能力 ID 冲突，默认报错；
4. 如需覆盖内置能力，必须显式声明 `override = true`；
5. 最终 registry 内容应可通过 CLI debug 命令或诊断输出查看。

## 并行化

### 并行 Meta 加载

大型项目通常会把 Meta 拆分为多个文件。实现应支持并行读取、解析和初步语法校验，然后在单线程或确定性阶段完成链接与全局语义校验。

推荐流程：

```text
Discover Meta Documents
  -> Parallel Read
  -> Parallel Parse
  -> Per-Document Syntax Diagnostics
  -> Deterministic Merge
  -> Global Link and Validation
```

约束：

- 文件发现顺序应排序，避免不同文件系统顺序导致输出波动。
- 每个文件的 parse diagnostics 可以并行产生，但最终输出必须按稳定 key 排序。
- `MetaModel` 合并必须能检测名称冲突、重复定义和跨文件引用。
- 若某些 Meta 文件解析失败，仍应尽量解析其他文件并报告更多错误；但全局链接可以在缺失必要定义时降级或停止。

### 并行 Validator 执行

Validator 应按 `ValidationPhase` 顺序执行。阶段之间保持顺序，阶段内部可以并行。

推荐规则：

- `Meta`、`SourceDeclaration`、`SourceNormalization` 阶段可按文件、表或 source 并行。
- `Field` 阶段可按表/记录/字段分片并行。
- `Record` 阶段可按记录并行。
- `Collection` 阶段可按表并行。
- `MultiCollection` 阶段可按独立 validator 并行，但需要声明依赖和读集合。
- `Export` 阶段可按 target 或 artifact 并行。
- Validator 不应修改共享数据；共享输入应只读，输出写入线程本地 `DiagnosticCollector` 后合并。

概念性 Rust 草图：

```rust
struct ValidationExecutionPlan {
    phases: Vec<ValidationPhasePlan>,
}

struct ValidationPhasePlan {
    phase: ValidationPhase,
    tasks: Vec<ValidationTask>,
    ordering: DiagnosticOrdering,
}

struct DiagnosticCollector {
    diagnostics: Vec<ValidationDiagnostic>,
}
```

### 并行导出

导出阶段应支持将相互独立的产物作为任务并行执行。

可并行的常见维度：

- 多个 export target；
- 同一 target 下按表拆分的 runtime data files；
- 代码生成中的多个模块/文件；
- special exporter 与常规 runtime exporter；
- asset manifest、POT、schema 文件等辅助产物。

约束：

- 同一个输出路径只能由一个任务写入；输出路径冲突必须在执行前验证。
- 生成内容应先写入临时文件或内存 artifact，再通过确定性 commit 阶段落盘。
- Artifact manifest 应按路径排序，避免并行完成顺序影响输出。
- 并行任务失败后，其他可继续任务应尽量完成，以便报告更多导出错误；最终命令仍应以失败退出。

## 异步化

### Async Source Provider

Source Provider 应以 async API 作为长期方向。网络 provider 需要避免阻塞整个 CLI，且应支持超时、重试、取消和认证刷新。

概念性 Rust 草图：

```rust
#[async_trait]
trait SourceProvider {
    fn id(&self) -> &'static str;
    async fn load(&self, request: SourceRequest, ctx: &SourceLoadContext) -> SourceResult<SourceSheet>;
}
```

推荐能力：

- 网络请求超时和重试策略；
- auth_ref 到实际凭证的外部解析；
- provider-specific rate limit；
- cancellation token；
- source cache；
- source location 保留；
- 结构化 provider diagnostics。

### Async File I/O

导出器和 source provider 都应支持异步文件读写。

推荐约束：

- 小文件可以一次性读写，但接口上仍应允许 async。
- 大型 runtime data、SQLite、Protobuf binary 或 custom binary 应支持流式写入。
- Artifact writer 应提供临时路径和原子替换能力。
- 写入失败应包含路径、artifact kind、target name 和底层错误。

概念性 Rust 草图：

```rust
#[async_trait]
trait ArtifactWriter {
    async fn write_artifact(&mut self, artifact: ExportArtifact) -> ExportResult<()>;
    async fn commit(self) -> ExportResult<ArtifactManifest>;
}
```

## 可调试性

### 批量错误收集

ConfigForge 应优先输出“可批量修复”的问题列表，而不是只输出第一个错误。

推荐策略：

- Meta 文件语法错误：继续解析其他文件。
- Source 加载错误：继续加载其他不依赖该 source 的表。
- Field parse 错误：同表其他字段和记录继续解析。
- Validator 错误：同阶段其他 validator 继续执行，除非该错误破坏阶段前置条件。
- Export 错误：其他独立 artifact 继续生成到临时位置，但最终不提交失败 target。

### 结构化诊断

所有面向用户的错误都应尽量转换为结构化诊断。

推荐字段：

```rust
struct Diagnostic {
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    message: String,
    source_location: Option<SourceLocation>,
    table: Option<String>,
    field_path: Option<FieldPath>,
    record_key: Option<String>,
    target: Option<String>,
    plugin: Option<String>,
    related_locations: Vec<RelatedLocation>,
    hint: Option<String>,
}
```

诊断必须具备：

- 稳定 `code`，例如 `field.required_missing`、`plugin.load_failed`、`export.path_conflict`；
- 人类可读 `message`；
- 可定位 `source_location`；
- 可选 `hint`；
- 多位置错误的 `related_locations`；
- 稳定排序规则。

### 诊断输出格式

CLI 至少应支持人类可读格式。长期建议同时支持 JSON Lines 或 JSON report，便于 CI、IDE 和内部平台消费。

建议格式：

- `human`：默认，适合终端；
- `json`：完整结构化报告；
- `jsonl`：一行一个诊断，适合流式处理；
- `summary`：只输出统计和最严重问题。

### 稳定排序

即使内部并行执行，诊断和 artifact manifest 也必须稳定排序。

推荐诊断排序 key：

1. severity；
2. source file / source id；
3. table name；
4. row index；
5. column index；
6. field path；
7. diagnostic code；
8. message。

## 推荐代码结构

未来 Rust 实现可以逐步演进为以下结构：

```text
src/
  plugin/
    mod.rs
    manifest.rs
    loader.rs
    registry.rs
    capability.rs
  runtime/
    mod.rs
    executor.rs
    async_io.rs
    task.rs
  diagnostics/
    mod.rs
    diagnostic.rs
    collector.rs
    reporter.rs
    ordering.rs
```

这些模块不替代 `meta`、`source`、`validation`、`exporter`，而是为它们提供公共基础设施。

## 验收标准

- 插件扩展点覆盖 Source Provider、Data Exporter、Code Generator、Validator、Builtin Value Type 和 Special Exporter。
- 动态库插件设计说明包含 manifest、API version、registry、冲突处理和加载失败诊断。
- 并行化说明覆盖 Meta 加载、Validator 分阶段并行和独立 artifact 并行导出。
- 异步化说明覆盖网络 Source Provider 和文件读写。
- 可调试性说明覆盖批量错误收集、结构化诊断、稳定错误码和稳定排序。
- 相关设计文档应引用本文档，避免每个领域重复定义实现基础设施。
