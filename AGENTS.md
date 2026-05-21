# ConfigForge Project Index

ConfigForge is a Rust CLI tool for game data export workflows. It lets game designers author data in spreadsheet-like sources, validates that data against explicit meta definitions, and produces runtime-ready artifacts for game projects.

## Project Goals

- Define exported data structures through Meta files: tables, fields, field types, indexes, and constraints.
- Load designer-authored data from CSV first, with room for additional Sheet providers later.
- Validate data deterministically before export so game projects never consume malformed configuration.
- Generate game-facing artifacts from the same source of truth:
  - data files suitable for runtime loading;
  - generated code stubs that make data access type-aware.

## Documentation Index

- `Documentation/README.md` — documentation map and maintenance rules.
- `Documentation/code-style.md` — Rust code conventions and test expectations.
- `Documentation/glossary.md` — project terminology, Chinese/English naming, and recommended code symbols.
- `Documentation/Design/` — consolidated canonical design set for product behavior, execution stages, configuration, Meta model, validation/export semantics, CLI, user workflows, and QA input.
- `Documentation/Implementation/` — program implementation design, module boundaries, detailed module plans, and test organization guidance.
- `Documentation/Tech/` — technical practice notes for engineering configuration, execution commands, third-party library choices, version constraints, and the rationale behind tooling decisions.
- `Documentation/DevLogs/` — raw phase-by-phase development logs for historical decision context, temporary choices, and source references.

## Current Source Layout

- `src/main.rs` — binary entry point.
- `src/cli.rs` — CLI command definitions.
- `src/meta/` — Meta schema loading, parsing, linking, validation, and tests.
- `src/source.rs` — sheet/data source loading, currently CSV.
- `src/validation.rs` — schema and constraint validation.
- `src/exporter.rs` — export orchestration and artifact generation.
- `src/error.rs` — shared error model.
- `tests/cli_integration.rs` — end-to-end CLI integration tests.
- `tests/fixtures/standard_project/` — canonical sample project for integration validation.

## Agent Working Rules

1. Keep `AGENTS.md` as the short project index; place detailed designs under `Documentation/`.
2. Every meaningful product design conversation should update the consolidated documents under `Documentation/Design/`.
3. Put program implementation design and module-level detailed design under `Documentation/Implementation/`.
4. Put technical configuration, execution workflows, third-party library selection, dependency version constraints, and tooling decision rationale under `Documentation/Tech/`.
5. Use `Documentation/DevLogs/` for raw phase snapshots, temporary decisions, and source material that should not be continuously rewritten as canonical design.
6. Prefer small, typed Rust modules with unit tests near the logic and integration tests under `tests/`.
7. Keep generated/exported outputs out of version control unless they are intentional fixtures.
8. Run `cargo fmt` and `cargo test` before considering a code change complete.
