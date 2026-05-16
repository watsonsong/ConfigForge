# Feature Spec 0008: Validator System Design

## Summary

Design a staged Validator system for ConfigForge correctness checks. The system supports built-in validators such as required fields, unique indexes, and foreign keys, plus configurable validators for field-level, record-level, collection-level, multi-collection, and export-level business rules.

## Documentation Scope

This feature is a documentation-only design step. It adds a dedicated Validator design document and updates project terminology and indexes so later implementation can introduce a structured validation module.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/validator-design.md`
- `Documentation/features/0002-meta-field-layout-design.md`
- `Documentation/features/0003-meta-syntax-index-foreign-key-design.md`
- `Documentation/features/0004-multi-source-table-design.md`
- `Documentation/features/0007-enum-named-struct-type-mapping-design.md`

## Requirements

- Support built-in validators required by core Meta semantics.
- Support configurable validators declared on fields, tables, project-level validation config, or export targets.
- Support field-level validators such as required, type parse, range, regex, array length, enum value, and asset exists.
- Support record-level validators for ObjectTable objects and CollectionTable rows, including cross-field checks.
- Support collection-level validators over one final logical table, such as unique indexes, dense indexes, group uniqueness, range overlap, and tree cycle checks.
- Support multi-collection validators across multiple tables, such as foreign keys and cross-table consistency checks.
- Support export validators for type mapping, output path conflicts, and generated-name conflicts.
- Validators should run in deterministic phases and produce stable diagnostics.
- Validators should collect multiple diagnostics in one run whenever possible.
- Diagnostics should include table, field path, record key when available, source location, related locations, stable code, and optional hint.

## Validator Scopes

The design defines these scopes:

- `field`: one value at one field path.
- `record`: one ObjectTable object or one CollectionTable record.
- `collection`: one final logical table after source composition.
- `multi_collection`: multiple final logical tables.
- `project`: Meta/source/export project-level checks.
- `export`: target-specific export configuration and generated artifact checks.

## Validation Phases

Recommended execution order:

1. Meta validation.
2. Source declaration validation.
3. Source normalization validation.
4. Field validation.
5. Record validation.
6. Source composition validation.
7. Collection validation.
8. Multi-collection validation.
9. Export validation.

## Recommended Code Structure

Future Rust implementation may organize validation as:

```text
src/validation/
  context.rs
  diagnostic.rs
  plan.rs
  registry.rs
  runner.rs
  scope.rs
  validators/
    meta.rs
    source.rs
    field.rs
    record.rs
    collection.rs
    multi_collection.rs
    export.rs
```

The preferred implementation shape is “scope-specific traits plus a unified runner”, because field validators and multi-collection validators need very different input contexts.

## Valuable Built-In Checks

High-priority built-in validators:

- table/field/enum/struct name uniqueness;
- named struct reference exists;
- named struct cycle detection;
- index field exists;
- primary_index references a unique index;
- CollectionTable access strategy is valid: indexes or `preserve_source_order = true`;
- field required;
- primitive type parse;
- builtin value parse for color/vector/rotation/datetime/range;
- enum value exists;
- JSON cell shape matches field type;
- array element validation;
- unique and composite unique index duplicates;
- dense integer index no holes;
- foreign key target exists;
- source provider and locator validity;
- header path exists and duplicate header path checks;
- base source split conflict;
- override duplicate key and missing target checks;
- export type mapping validity.

## Valuable Configurable Checks

High-value configurable validators:

- numeric range min/max;
- string length and regex pattern;
- allowed value whitelist;
- array length min/max;
- no duplicate array element;
- cross-field comparison such as `min <= max`;
- conditional required fields;
- mutually exclusive fields;
- group-level uniqueness;
- ordered monotonic sequences;
- no interval overlap and complete interval coverage;
- weight/probability sum checks;
- same-table parent exists and tree no cycle;
- cross-table ID namespace uniqueness;
- foreign key requires target fragment;
- generated code name conflict checks.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No final expression language syntax.
- No plugin ABI or dynamic loading implementation.
- No final CLI output format for validation diagnostics.

## Acceptance Criteria

- `Documentation/validator-design.md` exists and describes Validator scopes, phases, code structure, configuration model, diagnostics, and valuable checks.
- `Documentation/glossary.md` includes Validator terminology.
- `Documentation/meta-design.md` references the staged Validator system and dedicated Validator design document.
- `Documentation/README.md` links to Validator design and this feature spec.
- `AGENTS.md` links to Validator design and this feature spec.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
