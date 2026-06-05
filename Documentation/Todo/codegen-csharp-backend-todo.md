# C# Codegen Backend 实现 TODO

本文是 `csharp` 代码生成器的分步实现计划。稳定设计见 [`../Implementation/09k-codegen-csharp-backend.md`](../Implementation/09k-codegen-csharp-backend.md)。本文放在 `Documentation/Todo/`，用于推进实现；每一步都应保持仓库可构建、测试可运行，并在完成后单独提交。

## 全局目标

- 新增独立 codegen type：`csharp`。
- 内部生成器命名为 `CSharpCodeGenerator`。
- 第一阶段输出单文件 `generated.cs`，可通过 .NET compiler 编译。
- 生成代码只依赖 .NET BCL 和 generated runtime contract，不绑定 JSON / MessagePack / Unity / Godot。
- 功能目标追齐现有 Rust / C++ / TypeScript backend MVP：类型、loader、repository、index、relation、text、eager/lazy facade。
- standard_project 后续加入长期 C# regression target 和 harness。

## 全局验收规则

每个步骤完成后都要执行并通过：

1. `cargo fmt`
2. 与本步骤相关的 unit test，例如 `cargo test codegen::csharp` 或更窄的测试名。
3. 与本步骤相关的 integration / harness test。
4. 修改非文档 Rust 代码后运行 `cargo test`；具备本地 clippy wrapper 时运行 `cargo lint`。
5. 如果本机有 `dotnet`，涉及生成 C# 语法/运行时的步骤必须跑 C# compile / smoke harness；没有 `dotnet` 时记录跳过原因，但 Rust 测试不能跳过。
6. 每个步骤完成后单独 git commit，避免大提交。

## [x] Step 0：文档计划落地

目标：先固化 C# backend 的设计文档和分步 TODO，避免实现时边做边改方向。

主要修改：

- 新增 `Documentation/Implementation/09k-codegen-csharp-backend.md`。
- 新增 `Documentation/Todo/codegen-csharp-backend-todo.md`。
- 更新 `Documentation/README.md` 的 codegen 文档索引。
- 更新 `Documentation/Implementation/09-codegen-module-detail-design.md` 的专题索引和当前状态摘要。

验证：

- `git diff --check`
- 人工检查设计文档是否覆盖 options、类型映射、runtime contract、index、relation、text、lazy 和测试策略。

建议提交：

```text
docs(codegen): plan C# backend implementation
```

## [x] Step 1：注册 `csharp` 生成器骨架

目标：让 build / export registry 识别 `type = "csharp"`，先生成最小 `.cs` 文件。

主要修改：

- 新增 `src/codegen/csharp/`：`mod.rs`、`types.rs`、`symbols.rs`、`tests.rs`。
- `src/codegen/mod.rs` 注册 `CSharpCodeGenerator`。
- 定义 `CSharpCodegenOptions`：
  - `namespace: Option<String>`；
  - `nullable: bool`，默认 `true`；
  - `access_modifier: String`，默认 `public`；
  - `class_name: Option<String>`；
  - `lazy_class_name: Option<String>`；
  - `emit_partial: bool`，默认 `true`。
- 默认输出 `generated.cs`，支持 `file_name = "reference_game_config"` / `file_name = "ReferenceGameConfig.cs"`。
- 最小输出包含 copyright header、`#nullable enable`、namespace、runtime exception skeleton。

单元测试：

- `CodegenOptions::for_language("csharp")` 成功。
- `CodeExportTarget::from_task` 能读取 `[targets.exports.csharp]`。
- 非法 namespace / access modifier / class name 报 validation error。
- `generate()` 返回一个 `.cs` 文件。
- `generate_to_sink()` 与 `generate()` 输出等价。

集成测试：

- 临时 project CLI smoke：`type = "csharp"` 能生成 `.cs`。

建议提交：

```text
feat(codegen): register C# generator skeleton
```

## [x] Step 2：C# 命名、类型映射和 symbol collision

目标：建立 C# backend 的命名和类型系统，但暂不生成完整加载逻辑。

主要修改：

- 实现 `CSharpNames`：类型、property、method、field、constant 命名。
- C# keyword set 和冲突处理。
- 类型映射：`int -> long`、`float -> double`、`string/text/asset -> string`、`list<T> -> T[]`、optional -> nullable。
- enum 输出。
- value struct / row / object class 输出 get-init properties。

单元测试：

- 类型映射 snapshot 覆盖 primitive、list、optional、enum、struct、text、asset。
- C# keyword 和重复字段名 collision 报错。
- nullable enabled 时 optional reference 输出 `string?`，required string 初始化为 `string.Empty` 或构造时赋值。

集成测试：

- 如本机有 `dotnet`，编译只含类型的 generated `.cs`。

建议提交：

```text
feat(codegen): add C# naming and type mapping
```

## [x] Step 3：runtime reader contract、row/object reader 和 eager `GameData`

目标：生成可从 runtime data reader 读取 CollectionTable / ObjectTable 的 C# 代码。

主要修改：

- 生成 `IConfigForgeRuntimeDataReader`、`IConfigForgeRuntimeDataLoader`、`ConfigForgeRuntimeException`。
- 生成 enum / struct / row reader。
- CollectionTable 读取 rows 并创建 repository。
- ObjectTable 读取 singleton row/object。
- 生成 `GameData.Load(loader)`。
- 不绑定具体 JSON / MessagePack decoder。

单元测试：

- source 包含 `EnterRequiredField`、`EnterOptionalField`、`ReadRowCount`、`ReadListLength` 等 reader 调用。
- ObjectTable 不生成 table repository，只生成 row/object 和 `GameData` 属性。
- required / optional 字段读取分支正确。

集成测试：

- 临时 C# harness：mock reader / loader 编译并运行 `GameData.Load`，验证字段值。

建议提交：

```text
feat(codegen): load C# rows and game data
```

## [x] Step 4：unordered index 和 compound key

目标：支持 unique / non-unique hash lookup。

主要修改：

- unique index：`Dictionary<TKey, int>`。
- non-unique index：`Dictionary<TKey, int[]>`。
- `FromRows` 构建索引并检查 duplicate unique key。
- compound key：生成 readonly struct，实现 `IEquatable<T>`、`GetHashCode()`。
- public lookup：
  - unique：`Row? GetById(long id)`；
  - non-unique：`IReadOnlyList<Row> FindByRarity(ItemRarity rarity)`。

单元测试：

- 单字段 / 复合字段 index 输出正确。
- duplicate unique key error 分支存在。
- non-unique 构建 `List<int>` 后冻结为 array。

集成测试：

- C# harness 调用 `GetById`、`FindBy*`，验证命中和 miss。

建议提交：

```text
feat(codegen): generate C# hash indexes
```

## [x] Step 5：ordered index 和 range API

目标：支持 Meta `ordered = true` 的 lower / upper / range 查询。

主要修改：

- ordered index 存储 sorted entry array。
- 生成 binary search helper，不依赖 LINQ 分配。
- exact lookup 使用 lower bound。
- range API：`LowerBoundBy*`、`UpperBoundBy*`、`RangeBy*`。
- compound key ordered 需要实现 `IComparable<T>`。

单元测试：

- ordered storage、sort、binary search source 断言。
- ordered unique duplicate key 检查。
- range API 签名正确。

集成测试：

- C# harness 验证 exact / lower / upper / range 顺序。

建议提交：

```text
feat(codegen): support ordered C# indexes
```

## [ ] Step 6：relation helper 和 `IGameDataAccess`

目标：生成 linked_field relation helper，并让 eager/lazy 共享访问接口。

主要修改：

- 生成 `IGameDataAccess`。
- `GameData` 实现该接口。
- Row/Object 为每个 linked FK 生成 relation method 和 row-index cache。
- Table/Object 生成 `Link(IGameDataAccess data)`。
- `GameData.Load` 完成加载后调用 `LinkAll()`。
- optional / list FK 语义与其他 backend 保持一致。

单元测试：

- required / optional / list relation helper source 断言。
- cache sentinel / list cache 初始化正确。
- `LinkAll` 按 CodeModel table 顺序生成。
- relation helper 接收 `IGameDataAccess`，不硬编码 `GameData`。

集成测试：

- C# harness 覆盖 `game_rules.default_class_id -> classes.by_id` 等 relation。

建议提交：

```text
feat(codegen): add C# relation helpers
```

## [ ] Step 7：text helper 和 i18n options

目标：生成 C# text translation helper，支持 generic translator 和 global function 两种模式。

主要修改：

- generic 模式生成 `ITextTranslator`。
- 对每个 `text` 字段生成 `FieldText(...)` helper。
- global function 模式支持 `function`、`returns`，并增加 / 复用 `csharp_using` option。
- helper 缺失翻译时 fallback raw text。

单元测试：

- generic translator helper source。
- global function helper source。
- optional text helper 返回 `string?` 或 fallback 语义符合设计。
- helper 名与字段、relation、index method 冲突时 validation error。

集成测试：

- C# harness 验证 translator 返回翻译和 fallback。

建议提交：

```text
feat(codegen): add C# text helpers
```

## [ ] Step 8：`LazyGameData`

目标：生成同步 lazy facade，支持按需加载和 `PreloadAll()`。

主要修改：

- 生成 `LazyGameData : IGameDataAccess`。
- 每个 table/object 生成 nullable cache 字段和 property getter。
- getter 加载失败时不写 cache，允许下次重试。
- `PreloadAll()` 返回 eager `GameData` 并调用 `LinkAll()`。
- relation helper 可直接使用 lazy access interface。

单元测试：

- lazy cache 字段、getter、retry 语义 source 断言。
- `PreloadAll()` 生成完整 eager data。
- relation helper 同时接受 `GameData` / `LazyGameData`。

集成测试：

- C# harness 验证 lazy 首次访问、重复访问不重复 load、`PreloadAll()`。

建议提交：

```text
feat(codegen): add C# lazy game data
```

## [ ] Step 9：standard_project target 和 C# harness

目标：将 C# backend 纳入长期集成回归。

主要修改：

- 在 `tests/fixtures/standard_project/ConfigForge.toml` 的 client target 增加：

```toml
[[targets.exports]]
type = "csharp"
output = "out/client_code/csharp"
file_name = "reference_game_config"
namespace = "ReferenceGame.Config"

[targets.exports.csharp]
nullable = true
access_modifier = "public"
```

- 更新 `tests/cli_integration/standard_project.rs`，断言生成 `.cs` 文件存在。
- 新增 C# standard harness：临时 `.csproj` + generated code + mock / runtime reader，编译运行。
- 没有 `dotnet` 时跳过 C# compiler harness，但保留生成内容断言。

单元测试：

- standard_project 配置解析包含 C# target。

集成测试：

- `cargo test standard_project_generated_code_loads_exported_runtime_data`
- 本机有 `dotnet` 时 C# harness 运行通过。

建议提交：

```text
test(fixtures): add standard C# codegen target
```

## [ ] Step 10：文档收敛和最终回归

目标：把 C# backend 从规划状态更新为当前实现状态，补齐用户可见文档和测试说明。

主要修改：

- 更新 `Documentation/Design/06-export-cli.md`：加入 `type = "csharp"` 和 `[targets.exports.csharp]` options。
- 更新 `Documentation/Implementation/09-codegen-module-detail-design.md`：当前状态摘要从规划改为已实现 MVP。
- 更新 `Documentation/Implementation/09k-codegen-csharp-backend.md`：同步最终 API 与限制。
- 更新 `Documentation/Implementation/09h-codegen-generator-api-and-tests.md`：加入 C# harness。
- 如新增 runtime adapter 说明，更新 `Documentation/Implementation/09e-codegen-runtime-adapters.md`。

验证：

- `cargo fmt`
- `cargo test codegen::csharp`
- `cargo test --test cli_integration`
- `cargo test`
- `cargo lint`（本机可用时）
- `dotnet test` / `dotnet run` harness（本机可用时）

建议提交：

```text
docs(codegen): document C# backend status
```

## 不能做的事

- 不能让 C# generated core 绑定具体 JSON / MessagePack / Unity API。
- 不能要求普通 Rust CI 必须安装 dotnet 才能通过核心测试；dotnet harness 应可跳过。
- 不能把 mutable rows collection 暴露为 public API 后还依赖 index cache 不变。
- 不能在 relation helper 中硬编码 eager `GameData`，必须通过 generated access interface 支持 lazy。
- 不能让 C# backend 复用 C++/TypeScript emitter 的字符串拼接片段导致命名、nullable 或错误模型不符合 C# 习惯。
