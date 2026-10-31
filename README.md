[English](README.md) | [简体中文](README.zh-CN.md)

[![CI Status](https://github.com/watsonsong/ConfigForge/actions/workflows/ci.yml/badge.svg)](https://github.com/watsonsong/ConfigForge/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

# ConfigForge

ConfigForge is a schema-driven game configuration export tool written in Rust. Define schemas in TOML Meta files, validate designer-authored CSV/XLSX data, and generate runtime data, typed access code, and supporting documents from the same definitions.

## Features

- Tables, enums, structs, scalar lists, polymorphic fragments, indexes, foreign keys, defaults, and target tags.
- Local CSV/XLSX sources, with mixed providers under the supported multi-source and override rules. Remote Sheet providers are planned.
- Target-aware validation and asset reference checks/completion; range, regex, string length, conditional required, comparison, sum, and monotonic rules.
- JSON, MessagePack, and ConfigForge binary runtime data; Lua embedded data/code; gettext POT and asset manifests. CSV output is available for asset manifests and validation reports.
- Generated access code for Rust, C++, Unreal C++, C#, Java, and TypeScript; Lua access helpers accompany embedded data. See the [backend capability matrix](Documentation/Implementation/09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix) for each backend's scope.
- HTML/Markdown/JSON/CSV validation reports and Markdown/HTML schema documentation with table relationships and source maps.

## Quick start

Run the standard reference project from the repository root. Cargo uses the Rust toolchain pinned in `rust-toolchain.toml`; setup details are in the [development environment guide](Documentation/Tech/development-environment.md).

```bash
git clone https://github.com/watsonsong/ConfigForge.git
cd ConfigForge
cargo run -- --project tests/fixtures/standard_project/ConfigForge.toml --target client --dry-run
cargo run -- --project tests/fixtures/standard_project/ConfigForge.toml --target client
```

The `client` target writes its configured data, code, report, docs, asset manifest, and POT artifacts beneath `tests/fixtures/standard_project/out/`. Output directories come from `[[targets.exports]].output`, resolved relative to the project configuration; `--report-output` only selects the file for an additional validation report. `--dry-run` runs validation and generation without writing artifacts.

Generate schema documentation without loading source data:

```bash
cargo run -- --project tests/fixtures/standard_project/ConfigForge.toml --target schema_docs
```

Try the [XLSX fixture](tests/fixtures/xlsx/README.md):

```bash
cargo run -- --project tests/fixtures/xlsx/ConfigForge.toml --target client
```

Build a standalone executable with `cargo build --release` or check [GitHub Releases](https://github.com/watsonsong/ConfigForge/releases). Run `config-forge --help` for the current CLI; builds use `--project` and `--target` without a subcommand.

## Documentation and development

- [User workflows and QA](Documentation/Design/07-user-manual-and-qa.md), [configuration](Documentation/Design/03-configuration.md), and [CLI reference](Documentation/Design/06-export-cli.md#66-cli-总体形式).
- [Documentation map](Documentation/README.md), [feature status](Documentation/Design/README.md), and [Design HTML reading edition](Documentation/Design/html/index.html).
- [Architecture](Documentation/Implementation/01-module-outline.md), [code/commit conventions](Documentation/code-style.md), and [test/CI matrix](Documentation/Tech/test-ci-matrix.md).
- [AGENTS.md](AGENTS.md) is the short agent entry index; human and AI-assisted contributions use the same canonical docs.

Report bugs through [GitHub Issues](https://github.com/watsonsong/ConfigForge/issues). Before submitting code changes, run `cargo fmt`, `cargo lint`, and `cargo test`, including relevant runtime harnesses. Documentation-only changes follow the [documentation maintenance workflow](Documentation/Tech/documentation-maintenance.md).

## License

[Apache License 2.0](LICENSE).
