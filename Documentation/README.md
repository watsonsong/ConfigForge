# Documentation

This directory contains durable project knowledge for ConfigForge.

## Structure

- `code-style.md` records Rust style, module, and testing conventions.
- `glossary.md` records project terminology, Chinese/English naming, and recommended code symbols.
- `Design/` stores the consolidated canonical design set for product behavior, execution stages, configuration, Meta model, validation/export semantics, CLI, user workflows, and QA input.
- `Implementation/` stores program implementation design, module boundaries, detailed module plans, and test organization guidance.
- `Tech/` stores technical practice documents, including test coverage evaluation and engineering workflow notes.
- `DevLogs/` stores raw phase-by-phase development logs for historical decision context, temporary decisions, and source material references.

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

- `Implementation/01-module-outline.md` — Rust module boundaries, one-way dependency rules, execution pipeline mapping, and implementation document index.
- `Implementation/02-diagnostic-module-detail-design.md` — Shared diagnostic foundations and crate error bridge.
- `Implementation/03-project-module-detail-design.md` — Project Config loading, imports, path normalization, target aggregation, and Meta extraction.
- `Implementation/04-meta-module-detail-design.md` — Meta module data structures, Load-then-Link strategy, main APIs, migration path, and unit test plan.
- `Implementation/05-data-module-detail-design.md` — Data load/model/resolver flow, CSV readers, ObjectTable source layout, and DataSet design.
- `Implementation/06-validation-module-detail-design.md` — Validation flow, registered validator framework, diagnostics, ExportValidationView, and test plan.
- `Implementation/07-exporter-module-detail-design.md` — Runtime data exporter contracts, format registry, JSON / MessagePack output, and ObjectTable payload shape.
- `Implementation/08-codegen-module-detail-design.md` — Codegen IR, generator contracts, runtime reader contract, and multi-language output design.
- `Implementation/09-build-module-detail-design.md` — Build orchestration, target normalization, data/code task binding, and artifact write boundaries.
- `Implementation/10-foreign-key-detail-design.md` — Foreign key raw/link model, referential-integrity validation, target semantics, codegen staging, and test plan.
- `Implementation/11-struct-expanded-value-detail-design.md` — Struct source layout auto detection and expanded CSV field-path layout for CollectionTable/ObjectTable, validation/export plan, and test plan.

## Technical Practice Set

- `Tech/unit-test-coverage.md` — Rust unit test coverage tooling, commands, evaluation criteria, and current coverage baseline.
- `Tech/task-system-design.md` — Task system, async I/O, CPU worker pool, resource limits, and phased migration design.

## Developer Logs

- `DevLogs/Readme.md` — developer log purpose, relationship to canonical documents, file naming, and writing rules.
- `DevLogs/phase-0001.md` — first raw phase log covering foundation, targets, fragments, foreign keys, and workflow decisions through 2026-05-21.

## Maintenance Rules

- Keep this directory focused on decisions that future contributors need.
- Put concise navigation and project summaries in `../AGENTS.md`.
- Update the consolidated `Design/` documents whenever product behavior is introduced, redesigned, or meaningfully expanded.
- When introducing new domain concepts, update `glossary.md` before or together with feature specs.
- Use `DevLogs/` for phase snapshots and raw decision evidence; do not rewrite old logs as if they were canonical current design.
