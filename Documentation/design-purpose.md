# Design Purpose

ConfigForge exists to bridge game design spreadsheets and game runtime data.

## Workflow

1. Designers author data in spreadsheet-like files.
2. Engineers or tools define Meta files that describe table shape, field types, indexes, and constraints.
3. The CLI validates source data against Meta definitions.
4. The CLI exports validated runtime data and generated code into a game project.

## Initial Architecture

- `meta`: parse and represent table metadata.
- `source`: read sheet data into normalized rows.
- `validation`: check fields, types, uniqueness, indexes, and constraints.
- `exporter`: orchestrate validation and write generated artifacts.
- `plugin`: register built-in and dynamic extensions for sources, exporters, validators, generators, and builtin value types.
- `diagnostics`: collect, order, and report structured errors and warnings.
- `cli`: provide stable commands for local use and CI.

## Initial CLI Commands

- `config-forge validate --meta <META.toml> --data <DATA_DIR>` validates source data only.
- `config-forge export --meta <META.toml> --data <DATA_DIR> --out <OUT_DIR>` validates and writes generated files.

## Future Extensions

- Additional field types and constraint expressions.
- Multi-column unique indexes.
- XLSX, Google Sheets, or internal sheet providers.
- Target-specific code generators for game engines.
- Incremental export and cache-aware validation.
- Dynamic plugins for project-specific providers, exporters, validators, and builtin value types.
- Parallel validation/export and async network source loading.
