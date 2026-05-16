# Export and Type Mapping Design

This document records export-facing design decisions that should stay separate from logical Meta definitions. Meta describes what the data means. Export configuration describes how that logical model is represented for a specific runtime, language, engine, or pipeline.

## Design Goals

- Keep Meta reusable across different games, engines, and target languages.
- Let the same validated logical data export to multiple target representations.
- Let the same validated data set export to multiple runtime data formats.
- Avoid embedding target-language names such as `FLinearColor`, `UnityEngine.Color`, or TypeScript interfaces inside Meta.
- Allow generated code to remain type-aware while preserving Meta as the source of logical truth.
- Make type mapping explicit, deterministic, and reviewable.
- Support special exporters such as gettext POT and asset manifest CSV without treating them as normal runtime data formats.
- Allow exporters and code generators to be registered by built-in modules or dynamic plugins.
- Allow independent export artifacts to be generated in parallel while keeping output path validation and artifact manifest ordering deterministic.

Implementation-level requirements for plugin loading, parallel export, async artifact writing, and structured diagnostics are defined in `Documentation/implementation-requirements.md`.

## Separation of Responsibilities

| Concern | Owner | Examples |
| --- | --- | --- |
| Logical data shape | Meta | `int`, `text`, `enum<ItemRarity>`, `color_rgba`, `vector3`, `datetime`, `struct<Reward>` |
| Data validation | Meta + validation pipeline | enum value exists, required struct field exists, no struct cycles |
| Source layout | Meta sheet mapping | `json_cell`, `expanded_columns`, field paths |
| Target type spelling | Export configuration | `FLinearColor`, `Color`, `number`, `ItemRarity` |
| Generated code style | Export target | TypeScript interface, Lua table annotation, C# class, C++ struct |
| Runtime storage | Export target | JSON object, Lua table, binary blob, generated lookup map |
| Runtime data format | Data export target | JSON, YAML, MessagePack, Protocol Buffers, SQLite |
| Export filtering | Data export context | `shipping`, `global`, `dev`, region/channel tags |
| Special artifacts | Special exporter | gettext POT, asset manifest CSV |

Meta should define logical concepts such as `color_rgba` or `vector3`. Export configuration should decide whether those builtin values become generated structs, built-in engine types, tuples, or plain objects in each target.

## Export Target

An `ExportTarget` describes one output. A project may configure multiple targets from the same Meta and source data.

Recommended target fields:

- `name`: stable target name such as `ts_unreal_editor` or `lua_unity_runtime`;
- `language`: generated code language, such as `typescript`, `lua`, `csharp`, or `cpp`;
- `engine`: optional engine or runtime family, such as `unreal`, `unity`, or `custom`;
- `data_format`: runtime data output format, such as `json`, `lua_table`, `binary`, or `custom`;
- `tags`: optional default export context tags used by this target;
- `output`: output directory or artifact location;
- `type_mappings`: mapping rules from logical Meta types to target types;
- `special_exporters`: optional auxiliary exporters such as `gettext_pot` or `asset_manifest_csv`;
- `generator_options`: target-specific formatting and runtime API options.

Exporter and code generator implementations may be built in or plugin-provided. The target configuration should refer to stable exporter/generator IDs, not dynamic library paths directly; plugin paths belong to implementation/runtime configuration.

`data_format` should be interpreted as runtime data format, not source format. See `Documentation/data-design.md` for the full Data pipeline and exporter format design.

Recommended runtime data formats:

- `json`: initial practical default and best fit for JavaScript/TypeScript-native runtimes;
- `yaml`: readable text format for targets with mature YAML runtime support;
- `msgpack`: compact binary JSON-like representation;
- `protobuf`: schema plus binary data artifacts;
- `sqlite`: database artifact for query-heavy runtime usage;
- `custom_binary`: future target-specific optimized format.

Special exporters are configured beside runtime data export but produce different artifact kinds. Examples:

- `gettext_pot`: scans `text` fields and writes gettext POT templates with source references and comments;
- `asset_manifest_csv`: scans `asset` fields and writes packaging manifests.

## Exporter Plugins and Parallel Artifacts

Export targets should be planned before files are written. The planner resolves data exporters, code generators, special exporters, type mappings, output paths, and artifact dependencies.

Recommended rules:

- resolve exporters through `DataExporterRegistry`, `CodeGeneratorRegistry`, and `SpecialExporterRegistry`;
- reject unknown exporter IDs and incompatible runtime data formats before generating artifacts;
- reject duplicate output paths before parallel export begins;
- run independent targets, tables, code modules, and special artifacts in parallel when their inputs and outputs do not conflict;
- write generated content through an artifact writer that can stage temporary files and commit deterministically;
- sort the final `ArtifactManifest` by output path, independent of task completion order;
- report exporter plugin failures as structured diagnostics containing target name, exporter ID, plugin ID, artifact path, and underlying error.

## Type Mapping

`TypeMapping` maps a logical Meta type to a target representation. It is intentionally export-owned because the correct mapping depends on where the generated artifact will be consumed.

Mapping sources may include:

- built-in primitive defaults per language;
- target-specific overrides for primitives, builtin value types, enums, and named structs;
- generated-code naming conventions;
- external runtime type imports or requires;
- serialization conversion functions.

Recommended resolution order:

1. Exact mapping for a builtin logical type, such as `builtin:color_rgba` or `builtin:vector3`.
2. Exact mapping for a named logical type, such as `struct:Reward`.
3. Exact mapping for a named enum, such as `enum:ItemRarity`.
4. Target-level default mapping for logical type families, such as all `int`, all `text`, or all builtin vector fields.
5. Exporter built-in default for the target language.
6. Error if no safe default exists.

## Builtin ColorRGBA Example

Logical Meta definition:

```toml
[[tables.fields]]
name = "tint"
type = "color_rgba"
color_channel = "normalized"
color_space = "srgb"
```

This Meta says only that `color_rgba` is a logical four-channel color value. It does not say how Unreal, Unity, TypeScript, or Lua should spell that type.

Export configuration draft:

```toml
[[targets]]
name = "ts_unreal_editor"
language = "typescript"
engine = "unreal"
data_format = "json"
output = "Generated/TS"
tags = ["shipping", "global"]

[targets.data]
split = "by_table"
pretty = false

[[targets.special_exporters]]
kind = "gettext_pot"
output = "Generated/Localization/messages.pot"

[[targets.special_exporters]]
kind = "asset_manifest_csv"
output = "Generated/Build/assets.csv"

[[targets.type_mappings]]
logical = "builtin:color_rgba"
target_type = "FLinearColor"
import = "unreal"
conversion = "ColorRgbaToFLinearColor"

[[targets]]
name = "lua_unity_runtime"
language = "lua"
engine = "unity"
data_format = "lua_table"
output = "Assets/Lua/Generated"

[[targets.type_mappings]]
logical = "builtin:color_rgba"
target_type = "Color"
import = "UnityEngine"
conversion = "ColorRgbaToUnityColor"
```

The same Meta can therefore export to TypeScript + Unreal as `FLinearColor` while exporting to Lua + Unity as `Color`.

## Enum Mapping

Meta enum definitions validate allowed values. Export configuration decides representation.

Possible target representations:

- generated enum type;
- string literal union in TypeScript;
- numeric constants;
- engine-native enum type;
- plain string when the runtime prefers data-driven values.

Draft:

```toml
# Option A: engine-native enum representation.
[[targets.type_mappings]]
logical = "enum:ItemRarity"
target_type = "EItemRarity"
representation = "native_enum"

# Option B: data-driven string representation.
[[targets.type_mappings]]
logical = "enum:ItemRarity"
target_type = "string"
representation = "string_literal"
```

Validation of source values remains Meta-owned. The exporter should fail only if it cannot map a validated logical enum to the configured target representation.

## Named Struct Mapping

Named structs have two default export strategies:

1. **Generated type**: generate a target-language struct/interface/class with fields matching the Meta struct.
2. **Mapped external type**: use a target-provided type and optionally generate conversion code.

The default should be generated type unless a target mapping explicitly overrides it.

Generated type example:

```toml
[[targets.type_mappings]]
logical = "struct:Reward"
target_type = "Reward"
representation = "generated_struct"
```

External type example:

```toml
[[targets.type_mappings]]
logical = "struct:Reward"
target_type = "Game.Config.Reward"
representation = "external_type"
conversion = "RewardFromConfig"
```

## Builtin Value Type Mapping

Builtin value types have exporter defaults, but targets should be able to override them.

Examples:

```toml
[[targets.type_mappings]]
logical = "builtin:vector3"
target_type = "UnityEngine.Vector3"
representation = "external_type"
conversion = "Vector3FromConfig"

[[targets.type_mappings]]
logical = "builtin:rotation"
target_type = "FRotator"
representation = "external_type"
conversion = "EulerToFRotator"

[[targets.type_mappings]]
logical = "builtin:datetime"
target_type = "number"
representation = "unix_timestamp_ms"
```

Builtin value type mapping should preserve logical validation semantics. For example, a `datetime` field is parsed and validated by Meta rules first, then mapped to ISO string, Unix timestamp, engine date type, or generated struct according to the target.

## Primitive Defaults

Each exporter should define safe primitive defaults, with target configuration able to override them.

Example default table:

| Logical Meta Type | TypeScript default | Lua default | Notes |
| --- | --- | --- | --- |
| `int` | `number` | `number` | Exporter may add integer annotations or validation comments. |
| `float` | `number` | `number` | Precision is target-specific. |
| `bool` | `boolean` | `boolean` |  |
| `string` | `string` | `string` | Plain non-localized text. |
| `text` | `LocalizedText` or `string` | `LocalizedText` or `string` | Depends on selected i18n export policy. |
| `asset` | `AssetRef` or target asset type | `AssetRef` or target asset type | Depends on engine integration. |
| `color_rgb` / `color_rgba` | generated object or target color type | table or target color type | May map to `FLinearColor`, `Color`, tuple, or generated struct. |
| `vector2` / `vector3` / `vector4` | generated object or target vector type | table or target vector type | May map to `Vector3`, `FVector`, tuple, or generated struct. |
| `vector2_int` / `vector3_int` | generated object or target integer vector type | table or target integer vector type | Useful for grid coordinates. |
| `rotation` | generated object or target rotation type | table or target rotation type | May map to Euler, quaternion, or engine rotator by conversion policy. |
| `datetime` | `string` or `number` | `string` or `number` | Target config chooses ISO string, timestamp seconds/ms, or engine date type. |
| `duration` | `number` or generated object | `number` or table | Target config chooses seconds, milliseconds, or structured duration. |
| `int_range` / `float_range` | generated object or tuple | table or tuple | Usually `{ min, max }` or `[min, max]`. |
| `rect` / `bounds3` | generated object or target bounds type | table or target bounds type | Target config chooses component naming and representation. |

## Validation Rules

Export configuration validation should check:

- every `logical` mapping references an existing logical type or known type family;
- no duplicate exact mapping exists within one target;
- target type names are valid for the target generator or explicitly marked external;
- required imports/includes are present for external target types;
- conversion functions are present when a representation requires conversion;
- enum representation is compatible with enum variant codes or names;
- named-struct external mappings do not hide incompatible field requirements unless a conversion function is declared;
- data format is supported by the selected exporter or plugin;
- special exporter kind is supported;
- target default tags are valid when a tag registry exists;
- export filters referenced by the target are valid for field types;
- Protobuf targets declare stable field numbering policy;
- SQLite targets declare nested value storage strategy;
- artifact output paths do not conflict across runtime data, schema, generated code, POT, and asset manifest outputs;
- diagnostics include target name, logical type, mapping entry, and reason.

## Non-Goals

- This document does not finalize the concrete CLI flags for selecting targets.
- This document does not require implementation of any code generator.
- This document does not choose one universal runtime representation for all engines.
- This document does not require implementation of every runtime data format.
- This document does not finalize CLI syntax for export context tags.
- This document does not move logical enum or named-struct definitions out of Meta.
- This document does not put engine-specific type names into Meta.

## Design Decision

Enum definitions and named struct definitions belong in Meta because they define logical data shape and validation. Target-specific type mapping belongs in export configuration because it defines how validated logical data is represented for a specific language, engine, and runtime. Runtime data file format and special exporter configuration also belong in export configuration, while the detailed typed data pipeline is documented in `Documentation/data-design.md`.
