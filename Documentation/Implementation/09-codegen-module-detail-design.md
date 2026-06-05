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
  writer.rs           # CodeWriter / StreamingCodeWriter / indentation / trailing newline helper
  tests.rs            # IR、配置、命名等语言无关公共测试
```

Codegen 模块遵循开闭原则：核心模块只定义语言无关模型、crate 内部生成器 trait、注册表查找和公共配置结构；每种目标语言作为一等公民独立放在自己的目录中，语言标识常量、语言专属 options、类型映射、运行时 reader contract 渲染、命名默认值和测试都在该目录内维护。新增内置语言时应优先通过新增 `src/codegen/<language>/`、实现 crate 内部 `CodeGenerator` 并注册到 builtin registry 完成，不回到 IR 或 build 编排层增加语言分支，也不在核心 `mod.rs` 中新增语言常量或语言 options。

每个语言目录的 `mod.rs` 只负责实现 `CodeGenerator`、构造 `CodeModel`、调用语言内各渲染模块并写出生成文件；实际渲染逻辑按 runtime contract、类型声明、reader、repository 和符号解析拆分。当前 `generate_to_sink()` 是 build 调用的标准入口，内置 backend 均通过 `ArtifactSink` 和 streaming writer 直接写出 artifact；legacy `generate() -> Vec<GeneratedFile>` 已降级为 `#[cfg(test)]` 快照 / 等价性测试 helper，不参与 production build。类型映射当前随 `types.rs` / `reader.rs` 保持在语言目录内；若单一语言继续膨胀，可继续在该语言目录内拆出 `type_mapping.rs`、`index.rs` 等，不应把语言专属逻辑回流到核心 `src/codegen/mod.rs`。

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

`CodeGenerator` 的 crate-internal build-facing API 是 `generate_to_sink(input, sink)`：Build 只负责构造 `CodeExportInput` 并传入 shared `ArtifactSink`，不再遍历 `Vec<GeneratedFile>` 或了解生成文件的 materialization 方式。该 trait 不是稳定外部 generator plugin API，因此可以依赖内部 `ArtifactSink`；crate root 不 re-export `CodeGenerator`、builtin registry 或具体 generator 类型。`generate()` / `GeneratedFile` 仅在测试构建中存在，用于 String-backed snapshot 与 sink-backed streaming 输出等价性校验。

### 9.3.1 ObjectTable codegen 边界

ObjectTable Step 1 的 codegen 语义并入 Codegen 模块：

- `CodeTable.kind` 区分 `Object` / `Collection`。
- ObjectTable 生成目标语言中的 singleton object 类型和访问入口。
- ObjectTable 不生成 collection repository，不生成 index lookup helpers。
- ObjectTable 可以携带 `CodeForeignKey` 关系元数据，用于生成从 singleton object 字段跳转到 CollectionTable repository 的 helper；当前 Rust/C++/TypeScript 生成器已渲染 `linked_field` helper/cache/link。
- Rust / TypeScript / C++ 都应避免把 ObjectTable 误表达为 rows collection。
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
| Generator API and tests | [`09h-codegen-generator-api-and-tests.md`](09h-codegen-generator-api-and-tests.md) | `CodeGenerator` 接口、新语言接入步骤、文件组织、诊断和测试计划。 |
| Lua embedded data generator | [`09i-lua-embedded-data-generator.md`](09i-lua-embedded-data-generator.md) | 已实现的 Lua data+code 合一导出、每表 `.lua`、lazy require、row array + metatable、索引和实现说明。 |
| C# backend | [`09k-codegen-csharp-backend.md`](09k-codegen-csharp-backend.md) | 规划中的 C# backend、generated API、runtime reader contract、index/relation/text/lazy 设计和测试策略。 |

### 9.4.1 当前实现状态摘要

- Rust / C++ / TypeScript backend 均已实现 MVP。
- Unreal C++ 已作为独立 `cpp_unreal` backend 实现；不作为标准 `cpp` backend 的开关，以避免生成 public API 同时混用 `std::*` 和 Unreal API。当前支持 `support = "containers"` 和 `support = "blueprint"` 两级输出：containers 模式生成 Unreal 类型/容器、repository、index、relation、text、`GameData` 与 `FLazyGameData`；Blueprint 模式额外生成 `USTRUCT(BlueprintType)` / `UENUM(BlueprintType)` / `UPROPERTY(BlueprintReadOnly)` 和 `UBlueprintFunctionLibrary` 查询 facade。
- Eager loading 和 table-level `LazyGameData` 均已实现 MVP；lazy facade 保留 loader，首次访问 table/object 时加载并缓存。
- TypeScript 已生成 `loadSync`、lazy sync accessor、`preloadAllSync` 与 `linkAllSync`；同步 API 要求 loader 同步返回 reader，遇到 Promise 应报错。
- C# backend 处于设计规划阶段，目标是新增 `type = "csharp"`，生成 .NET BCL-only 的单文件 `generated.cs`，并逐步追齐 Rust / C++ / TypeScript backend 的类型、index、relation、text、eager/lazy facade 能力。
- 生成代码使用逻辑 table name，不绑定某个 `targets.exports` runtime data export task 的真实 artifact path。
- `linked_field` relation helper 和 `text` translation helper 已纳入 Rust / C++ / TypeScript 生成器。
- Lua embedded data generator 已作为 `type = "lua_table"` 的 MetaAndData exporter 落地，而不是现有 format-agnostic `CodeGenerator` backend。
- `preserve_source_order` 的 generated access 语义仍为规划项：目标行为是 indexed CollectionTable 默认不生成 public source-order `rows` API；只有显式声明 `preserve_source_order = true` 的表生成 row count / row_at / rows 访问，服务随机角色名这类按表格顺序随机访问的配置。

## 9.5 总览文档维护规则

本文只保留以下内容：

1. Codegen 模块目标、流程位置和模块边界。
2. ObjectTable 等跨专题边界的短说明。
3. 拆分专题索引和当前实现状态摘要。

如果需要新增正文，应优先落到 `09a`~`09h` 的专题文档；只有影响整个 Codegen 模块入口、依赖方向或专题划分时才修改本文。这样可以避免总览文档重新膨胀，保证 AI 和人类读者能在一次阅读内获得导航。
