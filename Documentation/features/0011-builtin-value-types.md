# Feature Spec 0011: Builtin Value Types

## Summary

Add common game-development composite values as builtin Meta field types so designers can author them conveniently in one cell without defining repeated custom structs. Initial builtin types include colors, vectors, Euler rotations, date/time values, durations, and numeric ranges.

## Documentation Scope

This feature is a documentation-only design step. It updates glossary, Meta design, export design, data design, validator design, and related feature specs so future implementation can add parsers, validators, typed values, and target-specific mappings for builtin value types.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/data-design.md`
- `Documentation/export-design.md`
- `Documentation/validator-design.md`
- `Documentation/features/0002-meta-field-layout-design.md`
- `Documentation/features/0007-enum-named-struct-type-mapping-design.md`
- `Documentation/features/0008-validator-system-design.md`

## Requirements

- Support builtin logical field types for common composite values.
- Builtin types should default to convenient `SingleCell` authoring.
- Builtin types should remain logical Meta types, not engine-specific target types.
- Export configuration should map builtin types to target-specific runtime types.
- Color types should include `color_rgb` and `color_rgba`.
- Color fields should support hex forms and rgb/rgba forms.
- Vector types should include `vector2`, `vector3`, and `vector4`.
- Rotation should represent Euler angles and support unit/order configuration.
- Date/time support should include `datetime` with field-level pattern configuration.
- Recommended additional builtin candidates should be documented for future implementation.

## Initial Builtin Types

| Meta type | Meaning | Example cell values |
| --- | --- | --- |
| `color_rgb` | RGB color | `#FF8800`, `rgb(255,136,0)`, `1,0.533,0` |
| `color_rgba` | RGBA color | `#FF8800CC`, `rgba(255,136,0,0.8)`, `1,0.533,0,0.8` |
| `vector2` | 2D float vector | `1,2`, `(1,2)`, `[1,2]` |
| `vector3` | 3D float vector | `1,2,3`, `(1,2,3)`, `[1,2,3]` |
| `vector4` | 4D float vector | `1,2,3,4`, `(1,2,3,4)`, `[1,2,3,4]` |
| `vector2_int` | 2D integer vector | `1,2`, `(1,2)`, `[1,2]` |
| `vector3_int` | 3D integer vector | `1,2,3`, `(1,2,3)`, `[1,2,3]` |
| `rotation` | Euler rotation | `0,90,0`, `pitch=0,yaw=90,roll=0` |
| `datetime` | Date-time value | `2026-05-16T12:30:00Z`, `2026-05-16 12:30:00` |
| `date` | Calendar date | `2026-05-16` |
| `time` | Time of day | `12:30:00` |
| `duration` | Time span | `10s`, `5m`, `2h`, `00:05:30` |
| `int_range` | Inclusive integer range | `1..10`, `1-10`, `[1,10]` |
| `float_range` | Inclusive float range | `0.1..0.5`, `[0.1,0.5]` |
| `rect` | 2D rectangle | `0,0,100,50`, `{x=0,y=0,w=100,h=50}` |
| `bounds3` | 3D bounds | `0,0,0;10,10,10` |

Lower-priority future candidates:

- project-specific curves, such as animation or difficulty curves;
- custom ID/reference wrappers when they are not better modeled as foreign keys;
- domain-specific compact values after repeated usage proves they deserve builtin status.

## Color Design

Color authoring should support:

- `#RRGGBB` for `color_rgb`;
- `#RRGGBBAA` for `color_rgba`;
- `rgb(255, 136, 0)`;
- `rgba(255, 136, 0, 0.8)`;
- comma tuple forms according to channel policy.

Meta draft:

```toml
[[tables.fields]]
name = "tint"
type = "color_rgba"
color_channel = "normalized" # normalized | byte | auto
color_space = "srgb"         # srgb | linear
```

Validation should check channel count, channel range, valid hex characters, and alpha policy.

## Vector Design

Vector fields support tuple-like one-cell values:

```toml
[[tables.fields]]
name = "spawn_position"
type = "vector3"
```

Accepted examples:

```text
1,2,3
(1, 2, 3)
[1,2,3]
x=1,y=2,z=3
```

Validation should check dimension and numeric components.

## Rotation Design

Rotation is a logical Euler rotation. It is not a quaternion in Meta.

```toml
[[tables.fields]]
name = "spawn_rotation"
type = "rotation"
rotation_unit = "degrees"
rotation_order = "xyz"
```

Exporters may map it to engine-specific Euler, rotator, or quaternion types.

## DateTime Design

`datetime` supports ISO-style defaults and field-level pattern configuration.

```toml
[[tables.fields]]
name = "open_at"
type = "datetime"
datetime_pattern = "%Y-%m-%d %H:%M:%S"
timezone = "Asia/Shanghai"
```

The implementation should standardize pattern syntax. A Rust implementation may use `chrono`-style tokens, but docs should treat the pattern as ConfigForge's declared pattern so it can be validated consistently.

## Export Mapping

Builtin value types map through export configuration.

Examples:

```toml
[[targets.type_mappings]]
logical = "builtin:color_rgba"
target_type = "FLinearColor"
conversion = "ColorRgbaToFLinearColor"

[[targets.type_mappings]]
logical = "builtin:vector3"
target_type = "UnityEngine.Vector3"
conversion = "Vector3FromConfig"

[[targets.type_mappings]]
logical = "builtin:datetime"
target_type = "number"
representation = "unix_timestamp_ms"
```

## Validation Rules

- Builtin type names must be known.
- Single-cell values must parse according to builtin type and field options.
- Color channels must match channel policy.
- Vector dimensions must match type.
- Rotation unit and order must be valid.
- Date/time pattern must be valid.
- Date/time values must parse according to pattern or default policy.
- Timezone policy must be explicit when needed.
- Export mappings must reference builtin types with a stable `builtin:<type>` logical key.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No final parser implementation for all listed syntaxes.
- No engine-specific default mapping commitment.
- No quaternion builtin in this step; rotation is Euler-based.
- No domain-specific builtins such as item references or currency references; those should use foreign keys, enums, strings, or validators.

## Acceptance Criteria

- `Documentation/glossary.md` includes builtin value type terminology.
- `Documentation/meta-design.md` describes builtin value types, cell syntax, field options, datetime pattern, and validation rules.
- `Documentation/data-design.md` includes builtin typed `DataValue` variants.
- `Documentation/export-design.md` describes builtin type mapping.
- `Documentation/validator-design.md` includes builtin value parse/check validators.
- `Documentation/README.md` links to this feature spec.
- `AGENTS.md` links to this feature spec.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
