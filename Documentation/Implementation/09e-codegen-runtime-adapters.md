# 09e. Codegen Runtime Adapters

本文从 code artifact toolkit 详细设计中拆出，专门描述顶层 `runtime/` 标准 adapters、reader contract compliance、C++ CMake、TypeScript packages、Rust templates 与测试矩阵。

## Related docs

- [`09-codegen-module-detail-design.md`](09-codegen-module-detail-design.md) — Codegen 总览、IR、命名、冲突检测和测试总览。
- [`../Design/06-export-cli.md`](../Design/06-export-cli.md) — 用户可见的导出、runtime loading 和 generated API 语义。
- [`13-runtime-binary-format-detail-design.md`](13-runtime-binary-format-detail-design.md) — `bin` runtime data schema revision 与 positional reader contract。

## 当前状态

`runtime/` 下标准 adapters 是 generated code 的辅助接入层，不是每次 codegen 的必然输出。核心 generated code 只依赖 loader/reader contract；标准 adapters 提供可复用的 JSON / MessagePack / `bin` reader 和常用 transport loader。当前 Rust 已有 `runtime/rust/reader_core` / JSON / MessagePack / binary Cargo packages，TypeScript 已有 core / JSON / MessagePack / binary / node-fs workspace packages，C++ runtime helper 已有 core header、adapter interface targets、install/export config 与 `find_package` smoke；C# / Java 保持 JSON / MessagePack / binary copyable templates。它们均声明 package/template version、reader contract version / supported range 和 distribution mode，并由 release metadata drift test、runtime compliance runner、TypeScript package dry-run、C++ install smoke 和 Rust package file-list/verification gate 覆盖。标准 runtime helpers 当前状态为 Stable；正式 crates.io / npm / NuGet / Maven 发布仍按各生态 release 流程执行，不改变 generated code 对三方 adapter 的零强依赖原则。C++ 三方依赖与 CMake 策略以 [`../Tech/cpp-runtime-dependencies.md`](../Tech/cpp-runtime-dependencies.md) 为准。

## 标准 runtime helper 工程结构

生成代码只声明 reader / loader contract，这解决了 Meta-specific 逻辑与业务资源系统耦合的问题，但实际游戏项目仍需要可运行的 JSON / MessagePack 等格式 reader。项目应提供标准 RuntimeDataReader adapter 源码/模板，作为 generated code 之外的 runtime support 层。

### 分层边界

标准 adapter 分三层：

1. **Contract glue**：把某语言的 concrete value/cursor 类型适配成 generated code 期望的 `RuntimeDataReader` / `IRuntimeDataReader` / `RuntimeDataReader` 形状。
2. **Format reader**：把 JSON、MessagePack 等具体格式解码为 value/cursor，并实现 field/index/len/scalar/null 语义。
3. **Transport loader**：从文件系统、HTTP、bundle、引擎资源系统等位置读取 bytes/text，再交给 format reader。Transport 可选，业务项目可以只复用 format reader 并接入自己的资源系统。

核心 codegen 不 import adapter，也不把 adapter 的第三方依赖写进 generated code。adapter 是可选 runtime support：可以被业务项目直接依赖，也可以由 codegen option 拷贝到目标目录，还可以作为测试 harness 的基础实现。

Core purity 原则：要么某语言暂时不提供独立 core library，让 generated code 自带 inline / nominal contract；要么 core library 只包含 generated parser 直接调用的最小 reader / loader 形状、error、decode limits 和 contract version。JSON / MessagePack / BSON named reader 的 `enter_field`、`length`，binary reader 的 `read_next_*`、optional bitmap、enum ordinal，transport loader 的文件系统 / HTTP / 资源包逻辑，都应留在 concrete adapter 的固有方法或 utility 模块中。少量 reader 间重复实现优先接受，以换取用户引用 core 的最低成本；只有重复逻辑足够大且与 generated contract 无关时，才考虑单独 helper / utility 包，不能污染 core contract。

### Reader contract 可执行规范

标准 adapter 不能只对齐接口形状，还必须对齐 reader contract 版本、value model、missing/null、scalar conversion 和错误上下文。当前 contract 版本定义为 `1.0`；后续所有 generated code 与 adapter 都应声明支持的 contract version。

generated code 输出常量：

```rust
pub const CONFIG_FORGE_READER_CONTRACT_VERSION: &str = "1.0";
pub const REQUIRED_CONFIG_FORGE_READER_CONTRACT: &str = "^1.0";
```

```ts
export const CONFIG_FORGE_READER_CONTRACT_VERSION = "1.0" as const;
export const REQUIRED_CONFIG_FORGE_READER_CONTRACT = "^1.0" as const;
```

```cpp
inline constexpr const char* CONFIG_FORGE_READER_CONTRACT_VERSION = "1.0";
inline constexpr const char* REQUIRED_CONFIG_FORGE_READER_CONTRACT = "^1.0";
```

adapter 也声明支持范围，例如 `SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0"`。当 codegen `emit = "copy"` 时，应把 adapter 支持范围写入复制文件头或 adapter manifest；当 `emit = "reference"` 时，应在生成 manifest / 诊断中说明所需 adapter 包或 CMake target 的 contract range。

`runtime_interface_emit = "reference"` 当前由 Rust / C++ 生成 `config_forge_runtime_interface.json` manifest，包含 `contract_version`、`required_contract`、language、runtime core crate path 或 C++ core header/namespace。TypeScript 已保留 `runtime_interface_emit` option，但 external import 形态尚未稳定，因此 `reference` 诊断为 unsupported；C# / Java 继续推荐 copyable template，`reference` 诊断提示使用 `copy`。

### Structured runtime error baseline

标准 adapters 和 generated inline/external contract 使用统一 error kind/code baseline，避免用户解析错误字符串：

| Kind | 典型来源 |
| --- | --- |
| `Io` | transport loader 文件/资源读取失败。 |
| `Decode` | JSON / MessagePack / `bin` payload 损坏、varint/escape/格式错误。 |
| `SchemaMismatch` | binary schema revision 或 payload shape 与 generated code 不兼容。 |
| `TypeMismatch` | scalar conversion 失败、范围溢出、对不兼容 runtime value 读取目标类型。 |
| `NotFound` | logical data path 或 required field 缺失。 |
| `Unsupported` | MessagePack bin/ext、non-string map key 等 contract 不支持值。 |
| `InvalidPath` | path traversal、enter index 越界、对 scalar 取 field/list length。 |

语言映射：Rust `RuntimeDataErrorKind` + `RuntimeDataError`；TypeScript `RuntimeDataError extends Error`；C++ `RuntimeDataError` + `RuntimeDataErrorKind`；C# `ConfigForgeRuntimeException.Kind/Code/Path`；Java `ConfigForgeRuntimeException.kind()/code()/path()`。Compliance negative cases 断言 normalized `kind` / `code` / path，message 只作为辅助 keyword。

Contract version bump 规则：

| 变化类型 | 版本级别 | 示例 |
| --- | --- | --- |
| reader / loader method signature 变更 | major | `field` 返回类型变化、loader open 参数变化。 |
| scalar conversion 或 missing/null 破坏性语义变更 | major | bool 不再接受 `"1"` / `"0"`。 |
| 新增可选方法或非破坏性能力 | minor | 新增 path context helper，但旧 adapter 仍可工作。 |
| 错误信息更清晰、内部性能优化 | patch | 增加 source path 到错误 message。 |

Contract 层定义的是 **ConfigForge Runtime Value**，不是 JSON Value。当前 Rust implementation 可以短期复用 `serde_json::Value` 降低实现成本，但 contract 应按下列 value model 描述：

| Runtime Value | JSON | MessagePack | Contract 语义 |
| --- | --- | --- | --- |
| object/map | object | map with string keys | `field(name)` 按字段名查找；非 string map key 暂不支持，应报错。 |
| array | array | array | `index(i)` 按 0-based 下标读取；越界报错。 |
| int / uint | number 或 numeric string | signed/unsigned integer 或 numeric string | generated parser 根据字段类型读取并检查范围；超出目标类型报错。 |
| float | finite number 或 numeric string | finite float / integer 或 numeric string | 非 finite 值必须报错。 |
| bool | bool 或 string | bool 或 string | string 仅接受大小写敏感的 `"true"` / `"false"` / `"1"` / `"0"`。 |
| string | string | string | 不做 number-to-string / bool-to-string 自动转换。 |
| null | null | nil | explicit null 在字段入口处按 missing 处理；scalar conversion 不接受 null。 |
| binary/ext | 不适用 | bin/ext | 当前 contract 不支持，adapter 必须明确报错，不能隐式转 string。 |

统一 scalar conversion policy：

| 读取路径 | 接受输入 | 禁止输入 | 额外要求 |
| --- | --- | --- | --- |
| int | integer value、可解析为整数的 string | float 小数、bool、object/array/null、溢出值 | 错误包含 path、expected `int`、actual type/value。 |
| float | finite number、可解析为 finite number 的 string | NaN、Infinity、bool、object/array/null | 错误包含 path、expected `float`、actual type/value。 |
| bool | bool、`"true"`、`"false"`、`"1"`、`"0"` | 大小写变体、number 1/0、其他 string、object/array/null | 保持与 ConfigForge validation/export 当前 bool string 语义一致。 |
| string | string | number/bool/object/array/null | 不自动 stringify。 |

missing/null 语义：

- `field(name)` 找不到字段或字段值为 null/nil 表示 missing：Rust/TS 返回 `None` / `undefined`，C++ 返回 `nullptr`。
- explicit null 不再依赖独立 reader null-check API；由 `EnterOptionalField` / `enter_optional_field` / `enterOptionalField` 的 bool 返回值统一表达 optional presence。
- `index(i)` 越界必须报错，不返回 null reader。
- `length()` / `len()` 对 payload array 返回元素数；对 object/map 返回字段数；对 scalar/null 报错。C++ adapter 不再暴露通用 `Size()`，generated collection 读取使用 `ReadRowCount()`；generated list 读取使用 `ReadListLength()` / `readListLength()` / `read_list_len()`。schema-aware binary reader 额外提供 list length / positional list element 读取能力，使生成代码能在 `bin` 中读取 length prefix 后顺序读取元素；named JSON / MessagePack reader 可在 list length API 内部复用 `length + index(i)`。

错误上下文要求：adapter 抛出的错误应包含 logical data path、reader path（field/index 链）、expected type、actual type/value。transport loader 还应包含 resolved resource path 或 URL。

### 工程化目标与当前缺口

`runtime/` 的目标不是只服务 standard_project harness，而是成为游戏项目可直接引用的 runtime support 层。当前仓库已经有首批源码：

| 当前目录 | 定位 | 当前状态 |
| --- | --- | --- |
| `runtime/rust/reader_core` | Rust reader contract crate | 已声明 `CONFIG_FORGE_READER_CONTRACT_VERSION = "1.0"` / `SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0"`，并提供 trait、error、decode limits；package-level tests 读取共享 compliance fixture。 |
| `runtime/rust/serde_json_reader` | Rust JSON reader module/package | 已有基于 `serde_json` 的 `JsonValueReader` 与 fs loader，补齐 Cargo package metadata，并接入 Rust adapter compliance runner。 |
| `runtime/rust/serde_msgpack_reader` | Rust MessagePack reader module/package | 已有基于 `rmp-serde` 的 `MessagePackValueReader` 与 fs loader，补齐 Cargo package metadata，并接入 Rust adapter compliance runner / standard_project Rust harness。 |
| `runtime/rust/binary_reader` | Rust binary reader package | 已有无第三方依赖的 schema-aware `bin` reader 与 fs loader，补齐 Cargo package metadata，并接入 crate tests / Rust compliance runner。 |
| `runtime/typescript/reader_core` | TypeScript reader contract package | 已补齐 npm workspace package metadata、CommonJS `exports` / `types`、`sideEffects` 和 reader contract range。 |
| `runtime/typescript/json_reader` | TypeScript JSON reader package | 已有 JSON reader，使用结构化类型，不依赖 generated.ts，补齐 package metadata，并通过 Rust 驱动的临时工程接入 JSON compliance runner。 |
| `runtime/typescript/messagepack_reader` | TypeScript MessagePack value reader package | 已有基于 injectable decoder 的 reader，生产可使用 `@msgpack/msgpack` peer；补齐 package metadata，临时 compliance 工程通过 JSON → MessagePack materialization 和测试 decoder 覆盖。 |
| `runtime/typescript/binary_reader` | TypeScript binary reader package | 已有 `Uint8Array` / `DataView` binary reader，补齐 package metadata，并接入 binary success 与 malformed compliance coverage。 |
| `runtime/typescript/loaders/node-fs` | TypeScript Node fs loader package | 已拆成可注入 decoder 的 transport loader package，补齐 package metadata。 |
| `runtime/cpp/rapidjson_reader` | C++ RapidJSON reader adapter | 已有 header-only RapidJSON adapter，可用于测试与生产接入，并通过 CMake / CTest compliance target 覆盖。 |
| `runtime/cpp/msgpackc_reader` | C++ msgpack-cxx reader adapter | 已有 header-only msgpack-cxx adapter，可用于 MessagePack runtime data 接入，并通过 CMake / CTest compliance target 覆盖。 |
| `runtime/cpp/binary_reader` | C++ binary reader adapter | 已有 header-only binary adapter，可通过 `config_forge_runtime::binary_reader` target 引入，并通过 CMake / CTest compliance target 覆盖。 |
| `runtime/csharp/json_reader` | C# JSON reader template | 已有基于 .NET BCL `System.Text.Json` 的 reader 与 fs loader；standard_project harness 通过薄 adapter 接入每次 generated code 声明的 nominal interface。 |
| `runtime/csharp/messagepack_reader` | C# MessagePack reader template | 已有 dependency-free MessagePack subset parser / reader 与 fs loader，覆盖当前 JSON 同构 named payload；standard_project harness 已同时验证 JSON / MessagePack。 |
| `runtime/csharp/binary_reader` | C# binary reader template | 已有 dependency-free schema-aware `bin` reader 与 fs loader，覆盖 CFGB header、schema revision、record boundary、optional bitmap、enum ordinal 和 exhausted check；standard_project harness 已验证 Binary。 |
| `runtime/java/json_reader` | Java JSON reader template | 已有 JDK-only JSON reader 与 fs loader；standard_project harness 通过薄 adapter 接入每次 generated code 声明的 nominal interface。 |
| `runtime/java/messagepack_reader` | Java MessagePack reader template | 已有 JDK-only MessagePack subset parser / reader 与 fs loader，覆盖当前 JSON 同构 named payload；standard_project harness 已同时验证 JSON / MessagePack。 |
| `runtime/java/binary_reader` | Java binary reader template | 已有 JDK-only schema-aware `bin` reader 与 fs loader，覆盖 CFGB header、schema revision、record boundary、optional bitmap、enum ordinal 和 exhausted check；standard_project harness 已验证 Binary。 |

当前结论：standard runtime helpers 已达到 Stable。仓库已经具备标准 reader 源码、统一 reader contract、跨语言 compliance runner、standard_project 端到端 smoke，以及默认不强迫 generated core 依赖具体 JSON / MessagePack / Binary adapter 的边界。Rust crate、TypeScript workspace package、C++ CMake install / `find_package`、C# / Java copyable template 都已声明版本和 supported reader contract range；structured error taxonomy、生产接入示例、generated `runtime_interface_emit = "reference"` manifest / diagnostic 和 release checklist 已收口。Stable 承诺覆盖源码 package / copyable template 的 contract 兼容性、metadata 一致性和 compliance 语义；正式外部 registry 发布是发布流程问题，不改变当前 generated code 与标准 helpers 的稳定 contract。

Stable release gate 当前执行结果：

1. `tests/fixtures/runtime_reader_compliance/` 已有 canonical payload / expected 文件，并通过 `tests/runtime_compliance.rs`、`tests/typescript_runtime_compliance.rs`、`runtime/cpp/compliance_test.cpp`、`tests/csharp_java_runtime_compliance.rs` 覆盖 Rust / TypeScript / C++ / C# / Java 的 JSON、MessagePack 和 binary 标准 reader。当前 Stable baseline 覆盖 success、scalar/path/enum/type mismatch、loader path traversal、decode limits、MessagePack unsupported shape 与 binary malformed corpus；后续可继续增加格式特有负例，但不阻塞 Stable。
2. Rust `reader_core`、`binary_reader`、`serde_json_reader`、`serde_msgpack_reader` 已补齐 Cargo metadata 和 package-level tests。`cargo package --manifest-path runtime/rust/reader_core/Cargo.toml --allow-dirty` 可完成 package verification；adapter crates 的 `cargo package --list` 可确认打包文件集合。由于 adapter crates 依赖尚未发布到 registry 的 `config-forge-runtime-reader-core = 0.1.0`，正式 registry verification 必须在 core crate 发布后按 reader_core → adapters 顺序执行。
3. TypeScript workspace packages 已补齐 CommonJS `exports` / `types` / `sideEffects` / contract range；`npm pack <package-dir> --dry-run` 已覆盖 core / JSON / MessagePack / binary / node-fs 包。`runtime/typescript/reader_core/dist` 与 node-fs dist 已随 structured error API 同步，避免 source/dist 漂移。
4. C++ generated inline `IRuntimeDataReader` 已移除非 generated-facing helper；`runtime/cpp/core/include/config_forge/runtime/reader.hpp`、`runtime_interface = "external"`、CMake install/export/version config、`find_package(ConfigForgeRuntime 0.1 REQUIRED CONFIG)` smoke test 与 `runtime_interface_emit = "reference"` manifest 均已落地，并通过 CTest `config_forge_runtime_find_package_smoke` 验证。
5. C# / Java JSON、MessagePack 与 Binary reader 已沉淀为 copyable template，并接入 runtime compliance runner；本阶段选择不发布 NuGet / Maven，而是在 template 中声明 version、contract range 和 `copyable-template` distribution mode。Jackson / Gson / Android / Newtonsoft.Json 等生态 adapter 暂不作为稳定 baseline。
6. `tests/runtime_helper_release_metadata.rs` 固化 generated constants、runtime package/template version、supported contract range 与 distribution mode 的一致性，防止后续 release metadata 漂移。

因此 runtime loader 工程化按“两层稳定 + 多 adapter 可选”的方向推进：

- **稳定契约层**：定义 reader/loader contract 版本、payload shape、错误语义、scalar 兼容策略。生成代码只依赖该契约，不依赖具体 adapter。
- **可选 adapter 层**：每个语言/格式组合是独立工程单元，明确三方依赖、构建方式、测试入口和生产推荐等级。

### 分阶段设计与开发路线

Runtime adapter 工程化应按“先可执行规范，再稳定接口，再发布包”的顺序推进，避免在 contract 尚未被自动验证前承诺长期包 API。

#### Phase R1：Adapter compliance runner MVP（已达到基础 MVP）

目标是把 `tests/fixtures/runtime_reader_compliance/` 升级为跨语言 / 跨格式 reader adapter 的可执行规范。此阶段不改变 generated code 对第三方库的零强依赖原则，也不要求立即发布 runtime package。当前 Rust、TypeScript、C++、C# 与 Java 的首批 runner 已接入，因此该阶段已经完成基础 MVP；后续工作属于扩展 case 覆盖和稳定化，不再是证明路线可行性的前置工作。

Compliance runner 的验证对象不是“每个语言必须有一个厚 core 模块”。core 模块只定义 generated code 与 adapter 之间的最小形状：reader / loader trait 或结构化接口、error、decode limits、contract version。某些语言可以只有 generated inline interface 或一份很薄的 structural type；只要 adapter 能被 generated code 通过薄 glue 调用，就可以参与 compliance。真正需要被反复验证的是具体 reader adapter 对 contract 语义的解释，而不是 core 本身承载业务逻辑。

跨语言 runner 的 profile、工具链要求、local skip 与 full/nightly fail-fast 策略统一记录在 [`../Tech/test-ci-matrix.md`](../Tech/test-ci-matrix.md)。本节只维护 adapter contract、case 边界和接入顺序，避免重复维护命令矩阵。

设计原则：

- compliance case 使用语言无关 `case_id`、payload path、expected TOML 和 expected error keyword；不同语言错误文本不需要完全一致，但必须包含稳定关键字和 reader path / expected type 等定位信息。
- runner 分成两层：共享 case schema + 各语言 adapter runner。共享 schema 不引用 Rust / TS / C++ 类型；语言 runner 只负责把 payload decode 成本语言 reader，再按同一批 operation 读取并断言结果。
- 先覆盖 JSON named-field reader；MessagePack / `bin` 用同一批 logical case 派生二进制 payload，避免语义漂移。
- fast/local profile 跑 Rust JSON 和无额外三方依赖的 runner；full/nightly profile 再跑 TypeScript、C++ RapidJSON/msgpack-cxx、C#、Java。缺工具时必须明确 skip 或 fail-fast，不能静默降级到其他 reader。

与 `standard_project` integration harness 的边界：

| 测试层 | 验证对象 | 保留原因 | 不应承担 |
| --- | --- | --- | --- |
| `tests/fixtures/runtime_reader_compliance` | reader adapter contract：missing/null、scalar conversion、path/index、decode limits、错误 kind/keyword、格式特有非法 payload。 | 让每个语言/格式 adapter 用同一批最小 logical cases 证明语义一致，适合用户自定义 reader 复用。 | 不验证 codegen 输出 API、target filtering、索引/关系 helper、完整导出流水线。 |
| `standard_project` integration | 真实项目 fixture 的 build/export/generated API smoke，以及各语言代表性 runtime 加载路径。 | 覆盖端到端组合风险：Meta / Data / data artifact generator / code artifact generator / runtime glue 是否能一起工作。 | 不作为 reader contract 的完整枚举测试；不复制所有 scalar/error 边界。 |

因此需要保留两层测试，但不能维护“两套重复语义用例”。`standard_project` 应保持少量真实世界端到端 smoke；reader contract 边界条件只放在 compliance logical cases 中。若发现 `standard_project` 只是为了覆盖某个 scalar/null/error 细节而增加人工 fixture，应优先迁移到 compliance；若 compliance case 开始依赖 generated API 行为或项目配置语义，也应迁回 integration harness。

用户视角与测试边界：

- `ComplianceJsonReader` 只是 Rust MVP 内部的 `#[cfg(test)]` reader，用来证明 `RuntimeDataReader` contract 与 fixtures 可执行；它不是面向业务项目的公开 JSON adapter，也不应出现在 generated code 的接入文档中。
- 面向用户的稳定入口是 `reader_core` 中的 trait / error / decode limits。用户通常有两种用法：直接依赖标准 adapter（例如未来稳定的 JSON / MessagePack / binary reader），或只依赖 `reader_core` 并实现自己的资源系统和 reader。
- 自定义 reader 只需实现 generated code 会直接调用的 contract API：schema-driven field helpers、index/exit、row/list length、record boundary、enum/scalar read、schema revision 和 exhausted check。JSON / MessagePack / BSON 等 named reader 可以把 `enter_field` / `len` / `length` 做成固有 helper，再由 `enter_required_field` / `enterOptionalField` / `read_row_count` / `readListLength` 复用；binary reader 可以把 `read_next_*` / optional bitmap / enum ordinal 做成固有 helper，再由 `as_*` / `enter_optional_field` / `read_enum_index` 复用。这些 helper 不属于 Rust stable trait、TypeScript core interface 或 C++ generated inline interface 的必选面。Transport loader（文件、HTTP、引擎资源包）应保持独立，避免把资源系统绑定进 format reader。
- 在 generated code 仍使用 inline nominal interface 的语言里，标准 reader 或用户 reader 需要一层薄 adapter；当语言支持 `runtime_interface = "external"` 后，generated code 才直接引用稳定 `reader_core` / core header / npm package。

Compliance runner 不能绑定 JSON。JSON 只是当前最容易 review 的 canonical payload 与 Rust MVP 起点；长期 runner 应提供一个很薄的 adapter 接入点，让任意新格式（例如 BSON、FlatBuffers-like 自定义格式、加密压缩包内格式）都能复用同一批 logical cases：

1. **case driver** 读取共享 logical case，负责执行 schema-driven field entry、index entry、scalar read / error assertion。
2. **payload materializer** 由格式 adapter 提供，把 canonical Runtime Value 转成该格式的 bytes / value。例如 BSON adapter 把 object/array/string/int/float/bool/null materialize 为 BSON document；binary adapter 把 logical case materialize 为带 schema/header 的 bytes。
3. **reader factory** 由格式 adapter 提供，把 materialized payload 解码成该语言的 `RuntimeDataReader` 实例。
4. **capability / skip policy** 由格式 adapter 显式声明：某格式无法表达的 case、或只在该格式存在的 negative case，必须可诊断地 skip / fail / 标记 unsupported，不能默默用 JSON reader 代跑。

以 BSON reader 为例，接入 compliance 不应复制一套 `bson_scalar_results.toml`。它应复用 `scalar_compat`、`collection_envelope`、`object_table` 等 logical cases，由 BSON materializer 生成 BSON document，再由 BSON reader factory 创建 reader 并跑同一批 operation/assertion。只有 BSON 特有边界（如 ObjectId / DateTime / Binary 是否 unsupported、document key 限制、BSON array 编码不连续等）才新增 BSON-only negative cases。

Compliance fixtures 长期不按“每种格式维护一整套手写用例”扩散，而按“逻辑用例一份，多格式 materialization”维护：

- 共享 schema 记录 logical case、读取 operation、path、expected scalar/error；这是唯一的语义来源。
- JSON payload 是可读 canonical fixture，优先用于 review 与新增 case。
- MessagePack / binary payload 应由 canonical fixture 或专门的 materializer 生成，必要时只额外维护格式特有 negative cases（例如 MessagePack non-string map key、bin header/schema revision 错误）。
- 各语言 / 各格式 runner 读取同一批 logical case 并绑定到自己的 decoder；如果某格式不能表达某个 value kind，应在 case capability 中显式标注 skip/unsupported，而不是复制一份近似但可能漂移的 expected 文件。

最小 case 范围：

| 类别 | 必须覆盖 |
| --- | --- |
| scalar | int / float / bool / string，numeric string，bool `true/false/1/0`，有限 float。 |
| missing/null | required missing/null 报错；optional missing/null 视为 absent。 |
| container | object field、array/list length、index 越界、collection envelope `{ meta, rows }`。 |
| string | escape、Unicode、换行、引号。 |
| error | int overflow、float NaN/Infinity、非法 bool string、非 string map key、binary/ext unsupported。 |

建议落地顺序：

1. 固化 compliance fixture schema：补齐 `collection_envelope.json`、`object_table.json`、`nested_struct.json`、`fragment.json` 和 expected TOML。（已完成首批 case）
2. Rust runner 先落地到 JSON adapter 或临时 test-only reader，借 `reader_core` 的 trait 形状快速证明 schema 可执行；稳定后应迁到具体 adapter crate / package，不把复杂 reader 逻辑沉淀进薄 core。（已通过 `tests/runtime_compliance.rs` 覆盖 `serde_json_reader` / `serde_msgpack_reader`）
3. TypeScript runner 作为 package-level test 或 Rust 驱动的临时测试工程，验证 `reader_core`、JSON reader、MessagePack reader 与 node-fs loader。（已用临时测试工程覆盖 JSON / MessagePack reader；node-fs 待接入）
4. C++ runner 通过 CMake / CTest 接入 RapidJSON、msgpack-cxx 和 binary reader，继续保持第三方依赖显式 opt-in。（已覆盖三种 reader 的首批 compliance target）
5. C# / Java runner 复用现有 JSON / MessagePack / Binary copyable template，先验证 BCL/JDK-only named reader 与 dependency-free binary reader，再决定是否补 Jackson / Gson / Android 友好 adapter。（已接入首批 compliance runner）

#### 初步稳定阶段准入标准

standard runtime helpers 从基础 MVP 推进到初步稳定，应至少满足下列条件：

1. **Contract freeze**：reader / loader method set、missing/null、scalar conversion、record boundary、schema revision、enum index 和 error keyword 经过 breaking-change review，并明确 `1.0` 兼容范围。
2. **Compliance coverage**：所有已列为标准 reader 的语言 / 格式组合继续通过 shared logical cases；MessagePack 非 string map key、bin header/schema revision、binary/ext unsupported、decode limit、loader path traversal 等格式/transport 特有 case 有明确覆盖或显式 unsupported 标注。
3. **Package / template versioning**：Rust crate、TypeScript package、C++ CMake target、C# / Java copyable template 或未来 package 都能声明 adapter version 与 supported reader contract range。
4. **Production integration path**：每个推荐 adapter 至少有一个最小接入示例，说明依赖安装、loader 组合、generated code 的 inline/external contract 选择，以及默认不联网策略。
5. **CI profile policy**：local / quick / full / nightly 对应工具要求、skip / fail-fast 和 `CONFIG_FORGE_ALLOW_NETWORK` 行为稳定记录在测试矩阵中，并由 focused tests 或 harness 输出验证。
6. **Release boundary**：明确哪些 adapter 是 stable、preview 或 custom/example；未进入 stable 的生态 adapter 不能被文档描述为默认生产依赖。

#### Phase R2：Rust stable runtime crate

目标是把 Rust 从“generated crate 本地 trait + copyable template”迁移到可选稳定 runtime crate，同时保持默认 generated code 不强制依赖该 crate。

当前实现已按 `runtime_interface = "inline" | "external"` 收敛：默认 inline 自包含 contract，external 作为 opt-in，并允许配置 `runtime_core_path` 指向 `config_forge_runtime_reader_core`、`crate::runtime` 或业务自有 re-export。这方便希望零额外依赖的用户直接复制 generated code，也允许真实 adapter crate 通过 external 模式实现共享 trait。

包边界（当前已按独立 Cargo package 落地 metadata，尚未发布 crates.io）：

| crate | 定位 | 默认依赖 |
| --- | --- | --- |
| `config-forge-runtime-reader-core` | 稳定 trait、error、decode limits、contract version。 | 无或仅 `std`。 |
| `config-forge-runtime-serde-json-reader` | `serde_json` reader + fs loader。 | `core` + `serde_json`。 |
| `config-forge-runtime-serde-msgpack-reader` | MessagePack reader + fs loader。 | `core` + `rmp-serde` + `serde_json`。 |
| `config-forge-runtime-binary-reader` | schema-aware `bin` reader。 | `core`。 |

Codegen 策略：

- 默认 `runtime_interface = "inline"`：generated code 自带最小 reader / loader trait、error、result 与 contract version，避免强制用户引入 runtime core crate。
- 可选 `runtime_interface = "external"`：generated Rust `use config_forge_runtime_reader_core::{RuntimeDataLoader, RuntimeDataReader, ...}`，adapter crate 可直接实现 trait。
- external 模式必须在生成 manifest / 诊断中声明 core crate version 与 `SUPPORTED_CONFIG_FORGE_READER_CONTRACT` 范围。

当前状态：Rust JSON / MessagePack / binary runners 已通过 compliance；`reader_core` package-level tests 直接读取共享 compliance fixture；standard_project Rust harness 覆盖 inline 与 external interface smoke；`runtime_interface_emit = "reference"` 已输出 runtime dependency manifest。正式发布前只剩 registry/package 级 dry-run、crate docs/API polish 与外部发布流程确认，不再阻塞仓库内 stable-ready baseline。

#### Phase R3：TypeScript stable npm packages

TypeScript 可以最早形成稳定 npm 包，因为 generated interface 是结构化类型，不需要 nominal trait 绑定。当前 workspace packages 已补齐 package metadata、CommonJS `exports` / `types`、`sideEffects` 和 reader contract range，可作为 workspace/path package 直接引用；生产接入示例和 release checklist 已在文档中收口，正式 npm 发布前只剩 `npm pack --dry-run` / registry 发布流程确认。

推荐包：

| package | 定位 | 依赖策略 |
| --- | --- | --- |
| `@config-forge/runtime-reader-core` | `RuntimeDataReaderLike`、`RuntimeDataLoaderLike`、limits、error helpers。 | 无 runtime dependency。 |
| `@config-forge/runtime-reader-json` | `JsonValueReader`、`decodeJson`。 | 依赖 core。 |
| `@config-forge/runtime-reader-messagepack` | MessagePack reader。 | 依赖 core，`@msgpack/msgpack` 作为 peer dependency。 |
| `@config-forge/runtime-reader-binary` | `bin` reader。 | 依赖 core。 |
| `@config-forge/runtime-loader-node-fs` | Node fs transport，可注入 decoder。 | 依赖 core，不依赖具体格式。 |

当前状态：使用 TypeScript 5.9.3 对五个 workspace package 逐 package typecheck + build 验证通过；compliance runner 覆盖 JSON、MessagePack、binary。package `exports` 明确当前只支持 CommonJS `require`，未来如增加 ESM 应作为显式 package boundary 变更处理。`runtime_interface_emit = "reference"` 当前诊断为 unsupported，避免在 external import 形态稳定前生成不可执行依赖提示；正式发布前只剩 npm publish / README/API docs polish。

#### Phase R4：C++ stable interface header

状态：核心 header、generated external opt-in、CTest compliance、install/export set/version config 与 `find_package(ConfigForgeRuntime 0.1 REQUIRED CONFIG)` smoke test 已落地；后续重点是生产文档和更多格式特有 negative cases。

目标是解除 C++ adapter 对某次 generated header 的宏绑定，让生产项目可以像普通第三方库一样链接 runtime adapter。

已落地设计：

- 新增 `runtime/cpp/core/include/config_forge/runtime/reader.hpp`，定义稳定 `IRuntimeDataReader`、`IRuntimeDataLoader`、decode limits 和 `CONFIG_FORGE_READER_CONTRACT_VERSION`。
- CMake 提供 `config_forge_runtime::core` target；RapidJSON、msgpack-cxx、binary reader target 链接 core。
- C++ codegen 支持 `runtime_interface = "inline" | "external"`：
  - `inline`：保持当前 generated header 自带 nominal interface，兼容现有用户；
  - `external`：generated code include `config_forge/runtime/reader.hpp`，并使用 core target 的接口类型。
- 迁移期 adapter 可同时支持 generated-header-bound 和 external interface；新生产文档推荐 external。

当前状态：C++ RapidJSON / msgpack-cxx / binary compliance 通过；standard_project C++ harness 覆盖 external interface smoke；CMake install / `find_package` 能重新找到 core 与 binary reader target；`runtime_interface_emit = "reference"` 已输出 CMake/header dependency manifest。后续可继续扩展格式特有 negative cases，但仓库内 stable-ready baseline 已具备生产接入示例和 release checklist。

#### Phase R5：C# / Java package 形态评估

C# / Java 当前保持 copyable template，不承诺 NuGet / Maven 包。原因是它们是 nominal interface：generated code 每次声明自己的 `IConfigForgeRuntimeDataReader` / `ConfigForgeRuntimeDataReader`，标准 reader 需要薄 adapter 接入。当前 JSON、MessagePack 和 Binary 三种 reader template 都已存在，template 内声明版本、supported contract range 和 distribution mode，并通过 standard_project smoke 与首批 compliance runner 验证。

当前审查结论：C# / Java generated inline reader interface 中的方法均被 generated parser 直接调用（field、row、list element、record boundary、enum index、schema revision、exhausted check、scalar），暂未发现类似 `enter_field` / `len` / `read_next_*` / `read_enum_ordinal` 这类只服务 concrete reader 的 helper 污染 core contract。后续若引入 stable interface assembly / package，也应保持这一最小形状；JSON / MessagePack / Binary reader 模板中的解析便利方法仍留在模板类或薄 adapter 中。

推进顺序：

1. 继续扩展 C# / Java JSON、MessagePack 与 Binary 的格式特有 negative cases，保持与 Rust / TypeScript / C++ shared logical cases 一致。
2. 重新评估 C# stable interface assembly / Java stable interface package：只有当 generated code 支持 external runtime interface 时，NuGet / Maven 包才有长期价值；否则继续保持 copyable template 的版本、contract range 和升级说明。
3. 验证 Android / 老 JDK 兼容边界另列，不把 JDK-only reader 的当前能力过度承诺为移动端完整支持。
4. Jackson / Gson 等生态 adapter 作为可选 adapter 层增加，不改变 generated core。

#### Phase R6：Codegen adapter emit / reference

只有当对应语言 adapter 已通过 compliance 且包形态稳定后，才建议给 codegen 增加 adapter 辅助选项：

```toml
[targets.exports.runtime_reader]
adapter = "typescript/json_reader"
emit = "none"       # none / copy / reference
loader = "node-fs"  # fs / fetch / none / custom
```

默认仍为 `emit = "none"`，即只生成核心 generated code。`copy` 和 `reference` 只是接入便利功能，不能让核心 generated code 直接依赖某个 JSON / MessagePack 库。

### 目录布局

推荐在仓库顶层维护 `runtime/` 作为标准 adapter 源码/模板目录：

```text
runtime/
  compliance/
    payloads/             # 跨语言共享 payload cases
    expected/             # scalar / error case expectations
  rust/
    serde_json_reader/
      src/runtime_reader.rs # JsonValueReader、JSON fs loader、generated trait glue
      Cargo.toml          # features: fs
    serde_msgpack_reader/
      src/runtime_reader.rs # MessagePackValueReader、MessagePack fs loader、generated trait glue
      Cargo.toml          # features: fs
  typescript/
    reader_core/
      src/                # shared reader helpers、error helpers
      package.json
    json_reader/
      src/                # JSON.parse value reader
      package.json
    messagepack_reader/
      src/                # @msgpack/msgpack based value reader
      package.json
    loaders/
      node-fs/            # Node fs transport loader
      fetch/              # browser/fetch transport loader
  cpp/
    rapidjson_reader/
      include/config_forge/runtime/rapidjson_reader/ # RapidJSON-backed header-only reader adapter
      CMakeLists.txt
    msgpackc_reader/
      include/config_forge/runtime/msgpackc_reader/ # msgpack-cxx-backed header-only reader adapter
      CMakeLists.txt
    loaders/
      filesystem/         # filesystem transport loader example
      CMakeLists.txt
    cmake/
      ConfigForgeRuntimeConfig.cmake.in
    CMakeLists.txt        # options、find_package、adapter targets、tests
  csharp/
    json_reader/          # System.Text.Json reader + fs loader
    messagepack_reader/   # dependency-free MessagePack subset reader + fs loader
    binary_reader/        # schema-aware bin reader + fs loader
  java/
    json_reader/          # JDK-only JSON reader + fs loader
    messagepack_reader/   # JDK-only MessagePack subset reader + fs loader
    binary_reader/        # JDK-only schema-aware bin reader + fs loader
```

`runtime/` 不是每次 export 的输出目录；它是 ConfigForge 源码仓库中维护的标准 runtime adapter 集合。未来如增加 `yaml`、`sqlite`、自定义二进制等 export type，应按同样结构补充对应语言子目录。

### 官方支持矩阵、推荐等级与依赖策略

对 stable runtime data export type，应覆盖官方 target language；对 experimental type 可以先只支持部分语言。Transport loader 不进入语言 × 格式矩阵，而是与 format reader 组合。每个 adapter 单元必须明确：

- 支持的 generated reader contract 版本；
- 支持的 data payload shape（当前至少 `{ meta, rows }` collection envelope 与 object table root object）；
- 依赖的三方库与启用方式；
- 是否提供 transport loader；
- 是否用于 CI / integration harness；
- 支持等级：`stable`、`preview`、`custom/example`；
- 形态：`standalone_package`、`copyable_template`、`generated_header_bound`。

首批矩阵：

| 语言 | 格式 | adapter 单元 | 支持等级 | 形态 | 依赖与启用方式 | 备注 |
| --- | --- | --- | --- | --- | --- | --- |
| Rust | JSON | `runtime/rust/serde_json_reader` | `preview package` | `standalone_package` + copyable module | `serde_json` | 已有 Cargo metadata 和 package-level tests；暂未发布 crates.io，业务可用 path/git dependency 或复制模块。 |
| Rust | MessagePack | `runtime/rust/serde_msgpack_reader` | `preview package` | `standalone_package` + copyable module | `rmp-serde` + `serde_json::Value` 作为中间 value | 已有 Cargo metadata 和 package-level tests；中期可收敛到真正的 ConfigForge Runtime Value。 |
| Rust | Binary | `runtime/rust/binary_reader` | `preview package` | `standalone_package` | 无第三方依赖 | 已有 Cargo metadata、crate tests 和 shared compliance coverage，可用 path/git dependency 引用。 |
| TypeScript | JSON | `runtime/typescript/json_reader` | `preview package` | `standalone_package` | 原生 `JSON.parse`；CommonJS workspace package | 不依赖 generated.ts，Node fs/fetch loader 分离；已声明 `exports` / `types` / contract range，尚未发布 npm。 |
| TypeScript | MessagePack | `runtime/typescript/messagepack_reader` | `preview package` | `standalone_package` | `@msgpack/msgpack` 作为 peer dependency；CommonJS workspace package | 解码后的 unknown value 复用 core reader 语义；已声明 `exports` / `types` / contract range，尚未发布 npm。 |
| TypeScript | Binary | `runtime/typescript/binary_reader` | `preview package` | `standalone_package` | 无第三方依赖 | 已声明 `exports` / `types` / contract range，并由 TS compliance runner 覆盖 success / malformed cases。 |
| C++ | JSON | `runtime/cpp/rapidjson_reader` | `stable target` | `external_interface` + generated-header-bound 兼容 | CMake option + `find_package(RapidJSON CONFIG)` / package manager / optional `FetchContent` | 用 RapidJSON 完整处理转义、Unicode、数字和 parse error；可通过 CMake target 引入。 |
| C++ | MessagePack | `runtime/cpp/msgpackc_reader` | `stable target` | `external_interface` + generated-header-bound 兼容 | CMake option + `find_package(msgpack-cxx CONFIG)` / package manager / `FetchContent` 固定 `cpp-6.1.1` | 不手写二进制 parser；解码到 msgpack object/value 后适配 reader，可通过 CMake target 引入。 |
| C++ | Binary | `runtime/cpp/binary_reader` | `stable target` | `external_interface` + generated-header-bound 兼容 | 无第三方依赖 | 通过 `config_forge_runtime::binary_reader` target 引入；install + `find_package` smoke 已覆盖。 |
| C# | JSON | `runtime/csharp/json_reader` | `copyable template` | `copyable_template` | .NET BCL `System.Text.Json` | template 声明 `0.1.0` / `^1.0` / `copyable-template`；不承诺 NuGet。 |
| C# | MessagePack | `runtime/csharp/messagepack_reader` | `copyable template` | `copyable_template` | 无第三方依赖，内置 MessagePack subset parser | template 声明 `0.1.0` / `^1.0` / `copyable-template`；当前 baseline 不含第三方 MessagePack adapter。 |
| C# | Binary | `runtime/csharp/binary_reader` | `copyable template` | `copyable_template` | 无第三方依赖 | template 声明 `0.1.0` / `^1.0` / `copyable-template`；standard_project 和 compliance runner 均覆盖。 |
| Java | JSON | `runtime/java/json_reader` | `copyable template` | `copyable_template` | JDK-only JSON parser | template 声明 `0.1.0` / `^1.0` / `copyable-template`；不承诺 Maven / Jackson / Gson / Android baseline。 |
| Java | MessagePack | `runtime/java/messagepack_reader` | `copyable template` | `copyable_template` | JDK-only MessagePack subset parser | template 声明 `0.1.0` / `^1.0` / `copyable-template`；standard_project 和 compliance runner 均覆盖。 |
| Java | Binary | `runtime/java/binary_reader` | `copyable template` | `copyable_template` | JDK-only，无第三方依赖 | template 声明 `0.1.0` / `^1.0` / `copyable-template`；standard_project 和 compliance runner 均覆盖。 |

依赖策略：

- Rust 使用 Cargo feature 控制，如 `json`、`messagepack`、`fs`，默认 feature 尽量为空或只包含 core。
- TypeScript 使用拆包和 peer dependency：`@config-forge/runtime-reader-json` 不依赖 MessagePack；`@config-forge/runtime-reader-messagepack` peer-depend on `@msgpack/msgpack`，避免只用 JSON 的项目被迫安装 MessagePack 包。
- C++ 使用 CMake options / package manager `find_package` 控制第三方库启用，不 vendored 到 generated code。允许在 examples/CI 中用 `FetchContent` 或包管理器（vcpkg、Conan、系统包）下载三方库，但必须是显式 opt-in。
- C# / Java 本阶段保持 copyable template，不发布 NuGet / Maven；只有当 generated code 支持稳定 external runtime interface 后，才重新评估 stable interface assembly / package 的长期价值。
- adapter 版本应与 generated reader contract 版本兼容；当 reader interface 发生 breaking change，adapter 需要同步升级并在包名、feature、manifest 或文档中标明兼容范围。

### C++ CMake 工程化方案

C++ runtime helpers 采用 CMake target 管理，目标是让游戏项目能通过 `add_subdirectory(runtime/cpp)` 或安装后的 `find_package(ConfigForgeRuntime CONFIG)` 引入 adapter。

#### CMake options

```cmake
option(CONFIG_FORGE_RUNTIME_BUILD_TESTS "Build ConfigForge runtime reader tests" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER "Enable RapidJSON reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER "Enable msgpack-cxx MessagePack reader adapter" OFF)
option(CONFIG_FORGE_RUNTIME_FETCH_DEPS "Allow FetchContent for missing third-party deps" OFF)
```

普通业务构建默认不启用 `rapidjson_reader`，避免最容易链接的 target 被误用到生产。测试构建可以在 `CONFIG_FORGE_RUNTIME_BUILD_TESTS=ON` 时显式打开 `CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER=ON`，用于 RapidJSON reader smoke test。生产项目推荐显式启用 RapidJSON 或 MessagePack adapter：

```cmake
add_subdirectory(path/to/ConfigForge/runtime/cpp)

target_link_libraries(game_config_runtime PRIVATE
  config_forge_runtime::rapidjson_reader
  config_forge_runtime::filesystem_loader
)

target_include_directories(game_config_runtime PRIVATE
  ${CMAKE_CURRENT_BINARY_DIR}/Generated/Cpp
)
```

#### CMake targets

| target | 类型 | 说明 |
| --- | --- | --- |
| `config_forge_runtime::core` | interface | 稳定 `IRuntimeDataReader` / `IRuntimeDataLoader` header target。 |
| `config_forge_runtime::binary_reader` | interface | `bin` schema-aware reader target，链接 core。 |
| `config_forge_runtime::rapidjson_reader` | interface | RapidJSON based reader，生产推荐 JSON adapter；启用 `CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER` 后提供。 |
| `config_forge_runtime::msgpackc_reader` | interface | msgpack-cxx based reader，生产推荐 MessagePack adapter；启用 `CONFIG_FORGE_RUNTIME_WITH_MSGPACKC_READER` 后提供。 |

当前 C++ adapter 同时支持 generated inline interface 与 stable external core header：默认仍可通过宏绑定 generated interface；当 generated C++ 使用 `runtime_interface = "external"` 时，adapter 与 generated code 共同使用 `config_forge_runtime::core` 中的 nominal virtual 类型。

#### 三方依赖解析

- `rapidjson_reader` 优先 `find_package(RapidJSON CONFIG QUIET)`；找不到时，如果 `CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON`，可用 `FetchContent` 拉取 RapidJSON；否则给出明确 CMake error，提示通过 vcpkg/Conan/system package 安装。
- `msgpackc_reader` 优先 `find_package(msgpack-cxx CONFIG QUIET)` 或项目约定的 `find_package(msgpack CONFIG QUIET)`；找不到时，如果 `CONFIG_FORGE_RUNTIME_FETCH_DEPS=ON`，可用 `FetchContent` 拉取 `msgpack/msgpack-c` 的 `cpp-6.1.1` tag；否则给出明确 CMake error。
- 三方库只链接到 adapter target，不出现在 generated code target 的 public API 中。业务可替换为引擎内置 JSON / MessagePack 实现，只要实现同一 `IRuntimeDataReader` contract。

#### generated header / stable interface 绑定

短期保持当前模式：C++ adapter include `generated.hpp`，并通过宏指定 generated interface 类型：

```cpp
#define CONFIG_FORGE_RUNTIME_CPP_READER my::game::config::IRuntimeDataReader
#define CONFIG_FORGE_RUNTIME_CPP_LOADER my::game::config::IRuntimeDataLoader
#include "config_forge/runtime/rapidjson_reader/runtime_loader.hpp"
```

Generated C++ 提供 `runtime_interface = "inline" | "external"` 选项。默认 `inline` 保持当前 generated header 自带 interface；production adapter 推荐 `external`，由 generated code 和 adapter 同时 include 稳定 header。这样 adapter library 不再依赖某次 generated header，CMake target 可以更像普通第三方库被复用。

#### C++ reader 语义要求

生产级 C++ adapter 必须覆盖：

- JSON string escape / Unicode / number parse error 的完整库语义，不手写简化 parser；
- MessagePack map / array / integer / float / bool / string / nil；
- `field` 将 missing 与 explicit null 都映射为字段不存在，由 `EnterOptionalField` 的 bool 返回值表达 optional presence；
- reader cursor 必须 owning 或共享持有 root document/value，不能返回指向临时 DOM 节点的悬垂指针；RapidJSON cursor 通常应通过 `std::shared_ptr<Document>` 或等价 owner 维持 root 生命周期；
- field / index cursor 都必须满足同样 ownership 规则；
- `Index` 越界应抛出 `std::runtime_error` 或 ConfigForge runtime 定义的派生异常；C++ `ReadRowCount()` 对 array 返回元素数，对非 array 抛错；
- bool string 兼容 `"true"` / `"false"` / `"1"` / `"0"`，与 ConfigForge validation/export 语义保持一致；
- float 必须有限，非 finite 值报错；
- 错误信息包含 path / field / index 上下文，便于定位运行时数据问题。

#### filesystem loader 安全边界

标准 filesystem loader 只处理 logical data file path 到本地文件的安全映射：

- 禁止 absolute path；
- normalize / lexical clean 后路径必须仍在 data root 内，禁止 `..` path traversal；
- 错误信息同时包含 logical path 和 resolved path；
- 不对大小写敏感文件系统差异做隐式兼容；
- 不负责格式选择，格式 decode 由注入的 JSON / MessagePack decoder 或 adapter target 决定。

### TypeScript MessagePack 工程化方案

TypeScript adapter 使用结构化类型，不需要 import generated module。包结构推荐：

```text
runtime/typescript/
  core/                 # RuntimeDataReaderLike、ValueReader、shared scalar conversion
  json/                 # JsonValueReader、decodeJsonValue
  messagepack/          # MessagePackValueReader、decodeMessagePackValue
  loaders/node-fs/      # FsRuntimeDataLoader，可注入 decoder
  loaders/fetch/        # FetchRuntimeDataLoader，可注入 decoder
```

推荐 npm 包边界：

| package | 依赖 | 导出 |
| --- | --- | --- |
| `@config-forge/runtime-reader-core` | 无 | `RuntimeDataReaderLike`、shared `ValueReader`、scalar conversion helpers。 |
| `@config-forge/runtime-reader-json` | `core` | `JsonValueReader`、`decodeJson`。 |
| `@config-forge/runtime-reader-messagepack` | `core` + peer `@msgpack/msgpack` | `MessagePackValueReader`、`decodeMessagePack`。 |
| `@config-forge/runtime-loader-node-fs` | `core` | Node fs loader；decoder 由 JSON/MessagePack 包注入。 |

Loader / decoder 边界允许同步或异步，保持与 generated `RuntimeDataLoader.open` 的 `Promise<RuntimeDataReader> | RuntimeDataReader` 契约一致：

```ts
export type RuntimeReaderDecoder =
  (bytes: Uint8Array, path: string) => RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
```

`FetchRuntimeDataLoader` 必然 async；Node fs 推荐提供 async `FsRuntimeDataLoader`，同时可以提供测试用 `SyncFsRuntimeDataLoader`，但两者都应通过同一个 decoder 类型组合 JSON / MessagePack reader。

示例：

```ts
import { decode } from "@msgpack/msgpack";
import { MessagePackValueReader } from "@config-forge/runtime-reader-messagepack";
import { FsRuntimeDataLoader } from "@config-forge/runtime-loader-node-fs";
import { GameData } from "./generated";

const loader = new FsRuntimeDataLoader(dataDir, bytes => new MessagePackValueReader(decode(bytes)));
const data = await GameData.load(loader);
```

MessagePack adapter 的 value 语义应与 JSON adapter 对齐：object/map 支持 schema-driven required/optional field entry，array 支持 `index` 与 list length，scalar 支持 string-to-number / string-to-bool 兼容策略，`null/undefined` 在字段入口处按 missing 处理。若 adapter 内部暴露 `field(name)` / `length` helper，它们仍是 concrete reader 的固有方法，不进入 core interface。

### 各语言 contract 约束

不同语言的 reader interface 绑定方式不同，adapter 形态也不同：

- **Rust**：当前 trait 由 generated crate 声明，外部 crate 无法不依赖用户 generated crate 就直接实现该 trait。短期方案是提供可复制 reader module，例如 `runtime/rust/serde_json_reader/src/runtime_reader.rs`，由 codegen option 或用户复制进 generated crate/业务 crate 后实现本地 trait；中期方案是拆出稳定 `config_forge_runtime` support crate，让 generated code 可配置为 `use config_forge_runtime::{RuntimeDataReader, RuntimeDataLoader}`，adapter crate 再直接实现这些 trait。
- **TypeScript**：接口是结构化类型，adapter package 只要导出同形状 class 即可直接传给 `GameData.load`。因此 TypeScript adapter 可以最早做成标准 npm workspace 包。
- **C++**：`IRuntimeDataReader` 是 nominal virtual interface，adapter 需要看到 generated header 或稳定 runtime interface header。短期方案是 adapter header/source 通过 include path 引用 generated header；中期方案是拆出稳定 `config_forge_runtime.hpp`，generated code 和 adapter 都依赖该 header，降低每次生成代码对 adapter 编译的耦合。

### 与 codegen / 测试集成

Codegen 后续可增加 runtime adapter 相关选项，但默认仍不强制输出 adapter：

```toml
[targets.exports.runtime_reader]
adapter = "rust/serde_msgpack_reader"   # 或 "typescript/messagepack_reader", "cpp/rapidjson_reader", "cpp/msgpackc_reader"
emit = "copy"                      # copy / reference / none
loader = "fs"                      # fs / fetch / none / custom
```

配置校验规则：

- `adapter` 的 language 必须与 code target language 匹配，例如 TypeScript target 不能选择 `cpp/rapidjson_reader`。
- `adapter` 的 format 不与 codegen export task 绑定；同一份生成代码可以配合不同格式 adapter 使用。若未来显式配置 adapter，校验只需确认该 adapter 支持目标语言和 reader contract。
- `loader` 必须被所选语言和 adapter 支持，例如 `fetch` 只对提供 fetch transport 的 TypeScript/browser adapter 合法。
- `emit = "reference"` 时，必须能解析到对应 package/crate/CMake target；无法解析时应在 config 阶段报错或生成明确 diagnostic，不应延迟到编译/运行时。
- `emit = "copy"` 时，应记录 adapter source version 和 supported contract range；默认不覆盖用户已修改文件，除非显式配置 `overwrite = true` 或输出到全新 generated adapter 目录。

`emit` 语义：

| emit | 行为 | 适用场景 |
| --- | --- | --- |
| `none` | 只生成核心 generated code；不输出 adapter 文件、不输出依赖接入提示。 | 用户完全自研 loader，或引擎已有资源系统。 |
| `copy` | 复制 adapter source/template 到目标输出目录；同时复制必要 package/Cargo/CMake 片段或 manifest。复制内容必须带 adapter version / contract range 注释。 | 快速接入、示例工程、没有包管理体系的游戏项目。 |
| `reference` | 不复制源码；生成依赖接入提示或 manifest，例如 Rust path/crate dependency、TypeScript npm package、C++ CMake target。 | 正式项目通过包管理或 submodule 引入 runtime support。 |

`copy` 模式应避免覆盖用户手改：adapter 输出目录建议独立于 generated core code，例如 `Generated/Runtime/<adapter>/`；若目标文件已存在且 adapter version 不一致，默认报 diagnostic，提示用户清理或显式允许覆盖。

建议推进顺序：

1. 将 C++ JSON adapter 收敛为 `rapidjson_reader`，移除无依赖 builtin fallback，测试与生产都通过 RapidJSON 路径验证。
2. 将 Rust / TypeScript templates 整理为清晰的 copyable crate/package template，并补齐版本与 contract range 标注。
3. 评估 Phase 1.5 C++ stable interface header，让 C++ adapter 从 generated header bound 迁移到 external interface。
4. 集成测试 harness 改为引用或复制这些 adapter，避免 harness 内长期维护独立 reader 实现。
5. 增加 adapter 自测：每个 adapter 至少用同一份 canonical payload 验证 field/index/len/scalar/null、missing vs null、string-to-number/bool、finite float、escaped string。
6. 再考虑是否提供 codegen option 自动复制 adapter 到目标工程输出目录。

### Adapter compliance suite 与 CI / 测试矩阵

canonical payload 应升级为正式 compliance suite，而不是只停留在矩阵描述。推荐目录：

```text
tests/fixtures/runtime_reader_compliance/
  payloads/
    collection_envelope.json
    object_table.json
    scalar_compat.json
    missing_vs_null.json
    nested_struct.json
    fragment.json
    escaped_string.json
  expected/
    scalar_results.toml
    error_cases.toml
```

Rust / TypeScript / C++ / C# / Java adapter 都必须跑同一套 compliance cases。覆盖范围包括 missing vs null、bool `true/false/1/0`、numeric string、NaN/Infinity、int overflow、escaped string、array 越界、MessagePack 非 string map key、binary/ext unsupported 等场景。MessagePack cases 可以由同一份 logical fixture 生成 `.msgpack` payload，避免 JSON 与 MessagePack fixture 漂移。

标准 runtime helpers 的 CI 至少覆盖：

| 层级 | Rust | TypeScript | C++ | C# | Java |
| --- | --- | --- | --- | --- | --- |
| adapter / compliance | `tests/runtime_compliance.rs` 复用 `runtime/rust/serde_json_reader` 与 `serde_msgpack_reader` | `tests/typescript_runtime_compliance.rs` 临时工程覆盖 JSON / MessagePack | `cmake -S runtime/cpp -B build -DCONFIG_FORGE_RUNTIME_BUILD_TESTS=ON ... && cmake --build build && ctest` | `tests/csharp_java_runtime_compliance.rs` 临时 `.NET` project 覆盖 JSON / MessagePack / Binary | `tests/csharp_java_runtime_compliance.rs` 临时 `javac` / `java` harness 覆盖 JSON / MessagePack / Binary |
| format matrix | JSON + MessagePack | JSON + MessagePack | RapidJSON JSON、msgpack-cxx MessagePack、Binary | JSON、MessagePack、Binary | JSON、MessagePack、Binary |
| canonical payload | collection envelope、object table、nested struct、fragment object、missing/null、bool `1/0`、finite float、escaped string | 同左，另含 MessagePack-only negative cases | 同左，binary 另覆盖 CFGB header / schema revision / record boundary / sequential rows | 同左，binary 另覆盖 CFGB header / schema revision / record boundary / scalar reads | 同左，binary 另覆盖 CFGB header / schema revision / record boundary / scalar reads |
| integration harness | standard_project Rust JSON / MessagePack / Binary glue 与 external smoke | standard_project TypeScript JSON runtime smoke；MessagePack reader 由 compliance 覆盖 | standard_project C++ RapidJSON / binary 代表路径与 external smoke | standard_project C# JSON / MessagePack / Binary smoke | standard_project Java JSON / MessagePack / Binary smoke |

CI 中三方依赖策略：

- 默认快速 CI 只跑无三方或已缓存依赖：Rust、TS JSON、C# / Java dependency-free reader、C++ binary 或已安装依赖的 C++ reader。
- 完整 CI / nightly 跑 production adapter：TS MessagePack、C++ RapidJSON、C++ msgpack-cxx、C# / Java JSON / MessagePack / Binary。
- 如果三方包不可用，测试应明确 skip 或 fail-fast 给出安装提示，不能静默退回 builtin parser 造成覆盖误判。

## Reader loader 落地状态

迁移已完成：

1. [`Codegen backend capability matrix`](09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) 中的 format-agnostic backend 均生成 `RuntimeDataReader` trait/interface 和通用 loader open API；具体命名见矩阵和对应 backend 专题文档。
2. table row、object table、named struct、fragment 均由生成代码输出 reader parser helper。
3. table/object `load` 已从 `loader.load_<table>_rows(DATA_FILE)` 切换为 `loader.open(DATA_FILE)`，由生成代码读取 `rows` 或 object payload。
4. codegen 单测和 standard_project harness 已改为通用 reader loader，不再按表手写 typed rows / objects。

当前验收标准：生成代码不再出现按表的 loader 方法；新增表、字段、fragment 或 object table 字段时，用户侧 loader / reader 接口和 harness loader 实现不需要改动，只需要重新生成代码并更新业务断言。
