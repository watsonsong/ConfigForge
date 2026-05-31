# ConfigForge Project Index

ConfigForge is a schema-driven game configuration export tool that validates designer-authored data and generates runtime-ready artifacts.

## Project Goals

- Define exported data structures through Meta files: tables, fields, field types, indexes, and constraints.
- Load designer-authored data from CSV first, with room for additional Sheet providers later.
- Validate data deterministically before export so game projects never consume malformed configuration.
- Generate game-facing artifacts from the same source of truth:
  - data files suitable for runtime loading;
  - generated code stubs that make data access type-aware.

## Documentation Entry Points for AI Agents

Use this file as the short entry index. For any non-trivial change, follow the links below instead of inferring architecture only from source code.

1. Start with [`Documentation/README.md`](Documentation/README.md) for the full documentation map and maintenance rules.
2. Read [`Documentation/code-style.md`](Documentation/code-style.md) before editing Rust code or committing changes.
3. Check [`Documentation/glossary.md`](Documentation/glossary.md) when adding or renaming domain concepts.
4. For product behavior, read the consolidated design set under [`Documentation/Design/`](Documentation/Design/):
   - [`Documentation/Design/README.md`](Documentation/Design/README.md) — design index and implementation status matrix.
   - [`Documentation/Design/01-overview.md`](Documentation/Design/01-overview.md) — tool purpose, roles, terms, and scope.
   - [`Documentation/Design/02-execution-pipeline.md`](Documentation/Design/02-execution-pipeline.md) — end-to-end execution stages.
   - [`Documentation/Design/03-configuration.md`](Documentation/Design/03-configuration.md) — Project / Meta / Source / Target config semantics.
   - [`Documentation/Design/04-meta-model.md`](Documentation/Design/04-meta-model.md) — tables, fields, types, indexes, foreign keys, multi-source, fragments.
   - [`Documentation/Design/05-data-validation.md`](Documentation/Design/05-data-validation.md) — ExportView, target filtering, validation, diagnostics.
   - [`Documentation/Design/06-export-cli.md`](Documentation/Design/06-export-cli.md) — runtime data export, codegen, artifacts, CLI.
   - [`Documentation/Design/07-user-manual-and-qa.md`](Documentation/Design/07-user-manual-and-qa.md) — user workflows and QA input dimensions.
5. For implementation changes, read [`Documentation/Implementation/01-module-outline.md`](Documentation/Implementation/01-module-outline.md) first, then the module document that matches the source area being changed.
6. For engineering practice, use [`Documentation/Tech/`](Documentation/Tech/):
   - [`Documentation/Tech/task-system-design.md`](Documentation/Tech/task-system-design.md) — concurrency/resource-limit design.
   - [`Documentation/Tech/unit-test-coverage.md`](Documentation/Tech/unit-test-coverage.md) — coverage tooling and evaluation workflow.
   - [`Documentation/Tech/cpp-runtime-dependencies.md`](Documentation/Tech/cpp-runtime-dependencies.md) — C++ runtime adapter dependency and CMake policy.
7. Use [`Documentation/DevLogs/`](Documentation/DevLogs/) only as historical evidence. Do not treat DevLogs as current canonical design when they conflict with `Design/` or `Implementation/`.

### Documentation Selection Guide

| Task type | Required docs |
| --- | --- |
| Product semantics or user-visible behavior | `Documentation/Design/README.md` plus the relevant `Design/*.md` file. |
| Module boundary, dependency direction, or refactor | [`Documentation/Implementation/01-module-outline.md`](Documentation/Implementation/01-module-outline.md) plus affected module docs. |
| Meta schema, link, validation | [`Documentation/Implementation/04-meta-module-detail-design.md`](Documentation/Implementation/04-meta-module-detail-design.md), and if behavior changes also [`Documentation/Design/04-meta-model.md`](Documentation/Design/04-meta-model.md). |
| Data loading, CSV, ObjectTable source layout, resolver | [`Documentation/Implementation/05-data-module-detail-design.md`](Documentation/Implementation/05-data-module-detail-design.md), [`Documentation/Design/05-data-validation.md`](Documentation/Design/05-data-validation.md). |
| Validation, validators, diagnostics | [`Documentation/Implementation/06-validation-module-detail-design.md`](Documentation/Implementation/06-validation-module-detail-design.md), [`Documentation/Implementation/02-diagnostic-module-detail-design.md`](Documentation/Implementation/02-diagnostic-module-detail-design.md), [`Documentation/Design/05-data-validation.md`](Documentation/Design/05-data-validation.md). |
| Artifact writing, dry-run, output path conflicts | [`Documentation/Implementation/07-artifact-module-detail-design.md`](Documentation/Implementation/07-artifact-module-detail-design.md), [`Documentation/Design/06-export-cli.md`](Documentation/Design/06-export-cli.md). |
| Foreign key semantics and linked helpers | [`Documentation/Design/04-meta-model.md`](Documentation/Design/04-meta-model.md), [`Documentation/Implementation/11-foreign-key-detail-design.md`](Documentation/Implementation/11-foreign-key-detail-design.md), [`Documentation/Implementation/09g-codegen-index-and-relation-helpers.md`](Documentation/Implementation/09g-codegen-index-and-relation-helpers.md). |
| Struct expanded value / ObjectTable layout | [`Documentation/Design/04-meta-model.md`](Documentation/Design/04-meta-model.md), [`Documentation/Implementation/12-struct-expanded-value-detail-design.md`](Documentation/Implementation/12-struct-expanded-value-detail-design.md), [`Documentation/Implementation/05-data-module-detail-design.md`](Documentation/Implementation/05-data-module-detail-design.md). |
| Runtime binary format | [`Documentation/Design/06-export-cli.md`](Documentation/Design/06-export-cli.md), [`Documentation/Implementation/13-runtime-binary-format-detail-design.md`](Documentation/Implementation/13-runtime-binary-format-detail-design.md), [`Documentation/Implementation/09a-codegen-runtime-reader-contract.md`](Documentation/Implementation/09a-codegen-runtime-reader-contract.md). |
| Runtime data export / gettext POT | [`Documentation/Implementation/08-exporter-module-detail-design.md`](Documentation/Implementation/08-exporter-module-detail-design.md), [`Documentation/Design/06-export-cli.md`](Documentation/Design/06-export-cli.md). |
| Code generation or runtime helpers | [`Documentation/Implementation/09-codegen-module-detail-design.md`](Documentation/Implementation/09-codegen-module-detail-design.md), split topics `09a`~`09h`, plus [`Documentation/Design/06-export-cli.md`](Documentation/Design/06-export-cli.md). |
| Build orchestration, target data/code tasks, parallel execution | [`Documentation/Implementation/10-build-module-detail-design.md`](Documentation/Implementation/10-build-module-detail-design.md), [`Documentation/Tech/task-system-design.md`](Documentation/Tech/task-system-design.md). |
| Testing/coverage quality | [`Documentation/code-style.md`](Documentation/code-style.md), [`Documentation/Tech/unit-test-coverage.md`](Documentation/Tech/unit-test-coverage.md). |

## Current Source Layout

- `src/main.rs` — binary entry point.
- `src/cli.rs` — CLI command definitions.
- `src/meta/` — Meta schema loading, parsing, linking, validation, and tests.
- `src/data/` — CSV data source loading, data load planning, normalization, and resolved row models.
- `src/codegen/` — language-neutral CodeModel IR plus Rust / TypeScript / C++ generators.
- `src/validation/` — schema, target-aware data validation, diagnostics, and validator registry.
- `src/project/` — ConfigForge project TOML schema, import merging, path normalization, and Meta extraction.
- `src/build/` — project build orchestration, target normalization, runtime/code task binding, and artifact generation scheduling.
- `src/exporter/` — runtime data export contracts, JSON / MessagePack exporters, and future data format exporter extension point.
- `src/diagnostic/` — shared diagnostic foundations and crate error model.
- `runtime/` — standard runtime helpers/templates and compliance fixtures.
- `tests/cli_integration.rs` — end-to-end CLI integration tests.
- `tests/fixtures/standard_project/` — canonical sample project for integration validation.

## Agent Working Rules

1. Keep `AGENTS.md` as the short project index; place detailed designs under `Documentation/`.
2. Every meaningful product design conversation should update the consolidated documents under [`Documentation/Design/`](Documentation/Design/).
3. Put program implementation design and module-level detailed design under `Documentation/Implementation/`.
4. Put technical configuration, execution workflows, third-party library selection, dependency version constraints, and tooling decision rationale under [`Documentation/Tech/`](Documentation/Tech/).
5. Use [`Documentation/DevLogs/`](Documentation/DevLogs/) for raw phase snapshots, temporary decisions, and source material that should not be continuously rewritten as canonical design.
6. Keep canonical documentation focused on current state and planned future state; put historical reasoning, removed approaches, and temporary transition notes in [`Documentation/DevLogs/`](Documentation/DevLogs/).
7. Prefer small, typed Rust modules with unit tests near the logic and integration tests under `tests/`.
8. Keep generated/exported outputs out of version control unless they are intentional fixtures.
9. Run `cargo fmt` and `cargo test` before considering a code change complete. Ensure the relevant integration tests and runtime harness checks are covered, not only unit tests; when changing testable logic, evaluate whether unit-test coverage remains sufficient and add focused tests where coverage is weak. Run `cargo lint` for code changes when the local toolchain has Clippy available.
10. Keep module dependencies one-way as defined in [`Documentation/Implementation/01-module-outline.md`](Documentation/Implementation/01-module-outline.md); do not introduce cycles for convenience.
