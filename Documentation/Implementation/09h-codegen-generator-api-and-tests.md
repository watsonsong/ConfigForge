# 09h. Codegen Generator API and Tests

本文从 Codegen 模块详细设计中拆出，专门描述 `CodeGenerator` 接口、新语言接入步骤、文件组织策略、codegen 诊断和测试计划。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 模块总览和专题索引。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、runtime loading 和 generated API 语义。
- [`01-module-outline.md`](01-module-outline.md) — Rust 模块边界与依赖方向。

## 9.11 生成器接口

```rust
pub(crate) trait CodeGenerator: Send + Sync {
    fn language(&self) -> &'static str;
    fn language_aliases(&self) -> &'static [&'static str] { &[] }
    fn supports_language(&self, language: &str) -> bool;
    fn default_module_name(&self) -> &'static str;
    fn default_naming(&self) -> NamingConfig;
    #[cfg(test)]
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>>;
    fn generate_to_sink(&self, input: &CodeExportInput<'_>, sink: &ArtifactSink) -> Result<()>;
}
```

`generate_to_sink()` 是 build 调用的 sink-first 标准入口，也是生产代码必须实现的写出接口。Build 只传入 shared `ArtifactSink`，由 generator 负责 `sink.create(path)`、通过 streaming writer 渲染内容并 `finish()` stream；trait 不再提供 `generate()` fallback，避免 production path materialize 完整 `String` / `Vec<GeneratedFile>`。`CodeGenerator` 当前是 crate 内部扩展点，不作为稳定外部 generator plugin API 暴露，因此 crate root 不 re-export trait、builtin registry 或具体 generator 类型。`generate()` 与 `GeneratedFile` 仅在 `#[cfg(test)]` 下保留，用于单元测试中的 String-backed snapshot 和 streaming equivalence 校验。

内置生成器：

```rust
pub(crate) struct RustCodeGenerator;
pub(crate) struct CppCodeGenerator;
pub(crate) struct CppUnrealCodeGenerator;
pub(crate) struct CSharpCodeGenerator;
pub(crate) struct JavaCodeGenerator;
pub(crate) struct TypeScriptCodeGenerator;
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
  -> ArtifactSink entries via generate_to_sink()
```

`validate CodeModel` 应检查：

- 目标语言命名合法；
- 命名冲突；
- unsupported field type；
- unsupported index key type；
- unsupported runtime reader contract capability（例如目标语言暂未支持 schema-aware reader API）；
- 输出路径冲突；
- 目标语言需要的 runtime dependency 是否已声明。

扩展新语言的步骤：

1. 新增 `src/codegen/<language>/mod.rs`，在模块内定义 `LANGUAGE`、语言专属 options、实现 `CodeGenerator`，返回规范语言名、可选 alias、默认命名规则和生成逻辑。
2. 新增 `src/codegen/<language>/tests.rs`，覆盖该语言的 loader/reader contract、row/object/parser、index 和命名冲突行为。
3. 如有语言专属配置，新增 option struct，并通过 `CodegenOptions::language_options<T>("<language>")` 解析。
4. 将 generator 加入 crate 内部 `builtin_generators()`；如果未来要支持外部插件 registry，应另行设计不暴露 concrete `ArtifactSink` 的稳定 public writer context trait，而不是直接公开当前内部 `CodeGenerator`。
5. 不修改 `CodeModel`、`src/build/` 的分发逻辑，也不新增中心化语言 enum。

## 9.12 文件组织策略

### 9.12.1 多文件模式，推荐默认

优点：便于阅读和差异比较。

```text
types.*
loader.*
tables.*
index.*
```

### 9.12.2 单文件模式

适合小项目或直接拷贝接入：

```text
config_forge_generated.rs
config_forge_generated.hpp
config_forge_generated.ts
```

第一阶段可以只实现单文件模式以降低复杂度；生产写出通过 `ArtifactSink` 支持单文件或多文件，测试专用 `GeneratedFile` helper 也可表示多文件快照。

## 9.13 错误与诊断

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

## 9.14 测试计划

### 9.14.1 CodeModel 单元测试

1. enum / struct / table 都能进入 `CodeModel`。
2. primitive / enum / struct 字段映射为正确 `CodeTypeRef`。
3. table / field / index 的 `export = false` 与 `export_when` 按 `target_tags` 过滤；primary index 引用当前 target 不导出的字段时报错，非 primary index 引用不导出字段时跳过。
4. primary index、unique index、non-unique index 都能生成 `CodeIndex`。
5. 命名 sanitize 后稳定且可预测。
6. 命名冲突能报错。
7. polymorphic table 的 common fields 与 fragments 能进入 `CodeModel`，fragment fields 按 target tags 过滤。
8. 同名 fragment 出现在不同 table 时，生成类型名应包含 table scope，避免冲突。
9. fragment field 不能作为 index key；Meta/CodeModel 层应给出明确错误或在 Meta validation 阶段提前拒绝。

### 9.14.2 语言生成器 snapshot 测试

对同一个小 Meta fixture 生成各内置 backend 代码，断言关键片段：

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

### 9.14.3 生成代码编译与运行测试

生成代码测试不应只停留在“文件存在”和字符串片段断言。集成测试需要把 runtime data artifact 与 generated code artifact 串起来，验证生成代码能被目标语言工具链接受，并能通过 generated loader interface 读取真实导出的数据。

当前测试 harness 只实现一次 `open + RuntimeDataReader` / `IRuntimeDataReader` / `IConfigForgeRuntimeDataReader`；typed rows / typed object 映射必须由生成代码完成，harness 不再按表手写 generated rows。

通用流程：

1. 在临时目录或被 `.gitignore` 忽略的 fixture `out/` 目录中执行 export，生成 runtime data 与对应语言代码。
2. 为每种目标语言创建测试 harness。harness 实现通用 `RuntimeDataLoader` / `IRuntimeDataLoader` / `IConfigForgeRuntimeDataLoader` 与 reader 接口，并把 JSON / MessagePack payload 暴露为通用 reader。
3. 编译或类型检查 generated code 与 harness。
4. 运行 harness 测试 `GameData` / facade 的加载入口。
5. 断言代表性查询函数正常工作。测试不需要覆盖每一个字段和每一个 index，但必须覆盖当前功能类型：
   - collection table 加载；
   - object table 加载；
   - 单字段 unique index 查询；
   - 多字段联合 unique index 查询；
   - 目标语言对默认 data file 常量的引用。
6. 如果未来标准工程新增 non-unique index、foreign-key helper、聚合 data manifest 等生成代码能力，应在同一类 generated-code runtime test 中补充一个代表性断言，而不是为所有表重复写同构断言。

测试 harness 的 loader 是测试代码，不是产品默认 loader。生成代码仍保持对文件系统、JSON/MessagePack 库和引擎资源系统无依赖；真实项目可以按自己的资源系统实现 loader。目标态下新增表或字段时，harness / 用户侧 loader 接口不应变化。

每种语言的 harness 代码应作为 fixture 的一部分归档进代码仓库，例如 `tests/fixtures/standard_project/harness/{rust,cpp,typescript}/`。集成测试运行时将已归档的 harness 与本次 export 产生的 generated code / runtime data 组合到临时目录或编译命令中：harness 本身稳定纳入 review，generated artifacts 仍位于被忽略的 `out/` 或临时目录中，避免把本次生成结果当作源文件提交。

### 9.14.4 分语言 generated-code runtime test 方案

标准工程当前按 target 拆分生成多语言 codegen；backend 清单、输出位置和 runtime harness 状态见 [`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix)。generated-code runtime test 可以复用标准工程配置，分别验证各语言。

本节描述 generated-code harness 的实现形态和断言边界；local / quick / full / nightly profile、工具链要求、skip / fail-fast 策略和 runtime compliance 覆盖矩阵见 [`../Tech/test-ci-matrix.md`](../Tech/test-ci-matrix.md)。当前 `standard_project` harness 已注册表化：每个语言声明 language、test name、required tools、network policy、profile policy 和 runner function，统一汇总 executed / skipped / failed 结果。

| 语言 | 输入产物 | 工具链 | Harness 做法 | 必测断言 |
| --- | --- | --- | --- | --- |
| Rust | `out/server_code/rust/generated.rs` + `out/server_data/*.msgpack` | `cargo test` | 实现通用 `RuntimeDataLoader` / reader，用 `rmp-serde` 读取真实 MessagePack payload，并把 payload 暴露给 generated parser；不手写 generated rows / objects 映射。 | `GameData::load` 成功；`items.get_by_id(id)` 命中；`class_levels.get_by_class_level(class_id, level)` 命中联合索引；`game_rules` object 字段可访问。 |
| TypeScript | `out/client_code/typescript/generated.ts` + `out/client_data/*.json` | `tsc` + `node` | 实现通用 `RuntimeDataLoader` / reader，用 Node `fs` 读取真实 JSON payload，并由 generated parser 映射 row interface/object。 | `GameData.load` 成功；`items.getById(id)` 命中；`classLevels.getByClassLevel(classId, level)` 命中联合索引；object table 可访问。 |
| C++ | `out/client_code/cpp/generated.hpp` / `generated.cpp` + `out/client_data/*.json` | `c++ -std=c++17` | 实现通用 `IRuntimeDataLoader` / reader，使用轻量 JSON reader 暴露 payload；不能按表手写 typed rows。 | `GameData::Load` 成功；`Items().GetById(id)` 命中；`ClassLevels().GetByClassLevel(class_id, level)` 命中联合索引；object table 可访问。 |
| Unreal C++ | `out/client_code/cpp_unreal/generated.h` / `generated.cpp` + `out/client_data/*.json` | `c++ -std=c++17` + lightweight Unreal shim | 使用测试专用 `CoreMinimal.h` / `Kismet/BlueprintFunctionLibrary.h` shim 提供 `TArray`、`TMap`、`FString`、`TOptional`、`USTRUCT`、`UFUNCTION` 等最小定义；真实 UnrealHeaderTool / Editor 验证留给引擎侧 CI。 | containers 模式语法 smoke 覆盖 `GameData::Load` 依赖的生成声明；Blueprint 模式额外验证 `.generated.h` include、`USTRUCT` / `UPROPERTY` / `UCLASS` / `UFUNCTION` facade 可被 shim 编译。 |
| C# | `out/client_code/csharp/reference_game_config.cs` + `out/client_data/*.json` / `*.msgpack` / `*.bin` | `dotnet run` | 临时写入最小 `.csproj` 和 `Program.cs` harness；harness 复制 `runtime/csharp/json_reader`、`runtime/csharp/messagepack_reader` 与 `runtime/csharp/binary_reader`，并用薄 adapter 实现 generated `IConfigForgeRuntimeDataLoader` / `IConfigForgeRuntimeDataReader`。 | JSON、MessagePack 与 Binary 路径均验证：`GameData.Load` 成功；`Items.GetById(id)` 命中；`GameRules.DefaultClass(data)` relation 命中；`NameText` fallback/翻译 helper 可用；`LazyGameData` 首次访问、缓存和 `PreloadAll()` 可用。 |
| Java | `out/client_code/java/ReferenceGameConfig.java` + `out/client_data/*.json` / `*.msgpack` / `*.bin` | `javac` + `java` | 临时写入 `RuntimeCheck.java` harness；harness 复制 `runtime/java/json_reader`、`runtime/java/messagepack_reader` 与 `runtime/java/binary_reader`，并用薄 adapter 实现 generated `ConfigForgeRuntimeDataLoader` / `ConfigForgeRuntimeDataReader`，不引入 Jackson/Gson 等依赖。 | JSON、MessagePack 与 Binary 路径均验证：`GameData.load` 成功；`getItems().getById(id)` 命中；`getGameRules().defaultClass(data)` relation 命中；`nameText` fallback/翻译 helper 可用；`LazyGameData` 首次访问、缓存和 `preloadAll()` 可用。 |

归档后的 harness 不应直接依赖仓库中某一次已生成的 `out/` 文件路径。推荐做法是：集成测试先执行 export，再把对应语言的 generated code 复制到临时 harness 工作目录（或通过 include path/link path 引用），最后执行目标语言测试命令。这样能确保测试的是“当前实现刚生成的代码和数据”，而不是历史残留 artifact。

工具链缺失时的处理原则：

- CI 环境应安装 Rust/Cargo、C++17 compiler、Node.js、TypeScript、.NET SDK 和 JDK，使各语言 runtime test 都能执行。
- 本地开发环境如果缺少 C++ compiler、Node、`tsc`、`dotnet`、`javac` 或 `java`，对应测试可以显式 skip，但测试输出必须说明 skip 原因，不能静默通过。
- `CONFIG_FORGE_TEST_PROFILE=full|nightly` 或 `CONFIG_FORGE_REQUIRE_TOOLS` 声明的工具缺失时应 fail-fast；`CONFIG_FORGE_SKIP_LANGUAGES` 可显式禁用可选语言 harness。
- `npx` fallback、C++ FetchContent 等潜在联网行为默认关闭，只能通过 `CONFIG_FORGE_ALLOW_NETWORK=1` 或 adapter 专用显式开关启用。
- Rust 生成代码测试不应 skip，因为项目本身依赖 Rust toolchain。

### 9.14.5 CLI 集成测试

在 `tests/fixtures/standard_project/` 上运行：

1. export 生成 runtime data。
2. export 生成 [`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) 中列入 standard_project runtime test 的 generated code。
3. 检查输出文件存在。
4. 检查关键 API 名称存在。
5. 运行分语言 generated-code runtime test，确认生成代码能通过 loader 加载真实导出数据并执行代表性查询。

集成测试写出的 artifact 应位于 fixture-local `out/` 目录或临时目录。fixture-local `out/` 必须通过 `.gitignore` 忽略；新增 fixture 如果也写出导出结果，应遵循 `tests/fixtures/**/out/` 约定，避免误提交生成产物。

## 9.15 当前实现状态

当前 codegen 主体能力：

- `src/codegen/` 提供 CodeModel IR、naming / type mapping、生成符号冲突检测和内置 generator registry。
- 内置 backend 的输出形态、核心能力和 runtime harness 状态见 [`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix)。
- format-agnostic backend 都使用 Meta-agnostic reader loader contract，不直接绑定 JSON / MessagePack / bin exporter 结果。
- runtime data 与 generated code 都以 artifact 形态参与统一写出；Artifact Writer 负责路径冲突检查、dry-run、direct write、`temp-commit` 与 bounded write queue。
- `CodeGenerator::generate_to_sink()` 是 codegen 的唯一 production 写出入口；`GeneratedFile` / `generate()` 只在测试构建中存在，用于快照与 streaming equivalence 测试。
- 标准工程 fixture 覆盖 runtime data + generated code 的端到端输出和代表性运行时加载；各 backend 的回归方式见 [`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix)。

未来规划：

- 如需降低单文件体量，可为 Rust / TypeScript / C++ / C# 增加多文件输出模式。
- Java backend 已按 [`09l-codegen-java-backend.md`](09l-codegen-java-backend.md) 落地；当前集成测试已改为复用 `runtime/java/json_reader` / `runtime/java/messagepack_reader` / `runtime/java/binary_reader`，并已接入 Java JSON / MessagePack / Binary compliance runner；后续可补充 Jackson / Gson / Android 友好 adapter。
- 如需发布稳定 runtime adapter，可为 C++ 优先拆出 external stable interface header。
- summary manifest 可作为 `targets.exports` 的聚合导出格式独立实现，不进入默认 codegen 决策。
