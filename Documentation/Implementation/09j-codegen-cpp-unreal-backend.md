# 09j. cpp_unreal backend 实现设计

本文描述 `cpp_unreal` 生成器的实现设计、当前能力边界和已执行的阶段性实现记录。新的 TODO / 分步任务清单应放在 `Documentation/Todo/`；本文不再作为待办清单维护，而是作为 `src/codegen/cpp_unreal/` backend 的实现设计文档。

## 当前实现状态

- `cpp_unreal` 是独立 codegen type，内部生成器为 `CppUnrealCodeGenerator`，不作为标准 `cpp` backend 的开关。
- `support = "containers"` 已支持 Unreal 容器和基础类型：`TArray`、`TMap`、`TPair`、`TOptional`、`TUniquePtr`、`FString`、`int32` / `int64`，并生成 row/object、repository、index、relation、text helper、`FGameData` 与 `FLazyGameData`。
- `support = "blueprint"` 已在 containers 模式基础上生成 `USTRUCT(BlueprintType)`、`UENUM(BlueprintType)`、required 字段 `UPROPERTY(BlueprintReadOnly)`、`.generated.h` include 和 `UBlueprintFunctionLibrary` 查询 facade；optional 字段仍生成为 `TOptional<T>`，但不直接生成 `UPROPERTY`。
- `FGameData` 与 `FLazyGameData` 通过生成的 `I<Prefix>ConfigForgeGameDataAccess` 共享只读表访问接口；relation helper 接收该公共接口，避免只支持 eager `FGameData`。未配置 `type_name_prefix` 时接口名为 `IConfigForgeGameDataAccess`。
- Blueprint 模式禁止 namespace，避免真实 UHT 解析 namespace 内反射类型失败；需要命名隔离时配置 `type_name_prefix`，该前缀会应用到所有生成类型名，例如 `FReferenceItemsRow`、`FReferenceGameData`、`FReferenceLazyGameData`、`IReferenceConfigForgeGameDataAccess`、`UReferenceConfigForgeBlueprintLibrary`。
- 标准工程已包含 `cpp_unreal` target，普通 CI 通过 lightweight Unreal shim 做语法 smoke；真实 UHT / Editor 集成验证留给引擎侧 CI。

## 历史实施记录

以下内容保留为该 backend 的阶段性实施记录，用于追溯设计意图、测试覆盖和提交粒度；后续新的 TODO 不应继续追加到本文。

## 目标边界

- 新增独立 codegen type：`cpp_unreal`，内部生成器命名为 `CppUnrealCodeGenerator`。
- 不把 Unreal 支持做成标准 `cpp` backend 的开关，避免 public API 混用 `std::*` 与 Unreal API。
- 第一阶段支持 `support = "containers"`：生成 Unreal 容器风格 C++，但不依赖 UnrealHeaderTool。
- 第二阶段支持 `support = "blueprint"`：在 containers 模式基础上生成 Blueprint 反射层。
- 标准工程 `tests/fixtures/standard_project/ConfigForge.toml` 最终要增加一个 client 侧 `cpp_unreal` 代码生成 export target，输出到 `out/client_code/cpp_unreal`。
- 集成测试不能要求本机安装 Unreal Engine。harness 使用轻量 Unreal compatibility shim 覆盖 containers 模式编译与运行；Blueprint 模式先做生成结果和 shim 编译 smoke，真正 UHT / Editor 集成留给后续引擎侧 CI。

## 全局验收规则

每个步骤完成后都要执行并通过：

1. `cargo fmt`
2. 与本步骤相关的 unit test，例如 `cargo test codegen::cpp_unreal` 或更窄的测试名。
3. 与本步骤相关的 integration / harness test：
   - 早期步骤至少覆盖 CLI 生成 smoke 或 shim 编译 smoke；
   - 引入 standard_project target 后必须覆盖 standard_project `cpp_unreal` harness；
   - 后续修改不得让已有 Rust / C++ / TypeScript / Lua standard_project harness 回退。
4. 如修改非文档 Rust 代码，运行 `cargo test`；具备本地 clippy wrapper 时运行 `cargo lint`。
5. 通过后立即单独 `git commit`，commit message 使用下方每步建议或同等语义。

## Step 0：文档和基线提交

目标：先把当前 Unreal C++ 设计文档与本 TODO 文档固化，避免实现过程中混入设计变更。

主要修改：

- `Documentation/Design/06-export-cli.md`
- `Documentation/Implementation/09-codegen-module-detail-design.md`
- `Documentation/Implementation/09c-codegen-cpp-backend.md`
- `Documentation/Implementation/09j-codegen-cpp-unreal-backend.md`

验证：

- `git diff --check`
- 人工检查本文 checklist 是否覆盖 unit test、harness、standard_project target 和单步提交要求。

建议提交：

```text
docs(codegen): plan cpp_unreal implementation steps
```

## Step 1：注册 `cpp_unreal` 生成器骨架

目标：让 build / export registry 能识别 `type = "cpp_unreal"`，但只生成最小可编译骨架文件。

主要修改：

- 新增 `src/codegen/cpp_unreal/` 模块，先包含 `mod.rs`、`types.rs`、`symbols.rs`、`tests.rs`。
- `src/codegen/mod.rs` 注册 `CppUnrealCodeGenerator` 到 `builtin_generators()`。
- 定义 `CppUnrealCodegenOptions`：
  - `support: CppUnrealSupportLevel`，默认 `containers`；
  - `api_macro: Option<String>`；
  - 预留 `blueprint_category: Option<String>`；
  - 对未知字段、非法 support、非法 api macro 给出 validation diagnostic。
- 生成默认文件名：`generated.h` / `generated.cpp`；有 `file_name = "reference_game_config"` 时生成 `reference_game_config.h` / `reference_game_config.cpp`。
- containers 模式最小输出只包含版权头、`#pragma once`、基础 include 和空 namespace / root facade 声明；不生成业务 rows。

单元测试：

- `CodegenOptions::for_language("cpp_unreal")` 成功。
- `CodeExportTarget::from_task` 能读取 `[targets.exports.cpp_unreal] support = "containers"`。
- `CppUnrealCodeGenerator.generate()` 返回 `.h` / `.cpp` 两个文件。
- 非法 `support`、非法 `api_macro` 报错。
- streaming `generate_to_sink()` 与 `generate()` 在测试中输出等价。

集成 / harness：

- 增加一个临时 project 的 CLI smoke test：`type = "cpp_unreal"` 能生成 `.h` / `.cpp`，但暂不放入 standard_project，避免后续未完成功能影响标准 fixture。

建议提交：

```text
feat(codegen): register cpp_unreal generator skeleton
```

## Step 2：Unreal 命名、类型映射和基础 include

目标：建立 Unreal backend 的命名和类型系统，不涉及完整读表逻辑。

主要修改：

- `cpp_unreal/types.rs` 实现 `CppUnrealNames`：
  - struct：`F<Item>Row` / `F<GameRules>`；
  - enum：`E<ItemRarity>`；
  - table repository：`F<Items>Table`；
  - root facade：`FReferenceGameConfig` 或 `FGameData`；
  - bool 字段使用 `b` 前缀；其他字段 PascalCase。
- 类型映射：
  - `int` -> `int64`；
  - `float` -> `double`；
  - `bool` -> `bool`；
  - `string` / `text` -> `FString`；
  - `list<T>` -> `TArray<T>`；
  - optional -> `TOptional<T>`。
- containers 模式 include 使用 `CoreMinimal.h` 或明确的 Unreal Core include；禁止生成 `std::vector`、`std::unordered_map`、`std::string`、`std::optional`。
- 输出 `.cpp` include 自身 header。

单元测试：

- 类型映射 snapshot：覆盖 enum、struct、bool 字段、list、optional、text。
- symbol collision：Unreal 前缀后重复的 type / field / method 能报错。
- 生成内容断言不包含 `std::vector`、`std::unordered_map`、`std::string`、`throw std::runtime_error`。

集成 / harness：

- CLI smoke test 增加 `c++ -fsyntax-only` 或等价编译 smoke，使用测试专用 shim 提供 `CoreMinimal.h`、`FString`、`TArray`、`TOptional` 的最小定义。

建议提交：

```text
feat(codegen): add Unreal naming and type mapping
```

## Step 3：containers 模式生成 row/object 读取和 eager `GameData`

目标：`cpp_unreal` 能读取 collection table 与 object table，构造 typed rows / objects，但暂不生成索引。

主要修改：

- 生成 Unreal 风格 runtime contract：
  - `IConfigForgeRuntimeDataReader`；
  - `IConfigForgeRuntimeDataLoader`；
  - `FConfigForgeError` / `FConfigForgeStatus` 或 `TValueOrError` 等统一错误返回模型。
- 不使用 C++ exceptions；所有 decode/schema 错误走 status/result。
- 生成 enum reader、struct reader、fragment reader、row reader。
- 生成 `F<GameData>::Load(...)` eager 入口。
- schema revision 检查、required / optional field 读取语义与标准 C++ backend 保持一致。

单元测试：

- `ReadRow` / `ReadObject` 生成包含 `EnterRequiredField`、`EnterOptionalField`、`ReadRowCount` 等通用 reader 调用。
- schema revision mismatch 生成 status error，不生成 `throw`。
- source-order opt-in 表生成 `Rows()` / `Num()` / `RowAt()`；普通 indexed table 暂不暴露 rows public API。

集成 / harness：

- 扩展 `tests/fixtures/standard_project/harness/cpp_unreal/` shim reader，使用 JSON fixture 运行最小加载 smoke：只验证 `GameData::Load` 成功和 object table 字段可读。
- 该 harness 仍使用临时 project 或专门小 fixture，不接入 standard_project export target。

建议提交：

```text
feat(codegen): load Unreal container rows and objects
```

## Step 4：unordered index、复合 key 和 unique / non-unique lookup

目标：containers 模式支持 hash-based exact lookup。

主要修改：

- unique index：`TMap<Key, int32>`。
- non-unique index：`TMap<Key, TArray<int32>>`。
- 复合 key：生成 `F<Table><Index>Key`，包含 `operator==` 和非成员 `GetTypeHash(const FKey&)`。
- enum key、string key、bool key 的 `GetTypeHash` 使用 Unreal hash 约定。
- duplicate unique key 返回 status error。
- lookup API：
  - unique：`const FItemRow* GetById(int64 Id) const`；
  - non-unique：`TArray<const FItemRow*> FindByRarity(EItemRarity Rarity) const` 或 backend 内部 view；Blueprint 层不复用 pointer 返回。

单元测试：

- 单字段 index 生成 `TMap<int64, int32>`。
- 复合 key index 生成 key struct、`operator==`、`GetTypeHash`。
- non-unique index 生成 `TMap<Key, TArray<int32>>`。
- 重复 unique key 生成 status error 分支。

集成 / harness：

- cpp_unreal harness 查询 `Items.GetById`、`ClassLevels.GetByClassLevel`、`FindByRarity`，验证与标准 C++ harness 相同业务断言。

建议提交：

```text
feat(codegen): generate Unreal hash indexes
```

## Step 5：ordered index 排序数组和 range API

目标：containers 模式支持 Meta `ordered = true` 语义。

主要修改：

- ordered index 存储：`TArray<TPair<Key, int32>>`。
- Load 阶段 `Emplace(Key, RowIndex)` 后 `StableSort`，不能依赖 `TMap` 的迭代顺序。
- exact lookup 使用 lower-bound 找 equal range。
- range API：`LowerBoundBy*`、`UpperBoundBy*`、`RangeBy*`，半开区间 `[lower, upper)`。
- 若不使用 Unreal `Algo::LowerBoundBy`，则生成局部二分 helper，保证 shim 和真实 UE 都能编译。

单元测试：

- ordered index 生成 `TArray<TPair<...>>`。
- source 包含 stable sort 和 lower/upper bound 逻辑。
- unordered 复合 key 不回退树结构。

集成 / harness：

- 在小 fixture 中加入 `ordered = true` index，harness 验证 lower / upper / range 顺序。
- 若 standard_project 当前没有 ordered index，可暂不改变其 Meta；只保持 unit + 小 fixture harness。

建议提交：

```text
feat(codegen): support ordered Unreal indexes
```

## Step 6：relation helper、text helper 和 `LazyGameData`

目标：containers 模式功能追齐标准 C++ backend 的 runtime API。

主要修改：

- 生成 FK link cache，cache 保存 row index，不保存裸 pointer。
- 必填 FK 返回 `const FTargetRow*` + status 或内部 check 后引用；保持错误模型一致。
- optional / list FK 与标准 backend 语义一致。
- 生成 `Link` / `LinkAll`。
- 生成 `FTextTranslator` 或 global function i18n helper；默认 text helper 返回 `FString`。
- 生成 `FLazyGameData`，保存 loader 生命周期，首次访问加载并缓存。

单元测试：

- `linked_field` helper 生成 cache 字段和 getter。
- `LinkAll` 按确定顺序生成。
- global i18n `cpp_unreal` include / function 配置生效，字符串字面量用 `TEXT()`。
- lazy accessor 不在 `const` 纯 getter 中吞掉 status error。

集成 / harness：

- cpp_unreal harness 覆盖 `game_rules.default_class_id -> classes.by_id` relation。
- 覆盖 text helper fallback 和 global translate 成功路径。
- 覆盖 lazy facade 首次访问和 `PreloadAll`。

建议提交：

```text
feat(codegen): add Unreal relations i18n and lazy data
```

## Step 7：standard_project 引入 `cpp_unreal` target 与 containers harness

目标：把 `cpp_unreal` 纳入标准工程，形成长期回归测试。

主要修改：

- 在 `tests/fixtures/standard_project/ConfigForge.toml` 的 `client` target 中新增：

```toml
[[targets.exports]]
type = "cpp_unreal"
output = "out/client_code/cpp_unreal"
file_name = "reference_game_config"
module_name = "ReferenceGameConfig"
namespace = "ReferenceGameConfig"

[targets.exports.cpp_unreal]
support = "containers"
api_macro = "REFERENCE_GAME_CONFIG_API"

[targets.exports.i18n]
mode = "global_function"
function = "GlobalTranslate"
returns = "optional"
```

- 扩展 `tests/cli_integration.rs::standard_project_generated_code_loads_exported_runtime_data`，断言：
  - `out/client_code/cpp_unreal/reference_game_config.h` 存在；
  - `out/client_code/cpp_unreal/reference_game_config.cpp` 存在；
  - 生成内容不包含 `std::vector` / `std::unordered_map` / `throw`。
- 新增 `verify_standard_generated_cpp_unreal_runtime(fixture_root)`。
- 新增 `tests/fixtures/standard_project/harness/cpp_unreal/`：
  - `CoreMinimal.h` compatibility shim；
  - `runtime_check.cpp`；
  - 必要的 `reference_game_i18n.hpp` 或复用现有 C++ i18n header。
- 更新 `tests/fixtures/standard_project/harness/run.sh`，支持 `cpp_unreal` 参数，并让默认目标包含 `cpp_unreal`。

单元测试：

- standard_project 配置解析包含 `cpp_unreal` export task。
- `cpp_unreal` 生成文件名覆盖逻辑与 `cpp` 一致。

集成 / harness：

- `cargo test standard_project_generated_code_loads_exported_runtime_data`
- `tests/fixtures/standard_project/harness/run.sh cpp_unreal`
- 本步骤之后每次修改 `cpp_unreal` 都必须保持 standard_project cpp_unreal harness 通过。

建议提交：

```text
test(fixtures): add standard cpp_unreal target and harness
```

## Step 8：Blueprint 类型生成 smoke

目标：实现 `support = "blueprint"` 的类型层输出，但先不暴露完整查询 facade。

主要修改：

- header include 顺序：所有 include 后最后一项为 `#include "<stem>.generated.h"`。
- row / object / fragment：`USTRUCT(BlueprintType)` + `GENERATED_BODY()`。
- 可见字段：required 字段生成 `UPROPERTY(BlueprintReadOnly, Category="ConfigForge")`；optional 字段因当前生成为 `TOptional<T>`，不生成 `UPROPERTY`，避免 UHT 反射不支持的模板类型。
- enum：`UENUM(BlueprintType)`。
- `api_macro` 在 Blueprint 模式必填或有明确默认策略；缺失时报错。
- Blueprint 模式禁止 `namespace`；如需避免全局 reflected type 冲突，使用 `type_name_prefix`。
- containers-only 内部 repository 可继续非 reflected，避免配置行进入 GC 管理。

单元测试：

- Blueprint support 生成 `.generated.h` include 且顺序正确。
- 生成 `USTRUCT(BlueprintType)`、`UPROPERTY(BlueprintReadOnly)`、`UENUM(BlueprintType)`。
- 缺少必需 `api_macro` 报错。
- Blueprint 模式配置 `namespace` 报错；`type_name_prefix` 应影响 row、struct、enum、`GameData`、`LazyGameData`、公共 access interface 和 Blueprint library 类型名。
- optional 字段在 Blueprint 模式不生成 `UPROPERTY`。
- Blueprint 模式 public API 不包含 `std::*`、`std::optional`、raw row pointer 暴露。

集成 / harness：

- 使用 shim 定义 `USTRUCT` / `UPROPERTY` / `UENUM` / `GENERATED_BODY` 等宏，做编译 smoke。
- 不要求真实 UHT；文档标明真实 Unreal 项目验证留给后续引擎 CI。

建议提交：

```text
feat(codegen): emit Blueprint Unreal config types
```

## Step 9：Blueprint facade 查询 API

目标：在 Blueprint 模式下生成可调用查询入口。

主要修改：

- 生成 `UBlueprintFunctionLibrary` 或可配置 facade 类。
- lookup API 使用 Blueprint 友好形态：
  - unique：`static bool GetItemById(..., FItemRow& OutRow)`；
  - non-unique：`static void FindItemsByRarity(..., TArray<FItemRow>& OutRows)`；
  - ordered range：`static void RangeByLevel(..., TArray<FClassLevelRow>& OutRows)`。
- 可能触发加载 / I/O 的函数使用 `BlueprintCallable`；纯内存只读 helper 才可使用 `BlueprintPure`。
- 不在纯 getter 中隐式 lazy load；加载生命周期交给项目 adapter 或 facade 初始化入口。

单元测试：

- 生成 `UCLASS()`、`UFUNCTION(BlueprintCallable)` / `BlueprintPure`、`UPARAM(ref)` 或 out 参数。
- unique / non-unique / ordered index Blueprint facade 形态正确。
- 函数名、Category、WorldContext 配置不冲突。

集成 / harness：

- shim 编译 Blueprint facade。
- runtime smoke 调用 facade 的 C++ 静态函数，验证返回值与 containers API 一致。

建议提交：

```text
feat(codegen): add Blueprint query facade
```

## Step 10：文档、清理和最终回归

目标：收敛文档和测试说明，删除临时 TODO 中已完成或过期的细节。

主要修改：

- 更新 `Documentation/Design/06-export-cli.md`：把 `cpp_unreal` 从“规划”调整为当前支持状态。
- 更新 `Documentation/Implementation/09-codegen-module-detail-design.md` 当前实现状态。
- 更新 `Documentation/Implementation/09c-codegen-cpp-backend.md` 或拆分正式 `cpp_unreal` backend 设计章节。
- 更新 `Documentation/Implementation/09h-codegen-generator-api-and-tests.md` 的 harness 表格，加入 `cpp_unreal`。
- 更新 `Documentation/Tech/cpp-runtime-dependencies.md`，说明 Unreal shim 只用于测试，不是生产 runtime adapter。

验证：

- `cargo fmt`
- `cargo test`
- `cargo lint`
- `tests/fixtures/standard_project/harness/run.sh`
- 若本机没有某些外部命令，记录跳过原因，但不能影响 Rust unit / integration 的核心断言。

建议提交：

```text
docs(codegen): document cpp_unreal backend status
```

## 不能做的事

- 不能在标准 `cpp` backend 中加入大量 `if unreal` 分支。
- 不能为了 harness 编译把生产生成代码退回 `std::*`。
- 不能要求普通 CI 安装完整 Unreal Engine 才能跑 ConfigForge 集成测试。
- 不能在 `support = "blueprint"` 的 Blueprint public API 中暴露裸 row pointer、`std::*`、模板化返回值或复杂 iterator。
- 不能让 standard_project 的已有 Rust / C++ / TypeScript / Lua harness 因新增 `cpp_unreal` 目标而退化或跳过。
