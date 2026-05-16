# Feature Spec 0007: Enum, Named Struct, and Type Mapping Design

## Summary

Add Meta-level enum definitions and reusable named struct definitions, including struct-reference cycle detection. Separate logical Meta types from target-specific code export type mapping so one Meta model can generate different runtime representations for different languages and engines.

## Documentation Scope

This feature is a documentation-only design step. It updates the glossary, Meta design, and export design so later implementation can validate enum values, reuse struct shapes, reject recursive value nesting, and configure target-specific generated type names outside Meta.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/export-design.md`
- `Documentation/features/0002-meta-field-layout-design.md`

## Requirements

- Meta must support named enum definitions.
- Enum fields must reference declared enums.
- Enum values must be validated before export.
- Meta must support reusable named struct definitions.
- Table fields, fragment fields, array elements, and named structs may reference named structs.
- Inline structs remain valid for one-off shapes.
- Named structs may nest other named structs.
- Named struct nesting must reject cycles by default.
- Cycle diagnostics should include the complete reference path, such as `A -> B -> A`.
- Target-language and engine-specific type names must not be stored in logical Meta definitions.
- Code export type mapping must live in export configuration.
- The same logical type may map differently per target, such as builtin `color_rgba` to TypeScript + Unreal `FLinearColor` and Lua + Unity `Color`.

## Enum Design

Enums are logical finite value sets. They belong in Meta because they affect authoring validation and generated type-aware APIs.

Recommended behavior:

- enum names are unique within the Meta model;
- variant names are unique within an enum;
- optional variant codes are unique when declared;
- source values resolve to declared variants by strict rules;
- target representation is selected later by export configuration.

Example:

```toml
[[enums]]
name = "ItemRarity"

[[enums.variants]]
name = "common"
code = 1

[[enums.variants]]
name = "rare"
code = 2

[[tables.fields]]
name = "rarity"
type = "enum"
enum = "ItemRarity"
```

## Named Struct Design

Named structs are reusable logical shapes. They should be used for common domain values such as rewards, costs, colors, vectors, stat blocks, and conditions.

Recommended behavior:

- named struct definitions are declared once and referenced by name;
- named structs contain regular fields;
- fields can reference named structs with `type = "struct"` and `struct = "Name"`;
- arrays can use named structs as their element type;
- fragments can use named structs in their own field schemas;
- inline structs and named structs can coexist.

Example:

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
```

## Struct Cycle Detection

Named structs are embedded value shapes. Recursive references would imply infinite expansion and ambiguous layout/code generation, so cycles are invalid unless a future design introduces an explicit indirection type.

Validation should:

- build a dependency graph between named structs;
- include dependencies through array elements;
- include dependencies inside inline structs nested in named structs;
- detect direct cycles such as `A -> A`;
- detect indirect cycles such as `A -> B -> C -> A`;
- report source locations for each edge in the cycle.

Non-cyclic references should produce a topological order that exporters can use for generated type ordering.

## Type Mapping Design

Type mapping belongs to export configuration, not Meta.

Reasoning:

- Meta should remain stable when adding a new code export target.
- Logical validation should not depend on TypeScript, Lua, Unity, Unreal, or any other target.
- The same logical struct or enum may need different target names and representations.
- Exporters can choose generated types, external engine types, string/numeric enum representations, or conversion functions.

Example:

```toml
[[targets]]
name = "ts_unreal_editor"
language = "typescript"
engine = "unreal"

[[targets.type_mappings]]
logical = "builtin:color_rgba"
target_type = "FLinearColor"
import = "unreal"
conversion = "ColorRgbaToFLinearColor"

[[targets]]
name = "lua_unity_runtime"
language = "lua"
engine = "unity"

[[targets.type_mappings]]
logical = "builtin:color_rgba"
target_type = "Color"
import = "UnityEngine"
conversion = "ColorRgbaToUnityColor"
```

## Validation Rules

Meta validation should check:

- enum definitions are well formed;
- enum fields reference existing enums;
- named struct definitions are well formed;
- struct fields reference existing named structs;
- named struct dependency graph has no cycles;
- source values match enum and struct field requirements.

Export configuration validation should check:

- mapping entries reference existing logical types;
- duplicate exact mappings are rejected within one target;
- target type names/imports/conversions are valid for the selected generator;
- every logical type has either an explicit mapping or a safe target default.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No final CLI flag design for export target selection.
- No final syntax for every supported target language.
- No recursive embedded structs without a future explicit indirection design.

## Acceptance Criteria

- `Documentation/glossary.md` includes terms for enums, enum variants, named structs, struct references, struct dependency graphs, struct cycles, export targets, and type mapping.
- `Documentation/meta-design.md` describes Meta-level enum definitions, named structs, struct references, and cycle detection.
- `Documentation/export-design.md` describes export targets and target-specific type mapping outside Meta.
- `Documentation/README.md` links to the export design and this feature spec.
- `AGENTS.md` links to the export design and this feature spec.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
