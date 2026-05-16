# Feature Spec 0009: Data Export Pipeline Design

## Summary

Define the Data portion of ConfigForge: normalized two-dimensional source sheets, typed in-memory datasets, export dataset views, extensible runtime data formats, special exporters, export context tags, filtering rules, and tool-only fields.

## Documentation Scope

This feature is a documentation-only design step. It adds a dedicated Data design document and updates related design/index documents so future implementation can support multiple runtime data exporters and context-dependent export behavior.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/export-design.md`
- `Documentation/data-design.md`
- `Documentation/validator-design.md`
- `Documentation/features/0004-multi-source-table-design.md`
- `Documentation/features/0006-extensible-source-provider-design.md`
- `Documentation/features/0007-enum-named-struct-type-mapping-design.md`
- `Documentation/features/0008-validator-system-design.md`

## Requirements

- Each source provider should normalize data into a two-dimensional sheet model.
- A normalized sheet should support row/column access, row iteration, column/header mapping, and source locations.
- Sheet data should be parsed according to Meta into a typed `DataSet`.
- Base sources and override sources should compose into final logical table data before export.
- Runtime data export should use an extensible `DataExporter` abstraction.
- Initial implementation may only need JSON, but the architecture should allow YAML, MessagePack, Protocol Buffers, SQLite, and custom binary formats.
- Protobuf export must account for both schema artifacts and data artifacts.
- Special exporters should support artifacts that are not normal runtime data files, such as POT localization templates and Asset manifest CSV files.
- Export operations should accept context tags such as `dev`, `test`, `shipping`, `cn`, or `global`.
- Export filters should be able to include/exclude records, fields, fragments, or special exporter entries based on context tags and field values.
- Meta should support fields that participate in filtering/validation but are not emitted to runtime data.

## Data Pipeline

Recommended pipeline:

```text
Source Provider
  -> Source Sheet
  -> Sheet Mapping + Field Layout
  -> Typed Table Data
  -> Source Composition
  -> Validation
  -> Export Context Filtering
  -> Export Dataset
  -> Data Exporter(s)
  -> Runtime Data Files / Special Artifacts
```

## Runtime Data Formats

The design should treat runtime data format as target configuration, not as source format or code language.

Planned formats:

- `json`: first practical default, good for JavaScript/TypeScript and debugging.
- `yaml`: readable text output when runtime has a reliable parser.
- `msgpack`: compact binary JSON-like representation.
- `protobuf`: schema + binary data artifacts.
- `sqlite`: database file for query-heavy runtime usage.
- `custom_binary`: future high-performance format.

## Special Exporters

Special exporters scan the typed dataset or export dataset and produce auxiliary artifacts.

Required design examples:

- `gettext_pot`: extract all `text` fields into gettext POT format with source references, comments, table/field context, and original text.
- `asset_manifest_csv`: extract all `asset` fields into a resource list for game packaging, with asset kind, table, field path, record key, and source location.

## Export Context Tags and Filtering

An export command may specify tags that describe the current build context.

Examples:

- `shipping`
- `dev`
- `test`
- `cn`
- `global`
- `ios`
- `android`

Example requirement:

> In `shipping` mode, records whose `test` field is `true` should not be exported.

This should be modeled as export filtering, not as source deletion. Full data remains available for validation and diagnostics, while the `ExportDataset` contains only the target-specific exported view.

## Tool-Only Fields

Some fields are needed only by ConfigForge tools and should not be emitted to runtime data.

Examples:

- `test`: used to exclude rows from shipping export.
- `designer_note`: used by validators or POT comments.
- `region_mask`: used for region-specific filtering.

Recommended field properties:

- `export`: whether the field enters regular runtime data; default `true`.
- `tool_only`: field participates in parsing/validation/filtering but defaults to `export = false`.
- `filter_field`: field can be referenced by export filters.
- `validator_only`: field is used only by validators.
- `special_exports`: whether special exporters may consume the field.

## Validation Rules

Future implementation should validate:

- export filters reference existing fields;
- filter operators match field types;
- tool-only fields are not emitted to runtime data unless explicitly allowed;
- runtime data schema does not depend on removed fields;
- special exporters reference supported field types;
- Protobuf field numbering is stable;
- SQLite nested data strategy is explicit;
- artifact paths do not conflict;
- exporter formats and plugins are known.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No immediate implementation of YAML, MessagePack, Protobuf, SQLite, POT, or Asset CSV exporters.
- No final CLI syntax for context tags.
- No final expression language for export filters.
- No complete i18n or asset bundle pipeline.

## Acceptance Criteria

- `Documentation/data-design.md` exists and describes source sheet data, typed datasets, data exporters, runtime formats, special exporters, context tags, filtering, and tool-only fields.
- `Documentation/glossary.md` includes Data pipeline, runtime data format, special exporter, export context, filtering, and tool-only field terminology.
- `Documentation/meta-design.md` references tool-only/export field semantics and Data design.
- `Documentation/export-design.md` references runtime data formats, special exporters, and context tags.
- `Documentation/README.md` links to Data design and this feature spec.
- `AGENTS.md` links to Data design and this feature spec.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
