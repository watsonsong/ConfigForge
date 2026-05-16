# Code Style

## Rust Conventions

- Use Rust 2021 edition.
- Keep domain logic in `src/lib.rs` modules; keep `src/main.rs` as a thin entry point.
- Prefer explicit domain errors through `ConfigForgeError` for library code.
- Use `anyhow` at CLI boundaries where human-facing error propagation is sufficient.
- Keep structs serializable/deserializable when they represent configuration or exportable data.
- Keep plugin-facing APIs behind explicit registries and stable capability IDs.
- Prefer async-capable boundaries for source loading and artifact writing.
- Keep parallel execution deterministic by sorting diagnostics and artifact manifests before reporting.

## Module Rules

- `meta` owns schema definitions.
- `source` owns input normalization.
- `validation` owns correctness checks.
- `exporter` owns generated outputs and orchestration.
- `plugin` owns dynamic plugin loading, plugin manifests, and capability registration.
- `diagnostics` owns structured diagnostics, collection, ordering, and reporting.
- `runtime` or `executor` owns shared async/parallel task execution primitives if those primitives outgrow individual modules.
- `cli` owns command-line parsing and user-facing messages.

## Testing Rules

- Add unit tests beside module logic for parsing, validation, and generation helpers.
- Add CLI-level tests under `tests/` for workflows that cross module boundaries.
- Keep a canonical integration fixture under `tests/fixtures/standard_project/`.
- Run `cargo fmt` and `cargo test` before handoff.
- After adding or changing features and tests, run unit test coverage evaluation as part of judging whether tests are complete. Use `cargo coverage` for summary and `cargo coverage-html` to inspect uncovered lines in changed modules. Treat coverage as design evidence for normal and error branch completeness, not only as a percentage target.
