# Phase 0004 Developer Log — Streaming Artifacts, Backend Matrix Expansion, and Cross-language Runtime Readers

## Timebox

- Approximate period: after `phase-0003` through 2026-06-07.
- Snapshot commit range observed locally:
  - after `20db79f docs(devlogs): record phase 0003 development`
  - through `fd77b64 docs: centralize status and backend references`

## Scope

Phase 0003 ended with stage-level build tracing, bounded artifact write memory, format-agnostic runtime loading, and Rust / C++ / TypeScript generated relation helpers. Phase 0004 shifted the project from “some backends plus runtime contract direction” to a broader multi-language codegen platform with sink-first output, explicit backend capability tracking, and reusable runtime reader templates for C# and Java.

The major work was:

- make artifact generation and code generation sink-first, removing the old `Vec<Artifact>` materialization path from production codegen/export paths;
- simplify artifact write resource management after streaming writes became the main authority;
- make source-order table access explicit and opt-in in generated APIs;
- add ordered index storage and lookup coverage across the remaining backends;
- implement the Unreal C++ backend as an independent `cpp_unreal` generator with containers and Blueprint-facing modes;
- implement the C# backend end-to-end, including rows/objects, indexes, relations, text helpers, lazy loading, and standard fixture harness coverage;
- implement the Java backend end-to-end, including Java naming/type mapping, runtime reader contract, indexes, relations, text helpers, lazy loading, and standard fixture harness coverage;
- introduce C# and Java JSON runtime reader templates under `runtime/` and change standard fixture harnesses to reuse those templates through thin generated-interface adapters;
- define cross-language toolchain guidance and a phased TODO for future ToolProbe / compliance runner work;
- centralize codegen backend status into a single capability matrix to avoid duplicating language lists across documents.

This log is historical context. The current canonical behavior remains in `Documentation/Design/`, `Documentation/Implementation/`, and `Documentation/Tech/`.

## Status Snapshot

### Implemented Capabilities Added or Stabilized in This Phase

- Artifact and codegen streaming:
  - `ArtifactSink` became the production write authority for runtime table exporters, aggregate exporters, and generated code;
  - table exporters and aggregate exporters moved to stream-first writing;
  - `CodeGenerator::generate_to_sink()` became the production path for codegen output;
  - Rust, TypeScript, C++, and later backends write through sink-oriented code writers rather than returning materialized generated files in production;
  - stale artifact compatibility paths and old memory-limit primitives were removed after the sink lifecycle and temp-file behavior were locked down.
- Generated API ordering semantics:
  - source-order access became explicit via `preserve_source_order`, while indexed tables hide source-order internals by default;
  - tests now guard indexed source-order API behavior so future backend changes do not re-expose implicit row order accidentally.
- Backend expansion:
  - `cpp_unreal` was added as a separate backend rather than as a mode of standard C++;
  - C# and Java were implemented as standalone format-agnostic codegen backends;
  - C++ / Unreal C++ / C# / Java gained hash indexes, ordered indexes, compound keys, relation helpers, text helpers where applicable, and lazy game-data facades.
- Unreal C++ specifics:
  - Unreal generated code uses Unreal-facing containers/types and supports containers and Blueprint modes;
  - repository tests use a lightweight shim and syntax smoke instead of requiring a full Unreal Engine install;
  - real UHT / Editor / cook integration remains a downstream or dedicated CI responsibility.
- C# specifics:
  - C# backend generates BCL-only single-file `.cs` code with generated runtime loader/reader interfaces;
  - generated API includes `GameData`, `LazyGameData`, `IGameDataAccess`, index lookup, relation helpers, and text translation helpers;
  - standard project tests compile and run via `dotnet run`.
- Java specifics:
  - Java backend generates one `.java` file with a public root class and nested public static generated types;
  - generated API includes `GameData`, `LazyGameData`, `GameDataAccess`, index lookup, relation helpers, and text translation helpers;
  - Java deliberately stays JDK-only in generated core and avoids Maven / Gradle / Jackson / Gson requirements in the standard harness.
- Runtime readers:
  - `runtime/csharp/json_reader` now provides a `System.Text.Json`-based reader and filesystem loader template;
  - `runtime/java/json_reader` now provides a JDK-only JSON parser, reader, and filesystem loader template;
  - standard project C# / Java harnesses copy these runtime templates and only keep thin adapters for the generated nominal interfaces.
- Testing and developer environment:
  - integration tests were split into support modules to keep the growing cross-language matrix maintainable;
  - standard project fixture now includes C# and Java codegen targets in addition to existing languages;
  - developer environment guidance now separates required Rust tooling from optional target-language toolchains;
  - a phased TODO captures future ToolProbe, test profile, compliance runner, and adapter work.
- Documentation consolidation:
  - codegen backend status moved to a canonical backend capability matrix in `09-codegen-module-detail-design.md`;
  - broader design docs now link to that matrix instead of repeating language lists;
  - Meta model status was redirected to `Design/README.md` to reduce stale per-document status matrices.

### Verification Snapshot

Representative verification commands run during this phase included:

```text
cargo fmt
git diff --check
cargo test --test cli_integration standard_project_generated_code_loads_exported_runtime_data
cargo test code_artifact::csharp
cargo test code_artifact::java
```

Observed results near the end of the phase:

- formatting and diff checks passed;
- standard project generated-code runtime test passed locally, including C# `dotnet run` and Java `javac` / `java` harnesses where the tools were available;
- C# codegen unit tests passed: 17 tests;
- Java codegen unit tests passed: 19 tests.

The full cross-language matrix is still environment-dependent. Local tests should not require every target-language toolchain, while future full CI can make selected tools fail-fast.

### Known Constraints at the End of This Phase

- Runtime compliance is still not automated. `runtime/compliance/` has seed payloads and expected files, but there is no complete shared runner across Rust / TypeScript / C++ / C# / Java.
- C# and Java JSON readers are copyable templates and standard-harness dependencies, not yet packaged artifacts with versioned distribution.
- C# / Java MessagePack readers do not exist yet.
- Java JSON reader is intentionally JDK-only and small; it is suitable for repository harness and copyable-template usage, but future production users may still prefer Jackson, Gson, or Android-friendly adapters.
- C++ adapters still bind generated nominal interfaces through generated-header or macro boundaries; the stable external interface header remains future work.
- Unreal C++ repository tests are syntax smoke with a shim. They do not validate real Unreal ABI, UHT behavior, reflection metadata, module rules, or Editor runtime behavior.
- The test tool detection story is still scattered around existing harness helpers. The ToolProbe / profile-aware skip vs fail-fast design is documented but not implemented.

## Key Decisions and Rationale

### 1. Production Output Should Be Sink-first, Not Artifact-list-first

Decision:

- Route runtime table exporters, aggregate exporters, and code generators through `ArtifactSink` / write streams as the production path.
- Keep materialized generated files only where useful for tests and snapshot equivalence.
- Remove earlier artifact memory-limit primitives after the sink lifecycle made them less central.

Rationale:

- The phase-0003 bounded writer queue helped control memory during writing, but build still had to collect generated artifacts before writing.
- Sink-first output reduces peak memory and makes generated-code output align with runtime export streaming.
- A single write authority improves path-conflict handling, dry-run behavior, temp-commit behavior, and future back-pressure semantics.

Source material:

- `Documentation/Implementation/07-artifact-module-detail-design.md`
- `Documentation/Implementation/09h-codegen-generator-api-and-tests.md`
- `Documentation/Implementation/10-build-module-detail-design.md`
- Representative commits:
  - `96dc66b feat(artifact): add streaming sink coordinator`
  - `57b0ce4 feat(build): route artifacts through shared sink`
  - `c28bff3 refactor(exporter): make table export stream-first`
  - `1e3106c refactor(exporter): make aggregate exports sink-first`
  - `cf74785 refactor(codegen): add sink-first generation API`
  - `8333091 refactor(codegen): remove generated file materialization path`

### 2. Source-order Access Must Be Explicit

Decision:

- Add explicit source-order generated access only when schema requests it.
- Hide source-order internals by default for indexed CollectionTables.

Rationale:

- Index-oriented repositories should not accidentally promise stable public row-order APIs when the schema did not request that contract.
- Some gameplay use cases still need declaration/export order, so an explicit opt-in is preferable to removing the capability entirely.
- Making the opt-in visible in Meta avoids backend-by-backend drift.

Source material:

- `Documentation/Implementation/09-codegen-module-detail-design.md`
- Representative commits:
  - `e5e87d0 feat(codegen): support source-order table access`
  - `c8d72cc feat(codegen): hide source-order internals by default`
  - `186d74d test(codegen): cover indexed source-order APIs`

### 3. Unreal C++ Should Be a Separate Backend

Decision:

- Implement `cpp_unreal` as an independent backend rather than a switch in standard C++.
- Support Unreal containers and Blueprint-facing output through Unreal-specific naming, containers, macros, and facade conventions.
- Use a local shim for repository tests and keep real Unreal integration outside normal ConfigForge CI.

Rationale:

- Standard C++ and Unreal C++ have different public API expectations: `std::*` and engine-independent ownership are not interchangeable with `TArray`, `TMap`, `FString`, reflection macros, and Blueprint constraints.
- Keeping a separate backend avoids mixing incompatible API styles in one generator.
- Requiring a full Unreal Engine install for ordinary repository tests would make local and CI setup too heavy; syntax smoke catches many generator regressions while preserving a clear boundary.

Source material:

- `Documentation/Implementation/09j-codegen-cpp-unreal-backend.md`
- `Documentation/Tech/cpp-runtime-dependencies.md`
- Representative commits:
  - `b2feff0 docs(codegen): plan cpp_unreal implementation steps`
  - `cca89ce feat(codegen): register cpp_unreal generator skeleton`
  - `25a88da feat(codegen): load Unreal container rows and objects`
  - `79f7616 feat(codegen): add Unreal lazy and relation helpers`
  - `418ad44 feat(codegen): emit Blueprint Unreal config types`
  - `17bd8df feat(codegen): add Blueprint query facade`
  - `f153803 fix(codegen): harden Unreal Blueprint output`

### 4. C# and Java Should Follow Existing Format-agnostic Backend Contracts

Decision:

- Add C# and Java as independent format-agnostic codegen backends.
- Generate runtime loader/reader contracts in generated code, but do not bind generated core to JSON / MessagePack libraries.
- Match the established backend capability set: rows/objects, repositories, hash and ordered indexes, compound keys, relation helpers, text helpers, eager `GameData`, and lazy facades.

Rationale:

- The project had already proven the Meta-agnostic reader contract in Rust / TypeScript / C++.
- C# and Java are common game tooling/runtime integration targets, but they each need language-idiomatic API shapes.
- Keeping generated core dependency-free lets users choose BCL / JDK-only, Jackson, Gson, Unity, Android, or custom resource loaders later.

Source material:

- `Documentation/Implementation/09k-codegen-csharp-backend.md`
- `Documentation/Implementation/09l-codegen-java-backend.md`
- Representative commits:
  - `c217c12 docs(codegen): plan C# backend implementation`
  - `bdd6644 feat(codegen): register C# generator skeleton`
  - `de57cf1 feat(codegen): add C# lazy game data`
  - `cf37d3d docs(codegen): mark C# backend implemented`
  - `7a617e7 docs(codegen): plan Java backend implementation`
  - `2d07600 feat(codegen): register Java generator skeleton`
  - `6e63c05 feat(codegen): add Java lazy game data`
  - `1336535 docs(codegen): document Java backend status`

### 5. Runtime Reader Logic Should Live Under `runtime/`, Not Inside Harness Strings

Decision:

- Move C# JSON reader logic into `runtime/csharp/json_reader`.
- Move Java JSON reader / parser logic into `runtime/java/json_reader`.
- Keep standard project harnesses responsible only for generated-interface adapters and behavioral assertions.

Rationale:

- Harness-embedded readers are hard to reuse and can drift from production examples.
- `runtime/` is the intended home for standard reader templates and copyable adapter source.
- Generated C# / Java interfaces are nominal, so a thin adapter remains necessary, but the actual value reader and filesystem loader can be shared.

Source material:

- `Documentation/Implementation/09e-codegen-runtime-adapters.md`
- `Documentation/Tech/development-environment.md`
- Representative commit:
  - `9c2adee feat(runtime): add C# and Java JSON readers`

### 6. Cross-language Tests Need Optional Tooling, Not a Mandatory Monolithic Environment

Decision:

- Document that Rust core development requires Rust tooling only.
- Treat CMake / C++ compiler / Node / TypeScript / .NET SDK / JDK / Lua as optional local tools that enable corresponding harnesses.
- Plan a future ToolProbe and profile-aware local-vs-full-CI strategy.

Rationale:

- As target languages grow, requiring every developer to install every runtime toolchain becomes impractical.
- Local developer experience should allow Rust-only work to proceed while still reporting explicit skips for missing optional harnesses.
- Full CI and release validation can choose a stricter profile where required tools fail-fast.

Source material:

- `Documentation/Tech/development-environment.md`
- `Documentation/Todo/cross-language-toolchains-and-test-matrix-todo.md`
- Representative commits:
  - `c1cc9b8 docs(testing): plan cross-language toolchain test matrix`
  - `4e74b56 docs(testing): add development environment guide`

### 7. Centralize Backend and Status References

Decision:

- Add a canonical codegen backend capability matrix.
- Update design, implementation, and tech docs to link to the matrix instead of repeating backend lists.
- Replace stale per-document status matrices with links to `Design/README.md` or the backend matrix.

Rationale:

- Backend lists changed rapidly during this phase: C++, TypeScript, Unreal C++, C#, Java, Lua embedded data, and runtime adapters all had different dependency and testing status.
- Repeating the list in many documents caused drift and required noisy documentation edits.
- A single status authority makes future backend additions less error-prone.

Source material:

- `Documentation/Design/README.md`
- `Documentation/Implementation/09-codegen-module-detail-design.md`
- Representative commits:
  - `7b4b254 docs(codegen): add backend capability matrix`
  - `07ca0af docs(codegen): align backend status documentation`
  - `fd77b64 docs: centralize status and backend references`

## Alternatives Considered or Deferred

- **Keep artifact memory-limit primitives as the main control surface.** Deferred after sink-first output simplified the write path; standard writer lifecycle and streaming are now the main authority.
- **Implement Unreal as a mode of standard C++.** Rejected because API, reflection, and ownership conventions differ too much.
- **Require a real Unreal Engine install in ordinary CI.** Rejected for repository CI; real engine validation remains a separate integration concern.
- **Embed C# / Java JSON readers permanently in test harness strings.** Rejected in favor of `runtime/` templates to reduce drift and provide reusable examples.
- **Require all language toolchains for local `cargo test`.** Rejected. Local tests should explicitly skip optional harnesses when tools are absent; full CI can be stricter later.
- **Maintain language lists in every design document.** Rejected in favor of the backend capability matrix and status index.

## Follow-up Questions

1. When should the ToolProbe / profile-aware skip vs fail-fast mechanism replace the current scattered `command_exists` checks?
2. What is the minimum useful shared runtime compliance runner, and should it start with Rust / TypeScript / C++ before adding C# / Java?
3. Should C# and Java JSON readers stay as copyable templates, or should they grow package/project metadata once compliance runners exist?
4. What is the right boundary for a stable C++ external runtime interface header so C++ adapters no longer need generated-header binding?
5. Which Java production adapter should be prioritized after the JDK-only template: Jackson, Gson, Android-specific, or MessagePack?
6. Should full CI / nightly install every language toolchain, or should it be split into independent per-language jobs?

## Source Material

- `Documentation/Design/README.md`
- `Documentation/Design/06-export-cli.md`
- `Documentation/Implementation/07-artifact-module-detail-design.md`
- `Documentation/Implementation/09-codegen-module-detail-design.md`
- `Documentation/Implementation/09e-codegen-runtime-adapters.md`
- `Documentation/Implementation/09h-codegen-generator-api-and-tests.md`
- `Documentation/Implementation/09j-codegen-cpp-unreal-backend.md`
- `Documentation/Implementation/09k-codegen-csharp-backend.md`
- `Documentation/Implementation/09l-codegen-java-backend.md`
- `Documentation/Tech/development-environment.md`
- `Documentation/Todo/cross-language-toolchains-and-test-matrix-todo.md`
- `runtime/csharp/json_reader/`
- `runtime/java/json_reader/`
- `tests/fixtures/standard_project/`
- Representative commit range: `20db79f..fd77b64`.
