# Documentation

This directory contains durable project knowledge for ConfigForge.

## Structure

- [`code-style.md`](code-style.md) records Rust style, module, and testing conventions.
- [`glossary.md`](glossary.md) records project terminology, Chinese/English naming, and recommended code symbols.
- [`Design/`](Design/) stores the consolidated canonical design set for product behavior, execution stages, configuration, Meta model, validation/export semantics, CLI, user workflows, and QA input.
- [`Implementation/`](Implementation/) stores program implementation design, module boundaries, detailed module plans, and test organization guidance.
- [`Tech/`](Tech/) stores technical practice documents, including test coverage evaluation and engineering workflow notes.
- [`Todo/`](Todo/) stores short-lived implementation task lists and step plans before they are promoted into canonical design documents.
- [`DevLogs/`](DevLogs/) stores raw phase-by-phase development logs for historical decision context, temporary decisions, and source material references.

## How AI Agents Should Use These Docs

[`../AGENTS.md`](../AGENTS.md) is the short workspace entry point. This file is the durable documentation map. For future development work, choose documents by intent:

| Intent | Read first | Then read |
| --- | --- | --- |
| Understand current product behavior | [`Design/README.md`](Design/README.md) | The relevant [`Design/0*.md`](Design/) topic file. |
| Change module boundaries or dependencies | [`Implementation/01-module-outline.md`](Implementation/01-module-outline.md) | The affected module documents under [`Implementation/`](Implementation/). |
| Add or modify a feature | Relevant `Design/*.md` | Relevant `Implementation/*.md`, plus [`code-style.md`](code-style.md). |
| Work on concurrency, resource limits, or execution strategy | [`Implementation/10-build-module-detail-design.md`](Implementation/10-build-module-detail-design.md) | [`Tech/task-system-design.md`](Tech/task-system-design.md). |
| Work on testing quality | [`code-style.md`](code-style.md) | [`Tech/unit-test-coverage.md`](Tech/unit-test-coverage.md). |
| Research historical reasoning | Current [`Design/`](Design/) / [`Implementation/`](Implementation/) docs | [`DevLogs/`](DevLogs/) only as supporting evidence. |

Canonical priority is: `Design/` for user-visible behavior, `Implementation/` for Rust module structure, `Tech/` for engineering workflow and tooling, `DevLogs/` for historical context only.

## Canonical Design Set

For future technical overview design, detailed design, user manual writing, and QA test planning, use the consolidated documents under `Design/`:

- [`Design/README.md`](Design/README.md) — consolidated design index and current confirmed decisions.
- [`Design/01-overview.md`](Design/01-overview.md) — tool purpose, users, solved problems, terms, and scope.
- [`Design/02-execution-pipeline.md`](Design/02-execution-pipeline.md) — execution phases from CLI/config resolution to diagnostics and artifacts.
- [`Design/03-configuration.md`](Design/03-configuration.md) — project, Meta, source, target, plugin, and field policy configuration.
- [`Design/04-meta-model.md`](Design/04-meta-model.md) — canonical Meta model for tables, fields, types, indexes, foreign keys, multi-source, and fragments.
- [`Design/05-data-validation.md`](Design/05-data-validation.md) — data views, filtering, validation semantics, aggregate data exporter inputs, and diagnostics.
- [`Design/06-export-cli.md`](Design/06-export-cli.md) — export targets, data export formats, artifact model, and CLI commands.
- [`Design/07-user-manual-and-qa.md`](Design/07-user-manual-and-qa.md) — user workflows, common operations, QA dimensions, and assertion guidance.

## Implementation Design Set

Use the implementation documents after product behavior has been confirmed in `Design/`:

- [`Implementation/01-module-outline.md`](Implementation/01-module-outline.md) — Rust module boundaries, one-way dependency rules, execution pipeline mapping, and implementation document index.
- [`Implementation/02-diagnostic-module-detail-design.md`](Implementation/02-diagnostic-module-detail-design.md) — Shared diagnostic foundations and crate error bridge.
- [`Implementation/03-project-module-detail-design.md`](Implementation/03-project-module-detail-design.md) — Project Config loading, imports, path normalization, target aggregation, and Meta extraction.
- [`Implementation/04-meta-module-detail-design.md`](Implementation/04-meta-module-detail-design.md) — Meta module data structures, Load-then-Link strategy, main APIs, migration path, and unit test plan.
- [`Implementation/05-data-module-detail-design.md`](Implementation/05-data-module-detail-design.md) — Data load/model/resolver flow, CSV readers, ObjectTable source layout, and DataSet design.
- [`Implementation/06-validation-module-detail-design.md`](Implementation/06-validation-module-detail-design.md) — Validation flow, registered validator framework, diagnostics, ExportValidationView, and test plan.
- [`Implementation/07-artifact-module-detail-design.md`](Implementation/07-artifact-module-detail-design.md) — Artifact model, writer, dry-run, direct/temp-commit strategy, and write result design.
- [`Implementation/08-exporter-module-detail-design.md`](Implementation/08-exporter-module-detail-design.md) — Runtime/aggregate data exporter contracts, format registry, JSON / MessagePack / bin / POT output, and ObjectTable payload shape.
- [`Implementation/09-codegen-module-detail-design.md`](Implementation/09-codegen-module-detail-design.md) — Codegen module overview, source layout, split-topic index, and current status summary.
  - [`Implementation/09a-codegen-runtime-reader-contract.md`](Implementation/09a-codegen-runtime-reader-contract.md) — Runtime loader/reader contract, eager/lazy loading, TypeScript sync API, logical table names, and schema revision coordination.
  - [`Implementation/09b-codegen-rust-backend.md`](Implementation/09b-codegen-rust-backend.md) — Rust backend generated API and lazy loading.
  - [`Implementation/09c-codegen-cpp-backend.md`](Implementation/09c-codegen-cpp-backend.md) — C++ backend generated API, ownership, namespace, and lazy loading.
  - [`Implementation/09d-codegen-typescript-backend.md`](Implementation/09d-codegen-typescript-backend.md) — TypeScript backend generated API, async/sync loading, and lazy loading.
  - [`Implementation/09e-codegen-runtime-adapters.md`](Implementation/09e-codegen-runtime-adapters.md) — Standard runtime adapters, C++ CMake, TS packages, Rust templates, and compliance fixtures.
  - [`Implementation/09f-codegen-core-model-and-options.md`](Implementation/09f-codegen-core-model-and-options.md) — Core model, `CodeExportTarget` / `CodeExportInput`, CodeModel IR, naming, type mapping, and writer options.
  - [`Implementation/09g-codegen-index-and-relation-helpers.md`](Implementation/09g-codegen-index-and-relation-helpers.md) — Index lookup and foreign-key relation helper design.
  - [`Implementation/09h-codegen-generator-api-and-tests.md`](Implementation/09h-codegen-generator-api-and-tests.md) — Generator API, new-language onboarding, diagnostics, file organization, and tests.
  - [`Implementation/09i-lua-embedded-data-generator.md`](Implementation/09i-lua-embedded-data-generator.md) — Implemented Lua data+code combined export, table file layout, row array/metatable memory strategy, indexes, and implementation notes.
  - [`Implementation/09j-codegen-cpp-unreal-backend.md`](Implementation/09j-codegen-cpp-unreal-backend.md) — Implemented Unreal C++ backend, containers / Blueprint support, shim testing, and historical implementation steps.
  - [`Implementation/09k-codegen-csharp-backend.md`](Implementation/09k-codegen-csharp-backend.md) — Implemented C# backend, generated API, runtime reader contract, index/relation/text/lazy design, and test strategy.
  - [`Implementation/09l-codegen-java-backend.md`](Implementation/09l-codegen-java-backend.md) — Implemented Java backend, single-file nested public API, runtime reader contract, index/relation/text/lazy design, and test strategy.
- [`Implementation/10-build-module-detail-design.md`](Implementation/10-build-module-detail-design.md) — Build orchestration, target normalization, export task DAG execution, and artifact write boundaries.
- [`Implementation/11-foreign-key-detail-design.md`](Implementation/11-foreign-key-detail-design.md) — Foreign key raw/link model, referential-integrity validation, target semantics, codegen staging, and test plan.
- [`Implementation/12-struct-expanded-value-detail-design.md`](Implementation/12-struct-expanded-value-detail-design.md) — Struct source layout auto detection and expanded CSV field-path layout for CollectionTable/ObjectTable, validation/export plan, and test plan.
- [`Implementation/13-runtime-binary-format-detail-design.md`](Implementation/13-runtime-binary-format-detail-design.md) — Implemented `bin` runtime data packed binary format, schema revision, encoding rules, exporter/codegen/reader integration, and compliance fixtures.

## Technical Practice Set

- [`Tech/unit-test-coverage.md`](Tech/unit-test-coverage.md) — Rust unit test coverage tooling, commands, evaluation criteria, and current coverage baseline.
- [`Tech/task-system-design.md`](Tech/task-system-design.md) — Task system, async I/O, CPU worker pool, resource limits, and phased migration design.
- [`Tech/cpp-runtime-dependencies.md`](Tech/cpp-runtime-dependencies.md) — C++ runtime reader 三方依赖的 CMake 查找、自动下载、target 接入与新增 reader checklist。
- [`Tech/platform-filesystem-semantics.md`](Tech/platform-filesystem-semantics.md) — macOS / Linux / Windows 文件系统大小写、symlink / junction 与 artifact path conflict key 策略。

## Developer Logs

- [`DevLogs/README.md`](DevLogs/README.md) — developer log purpose, relationship to canonical documents, historical path warning, file naming, and writing rules.
- [`DevLogs/phase-0001.md`](DevLogs/phase-0001.md) — first raw phase log covering foundation, targets, fragments, foreign keys, and workflow decisions through 2026-05-21.
- [`DevLogs/phase-0002.md`](DevLogs/phase-0002.md) — second raw phase log covering runtime helpers, module boundary cleanup, Artifact Writer, dry-run/write-mode, and documentation maintenance through 2026-05-24.

## Todo / Task Lists

- `Documentation/Todo/` is the expected home for short-lived task lists, phased implementation checklists, and tactical TODO documents.
- Todo documents are not canonical product / implementation contracts. Once a plan is implemented or stabilized, promote the durable design into the relevant `Design/`, `Implementation/`, or `Tech/` document, and either archive/remove the obsolete TODO or convert it into a historical implementation record with a non-TODO name.
- Do not place new TODO checklists under `Documentation/Implementation/`; Implementation documents should describe current architecture, durable implementation design, and future extension sections.
- Every phased TODO step must include a Markdown checkbox in its step heading, for example `## [ ] Step 1：...` or `## [x] Step 1：...`. Each step must be scoped so it can be developed independently, validated independently, and committed independently; mark the checkbox as complete only after that step has passed its documented validation and has been committed.

## Documentation Writing Standards

Use these rules when creating or updating documents in this directory:

1. **Choose one canonical home per topic.** Product behavior belongs in [`Design/`](Design/), implementation structure belongs in [`Implementation/`](Implementation/), engineering workflow belongs in [`Tech/`](Tech/), and historical reasoning belongs in [`DevLogs/`](DevLogs/). Do not duplicate the same canonical explanation in multiple places; cross-link instead.
2. **Keep current state and target state explicit.** Use the status terms from [`Design/README.md`](Design/README.md): `Stable`, `Implemented MVP`, `Partial`, and `Planned / Not implemented`. Do not describe an implemented feature as future work, and do not present target-state behavior as current behavior.
3. **Prefer Markdown links for navigation.** Entry pages and “Related docs” sections should use clickable links rather than only code-formatted paths. Use repo-relative links inside `Documentation/`.
4. **Reference code precisely when reviewing implementation coverage.** When a document discusses concrete source behavior, include `file_path:line_number` references where useful so future readers can verify the claim.
5. **Keep examples executable against the current CLI and config schema.** TOML examples must match the current flattened / nested option rules, path resolution rules, and supported fields. CLI examples must match the current binary interface.
6. **Separate current contract from future extensions.** Use headings or tables such as “Current behavior”, “Current limitations”, and “Future extensions” when a topic includes both implemented behavior and planned design.
7. **Control document size.** When a document grows beyond roughly 600 lines, add a table of contents and “Related docs”; when it grows beyond roughly 900 lines, prefer splitting it into focused topic files with an index document.
8. **Avoid hidden canonical areas.** Do not add new long-lived canonical directories under `Documentation/` without listing them in this README. Short-lived task lists and tactical TODO documents belong under `Documentation/Todo/`; durable design must be promoted into `Design/`, `Implementation/`, or `Tech/`.
9. **Update indexes together with topic changes.** When adding, renaming, splitting, or retiring a document, update [`../AGENTS.md`](../AGENTS.md), this README, and any affected directory index in the same change.
10. **Treat DevLogs as evidence, not authority.** Phase logs may contain old paths, old names, or superseded decisions. Always resolve current behavior through `Design/`, `Implementation/`, `Tech/`, and source code.

## Maintenance Rules

- Keep this directory focused on decisions that future contributors need.
- Put concise navigation and project summaries in [`../AGENTS.md`](../AGENTS.md).
- Update the consolidated `Design/` documents whenever product behavior is introduced, redesigned, or meaningfully expanded.
- Update `Implementation/01-module-outline.md` and the affected module document whenever source module boundaries, dependency direction, or public APIs change.
- Update `Tech/` documents when commands, toolchain requirements, concurrency/resource policies, or test/coverage workflows change.
- When introducing new domain concepts, update [`glossary.md`](glossary.md) before or together with feature specs.
- Keep canonical `Design/`, `Implementation/`, and `Tech/` documents focused on current state and planned future state; put historical reasoning, removed approaches, and temporary transition notes in `DevLogs/`.
- Use `DevLogs/` for phase snapshots and raw decision evidence; do not rewrite old logs as if they were canonical current design.
