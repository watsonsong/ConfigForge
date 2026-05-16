# Feature Spec 0006: Extensible Source Provider Design

## Summary

Extend source declarations so one Meta table can read data from different provider backends, including local CSV, local XLSX, Lark/Feishu Sheet, Google Sheet, and future plugin providers.

## Documentation Scope

This feature is a documentation-only design step. It updates the glossary and Meta design so later implementation can separate table shape from source loading, while keeping all providers normalized into the same logical Sheet model.

Related documents:

- `Documentation/glossary.md`
- `Documentation/meta-design.md`
- `Documentation/features/0004-multi-source-table-design.md`

## Requirements

- Meta source declarations must support a provider field.
- Local sources may support CSV and XLSX.
- Remote sources may support Lark/Feishu Sheet and Google Sheet.
- Provider-specific locators should be explicit, such as `path`, `uri`, `sheet`, and `range`.
- Remote providers should reference credentials through `auth_ref`; secrets must not be stored in Meta files.
- Source providers must normalize physical input into the same sheet abstraction: rows, columns, headers, and cells.
- `sources` and `override_sources` should both support the same provider declaration shape.
- Simple string `source = "items.csv"` may remain a compatibility shorthand for local CSV inferred from file extension.

## Source Declaration Shape

Recommended fields:

- `provider`: `csv`, `xlsx`, `lark_sheet`, `google_sheet`, or future provider name.
- `path`: local path for local file providers.
- `uri`: provider-specific remote locator.
- `sheet`: workbook/sheet/tab name.
- `range`: optional range selector.
- `auth_ref`: external credential reference.
- `layer` / `priority`: source composition metadata for override sources.

Example:

```toml
sources = [
  { provider = "csv", path = "items/base.csv" },
  { provider = "xlsx", path = "items/events.xlsx", sheet = "Items" },
]

override_sources = [
  { provider = "google_sheet", uri = "https://docs.google.com/spreadsheets/d/...", sheet = "Items", range = "A1:K500", auth_ref = "google-prod", priority = 20 },
]
```

## Provider Validation

Validation should check:

- provider name is supported;
- required locator fields are present;
- local files exist for local providers;
- remote sources have credential references when required;
- selected sheet/range exists;
- loaded data can normalize into the common Sheet model.

## Non-Goals

- No implementation in `src/`.
- No test fixture changes in `tests/`.
- No changes to `Cargo.toml` or `Cargo.lock`.
- No actual Lark/Google authentication integration.
- No final plugin loading mechanism.
- No secrets or credentials in Meta files.

## Acceptance Criteria

- `Documentation/glossary.md` includes terms for source provider, source URI, local file source, CSV source, XLSX source, Lark/Feishu Sheet source, Google Sheet source, source auth reference, and source range.
- `Documentation/meta-design.md` describes provider-based source declarations, TOML/XML examples, provider extensibility, provider validation, and credential-reference rules.
- `Documentation/README.md` links to this feature spec.
- `AGENTS.md` links to this feature spec.
- No `src/`, `tests/`, `Cargo.toml`, or `Cargo.lock` files are changed as part of this documentation-only feature.
