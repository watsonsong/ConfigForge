# Documentation

This directory contains durable project knowledge for ConfigForge.

## Structure

- `code-style.md` records Rust style, module, and testing conventions.
- `glossary.md` records project terminology, Chinese/English naming, and recommended code symbols.
- `Design/` stores the consolidated canonical design set for product behavior, execution stages, configuration, Meta model, validation/export semantics, CLI, user workflows, and QA input.
- `Implementation/` stores program implementation design, module boundaries, detailed module plans, and test organization guidance.
- `Tech/` stores technical practice documents, including test coverage evaluation and engineering workflow notes.

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

## Implementation Design Set

Use the implementation documents after product behavior has been confirmed in `Design/`:

- `Implementation/01-module-outline.md` — Rust module boundaries, dependency direction, execution pipeline mapping, and first-stage completion criteria.
- `Implementation/02-meta-module-detail-design.md` — Meta module data structures, Rust inheritance/composition strategy, main APIs, migration path, and unit test plan.
- `Implementation/03-data-module-detail-design.md` — Data load/model/validation/exporter flow design.
- `Implementation/04-code-export-module-detail-design.md` — Code export IR, generator contracts, and multi-language output design.
- `Implementation/05-validator-framework-detail-design.md` — Registered validator framework, Meta validator configuration, built-in validator migration, and test plan.

## Technical Practice Set

- `Tech/unit-test-coverage.md` — Rust unit test coverage tooling, commands, evaluation criteria, and current coverage baseline.
- `Tech/task-system-design.md` — Task system, async I/O, CPU worker pool, resource limits, and phased migration design.

## Maintenance Rules

- Keep this directory focused on decisions that future contributors need.
- Put concise navigation and project summaries in `../AGENTS.md`.
- Update the consolidated `Design/` documents whenever product behavior is introduced, redesigned, or meaningfully expanded.
- When introducing new domain concepts, update `glossary.md` before or together with feature specs.
