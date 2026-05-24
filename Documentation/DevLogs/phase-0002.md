# Phase 0002 Developer Log — Runtime Readers, Module Boundaries, and Artifact Writer

## Timebox

- Approximate period: after `phase-0001` through 2026-05-24.
- Snapshot commit range observed locally:
  - `d17d7c0 fix(codegen): address runtime reader review feedback`
  - through `b866df4 fix: expose artifact dry-run plan`

## Scope

Phase 0001 established the first practical ConfigForge foundation: TOML project / Meta loading, linked Meta, CSV data loading, target-aware validation, JSON / MessagePack runtime export, Rust / TypeScript / C++ code generation, fragments, foreign keys, and basic execution controls.

Phase 0002 moved the project from “feature-complete MVP pieces” toward clearer engineering boundaries and a more stable export/codegen toolchain. The major work was:

- turn generated code from plain type stubs into runtime-reader-aware access code;
- add standard runtime helper packages and cross-language compliance fixtures;
- migrate the Rust baseline to Rust 2024 and handle new identifier constraints;
- refactor exporter, build, diagnostic, and module ownership boundaries;
- introduce a dedicated `src/artifact/` bottom-layer module so runtime data and generated code share one write pipeline;
- expose CLI dry-run and temp-commit write modes through build options and artifact write results;
- clean up canonical documentation so current docs describe current/planned state, while historical reasoning lives in DevLogs.

This log is historical context. The current canonical behavior is still defined by `Documentation/Design/`, `Documentation/Implementation/`, and `Documentation/Tech/`.

## Status Snapshot

### Implemented Capabilities Added or Stabilized in This Phase

- Runtime readers and generated loading APIs:
  - generated Rust, TypeScript, and C++ code now includes runtime reader / loader contracts;
  - generated readers parse exported runtime payloads rather than leaving parsing entirely to caller code;
  - generated code is aware of runtime data file paths from selected data tasks;
  - ObjectTable codegen is represented as typed singleton objects rather than collection repositories.
- Standard runtime helper packages:
  - `runtime/` now contains standard reader adapters/templates and compliance fixtures;
  - generated runtime code can be validated against exported fixture data across Rust, TypeScript, and C++ paths.
- Runtime format support:
  - JSON and MessagePack exports remain registry-backed runtime data formats;
  - codegen consumes `RuntimeDataFile` contracts rather than guessing output file names independently.
- Rust toolchain baseline:
  - project baseline moved to Rust 2024;
  - codegen sanitization covers the Rust 2024 `gen` keyword across generated API surfaces.
- Module boundary cleanup:
  - exporter split runtime data export flow from project config and data format concerns;
  - build orchestration was split into target/runtime/options-oriented modules;
  - shared diagnostic foundations moved under `src/diagnostic/` with crate errors bridged there;
  - Meta loading ownership was clarified so project config owns imports and aggregation, while `meta` owns schema/link validation.
- Artifact writing:
  - `src/artifact/` introduces `Artifact`, `ArtifactContent`, `ArtifactWriteMode`, `ArtifactWriter`, and `ArtifactWriteResult`;
  - runtime data exporter returns artifacts instead of directly writing files in the CLI build path;
  - codegen outputs are converted to text artifacts by build before writing;
  - artifact writer performs path normalization, stable sorting, duplicate final path rejection, dry-run planning, direct writes, and temp-commit writes;
  - `BuildOutput` exposes `ArtifactWriteResult`, and CLI dry-run prints the artifact plan.
- CLI behavior:
  - `--dry-run` executes full config/meta/data/validation/export/codegen planning but does not create output files;
  - `--write-mode <direct|temp-commit>` selects artifact write behavior for non-dry-run builds;
  - `--dry-run` takes precedence over `--write-mode`.
- Documentation organization:
  - Implementation docs were reorganized into module-level detailed design documents;
  - Artifact module design was promoted into `Implementation/07-artifact-module-detail-design.md`;
  - `AGENTS.md` and documentation maintenance rules now explicitly keep canonical docs focused on current/planned state and move historical reasoning into DevLogs.

### Verification Snapshot

The latest verification before this log used the local Cargo binary at `~/.cargo/bin/cargo`.

```text
~/.cargo/bin/cargo fmt --manifest-path /Users/bytedance/Documents/ConfigForge/Cargo.toml --all
git -C /Users/bytedance/Documents/ConfigForge diff --check
~/.cargo/bin/cargo check --manifest-path /Users/bytedance/Documents/ConfigForge/Cargo.toml
~/.cargo/bin/cargo test --manifest-path /Users/bytedance/Documents/ConfigForge/Cargo.toml artifact
~/.cargo/bin/cargo test --manifest-path /Users/bytedance/Documents/ConfigForge/Cargo.toml --test cli_integration dry_run_builds_plan_without_writing_artifacts
~/.cargo/bin/cargo test --manifest-path /Users/bytedance/Documents/ConfigForge/Cargo.toml
~/.cargo/bin/cargo clippy --manifest-path /Users/bytedance/Documents/ConfigForge/Cargo.toml --all-targets
```

Observed result:

- `cargo fmt` passed.
- `git diff --check` passed.
- `cargo check` passed.
- Artifact-focused tests passed.
- Dry-run CLI integration test passed.
- Full test suite passed: 182 unit tests, 20 CLI integration tests, and doctests.
- Clippy completed with one known pre-existing warning in `src/diagnostic/mod.rs:238` for `clippy::too_many_arguments`.

### Known Constraints at the End of This Phase

- `temp-commit` is not a global transaction. It writes same-directory temporary files first, then renames them in stable order. Temporary-file write failure is cleaned up before final submission, but a rename/commit-stage failure may leave already-renamed final files in place.
- Artifact writer currently lives in one `src/artifact/mod.rs` file. It is still manageable, but future rollback/manifest/remote-writer work may justify splitting into `model.rs`, `writer.rs`, `path.rs`, and tests.
- Build/exporter/runtime layers still perform some early conflict checks before the final artifact writer check. This is acceptable for early diagnostics, but final normalized path conflict authority is the artifact writer.
- CSV remains the implemented source provider. Remote or spreadsheet providers are still future work.
- Struct arrays/maps, asset existence validation, POT/text export, and general override-source merge behavior remain planned or partial depending on the topic-specific canonical docs.

## Key Decisions and Rationale

### 1. Runtime Reader Contract Belongs in Generated APIs

Decision:

- Generated Rust, TypeScript, and C++ code should include runtime data reader / loader contracts and parse exported data through those contracts.
- Runtime data file paths are passed into codegen via `RuntimeDataFile` generated by runtime export planning.

Rationale:

- Users need generated code that can actually load the exported artifacts, not only type definitions.
- Keeping runtime file contracts in exporter/build and passing them into codegen avoids language generators independently reconstructing file names or formats.
- The generated API becomes testable against fixture exports and standard runtime helpers.

Source material:

- `Documentation/Implementation/09-codegen-module-detail-design.md`
- `runtime/`
- Commits:
  - `e6fca57 feat(codegen): move runtime parsing into generated readers`
  - `5199d6b feat(codegen): add standard runtime helpers`
  - `409803b feat(runtime): engineer reader packages`

### 2. ObjectTable Should Be a Typed Singleton Shape

Decision:

- ObjectTable generated code should expose typed object access rather than collection-style repositories or index helpers.

Rationale:

- ObjectTable source shape is key/value-like and semantically represents one logical object.
- Treating it as a row repository would leak CollectionTable assumptions into generated APIs and confuse users.

Source material:

- `Documentation/Design/04-meta-model.md`
- `Documentation/Implementation/09-codegen-module-detail-design.md`
- CLI and codegen integration tests around ObjectTable runtime loading.

### 3. Rust 2024 Baseline Requires Codegen Sanitization Discipline

Decision:

- Move the crate baseline to Rust 2024 and treat edition-reserved keywords such as `gen` as codegen naming hazards.

Rationale:

- The project should track a modern Rust baseline early while generated code surfaces are still evolving.
- Codegen must never emit target-language invalid identifiers, especially for user-authored Meta names.

Source material:

- `Cargo.toml`
- Commit `b0b9526 chore(rust): migrate toolchain baseline to Rust 2024`
- Rust edition guide: https://doc.rust-lang.org/edition-guide/rust-2024/

### 4. Exporter Should Produce Runtime Artifacts, Not Own All Writing

Decision:

- Add `export_runtime_data_artifacts()` and use it in the build path.
- Keep `export_runtime_data()` as a compatibility wrapper that writes through `ArtifactWriter::Direct`.

Rationale:

- Runtime export has two responsibilities that should be separated: serialize logical runtime data and write filesystem artifacts.
- Build needs to combine runtime data artifacts with generated code artifacts before final path conflict checking and write mode selection.

Source material:

- `Documentation/Implementation/08-exporter-module-detail-design.md`
- `Documentation/Implementation/10-build-module-detail-design.md`
- Commits:
  - `1f42861 refactor(exporter): split runtime data export flow`
  - `7f8dcff refactor(exporter): separate project config and data formats`
  - `449679d feat: route artifact writes through unified writer`

### 5. Artifact Writer Is a Bottom-Layer Service and Should Not Know Business Kinds

Decision:

- Introduce `src/artifact/` as a bottom-layer service module.
- Artifact writer only needs final path and content bytes/text. It does not carry `ArtifactKind` such as runtime data or generated code.

Rationale:

- File writing rules depend on path, content, write mode, and conflict detection, not on business source semantics.
- Keeping artifact free of exporter/codegen/meta/data/build dependencies preserves one-way module direction and makes it reusable for future special exporters.
- Removing `ArtifactKind` avoids making the writer responsible for domain classification that belongs to build/exporter/codegen layers.

Source material:

- `Documentation/Implementation/01-module-outline.md`
- `Documentation/Implementation/07-artifact-module-detail-design.md`
- Commits:
  - `e9f9941 docs: add artifact module design`
  - `449679d feat: route artifact writes through unified writer`

### 6. Dry-run Must Expose the Artifact Plan, Not Merely Avoid Writes

Decision:

- `ArtifactWriter::write()` returns `ArtifactWriteResult` in every write mode.
- `BuildOutput` exposes that result.
- CLI dry-run prints the artifact plan entries and keeps `written: false` for planned outputs.

Rationale:

- The value of dry-run is not only safety; it lets users inspect exactly which files would be written, in stable path order, with byte counts.
- Returning the write result through build also gives future programmatic callers the same plan visibility as the CLI.

Source material:

- `Documentation/Implementation/07-artifact-module-detail-design.md`
- Commit `b866df4 fix: expose artifact dry-run plan`

### 7. Temp-commit Is Currently Best-effort Commit, Not Global Rollback

Decision:

- Expose `--write-mode temp-commit` through CLI.
- Document current safety boundary explicitly: temporary-file phase is protected, but commit/rename phase is not a global transaction.

Rationale:

- Same-directory temp files reduce partial-write risk before final paths are touched.
- Implementing true all-or-nothing across multiple existing final files requires backup/rollback semantics that need separate design, especially across platforms and failure modes.
- Making the current boundary explicit is safer than implying atomic multi-file commits.

Source material:

- `Documentation/Design/06-export-cli.md`
- `Documentation/Tech/task-system-design.md`
- Commits:
  - `449679d feat: route artifact writes through unified writer`
  - `b866df4 fix: expose artifact dry-run plan`

### 8. Canonical Docs Should Avoid Historical Churn

Decision:

- Keep `Design/`, `Implementation/`, and `Tech/` focused on current state and planned future state.
- Move removed approaches, historical reasoning, temporary tradeoffs, and review context into DevLogs.

Rationale:

- Canonical docs had started mixing current architecture with removed transitional designs.
- Future agents and contributors need a clear source of truth for implementation, while still preserving why earlier decisions were made.

Source material:

- `AGENTS.md`
- `Documentation/README.md`
- `Documentation/DevLogs/Readme.md`

## Alternatives Considered or Deferred

### ArtifactKind in the Write Layer

- Considered: keep `ArtifactKind` in `Artifact` for diagnostics and classification.
- Deferred/rejected for this phase: writer does not need business semantics to write files. Classification can be added in a higher-level manifest or build report if needed.

### Full Transactional Temp Commit

- Considered: make `TempCommit` all-or-nothing across all artifacts.
- Deferred: true rollback requires preserving old final files, handling partial rename failures, and defining platform-specific behavior. Current implementation documents best-effort commit semantics instead.

### Splitting `src/artifact/mod.rs` Immediately

- Considered: split artifact into `model.rs`, `writer.rs`, `path.rs`, and test modules right away.
- Deferred: current file size is still manageable. Splitting is better paired with future rollback/manifest/remote-writer work.

### Removing All Early Path Conflict Checks Outside Artifact Writer

- Considered: make artifact writer the only conflict checker.
- Deferred: exporter/build early checks still provide more local error messages before all content is generated. Artifact writer remains final authority for normalized final path conflicts.

## Source Material

Primary canonical documents updated or referenced during this phase:

- `Documentation/Design/06-export-cli.md`
- `Documentation/Implementation/01-module-outline.md`
- `Documentation/Implementation/07-artifact-module-detail-design.md`
- `Documentation/Implementation/08-exporter-module-detail-design.md`
- `Documentation/Implementation/09-codegen-module-detail-design.md`
- `Documentation/Implementation/10-build-module-detail-design.md`
- `Documentation/Tech/task-system-design.md`
- `Documentation/glossary.md`
- `AGENTS.md`

Representative commits:

```text
d17d7c0 fix(codegen): address runtime reader review feedback
44eb592 docs(codegen): plan standard runtime helpers
4fe25bf fix(meta): address review feedback
5199d6b feat(codegen): add standard runtime helpers
b0b9526 chore(rust): migrate toolchain baseline to Rust 2024
7291648 docs(codegen): refine runtime reader engineering design
409803b feat(runtime): engineer reader packages
a7fb769 fix(docs): align runtime reader review follow-ups
1f42861 refactor(exporter): split runtime data export flow
7f8dcff refactor(exporter): separate project config and data formats
0b987f0 refactor(build): organize orchestration modules
56cd26f refactor(diagnostic): share diagnostic foundations
ec90699 docs(implementation): reorganize module design docs
8feae64 refactor(diagnostic): move crate errors into diagnostic module
b27e120 refactor: clarify meta loading ownership
a481f84 refactor: trim module re-exports
e9f9941 docs: add artifact module design
449679d feat: route artifact writes through unified writer
b866df4 fix: expose artifact dry-run plan
```

## Follow-up Questions

- Should `TempCommit` eventually become a true multi-file transaction with backup/rollback, or remain documented as best-effort commit?
- Should artifact path normalization be exposed as a shared utility to reduce duplication with project/build path helpers?
- When artifact manifests and summary reports are added, should they live as regular artifacts produced by build/special exporters, or as a separate reporting channel?
- Should CLI gain a structured dry-run output mode such as JSON for IDE/CI consumption?
- When remote or spreadsheet providers are introduced, does artifact writing need async I/O and bounded content generation, or is the current synchronous writer sufficient for the next phase?
