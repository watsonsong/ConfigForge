# Feature Spec 0002: Meta Field and Sheet Layout Design

## Summary

Extend the Meta documentation model so both ObjectTable and CollectionTable are based on Field definitions. Define primitive, builtin value types, text, asset, struct, array, struct-array, nested layout, and Sheet mapping concepts without implementing code logic.

## Documentation Scope

This feature is a documentation-only design step. It extends `Documentation/glossary.md` and `Documentation/meta-design.md` so later implementation work can follow a stable vocabulary and mapping model.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/features/0001-project-foundation.md`

## Requirements

- ObjectTable and CollectionTable must both be described through fields.
- A table's physical Sheet mapping must be separate from its logical Field model.
- Field types must include primitive values, builtin value types, text/i18n values, asset references, structs, arrays, and struct arrays.
- Header cells may map to field paths.
- Structs and arrays may be represented by expanded layouts or JSON cell layouts.
- Nested validation rules must be recursive and report field paths.
- Asset fields must declare validation expectations for future export-time resource existence checks.
- Text fields must be reserved for i18n-aware workflows.

## Field as Table Unit

Field is the common Meta unit for all table forms:

- An ObjectTable exports one constant object and can be treated as an anonymous top-level Struct.
- A CollectionTable exports many records, and each record can be treated as an anonymous top-level Struct.
- A Struct is a group of child Fields.
- An Array repeats one element type, which may be primitive or composite.

## Supported Field Types

The documented Meta type strings are:

- `int`: integer number.
- `float`: floating-point number.
- `bool`: boolean value.
- `string`: plain non-localized text.
- `text`: i18n-aware localized text.
- `asset`: game engine resource reference.
- `color_rgb` / `color_rgba`: builtin color values authorable as hex, rgb/rgba function syntax, or tuples.
- `vector2` / `vector3` / `vector4` and `vector2_int` / `vector3_int`: builtin vector values authorable as tuples or arrays.
- `rotation`: builtin Euler rotation value.
- `datetime` / `date` / `time` / `duration`: builtin time-related values, with datetime supporting field-level pattern configuration.
- `int_range` / `float_range`: builtin inclusive numeric range values.
- `rect` / `bounds3`: builtin spatial bounds values for UI/screen regions and 3D volumes.
- `struct`: composite value containing child Fields.
- `array`: repeated values with an element type.

## Sheet Header Mapping

Data is modeled as Sheets. A Sheet normally uses the first row as a header row. Headers map physical cells to logical Fields or FieldPaths; they do not define the field types by themselves.

Examples of FieldPath syntax:

- `reward.item_id`
- `reward.count`
- `rewards[0].item_id`
- `rewards[0].count`
- `conditions[2].params.min_level`

## Struct Layouts

Struct values may be represented in multiple physical forms:

- Expanded columns: `reward.item_id`, `reward.count`.
- Expanded rows: useful for object-style field/value rows or advanced multi-row layouts.
- JSON cell: `{ "item_id": 2001, "count": 1 }`.

Expanded columns are usually easier for designers to edit. JSON cells are compact and useful for deeply nested values.

## Array Layouts

Array values may be represented as:

- Expanded columns: `reward_ids[0]`, `reward_ids[1]`, `reward_ids[2]`.
- Expanded rows: repeated child rows associated with a parent record key.
- JSON cell: `[1001, 1002, 1003]`.

Expanded column arrays may imply fixed slots. Expanded row arrays are better for variable length. JSON cells are compact but less spreadsheet-friendly.

## Struct Array and Nested Layouts

Struct arrays combine repeated values with child Field schemas.

Expanded column example:

```text
rewards[0].item_id | rewards[0].count | rewards[1].item_id | rewards[1].count
```

JSON cell example:

```json
[
  { "item_id": 2001, "count": 1 },
  { "item_id": 2002, "count": 3 }
]
```

Nested struct arrays are supported at the design level through FieldPath, for example:

```text
rewards[0].conditions[0].type
rewards[0].conditions[0].params.min_level
```

## Asset Validation

Asset fields are resource references, not plain strings. During future validation/export, ConfigForge should check that each referenced asset exists according to configured asset roots, asset kind, or provider-specific lookup rules.

Asset validation errors should report:

- table name;
- row/column/cell or object field source location;
- full field path;
- raw asset reference;
- expected asset kind or search root when declared.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No final engine-specific asset validation backend.
- No final localization pipeline.
- No final TOML schema commitment for nested Meta parsing.

## Acceptance Criteria

- `Documentation/glossary.md` includes terms for asset, text, builtin value types, struct, array, field path, header mapping, JSON cell, and expanded layout.
- `Documentation/meta-design.md` describes the Field model, Field type system, builtin value types, Sheet header mapping, layout strategies, asset validation, and nested validation rules.
- `Documentation/README.md` links to this feature spec.
- `AGENTS.md` links to this feature spec.
- `Documentation/features/0001-project-foundation.md` briefly references that advanced Field/Layout design continues in this feature.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
