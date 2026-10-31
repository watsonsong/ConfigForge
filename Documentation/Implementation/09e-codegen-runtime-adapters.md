# 09e. Codegen Runtime Adapters

相关文档：[reader contract](09a-codegen-runtime-reader-contract.md)、[backend 能力矩阵](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix)、[开发环境](../Tech/development-environment.md)、[测试/CI 矩阵](../Tech/test-ci-matrix.md)、[C++ 依赖策略](../Tech/cpp-runtime-dependencies.md)。

## 当前状态与分层

`runtime/` 是仓库维护的标准 reader/core/loader 源码，不是每次 export 必然输出的目录。生成代码只依赖 RuntimeDataReader/Loader contract；具体格式 parser、文件系统或引擎资源访问留在 adapter 和 transport loader。

必须区分以下状态：

- Reader contract、structured error taxonomy、版本 metadata、compliance 与 package 验证流程已形成稳定契约。
- 每个 adapter 的 support level 单独声明，不能把全部组合统称为 stable。
- 源码包/模板可接入，不表示已经发布 crates.io/npm/NuGet/Maven。正式 registry 发布属于独立发布流程。

## 当前目录与支持矩阵

| 语言/格式 | 物理目录 | Support level / 分发 | Parser backend |
| --- | --- | --- | --- |
| Rust core | `runtime/rust/reader_core` | stable / Cargo package | 无格式 parser。 |
| Rust JSON | `runtime/rust/serde_json_reader` | stable / Cargo package | serde_json。 |
| Rust MessagePack | `runtime/rust/serde_msgpack_reader` | stable / Cargo package | rmp-serde。 |
| Rust bin | `runtime/rust/binary_reader` | stable / Cargo package | ConfigForge binary。 |
| TypeScript core | `runtime/typescript/reader_core` | stable / CommonJS workspace package | 共享 reader/value/error。 |
| TypeScript JSON | `runtime/typescript/json_reader` | stable / workspace package | JSON.parse。 |
| TypeScript MessagePack | `runtime/typescript/messagepack_reader` | stable / workspace package | @msgpack/msgpack peer dependency；injectable decoder 为 custom hook。 |
| TypeScript bin | `runtime/typescript/binary_reader` | stable / workspace package | ConfigForge binary。 |
| TypeScript fs loader | `runtime/typescript/loaders/node-fs` | workspace package | 注入 decoder，不绑定格式。 |
| C++ core / bin | `runtime/cpp/core`、`runtime/cpp/binary_reader` | stable / CMake targets | 无三方依赖。 |
| C++ JSON | `runtime/cpp/rapidjson_reader` | stable / CMake target | RapidJSON，显式启用。 |
| C++ MessagePack | `runtime/cpp/msgpackc_reader` | stable / CMake target | msgpack-cxx，显式启用。 |
| C# JSON | `runtime/csharp/json_reader`、`system_text_json_reader` | stable / copyable template | System.Text.Json；后者明确 backend identity。 |
| C# MessagePack | `runtime/csharp/messagepack_reader` | legacy-bootstrap / copyable template | dependency-free subset parser。 |
| C# bin | `runtime/csharp/binary_reader` | stable / copyable template | ConfigForge binary。 |
| Java JSON | `runtime/java/json_reader` | legacy-bootstrap / copyable template | JDK-only 手写 parser。 |
| Java MessagePack | `runtime/java/messagepack_reader` | legacy-bootstrap / copyable template | JDK-only subset parser。 |
| Java bin | `runtime/java/binary_reader` | stable / copyable template | ConfigForge binary。 |

目录与 metadata 以源码 package/template 为准；`legacy-bootstrap` 仍有测试覆盖，但不是 library-bound 标准 parser 的生产承诺。C# MessagePack-CSharp、Java Jackson JSON/MessagePack 为后续适配方向，当前不存在可直接依赖的对应 package。Fetch/browser loader 尚未提供，不应放进当前目录树。

## Contract 与 metadata

Reader API 只覆盖生成 parser 直接调用的最小形状：schema-driven field entry、index/exit、row/list length、record boundary、enum/scalar、schema revision 和 exhausted check。DOM field/len、binary read_next/ordinal 等便利方法属于 concrete reader，不扩大 core contract。

所有包/模板声明 reader contract version、required/supported range、adapter version、distribution mode、parser backend identity 与 support level。错误包含 kind/code/path，跨语言 kind 映射为 `CF_RT_*`；IO/path 错误需包装成 Io / CF_RT_IO，不泄漏普通底层异常。decode limits、missing/null、scalar conversion、record boundary 与 enum ordinal 的规范见 [09a contract](09a-codegen-runtime-reader-contract.md)。

`tests/runtime_helper_release_metadata.rs` 检查 generated constants、package/template metadata、支持范围和分发形态，防止版本漂移。

## 各语言接入边界

### Rust

默认 `runtime_interface = "inline"`：generated code 自带 trait/error/result，保留自包含使用方式。`external` 为 opt-in，generated code 引用 reader_core；`runtime_core_path` 可指向标准 crate 或业务 re-export，标准 adapter crate 实现共享 trait。

runtime crates 与 ConfigForge 工具有独立 MSRV 和 Cargo metadata。不要把独立 Cargo packages 称为尚待拆分的 copyable templates。

### TypeScript

接口是结构化类型，标准 adapter 可直接传给生成的 GameData。Core/JSON/MessagePack/bin/node-fs 分包，JSON 用户不强制安装 MessagePack backend。当前发布形态为 CommonJS，不能把未来 ESM exports 当作现能力。

Node fs loader 与 decoder 组合，sync/async 约束由 reader contract 与 [TS backend](09d-codegen-typescript-backend.md)定义；Promise decoder 不能传入要求同步返回 reader 的 loadSync 路径。

### C++

支持默认 inline nominal interface 和 opt-in external core header。External 使用 `config_forge/runtime/reader.hpp`，core、binary、RapidJSON、msgpack-cxx 都通过 CMake targets 组合；安装后支持 find_package smoke。

```cmake
set(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER ON CACHE BOOL "" FORCE)
add_subdirectory(path/to/ConfigForge/runtime/cpp)
target_link_libraries(game_config_runtime PRIVATE
  config_forge_runtime::core
  config_forge_runtime::rapidjson_reader)
```

依赖解析、FetchContent opt-in、install/export 和 target 名称以 [C++ 工程指南](../Tech/cpp-runtime-dependencies.md)为准。该示例要求宿主工程已创建 game_config_runtime target，且安装 RapidJSON 或显式允许下载。

### C# / Java

当前生成 nominal inline interface，copyable reader template 通过薄 adapter 接入。模板标注版本和 contract range，standard_project 与 compliance 复用它们；不承诺 NuGet/Maven package 或任意移动端/JDK 版本。只有支持稳定 external assembly/package 后，再评估独立分发形态。

## runtime_interface_emit 与未来 adapter emit

当前的 runtime_interface_emit 是已有 codegen 选项，语言支持范围见各 backend：

- Rust/C++ 的 reference 可生成 `config_forge_runtime_interface.json`，描述 contract、runtime core 和 reader 依赖 metadata。
- TypeScript reference 尚未支持，必须明确报错。
- C#/Java 仍推荐 copy；reference 诊断提示当前 backend 和 support level，不生成不可执行的 package 引用。

未来 `[targets.exports.runtime_reader] adapter/emit/loader` 配置尚未实现，与 runtime_interface_emit 不是同一个入口。若引入，需明确 adapter 版本、依赖、path、覆盖用户修改和 loader 同步/异步限制，不能仅把草案配置放入用户示例。

## Compliance 与端到端测试

两层测试分别保护不同契约：

| 测试层 | 验证对象 | 边界 |
| --- | --- | --- |
| [runtime_reader_compliance](../../tests/fixtures/runtime_reader_compliance/) | reader missing/null、scalar、container、path/index、decode limits、错误 kind/code 与格式非法 payload。 | 不依赖业务 generated API，不重复完整工程流水线。 |
| [standard_project](../../tests/fixtures/standard_project/) | build/export/generated API、target filtering、index/relation、真实 runtime glue 与 package 接入。 | 少量真实端到端场景，不复制所有 reader 边界。 |

JSON 提供可读 canonical payload；MessagePack/bin materialization 复用相同逻辑用例，格式独有 negative cases 单列。不能缺少某 backend 时静默用 JSON 或 bootstrap parser 代跑。Rust、TypeScript、C++、C#、Java runner 均覆盖 JSON/MessagePack/bin，必需工具、skip 与网络策略只在 [测试矩阵](../Tech/test-ci-matrix.md)维护。

Release gate 包括 Rust core package verification 与 adapter package file-list、TypeScript npm pack dry-run、C++ install/find_package、metadata drift 和 compliance。Rust adapter 依赖尚未发布的 core 时，正式 registry verification 需按 core → adapters 发布顺序执行；仓库本地源码测试不等价于 registry 已发布。

## 后续扩展

当前尚待推进的是 library-bound C#/Java parser adapters、更多格式负例与 transport 集成、正式 registry 发布及按实际需要设计 adapter emit。它们不要求再建立一套 runtime model 或修改 generated code 对具体 parser 的零强依赖边界。
