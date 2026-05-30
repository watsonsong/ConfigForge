# Runtime binary format TODO

本文记录 `bin` runtime data 首版实现后的后续工作。当前实现已覆盖导出器、Rust / TypeScript / C++ codegen 以及三端基础 runtime reader；以下事项用于跟进稳定化、集成体验和跨语言一致性。

## P0：首版稳定化前必须完成

- [x] 统一 Rust runtime reader contract。
  - 已选择共享 core crate 模式：`runtime/rust/reader_core` 定义 `RuntimeDataReader` / `RuntimeDataLoader`，generated Rust 通过 `config_forge_runtime_reader_core` 复用同一套 trait。
  - Rust runtime helper crates 不再各自定义同名 trait，避免名义类型不兼容。
- [x] 补齐 optional scalar / struct field 的目标语言类型表达。
  - CodeModel 已传播 optional 字段语义，Rust / TypeScript / C++ 分别生成 `Option<T>`、optional property、`std::optional<T>`。
  - Binary exporter 现在可通过 presence bitmap 省略缺失 optional scalar / struct field，不再因 generated binary reader 不支持而拒绝导出。
- [x] 增加真实 generated code + binary runtime helper 的端到端测试。
  - Rust：standard project generated crate 通过 `config-forge-runtime-binary-reader` 加载 `.bin` fixture。
  - TypeScript：standard project generated code 通过 `decodeBinaryRuntimeData` 加载 `.bin` fixture。
  - C++：standard project generated C++ 通过 binary adapter 编译并加载 `.bin` fixture。

## P1：首版发布前建议完成

- [x] 明确 TypeScript `int` contract。
  - 已选择方案二：TypeScript codegen 继续将 Meta `int` 生成为 `number`，不生成 `bigint`。
  - `bin` exporter 会拒绝超出 JavaScript safe integer 范围的 `int` 值，避免 TypeScript binary reader 产生不可精确表达的 number。
  - 如果未来确实需要超过 safe integer 的整数，会额外定义特殊 primitive type，而不是改变现有 `int` contract。
- [x] 为 binary reader 增加可配置 decode limits。
  - 已完成：Rust / TypeScript / C++ binary reader 均支持默认 limits 与业务覆盖，覆盖 `max_file_bytes`、`max_rows`、`max_array_length`、`max_string_bytes`、`max_nesting_depth`。
  - 已完成：文件大小在读取 payload 前预检查，`row_count`、list length、string length、record nesting 在消费/分配前检查。
- [ ] 改进 C++ adapter 与 generated namespace/include 的集成方式。
  - 当前 adapter 仍默认 include `generated.hpp` 并假设 `reference::game::config` namespace。
  - 后续应提供稳定 alias / macro header，或要求用户显式定义 reader/loader 类型宏，并用真实 generated integration test 覆盖。
- [ ] 建立跨语言 binary compliance fixtures。
  - 覆盖 trailing bytes、invalid UTF-8、oversized varuint、oversized row count / string length、schema revision mismatch、optional fragment presence 等场景。

## P2：后续重构与体验优化

- [ ] 将 binary schema revision / layout signature 抽成清晰的共享 contract 模块。
  - 当前 codegen 通过 `pub(crate)` 复用 exporter 内部 revision 逻辑，可以保证一致性，但模块边界偏重。
  - 后续可抽为 `runtime_format` / `binary_schema` 类模块，只暴露窄 API。
- [ ] 完善 binary reader 错误上下文。
  - 建议错误信息包含 table、field path、row index、byte offset、expected / actual 等信息。
- [ ] 增加 canonical varuint 校验策略。
  - 当前已校验第 10 字节 u64 上界。
  - 后续如要求 canonical encoding，应拒绝非最短 ULEB128 表达。
- [ ] 补全文档中的 runtime helper 使用示例。
  - 覆盖 JSON / MessagePack / bin 三种格式 reader 的组合方式。
  - 明确各语言包的安装、构建和集成约束。
