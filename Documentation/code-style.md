# Code Style

## Rust Conventions

- Use Rust 2024 edition for ConfigForge tool code.
- Require Rust 1.85+ as the minimum supported toolchain for the ConfigForge crate.
- Keep generated Rust code Rust 2021-compatible by default unless a target option explicitly selects Rust 2024-only output.
- Keep domain logic in `src/lib.rs` modules; keep `src/main.rs` as a thin entry point.
- Prefer explicit domain errors through `ConfigForgeError` for library code.
- Use `anyhow` at CLI boundaries where human-facing error propagation is sufficient.
- Keep structs serializable/deserializable when they represent configuration or exportable data.
- Keep plugin-facing APIs behind explicit registries and stable capability IDs.
- Prefer the smallest practical public API. Use private `mod` plus explicit `pub use` facades
  instead of `pub mod` when submodules are implementation organization. Do not expose struct
  fields, helper functions, concrete modules, or nested namespaces unless callers need them for a
  stable contract, configuration schema, plugin extension point, or testable cross-module result.
  Start with private visibility, widen to `pub(crate)` or `pub` only when a real caller requires it.
- Prefer async-capable boundaries for source loading and artifact writing.
- Keep parallel execution deterministic by sorting diagnostics and artifact manifests before reporting.

## General Code Size Rules

- Keep source files small enough to review and reason about. With a reasonable design, any
  non-test source file over roughly 800 lines should trigger extra scrutiny for cohesion,
  naming, and hidden submodules; over roughly 1,000 lines should normally prompt a module
  split or a clear documented reason to keep it together.
- Unit tests, integration tests, fixtures, and generated or vendored artifacts can be looser:
  when the structure is repetitive but clear, test files of several thousand lines may be
  acceptable.
- Prefer keeping code lines within roughly 100 characters, but prioritize readability over
  mechanical wrapping. Break long expressions, fluent chains, generic bounds, and data
  literals according to the target language's idioms; if a language lacks a clear split point,
  or forced line breaks make the code harder to read, keeping a longer line is acceptable.
  This commonly applies to text-rendering string templates.

## Module Rules

- `diagnostic` owns shared diagnostic foundations and the crate error facade.
- `project` owns Project Config loading, imports, path normalization, target aggregation, and Meta extraction.
- `meta` owns schema definitions, Load-then-Link, linked views, and Meta structural diagnostics.
- `data` owns source loading, CSV readers, raw `DataSet`, row source trace, and Meta-aware resolving.
- `validation` owns correctness checks, target-aware validation views, `ValidationReport`, and validator registry.
- `exporter` owns runtime data export contracts, export type registry, JSON / MessagePack payloads, and future export type plugins.
- `codegen` owns language-neutral CodeModel IR and built-in generated-code backends; backend list and capabilities are tracked in `Documentation/Implementation/09-codegen-module-detail-design.md`.
- `build` owns project build orchestration, target normalization, export dependency branch pipeline execution, and artifact scheduling/writes.
- `cli` owns command-line parsing and user-facing messages. It should call `build` rather than orchestrating lower-level modules directly.
- Future plugin or executor modules should follow the one-way dependency rules in `Documentation/Implementation/01-module-outline.md`.
- Public module roots should act as narrow facades. Internal files may be split freely for cohesion,
  but those files should normally stay private and be exposed through selected root-level re-exports.

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
- Use `Documentation/Tech/development-environment.md` for required Rust tools, optional cross-language harness tools, runtime adapter dependencies, and the rule that local Rust development must not require every target-language environment.

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
