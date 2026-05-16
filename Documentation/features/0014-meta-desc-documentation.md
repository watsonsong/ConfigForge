# Feature 0014: Meta `desc` Documentation Metadata

## Summary

Add canonical design support for a `desc` field on Meta definitions. `desc` provides structured human-readable descriptions for definitions such as tables, fields, enums, enum values, named structs, struct fields, fragments, indexes, foreign keys, validators, and export filters.

## Motivation

Plain TOML/XML comments help humans reading source files, but comments are not preserved in the parsed MetaModel and cannot reliably feed code generators, inspect commands, or generated documentation. A structured `desc` property lets Meta authors document configuration semantics once and reuse that documentation in generated code comments and tooling output.

## Design Decisions

- `desc` is optional and may appear on multiple Meta definition levels.
- `desc` is stored in the canonical MetaModel.
- `desc` does not affect data parsing, validation, filtering, runtime export, or generated runtime data content.
- Code generators should convert `desc` into target-language comments with proper escaping and multi-line formatting.
- Inspect/debug commands should display `desc` when showing merged Meta definitions.
- TOML/XML source comments remain source-only; generated comments should use `desc`.

## Acceptance Criteria

- Consolidated design docs describe `desc` semantics and supported levels.
- TOML and XML examples show `desc` on tables, fields, enums, enum values, and structs.
- Export/CLI docs define how code generators use `desc`.
- User manual and QA docs include workflows and test dimensions for `desc` and generated comments.

