# Feature Spec 0005: Polymorphic Fragment Table Design

## Summary

Support polymorphic records in one logical collection table through composition-based fragments. This supports game item-style tables where all records share common identity and fields, while different item functions attach different fragment data such as stats, skills, or usable effects.

## Documentation Scope

This feature is a documentation-only design step. It updates the glossary and Meta design so later implementation can support polymorphic collection records, fragment definitions, fragment source tables, validation, and generated fragment query APIs.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/features/0002-meta-field-layout-design.md`
- `Documentation/features/0004-multi-source-table-design.md`

## Requirements

- Prefer composition over inheritance.
- One logical polymorphic table owns one shared ID/index namespace.
- Common fields are available on every record.
- Variant-specific fields are modeled as optional fragments.
- One record may own zero or more fragments.
- Physical data may be split into different fragment source tables, such as weapons, equipment, and usable items.
- Fragment source tables contribute records to the same logical table.
- IDs must not conflict across base fragment source tables.
- Override sources use the table primary index and treat duplicate primary-index keys as overrides, not separate variants.
- Generated runtime access should support checking whether a record owns a fragment and retrieving it safely.

## Item Table Example

Logical table: `items`.

Common fields:

- `id`
- `name`
- `icon`

Fragments:

- `stats`: attributes/stat structure, shared by weapons and equipment.
- `skills`: skill data, used by weapons.
- `usable`: use-effect data, used by consumables.

Physical sources:

- `items/weapons.csv`: common fields + `stats` + `skills`.
- `items/equipment.csv`: common fields + `stats`.
- `items/usable.csv`: common fields + `usable`.

## Fragment Model

A fragment is a named composable data block with its own field schema. Fragment fields can use the same Field system as normal fields, including structs, arrays, assets, text, and nested layouts.

Fragments are optional unless source or Meta rules make them required. A record may contain multiple fragments.

## Validation Rules

- Fragment names must resolve to declared fragment definitions.
- Common fields must be present in every source row.
- Fragment fields are validated only for records/sources that declare the fragment.
- Fragment field names should be namespaced to avoid collisions with common fields.
- Base source composition rejects duplicate primary-index or unique-index values across fragment sources.
- Override sources resolve duplicate primary-index keys by source priority/order.
- Final logical-table unique-index validation runs after fragment source normalization.

## Generated Query Semantics

Generated code should expose fragment-aware access patterns:

- common fields are directly available on every record;
- `has_fragment(fragment_type)` or typed helpers such as `has_stats()` check ownership;
- `get_fragment(fragment_type)` or typed helpers such as `stats()` return optional fragment data;
- missing fragments are safe and should not require downcasting or inheritance-style runtime type checks.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No final generated API naming for target languages.
- No inheritance model or class hierarchy generation.
- No final row-level fragment discriminator syntax beyond design drafts.

## Acceptance Criteria

- `Documentation/glossary.md` includes terms for polymorphic record, polymorphic collection table, common field, fragment, fragment type, fragment source, and fragment query.
- `Documentation/meta-design.md` describes fragment-based polymorphic records, item-table examples, source table rules, TOML/XML drafts, generated query semantics, and validation rules.
- `Documentation/README.md` links to this feature spec.
- `AGENTS.md` links to this feature spec.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
