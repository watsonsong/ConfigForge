# Feature Spec 0001: Project Foundation

## Summary

Create the initial ConfigForge Rust CLI project foundation, including Git repository initialization, module structure, documentation layout, Meta terminology, Meta table-kind design, and test fixtures.

## Documentation Scope

This feature includes the first durable documentation pass for project terminology and Meta design. The documentation establishes shared vocabulary and design direction before deeper implementation work.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`

Advanced Field type, nested struct/array layout, asset validation, text/i18n semantics, and Sheet header mapping are continued in `Documentation/features/0002-meta-field-layout-design.md`.

## Requirements

- Initialize a Git repository and Rust CLI package.
- Provide a clear `AGENTS.md` project index.
- Create `Documentation/` for durable design, rules, code style, and feature specs.
- Maintain a project glossary for core domain terms, Chinese/English naming, and recommended code symbols.
- Document the Meta design in detail before implementation.
- Model the first version of Meta data using TOML.
- Define two supported Meta table kinds:
  - Object tables: each row describes one field, and the whole table represents one constant object.
  - Collection tables: each row describes one object, and each column describes one field.
- Read CSV data into normalized table rows.
- Validate fields for presence, required values, primitive types, uniqueness, and index references.
- Export validated data as JSON and generated Rust code stubs.
- Include unit tests and an integration fixture representing a standard project.

## Meta Table-Kind Foundation

Feature 0001 establishes the following table-kind concepts for future implementation:

- Object Table: a table where each source row describes one field and the full table exports as one constant object.
- Collection Table: a table where each source row describes one object and each source column describes one field.

The recommended future Meta discriminator is `kind = "object"` or `kind = "collection"`. The recommended Rust naming is `TableKind::Object` and `TableKind::Collection`.

## Non-Goals

- Full spreadsheet provider support beyond CSV.
- Production-ready generated code for a specific game engine.
- Complete constraint expression language.
- Full multi-column index enforcement.
- Implementing object-table or collection-table logic in this documentation-only update.
- Changing `src/` or `tests/` as part of the terminology and Meta design documentation step.

## Acceptance Criteria

### Documentation Acceptance for This Update

- `Documentation/glossary.md` exists and defines core project terms.
- `Documentation/meta-design.md` exists and describes object tables and collection tables.
- `AGENTS.md` and `Documentation/README.md` link to the new documents.
- This feature spec links to the glossary and Meta design documents.
- “对象表/列表” is consistently recommended as Object Table / `ObjectTable` / `TableKind::Object`.
- “集合表/行表” is consistently recommended as Collection Table / `CollectionTable` / `TableKind::Collection`.
- No source code or test files are changed in this documentation-only step.

### Broader Implementation Acceptance

- `cargo test` passes.
- `config-forge validate --meta tests/fixtures/standard_project/meta.toml --data tests/fixtures/standard_project/data` succeeds.
- `config-forge export --meta tests/fixtures/standard_project/meta.toml --data tests/fixtures/standard_project/data --out <tmp>` writes JSON and Rust stub outputs.
