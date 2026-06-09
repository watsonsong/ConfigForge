# Runtime Reader Contract Stabilization TODO

本文拆分 `schema-derived binary materializer`、共享 runtime compliance corpus、跨语言错误一致性，以及 runtime package / distribution 稳定化工作。结论：这不是适合一次性完成的单点任务，而是跨 exporter、fixture、runtime adapter、测试矩阵和发布形态的稳定化工作流，应按下列步骤逐步执行、逐步提交。

## 范围判断

该工作流同时影响：

1. `bin` 格式测试数据生成：需要从 schema / logical case 自动派生 binary bytes，避免继续手写 happy-path fixture。
2. Runtime reader compliance：JSON、MessagePack、Binary 应尽量共享同一批 logical cases，减少格式间语义漂移。
3. Binary negative corpus：bad magic、unsupported version、truncated header、schema revision mismatch、invalid bool、invalid UTF-8、varuint overflow、presence bitmap / record boundary 错误等仍需要格式特有 malformed fixture。
4. 跨语言 reader 错误一致性：Rust、TypeScript、C++、C#、Java 的错误不必逐字一致，但应有稳定 error kind / keyword、reader path、expected / actual type 信息。
5. Runtime distribution：Rust crate、TypeScript npm package、C++ CMake install / `find_package`、C# / Java copyable template 或正式包策略，都需要 package metadata、版本范围、测试和生产接入示例。

因此推荐先完成 R1/R2 的“可执行规范和 fixture 自动化”，再推进 R3/R4 的包形态稳定；不要在 compliance 自动化不足时先承诺长期 package API。

## 执行原则

- 每一步都应独立可提交；除非当前步骤显式要求，不同时改多个语言的 package 发布形态。
- 共享 logical case schema 先稳定，再让各语言 runner 接入，避免五种语言各自发明 fixture 结构。
- Binary happy-path fixture 应由 schema-derived materializer 生成；malformed binary fixture 保持手写或 byte-level builder，以便精确覆盖非法 wire。
- 错误文本不要求完全一致，但必须断言稳定关键字、path、expected type、actual type/value 或 error category。
- Runtime package 稳定化必须以 compliance runner 作为 package-level test，不只验证能编译。

## Step 1：实现 schema-derived binary materializer

- [x] 明确 materializer 输入：logical compliance case、table schema/layout、expected schema revision、target visibility。
- [x] 复用现有 binary layout / wire helper 规则，输出 CFGB header、row/object payload、presence bitmap、typed scalar bytes。
- [x] 覆盖 primitive、struct、list、fragment、ObjectTable、CollectionTable 的 happy-path binary materialization。
- [x] 对 `int` JavaScript safe integer、finite float、UTF-8 string 等规则增加 materializer 侧断言。
- [x] 增加 Rust reference compliance 测试：同一 logical case 生成 deterministic bytes；reader 看到 schema-derived revision。

Batch A 记录：当前 materializer 先落在 Rust runtime compliance 测试侧，用 shared JSON payload + 测试侧 schema layout 派生 `.bin` bytes；暂不引入持久化 `.bin` fixture 文件，避免在 cross-language runner 统一读取方式确定前增加重复 fixture。enum happy-path 暂无现有 shared logical case，先由 malformed corpus 覆盖 ordinal 越界，后续 Batch B/C 需要补一个 enum success logical case。

交付标准：binary happy-path compliance fixture 不再依赖手写 bytes；新增或修改 logical case 时可自动派生 `.bin` payload。

## Step 2：统一 shared logical runtime compliance corpus

- [x] 复用现有 shared JSON payload + TOML expected 作为第一批 logical corpus 来源。
- [x] 将 Binary 首批 success case 归并到同一批 logical payload，binary bytes 由 materializer 派生。
- [x] 明确 JSON / MessagePack named payload 与 Binary positional payload 的等价边界：missing/null、scalar conversion、list length、struct/fragment、object/collection envelope。
- [x] 保留 format-specific case 扩展点，malformed binary 不强行放进通用 logical success case。
- [x] 更新 Rust compliance runner 作为第一批 reference runner。

Batch A 记录：没有新增独立 `logical_cases.toml`，而是先让 Rust reference runner 复用现有 payload/expected 文件并补齐 binary positional operation sequence。后续 Batch B 若要让 TypeScript / C++ / C# / Java 共享同一 operation plan，再决定是否抽出独立 case schema。

交付标准：Rust runner 能基于同一批 logical cases 验证 JSON、MessagePack、Binary 的一致 contract 语义。

## Step 3：补齐 binary-only malformed corpus

- [x] Header negative：bad magic、unsupported version、wrong header size、truncated header。
- [x] Revision negative：schema revision mismatch；错误优先于 payload decode。
- [x] Wire negative：truncated varuint、varuint overflow、invalid UTF-8、invalid bool byte。
- [x] Layout negative：invalid optional bitmap、trailing bytes。
- [x] Enum negative：enum ordinal 越界。
- [x] 为每个 malformed fixture 定义稳定 error keyword，而不是只断言“失败”。

Batch A 记录：malformed corpus 当前也先落在 Rust reference runner 内部的 byte-level builder，覆盖 header / revision / wire / bitmap / ordinal / trailing bytes。后续 Batch B 接入其他语言时，应把这些 case 的 ID、bytes 来源和 expected keyword 抽成共享数据，减少各语言内联手写。

交付标准：Binary reader 的格式特有错误由 shared corpus 覆盖，且每个语言 runner 至少验证核心 header / revision / wire / boundary negative case。

## Step 4：跨语言 runner 接入共享 corpus

- [x] TypeScript runner 接入 shared logical corpus 与 binary materialized payload。
- [x] C++ CTest runner 接入 shared logical corpus，覆盖 RapidJSON / msgpack-cxx / binary adapter。
- [x] C# runner 接入 shared logical corpus，覆盖 JSON / MessagePack / Binary reader template。
- [x] Java runner 接入 shared logical corpus，覆盖 JSON / MessagePack / Binary reader template。
- [x] 明确缺工具链时 local skip / full CI fail-fast 策略，保持与测试矩阵文档一致。

Batch B1 记录：Rust、TypeScript、C++、C#、Java 的 binary success coverage 已扩展到同一批 logical case：scalar、optional null、collection list、ObjectTable、nested struct、fragment。当前仍是各语言 runner 内的轻量 byte builder，而不是共享持久化 `.bin` corpus；这样避免过早引入 fixture 生成/提交流程，但后续如果继续增加 binary case，应优先抽出共享 operation/case schema，避免重复维护。

交付标准：Rust、TypeScript、C++、C#、Java 使用同一 logical case ID 报告结果；新增 case 不需要在每种语言复制一份语义定义。

## Step 5：统一 reader error taxonomy 与断言策略

- [x] 初步梳理 reader error category：missing field、wrong type、parse/range overflow、invalid enum ordinal、decode limit、malformed binary、schema revision mismatch、not exhausted。
- [x] 现有 runner 统一采用稳定 keyword 断言，避免全文匹配。
- [ ] 修改 compliance expected 文件，从全文匹配改为 category / keyword / path / expected / actual 的结构化断言。
- [ ] 修正各语言 reader 中缺失 path 或 expected/actual 的错误。
- [ ] 保留语言惯用 error type；不强制五种语言使用同名异常类。

Batch B1 记录：本批先统一 binary success logical coverage，并维持现有 keyword-based negative assertion；尚未改造 expected schema 为结构化 error taxonomy。下一批应把 Step 5 拆成独立 Batch B2，集中处理 `expected_kind/category/path/expected/actual` schema 和各语言错误上下文补齐，再进入 package 稳定化。

交付标准：跨语言错误不会因标点或自然语言微调导致测试脆弱，但关键诊断信息稳定可验证。

## Step 6：Rust runtime crate 稳定化

- [ ] 明确 `runtime/rust/reader_core` crate 的 public API：contract version、reader trait、loader trait、error、decode limits。
- [ ] 为 JSON / MessagePack / Binary reader 选择 crate 边界：独立 crate、feature flag，或 workspace 内 packages。
- [ ] 补齐 `Cargo.toml` metadata、features、docs、examples 和 license / repository 字段。
- [ ] package-level tests 直接运行 compliance runner。
- [ ] 生成代码 `emit = "reference"` 时能提示所需 crate 名称、版本范围和 contract range。

交付标准：Rust runtime support 具备可发布 crate 形态，即使暂不发布 crates.io，也可被业务项目以 git/path dependency 稳定引用。

## Step 7：TypeScript npm package 稳定化

- [ ] 明确 package 拆分：core、json reader、messagepack reader、binary reader、node fs loader。
- [ ] 补齐 `package.json` exports、types、sideEffects、peer dependency、版本和 contract range。
- [ ] 增加 package-level tests，覆盖 ESM / CJS 或明确只支持其中一种。
- [ ] 校验 generated TypeScript `emit = "reference"` 对 npm package 的引用方式和错误提示。
- [ ] 编写最小生产接入示例：同步 loader、lazy facade、binary payload。

交付标准：TypeScript runtime helper 可作为 npm workspace package 被引用，API 边界和依赖策略清晰。

## Step 8：C++ CMake install / find_package 稳定化

- [ ] 补齐 `runtime/cpp/core` install target、export set、version file 和 `ConfigForgeRuntimeConfig.cmake`。
- [ ] 为 RapidJSON、msgpack-cxx、binary adapter 明确 header-only target 与 optional dependency policy。
- [ ] 增加 `find_package(ConfigForgeRuntime)` 示例工程，并在 CI / test profile 中编译。
- [ ] 校验 generated C++ `runtime_interface = "external"` 与 installed core header 的兼容性。
- [ ] 记录 CMake minimum version、namespace target 命名和 ABI / header-only 兼容策略。

交付标准：C++ 项目可通过 install + `find_package` 使用 stable core header 和可选 adapters，而不是依赖源码相对路径。

## Step 9：C# package / template 策略定稿

- [ ] 评估是否提供 stable interface assembly / NuGet package，或继续保持 copyable template。
- [ ] 若选择 NuGet：定义 target framework、package metadata、public API、source generator / template 边界和 compliance tests。
- [ ] 若选择 copyable template：补齐模板版本、contract range、升级指南和 generated code glue 约束。
- [ ] 明确第三方生态 adapter（如 Newtonsoft.Json）的支持级别；当前 BCL / dependency-free reader 仍作为 baseline。
- [ ] 增加最小生产接入示例。

交付标准：C# runtime helper 的分发策略明确，业务项目知道应该引用包、复制模板，还是接入自定义 reader。

## Step 10：Java package / template 策略定稿

- [ ] 评估是否提供 Maven package，或继续保持 copyable JDK-only template。
- [ ] 若选择 Maven：定义 groupId/artifactId、Java baseline、module-info 策略、public API 和 compliance tests。
- [ ] 若选择 copyable template：补齐模板版本、contract range、升级指南和 generated code glue 约束。
- [ ] 明确 Jackson / Gson / Android 等生态 adapter 的支持级别；当前 JDK-only reader 仍作为 baseline。
- [ ] 增加最小生产接入示例。

交付标准：Java runtime helper 的分发策略明确，且不会在没有 compliance 覆盖时过早承诺生态 adapter。

## Step 11：文档与状态矩阵收口

- [ ] 更新 runtime adapter 详细设计，记录 materializer、shared corpus、error taxonomy 和各语言 package 状态。
- [ ] 更新 binary format 详细设计中的测试计划，区分 schema-derived happy-path 与 malformed byte-level corpus。
- [ ] 更新测试矩阵文档，记录 local / quick / full / nightly 对 runtime compliance 的覆盖边界。
- [ ] 根据实际完成情况评估 `standard runtime helpers` 是否仍为 `Implemented MVP`，或是否可细分出稳定的 contract 层。
- [ ] 更新用户可见 runtime loading / package reference 文档，避免把未发布包描述为稳定依赖。

交付标准：Design / Implementation / Tech 文档对 runtime reader contract、binary fixture 生成和 package 分发状态表述一致。

## 建议执行批次

1. **Batch A：可执行规范基础** — Step 1、Step 2、Step 3。先解决 binary materializer 和 corpus，不碰 package 发布承诺。
2. **Batch B：跨语言一致性** — Step 4、Step 5。把所有语言 runner 拉到同一套 case / error taxonomy。
3. **Batch C：Rust / TypeScript / C++ 稳定引用形态** — Step 6、Step 7、Step 8。优先稳定最可能被生产项目直接引用的三类生态。
4. **Batch D：C# / Java 分发策略** — Step 9、Step 10。根据是否需要正式包决定 NuGet / Maven 或 copyable template。
5. **Batch E：文档与状态收口** — Step 11。只根据已完成事实更新状态矩阵，不提前标 Stable。
