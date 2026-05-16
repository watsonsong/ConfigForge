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
- `Documentation/core-rules.md` — core project rules and development invariants.
- `Documentation/design-purpose.md` — product purpose and high-level architecture.
- `Documentation/code-style.md` — Rust code conventions and test expectations.
- `Documentation/glossary.md` — project terminology, Chinese/English naming, and recommended code symbols.
- `Documentation/meta-design.md` — detailed Meta design, including object tables, collection tables, field types, builtin value types, enums, named structs, nested structures, sheet layout mapping, Meta syntax, indexes, foreign keys, multi-source tables, polymorphic fragments, and source providers.
- `Documentation/data-design.md` — Data pipeline design, including normalized source sheets, typed datasets, runtime data formats, special exporters, export context tags, filtering, and tool-only fields.
- `Documentation/export-design.md` — export target design and target-specific logical-to-runtime type mapping.
- `Documentation/validator-design.md` — Validator scopes, phases, code structure, configuration model, diagnostics, and recommended correctness checks.
- `Documentation/implementation-requirements.md` — implementation-level requirements for dynamic plugins, parallel execution, async I/O, and debuggable structured diagnostics.
- `Documentation/Design/` — consolidated canonical design set for product behavior, execution stages, configuration, Meta model, validation/export semantics, CLI, user workflows, and QA input.
- `Documentation/features/0001-project-foundation.md` — first feature spec for this foundation work.
- `Documentation/features/0002-meta-field-layout-design.md` — feature spec for Field model, struct/array layout, asset validation, text/i18n semantics, and header mapping.
- `Documentation/features/0003-meta-syntax-index-foreign-key-design.md` — feature spec for TOML/XML Meta syntax, XML Schema support, indexes, generated query semantics, and foreign keys.
- `Documentation/features/0004-multi-source-table-design.md` — feature spec for multi-source table split/override modes and source conflict validation.
- `Documentation/features/0005-polymorphic-fragment-table-design.md` — feature spec for composition-based polymorphic records and fragments.
- `Documentation/features/0006-extensible-source-provider-design.md` — feature spec for extensible source providers such as CSV, XLSX, Lark/Feishu Sheet, and Google Sheet.
- `Documentation/features/0007-enum-named-struct-type-mapping-design.md` — feature spec for enums, reusable named structs, struct cycle detection, and export type mapping.
- `Documentation/features/0008-validator-system-design.md` — feature spec for staged Validator architecture and correctness check catalog.
- `Documentation/features/0009-config-forge-pipeline-design.md` — feature spec for Data pipeline, runtime data format extensibility, special exporters, context filtering, and tool-only fields.
- `Documentation/features/0010-source-ordered-collection-access.md` — feature spec for CollectionTable source-ordered array access without requiring indexes.
- `Documentation/features/0011-builtin-value-types.md` — feature spec for builtin color, vector, rotation, date/time, duration, and range value types.
- `Documentation/features/0012-implementation-technical-requirements.md` — feature spec for pluginization, parallelization, async I/O, and debuggability requirements.
- `Documentation/features/0013-consolidated-design-docs.md` — feature spec for the consolidated design documentation set.
- `Documentation/features/0014-meta-desc-documentation.md` — feature spec for `desc` metadata on Meta definitions and generated comments.

## Current Source Layout

- `src/main.rs` — binary entry point.
- `src/cli.rs` — CLI command definitions.
- `src/meta.rs` — Meta schema loading and parsing.
- `src/source.rs` — sheet/data source loading, currently CSV.
- `src/validation.rs` — schema and constraint validation.
- `src/exporter.rs` — export orchestration and artifact generation.
- `src/error.rs` — shared error model.
- `tests/cli_integration.rs` — end-to-end CLI integration tests.
- `tests/fixtures/standard_project/` — canonical sample project for integration validation.

## Agent Working Rules

1. Keep `AGENTS.md` as the short project index; place detailed designs under `Documentation/`.
2. Every meaningful feature conversation should produce or update one spec under `Documentation/features/`.
3. Prefer small, typed Rust modules with unit tests near the logic and integration tests under `tests/`.
4. Keep generated/exported outputs out of version control unless they are intentional fixtures.
5. Run `cargo fmt` and `cargo test` before considering a code change complete.
