# Meta Design

Meta is the source of truth for describing designer-authored game data. It defines table identity, table kind, fields, field types, indexes, constraints, and export-facing structure.

This document records design intent for the Meta system. It does not require the current code to already support every concept described here.

## Design Goals

- Describe data shape explicitly before reading source data.
- Support both constant-object-style configuration and collection-style configuration.
- Keep physical sheet layout separate from logical export shape.
- Make validation deterministic and actionable.
- Provide a stable vocabulary for future code generation and runtime data export.
- Keep Meta loading compatible with future parallel parsing, deterministic merging, and plugin-registered builtin types.

## Core Concepts

- `Meta`: schema and definition layer for exported data.
- `MetaModel`: canonical in-memory object model produced after parsing Meta documents.
- `MetaDocument`: physical authored file representation of Meta, initially TOML or XML.
- `MetaSyntax`: file syntax used by a Meta document. Initial supported syntaxes are TOML and XML.
- `Table`: one exportable data unit described by Meta.
- `TableKind`: discriminator for how a table source should be interpreted.
- `ObjectTable`: a table whose source rows describe fields and whose logical result is one object.
- `CollectionTable`: a table whose source rows describe records and whose logical result is a collection.
- `PolymorphicRecord`: one logical collection-table record composed from common fields and optional fragments.
- `Fragment`: composable data block attached to a record for variant-specific behavior or data.
- `Field`: the basic logical unit of Meta. Both object tables and collection tables are composed from fields.
- `FieldType`: the logical value type of a field, independent from physical sheet layout.
- `Enum`: a named finite set of allowed values declared in Meta and validated before export.
- `Struct`: a composite field type that groups multiple child fields.
- `NamedStruct`: a reusable struct definition that can be referenced by fields, fragments, or other named structs.
- `Array`: a repeated field type whose element may be primitive, struct, or another supported field shape.
- `Asset`: a resource reference that should be validated against configured asset roots or providers.
- `Text`: localized text that should be exported with i18n-aware semantics.
- `Builtin Value Type`: a common composite value type provided by ConfigForge without requiring a user-defined struct, such as `color_rgba`, `vector3`, `rotation`, or `datetime`.
- `SheetMapping`: table-level rule describing how physical sheet cells map to logical fields.
- `FieldLayout`: field-level rule describing whether a value is read from one cell, a JSON cell, expanded columns, or expanded rows.
- `FieldPath`: stable path used to address nested fields during mapping, validation, diagnostics, and future code generation.
- `DataSet`: typed in-memory logical data parsed from source sheets according to Meta.
- `ExportDataset`: target/context-specific data view derived from `DataSet` for runtime data export.
- `ExportContext`: target, tags, and variables for one export run.
- `ExportFilter`: rule that includes or excludes records, fields, fragments, or special exporter entries based on export context and data values.
- `ToolOnlyField`: field that participates in parsing, validation, and filtering but is omitted from regular runtime data by default.
- `Index`: a lookup-oriented field or field group that drives validation, runtime storage, and generated query code.
- `PrimaryIndex`: the default unique lookup index of a collection table.
- `ForeignKey`: field-level reference to another table through the target table's primary index or another named unique index.
- `Constraint`: a validation rule attached to table, index, or field definitions.
- `Validator`: executable correctness check that validates Meta, source layout, field values, records, collections, cross-table relationships, or export targets.

See `Documentation/glossary.md` for the canonical terminology table.

See `Documentation/data-design.md` for Data pipeline design, including normalized source sheets, typed datasets, runtime data exporters, special exporters, context tags, export filters, and tool-only fields.

See `Documentation/validator-design.md` for the dedicated Validator system design, including scopes, phases, code structure, configuration drafts, diagnostics, and recommended checks.

See `Documentation/implementation-requirements.md` for implementation-level requirements such as plugin registries, parallel Meta loading, async I/O, and structured diagnostics.

## Meta Representation Layers

Meta has two representation layers:

1. **In-memory Meta model**: the canonical object graph used by validation, export, and code generation.
2. **Meta documents**: authored files that describe the same model using a supported syntax.

All supported file syntaxes should parse into the same in-memory model. Validation and export should depend on the in-memory model rather than syntax-specific structures.

Implementation should allow multiple Meta documents to be read and parsed in parallel, then merged through a deterministic global linking phase. File discovery order, diagnostic ordering, duplicate-definition checks, and cross-file reference resolution must remain stable regardless of worker scheduling.

Recommended conceptual split:

```rust
enum MetaSyntax {
    Toml,
    Xml,
}

struct MetaDocument {
    syntax: MetaSyntax,
    source_path: PathBuf,
}

struct MetaModel {
    tables: Vec<TableMeta>,
}
```

Future plugin-provided builtin value types should register through `BuiltinTypeRegistry` before Meta linking validates field type names. Plugin loading failures, builtin type conflicts, and unknown plugin-provided type references should be reported as structured diagnostics.

## Meta File Syntax

ConfigForge should support multiple Meta authoring syntaxes while preserving one semantic model.

### TOML Meta

TOML is the initial human-friendly syntax for simple hand-authored Meta definitions. It should be concise, readable in code review, and easy to edit without specialized tooling.

### XML Meta

XML is the initial structured syntax for schema-assisted authoring. XML Meta should be accompanied by an XML Schema Definition (XSD) so XML-aware IDEs can validate element names, attributes, required fields, nesting, and enum-like values while designers or engineers edit Meta files.

The XML schema should cover at least:

- table kind values: `object` and `collection`;
- field type values: `int`, `float`, `bool`, `string`, `text`, `asset`, builtin value types, `enum`, `struct`, `array`;
- enum definitions and enum variant declarations;
- named struct definitions and struct references;
- struct-reference cycle diagnostics;
- layout values: `single_cell`, `json_cell`, `expanded_columns`, `expanded_rows`;
- index cardinality and ordering values;
- collection access strategy fields such as `preserve_source_order`;
- primary index declaration;
- foreign key target declaration;
- required child elements for struct and array fields.

### Syntax Equivalence

TOML and XML documents are different physical syntaxes for the same `MetaModel`. A table, field, index, or foreign key should have the same meaning after parsing regardless of whether it came from TOML or XML.

## Field Model

A `Field` is the basic logical unit in Meta. Table kinds describe how source sheets produce top-level values, but the values themselves are always defined through fields.

Conceptually:

- An `ObjectTable` exports one top-level object, which can be treated as an anonymous top-level `Struct`.
- A `CollectionTable` exports many records, and each record can be treated as an anonymous top-level `Struct`.
- A `Struct` contains multiple named fields.
- An `Array` repeats one element type. The element type may be primitive, struct, or another supported field shape.
- A nested value is addressed through a `FieldPath`, for example `reward.item_id`, `rewards[0].count`, or `conditions[2].params.min_level`.

The following Rust-like sketch is a design aid only; it does not require the current code to use this exact shape:

```rust
enum FieldType {
    Primitive(PrimitiveFieldType),
    Builtin(BuiltinValueType),
    Enum(EnumRef),
    Struct(StructMeta),
    StructRef(StructRef),
    Array(ArrayMeta),
}

enum PrimitiveFieldType {
    Int,
    Float,
    Bool,
    String,
    Text,
    Asset,
}

enum BuiltinValueType {
    ColorRgb,
    ColorRgba,
    Vector2,
    Vector3,
    Vector4,
    Vector2Int,
    Vector3Int,
    Rotation,
    DateTime,
    Date,
    Time,
    Duration,
    IntRange,
    FloatRange,
    Rect,
    Bounds3,
}

struct FieldMeta {
    name: String,
    field_type: FieldType,
    required: bool,
    layout: Option<FieldLayout>,
    export: bool,
    tool_only: bool,
    filter_field: bool,
}
```

Named definitions such as `EnumDef` and `NamedStruct` belong to the same `MetaModel` as tables. They are reusable logical type definitions, not target-language code-generation declarations.

Field definitions may also include export-facing properties. These properties do not change the logical field type; they describe whether a field appears in regular runtime data or only participates in tooling.

Recommended properties:

- `export`: whether the field is emitted to regular runtime data. Default is `true`.
- `tool_only`: field is parsed, validated, and available to filters, but omitted from regular runtime data by default.
- `filter_field`: field may be referenced by export filters.
- `validator_only`: field is intended only for validation rules and omitted from regular runtime data.
- `special_exports`: optional policy controlling whether special exporters such as POT or asset manifest can consume the field.

Example:

```toml
[[tables.fields]]
name = "test"
type = "bool"
required = true
tool_only = true
filter_field = true
export = false
```

This field can drive an export filter such as “exclude test rows in shipping builds” while staying out of the final runtime data files.

## Field Type System

`FieldType` describes logical value shape. It is intentionally separate from source sheet layout: the same logical struct can be represented by expanded columns, expanded rows, or a JSON cell.

### Primitive Types

| Meta type | Meaning | Validation Notes |
| --- | --- | --- |
| `int` | Integer number | Must parse as an integer. |
| `float` | Floating-point number | Must parse as a number. |
| `bool` | Boolean value | Accepted textual forms should be standardized by implementation. |
| `string` | Plain text | Exported directly without localization behavior. |
| `text` | Localized text | Reserved for i18n-aware export, such as localization keys, text IDs, or locale maps. |
| `asset` | Engine resource reference | Must validate referenced resource existence before export succeeds. |

### Builtin Value Types

Some common game-development value shapes are used often enough that requiring a custom named struct every time would be noisy. ConfigForge should provide **Builtin Value Types** for these cases. They are logical Meta field types, can be written compactly in one cell by default, and can still be mapped to target-specific runtime types through export configuration.

Builtin value types are not engine-specific. For example, `color_rgba` is a logical four-channel color. Export configuration may map it to `FLinearColor`, `UnityEngine.Color`, a generated struct, a tuple, or a plain object.

Recommended builtin types:

| Meta type | Logical shape | Single-cell examples | Notes |
| --- | --- | --- | --- |
| `color_rgb` | RGB color | `#FF8800`, `rgb(255,136,0)`, `1,0.533,0` | Three channels. Hex and rgb function syntax should be supported. |
| `color_rgba` | RGBA color | `#FF8800CC`, `rgba(255,136,0,0.8)`, `1,0.533,0,0.8` | Four channels. Alpha defaults only when field policy explicitly allows it. |
| `vector2` | 2D float vector | `1,2`, `(1, 2)`, `[1,2]` | Common for UI positions, 2D coordinates, offsets. |
| `vector3` | 3D float vector | `1,2,3`, `(1, 2, 3)`, `[1,2,3]` | Common for positions, scale, direction. |
| `vector4` | 4D float vector | `1,2,3,4`, `(1, 2, 3, 4)`, `[1,2,3,4]` | Common for shader params and packed values. |
| `vector2_int` | 2D integer vector | `1,2`, `(1, 2)`, `[1,2]` | Common for grid/tile coordinates. |
| `vector3_int` | 3D integer vector | `1,2,3`, `(1, 2, 3)`, `[1,2,3]` | Common for voxel/grid coordinates. |
| `rotation` | Euler rotation | `0,90,0`, `(pitch=0,yaw=90,roll=0)` | Logical Euler rotation; unit/order must be explicit or defaulted consistently. |
| `datetime` | Date-time instant or local date-time | `2026-05-16T12:30:00Z`, `2026-05-16 12:30:00` | Supports field-level pattern configuration. |
| `date` | Calendar date | `2026-05-16` | Useful for events and releases when time-of-day is irrelevant. |
| `time` | Time of day | `12:30:00` | Useful for daily reset/opening windows. |
| `duration` | Time duration | `10s`, `5m`, `2h`, `1d`, `00:05:30` | Useful for cooldowns, timers, buffs. |
| `int_range` | Integer inclusive range | `1..10`, `1-10`, `[1,10]` | Useful for level ranges, random integer ranges. |
| `float_range` | Float inclusive range | `0.1..0.5`, `[0.1,0.5]` | Useful for random scalar ranges. |
| `rect` | 2D rectangle | `0,0,100,50`, `{x=0,y=0,w=100,h=50}` | Useful for UI/screen regions. |
| `bounds3` | 3D bounds | `0,0,0;10,10,10` | Useful for spatial volumes; authoring convention should be project-standardized. |

Recommended single-cell parsing policy:

- Builtin types default to `SingleCell` layout because designer convenience is the main goal.
- JSON array syntax such as `[1,2,3]` may be accepted for vector/range values if the field layout allows JSON-like parsing.
- Expanded columns remain valid when a table wants spreadsheet-visible components, for example `position.x`, `position.y`, `position.z`.
- Raw text should be preserved in diagnostics even after parsing to typed builtin values.

#### Color Parsing

Color fields should support common authoring forms:

- `#RRGGBB` for `color_rgb`;
- `#RRGGBBAA` for `color_rgba`;
- `#RGB` / `#RGBA` may be accepted as shorthand if the project enables shorthand parsing;
- `rgb(255, 136, 0)`;
- `rgba(255, 136, 0, 0.8)`;
- comma tuple forms such as `255,136,0` or `1,0.533,0` depending on channel policy.

Recommended field options:

```toml
[[tables.fields]]
name = "tint"
type = "color_rgba"
color_channel = "normalized" # normalized | byte | auto
color_space = "srgb"         # srgb | linear, logical metadata for exporters
```

Validation rules:

- channel count must match type unless an explicit alpha default policy is configured;
- byte channels must be in `[0, 255]`;
- normalized channels must be in `[0.0, 1.0]`;
- hex strings must have valid length and characters;
- diagnostics should mention accepted forms.

#### Vector Parsing

Vector fields should support compact tuple forms:

```toml
[[tables.fields]]
name = "spawn_position"
type = "vector3"
vector_components = "x,y,z"
```

Accepted single-cell examples:

```text
1,2,3
(1, 2, 3)
[1,2,3]
x=1,y=2,z=3
```

Validation rules:

- component count must match vector dimension;
- all components must parse as floats;
- optional min/max component validators may be applied;
- named components must not be duplicated.

#### Rotation Parsing

`rotation` represents Euler angles, not a quaternion. This makes it designer-friendly while keeping target conversion configurable.

Recommended field options:

```toml
[[tables.fields]]
name = "spawn_rotation"
type = "rotation"
rotation_unit = "degrees" # degrees | radians
rotation_order = "xyz"    # xyz, xzy, yxz, yzx, zxy, zyx
```

Accepted single-cell examples:

```text
0,90,0
(0, 90, 0)
pitch=0,yaw=90,roll=0
x=0,y=90,z=0
```

Validation rules:

- exactly three components are required;
- unit defaults should be project-wide or field-level, with `degrees` recommended for designers;
- order defaults should be explicit in documentation/config, with `xyz` as the recommended neutral default;
- target exporters may map `rotation` to engine-specific Euler, quaternion, or rotator types.

#### DateTime Parsing and Pattern

`datetime` supports a default ISO-8601-like form and field-level pattern configuration.

Recommended defaults:

- Prefer ISO-8601 with timezone when representing an instant: `2026-05-16T12:30:00Z`.
- Allow local date-time only when the field or target config defines timezone policy.
- Use `date` when only a calendar date is needed.
- Use `time` when only time-of-day is needed.

TOML draft:

```toml
[[tables.fields]]
name = "open_at"
type = "datetime"
datetime_pattern = "%Y-%m-%d %H:%M:%S"
timezone = "Asia/Shanghai"
```

Alternative nested form:

```toml
[[tables.fields]]
name = "open_at"
type = "datetime"

[tables.fields.datetime]
pattern = "%Y-%m-%d %H:%M:%S"
timezone = "Asia/Shanghai"
```

Pattern syntax should be standardized by implementation. A Rust implementation may use `chrono`-style pattern tokens, but Meta docs should treat the pattern as ConfigForge's declared time pattern so future implementations can validate it consistently.

Validation rules:

- the pattern itself must be valid;
- source values must parse according to the pattern or default ISO policy;
- timezone policy must be explicit when source values omit timezone but the target needs an instant;
- diagnostics should include raw value, pattern, timezone, and field path.

#### Additional Recommended Builtins

The initial builtin set should prioritize types that are common, compact in one cell, and have clear validation rules. In addition to colors, vectors, rotations, and date/time values, these are useful candidates:

- `duration`: cooldowns, timers, buff durations, schedule offsets;
- `date`: event start/end dates without time-of-day;
- `time`: daily reset time or opening time;
- `int_range` / `float_range`: level ranges, random roll ranges, spawn ranges;
- `vector2_int` / `vector3_int`: grid coordinates, tile positions, cell coordinates;
- `rect`: UI rectangles or 2D bounds, shape `{x,y,width,height}`;
- `bounds3`: 3D bounds, shape `{center,size}` or `{min,max}`; exact authoring convention should be standardized per project/target.

Types such as gameplay tags, localization keys, item references, or currency references are valuable but are better modeled as `string`/`enum`/foreign-key/domain-specific validators rather than builtin value types.

### Enum Types

An `enum` is a finite set of symbolic values. It should be declared once in Meta and referenced by fields. Enum support belongs in Meta because it affects authoring validation, source parsing, foreign-key-like consistency checks, generated data shape, and code generation.

Recommended enum semantics:

- enum names are stable Meta identifiers, such as `ItemRarity` or `ElementType`;
- enum variants are stable symbolic identifiers, such as `common`, `rare`, or `fire`;
- variants may optionally declare numeric or string codes used in exported data;
- the authored cell value should match one declared variant name or declared alias/code according to a strict policy;
- enum values are validated before export, independent of target language;
- target-language enum class names or engine enum types are not part of Meta and should be configured by export type mapping.

TOML draft:

```toml
[[enums]]
name = "ItemRarity"

[[enums.variants]]
name = "common"
code = 1

[[enums.variants]]
name = "rare"
code = 2

[[enums.variants]]
name = "legendary"
code = 5

[[tables.fields]]
name = "rarity"
type = "enum"
enum = "ItemRarity"
required = true
```

XML draft:

```xml
<enum name="ItemRarity">
  <variant name="common" code="1" />
  <variant name="rare" code="2" />
  <variant name="legendary" code="5" />
</enum>

<field name="rarity" type="enum" enum="ItemRarity" required="true" />
```

Enum validation should check:

- enum names are unique within one `MetaModel` namespace;
- variant names are unique within one enum;
- variant codes are unique when codes are declared;
- every enum field references an existing enum definition;
- every source value resolves to exactly one enum variant;
- diagnostics include table, row/cell or object field source location, field path, raw value, and enum name.

### Composite Types

- `struct`: a packed group of child fields. Each child is also a `Field`. A struct may be declared inline or reference a named struct.
- `named struct`: a reusable struct definition that can be referenced from table fields, fragment fields, arrays, or other named structs.
- `array`: repeated values with one declared element type.
- `array<struct>`: repeated composite values, commonly used for rewards, costs, conditions, effects, and nested game rules.

Nested composite values are allowed by design. For example, a record can contain `rewards`, where each reward is a struct and each reward contains `conditions`, where each condition contains another struct named `params`.

```json
{
  "id": 1001,
  "rewards": [
    {
      "item_id": 2001,
      "count": 3,
      "conditions": [
        {
          "type": "level",
          "params": {
            "min_level": 10
          }
        }
      ]
    }
  ]
}
```

### Named Structs

Inline structs are useful for one-off field shapes. Named structs are needed when the same domain shape appears in multiple tables, fragments, or nested fields. Examples include `Reward`, `Cost`, `StatBlock`, `Condition`, and `SpawnRule`. Common math/color/time shapes such as `color_rgba` and `vector3` should prefer builtin value types unless a project needs custom semantics.

Named struct support belongs in Meta because it describes logical data shape and validation rules. It should not contain target-language type names such as `FLinearColor`, `UnityEngine.Color`, or `Color`; those belong to export type mapping.

Recommended semantics:

- named structs live in a Meta-level namespace separate from table names and enum names unless a later namespace policy combines them;
- named struct names are stable logical identifiers;
- a field can define an inline `struct` or reference a named struct;
- an array element can reference a named struct;
- fragment fields can reference named structs;
- a named struct can contain fields whose types include primitives, enums, assets, arrays, inline structs, and other named-struct references;
- named struct references are expanded logically for validation and field-path resolution;
- diagnostics should preserve both the reference site and the named-struct definition site.

TOML draft:

```toml
[[structs]]
name = "Reward"

[[structs.fields]]
name = "item_id"
type = "int"
required = true

[[structs.fields]]
name = "count"
type = "int"
required = true

[[tables.fields]]
name = "reward"
type = "struct"
struct = "Reward"
layout = "expanded_columns"

[[tables.fields]]
name = "rewards"
type = "array"
layout = "json_cell"

[tables.fields.element]
type = "struct"
struct = "Reward"
```

XML draft:

```xml
<struct name="Reward">
  <field name="item_id" type="int" required="true" />
  <field name="count" type="int" required="true" />
</struct>

<field name="reward" type="struct" struct="Reward" layout="expanded_columns" />
<field name="rewards" type="array" layout="json_cell">
  <element type="struct" struct="Reward" />
</field>
```

### Struct Cycle Detection

Named struct nesting creates a directed dependency graph. If `A` contains a field of type `struct = "B"`, then `A -> B`. ConfigForge should reject cycles in this graph because current Meta structs are value-shaped, embedded data. A cycle would imply infinite value expansion and ambiguous layout/code generation.

Invalid examples:

```text
A -> A
A -> B -> A
Quest -> Condition -> Quest
```

Cycle detection rules:

- build dependencies from named-struct references inside named struct fields;
- include dependencies through array elements, such as `array<struct Reward>`;
- include references inside inline structs that are nested within named structs;
- table fields and fragment fields may reference named structs, but those references are graph roots rather than cycle edges unless the referenced named struct points back through named-struct definitions;
- cycles are errors by default;
- diagnostics should print the complete cycle path and source locations for each edge;
- recursive references may be reconsidered later only with an explicit indirection concept such as `ref<T>` or ID-based foreign-key references.

A topological order of named structs should exist after validation. Exporters can use that order when generating target code, while still applying target-specific type names from export configuration.

Target-specific enum and named-struct type mapping is intentionally documented separately in `Documentation/export-design.md`. Meta owns logical definitions and validation; export configuration owns language/engine representation.

## Sheet Header Mapping

Source data is normalized as sheets. A sheet normally uses the first row as a header row. Headers do not define field types by themselves; they map physical cells to fields or field paths declared in Meta.

For `CollectionTable`:

- Each data row maps to one record.
- Each header cell usually maps one column to a `Field` or `FieldPath`.
- Nested fields can be addressed with field paths such as `reward.item_id` or `rewards[0].count`.

For `ObjectTable`:

- The sheet may use field/value rows, where the `field` column contains a field path and the `value` column contains the raw value.
- The sheet may also use a header-driven object layout when declared by `SheetMapping`; in that case columns map to fields of one constant object rather than multiple records.
- The object itself is still one top-level struct-like value.

Recommended `FieldPath` syntax:

- Dot syntax addresses struct members: `reward.item_id`.
- Bracket syntax addresses array positions in expanded layouts: `rewards[0].count`.
- Paths can be nested: `conditions[2].params.min_level`.
- Field paths are for mapping and diagnostics; they do not require arrays to be fixed-size unless the layout declares fixed slots.

Default mapping errors should include:

- unknown headers or field paths;
- missing required fields;
- duplicate physical mappings for the same field path;
- malformed array indexes;
- reserved/comment columns that were not explicitly allowed by mapping rules.

## Multi-Source Tables

A table may read from one or more physical sources. Multi-source support solves two practical authoring problems:

1. **Physical split for collaboration**: one logical table is split into multiple physical files or sheets so different people can edit different files with fewer merge conflicts.
2. **Layered override for parallel versions**: multiple development versions or product branches can override records/fields at logical-unit granularity without editing the same physical file.

The logical output is still one table after source normalization, conflict checks, and optional overrides.

### Source Declaration Draft

Single-source tables may continue to use `source` as shorthand. Multi-source tables should use `sources` and/or `override_sources`.

To support extensible source backends, source declarations should describe both **what provider loads the data** and **where the provider should read from**. Meta should not assume every source is a local CSV file. Instead, source entries should normalize into the same logical sheet model before table validation.

Recommended source fields:

- `provider`: loader backend, such as `local_file`, `csv`, `xlsx`, `lark_sheet`, or `google_sheet`.
- `path`: local file path for local providers.
- `uri`: provider-specific locator for remote providers.
- `sheet`: sheet/tab name when the provider contains multiple sheets.
- `range`: optional sheet range selector.
- `format`: optional file format hint when `provider = "local_file"`.
- `auth_ref`: optional credential reference for remote providers. Secrets should not be embedded in Meta files.
- `layer` / `priority`: optional source ordering metadata for override sources.

The design should not use one table-level `source_merge` enum that forces the entire table to be either split or override. A table often needs both behaviors:

1. `sources` define the base logical table through split-style composition.
2. `override_sources` are applied on top of that base table as deterministic override layers.

This means a table can be physically split for collaboration and still support version/branch override layers.

```toml
[[tables]]
name = "items"
kind = "collection"
sources = [
  { provider = "csv", path = "items/base.csv", layer = "base" },
  { provider = "xlsx", path = "items/events.xlsx", sheet = "Items", layer = "base" },
]
override_sources = [
  { provider = "lark_sheet", uri = "lark://sheet/items_v2", sheet = "Items", range = "A1:K500", auth_ref = "lark-prod", layer = "v2", priority = 20 },
]
primary_index = "by_id"
```

Single source shorthand may remain concise:

```toml
source = { provider = "csv", path = "items.csv" }
```

If `source = "items.csv"` is supported as a compatibility shorthand, it should be interpreted as a local CSV source inferred from the file extension.

XML draft:

```xml
<table name="items" kind="collection" primaryIndex="by_id">
  <source provider="csv" path="items/base.csv" layer="base" />
  <source provider="xlsx" path="items/events.xlsx" sheet="Items" layer="base" />
  <overrideSource provider="lark_sheet" uri="lark://sheet/items_v2" sheet="Items" range="A1:K500" authRef="lark-prod" layer="v2" priority="20" />
</table>
```

### Source Provider Extensibility

Source provider details are input concerns, not table-shape concerns. A provider is responsible for loading physical data and normalizing it into the same sheet abstraction used by header mapping, field layout, validation, and source composition.

Initial provider targets:

- `csv`: local CSV file.
- `xlsx`: local Excel workbook, requiring sheet selection when ambiguous.
- `lark_sheet`: remote Lark/Feishu Sheet.
- `google_sheet`: remote Google Sheet.

Provider-specific fields should remain optional and namespaced by provider semantics. For example, `xlsx` may need `sheet`, remote sheets may need `uri`, `sheet`, `range`, and `auth_ref`.

Validation should check provider declarations before loading:

- provider name is known or supported by an installed plugin;
- required locator fields are present for that provider;
- local files exist when using local file providers;
- remote providers have an `auth_ref` when required;
- selected sheet/range exists after provider metadata is loaded;
- provider output can be normalized into rows, columns, headers, and cells.

Meta files should store credential references only. Tokens, cookies, service-account secrets, or other credentials must live outside Meta documents.

### Base Sources: Split Merge

`sources` combine multiple physical sources into one base logical table without override semantics.

Use cases:

- split a large table by feature, owner, region, or content category;
- reduce source-control conflicts;
- keep one logical runtime table while allowing multiple authored files.

Rules for base sources:

- All base sources use the same Meta table definition.
- Each base source is normalized independently, then appended into one logical table.
- Unique indexes must remain unique across all base sources.
- Duplicate values for any unique index across base sources are conflicts.
- Non-unique indexes may naturally contain the same key across base sources.
- Diagnostics should include both conflicting source locations.

For `ObjectTable`, base sources contribute different fields or field paths to the same constant object. The same field path must not be assigned by multiple base sources. To replace a field path, use `override_sources`.

### Override Sources

`override_sources` are applied after base `sources` have produced the base logical table.

Use cases:

- version 1.0 and version 2.0 develop changes to the same logical table in different files;
- hotfix or branch-specific source layers override base data;
- row-level or field-level differences are easier to review than full-file edits.

Override granularity depends on table kind:

- `CollectionTable`: row-level override. One record overrides another record with the same primary-index key.
- `ObjectTable`: field-level override. One field or field path overrides the same field path from a lower-priority source.

Collection-table override sources require `primary_index`. ConfigForge does not define a primary key; the primary index is the logical identity used for override matching. The primary index must point to a unique index, and it may be single-field or composite.

For collection tables, override sources should validate:

- the table declares `primary_index`;
- the primary index exists and is unique;
- every record in every override source can produce a complete primary-index key;
- within one override source, duplicate primary-index keys are errors;
- across override sources, duplicate primary-index keys are valid override events, resolved by source order or priority;
- an override row may replace a base row with the same primary-index key;
- behavior for override rows that do not match any base row should be explicit in a later policy, such as `allow_insert`, but should default to error until specified;
- after override resolution, all unique indexes across the final logical table remain unique.

For object tables, override sources should validate:

- each source row/column resolves to a field path;
- within one override source, duplicate field paths are errors;
- across override sources, duplicate field paths are valid override events, resolved by source order or priority;
- an override field path may replace a base field path;
- behavior for override field paths that do not exist in base data should be explicit in a later policy, but should default to error until specified;
- required fields are checked after override resolution.

### Source Ordering and Priority

Override sources need deterministic ordering. The design should support either declaration order or explicit priority/layer metadata. If both are present, explicit priority should be preferred.

Recommended rule:

- base `sources` are composed first;
- lower priority or earlier override source is applied first;
- higher priority or later override source overrides earlier override results;
- diagnostics and optional export metadata should preserve the winning source and overridden source locations.

### Multi-Source Validation Order

Recommended validation pipeline:

1. Parse Meta into one `MetaModel`.
2. Load and normalize each source independently.
3. Validate source-local syntax, header mapping, and field types.
4. Compose base `sources` with split-merge rules.
5. Apply `override_sources` in deterministic order when present.
6. Validate table-level constraints, unique indexes, foreign keys, dense indexes, and required fields on the final logical table.
7. Generate runtime data and query code from the final logical table.

For the broader Validator architecture, this pipeline should be interpreted as part of the staged Validator system. Built-in validators such as source conflicts, required fields, unique indexes, dense indexes, and foreign keys should run in deterministic phases together with configured business validators. The detailed Validator design is maintained in `Documentation/validator-design.md`.

## Export Context and Filtering

ConfigForge should support context-dependent export views. The complete `DataSet` is parsed, composed, and validated first. Then an `ExportContext` can derive a target-specific `ExportDataset` by applying export filters and field export policies.

Common context tags:

- `dev`
- `test`
- `shipping`
- `cn`
- `global`
- `ios`
- `android`

Record filtering example:

```toml
[[tables.export_filters]]
scope = "record"
when_tag = "shipping"
field = "test"
op = "=="
value = true
action = "exclude"
```

When the export context contains `shipping`, records whose `test` field is `true` are excluded from regular runtime data. The source data is not deleted; it remains available in the full `DataSet` for diagnostics, validation, and optional special exporter policies.

Tool-only field example:

```toml
[[tables.fields]]
name = "test"
type = "bool"
tool_only = true
filter_field = true
export = false
```

Rules:

- export filters must reference declared fields or field paths;
- filter operators must be compatible with field types;
- `tool_only` fields are parsed and validated but omitted from regular runtime data by default;
- `export = false` fields are omitted from regular runtime data, while special exporters may consume them only when explicitly allowed by policy;
- export filtering should happen after full-data validation by default;
- validators may later declare whether they run on full `DataSet` or filtered `ExportDataset`.

The detailed Data pipeline and export filtering design is maintained in `Documentation/data-design.md`.

## Polymorphic Records and Fragments

Some collection tables need polymorphic records. A common example is a game item table: every item has shared fields such as `id`, `name`, and `icon`, while different item functions require different additional data. Weapons may have stats and skills, equipment may have stats, and usable items may have use effects.

ConfigForge should model this with composition rather than inheritance. One logical record owns common fields plus zero or more fragments. All records still live in one logical table and share one ID/index namespace, but source data may be authored in multiple physical fragment source tables.

### Design Intent

- Prefer composition over inheritance.
- Keep one logical table for shared runtime identity and lookup.
- Keep IDs unique across all fragment source tables.
- Allow records to query whether they have a fragment.
- Allow generated code to retrieve a fragment only when the record owns it.
- Let each fragment define its own fields, including nested structs and arrays.

### Item Table Example

Logical table: `items`.

Common fields:

- `id`
- `name`
- `icon`
- `description`

Fragments:

- `stats`: attribute/stat struct, used by weapons and equipment.
- `skills`: skill list or skill struct, used by weapons.
- `usable`: use-effect data, used by consumable items.

Physical source tables may be split by variant or fragment-bearing subset:

| physical source | common fields | fragments |
| --- | --- | --- |
| `items/weapons.csv` | `id`, `name`, `icon` | `stats`, `skills` |
| `items/equipment.csv` | `id`, `name`, `icon` | `stats` |
| `items/usable.csv` | `id`, `name`, `icon` | `usable` |

After normalization, all rows become records in the same logical `items` table. Their IDs must not conflict unless an explicit override layer is being applied.

### Fragment Model

A fragment is a named data block with its own field schema.

Conceptual sketch:

```rust
struct FragmentMeta {
    name: String,
    fields: Vec<FieldMeta>,
}

struct PolymorphicRecordMeta {
    common_fields: Vec<FieldMeta>,
    fragments: Vec<FragmentMeta>,
}
```

Fragments are optional by default unless a source or variant rule requires them. A record can own multiple fragments.

### Fragment Source Rules

Fragment source tables are physical sources for the same logical collection table. They may include:

- common fields required for every item;
- fragment fields for one or more fragments;
- a discriminator or declared source-level fragment list.

Rules:

- Every fragment source contributes records to the same logical table.
- Primary-index or unique-index values must be unique across all base fragment sources.
- In `override_sources`, records with the same primary-index key are override events, not independent polymorphic variants.
- If two base fragment sources provide the same record ID, that is a conflict even if their fragments differ.
- If one physical source declares it provides `stats`, rows from that source must satisfy the `stats` fragment schema.
- Missing optional fragments are valid.
- Missing required common fields are errors.

### Meta TOML Draft

```toml
[[tables]]
name = "items"
kind = "collection"
polymorphic = true
primary_index = "by_id"
sources = [
  { path = "items/weapons.csv", fragments = ["stats", "skills"] },
  { path = "items/equipment.csv", fragments = ["stats"] },
  { path = "items/usable.csv", fragments = ["usable"] },
]

[[tables.fields]]
name = "id"
type = "int"
required = true

[[tables.fields]]
name = "name"
type = "text"
required = true

[[tables.fields]]
name = "icon"
type = "asset"
asset_kind = "icon"
required = true

[[tables.fragments]]
name = "stats"

[[tables.fragments.fields]]
name = "attributes"
type = "struct"

[[tables.fragments]]
name = "skills"

[[tables.fragments.fields]]
name = "skill_ids"
type = "array"
layout = "json_cell"

[tables.fragments.fields.element]
type = "int"

[[tables.fragments]]
name = "usable"

[[tables.fragments.fields]]
name = "use_effect"
type = "struct"

[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
order = "unordered"
```

### Meta XML Draft

```xml
<table name="items" kind="collection" polymorphic="true" primaryIndex="by_id">
  <source path="items/weapons.csv" fragments="stats skills" />
  <source path="items/equipment.csv" fragments="stats" />
  <source path="items/usable.csv" fragments="usable" />

  <field name="id" type="int" required="true" />
  <field name="name" type="text" required="true" />
  <field name="icon" type="asset" assetKind="icon" required="true" />

  <fragment name="stats">
    <field name="attributes" type="struct" />
  </fragment>

  <fragment name="skills">
    <field name="skill_ids" type="array" layout="json_cell">
      <element type="int" />
    </field>
  </fragment>

  <fragment name="usable">
    <field name="use_effect" type="struct" />
  </fragment>

  <index name="by_id" unique="true" order="unordered">
    <fieldRef path="id" />
  </index>
</table>
```

### Generated Query Semantics

Generated runtime access should reflect fragment ownership:

- exact record lookup still uses table indexes, such as `items.by_id(id)`;
- each record can answer whether it has a fragment, such as `has_fragment("stats")`;
- typed generated APIs may expose `has_stats()`, `stats()`, `has_skills()`, or `skills()`;
- missing fragments should be represented safely, for example `Option<&StatsFragment>` in Rust-like APIs;
- common fields remain available on every record.

### Validation Rules

- Common fields are validated for every row in every fragment source.
- Fragment fields are validated according to the source-declared fragment list or row-level fragment discriminator.
- Fragment names must resolve to declared fragment definitions.
- Fragment field names must not collide with common field names unless explicitly namespaced.
- Unique indexes, especially the primary index, are validated across the final logical table.
- In base `sources`, duplicate IDs across weapon/equipment/usable physical tables are conflicts.
- In `override_sources`, duplicate primary-index keys are override events, not separate variants.
- Foreign keys may target polymorphic tables by any unique index, including primary index.
- Foreign keys to a polymorphic table validate record existence, not fragment presence, unless the foreign key later declares a required fragment constraint.

## Field Layouts

`FieldLayout` describes how one logical field maps to source cells. Layout is separate from `FieldType`: a struct may use expanded columns in one sheet and a JSON cell in another, while retaining the same logical type.

### Single Cell Layout

A primitive field usually maps to one cell.

```csv
id,name,price
1001,Iron Sword,150
```

### JSON Cell Layout

A `JsonCell` layout stores a struct, array, or nested value in one source cell as JSON. This is compact and works well for variable-length or deeply nested values.

```csv
id,rewards
1001,"[{""item_id"":2001,""count"":1},{""item_id"":2002,""count"":3}]"
```

Logical result:

```json
{
  "id": 1001,
  "rewards": [
    { "item_id": 2001, "count": 1 },
    { "item_id": 2002, "count": 3 }
  ]
}
```

### Expanded Columns Layout

An `ExpandedColumns` layout spreads a struct or fixed-slot array across multiple columns. This is often more designer-friendly for shallow data.

```csv
id,reward.item_id,reward.count,rewards[0].item_id,rewards[0].count,rewards[1].item_id,rewards[1].count
1001,2001,1,3001,5,3002,10
```

### Expanded Rows Layout

An `ExpandedRows` layout spreads repeated values across multiple rows. This requires a parent-record relationship such as `id`, `parent_id`, `group`, or another declared join key.

```csv
item_id,reward_index,reward.item_id,reward.count
1001,0,3001,5
1001,1,3002,10
```

### Layout Decision Table

| Value Shape | Recommended Layouts | Notes |
| --- | --- | --- |
| primitive | single cell | Default and simplest mapping. |
| struct | expanded columns, JSON cell, expanded rows for object tables | Expanded columns are designer-friendly; JSON is compact. |
| array<primitive> | expanded columns, JSON cell, expanded rows | Expanded columns may imply fixed slots. |
| array<struct> | expanded columns, expanded rows, JSON cell | Expanded rows scale better for variable length. |

## Meta TOML Draft for Field Types and Layouts

The following TOML is a design draft. Current implementation is not required to support this exact structure. Future implementation may adjust nesting shape for parsing convenience while preserving the semantics.

```toml
[[tables]]
name = "items"
kind = "collection"
source = "items.csv"
primary_index = "by_id"

[[tables.fields]]
name = "id"
type = "int"
required = true

[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
order = "unordered"

[[tables.fields]]
name = "display_name"
type = "text"
required = true

[[tables.fields]]
name = "icon"
type = "asset"
required = true
asset_kind = "texture"

[[tables.fields]]
name = "reward"
type = "struct"
layout = "expanded_columns"

[[tables.fields.fields]]
name = "item_id"
type = "int"
required = true

[[tables.fields.fields]]
name = "count"
type = "int"
required = true

[[tables.fields]]
name = "rewards"
type = "array"
layout = "json_cell"

[tables.fields.element]
type = "struct"

[[tables.fields.element.fields]]
name = "item_id"
type = "int"

[[tables.fields.element.fields]]
name = "count"
type = "int"
```

Recommended logical type strings:

- `int`
- `float`
- `bool`
- `string`
- `text`
- `asset`
- `color_rgb`
- `color_rgba`
- `vector2`
- `vector3`
- `vector4`
- `vector2_int`
- `vector3_int`
- `rotation`
- `datetime`
- `date`
- `time`
- `duration`
- `int_range`
- `float_range`
- `rect`
- `bounds3`
- `enum`
- `struct`
- `array`

Recommended layout strings:

- `single_cell`
- `json_cell`
- `expanded_columns`
- `expanded_rows`

## Asset Validation

Asset fields represent engine resource references. They are not plain strings, even if their source representation is textual.

During validation/export, the system should verify that referenced assets exist according to configured asset roots or provider-specific lookup rules. A future implementation may support `asset_kind`, allowed extensions, engine-specific GUIDs, or project asset databases.

Example draft:

```toml
[[tables.fields]]
name = "prefab"
type = "asset"
asset_kind = "prefab"
required = true
```

Validation diagnostics should report:

- table name;
- row/column/cell or object field source location;
- full field path;
- raw asset reference;
- expected asset kind or search root when declared.

This design does not bind ConfigForge to Unity, Unreal, Godot, or any other engine-specific asset system.

## Nested Validation Rules

- Every physical mapping must resolve to a declared field or field path.
- Every required nested field must be present unless its parent field is optional and absent.
- Unknown nested properties in JSON cells should be rejected by default.
- JSON cell values must parse and match the declared `FieldType`.
- Struct fields should validate all child fields recursively.
- Array fields should validate every element recursively.
- Struct array elements should validate against their struct schema recursively.
- Expanded array indexes should be contiguous unless the layout explicitly allows sparse arrays.
- The same field path should not be written by multiple physical cells unless a merge policy is explicitly declared.
- Asset fields should validate resource existence before export succeeds.
- Text fields should validate localization requirements according to the selected i18n policy.
- Validation diagnostics should include the complete `FieldPath`.

## Index Design

Indexes describe how collection-table records can be looked up and how generated runtime query code should expose those lookups.

### Collection Table Access Requirement

Most `CollectionTable` definitions should declare at least one index because runtime consumers usually need predictable query entry points. A collection table may define multiple indexes for different query patterns. One index may be marked as the table's `primary_index`.

However, some collection tables are intentionally consumed as a source-ordered array instead of through keyed lookup. Examples include random-name lists, weighted phrase lists, ordered tutorial steps, ordered credit entries, and other content where source order is the runtime shape.

ConfigForge should therefore support an explicit table-level opt-in:

```toml
[[tables]]
name = "random_names"
kind = "collection"
source = "random_names.csv"
preserve_source_order = true
```

Recommended semantics:

- `preserve_source_order = true` means the final collection should expose an ordered array view in source composition order.
- The option is explicit and defaults to `false`, because most collection tables should not accidentally expose source-order APIs.
- A collection table may declare indexes and also set `preserve_source_order = true`; this supports both keyed lookup and ordered iteration/random selection.
- A collection table may omit indexes only when `preserve_source_order = true` and no feature such as foreign-key targeting, row-level override identity, or generated lookup API requires an index.
- A collection table with neither indexes nor `preserve_source_order = true` is invalid.
- `preserve_source_order` is not an index and does not imply uniqueness, key lookup, range query, or foreign-key targetability.

`ObjectTable` exports one object and does not require indexes, but its fields may still declare foreign keys to collection tables.

### Source-Ordered Collection Export

Source-ordered collection export preserves the final logical record order as an array. It is useful when the runtime needs to:

- select a random element from all records;
- keep designer-authored display/order semantics;
- iterate records in authored order;
- avoid declaring meaningless synthetic indexes for data that has no natural key.

Ordering rules:

1. Each source provider produces rows in its provider-defined stable order.
2. Base `sources` are composed in declaration order unless an explicit source priority/order policy is declared.
3. Rows inside one source preserve source row order after header/comment filtering.
4. `override_sources` that replace existing rows should keep the replaced row's array position by default.
5. Insert behavior for override rows that do not match an existing row is still a future policy and should default to error until explicitly designed.
6. Filtering for export context tags removes rows from the exported array without reordering remaining rows.

TOML draft with ordered-only collection:

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
required = false
```

TOML draft with both index lookup and ordered array access:

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

Generated runtime access should reflect this distinction:

- indexed collection: generate lookup APIs from indexes;
- source-ordered collection: generate array/list access such as `all()`, `records()`, `len()`, or random helper APIs when configured;
- collection with both: generate both lookup APIs and ordered array access.

Validation rules:

- `preserve_source_order` is valid only for `CollectionTable`.
- A collection table must have at least one access strategy: one or more indexes, or `preserve_source_order = true`.
- Foreign keys cannot target `preserve_source_order`; they still require the target table to expose a unique index or primary index.
- Row-level override still requires `primary_index` because source order alone cannot identify which row should be replaced.
- Diagnostics should warn when a table uses `preserve_source_order = true` with multiple base sources but no explicit source ordering policy, if declaration order is considered too implicit for the project.

### Primary Index

ConfigForge defines a primary index, not a primary key. `primary_index` is a table-level alias pointing to one declared unique index. Its only special purpose is to let foreign keys omit the target index name.

`primary_index` does not define index fields by itself. The referenced index still owns all index details, including:

- single-field or composite key shape;
- unordered or ordered behavior;
- dense integer storage when applicable;
- generated query shape;
- runtime storage implications.

Because the primary index is just a named index reference, it may point to a composite unique index. It is not restricted to one field.

`primary_key` is not part of the canonical Meta design. If accepted in a future compatibility layer, it should only be treated as shorthand for declaring a single-field unique index and assigning it as `primary_index`. New Meta documents should prefer explicit `primary_index` plus an explicit `[[tables.indexes]]` entry.

### Index Cardinality

An index may be unique or non-unique:

- `unique`: one key maps to at most one object. Generated query code should return an optional/single record result.
- `multi`: one key may map to multiple objects. Generated query code should return a collection or iterator of records.

Foreign keys must target a unique index because a field reference should resolve to at most one object.

### Index Key Shape

An index key may be built from one or more fields:

- Single-field index: key is one field value, such as `id`.
- Composite index: key is a tuple of multiple field values, such as `(item_type, rarity)`.

All index fields must resolve to declared fields or field paths. Composite index field order is part of the key definition.

### Index Ordering

Indexes may be unordered or ordered:

- `unordered`: optimized for exact key lookup. A runtime representation may be a hash table.
- `ordered`: preserves key ordering and can support range queries. A runtime representation may be a sorted array or ordered map.

Range queries are only meaningful for ordered indexes. Composite ordered indexes should define range semantics lexicographically unless a later design specifies a different policy.

### Dense Integer Index

A single-field integer index may declare dense layout. Dense indexes require keys to be contiguous with no holes according to their declared range policy. This enables runtime storage as a dense array rather than a hash table or sorted array.

Dense integer indexes should validate:

- the indexed field type is integer;
- the index uses exactly one field;
- keys are unique;
- no required key is missing in the declared dense range;
- duplicate keys are rejected.

### Runtime Storage Implications

Index metadata guides generated query code and runtime storage strategy:

| Index Shape | Suggested Runtime Storage | Query Shape |
| --- | --- | --- |
| unordered unique | hash table | exact key -> optional record |
| unordered multi | hash table to list | exact key -> records |
| ordered unique | sorted array / ordered map | exact key and range query |
| ordered multi | sorted array grouped by key | exact key and range query -> records |
| dense integer unique | dense array | direct integer key -> optional record |

The exact generated data structure may depend on the target language or game engine, but the query semantics should follow the index metadata.

### Index TOML Draft

```toml
[[tables]]
name = "items"
kind = "collection"
source = "items.csv"
primary_index = "by_id"

[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
order = "unordered"

[[tables.indexes]]
name = "by_type_and_code"
fields = ["item_type", "code"]
unique = true
order = "ordered"

[[tables.indexes]]
name = "by_type_and_rarity"
fields = ["item_type", "rarity"]
unique = false
order = "ordered"

[[tables.indexes]]
name = "by_dense_id"
fields = ["id"]
unique = true
order = "ordered"
storage = "dense_array"
```

### Index XML Draft

```xml
<table name="items" kind="collection" source="items.csv" primaryIndex="by_id">
  <index name="by_id" unique="true" order="unordered">
    <fieldRef path="id" />
  </index>
  <index name="by_type_and_rarity" unique="false" order="ordered">
    <fieldRef path="item_type" />
    <fieldRef path="rarity" />
  </index>
  <index name="by_dense_id" unique="true" order="ordered" storage="dense_array">
    <fieldRef path="id" />
  </index>
</table>
```

## Foreign Key Constraints

A `ForeignKey` declares that a field value references an object in another table.

Foreign keys may be declared on fields of both collection tables and object tables. A foreign key target contains:

- target table name;
- optional target index name;
- optional target field paths when the target is a composite unique index.

If the target index name is omitted, the foreign key resolves through the target table's `primary_index`. If a target index name is provided, that index must exist and must be unique. Non-unique indexes are not valid foreign-key targets.

Foreign keys can target composite unique indexes. In that case, the source side must provide one value per target index field, in the same order as the target index definition unless a later mapping policy explicitly names each source-to-target pair.

### Foreign Key Validation

Foreign key validation should check:

- the target table exists;
- the target index exists, or the target table has a primary index when no index name is specified;
- the target index is unique;
- source field type(s) are compatible with target index field type(s);
- composite foreign keys provide the same number of key parts as the target unique index;
- composite foreign-key values use a deterministic field order or explicit source-to-target mapping;
- each non-empty source value resolves to one target object;
- diagnostics include source table, source field path, row/cell location, target table, target index, and missing key value.

### Foreign Key TOML Draft

```toml
[[tables.fields]]
name = "item_id"
type = "int"
required = true

[tables.fields.foreign_key]
table = "items"
# index omitted: use target table primary_index
```

Composite foreign key targeting the target table primary index:

```toml
[[tables.fields]]
name = "item_ref"
type = "struct"

[[tables.fields.fields]]
name = "item_type"
type = "string"

[[tables.fields.fields]]
name = "code"
type = "string"

[tables.fields.foreign_key]
table = "items"
# index omitted: use target table primary_index, which may be composite
fields = ["item_type", "code"]
```

```toml
[[tables.fields]]
name = "reward_item_key"
type = "struct"

[tables.fields.foreign_key]
table = "items"
index = "by_type_and_rarity_unique"
fields = ["item_type", "rarity"]
```

### Foreign Key XML Draft

```xml
<field name="item_id" type="int" required="true">
  <foreignKey table="items" />
</field>

<field name="item_ref" type="struct">
  <field name="item_type" type="string" />
  <field name="code" type="string" />
  <foreignKey table="items">
    <fieldRef path="item_type" />
    <fieldRef path="code" />
  </foreignKey>
</field>

<field name="reward_item_key" type="struct">
  <foreignKey table="items" index="by_type_and_rarity_unique">
    <fieldRef path="item_type" />
    <fieldRef path="rarity" />
  </foreignKey>
</field>
```

## Table Kinds

ConfigForge supports two table kinds at the Meta design level:

1. Object Table
2. Collection Table

The recommended Meta discriminator is `kind = "object"` or `kind = "collection"`.

## Object Table

An object table represents one constant object. It is suitable for global settings, tuning constants, feature switches, and other single-instance configuration.

Conceptually, an object table is a top-level struct-like value. Its fields come from Meta, and source rows or columns assign values to those fields or nested field paths.

### Physical Shape

Each source row describes one field.

| field | value | note |
| --- | --- | --- |
| max_level | 100 | Maximum player level |
| default_gold | 1000 | Initial player gold |
| enable_pvp | true | Whether PvP is enabled |
| default_reward.item_id | 2001 | Struct field path |
| default_reward.count | 1 | Struct field path |

The recommended key/value source columns are:

- `field`: field name matching a Meta field.
- `value`: raw field value.
- `note` or `description`: optional designer-facing comment, not part of runtime output.

The `field` value may be a simple field name such as `max_level` or a nested field path such as `default_reward.item_id`. The `value` cell may contain a primitive value or a JSON cell for struct/array values.

### Logical Result

```json
{
  "max_level": 100,
  "default_gold": 1000,
  "enable_pvp": true
}
```

### Meta TOML Draft

```toml
[[tables]]
name = "game_settings"
kind = "object"
source = "game_settings.csv"

[[tables.fields]]
name = "max_level"
type = "int"
required = true

[[tables.fields]]
name = "default_gold"
type = "int"
required = true

[[tables.fields]]
name = "enable_pvp"
type = "bool"
required = true
```

### Validation Rules

- Each source row maps to exactly one logical field assignment.
- Field names in the `field` column should be unique.
- Field paths in the `field` column must resolve to declared Meta fields.
- Required Meta fields must exist in the source data.
- Values must match declared field types.
- JSON cell values must match the target field type when used.
- Unknown fields should be rejected unless a future extension policy explicitly allows them.
- Designer-only columns such as `note` should not affect runtime output.

### Recommended Code Naming

- `TableKind::Object`
- `ObjectTable`
- `ConstantObject`

## Collection Table

A collection table represents a list of same-shaped objects. It is suitable for item tables, monster tables, level tables, skill tables, and most regular game configuration sheets.

Conceptually, each collection record is a top-level struct-like value. Headers map physical columns to fields or nested field paths on that record.

### Physical Shape

Each source row describes one logical object. Each source column maps to one field.

| id | name | price | enabled |
| --- | --- | --- | --- |
| 1001 | Iron Sword | 150 | true |
| 1002 | Health Potion | 25 | true |
| 1003 | Debug Token | 0 | false |

Nested fields can be expanded into headers:

| id | name | display_name | icon | reward.item_id | reward.count |
| --- | --- | --- | --- | --- | --- |
| 1001 | iron_sword | Iron Sword | assets/icons/iron_sword.png | 2001 | 1 |

### Logical Result

```json
[
  {
    "id": 1001,
    "name": "Iron Sword",
    "price": 150,
    "enabled": true
  },
  {
    "id": 1002,
    "name": "Health Potion",
    "price": 25,
    "enabled": true
  },
  {
    "id": 1003,
    "name": "Debug Token",
    "price": 0,
    "enabled": false
  }
]
```

### Meta TOML Draft

```toml
[[tables]]
name = "items"
kind = "collection"
source = "items.csv"
primary_index = "by_id"

[[tables.fields]]
name = "id"
type = "int"
required = true
unique = true

[[tables.fields]]
name = "name"
type = "string"
required = true

[[tables.fields]]
name = "price"
type = "int"
required = true

[[tables.fields]]
name = "enabled"
type = "bool"
required = true

[[tables.indexes]]
name = "by_id"
fields = ["id"]
unique = true
order = "unordered"
```

### Validation Rules

- Header columns should map to Meta fields.
- Header columns may map to nested field paths.
- Required fields must be present in the source header.
- Values must match declared field types.
- JSON cell values must match declared struct or array types when used.
- Every collection table must declare at least one access strategy: one or more indexes, or `preserve_source_order = true`.
- The primary index must refer to an existing unique index when declared.
- Primary keys and unique indexes must not contain duplicate values.
- Unknown columns should be rejected unless a future extension policy explicitly allows them.
- Each row should provide enough context for actionable validation errors.

### Recommended Code Naming

- `TableKind::Collection`
- `CollectionTable`
- `Record`

## Design Decisions

- Use `kind = "object"` and `kind = "collection"` as the stable Meta discriminator.
- Parse all Meta syntaxes into one canonical in-memory Meta model.
- Support TOML and XML as initial Meta document syntaxes.
- Provide an XML Schema for XML Meta so IDEs can validate authored Meta structure.
- Treat `Field` as the common base unit for both object and collection tables.
- Treat table kinds as source interpretation rules, not separate field systems.
- Treat `FieldType` as independent from physical sheet layout.
- Provide builtin value types for common one-cell composite values such as colors, vectors, rotations, date/time values, durations, and ranges.
- Use `FieldPath` for nested mapping, diagnostics, and future code generation.
- Support both designer-friendly expanded layouts and compact JSON cell layouts.
- Validate nested values recursively.
- Run built-in and configured validators through deterministic validation phases.
- Treat normalized source sheets as provider-independent two-dimensional inputs.
- Treat full `DataSet` and target-specific `ExportDataset` as separate concepts.
- Support export context tags and export filters for target/version/region-specific output views.
- Allow `tool_only` fields to participate in parsing, validation, and filtering while staying out of regular runtime data.
- Support Meta-level enums for finite allowed symbolic values.
- Support reusable named structs for shared logical data shapes.
- Reject named-struct reference cycles by default because structs are embedded value shapes.
- Validate `asset` references during export validation.
- Keep `text` distinct from `string` for future localization workflows.
- Keep target-language and engine-specific type mapping outside Meta; exporters own that configuration.
- Require every collection table to define at least one access strategy: indexed lookup or explicit source-ordered array export.
- Allow `preserve_source_order = true` for collection tables whose runtime shape is an authored-order array rather than keyed lookup.
- Allow one primary index per collection table, and require it to be unique.
- Allow foreign keys from object-table or collection-table fields to another table's primary index or a named unique index.
- Allow one logical table to read from multiple physical sources.
- Use base `sources` for conflict-free split composition across physical files.
- Use `override_sources` for deterministic row-level or field-level version layering.
- Require collection-table override sources to use `primary_index` as logical record identity.
- Model polymorphic records with fragments and composition rather than inheritance.
- Keep polymorphic records in one logical table and one shared index namespace.
- Generate safe fragment query APIs for checking and retrieving optional fragments.
- Prefer `collection` over `row` in Meta because it describes the logical output rather than the physical sheet layout.
- Prefer `object` over `list` because the logical output is one object, not a list container.
- Keep physical layout details in parsing and normalization rules.
- Keep logical output concepts in Meta naming.
- Keep designer-facing comments separate from runtime data unless a field is explicitly declared in Meta.

## Non-Goals

- This document does not implement parsing, validation, or export behavior.
- This document does not implement builtin value parsers for color/vector/rotation/datetime/range yet.
- This document does not implement the Validator runner, registry, plugin system, or diagnostic output format.
- This document does not implement runtime data exporters, special exporters, or export context filtering.
- This document does not define the full advanced constraint expression language.
- This document does not require spreadsheet providers beyond CSV.
- This document does not finalize generated code shape for any specific game engine.
- This document does not require existing fixtures to immediately include both table kinds.
- This document does not implement nested field parsing.
- This document does not define a complete i18n export pipeline.
- This document does not define engine-specific asset database integration.
- This document does not define target-specific type names for enums or named structs.
- This document does not support recursive embedded structs without an explicit future indirection policy.
- This document does not implement TOML/XML parsers or generate the XML Schema yet.
- This document does not implement multi-source loading, layer selection, or override resolution.
- This document does not implement fragment composition, fragment-aware code generation, or row-level fragment discriminators.
- This documentation-only update does not modify `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock`.

## Future Implementation Impact

When implementation begins, likely affected areas are:

- `src/meta.rs`: add `TableKind` and table-kind parsing.
- `src/source.rs`: support object-table source normalization in addition to collection-table rows.
- `src/validation.rs`: validate object tables and collection tables with different contexts.
- `src/exporter.rs`: choose single-object or collection export shape by table kind.
- `tests/fixtures/standard_project/`: add fixtures that cover both object and collection tables.

Later implementations may also extend these areas for TOML/XML parsing, XML Schema publishing, enum parsing and validation, builtin value parsing and validation, datetime pattern validation, named-struct registration, struct-reference cycle detection, nested field parsing, header mapping, layout normalization, typed DataSet construction, export context filtering, multi-source loading, source conflict detection, override resolution, polymorphic fragment normalization, fragment-aware validation, Validator registry/runner/diagnostics, index validation, foreign-key validation, generated query APIs, export type mapping, runtime data exporters, special exporters, asset existence checks, and i18n-aware text export. This documentation update does not modify those files.
