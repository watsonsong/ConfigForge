# Java Codegen Backend 实现 TODO

本文是 `java` 代码生成器的分步实现计划。稳定设计见 [`../Implementation/09l-codegen-java-backend.md`](../Implementation/09l-codegen-java-backend.md)。本文放在 `Documentation/Todo/`，用于推进实现；每一步都应保持仓库可构建、测试可运行，并在完成后单独提交。

## 全局目标

- 新增独立 codegen type：`java`。
- 内部生成器命名为 `JavaCodeGenerator`。
- 第一阶段输出单文件 `GeneratedGameConfig.java`，可通过 `javac` 编译。
- 生成代码只依赖 JDK 标准库和 generated runtime contract，不绑定 JSON / MessagePack / Android / Spring / libGDX。
- 采用 public root class + nested public static generated types，规避 Java 单文件 public top-level type 限制。
- 功能目标追齐现有 Rust / C++ / TypeScript / C# backend MVP：类型、loader、repository、index、relation、text、eager/lazy facade。
- standard_project 后续加入长期 Java regression target 和 harness。

## 全局验收规则

每个步骤完成后都要执行并通过：

1. `cargo fmt`
2. 与本步骤相关的 unit test，例如 `cargo test codegen::java` 或更窄的测试名。
3. 与本步骤相关的 integration / harness test。
4. 修改非文档 Rust 代码后运行 `cargo test`；具备本地 clippy wrapper 时运行 `cargo lint`。
5. 如果本机有 `javac` / `java`，涉及生成 Java 语法/运行时的步骤必须跑 Java compile / smoke harness；没有 JDK 时记录跳过原因，但 Rust 测试不能跳过。
6. 每个步骤完成后单独 git commit，避免大提交。

## [x] Step 0：文档计划落地

目标：先固化 Java backend 的设计文档和分步 TODO，避免实现时边做边改方向。

主要修改：

- 新增 `Documentation/Implementation/09l-codegen-java-backend.md`。
- 新增 `Documentation/Todo/codegen-java-backend-todo.md`。
- 更新 `Documentation/README.md` 的 codegen 文档索引。
- 更新 `Documentation/Implementation/09-codegen-module-detail-design.md` 的专题索引和当前状态摘要。
- 更新 `Documentation/Design/06-export-cli.md`，声明 Java backend 是规划项且尚未注册。

验证：

- `git diff --check`
- 人工检查设计文档是否覆盖 options、类型映射、runtime contract、index、relation、text、lazy 和测试策略。

建议提交：

```text
docs(codegen): plan Java backend implementation
```

## [x] Step 1：注册 `java` 生成器骨架

目标：让 build / export registry 识别 `type = "java"`，先生成最小 `.java` 文件。

主要修改：

- 新增 `src/codegen/java/`：`mod.rs`、`types.rs`、`symbols.rs`、`tests.rs`。
- `src/codegen/mod.rs` 注册 `JavaCodeGenerator`。
- 定义 `JavaCodegenOptions`：
  - `package: Option<String>`；
  - `root_class_name: Option<String>`；
  - `game_data_class_name: Option<String>`；
  - `lazy_class_name: Option<String>`；
  - `access_modifier: String`，默认 `public`；
  - `emit_final: bool`，默认 `true`。
- 默认输出 `GeneratedGameConfig.java`，支持 `file_name = "ReferenceGameConfig"` / `file_name = "ReferenceGameConfig.java"`。
- 最小输出包含 file header、package、root class、runtime exception skeleton。

单元测试：

- `CodegenOptions::for_language("java")` 成功。
- `CodeExportTarget::from_task` 能读取 `[targets.exports.java]`。
- 非法 package / access modifier / root class name 报 validation error。
- `file_name` stem 与 `root_class_name` 不一致时报 validation error。
- `generate()` 返回一个 `.java` 文件。
- `generate_to_sink()` 与 `generate()` 输出等价。

集成测试：

- 临时 project CLI smoke：`type = "java"` 能生成 `.java`。

建议提交：

```text
feat(codegen): register Java generator skeleton
```

## [ ] Step 2：Java 命名、类型映射和 symbol collision

目标：建立 Java backend 的命名和类型系统，但暂不生成完整加载逻辑。

主要修改：

- 实现 `JavaNames`：root class、nested type、getter、method、field、constant 命名。
- Java keyword set 和冲突处理。
- 类型映射：`int -> long`、`float -> double`、`bool -> boolean`、`string/text/asset -> String`、`list<T> -> T[]` / primitive array、optional primitive -> boxed、optional reference -> nullable reference。
- enum 输出为 nested enum，enum value 默认 UPPER_SNAKE_CASE。
- value struct / row / object class 输出 private final fields + getter。

单元测试：

- 类型映射 snapshot 覆盖 primitive、list、optional、enum、struct、text、asset。
- Java keyword 和重复 getter / 字段名 collision 报错。
- optional primitive 输出 boxed type，required string constructor 参数和 getter 正确。
- root class 与 nested generated type 冲突时报错。

集成测试：

- 如本机有 `javac`，编译只含类型的 generated `.java`。

建议提交：

```text
feat(codegen): add Java naming and type mapping
```

## [ ] Step 3：runtime reader contract、row/object reader 和 eager `GameData`

目标：生成可从 runtime data reader 读取 CollectionTable / ObjectTable 的 Java 代码。

主要修改：

- 生成 `ConfigForgeRuntimeDataReader`、`ConfigForgeRuntimeDataLoader`、`ConfigForgeRuntimeException`。
- 生成 enum / struct / row reader。
- CollectionTable 读取 rows 并创建 repository。
- ObjectTable 读取 singleton row/object。
- 生成 `GameData.load(loader)`。
- 不绑定具体 JSON / MessagePack decoder。

单元测试：

- source 包含 `enterRequiredField`、`enterOptionalField`、`readRowCount`、`readListLength` 等 reader 调用。
- ObjectTable 不生成 table repository，只生成 row/object 和 `GameData` getter。
- required / optional 字段读取分支正确。
- runtime exception 包装 table / row / field context。

集成测试：

- 临时 Java harness：generic in-memory reader / loader 编译并运行 `GameData.load`，验证字段值。

建议提交：

```text
feat(codegen): load Java rows and game data
```

## [ ] Step 4：unordered index 和 compound key

目标：支持 unique / non-unique hash lookup。

主要修改：

- unique index：`Map<TKey, Integer>`。
- non-unique index：`Map<TKey, int[]>`。
- `fromRows` 构建索引并检查 duplicate unique key。
- compound key：生成 static final key class，实现 `equals`、`hashCode()`。
- public lookup：
  - unique：`Row getById(long id)`，miss 返回 `null`；
  - non-unique：`List<Row> findByRarity(ItemRarity rarity)`。

单元测试：

- 单字段 / 复合字段 index 输出正确。
- primitive key boxing 正确。
- duplicate unique key error 分支存在。
- non-unique 构建 `List<Integer>` 后冻结为 `int[]`。

集成测试：

- Java harness 调用 `getById`、`findBy*`，验证命中和 miss。

建议提交：

```text
feat(codegen): generate Java hash indexes
```

## [ ] Step 5：ordered index 和 range API

目标：支持 Meta `ordered = true` 的 lower / upper / range 查询。

主要修改：

- ordered index 存储 per-index sorted entry array。
- 生成 binary search helper，不依赖 stream / lambda 分配。
- exact lookup 使用 lower bound。
- range API：`lowerBoundBy*`、`upperBoundBy*`、`rangeBy*`。
- compound key ordered 需要实现 `Comparable<T>`。

单元测试：

- ordered storage、sort、binary search source 断言。
- ordered unique duplicate key 检查。
- range API 签名正确。

集成测试：

- Java harness 验证 exact / lower / upper / range 顺序。

建议提交：

```text
feat(codegen): support ordered Java indexes
```

## [ ] Step 6：relation helper 和 `GameDataAccess`

目标：生成 linked_field relation helper，并让 eager/lazy 共享访问接口。

主要修改：

- 生成 `GameDataAccess`。
- `GameData` 实现该接口。
- Row/Object 为每个 linked FK 生成 relation method 和 row-index cache。
- Table/Object 生成 `link(GameDataAccess data)`。
- `GameData.load` 完成加载后调用 `linkAll()`。
- optional / list FK 语义与其他 backend 保持一致。

单元测试：

- required / optional / list relation helper source 断言。
- cache sentinel / list cache 初始化正确。
- `linkAll` 按 CodeModel table 顺序生成。
- relation helper 接收 `GameDataAccess`，不硬编码 `GameData`。

集成测试：

- Java harness 覆盖 `game_rules.default_class_id -> classes.by_id` 等 relation。

建议提交：

```text
feat(codegen): add Java relation helpers
```

## [ ] Step 7：text helper 和 i18n options

目标：生成 Java text translation helper，支持 generic translator 和 global function 两种模式。

主要修改：

- generic 模式生成 `TextTranslator`。
- 对每个 `text` 字段生成 `fieldText(...)` helper。
- global function 模式支持 `function`、`returns`，并增加 `java_import` option。
- helper 缺失翻译时 fallback raw text。

单元测试：

- generic translator helper source。
- global function helper source。
- optional text helper nullable / fallback 语义符合设计。
- helper 名与 getter、relation、index method 冲突时 validation error。

集成测试：

- Java harness 验证 translator 返回翻译和 fallback。

建议提交：

```text
feat(codegen): add Java text helpers
```

## [ ] Step 8：`LazyGameData`

目标：生成同步 lazy facade，支持按需加载和 `preloadAll()`。

主要修改：

- 生成 `LazyGameData implements GameDataAccess`。
- 每个 table/object 生成 nullable cache 字段和 getter。
- getter 加载失败时不写 cache，允许下次重试。
- `preloadAll()` 返回 eager `GameData` 并调用 `linkAll()`。
- relation helper 可直接使用 lazy access interface。

单元测试：

- lazy cache 字段、getter、retry 语义 source 断言。
- `preloadAll()` 生成完整 eager data。
- relation helper 同时接受 `GameData` / `LazyGameData`。

集成测试：

- Java harness 验证 lazy 首次访问、重复访问不重复 load、`preloadAll()`。

建议提交：

```text
feat(codegen): add Java lazy game data
```

## [ ] Step 9：standard_project target 和 Java harness

目标：将 Java backend 纳入长期集成回归。

主要修改：

- 在 `tests/fixtures/standard_project/ConfigForge.toml` 的 client target 增加：

```toml
[[targets.exports]]
type = "java"
output = "out/client_code/java"
file_name = "ReferenceGameConfig"
namespace = "referencegame.config"

[targets.exports.java]
package = "referencegame.config"
root_class_name = "ReferenceGameConfig"
access_modifier = "public"
```

- 更新 `tests/cli_integration/standard_project.rs`，断言生成 `.java` 文件存在。
- 新增 Java standard harness：临时编译目录 + generated code + generic in-memory reader，编译运行。
- 没有 `javac` / `java` 时跳过 Java compiler harness，但保留生成内容断言。

单元测试：

- standard_project 配置解析包含 Java target。

集成测试：

- `cargo test standard_project_generated_code_loads_exported_runtime_data`
- 本机有 JDK 时 Java harness 运行通过。

建议提交：

```text
test(fixtures): add standard Java codegen target
```

## [ ] Step 10：文档收敛和最终回归

目标：把 Java backend 从规划状态更新为当前实现状态，补齐用户可见文档和测试说明。

主要修改：

- 更新 `Documentation/Design/06-export-cli.md`：加入 `type = "java"` 和 `[targets.exports.java]` options。
- 更新 `Documentation/Design/README.md`：当前状态摘要从规划改为已实现 MVP。
- 更新 `Documentation/Implementation/09-codegen-module-detail-design.md`：当前状态摘要从规划改为已实现 MVP。
- 更新 `Documentation/Implementation/09l-codegen-java-backend.md`：同步最终 API 与限制。
- 更新 `Documentation/Implementation/09h-codegen-generator-api-and-tests.md`：加入 Java harness。
- 如新增 runtime adapter 说明，更新 `Documentation/Implementation/09e-codegen-runtime-adapters.md`。

验证：

- `cargo fmt`
- `cargo test codegen::java`
- `cargo test --test cli_integration`
- `cargo test`
- `cargo lint`（本机可用时）
- `javac` / `java` harness（本机可用时）

建议提交：

```text
docs(codegen): document Java backend status
```

## 不能做的事

- 不能让 Java generated core 绑定具体 JSON / MessagePack / Android / Spring / libGDX API。
- 不能要求普通 Rust CI 必须安装 JDK 才能通过核心测试；Java harness 应可跳过。
- 不能生成多个 public top-level type 到同一个 `.java` 文件。
- 不能把 mutable rows collection 暴露为 public API 后还依赖 index cache 不变。
- 不能在 relation helper 中硬编码 eager `GameData`，必须通过 generated access interface 支持 lazy。
- 不能让 Java backend 复用 C++/TypeScript/C# emitter 的字符串拼接片段导致命名、nullable 或错误模型不符合 Java 习惯。
