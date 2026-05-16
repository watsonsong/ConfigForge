# Core Rules

## Source of Truth

Meta definitions are the source of truth for exported data shape. Data files and generated code must be derived from the Meta model rather than maintaining duplicate schemas.

## Deterministic Validation

All exports must validate first. Invalid data should fail with actionable errors that include table, field, and row context whenever possible.

Validation and export output must remain deterministic even when implementation uses parallel tasks. Diagnostics and artifact manifests should be sorted by stable keys before being shown or written.

## Debuggable Failure Reporting

Recoverable failures should be collected in batches. The CLI should avoid stopping at the first field, record, source, or artifact error when later independent checks can still provide useful diagnostics.

## Designer-Friendly Inputs

CSV is the first supported sheet input. Future sheet providers should normalize into the same internal row model before validation.

## Runtime-Friendly Outputs

Exports should be optimized for game runtime consumption: predictable file names, stable formats, and generated code that avoids hand-written access boilerplate.

Generated artifacts should be written through a controlled writer so output path conflicts, partial writes, and artifact manifest ordering can be handled consistently.

## Testability

Each module should expose logic that can be unit tested without invoking the CLI. End-to-end behavior should be covered through integration fixtures.
