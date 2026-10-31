# 09j. cpp_unreal backend 实现设计

本文描述 `cpp_unreal` 生成器的当前实现设计和能力边界。临时任务清单与阶段性实施记录统一放在 DevLogs；本文维护 src/generator/cpp_unreal/ 的当前契约。

## 当前实现状态

- `cpp_unreal` 是独立 codegen type，内部生成器为 `CppUnrealCodeGenerator`，不作为标准 `cpp` backend 的开关。
- `support = "containers"` 已支持 Unreal 容器和基础类型：`TArray`、`TMap`、`TPair`、`TOptional`、`TUniquePtr`、`FString`、`int32` / `int64`，并生成 row/object、repository、index、relation、text helper、`FGameData` 与 `FLazyGameData`。
- `support = "blueprint"` 已在 containers 模式基础上生成 `USTRUCT(BlueprintType)`、`UENUM(BlueprintType)`、required 字段 `UPROPERTY(BlueprintReadOnly)`、`.generated.h` include 和 `UBlueprintFunctionLibrary` 查询 facade；optional 字段仍生成为 `TOptional<T>`，但不直接生成 `UPROPERTY`。
- `FGameData` 与 `FLazyGameData` 通过生成的 `I<Prefix>ConfigForgeGameDataAccess` 共享只读表访问接口；relation helper 接收该公共接口，避免只支持 eager `FGameData`。未配置 `type_name_prefix` 时接口名为 `IConfigForgeGameDataAccess`。
- Blueprint 模式禁止 namespace，避免真实 UHT 解析 namespace 内反射类型失败；需要命名隔离时配置 `type_name_prefix`，该前缀会应用到所有生成类型名，例如 `FReferenceItemsRow`、`FReferenceGameData`、`FReferenceLazyGameData`、`IReferenceConfigForgeGameDataAccess`、`UReferenceConfigForgeBlueprintLibrary`。
- 标准工程已包含 `cpp_unreal` target，普通 CI 通过 lightweight Unreal shim 做语法 smoke；真实 UHT / Editor 集成验证留给引擎侧 CI。

## 目标边界

- `cpp_unreal` 是独立 codegen type，内部生成器命名为 `CppUnrealCodeGenerator`；Unreal 支持不能做成标准 `cpp` backend 的开关。
- `support = "containers"` 输出 Unreal 容器风格 C++，但不依赖 UnrealHeaderTool；`support = "blueprint"` 在 containers 模式基础上额外输出 Blueprint 反射层。
- 生成代码必须保持 format-agnostic：只声明 Unreal 版 runtime loader / reader contract，不直接绑定 JSON、MessagePack、Pak、Asset Manager、Data Asset、网络或本地文件系统。
- 普通 ConfigForge CI 不要求安装 Unreal Engine；仓库内集成测试只使用轻量 Unreal compatibility shim 做生成代码语法 smoke 和 standard_project harness。真实 UHT / Editor 集成由引擎侧 CI 覆盖。

## 模块结构

`src/generator/cpp_unreal/` 按生成职责拆分：

| 模块 | 职责 |
| --- | --- |
| `mod.rs` | `CppUnrealCodeGenerator`、选项解析、入口编排、文件头、namespace glue。 |
| `symbols.rs` | `CppUnrealNames`、Unreal 命名转换、关键字和生成符号冲突校验。 |
| `types.rs` | enum、struct、row/object、table declaration、复合 index key、text helper 和 relation declaration。 |
| `reader.rs` | runtime reader contract、value/field/row/object 读取代码、schema revision 检查。 |
| `repository.rs` | table load、index storage/build/query、ordered range、relation source、optional key 判空。 |
| `runtime.rs` | `FGameData`、`FLazyGameData`、公共只读 access interface、加载和 lazy preload。 |
| `blueprint.rs` | Blueprint reflected 类型辅助和 `UBlueprintFunctionLibrary` facade。 |
| `tests.rs` | backend 单元测试和生成结果 smoke 断言。 |

根模块只保留入口编排和跨模块 glue；新增生成逻辑应优先放入对应职责模块，避免重新堆回单文件。

## Containers API 设计

- 基础类型映射使用 Unreal 风格类型：`int -> int64`、`float -> double`、`bool -> bool`、`string/text/asset -> FString`、`list<T> -> TArray<T>`、optional -> `TOptional<T>`。
- row/object、repository、`FGameData`、`FLazyGameData` 和 relation helper 均使用 Unreal 容器和错误返回模型，不生成 `std::vector`、`std::unordered_map`、`std::string`、`std::optional` 或 C++ exception public API。
- unique / non-unique exact lookup 使用 `TMap`，复合 key 生成 key struct、`operator==` 和 `GetTypeHash`；ordered index 使用排序后的 `TArray<TPair<Key, int32>>`，range API 使用 lower / upper bound。
- relation helper 缓存目标 row index 或 empty 状态，不缓存裸 pointer；optional FK key 的空关系语义通过语言无关 `RelationKeyPlan` 集中分析，避免与标准 C++ / Rust / TypeScript backend 漂移。
- `FGameData` eager load 在调用期间借用 loader；`FLazyGameData` 持有 loader 并在首次访问 table/object 时加载。两者通过生成的 `I<Prefix>ConfigForgeGameDataAccess` 共享只读访问接口，使 relation helper 不绑定 eager-only 数据集。

## Blueprint API 设计

- `support = "blueprint"` 下 header 最后 include `<stem>.generated.h`，并为 reflected 类型生成 `USTRUCT(BlueprintType)`、`UENUM(BlueprintType)`、`GENERATED_BODY()`。
- required 且 UHT 可反射的字段生成 `UPROPERTY(BlueprintReadOnly, Category="...")`；optional 字段仍作为 C++ API 的 `TOptional<T>` 存在，但不生成 `UPROPERTY`。
- Blueprint 查询 facade 通过 `UBlueprintFunctionLibrary` 暴露 `UFUNCTION`，避免在 Blueprint public API 中暴露 raw row pointer、`std::*`、模板化 helper 或复杂 iterator；unique lookup 使用 `bool + OutRow`，non-unique / range lookup 使用 `TArray<Struct>& OutRows`。
- Blueprint 模式禁止 namespace；需要命名隔离时配置 `type_name_prefix`，该前缀必须覆盖 row、struct、enum、`GameData`、`LazyGameData`、公共 access interface 和 Blueprint library 类型名。

## 测试与验证边界

- 单元测试覆盖选项解析、命名/关键字冲突、类型映射、reader contract 代码片段、index/relation/lazy/Blueprint 生成形态，以及生成内容不包含标准 C++ backend 的 `std::*` public API。
- standard_project 已包含 `cpp_unreal` target 和 harness，长期覆盖 containers API、runtime load、index lookup、relation helper、text helper、lazy load，以及 Blueprint facade 的 shim 编译 smoke。
- 测试 shim 只模拟当前生成代码需要的最小 Unreal API；它不是生产 runtime adapter，也不承诺 ABI、GC、反射或 UHT 行为与真实 Unreal Engine 等价。

## 不能做的事

- 不能在标准 `cpp` backend 中加入大量 `if unreal` 分支。
- 不能为了 harness 编译把生产生成代码退回 `std::*`。
- 不能要求普通 CI 安装完整 Unreal Engine 才能跑 ConfigForge 集成测试。
- 不能在 `support = "blueprint"` 的 Blueprint public API 中暴露裸 row pointer、`std::*`、模板化返回值或复杂 iterator。
- 不能让 standard_project 的已有 Rust / C++ / TypeScript / Lua harness 因新增 `cpp_unreal` 目标而退化或跳过。
