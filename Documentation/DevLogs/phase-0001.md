# Phase 0001 Developer Log — Foundation, Targets, Fragments, and Foreign Keys

## Timebox

- Approximate period: initial repository work through 2026-05-21.
- Snapshot commit range observed locally:
  - `52d83c0 feat(validation): validate defaults, enums, and field constraints`
  - through `be4e210 docs(style): define comment rules and clarify complex paths`

## Scope

This phase established the first working shape of ConfigForge as a Rust CLI for
designer-authored game data. The implementation moved from schema validation and
CSV loading toward target-aware validation, runtime data export, generated code,
parallel execution controls, variant-source fragments, and first-class foreign
key metadata.

The canonical product design lives under `Documentation/Design/`, while module
and detailed implementation plans live under `Documentation/Implementation/`.
This log intentionally records raw historical context and decision evidence for
future migrations.

## Current Status Snapshot

### Implemented Capabilities

- Rust 2021 library and CLI crate named `config_forge`.
- Meta loading from TOML, including multiple Meta files merged into one model.
- Linked Meta model for resolving table fields, custom enum/struct types,
  indexes, primary indexes, validators, fragments, and foreign keys.
- CSV data loading and data load plans, including multi-source table loading and
  polymorphic variant-source fragment columns.
- Data normalization and validation for:
  - required fields and defaults;
  - primitive and enum values;
  - struct JSON cell values;
  - fragment-scoped fields;
  - registered validators such as range validation;
  - target-specific table, field, row, validator, fragment, and FK visibility;
  - FK referential integrity.
- Runtime data export to JSON and MessagePack through a registry of exporters.
- Code generation IR and generators for Rust, TypeScript, and C++.
- Generated loader/repository helpers, runtime data file awareness, target-aware
  generation, and code-only target fallback paths.
- CLI integration tests and a canonical standard project fixture.
- Execution resource controls using `jobs`, `io_concurrency`,
  `network_concurrency`, and `serial` options.
- Project documentation structure for `Design/`, `Implementation/`, `Tech/`,
  code style, glossary, and now `DevLogs/`.

### Verification Status

The latest verification before this log was written used the local Cargo binary
at `~/.cargo/bin/cargo` because `cargo` was not on the shell `PATH`.

```text
~/.cargo/bin/cargo fmt
~/.cargo/bin/cargo test
~/.cargo/bin/cargo lint
```

Observed result:

- 126 unit tests passed.
- 11 CLI integration tests passed.
- `cargo lint` passed.

### Known Constraints and Phase Boundaries

- CSV is the implemented local source path. Remote Sheet providers remain a
  future extension point.
- Variant-source fragments are implemented; fragment join mode is explicitly not
  part of the current phase.
- Struct JSON cell validation and export are supported; whole-struct default
  values remain out of phase-one scope.
- Foreign keys are available in Meta/link/validation/codegen IR, but language
  generators may still need later user-facing FK helper APIs.
- The task system currently uses practical local parallelism controls rather
  than a full async DAG runtime.
- Global compatibility/version migration is not introduced. Stable string names
  remain the primary identity mechanism for current configs.

## Key Decisions and Rationale

### 1. Rust CLI with a Thin Binary and Library-First Modules

Decision:

- Keep `src/main.rs` thin and implement domain behavior in library modules such
  as `meta`, `data`, `validation`, `config_forge`, `codegen`, and `exporter`.

Rationale:

- The project needs unit-testable domain logic and CLI integration tests.
- A library-first shape keeps future IDE integration, plugins, or alternate
  frontends possible without moving CLI code out of the way.

Source material:

- `AGENTS.md`
- `Documentation/code-style.md`
- Rust package metadata in `Cargo.toml`
- Rust edition reference: https://doc.rust-lang.org/edition-guide/rust-2021/

### 2. TOML for Meta and Project Configuration

Decision:

- Use TOML and Serde for project and Meta configuration.
- Project Config owns imports; Meta files themselves do not declare imports.

Rationale:

- TOML is readable for configuration-heavy authoring and maps well to Serde data
  structures.
- Keeping imports at the project/config layer avoids circular responsibilities in
  individual Meta schema files.

Alternatives considered:

- YAML or JSON configs. Deferred because TOML is less noisy for the current
  table-oriented schema and is already supported by a stable Rust crate.

Source material:

- `Documentation/Design/03-configuration.md`
- `Documentation/Design/README.md`
- `toml` crate: https://crates.io/crates/toml
- `serde` crate: https://serde.rs/

### 3. CSV First, with Provider-Oriented Expansion Later

Decision:

- Implement CSV loading first through explicit data load plans.
- Keep future providers such as xlsx, Lark Sheet, and Google Sheet as design
  targets but not first-phase implementation requirements.

Rationale:

- CSV gives deterministic local tests and simple fixtures.
- Provider-oriented design avoids hard-coding local-file assumptions into later
  remote source support.

Alternatives considered:

- Generic `local_file` provider. Rejected in canonical design in favor of clear
  provider names like `csv`, `xlsx`, `lark_sheet`, and `google_sheet`.

Source material:

- `Documentation/Design/README.md`
- `Documentation/Implementation/03-data-module-detail-design.md`
- `csv` crate: https://crates.io/crates/csv

### 4. Linked Meta Model Instead of Late String Resolution Everywhere

Decision:

- Parse raw Meta into serializable structs, then build a linked model for
  references such as field types, indexes, fragments, validators, and FKs.

Rationale:

- Link-time diagnostics catch structural problems before validation/export.
- Later stages can rely on resolved shape and focus on target/data semantics.
- The raw model remains close to TOML for parsing, validation messages, and
  documentation.

Temporary constraint:

- Self-referential linked structures are avoided. For foreign keys, linked FK
  metadata stores target table/index names instead of embedding target table
  references.

Source material:

- `Documentation/Implementation/02-meta-module-detail-design.md`
- `Documentation/Implementation/06-foreign-key-detail-design.md`

### 5. Primary Index, Not Primary Key

Decision:

- Use `primary_index` and `primary = true` on indexes. Do not keep a separate
  `primary_key` concept.

Rationale:

- The project already needs multi-field indexes and generated repository helpers.
  A primary index keeps identity semantics inside the same index model.

Source material:

- `Documentation/Design/README.md`
- `Documentation/Design/04-meta-model.md`

### 6. Target Tags as the Central Filtering Mechanism

Decision:

- Targets expose tags. Tables, fields, indexes, validators, fragments, and FKs
  use `export_when` to match those tags.
- Collection rows use `row_export` for row-level inclusion/exclusion.

Rationale:

- A target-aware view lets validation, runtime export, and codegen share the
  same visibility semantics.
- This replaces older concepts such as `tool_only`, `validator_only`, and
  `filter_field` with one mechanism.

Source material:

- Commit `3b2bedf docs(targets): define target tag filtering semantics`
- Commit `b1326d6 feat(targets): implement target tags across validation, export, and codegen`
- `Documentation/Design/05-data-validation.md`
- `Documentation/Design/06-export-cli.md`

### 7. Registered Validator Framework

Decision:

- Use a registry for validators and implement range validation as a first built-in
  validator.

Rationale:

- Validators need consistent scope handling, diagnostics, target visibility, and
  parameter parsing.
- A registry supports future built-ins and plugin-facing extension points without
  hard-coding each validator into the validation pipeline.

Source material:

- `Documentation/Implementation/05-validator-framework-detail-design.md`
- Commit `a3a0e26 docs(validation): add validator framework design`
- Commit `f6fe760 feat(validation): introduce validator registry and range validator`

### 8. Coverage as Design Evidence, Not Just a Percentage

Decision:

- Use `cargo-llvm-cov` aliases for coverage and inspect changed modules after
  meaningful feature/test changes.

Rationale:

- The project has many rule branches. Coverage helps confirm normal and error
  paths, but the important check is whether meaningful rule branches were tested.

Temporary constraint:

- No hard global coverage threshold is enforced yet.

Source material:

- `Documentation/Tech/unit-test-coverage.md`
- `cargo-llvm-cov`: https://github.com/taiki-e/cargo-llvm-cov
- LLVM source-based coverage:
  https://doc.rust-lang.org/rustc/instrument-coverage.html

### 9. Runtime Data Export Registry and MessagePack Support

Decision:

- Runtime data export uses a registry with built-in JSON and MessagePack
  exporters.

Rationale:

- A registry keeps exporter selection explicit and supports future custom data
  formats or special exporters.
- MessagePack is useful for compact runtime payloads while JSON remains the
  default and easiest debugging format.

Source material:

- `Documentation/Design/06-export-cli.md`
- `serde_json` crate: https://crates.io/crates/serde_json
- `rmp-serde` crate: https://crates.io/crates/rmp-serde
- MessagePack specification: https://msgpack.org/

### 10. Multi-Language Codegen Through an Internal IR

Decision:

- Build a `CodeModel` IR before emitting Rust, TypeScript, or C++.
- Codegen is target-aware and receives runtime data artifact paths when data
  export is enabled.

Rationale:

- A language-neutral IR avoids duplicating Meta filtering and naming logic in
  each generator.
- Target-aware codegen prevents hidden fields/indexes from leaking into generated
  APIs.
- Code-only targets still use stable conventional data paths so generated APIs
  remain predictable.

Source material:

- `Documentation/Implementation/04-code-export-module-detail-design.md`
- Commits `2f29fd8`, `04cf7aa`, and `fcdb240`

### 11. Deterministic Resource-Controlled Parallelism

Decision:

- Add execution options such as `jobs`, `io_concurrency`, `network_concurrency`,
  and `serial`.
- Use a local Rayon CPU pool for current CPU/blocking work rather than a global
  pool or full async runtime.

Rationale:

- ConfigForge is a batch CLI tool and should not occupy all machine resources by
  default.
- A local Rayon pool lets each build honor CLI `jobs` settings.
- Full Tokio integration is deferred until remote providers or async artifact
  writers make it worthwhile.

Alternatives considered:

- Tokio-only execution. Deferred because CPU-heavy validation/codegen should not
  occupy async workers and `spawn_blocking` does not directly express a strict
  user-controlled CPU budget.

Source material:

- `Documentation/Tech/task-system-design.md`
- `rayon` crate: https://crates.io/crates/rayon
- Tokio runtime docs for future reference: https://docs.rs/tokio/latest/tokio/runtime/

### 12. Variant-Source Fragments, Not Fragment Join Mode

Decision:

- Implement polymorphic fragments through variant source mode.
- Each row exports only fragments activated by its originating source metadata.
- Fragment join mode is not implemented in this phase.

Rationale:

- Variant source mode maps naturally to table authoring where different source
  files provide different row shapes.
- Per-row source metadata gives validation/export enough context to avoid
  exporting all declared fragments for every row.

Source material:

- Commit `201dc23 docs(fragment): design polymorphic fragment support`
- Commit `c9d38a5 feat(fragment): implement variant source fragments end-to-end`
- Commit `bb97834 fix(fragment): tighten variant source mode and target validation`
- `Documentation/Design/04-meta-model.md`

### 13. Foreign Keys as First-Class Meta Relations

Decision:

- Model foreign keys as first-class Meta relationships rather than ordinary
  validators.
- Support field-level single-key sugar and table-level composite FKs.
- Normalize both forms into `ResolvedForeignKeyMeta` before linking.
- Use raw string cell values for FK key comparison.
- Phase one forbids ObjectTable participation in FKs.

Rationale:

- FKs affect Meta link semantics, target visibility, validation, and codegen IR,
  so keeping them as plain validators would hide important structural rules.
- Raw string key semantics avoid target-dependent typed canonicalization and keep
  source/target comparison predictable.
- ObjectTable FK support is deferred to avoid ambiguous relationship semantics in
  phase one.

Review-driven follow-up:

- Link-time FK checks were strengthened so linked FKs are structurally
  trustworthy before validation/codegen:
  - target index must be unique or primary;
  - field-level FK can only target a single-field index;
  - source and target field counts must match;
  - source and target key field types must be compatible.

Source material:

- `Documentation/Implementation/06-foreign-key-detail-design.md`
- Commit `16b802c docs(foreign-key): design FK meta, validation, and codegen staging`
- Commit `4485a49 feat(foreign-key): implement FK meta, validation, and codegen IR`
- Commit `0bfc945 fix(foreign-key): strengthen link guards and target-aware tests`
- Local review notes in `review.txt` during FK implementation review.

### 14. Comment and Commit Style as Explicit Workflow Rules

Decision:

- Commit messages use concise English Conventional Commits style.
- Code comments must be in English and should explain why, not how.

Rationale:

- The project is accumulating many design documents and rule-heavy modules.
  Consistent commit and comment style makes future archaeology easier.
- Comments should be rare and focused on constraints, trade-offs, and invariants.

Source material:

- `Documentation/code-style.md`
- Conventional Commits: https://www.conventionalcommits.org/
- Commit `fc3ca24 docs(workflow): document git commit message rules`
- Commit `be4e210 docs(style): define comment rules and clarify complex paths`

## Current Documentation Map

- `Documentation/Design/` — current canonical product behavior.
- `Documentation/Implementation/` — implementation plans and module designs.
- `Documentation/Tech/` — engineering workflow and technical practice notes.
- `Documentation/code-style.md` — Rust, testing, commit, and comment rules.
- `Documentation/glossary.md` — terminology and naming.
- `Documentation/DevLogs/` — raw historical phase logs, starting with this file.

## External Source References Recorded in This Phase

The following URLs were recorded as relevant source material or technology
references. This log does not claim each URL was freshly consulted on
2026-05-21; it records the sources that explain the choices made in this phase.

- Rust 2021 edition: https://doc.rust-lang.org/edition-guide/rust-2021/
- Serde: https://serde.rs/
- TOML crate: https://crates.io/crates/toml
- CSV crate: https://crates.io/crates/csv
- Clap crate: https://docs.rs/clap/latest/clap/
- Rayon crate: https://crates.io/crates/rayon
- Tokio runtime docs: https://docs.rs/tokio/latest/tokio/runtime/
- serde_json crate: https://crates.io/crates/serde_json
- rmp-serde crate: https://crates.io/crates/rmp-serde
- MessagePack: https://msgpack.org/
- cargo-llvm-cov: https://github.com/taiki-e/cargo-llvm-cov
- Rust source-based coverage:
  https://doc.rust-lang.org/rustc/instrument-coverage.html
- Conventional Commits: https://www.conventionalcommits.org/

## Follow-up Questions

- When remote providers are introduced, should the project add Tokio immediately
  or first wrap provider-specific async clients behind a sync boundary?
- Should FK metadata eventually generate language-specific helper methods or
  remain IR-only until a concrete runtime API request appears?
- What is the right migration path if ObjectTable FKs become necessary?
- Should target-aware validation eventually emit structured diagnostics rather
  than string-only `ConfigForgeError::Validation` messages?
- When project configs grow, should compatibility be handled through explicit
  schema versions or small targeted migrations?
