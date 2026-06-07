# 09. Codegen 模块详细设计

本文档描述 ConfigForge 的代码导出任务设计。目标是在已加载、已链接并通过校验的 Meta 与 Data 基础上，生成 Rust、C++、TypeScript、Unreal C++、C# 和 Java 等语言的运行时代码，覆盖枚举、结构体、数据表、索引辅助查找函数，并提供抽象的数据文件加载器接口，让接入方可以从本地文件、包内资源、网络或引擎资源系统中加载实际数据。加载器接口必须与具体 Meta、表和字段解耦；所有按表/字段解析和 typed row/object 构造职责属于生成代码。

产品层语义仍以 `Documentation/Design/` 为准；本文档聚焦程序实现模块、生成代码形态、跨语言约束、运行时加载抽象和测试策略。

## 9.1 设计目标

当前已落地第一阶段 Code Export 需要达成：

1. 基于 `LinkedMetaModel<'meta>` 生成完整类型代码，而不是只为每张表生成简单 stub。
2. 支持多种目标语言：Rust、C++、TypeScript、Unreal C++、C#、Java。
3. 生成所有 Meta 中声明的：
   - enum；
   - named struct；
   - table row type；
   - table repository / collection type；
   - index key type 和辅助查找函数。
4. 生成一个统一的 DataSet / Database facade，便于业务侧一次加载后按表访问。
5. 支持 JSON、MessagePack 与 `bin` 紧密二进制 runtime data export type。生成代码使用逻辑 table name（例如 `items`）作为默认加载键；真实 artifact 目录、后缀和格式由运行时 loader / reader 映射。`bin` 额外携带每表 schema revision 供 generated reader 校验。
6. 生成抽象 `RuntimeDataLoader` / `RuntimeDataReader`，由接入方 loader 只负责打开数据文件并提供通用 reader，生成代码负责所有表/字段解析和 typed row/object 构造。
7. 保持生成代码确定性：同一份 Meta 生成稳定文件名、声明顺序和 API 名称。
8. 在生成前发现命名冲突、保留字、目标语言类型映射缺失、输出路径冲突等问题。

当前 loader contract 使用 Meta-agnostic reader loader：生成抽象 `RuntimeDataLoader` / `RuntimeDataReader`，由接入方负责打开资源并提供通用 object / array / scalar 读取能力；生成代码负责读取 payload envelope `{ meta, rows }`、按 Meta 字段映射到 typed rows / typed objects。loader 不能按表生成方法，也不能返回具体 `TableRow` 类型。

第一阶段不实现：

- 生成代码的插件化动态加载；
- `bin` v1 之外的复杂二进制数据格式、schema evolution 和随机访问；
- 跨语言运行时库发布；
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
  -> data-backed generators (JSON / MessagePack / bin / Lua / POT / Asset Manifest)
  -> meta-only generators (Rust / C++ / TypeScript / C# / Java / Schema Doc)
  -> shared ArtifactSink
```

本文档描述当前 `src/code_artifact/` 的 code artifact 实现边界，以及目标态 code artifact toolkit 的迁移方向。`codegen` 这个名称在历史上同时承载“具体语言 generator”和“共享代码产物 helper”，在统一 `ArtifactGenerator` 模型下容易误导为与 `generator` 平级的产物归属地；目标态应把 Rust / C++ / TypeScript / C# / Java 等具体语言 export type 迁入 `src/generator/<type_id>/`，而把当前 codegen 中可复用的 CodeModel IR、CodeWriter、naming / formatting / keyword conflict、`CodeArtifactOptions` 和 runtime reader contract helper 沉淀为 code artifact toolkit。

目标态 language code-like generator 内部流程仍可继续演进为：

```text
Meta Load / Link
Data Load / Normalize / Validate
  -> ExportInput
  -> CodeExportPlan::build
  -> GeneratorRegistry dispatch (build-facing)
  -> generator/<language> converts to CodeExportPlan / CodeModel
  -> code artifact toolkit renders through CodeWriter
  -> ArtifactSink
```

关键约束：

- Code Export 只依赖已 linked 的 Meta，不直接读取 TOML。
- Code Export 不重新读取 CSV，不直接依赖 `DataLoader`。
- Runtime data 文件和 generated code 文件都应进入同一套 Artifact Plan，统一检查路径冲突。
- Data 文件格式第一阶段支持 JSON、MessagePack 与 `bin`。`bin` 使用 positional packed layout，并通过 exporter 与 CodeModel 共享的 schema revision 保证 generated reader 与数据文件布局一致。目标态生成代码只约束通用 `RuntimeDataLoader` / reader 协议。loader 实现负责打开具体文件并暴露格式无关的读取 cursor，生成代码本身不绑定文件系统、引擎资源系统、JSON 库、MessagePack 库或 binary decoder，但负责所有 table / field 级解析逻辑。

## 9.3 模块结构建议

当前 `src/code_artifact/` 是 code artifact toolkit；具体语言 backend 已迁入 `src/generator/<type_id>/`：

```text
src/code_artifact/
  mod.rs              # 对外 API、re-export、CodeExportTarget / CodeExportInput / Artifact 等核心模型
  ir.rs               # Language-neutral CodeModel IR
  naming.rs           # 目标语言命名规范、保留字处理、冲突检测
  writer.rs           # CodeWriter / StreamingCodeWriter / indentation / trailing newline helper
  tests.rs            # IR、配置、命名等语言无关公共测试

src/generator/
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
  cpp_unreal/
    mod.rs            # CppUnrealCodeGenerator 与 Unreal header/source 编排
    types.rs          # Unreal C++ enum/struct/table row/fragment 类型渲染
    symbols.rs        # Unreal C++ 符号冲突检查
    tests.rs          # Unreal C++ 生成器测试
  csharp/
    mod.rs            # CSharpCodeGenerator 与单文件编排
    types.rs          # C# 命名、类型映射、key / helper 名称
    symbols.rs        # C# 符号冲突检查
    tests.rs          # C# 生成器测试
  java/
    mod.rs            # JavaCodeGenerator 与单文件编排
    types.rs          # Java 命名、类型映射、key / helper 名称
    symbols.rs        # Java 符号冲突检查
    tests.rs          # Java 生成器测试
  typescript/
    mod.rs            # TypeScriptCodeGenerator 与生成编排
    runtime.rs        # TypeScript runtime loader/reader contract 渲染
    types.rs          # TypeScript enum/interface/fragment 类型渲染
    reader.rs         # TypeScript table/object payload reader 渲染
    repository.rs     # TypeScript repository、index、database facade 渲染
    symbols.rs        # TypeScriptNames 与符号冲突检查
    tests.rs          # TypeScript 生成器测试
```

Java backend 已落地为 `src/generator/java/`，包含 `JavaCodeGenerator`、Java 命名/类型映射、符号冲突检查和单元测试，并复用 code artifact toolkit 中的 CodeModel / writer / options helper。设计见 [`09l-codegen-java-backend.md`](09l-codegen-java-backend.md)。

Code artifact toolkit 遵循开闭原则：核心模块只定义语言无关模型、crate 内部 code artifact 辅助 trait、公共配置结构和 writer/helper；每种目标语言作为一等公民独立放在自己的 generator 目录中，语言标识常量、语言专属 options、类型映射、运行时 reader contract 渲染、命名默认值和测试都在该目录内维护。新增内置语言应新增 `src/generator/<language>/`，通过 `GeneratorRegistry` 注册为 build-facing generator，并依赖 code artifact toolkit，不回到 IR 或 build 编排层增加语言分支，也不在 toolkit 核心中新增语言常量或语言 options。

概念边界：`CodeGenerator` 名称中虽然包含 “Generator”，但它不是直接提供给任务系统调度的 build-facing generator trait，而是迁移期 code artifact toolkit 内部的能力接口：提供 language id / alias、默认 module name、naming / formatting、`line_comment_prefix()`、文件头渲染约定，以及最终调用语言 backend 写出代码的入口。当前 language codegen backend 的 requirements 均为 MetaOnly；但 requirements 属于 build-facing generator 调度属性，由统一 `GeneratorRegistry` / `GeneratorRequirements` 表达，并通过 code artifact adapter 暴露成 `ArtifactGenerator`。`schema_doc` 已迁为 direct doc `ArtifactGenerator`，不再注册到 codegen builtin registry；Lua 这类 requirements 包含 Meta + Data 的 code-like artifact 复用 `CodeArtifactOptions`，而不是复制配置或伪装成当前 format-agnostic `CodeGenerator`。

每个语言 generator 目录的 `mod.rs` 只负责实现 build-facing generator 或迁移期 `CodeGenerator` adapter、构造 `CodeModel`、调用语言内各渲染模块并写出生成文件；实际渲染逻辑按 runtime contract、类型声明、reader、repository 和符号解析拆分。当前 `generate_to_sink()` 是 build 调用的标准入口，内置 backend 均通过 `ArtifactSink` 和 streaming writer 直接写出 artifact；legacy `generate() -> Vec<GeneratedFile>` 已降级为 `#[cfg(test)]` 快照 / 等价性测试 helper，不参与 production build。类型映射当前随 `types.rs` / `reader.rs` 保持在语言目录内；若单一语言继续膨胀，可继续在该语言 generator 目录内拆出 `type_mapping.rs`、`index.rs` 等，不应把语言专属逻辑回流到 code artifact toolkit 核心。

`src/build/` 保留为执行编排层，负责：

- 加载 Meta 和 Data；
- 归一化与校验；
- 构造统一 `GeneratorCommonInput` / `GeneratorDataInput`；
- 通过 `GeneratorRegistry::require_generator()` 查询 build-facing `ArtifactGenerator`；
- 写出 Artifact。

Artifact Writer 已拆为独立 `src/artifact/` 模块，code-like generator 后续应通过 Artifact 模块统一写出：

```text
src/artifact/
  mod.rs
  model.rs            # Artifact / ArtifactContent / ArtifactPath
  writer.rs           # dry-run / direct / temp-commit 写入策略
```

`CodeGenerator` 的迁移期 crate-internal API 是 `generate_to_sink(input, sink)`：codegen adapter 从 `GeneratorCommonInput` 构造 `CodeExportInput` 并传入 shared `ArtifactSink`，Build 本身不再区分 language codegen、direct generator 或 data exporter，也不再遍历 `Vec<GeneratedFile>` 或了解生成文件的 materialization 方式。该 trait 不是稳定外部 generator plugin API，因此可以依赖内部 `ArtifactSink`；crate root 不 re-export `CodeGenerator`、builtin registry 或具体 generator 类型。`generate()` / `GeneratedFile` 仅在测试构建中存在，用于 String-backed snapshot 与 sink-backed streaming 输出等价性校验。目标态每个 language generator 可直接实现 `ArtifactGenerator`，内部继续复用 `CodeExportInput` / `CodeModel` / writer helper。

### 9.3.1 ObjectTable codegen 边界

ObjectTable Step 1 的 codegen 语义并入 Codegen 模块：

- `CodeTable.kind` 区分 `Object` / `Collection`。
- ObjectTable 生成目标语言中的 singleton object 类型和访问入口。
- ObjectTable 不生成 collection repository，不生成 index lookup helpers。
- ObjectTable 可以携带 `CodeForeignKey` 关系元数据，用于生成从 singleton object 字段跳转到 CollectionTable repository 的 helper；当前各 backend 的 `linked_field` helper/cache/link 覆盖范围见 [Codegen backend capability matrix](#941-codegen-backend-capability-matrix)。
- capability matrix 中的 format-agnostic backend 都应避免把 ObjectTable 误表达为 rows collection。
- Runtime data load key 使用逻辑 table name，不由 exporter 真实写出的 `RuntimeDataFile.path` 决定；codegen 不关心 ObjectTable 的 CSV `key,value` 物理布局。

建议 API 形态：

- Rust：`GameData::game_config() -> &GameConfig` 或 public field `game_config: GameConfig`。
- TypeScript：`data.gameConfig` 或 `getGameConfig()`。
- C++：`GetGameConfig()` 返回 const reference。

## 9.4 Codegen 专题索引

为降低单文件体量，Codegen 的详细设计已拆到相邻专题文档。本文只保留 Codegen 总览、模块边界、专题索引和当前实现状态摘要；CodeModel IR、命名/冲突、索引、FK relation、生成器接口与测试计划等正文均迁移到专题文档。

| 专题 | 文档 | 内容 |
| --- | --- | --- |
| Runtime reader contract | [`09a-codegen-runtime-reader-contract.md`](09a-codegen-runtime-reader-contract.md) | `RuntimeDataLoader` / `RuntimeDataReader`、eager/lazy loading、TypeScript sync API、逻辑 table name 与 artifact path、schema revision 协作。 |
| Rust backend | [`09b-codegen-rust-backend.md`](09b-codegen-rust-backend.md) | Rust 类型、repository、`GameData` / `LazyGameData`、relation helper 与 text helper。 |
| C++ backend | [`09c-codegen-cpp-backend.md`](09c-codegen-cpp-backend.md) | C++ header/source、namespace、ownership、`GameData` / `LazyGameData`、relation helper 与 text helper。 |
| Unreal C++ backend | [`09j-codegen-cpp-unreal-backend.md`](09j-codegen-cpp-unreal-backend.md) | 已实现的 `cpp_unreal` 独立生成器、Unreal 容器模式、Blueprint 反射模式、公共 access interface，以及与标准 C++ backend 的复用边界。 |
| TypeScript backend | [`09d-codegen-typescript-backend.md`](09d-codegen-typescript-backend.md) | TypeScript interface/class、async/sync loading、`LazyGameData`、relation helper 与 text helper。 |
| Runtime adapters | [`09e-codegen-runtime-adapters.md`](09e-codegen-runtime-adapters.md) | `runtime/` 标准 adapters、C++ CMake、TS packages、Rust templates、compliance fixtures。 |
| Core model / IR / options | [`09f-codegen-core-model-and-options.md`](09f-codegen-core-model-and-options.md) | `CodeExportTarget` / `CodeExportInput`、测试专用 `GeneratedFile`、CodeModel IR、命名策略、类型映射和 string-backed / streaming 通用 writer。 |
| Index and relation helpers | [`09g-codegen-index-and-relation-helpers.md`](09g-codegen-index-and-relation-helpers.md) | CollectionTable index lookup、`linked_field` relation helper、row-index cache、`link` / `link_all`。 |
| Generator API and tests | [`09h-codegen-generator-api-and-tests.md`](09h-codegen-generator-api-and-tests.md) | 迁移期 `CodeGenerator` 接口、新语言接入步骤、文件组织、诊断和测试计划；目标态应收敛到 build-facing `ArtifactGenerator`。 |
| Lua embedded data generator | [`09i-lua-embedded-data-generator.md`](09i-lua-embedded-data-generator.md) | 已实现的 Lua data+code 合一导出、每表 `.lua`、lazy require、row array + metatable、索引和实现说明。 |
| C# backend | [`09k-codegen-csharp-backend.md`](09k-codegen-csharp-backend.md) | 已实现的 C# backend、generated API、runtime reader contract、index/relation/text/lazy 设计和测试策略。 |
| Java backend | [`09l-codegen-java-backend.md`](09l-codegen-java-backend.md) | 已实现的 Java backend、单文件嵌套 public API、runtime reader contract、index/relation/text/lazy 设计和测试策略。 |

### 9.4.1 Codegen backend capability matrix

本表是当前内置 code-like generator / 迁移期 codegen backend 的权威能力矩阵。新增、删除或重命名 backend 时，优先更新本表；其他 Design / Implementation / Tech 文档应链接到本节，而不是复制完整 backend 清单。Source module 列记录当前物理路径。

| Backend | Export type / aliases | Current source module | Topic doc | Output shape | Dependency | Core capabilities | Runtime test / adapter status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Rust | `rust` | `src/generator/rust/` | [`09b-codegen-rust-backend.md`](09b-codegen-rust-backend.md) | single `.rs` | `MetaOnly` | enum、struct、row/object、repository、eager `GameData`、`LazyGameData`、hash/ordered/compound index、relation/text helper、source-order opt-in | standard_project `cargo test`；`runtime/rust/` 已有 JSON / MessagePack / binary reader 模板 |
| C++ | `cpp` / `c++` | `src/generator/cpp/` | [`09c-codegen-cpp-backend.md`](09c-codegen-cpp-backend.md) | `.hpp` + `.cpp` | `MetaOnly` | enum、struct、row/object、repository、eager `GameData`、`LazyGameData`、hash/ordered/compound index、relation/text helper、source-order opt-in | standard_project C++17 harness；`runtime/cpp/` 已有 RapidJSON / msgpack-cxx / binary reader adapter |
| TypeScript | `typescript` / `ts` | `src/generator/typescript/` | [`09d-codegen-typescript-backend.md`](09d-codegen-typescript-backend.md) | single `.ts` | `MetaOnly` | interface/class、repository、eager `GameData`、`LazyGameData`、async + sync loading、Map index、relation/text helper、source-order opt-in | standard_project `tsc` + node harness；`runtime/typescript/` 已有 core / JSON / MessagePack / binary / node-fs packages |
| Unreal C++ | `cpp_unreal` | `src/generator/cpp_unreal/` | [`09j-codegen-cpp-unreal-backend.md`](09j-codegen-cpp-unreal-backend.md) | `.h` + `.cpp` | `MetaOnly` | Unreal containers mode、Blueprint mode、eager `FGameData`、`FLazyGameData`、hash/ordered/compound index、relation/text helper、`type_name_prefix` | standard_project lightweight Unreal shim syntax smoke；真实 UHT / Editor 验证由引擎侧 CI 覆盖；生产 adapter 由 Unreal module 提供 |
| C# | `csharp` / `cs` | `src/generator/csharp/` | [`09k-codegen-csharp-backend.md`](09k-codegen-csharp-backend.md) | single `.cs` | `MetaOnly` | enum、class/row/object、repository、eager `GameData`、同步 `LazyGameData`、hash/ordered/compound index、`IGameDataAccess` relation helper、text helper、positional `bin` reader contract | standard_project `dotnet run` + `runtime/csharp/json_reader` / `runtime/csharp/messagepack_reader` / `runtime/csharp/binary_reader` |
| Java | `java` / `jvm` | `src/generator/java/` | [`09l-codegen-java-backend.md`](09l-codegen-java-backend.md) | single `.java` root class + nested public static types | `MetaOnly` | enum、class/row/object、repository、eager `GameData`、同步 `LazyGameData`、hash/ordered/compound index、`GameDataAccess` relation helper、text helper、positional `bin` reader contract | standard_project `javac` + `java` + `runtime/java/json_reader` / `runtime/java/messagepack_reader` / `runtime/java/binary_reader` |
| Schema Doc | `schema_doc` | `src/generator/schema_doc.rs` | [`Design/11-schema-doc-generator.md`](../Design/11-schema-doc-generator.md) | `index.md` + per-table `.md` | `MetaOnly` | SchemaDoc normalized model、target-aware table/field/index/FK visibility、asset/text/validator 摘要、Markdown renderer | schema_doc unit tests；CLI docs-only target integration；standard_project `schema_docs` target |

横向约束：

- 上表中的 format-agnostic backend 均使用 Meta-agnostic reader loader contract：接入方只提供通用 `open(path)` / `Open(path)` 和 reader cursor，生成代码负责 table / field / fragment / object parsing。
- 上表中的 `MetaOnly` backend 当前都通过迁移期 `CodeGenerator::generate_to_sink()` 进入 production 写出路径，产物统一交给 `ArtifactSink`；目标态可直接实现 `ArtifactGenerator`。
- Lua embedded data generator 是 `type = "lua_table"` 的 MetaAndData mixed generator，不属于本矩阵中的 format-agnostic `CodeGenerator` backend，但应复用 code artifact toolkit 的 option / writer helper。
- `preserve_source_order` 的 generated access 语义仍为规划项：目标行为是 indexed CollectionTable 默认不生成 public source-order `rows` API；只有显式声明 `preserve_source_order = true` 的表生成 row count / row_at / rows 访问，服务随机角色名这类按表格顺序随机访问的配置。

## 9.5 总览文档维护规则

本文只保留以下内容：

1. 当前 Codegen 模块与目标 code artifact toolkit 的职责边界。
2. ObjectTable 等跨专题边界的短说明。
3. 拆分专题索引和 backend capability matrix。

如果需要新增正文，应优先落到 `09a`~`09l` 的专题文档；只有影响整个 Codegen 模块入口、依赖方向或专题划分时才修改本文。这样可以避免总览文档重新膨胀，保证 AI 和人类读者能在一次阅读内获得导航。
