# Phase 0006 Developer Log — ObjectTable Multi-source / Override Sources Stabilization

## Timebox

- Approximate period: after `phase-0005` through 2026-06-14.
- Snapshot range observed locally:
  - after schema docs and builtin Color / Vector stabilization;
  - through ObjectTable CSV `sources` / `override_sources` promotion to Stable.

## Scope

Phase 0006 focused on moving `override_sources` from an Implemented MVP centered on CollectionTable row override to a Stable CSV contract that also covers ObjectTable multi-source materialization.

The major work was:

- allow ObjectTable to declare CSV `sources` and `override_sources` while preserving existing ObjectTable restrictions around indexes, row export, fragments, polymorphic mode, and FK target use;
- extend source planning so ObjectTable source refs use the `ObjectKeyValueRows` layout, default `data_start_row = 2`, empty override primary keys, and target-tag filtering for override sources;
- materialize ObjectTable base sources with field-path split merge into one singleton object;
- apply ObjectTable override sources as exact field-path patches against already-materialized base fields;
- preserve key-level source trace so validation errors for patched fields point at the override CSV key row;
- update standard project fixtures and CLI integration coverage to exercise default and tag-gated ObjectTable override output;
- promote the Design / Implementation documentation status for CSV `override_sources` to Stable and remove the completed TODO.

This log is historical context. The canonical behavior remains in `Documentation/Design/`, `Documentation/Implementation/`, and `Documentation/Tech/`.

## Status Snapshot

### Implemented Capabilities Added or Stabilized in This Phase

- ObjectTable source declarations:
  - `source = "...csv"` remains the compatible single-source spelling;
  - `sources = [...]` is accepted for ObjectTable CSV sources and means multiple key/value inputs for one logical singleton object;
  - `override_sources = [...]` is accepted for ObjectTable CSV sources and runs after all base sources;
  - base `sources[].export_when` remains unsupported to avoid target-specific base materialization semantics.
- ObjectTable base split merge:
  - every ObjectTable CSV source still uses `key,value` or `key,title,value` rows;
  - multiple base sources merge into one singleton object row;
  - base source field paths must be disjoint, and duplicates fail with a source-schema diagnostic suggesting `override_sources` for replacement values;
  - empty ObjectTable sources still produce a singleton object row so required/default/type validation remains deterministic.
- ObjectTable override patch:
  - override sources only replace exact field paths present in the patch source;
  - missing target field paths fail instead of silently becoming append/upsert;
  - later override sources win over earlier override sources;
  - `override_sources[].export_when` is evaluated against effective target tags before reading the source.
- Source trace and validation:
  - `CellSourceInfo` carries `source_path` as well as row/column data;
  - ObjectTable merge and override paths retag cell sources so field-level validation can report the physical source file and key row that supplied the final value;
  - downstream validation and runtime payload generation consume the final materialized singleton object without needing per-source special cases.
- Standard fixture coverage:
  - `game_rules` now demonstrates ObjectTable split base sources and a tag-gated S1 override source;
  - CLI integration asserts default output, `--tag s1` output, and JSON / MessagePack / binary final payload values.

### Verification Snapshot

Representative verification during the implementation steps included focused unit and integration tests for Meta validation, DataLoadPlan, DataLoader, validation diagnostics, data artifacts, and standard project CLI output. The final phase gate remains `cargo fmt`, `cargo test`, `cargo lint`, and `git diff --check` before considering the stabilization work complete.

### Known Constraints at the End of This Phase

- Only CSV provider behavior is Stable. `xlsx`, `lark_sheet`, and `google_sheet` remain planned / unsupported.
- Base source `export_when` remains unsupported for both CollectionTable and ObjectTable.
- CollectionTable still does not support general split merge by primary key; stable CollectionTable multi-source behavior is variant source append plus primary-index override.
- Override insert/upsert is intentionally not supported. CollectionTable override must match an existing base row; ObjectTable override must match an existing base field path.
- ObjectTable still cannot declare indexes / primary_index / row_export / fragments / polymorphic / preserve_source_order / unique field indexes and still cannot be an FK target.

## Key Decisions and Rationale

### 1. ObjectTable Multi-source Means Field-path Split Merge

Decision:

- Treat every ObjectTable source as a singleton object fragment expressed by field paths.
- Merge base sources only when their field paths are disjoint.

Rationale:

- ObjectTable has no record identity or primary index; field path is the natural address for a constant object value.
- Disjoint merge avoids ambiguity and gives duplicate field paths a clear error with a clear fix: move replacement values to `override_sources`.

Source material:

- `src/data/loader.rs`
- `src/data/tests.rs`
- `Documentation/Design/04-meta-model.md`
- `Documentation/Implementation/05-data-module-detail-design.md`

### 2. ObjectTable Override Is Patch-only, Not Upsert

Decision:

- Override sources may only patch field paths that already exist after base materialization.
- Missing target paths are errors.

Rationale:

- Allowing patch sources to add new paths would blur base split merge, target-specific base materialization, and override semantics.
- Patch-only behavior keeps target-gated hotfixes safe and predictable.

Source material:

- `src/data/loader.rs`
- `src/data/tests.rs`
- `tests/fixtures/standard_project/meta/misc.toml`

### 3. Cell Source Trace Owns ObjectTable Diagnostic Location

Decision:

- Store the physical source path in `CellSourceInfo` and prefer cell source over singleton row source for field-level validation diagnostics.
- Retag ObjectTable cells during merge and override so the final materialized value carries its true source.

Rationale:

- After multi-source merge, a singleton `RowSourceInfo` can no longer represent the source of every field in the object.
- Designers need invalid override values to point to the override file/key row, not to the first base source or logical row fallback.

Source material:

- `src/data/model.rs`
- `src/data/csv.rs`
- `src/validation/table.rs`
- `src/validation/tests.rs`

### 4. Stable Scope Remains CSV-only

Decision:

- Promote `override_sources` to Stable for CSV CollectionTable and CSV ObjectTable behavior only.
- Keep non-CSV providers, base source `export_when`, CollectionTable split merge, and override upsert out of scope.

Rationale:

- The implemented behavior now has end-to-end tests and fixture coverage.
- Expanding providers or base target selection would add new caching and materialization contracts that should be designed separately.

Source material:

- `Documentation/Design/README.md`
- `Documentation/Design/03-configuration.md`
- `Documentation/Implementation/10-build-module-detail-design.md`

## Follow-up Work

- Design non-CSV provider behavior only after sheet provider contracts exist.
- Decide whether base source `export_when` should be supported, and if so update DataSet cache keys and source-read planning semantics explicitly.
- If users need override additions, design an explicit `override_mode = "upsert"` or equivalent; do not change patch-only defaults silently.
- Continue moving validation toward resolved-value source traces so raw and resolved validation paths report identical ObjectTable key locations.
