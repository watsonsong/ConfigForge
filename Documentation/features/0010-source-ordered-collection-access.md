# Feature Spec 0010: Source-Ordered Collection Access

## Summary

Add an explicit CollectionTable access strategy for tables that should be exported as a source-ordered array without requiring an index. This supports data such as random-name lists, ordered tutorial steps, credit lists, or other authored-order collections where keyed lookup is unnecessary.

## Documentation Scope

This feature is a documentation-only design step. It updates glossary, Meta design, and index feature documentation so CollectionTables can choose indexed lookup, source-ordered array access, or both.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/data-design.md`
- `Documentation/features/0003-meta-syntax-index-foreign-key-design.md`
- `Documentation/features/0009-config-forge-pipeline-design.md`

## Requirements

- CollectionTables should have at least one runtime access strategy.
- Indexed lookup remains the default/recommended strategy for most CollectionTables.
- A CollectionTable may opt in to ordered array access with `preserve_source_order = true`.
- `preserve_source_order` defaults to `false`.
- A CollectionTable may declare indexes and also set `preserve_source_order = true`.
- A CollectionTable may omit indexes only when `preserve_source_order = true` and no other feature requires an index.
- `preserve_source_order` is not an index and does not imply uniqueness or key lookup.
- Foreign keys cannot target source-order access; they still require a unique index or `primary_index` on the target table.
- Row-level override still requires `primary_index` because source order alone cannot identify replacement rows.
- Export filtering should remove rows without reordering remaining rows.

## Motivation

Some collection tables are not lookup maps. For example:

- random name lists;
- ordered tutorial steps;
- ordered credits;
- flavor text pools;
- random phrase lists;
- designer-authored display sequences.

For these tables, forcing an artificial index creates meaningless metadata and misleading generated APIs. The table should instead declare that the runtime needs the records as an ordered array.

## Meta Draft

Ordered-only collection:

```toml
[[tables]]
name = "random_names"
kind = "collection"
source = "random_names.csv"
preserve_source_order = true

[[tables.fields]]
name = "name"
type = "text"
required = true

[[tables.fields]]
name = "weight"
type = "int"
```

Collection with both indexed lookup and source-order array access:

```toml
[[tables]]
name = "tutorial_steps"
kind = "collection"
source = "tutorial_steps.csv"
primary_index = "by_id"
preserve_source_order = true

[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
order = "unordered"
```

XML draft:

```xml
<table name="random_names" kind="collection" source="random_names.csv" preserveSourceOrder="true">
  <field name="name" type="text" required="true" />
  <field name="weight" type="int" />
</table>
```

## Ordering Rules

Recommended ordering semantics:

1. Provider row order is stable within one source.
2. Base `sources` compose in declaration order unless a later explicit source ordering policy exists.
3. Rows inside one source preserve source row order after headers/comments are removed.
4. Override rows replacing existing rows keep the replaced row's array position by default.
5. Override insertion remains a future policy and should default to error until explicitly designed.
6. Export context filtering removes rows but preserves relative order.

## Generated Runtime Access

Recommended generated access shape:

- indexed-only table: lookup APIs based on indexes;
- source-ordered-only table: array/list APIs such as `all()`, `records()`, `len()`, optional configured random helper;
- table with both: both lookup APIs and ordered array/list APIs.

Random helpers should be optional because random selection policy may depend on runtime requirements such as weight, deterministic seed, or platform random source.

## Validation Rules

- `preserve_source_order` is valid only on CollectionTable.
- A CollectionTable must declare at least one access strategy: indexes or `preserve_source_order = true`.
- If a table has foreign-key dependents, it must expose a unique index target; source-order access is not sufficient.
- If a table uses `override_sources`, row-level override requires `primary_index`.
- If a table uses multiple base sources and `preserve_source_order = true`, declaration order should be treated as deterministic source order unless a later source priority/order policy overrides it.
- Diagnostics should explain why a no-index collection is allowed or rejected.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No final generated API design for random helpers.
- No override insert policy.
- No separate sort expression language; this feature preserves source composition order only.

## Acceptance Criteria

- `Documentation/glossary.md` includes terms for collection access strategy, source-ordered collection, preserve source order, and ordered array export.
- `Documentation/meta-design.md` allows CollectionTables to omit indexes when `preserve_source_order = true` and describes validation rules.
- `Documentation/features/0003-meta-syntax-index-foreign-key-design.md` is updated so it no longer states that every CollectionTable must always declare an index.
- `Documentation/README.md` links to this feature spec.
- `AGENTS.md` links to this feature spec.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
