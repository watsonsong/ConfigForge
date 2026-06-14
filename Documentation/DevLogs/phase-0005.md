# Phase 0005 Developer Log — Schema Docs Stabilization and Builtin Color / Vector Value Types

## Timebox

- Approximate period: after `phase-0004` through 2026-06-14.
- Snapshot range observed locally:
  - after the Phase 0004 backend/runtime reader expansion;
  - through the current Color / Vector builtin value type completion work before the phase log commit.

## Scope

Phase 0004 left ConfigForge with a broad multi-language codegen matrix, sink-first artifact output, and C# / Java runtime reader templates. Phase 0005 focused on two product-quality tracks:

- stabilize schema documentation as a first-class collaboration artifact;
- finish the remaining `color` / `vector` builtin value type work across validation, reporting, codegen mappings, standard fixtures, and documentation.

The major work was:

- lock down schema doc behavior and output quality for target-aware table docs;
- remove stale schema-doc TODOs after current behavior was documented as stable;
- add Color / Vector builtin validators and validation report enrichment;
- carry Color / Vector schema metadata into the CodeModel IR for backend mapping decisions;
- add engine/profile mapping support for C#, C++, Java, TypeScript Cocos Creator, and Unreal C++ defaults;
- add standard project C# Unity / .NET targets and CLI integration coverage;
- update design and implementation docs so Color / Vector builtin value types are no longer described as unimplemented;
- delete the completed Color / Vector TODO after all remaining steps were closed.

This log is historical context. The canonical behavior remains in `Documentation/Design/`, `Documentation/Implementation/`, and `Documentation/Tech/`.

## Status Snapshot

### Implemented Capabilities Added or Stabilized in This Phase

- Schema doc stabilization:
  - schema docs now have stronger output-quality coverage and path/error regression tests;
  - standard fixture docs exercise target-aware table content and generated HTML/Markdown behavior;
  - completed schema-doc stabilization TODOs were removed after current status was documented.
- Color / Vector validation:
  - `color` and `vector` builtin validators were added to the validator registry;
  - Vector validation supports selected component bounds and magnitude checks, including custom component names;
  - Color validation supports channel / alpha bounds and compile-time compatibility checks against field metadata;
  - validation tests cover valid/invalid validator configuration and runtime data failures.
- Designer validation report enrichment:
  - report issues can now include `schema_metadata` derived from linked Meta for builtin Color / Vector fields;
  - Markdown, CSV, and HTML renderers include a Schema column / field so designers and QA can see format, channel, component, and unit context next to failures.
- CodeModel builtin metadata:
  - `CodeField` now carries `builtin_options` for Color / Vector fields;
  - CodeModel tests verify that color channel/space/format and vector dimension/component/unit metadata survive target-aware model construction;
  - codegen backends can make profile decisions without re-reading Meta internals.
- C# backend mappings:
  - `CSharpCodegenOptions` gained profile-aware `type_mappings` and extra `using` support;
  - Unity / Godot byte color channels are normalized during generated reader construction where needed;
  - tests cover user overrides, invalid options, and byte-channel normalization.
- C++ / Java custom mappings:
  - standard C++ gained `type_mappings` for Color and Vector builtin API types while keeping generated fallback reader suffixes stable;
  - Java gained equivalent `type_mappings` for JVM / Android / LibGDX-style math types;
  - tests cover mapped public field types and generated reader constructors.
- TypeScript / Cocos Creator mappings:
  - Cocos Creator import path is configurable through `cocos_import`, defaulting to `cc`;
  - tests cover both default `cc` import and project-specific re-export paths.
- Unreal C++ builtin defaults:
  - `cpp_unreal` continues to map builtin Color / Vector values to Unreal types such as `FLinearColor`, `FVector`, and `FIntPoint`;
  - tests cover reflected/public field type output and generated reader construction for custom vector component names.
- Standard project fixture coverage:
  - `client_unity` target generates C# with Unity profile mappings for Color / Vector fields;
  - `server_dotnet` target generates C# with .NET profile mappings and fallback structs where appropriate;
  - CLI integration tests build both targets and assert generated API type shapes.
- Documentation cleanup:
  - Design / Implementation docs now describe Color / Vector builtin value types as Implemented MVP rather than Planned / Not implemented;
  - configuration docs include C++ / Java type mapping and TypeScript `cocos_import` options;
  - the completed Color / Vector TODO file was deleted.

### Verification Snapshot

Representative verification commands run near the end of this phase:

```text
cargo fmt
cargo test
cargo lint
```

Observed results:

- `cargo fmt` completed successfully;
- `cargo test` passed across unit tests, CLI integration tests, runtime compliance tests, and doc tests;
- `cargo lint` completed successfully but still reports existing repository-wide Clippy warnings, mostly unrelated to this phase, such as legacy `too_many_arguments` warnings in build/runtime/validation helpers and a few older test/style suggestions.

### Known Constraints at the End of This Phase

- `cargo lint` is not warning-clean for the whole repository. This phase fixed Color / Vector related warnings that were practical to address, but pre-existing broad refactor warnings remain.
- Custom C++ and Java `type_mappings` assume positional constructor / aggregate initialization compatibility. Types that require factories such as `FromRgba` or static builders still need a future constructor/factory policy.
- Cocos Creator support covers generated TypeScript API imports and constructors, not a full runtime loader adapter for Cocos asset bundles.
- Unreal C++ coverage remains repository syntax/content smoke and does not replace real Unreal Engine / UHT / Editor validation.
- C# Unity profile emits UnityEngine types but does not generate Unity `ScriptableObject`, Addressables, Resources, or `JsonUtility` integration.

## Key Decisions and Rationale

### 1. Schema Metadata Should Travel with Validation Reports

Decision:

- Enrich designer-facing validation report issues with schema metadata for builtin value fields.
- Render the metadata in Markdown, CSV, and HTML reports.

Rationale:

- Color / Vector failures are much easier to understand when the report shows source format, channel range, color space, vector components, and units.
- Report generation already has access to linked Meta in build export flow, so the enrichment can be centralized instead of repeated per renderer.

Source material:

- `src/validation/report/model.rs`
- `src/validation/report/render.rs`
- `src/generator/validation_report/mod.rs`
- `src/validation/report/tests.rs`

### 2. Builtin Validators Should Compile Against Field Metadata

Decision:

- Add dedicated `color` and `vector` builtin validator factories.
- Validate configuration against the field type and field metadata before evaluating data rows.

Rationale:

- Generic range checks cannot express Color alpha policy, channel policy, or vector magnitude cleanly.
- Compile-time metadata checks prevent misleading runtime failures, for example a normalized color validator attached to a byte-channel field.

Source material:

- `src/validation/builtins/color.rs`
- `src/validation/builtins/vector.rs`
- `src/validation/registry.rs`
- `src/validation/tests.rs`

### 3. Codegen Must Use CodeModel Metadata, Not Meta Re-parsing

Decision:

- Store Color / Vector options on `CodeField::builtin_options`.
- Let backends consume `CodeBuiltinFieldOptions` when they need mapping or decode behavior.

Rationale:

- CodeModel is the language-neutral boundary for code generation.
- Backends should not re-derive engine mapping details from raw Meta fields, and should not infer byte-vs-normalized color behavior from target type names.

Source material:

- `src/code_artifact/ir.rs`
- `src/code_artifact/tests.rs`
- `src/generator/csharp/types.rs`

### 4. Profiles Provide Defaults; Type Mappings Provide Escape Hatches

Decision:

- Keep profile defaults for common engines and language ecosystems.
- Add explicit override tables for C#, C++, and Java, plus Cocos import path configuration for TypeScript.

Rationale:

- Most projects want a zero-config path for common targets such as .NET, Unity, Unreal, and Cocos Creator.
- Engine teams and server tooling often use custom math/color libraries, so generated API types need safe, explicit override points.
- Keeping overrides at target/export config level preserves Meta as engine-neutral schema.

Source material:

- `src/generator/csharp/mod.rs`
- `src/generator/cpp/mod.rs`
- `src/generator/java/mod.rs`
- `src/generator/typescript/mod.rs`
- `Documentation/Design/03-configuration.md`

### 5. Standard Fixture Targets Should Prove Real Build Paths

Decision:

- Add `client_unity` and `server_dotnet` targets to the standard project fixture.
- Cover both targets through CLI integration tests instead of only backend unit tests.

Rationale:

- Backend unit tests prove code snippets, but standard fixture targets prove Project config, target filtering, export task parsing, generated file writing, and real fixture schema/data all work together.
- Unity and .NET are representative client/server C# profiles for Color / Vector mapping coverage.

Source material:

- `tests/fixtures/standard_project/ConfigForge.toml`
- `tests/cli_integration/standard_project.rs`

## Follow-up Work

- Decide whether `cargo lint` should become warning-clean repository-wide or continue to tolerate known legacy warnings.
- Add constructor/factory policy for external mapped Color / Vector types that do not expose positional constructors.
- Consider runtime adapter examples for Cocos Creator and Unity-specific project integration.
- Continue expanding runtime compliance coverage for engine/profile-specific generated APIs where practical.
