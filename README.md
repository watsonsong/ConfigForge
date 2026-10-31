[English](README.md) | [简体中文](README.zh-CN.md)
[![CI Status](https://github.com/watsonsong/ConfigForge/actions/workflows/ci.yml/badge.svg)](https://github.com/watsonsong/ConfigForge/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/config-forge.svg)](https://crates.io/crates/config-forge)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Docs](https://img.shields.io/badge/docs-latest-green.svg)](Documentation/README.md)
# ConfigForge
ConfigForge is an **AI-driven, schema-first game configuration export tool** built in Rust. It provides end-to-end configuration workflow for game projects: define schemas declaratively, validate designer-authored data, and generate type-safe runtime artifacts for 7+ languages and platforms out of the box.
Designed for AI-assisted development, you can build custom validators, exporters, code generators, or runtime adapters in minutes with the provided architecture context and AI tooling.
## ✨ Core Features
- 📋 **Schema-Driven Workflow**: Define tables, fields, types, indexes, foreign keys, and constraints using declarative TOML meta files as a single source of truth.
- **CSV and XLSX Inputs**: Read local Excel workbooks without Excel installed; select worksheets explicitly and mix CSV/XLSX sources using the existing table and override rules. See [XLSX configuration](Documentation/Design/03-configuration.md#341-本地-xlsx-输入).
- ✅ **Deterministic Validation**: Catch data errors (type mismatches, missing references, invalid values, broken asset paths) before export, never ship malformed configurations to production.
- 📦 **Multi-Format Data Export**: Supports JSON, MessagePack, optimized binary, Lua embedded tables, CSV, POT (i18n), and validation reports out of the box; easily extend to custom formats.
- 🔌 **Multi-Language Code Generation**: Generates type-safe, IDE-friendly data access code for Rust, C++ (including Unreal Engine), C#, Java, TypeScript, Lua, with more languages coming soon.
- 🛡️ **Built-in Validators**: Common game development validators included: regex, range, string length, foreign key, asset reference, field sum, monotonic sequence, conditional required, and more.
- 🎮 **Game Pipeline Friendly**: Native support for Unreal/Unity project structure, asset reference validation/path completion, asset manifest generation, target-aware configuration filtering (client/server/region-specific builds).
- 🤖 **AI-First Extensibility**: Modular architecture with clear boundaries; feed the provided AI context documents to your preferred AI assistant to build custom extensions without reading the full codebase.
## 🚀 5-Minute Quick Start
### 1. Install
```bash
# Install via cargo
cargo install config-forge
# Or download prebuilt binaries from the [Releases page](https://github.com/watsonsong/ConfigForge/releases)
```
### 2. Run the full demo
```bash
git clone https://github.com/watsonsong/ConfigForge.git
cd ConfigForge/examples/full_game_demo
config-forge export
```
### 3. Check outputs
Generated artifacts will be available in the `out/` directory, including:
- Runtime data files (JSON/MessagePack/binary/Lua)
- Type-safe access code for all supported languages
- Validation reports (HTML/Markdown/JSON/CSV)
- Auto-generated schema documentation
- Asset manifest and i18n POT files
## 📚 Documentation
- [User Manual & FAQ](Documentation/Design/07-user-manual-and-qa.md) - Complete usage guide for designers and developers
- [Architecture Documentation Index](Documentation/README.md) - Full design, implementation, and technical docs
- [AI Extension Guide](Documentation/Design/ai-extension-guide.md) - Build custom extensions with AI assistance (coming soon)
- CLI Reference: Run `config-forge --help` for full command documentation
## 🤝 Contributing
Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) (coming soon) for development environment setup, contribution guidelines, and how to use AI tooling to contribute efficiently.
- Report bugs or request features via [GitHub Issues](https://github.com/watsonsong/ConfigForge/issues)
- Submit pull requests following the code style and testing requirements defined in [Documentation/code-style.md](Documentation/code-style.md)
## 📄 License
ConfigForge is licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the full license text.