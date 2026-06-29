# Runtime Reader Parser Adapter Alignment TODO

## 背景

当前标准 runtime reader contract 已经稳定，但各语言对 JSON / MessagePack 这类标准格式的 adapter 依赖策略不一致：

- Rust 明确提供 `serde_json_reader` 与 `serde_msgpack_reader`，reader 名称绑定具体生态库。
- C++ 明确提供 `rapidjson_reader` 与 `msgpackc_reader`，reader 名称绑定具体三方库。
- TypeScript `messagepack_reader` 当前以 injectable decoder 方式工作，标准包没有直接给出一套默认 `@msgpack/msgpack` reader + loader 组合。
- C# JSON reader 基于 .NET BCL `System.Text.Json`，但命名仍是泛化 `json_reader`；MessagePack reader 是 dependency-free subset parser。
- Java JSON / MessagePack reader 是 JDK-only 手写 parser / subset parser；Java 标准库本身没有 JSON / MessagePack parser，这会让“标准 adapter”承担格式解析器维护责任。

这会带来两个问题：

1. **命名和依赖策略不统一**：C++ 的 `rapidjson_reader` 表达“这是 RapidJSON adapter”；Java/C#/TS 的 `json_reader` / `messagepack_reader` 更像“ConfigForge 自己维护标准格式 parser”。
2. **维护边界不合理**：JSON / MessagePack 是成熟标准格式，ConfigForge reader 应优先适配生态中被维护的 parser/value model，而不是长期维护手写 parser。手写解析器只适合作为测试 fixture、最小 bootstrap、custom/example 或 schema-aware `bin` reader，而不应成为标准格式 adapter 的主推荐方案。

## 结论

这个方向是合理的：**标准格式 reader adapter 应绑定一个明确 parser backend，并以 backend 命名；generated code 仍只依赖 ConfigForge reader contract，不直接依赖 parser 库。**

推荐长期命名原则：

```text
runtime/<language>/<parser_or_backend>[_<format>]_reader
```

其中 `<format>` 只在 backend 名称本身无法表达格式时补充；如果 backend 名已经包含格式信息，不重复追加格式。也就是说，我们不会命名为 `rapidjson_json_reader` 或 `msgpackc_msgpack_reader`，而是使用 `rapidjson_reader`、`msgpackc_reader`；但 `serde` 本身只是 Rust 序列化生态，不指明格式，因此需要 `serde_json_reader`、`serde_msgpack_reader`。

示例：

- Rust：`serde_json_reader`、`serde_msgpack_reader`（现状符合）
- C++：`rapidjson_reader`、`msgpackc_reader`（现状符合）
- C#：`system_text_json_reader`、`messagepack_csharp_reader`
- Java：`jackson_reader`、`gson_reader`、`msgpack_core_reader`（具体首选见下方 TODO）
- TypeScript：`msgpack_msgpack_reader` 或继续 `messagepack_reader` 但 package 明确依赖 / peer-depend `@msgpack/msgpack` 并提供默认 decoder factory

如果未来要支持其他库，新增并列 adapter，而不是修改既有 adapter 的语义。例如 C++ 可以同时有 `rapidjson_reader`、`simdjson_reader`；Java 可以同时有 `jackson_reader`、`gson_reader`；C# 可以同时有 `system_text_json_reader`、`newtonsoft_json_reader`。

`binary_reader` 例外：ConfigForge `bin` 是自有 schema-aware runtime wire format，手写 reader 是合理且必要的标准实现。

## P1：对齐标准格式 adapter 依赖与命名

### P1-1：确定每个语言的推荐 parser backend

- [x] C# JSON：推荐 backend 固定为 .NET BCL `System.Text.Json`；当前 `runtime/csharp/json_reader` 先作为 compatibility path，metadata 标注 `parser_backend = "System.Text.Json"`，后续可新增或改名为 `runtime/csharp/system_text_json_reader`。
- [x] C# MessagePack：推荐 backend 固定为 `MessagePack-CSharp`；新增/迁移目标为 `runtime/csharp/messagepack_csharp_reader`。
- [x] Java JSON：推荐 backend 固定为 Jackson Databind（tree/value model 成熟，服务器生态广泛）；新增目标为 `runtime/java/jackson_reader`。Gson 保留为 P2 可选 adapter。
- [x] Java MessagePack：推荐 backend 固定为 Jackson Dataformat MessagePack（复用 Jackson `JsonNode` tree adapter，避免同时维护两套 Java named-reader cursor）；新增目标为 `runtime/java/jackson_messagepack_reader`。`msgpack-core` 可作为 P2 低层 streaming adapter 另行评估。
- [x] TypeScript MessagePack：推荐 backend 固定为 `@msgpack/msgpack`；当前 injectable decoder 保留为 lower-level/custom hook，但标准 reader/loader 应提供默认 backend。
- [x] TypeScript JSON：`JSON.parse` 是语言内建能力，`json_reader` 命名可接受；若未来提供 streaming/large-payload adapter，应以具体库命名另建 package。

### P1-2：调整 runtime 目录与 package/template 形态

- [ ] 新增 library-bound adapter 目录，不在第一步直接删除旧目录，避免破坏现有 compliance 和 standard_project harness。
- [ ] 每个新 adapter 的 README/package metadata/Cargo/CMake/NuGet/Maven/npm 信息必须声明：parser backend、backend version range、reader contract range、distribution mode。
- [ ] 对现有泛化目录制定迁移策略：
  - `runtime/csharp/json_reader`：已保留为 compatibility path，并新增 backend-named `runtime/csharp/system_text_json_reader` wrapper 作为 generated adapter 推荐入口。
  - `runtime/csharp/messagepack_reader`：降级为 `custom/example` 或 `legacy_dependency_free_messagepack_reader`，不作为推荐 Stable adapter。
  - `runtime/java/json_reader`：降级为 `custom/example` / compliance bootstrap，不作为推荐 Stable adapter。
  - `runtime/java/messagepack_reader`：降级为 `custom/example` / compliance bootstrap，不作为推荐 Stable adapter。
  - `runtime/typescript/messagepack_reader`：已保留 package 名并在 package metadata / 默认 decode 入口中绑定 `@msgpack/msgpack`；injectable decoder API 保留为 advanced/custom hook。

### P1-3：更新 generated code copy/reference 选项

- [x] C# / Java 的 `runtime_interface = copy` 不应默认复制手写 JSON / MessagePack parser 作为“标准推荐”；当前 generated adapter 注释和 unsupported `reference` 诊断均显式标出 C# JSON 的 `System.Text.Json` backend、C# / Java legacy-bootstrap 状态，以及后续 library-bound adapter 目标。
- [x] `runtime_interface_emit = "reference"` manifest 对 library-bound adapter 增加 backend identity，例如 `parser_backend = "jackson"` / `"System.Text.Json"` / `"@msgpack/msgpack"`。当前 Rust / C++ reference manifest 已输出 `runtime_readers[].parser_backend` / `parser_backend_version` / `adapter_support_level`；C# / Java 在 library-bound package 稳定前继续拒绝 `reference` 并在诊断中说明 backend 状态。
- [x] 文档中明确：generated code 只依赖 ConfigForge reader contract；parser 库依赖只存在于 adapter package/template，不进入 generated core。

### P1-4：同步 compliance 与 integration harness

- [ ] `tests/csharp_java_runtime_compliance.rs` 增加 library-bound C# / Java adapter runner；旧 hand-written reader runner 可保留为 custom/example 覆盖，但不能作为唯一 Stable 标准。
- [x] `tests/typescript_runtime_compliance.rs` 对 MessagePack runner 使用标准 `@msgpack/msgpack` decode 路径；测试中通过本地 shim 保持离线和确定性，injectable decoder 只保留为 advanced/custom hook。
- [x] `tests/cli_integration/support/standard_runtime.rs` 的 TypeScript standard_project harness 迁移到推荐 adapter，确保用户样例使用默认 `@msgpack/msgpack` 接入路径。
- [x] release metadata drift test 增加 parser backend / dependency policy 校验，避免 package 名称、文档和实际依赖漂移。

## P2：补充可选生态 adapter

- [ ] C++：按需求新增 `simdjson_reader` 或其他 JSON backend reader，保持与 `rapidjson_reader` 并列。
- [ ] Java：在 Jackson 稳定后评估 `gson_reader`、Android 友好 reader。
- [ ] C#：在 `System.Text.Json` 稳定后评估 `newtonsoft_json_reader`。
- [ ] TypeScript：按需要提供 streaming JSON / binary transport 组合；避免把 transport loader 与 format reader 强耦合。

## 验收标准

- [ ] 标准 JSON / MessagePack adapter 都有明确 parser backend identity；文档、目录名、package metadata 和 manifest 一致。
- [ ] 标准格式 reader 不再以手写 parser 作为主推荐实现；手写 parser 若保留，必须标注为 custom/example/legacy/bootstrap。
- [ ] `cargo test --test csharp_java_runtime_compliance`、`cargo test --test typescript_runtime_compliance`、C++ CTest compliance 和 full `cargo test` 通过。
- [ ] standard_project runtime harness 使用推荐 adapter 路径，不再暗示用户应复制 ConfigForge 自维护 JSON / MessagePack parser。
