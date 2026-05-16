# Documentation

This directory contains durable project knowledge for ConfigForge.

## Structure

- `core-rules.md` records project rules that should remain true across features.
- `design-purpose.md` records the product purpose and architectural direction.
- `code-style.md` records Rust style, module, and testing conventions.
- `glossary.md` records project terminology, Chinese/English naming, and recommended code symbols.
- `meta-design.md` records the detailed design of Meta concepts, table forms, field types, and builtin value types.
- `data-design.md` records normalized source sheets, typed datasets, runtime data exporters, special exporters, context tags, filtering, and tool-only fields.
- `export-design.md` records export target and target-specific type mapping design.
- `validator-design.md` records Validator scopes, phases, code structure, configuration model, diagnostics, and valuable checks.
- `implementation-requirements.md` records implementation-level requirements for plugins, parallel execution, async I/O, and debuggable diagnostics.
- `features/` stores one spec per feature or development conversation.
- `features/0001-project-foundation.md` records the initial project foundation spec.
- `features/0002-meta-field-layout-design.md` records advanced Meta field type and sheet layout design.
- `features/0003-meta-syntax-index-foreign-key-design.md` records Meta syntax, index, and foreign-key design.
- `features/0004-multi-source-table-design.md` records multi-source table split/override design.
- `features/0005-polymorphic-fragment-table-design.md` records polymorphic record and fragment composition design.
- `features/0006-extensible-source-provider-design.md` records extensible source provider design.
- `features/0007-enum-named-struct-type-mapping-design.md` records enum, reusable named struct, struct cycle detection, and export type mapping design.
- `features/0008-validator-system-design.md` records the staged Validator system and recommended correctness checks.
- `features/0009-config-forge-pipeline-design.md` records Data pipeline, runtime data format extensibility, special exporters, export context filtering, and tool-only fields.
- `features/0010-source-ordered-collection-access.md` records CollectionTable source-ordered array access without requiring indexes.
- `features/0011-builtin-value-types.md` records builtin color, vector, rotation, date/time, duration, and range value types.
- `features/0012-implementation-technical-requirements.md` records pluginization, parallelization, async I/O, and debuggability requirements.

## Maintenance Rules

- Keep this directory focused on decisions that future contributors need.
- Put concise navigation and project summaries in `../AGENTS.md`.
- Add or update a feature spec whenever a feature is introduced, redesigned, or meaningfully expanded.
- When introducing new domain concepts, update `glossary.md` before or together with feature specs.
