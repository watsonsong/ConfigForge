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

## Comment Rules

- Always write code comments in English.
- Prefer comments that explain why a decision exists, not how the code executes. The implementation should make the how clear through names, types, and structure.
- Do not add comments that merely repeat expressive symbol names. If a class, function, variable, or type already states the intent, leave it uncommented.
- Keep comments concise and rare. Add them only around complex domain rules, non-obvious invariants, external constraints, or trade-offs that are difficult to infer from code alone.
- When several reasonable approaches exist, briefly document why the current approach was chosen, especially if the choice protects compatibility, determinism, performance, or future extensibility.
- Use comments to record important invariants, preconditions, and cross-module assumptions that callers or future maintainers must preserve.
- Prefer refactoring over commenting when clearer names, smaller functions, or stronger types can remove the need for explanation.
- Keep comments close to the code they explain, and update or remove them in the same change when the behavior changes. Stale comments are worse than missing comments.
- Avoid speculative roadmap comments. Use `TODO` only for actionable follow-up work with enough context to understand the trigger and intended direction.
- For public APIs, write doc comments when they define contracts, target-specific behavior, error semantics, or compatibility expectations. Avoid doc comments that only restate the item name.
- Comment surprising workarounds with the constraint that requires them, such as third-party behavior, generated format compatibility, or a documented product decision.

## Testing Rules

- Add unit tests beside module logic for parsing, validation, and generation helpers.
- Add CLI-level tests under `tests/` for workflows that cross module boundaries.
- Keep a canonical integration fixture under `tests/fixtures/standard_project/`.
- Keep integration-test generated outputs under fixture-local `out/` directories and ignore them through `.gitignore` (for example `tests/fixtures/**/out/`). Future integration fixtures that write exported artifacts should follow the same convention instead of committing generated outputs by accident.
- Run `cargo fmt`, `cargo lint`, and `cargo test` before handoff. `cargo lint` uses Clippy with all targets and all features; the project forbids `unsafe_code` and treats the default Clippy lint set as warnings.
- Use `cargo lint-strict` when evaluating broader API/design cleanup candidates. Treat strict/pedantic findings as review input rather than automatic rewrite requirements, especially for domain naming such as `MetaModel` and TOML schema structs with multiple boolean flags.
- After adding or changing features and tests, run unit test coverage evaluation as part of judging whether tests are complete. Use `cargo coverage` for summary and `cargo coverage-html` to inspect uncovered lines in changed modules. Treat coverage as design evidence for normal and error branch completeness, not only as a percentage target.

## Git Commit Message Rules

- Write commit messages in English.
- Use a concise Conventional Commits style subject:
  - `feat(scope): add new user-visible capability`
  - `fix(scope): correct broken behavior`
  - `docs(scope): update design or user-facing documentation`
  - `test(scope): add or improve tests`
  - `refactor(scope): restructure code without changing behavior`
  - `perf(scope): improve performance without changing behavior`
  - `chore(scope): update tooling, build configuration, or maintenance files`
- Use an optional scope when it clarifies the affected area, such as `meta`, `data`, `validation`, `export`, `codegen`, `fragment`, `foreign-key`, `tasks`, or `targets`.
- Keep the subject specific and action-oriented. Prefer `feat(fragment): implement variant source fragments end-to-end` over vague messages such as `fix`, `update`, or `modify code`.
- Keep the subject within roughly 72 characters when practical.
- Add a body for non-trivial commits. The body should explain what changed and why, not just repeat the subject.
- Use one blank line between the subject and body.
- Wrap body lines at roughly 72 characters.
- Mention important validation or compatibility notes in the body when relevant, for example target semantics, migration behavior, or generated output changes.
- Do not mix unrelated work in one commit. Split documentation, feature implementation, tests, and follow-up fixes when they are independently understandable.

Recommended examples:

```text
feat(foreign-key): implement FK meta, validation, and codegen IR

Add field-level and table-level FK meta, linked FK resolution, meta checks,
referential integrity validation, CodeModel FK entries, and tests.
```

```text
fix(fragment): tighten variant source mode and target validation

Centralize variant source mode checks, restrict unsupported multi-source
cases, improve visible row handling, and expand fragment codegen tests.
```

```text
docs(validation): add validator framework design

Document the registered validator framework, built-in validator migration,
meta validator configuration, and test plan.
```
