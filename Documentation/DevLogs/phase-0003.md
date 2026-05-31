# Phase 0003 Developer Log — Runtime Loading Semantics, Export Task Concurrency, and Build Traceability

## Timebox

- Approximate period: after `phase-0002` through 2026-05-31.
- Snapshot commit range observed locally:
  - `11c8939 refactor: narrow public API and remove dead code`
  - through `7ae4e29 fix(build): write traces for failed builds`

## Scope

Phase 0002 moved ConfigForge toward clearer module ownership and a unified artifact writer. Phase 0003 continued that work by tightening runtime loading contracts, expanding export formats and generated access APIs, and making the build pipeline more concurrent and observable without committing to a full general-purpose DAG executor.

The major work was:

- narrow public API and split large implementation areas into more focused modules;
- harden runtime reader contracts and generated loading behavior across Rust, TypeScript, and C++;
- add tag-gated override sources, scalar list fields, ObjectTable foreign keys, and linked-field relation helpers;
- add gettext POT and Lua table export paths, including global i18n integration in generated code and fixture harnesses;
- unify target exports around explicit exporter dependency contracts and target-local execution views;
- split source read planning, introduce an async provider shell, and overlap Meta link with source reading;
- parallelize normalization, validation, exporter artifact generation, and artifact writes where stable ordering can be preserved;
- add bounded artifact write memory controls;
- add stage-level `BuildTrace` / `--trace-tasks` metrics and fix trace correctness for nested exporter failures and failed builds;
- clean canonical docs so they describe current state and future plans, while this file preserves the historical phase context.

This log is historical context. The current canonical behavior is still defined by `Documentation/Design/`, `Documentation/Implementation/`, and `Documentation/Tech/`.

## Status Snapshot

### Implemented Capabilities Added or Stabilized in This Phase

- Runtime loading and codegen behavior:
  - runtime readers now use tighter format contracts, cursor-based loading, configurable decode limits, and clearer optional-presence / length semantics;
  - generated runtime loading is more schema-driven and format-agnostic;
  - table lazy loading was added for generated access APIs;
  - TypeScript codegen gained synchronous loading APIs in addition to async loading;
  - generated APIs now include linked-field relation helpers for Rust, C++, and TypeScript.
- Data and Meta model semantics:
  - tag-gated override sources are implemented and aligned with project/source contracts;
  - ObjectTable foreign keys are supported;
  - scalar list fields are supported through a unified typed-value parsing path;
  - data resolver internals now carry lighter array elements and explicit value context;
  - foreign-key validation caches target keys to reduce repeated index construction.
- Additional export formats and i18n support:
  - gettext POT export and generated text translation helpers are implemented;
  - generated code supports global i18n function mode;
  - standard fixtures and harnesses load gettext catalogs for global i18n validation;
  - Lua table export was added as another runtime/exporter path.
- Target export and build orchestration:
  - target exports were unified around `[[targets.exports]]` plus project-level input;
  - exporters now declare dependency contracts such as `MetaOnly` and `MetaAndData`;
  - build constructs a target-local execution view from those contracts;
  - a target dependency plan and baseline tests preserve the migration path for future DAG-like scheduling;
  - current canonical design intentionally keeps the executor stage-oriented and does not require a generic node scheduler yet.
- Source read and provider concurrency:
  - source read planning was split from data loading execution;
  - source read concurrency tests were hardened for deterministic order and failure behavior;
  - an async provider shell was introduced for future remote/spreadsheet providers;
  - build now overlaps Meta linking and source reading where a data-backed target requires both.
- Validation and artifact generation concurrency:
  - table normalization can run in parallel and reports the first error by plan order;
  - validation can collect reports before failing so multiple issues are preserved;
  - target-view validation can run visible table work in parallel and merge diagnostics stably;
  - artifact planning/generation for exporter tasks can run concurrently while preserving declaration-order output.
- Artifact write resource controls:
  - artifact writing now supports memory-budgeted in-flight limits via `ArtifactWriteLimits`;
  - CLI exposes `--artifact-memory-limit <SIZE>` and `--artifact-memory-percent <PERCENT>`;
  - bounded writer tests cover tiny queues, oversized single artifacts, and Direct / TempCommit / DryRun semantics.
- Build tracing / metrics:
  - `BuildOutput` includes `BuildTrace` with resource settings, total duration, and per-stage timings;
  - `--trace-tasks <FILE>` writes pretty JSON without changing stable stdout;
  - trace stage statuses distinguish `succeeded` and `failed`;
  - nested branch failures inside `run_exporters` now mark the stage as failed;
  - failed stage-oriented builds now still write trace output through `BuildErrorWithTrace`.
- Documentation maintenance:
  - canonical `Design/`, `Implementation/`, and `Tech/` docs were reorganized and aligned with the current state;
  - filesystem path conflict limits and platform semantics were documented;
  - Phase 12 in `task-system-design.md` was re-scoped from “introduce a general DAG executor” to “stage-level tracing / metrics now, executor only if future complexity justifies it”.

### Verification Snapshot

The latest verification before this log was run from `/Users/bytedance/Documents/ConfigForge`.

```text
cargo fmt
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo coverage
```

Observed result:

- `cargo fmt` passed.
- Full test suite passed: 300 unit tests, 30 CLI integration tests, and doctests.
- Clippy passed with warnings denied.
- Coverage completed successfully; the reported total was 90.85% line coverage, with the coverage tool emitting one `mismatched data` warning during report generation.

### Known Constraints at the End of This Phase

- Build tracing is stage-level only. It does not expose node-level critical path, resource wait time, cancellation propagation, or per-exporter spans.
- The target-local execution view and `BuildDagNodeKey` provide a migration skeleton, but there is still no general `TaskSpec` / `TaskGraph` / readiness queue executor in production.
- The async provider shell exists, but local CSV remains the implemented source provider; remote and spreadsheet providers remain future work.
- Artifact writing has bounded writer-queue memory, but build still collects generated `Artifact` values before final write. A streaming generator would be needed to reduce generation-stage peak memory further.
- Validation now collects more reports before failing, but cancellation is still cooperative only where explicit future provider/runtime hooks are added.
- `TempCommit` remains best-effort commit rather than a full multi-file rollback transaction, as documented in Phase 0002.

## Key Decisions and Rationale

### 1. Runtime Loading Should Be Schema-driven and Format-agnostic

Decision:

- Keep generated loading APIs bound to runtime reader contracts rather than to ad hoc format-specific parsing in each language generator.
- Tighten reader behavior around cursor use, optional presence, length/decode limits, and C++ harness expectations.
- Add table lazy loading and TypeScript sync loading without changing exporter file contracts.

Rationale:

- Generated code should remain convenient for runtime projects while preserving one source of truth for format semantics.
- Cursor-based and decode-limit-aware readers reduce accidental over-read and denial-of-service risks.
- Lazy loading lets generated access APIs avoid eagerly decoding every table when callers only need a subset.

Source material:

- `Documentation/Implementation/09-codegen-module-detail-design.md`
- `Documentation/Implementation/09a-codegen-runtime-reader-contract.md`
- `runtime/`
- Representative commits:
  - `7c17636 perf(runtime): use cursor readers for runtime loading`
  - `1c0ddb6 feat(runtime): add configurable decode limits`
  - `bcbe090 refactor(codegen): make runtime loading format agnostic`
  - `7211f02 feat(codegen): add table lazy loading`
  - `9384d30 feat(codegen): add TypeScript sync loading APIs`

### 2. Target-visible Data Semantics Need First-class Override, List, and Relation Support

Decision:

- Implement tag-gated override sources as part of source planning.
- Support scalar list fields through shared typed-value parsing.
- Support ObjectTable foreign keys and generated linked-field relation helpers.

Rationale:

- Target-specific builds need deterministic source override behavior without requiring separate project files for every tag combination.
- Lists and linked relations are common game configuration shapes; handling them centrally prevents each exporter or generator from inventing separate interpretations.
- ObjectTable foreign keys let singleton configuration objects participate in the same referential integrity model as collection rows where semantics are well-defined.

Source material:

- `Documentation/Design/04-meta-model.md`
- `Documentation/Design/05-data-validation.md`
- `Documentation/Implementation/05-data-module-detail-design.md`
- `Documentation/Implementation/09g-codegen-index-and-relation-helpers.md`
- `Documentation/Implementation/11-foreign-key-detail-design.md`
- Representative commits:
  - `ce97c21 docs(data): design tag-gated override sources`
  - `3974d12 feat(data): support tag-gated override sources`
  - `6207d6c feat(meta): support ObjectTable foreign keys`
  - `2bf6032 feat(meta): support scalar list fields`
  - `1ca323d feat(codegen): add Rust linked field relations`
  - `193f775 feat(codegen): add C++ linked field relations`
  - `54fb450 feat(codegen): add TypeScript linked field relations`

### 3. Exporter Dependency Contracts Should Drive Build Branching

Decision:

- Exporters declare whether they need only linked Meta or both Meta and Data.
- Build normalizes target exports into a target-local execution view and can run MetaOnly and MetaAndData work independently where safe.
- Keep `BuildDagNodeKey` and dependency-plan tests as migration scaffolding, but avoid introducing a general DAG scheduler until there is a demonstrated need.

Rationale:

- The old “runtime data first, codegen second” mental model no longer matched mixed targets such as runtime data + codegen + Lua + POT.
- Dependency contracts give build enough information to avoid unnecessary source reads for code-only targets and to parallelize independent branches.
- A full DAG executor would add scheduler complexity before there are enough node types, cancellation/resource-wait requirements, or multi-target reuse cases to justify it.

Source material:

- `Documentation/Implementation/10-build-module-detail-design.md`
- `Documentation/Tech/task-system-design.md`
- Representative commits:
  - `817f60c refactor(build): unify target exports and project input`
  - `e5ec050 feat(registry): add exporter dependency contract`
  - `666946e feat(build): add target export DAG executor`
  - `64d2dd1 refactor(build): clarify export dependency contracts`
  - `6e91753 test(build): add DAG migration baseline guards`
  - `74182ef feat(build): add target dependency plan`

### 4. Prefer Deterministic Parallelism Over Maximum Throughput

Decision:

- Parallelize source reads, normalization, validation, exporter planning/generation, and artifact writes only where result order and diagnostics can be stabilized.
- Join or merge parallel results by plan order, declaration order, or stable diagnostic keys.
- Collect validation reports before failing when possible.

Rationale:

- Config build output must be reproducible across local, CI, and platform environments.
- Users should not see nondeterministic first errors just because tasks finish in a different order.
- Collecting more validation reports improves feedback quality without allowing invalid exports to proceed.

Source material:

- `Documentation/Implementation/05-data-module-detail-design.md`
- `Documentation/Implementation/06-validation-module-detail-design.md`
- `Documentation/Implementation/10-build-module-detail-design.md`
- Representative commits:
  - `4846bcd refactor(data): split source read planning`
  - `4df7d01 test(data): harden source read concurrency`
  - `2b3c4bb feat(data): add async provider shell`
  - `cb04dfd feat(build): overlap meta link and source read`
  - `41cd077 feat(validation): parallelize table normalization`
  - `fa8cc5a feat(validation): collect validation reports before failing`
  - `7c097a5 feat(validation): parallelize target view validation`
  - `3a8faba feat(build): parallelize artifact generation`

### 5. Add Artifact Write Memory Bounds Before Streaming Generation

Decision:

- Introduce `ArtifactWriteLimits` and apply it to the artifact writer queue.
- Expose CLI controls for fixed memory limit and available-memory percentage.
- Keep the existing `Vec<Artifact>` generation boundary for now.

Rationale:

- Writer-queue bounds provide an immediate guardrail for large artifact sets without redesigning every exporter as a streaming producer.
- A future streaming generator can reduce generation peak memory, but the bounded writer is still useful as the final back-pressure layer.
- CLI memory controls make the behavior explicit and testable in CI.

Source material:

- `Documentation/Implementation/07-artifact-module-detail-design.md`
- `Documentation/Design/06-export-cli.md`
- `Documentation/Tech/task-system-design.md`
- Representative commit:
  - `56b334e feat(build): bound artifact writes by memory budget`

### 6. Build Traceability Is Valuable Without a Full DAG Executor

Decision:

- Implement stage-level `BuildTrace` and CLI `--trace-tasks <FILE>` first.
- Record stable stage names, relative start, duration, success/failure status, and execution resource settings.
- Return trace on failed stage-oriented builds through `BuildErrorWithTrace` so the CLI can write trace JSON before propagating the original error.

Rationale:

- The current build structure has a small number of meaningful stages; stage-level metrics answer the immediate performance/debugging questions.
- A general executor would only be justified if future work introduces enough node reuse, resource waits, remote cancellation, or multi-target scheduling complexity.
- Failure-path traces are especially useful when diagnosing slow or broken builds; losing trace data on error made the CLI flag much less useful.

Source material:

- `Documentation/Implementation/10-build-module-detail-design.md`
- `Documentation/Tech/task-system-design.md`
- Representative commits:
  - `1d8f01f feat(build): add stage trace metrics`
  - `a6b819b fix(build): mark exporter trace failures`
  - `7ae4e29 fix(build): write traces for failed builds`

### 7. i18n and Lua Export Should Use the Same Export Task Model

Decision:

- Add gettext POT export and Lua table export as regular exporter tasks.
- Add generated global i18n function hooks and fixture harness coverage rather than treating i18n as an external post-process.

Rationale:

- Text extraction and Lua embedded-data output are runtime-facing artifacts and should participate in target filtering, artifact path checks, and build ordering like other exporters.
- Keeping them inside the exporter registry validates the dependency-contract model against more than JSON/MessagePack/codegen.

Source material:

- `Documentation/Implementation/08-exporter-module-detail-design.md`
- `Documentation/Implementation/09i-lua-embedded-data-generator.md`
- `Documentation/Design/06-export-cli.md`
- Representative commits:
  - `834f0e2 docs(i18n): design gettext POT data export`
  - `04974d0 feat(i18n): add gettext POT export and translation helpers`
  - `9df1d93 feat(codegen): add i18n global function mode`
  - `6bdc38c test(fixtures): use global i18n in standard project`
  - `dee66ef test(fixtures): load gettext catalogs in global i18n harnesses`
  - `fb11b4d docs(lua): design embedded data generator`
  - `5b90bf7 feat(exporter): add Lua table export`

### 8. Canonical Docs Should Record Current State, Not Historical Phase Progress

Decision:

- Keep `Design/`, `Implementation/`, and `Tech/` documents focused on current state and future planning.
- Move phase history, removed approaches, and rationale snapshots into DevLogs.
- Update canonical build/task docs to describe stage-level tracing as the completed Phase 12 scope rather than preserving obsolete executor migration status text.

Rationale:

- Historical phase text can mislead future contributors if it sits next to canonical design language.
- DevLogs preserve why a decision was made without forcing every old plan to remain current.

Source material:

- `AGENTS.md`
- `Documentation/README.md`
- `Documentation/Design/README.md`
- `Documentation/Tech/task-system-design.md`
- Representative commits:
  - `4d03b83 docs: align documentation with current implementation`
  - `7418188 docs: reorganize and align documentation`
  - `396168f docs(tech): document filesystem path conflict limits`
  - `dd0bce8 docs(build): finish target exports cleanup`

## Alternatives Considered or Deferred

### Full General DAG Executor in This Phase

- Considered: replace hand-written target-stage orchestration with a generic DAG executor and scheduler.
- Deferred: current task graph is small enough that a full executor would add complexity without clear immediate benefits. The project kept target-local dependency keys and tests as scaffolding while delivering stage-level traceability first.

### Node-level Tracing and Critical Path Analysis

- Considered: report per-node spans, resource wait time, critical path, and cancellation propagation.
- Deferred: current `BuildTrace` intentionally records only stable stage boundaries. Node-level spans can be added later if a real executor or remote provider cancellation path creates demand.

### Fully Streaming Artifact Generation

- Considered: stream exporter/codegen output directly into a bounded writer pipeline.
- Deferred: bounded write queue limits are in place, but exporters still materialize artifacts. Streaming generation should be paired with exporter API changes and peak-memory instrumentation.

### Remote/Spreadsheet Providers in the Async Shell

- Considered: immediately implement remote or spreadsheet providers on top of the new async provider shell.
- Deferred: local CSV remains the only concrete provider. The shell and concurrency tests prepare the execution model without committing to provider-specific retry, backoff, credential, or cancellation policy.

### Trace Serialization Before CLI Validation

- Considered: make `--trace-tasks` available for failures before build starts, such as invalid CLI resource flags.
- Deferred: current trace is build-stage trace. CLI argument/configuration validation errors that occur before `build_target_with_trace()` do not have a meaningful build trace yet.

## Source Material

Primary canonical documents updated or referenced during this phase:

- `Documentation/Design/03-configuration.md`
- `Documentation/Design/04-meta-model.md`
- `Documentation/Design/05-data-validation.md`
- `Documentation/Design/06-export-cli.md`
- `Documentation/Implementation/05-data-module-detail-design.md`
- `Documentation/Implementation/06-validation-module-detail-design.md`
- `Documentation/Implementation/07-artifact-module-detail-design.md`
- `Documentation/Implementation/08-exporter-module-detail-design.md`
- `Documentation/Implementation/09-codegen-module-detail-design.md`
- `Documentation/Implementation/09a-codegen-runtime-reader-contract.md`
- `Documentation/Implementation/09g-codegen-index-and-relation-helpers.md`
- `Documentation/Implementation/09i-lua-embedded-data-generator.md`
- `Documentation/Implementation/10-build-module-detail-design.md`
- `Documentation/Implementation/11-foreign-key-detail-design.md`
- `Documentation/Tech/platform-filesystem-semantics.md`
- `Documentation/Tech/task-system-design.md`
- `Documentation/Tech/unit-test-coverage.md`
- `AGENTS.md`

Representative commits:

```text
11c8939 refactor: narrow public API and remove dead code
7c17636 perf(runtime): use cursor readers for runtime loading
f516259 refactor: split meta and validation modules
e255dab refactor: split codegen modules
4d03b83 docs: align documentation with current implementation
3974d12 feat(data): support tag-gated override sources
6207d6c feat(meta): support ObjectTable foreign keys
2bf6032 feat(meta): support scalar list fields
1c0ddb6 feat(runtime): add configurable decode limits
1ca323d feat(codegen): add Rust linked field relations
193f775 feat(codegen): add C++ linked field relations
54fb450 feat(codegen): add TypeScript linked field relations
7211f02 feat(codegen): add table lazy loading
9384d30 feat(codegen): add TypeScript sync loading APIs
04974d0 feat(i18n): add gettext POT export and translation helpers
9df1d93 feat(codegen): add i18n global function mode
5b90bf7 feat(exporter): add Lua table export
817f60c refactor(build): unify target exports and project input
e5ec050 feat(registry): add exporter dependency contract
666946e feat(build): add target export DAG executor
74182ef feat(build): add target dependency plan
4846bcd refactor(data): split source read planning
2b3c4bb feat(data): add async provider shell
cb04dfd feat(build): overlap meta link and source read
41cd077 feat(validation): parallelize table normalization
fa8cc5a feat(validation): collect validation reports before failing
7c097a5 feat(validation): parallelize target view validation
3a8faba feat(build): parallelize artifact generation
56b334e feat(build): bound artifact writes by memory budget
1d8f01f feat(build): add stage trace metrics
a6b819b fix(build): mark exporter trace failures
7ae4e29 fix(build): write traces for failed builds
```

## Follow-up Questions

- When multi-target build arrives, should dependency planning share normalized `DataSet` values across targets with compatible `(input, effective_tags)` keys, or should it keep target-scoped data views isolated until correctness is proven?
- What concrete remote/spreadsheet provider should be implemented first on top of the async provider shell, and what retry/backoff/cancellation semantics should it require?
- When artifact generation becomes streaming, should exporter APIs yield `Artifact` streams, typed chunks, or writer callbacks?
- Should trace JSON eventually support a stable schema version and machine-readable error summary for CI dashboards?
- Is there enough user value to expose structured dry-run/trace output together as one build report, or should they remain separate files/commands?
- If node-level tracing is added later, should it be tied to a real DAG executor or layered as spans around the current stage executor?
