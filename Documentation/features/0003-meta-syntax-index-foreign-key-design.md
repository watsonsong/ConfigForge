# Feature Spec 0003: Meta Syntax, Index, and Foreign Key Design

## Summary

Extend the Meta design with a canonical in-memory model, multiple authored file syntaxes, XML Schema support, index definitions, generated query semantics, and foreign-key constraints.

## Documentation Scope

This feature is a documentation-only design step. It updates the project glossary and Meta design so later implementation can support TOML/XML Meta authoring, XML IDE validation, index-driven runtime queries, and foreign-key validation.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/features/0001-project-foundation.md`
- `Documentation/features/0002-meta-field-layout-design.md`

## Requirements

- Meta must have one canonical in-memory object model.
- Meta files may use different authored syntaxes, initially TOML and XML.
- XML Meta must have an XML Schema / XSD so XML-aware IDEs can validate Meta syntax while editing.
- TOML and XML must parse into equivalent in-memory Meta semantics.
- Every CollectionTable must declare at least one runtime access strategy: one or more indexes, or explicit source-ordered array export.
- A CollectionTable may set `preserve_source_order = true` to expose records as an array in final source composition order.
- A CollectionTable may omit indexes only when `preserve_source_order = true` and no feature such as foreign-key targeting or row-level override identity requires an index.
- A CollectionTable may declare one primary index as an alias to one declared unique index.
- Primary index exists only to let foreign keys omit an explicit target index name.
- Primary index may reference a composite unique index and is not restricted to a single field.
- Indexes may be unique or multi-value.
- Indexes may be single-field or composite-field.
- Indexes may be unordered or ordered.
- Ordered indexes may support range queries.
- Single-field integer indexes may declare dense layout, requiring contiguous keys with no holes.
- Runtime storage and generated query code should reflect index semantics.
- ObjectTable and CollectionTable fields may declare foreign keys to another table.
- A foreign key without an explicit index name targets the referenced table's primary index.
- A foreign key with an explicit index name may target any unique index.
- Non-unique indexes are not valid foreign-key targets.
- Composite foreign keys must provide multiple key parts compatible with the target unique index.

## Meta Representation Layers

Meta design distinguishes:

- `MetaModel`: canonical in-memory object graph used by validators, exporters, and code generators.
- `MetaDocument`: physical authored file representation.
- `MetaSyntax`: syntax discriminator, initially `toml` and `xml`.

All supported syntaxes must map to the same semantic model.

## Meta Syntax Support

Initial syntax support targets:

- TOML: concise hand-authored format.
- XML: structured authoring format for schema-assisted editing.

XML support should include an XSD that validates:

- table kind values;
- field type values;
- layout values;
- collection access strategy fields such as `preserve_source_order`;
- index cardinality and ordering values;
- primary index declaration;
- foreign key declaration;
- struct and array nesting rules.

## Index Design

Most collection tables require indexes because runtime consumers need predictable query entry points. Some collection tables instead need source-ordered array access, such as random-name lists or authored-order lists. These tables should opt in with `preserve_source_order = true` rather than declaring meaningless synthetic indexes.

ConfigForge defines `primary_index`, not a canonical primary key. `primary_index` is a table-level reference to one declared unique index. The referenced index still defines whether it is unordered or ordered, single-field or composite, and whether it has storage hints such as dense array.

Index dimensions:

- Cardinality:
  - `unique`: one key maps to at most one object.
  - `multi`: one key maps to multiple objects.
- Key shape:
  - single-field index;
  - composite index.
- Ordering:
  - `unordered`: exact lookup only.
  - `ordered`: exact lookup plus range-query support.
- Storage hint:
  - hash table;
  - sorted array;
  - dense array for dense integer indexes.

Generated query code should reflect index shape. For example, a unique index returns one optional object, while a multi-value index returns an iterator or collection.

## Source-Ordered Collection Access

`preserve_source_order = true` is a table-level opt-in for CollectionTables whose runtime shape is an ordered array. It is not an index and does not provide key lookup or foreign-key targetability.

Example:

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
```

Rules:

- The option defaults to `false`.
- A table may use both indexes and `preserve_source_order = true`.
- A table with no indexes is valid only when `preserve_source_order = true`.
- Foreign keys still require a unique target index or target table `primary_index`.
- Row-level override still requires `primary_index` because source order alone cannot identify replacement targets.
- Export context filtering removes records without reordering remaining records.

## Dense Integer Index

A dense integer index is a single-field unique integer index with contiguous keys and no holes. It allows runtime storage as a dense array for direct integer lookup.

Validation must ensure:

- the indexed field is integer;
- only one field participates;
- keys are unique;
- keys are contiguous according to the declared dense range policy;
- missing keys and duplicate keys are errors.

## Foreign Key Design

Foreign keys can be declared on fields in both object tables and collection tables.

Foreign key target rules:

- If no target index is specified, use the target table's primary index.
- If a target index is specified, it must exist and be unique.
- A non-unique index cannot be used as a foreign-key target.
- Composite foreign keys must provide values compatible with the target composite unique index, including matching key-part count and deterministic field order or explicit mapping.

Validation must ensure every non-empty foreign-key value resolves to one target object and report diagnostics with source table, source field path, source location, target table, target index, and missing key value.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No actual XML parser implementation in this step.
- No generated XML Schema file in this step.
- No generated runtime query code in this step.
- No final target-language API design for generated query code.

## Acceptance Criteria

- `Documentation/glossary.md` includes terms for Meta model/document/syntax, TOML Meta, XML Meta, XML Schema, primary index, unique/multi index, ordered/unordered index, dense integer index, index storage, source-ordered collection access, and foreign key.
- `Documentation/meta-design.md` describes in-memory Meta representation, TOML/XML syntax support, XML Schema expectations, index design, source-ordered collection access, dense integer indexes, and foreign-key constraints.
- `Documentation/README.md` links to this feature spec.
- `AGENTS.md` links to this feature spec.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
