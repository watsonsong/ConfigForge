# Documentation

This directory contains durable project knowledge for ConfigForge.

## Structure

- `code-style.md` records Rust style, module, and testing conventions.
- `glossary.md` records project terminology, Chinese/English naming, and recommended code symbols.
- `Design/` stores the consolidated canonical design set for product behavior, execution stages, configuration, Meta model, validation/export semantics, CLI, user workflows, and QA input.

## Canonical Design Set

For future technical overview design, detailed design, user manual writing, and QA test planning, use the consolidated documents under `Design/`:

- `Design/README.md` — consolidated design index and current confirmed decisions.
- `Design/01-overview.md` — tool purpose, users, solved problems, terms, and scope.
- `Design/02-execution-pipeline.md` — execution phases from CLI/config resolution to diagnostics and artifacts.
- `Design/03-configuration.md` — project, Meta, source, target, plugin, and field policy configuration.
- `Design/04-meta-model.md` — canonical Meta model for tables, fields, types, indexes, foreign keys, multi-source, and fragments.
- `Design/05-data-validation.md` — data views, filtering, validation semantics, special exporter inputs, and diagnostics.
- `Design/06-export-cli.md` — export targets, runtime data formats, special exporters, artifact model, and CLI commands.
- `Design/07-user-manual-and-qa.md` — user workflows, common operations, QA dimensions, and assertion guidance.

## Maintenance Rules

- Keep this directory focused on decisions that future contributors need.
- Put concise navigation and project summaries in `../AGENTS.md`.
- Update the consolidated `Design/` documents whenever product behavior is introduced, redesigned, or meaningfully expanded.
- When introducing new domain concepts, update `glossary.md` before or together with feature specs.
