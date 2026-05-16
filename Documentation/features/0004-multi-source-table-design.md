# Feature Spec 0004: Multi-Source Table Design

## Summary

Support multiple physical sources for one logical Meta table. Multi-source tables solve collaboration conflicts through physical file splitting and support parallel version development through row/field-level override layers.

## Documentation Scope

This feature is a documentation-only design step. It updates the project glossary and Meta design so later implementation can support base `sources`, layered `override_sources`, deterministic source priority, and multi-source validation.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/features/0003-meta-syntax-index-foreign-key-design.md`

## Requirements

- Each table may declare one source or multiple sources.
- `source` remains valid shorthand for a single source.
- `sources` declares base physical files or sheets for one logical table.
- `override_sources` declares layered override files or sheets applied after base sources.
- A table may use both `sources` and `override_sources` at the same time.
- The design should not use one table-level `source_merge` enum that forces split and override to be mutually exclusive.
- Base sources are split-merged without override semantics.
- Override sources apply higher-priority or later source rows/fields over the base logical table.
- Base source split-merge must reject duplicate values across all unique indexes.
- CollectionTable override sources require `primary_index`.
- CollectionTable override granularity is one logical record selected by the primary-index key.
- ObjectTable override granularity is one field or field path.
- The primary index used for override identity must be unique and may be single-field or composite.
- Final table-level validation runs after base source merge and override-source resolution.

## Base Sources

Base `sources` are intended for collaboration. Authors can split a large logical table into multiple physical files to reduce source-control conflicts.

Validation rules:

- each base source uses the same table Meta;
- each base source is normalized independently;
- values for every unique index must be unique across all base sources;
- duplicate unique-index values across base sources are conflicts;
- non-unique indexes may contain duplicate keys;
- ObjectTable base sources must not assign the same field path from multiple sources.

## Override Sources

`override_sources` are intended for version layering and branch-specific changes. They are applied after the base logical table is produced from `sources`.

CollectionTable rules:

- override sources require `primary_index`;
- `primary_index` points to a unique index and is not a primary key;
- the primary index may be composite;
- records with the same primary-index key as base records are override events;
- records with the same primary-index key across override sources are resolved by priority/order;
- duplicate primary-index keys within one override source are errors;
- override rows that do not match base rows should default to error until an explicit insert policy is designed;
- final unique-index validation runs after override resolution.

ObjectTable rules:

- each logical unit is a field path;
- duplicate field paths across override sources are resolved by priority/order;
- duplicate field paths within one override source are errors;
- override field paths that do not exist in base data should default to error until an explicit insert policy is designed;
- required fields are validated after override resolution.

## Source Priority

Override resolution must be deterministic.

Recommended rule:

- explicit override source priority/layer wins over declaration order;
- if no explicit priority exists, later override source declarations override earlier ones;
- diagnostics should preserve both winning and overridden source locations.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No final CLI flag design for selecting source layers.
- No final branch/version packaging workflow.

## Acceptance Criteria

- `Documentation/glossary.md` includes terms for multi-source, base source, override source, source composition, source priority, source conflict, row-level override, and field-level override.
- `Documentation/meta-design.md` describes multi-source tables, base sources, override sources, source ordering, and validation order.
- `Documentation/README.md` links to this feature spec.
- `AGENTS.md` links to this feature spec.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
