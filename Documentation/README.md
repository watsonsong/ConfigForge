# Documentation

This directory contains durable project knowledge for ConfigForge.

## Structure

- `code-style.md` records Rust style, module, and testing conventions.
- `glossary.md` records project terminology, Chinese/English naming, and recommended code symbols.
- `Design/` stores the consolidated canonical design set for product behavior, execution stages, configuration, Meta model, validation/export semantics, CLI, user workflows, and QA input.
- `Implementation/` stores program implementation design, module boundaries, detailed module plans, and test organization guidance.
- `Tech/` stores technical practice documents, including test coverage evaluation and engineering workflow notes.
- `DevLogs/` stores raw phase-by-phase development logs for historical decision context, temporary decisions, and source material references.

## How AI Agents Should Use These Docs

`../AGENTS.md` is the short workspace entry point. This file is the durable documentation map. For future development work, choose documents by intent:

| Intent | Read first | Then read |
| --- | --- | --- |
| Understand current product behavior | `Design/README.md` | The relevant `Design/0*.md` topic file. |
| Change module boundaries or dependencies | `Implementation/01-module-outline.md` | The affected module documents under `Implementation/02`～`09`. |
| Add or modify a feature | Relevant `Design/*.md` | Relevant `Implementation/*.md`, plus `code-style.md`. |
| Work on concurrency, resource limits, or execution strategy | `Implementation/10-build-module-detail-design.md` | `Tech/task-system-design.md`. |
| Work on testing quality | `code-style.md` | `Tech/unit-test-coverage.md`. |
| Research historical reasoning | Current `Design/` / `Implementation/` docs | `DevLogs/` only as supporting evidence. |

Canonical priority is: `Design/` for user-visible behavior, `Implementation/` for Rust module structure, `Tech/` for engineering workflow and tooling, `DevLogs/` for historical context only.

## Canonical Design Set

For future technical overview design, detailed design, user manual writing, and QA test planning, use the consolidated documents under `Design/`:

- `Design/README.md` — consolidated design index and current confirmed decisions.
- `Design/01-overview.md` — tool purpose, users, solved problems, terms, and scope.
- `Design/02-execution-pipeline.md` — execution phases from CLI/config resolution to diagnostics and artifacts.
- `Design/03-configuration.md` — project, Meta, source, target, plugin, and field policy configuration.
- `Design/04-meta-model.md` — canonical Meta model for tables, fields, types, indexes, foreign keys, multi-source, and fragments.
- `Design/05-data-validation.md` — data views, filtering, validation semantics, aggregate data exporter inputs, and diagnostics.
- `Design/06-export-cli.md` — export targets, data export formats, artifact model, and CLI commands.
- `Design/07-user-manual-and-qa.md` — user workflows, common operations, QA dimensions, and assertion guidance.

## Implementation Design Set

Use the implementation documents after product behavior has been confirmed in `Design/`:

- `Implementation/01-module-outline.md` — Rust module boundaries, one-way dependency rules, execution pipeline mapping, and implementation document index.
- `Implementation/02-diagnostic-module-detail-design.md` — Shared diagnostic foundations and crate error bridge.
- `Implementation/03-project-module-detail-design.md` — Project Config loading, imports, path normalization, target aggregation, and Meta extraction.
- `Implementation/04-meta-module-detail-design.md` — Meta module data structures, Load-then-Link strategy, main APIs, migration path, and unit test plan.
- `Implementation/05-data-module-detail-design.md` — Data load/model/resolver flow, CSV readers, ObjectTable source layout, and DataSet design.
- `Implementation/06-validation-module-detail-design.md` — Validation flow, registered validator framework, diagnostics, ExportValidationView, and test plan.
- `Implementation/07-artifact-module-detail-design.md` — Artifact model, writer, dry-run, direct/temp-commit strategy, and write result design.
- `Implementation/08-exporter-module-detail-design.md` — Runtime data exporter contracts, format registry, JSON / MessagePack output, and ObjectTable payload shape.
- `Implementation/09-codegen-module-detail-design.md` — Codegen IR, generator contracts, runtime reader contract, and multi-language output design.
- `Implementation/10-build-module-detail-design.md` — Build orchestration, target normalization, data/code task binding, and artifact write boundaries.
- `Implementation/11-foreign-key-detail-design.md` — Foreign key raw/link model, referential-integrity validation, target semantics, codegen staging, and test plan.
- `Implementation/12-struct-expanded-value-detail-design.md` — Struct source layout auto detection and expanded CSV field-path layout for CollectionTable/ObjectTable, validation/export plan, and test plan.
- `Implementation/13-runtime-binary-format-detail-design.md` — Planned `bin` runtime data packed binary format, schema revision, encoding rules, exporter/codegen/reader integration, and compliance fixtures.

## Technical Practice Set

- `Tech/unit-test-coverage.md` — Rust unit test coverage tooling, commands, evaluation criteria, and current coverage baseline.
- `Tech/task-system-design.md` — Task system, async I/O, CPU worker pool, resource limits, and phased migration design.
- `Tech/cpp-runtime-dependencies.md` — C++ runtime reader 三方依赖的 CMake 查找、自动下载、target 接入与新增 reader checklist。

## Developer Logs

- `DevLogs/Readme.md` — developer log purpose, relationship to canonical documents, file naming, and writing rules.
- `DevLogs/phase-0001.md` — first raw phase log covering foundation, targets, fragments, foreign keys, and workflow decisions through 2026-05-21.
- `DevLogs/phase-0002.md` — second raw phase log covering runtime helpers, module boundary cleanup, Artifact Writer, dry-run/write-mode, and documentation maintenance through 2026-05-24.

## Maintenance Rules

- Keep this directory focused on decisions that future contributors need.
- Put concise navigation and project summaries in `../AGENTS.md`.
- Update the consolidated `Design/` documents whenever product behavior is introduced, redesigned, or meaningfully expanded.
- Update `Implementation/01-module-outline.md` and the affected module document whenever source module boundaries, dependency direction, or public APIs change.
- Update `Tech/` documents when commands, toolchain requirements, concurrency/resource policies, or test/coverage workflows change.
- When introducing new domain concepts, update `glossary.md` before or together with feature specs.
- Keep canonical `Design/`, `Implementation/`, and `Tech/` documents focused on current state and planned future state; put historical reasoning, removed approaches, and temporary transition notes in `DevLogs/`.
- Use `DevLogs/` for phase snapshots and raw decision evidence; do not rewrite old logs as if they were canonical current design.
