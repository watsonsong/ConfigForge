# Developer Logs

Developer logs preserve raw phase-by-phase engineering context for ConfigForge.
They are not canonical design documents and are not expected to be continuously
rewritten as the project evolves.

## Purpose

Use this directory to record what the project team knew at a specific point in
time:

- important technical choices and their local context;
- major design or implementation decisions;
- temporary decisions, shortcuts, known constraints, and deferred work;
- links, documents, library references, external URLs, review notes, and other
  source material used when making decisions;
- current project status snapshots that may become useful when interpreting old
  code or later migrations.

The goal is to keep durable raw decision evidence. Future contributors should be
able to answer: "Why did the project choose this path at that time?" even if the
canonical `Design/` and `Implementation/` documents have since changed.

## Relationship to Other Documentation

- `Design/` is the current canonical product behavior and user-visible design.
- `Implementation/` is the current implementation design and module plan.
- `Tech/` is current technical practice, tooling, and workflow guidance.
- `DevLogs/` is historical raw context. Do not treat old logs as the latest
  truth unless they are also reflected in the canonical documents.


## Historical Path Warning

历史日志中的文档路径、文件编号和实现状态可能已经迁移或过期。当前导航入口以 [`../README.md`](../README.md)、[`../Design/README.md`](../Design/README.md) 和 [`../Implementation/01-module-outline.md`](../Implementation/01-module-outline.md) 为准；DevLogs 只作为历史证据，不作为当前设计入口。

## File Naming

Create one file per meaningful development phase:

```text
phase-0001.md
phase-0002.md
phase-0003.md
```

Use zero-padded sequential numbers. Do not rename old phase logs after they have
been referenced by commits, reviews, or tasks.

## Recommended Structure

Each phase log should include the sections that are useful for that phase:

- **Timebox** — approximate date or period.
- **Scope** — what changed or was evaluated during the phase.
- **Status Snapshot** — current implementation, tests, limitations, and known
  risks.
- **Decisions** — decisions made at that time, including temporary decisions.
- **Rationale** — why those decisions were reasonable with the available
  information.
- **Alternatives Considered** — options rejected or deferred, with reasons.
- **Source Material** — internal documents, review notes, external URLs, crate
  documentation, standards, or other references.
- **Follow-up Questions** — unresolved topics that should be revisited later.

## Writing Rules

- Prefer factual snapshots over polished final design language.
- Preserve uncertainty. If a decision is temporary, explicitly mark it as such.
- Record source URLs and document paths near the decision they informed.
- Link to canonical documents when a decision has already been promoted there.
- Do not silently rewrite old logs to match new design. If the understanding
  changes, create a later phase log and explain the change.
- Keep logs readable, but do not over-normalize them; they are allowed to contain
  raw notes, rejected ideas, and historical context.
- Use English for code symbols, commit subjects, crate names, URLs, and command
  snippets. Narrative text may follow the repository documentation language used
  by the surrounding document.
