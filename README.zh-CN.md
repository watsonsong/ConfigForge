[English](README.md) | [简体中文](README.zh-CN.md)

[![CI Status](https://github.com/watsonsong/ConfigForge/actions/workflows/ci.yml/badge.svg)](https://github.com/watsonsong/ConfigForge/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

# ConfigForge

ConfigForge 是使用 Rust 开发的 schema 驱动游戏配置导出工具。通过 TOML Meta 定义数据结构，校验策划维护的 CSV/XLSX 数据，并从同一份定义生成运行时数据、类型安全的访问代码和辅助文档。

## 核心能力

- 表、枚举、结构体、标量列表、多态 fragment、索引、外键、默认值和 target tags。
- 本地 CSV/XLSX 输入，可按既有多源和 override 规则混用；远端 Sheet provider 为未来扩展。
- target-aware 校验、资源引用检查和补齐，以及范围、正则、字符串长度、条件必填、比较、求和、单调性等规则。
- JSON、MessagePack、ConfigForge binary 运行时数据；Lua 内嵌数据/代码；gettext POT 和资源清单。CSV 输出用于资源清单与验证报告。
- Rust、C++、Unreal C++、C#、Java、TypeScript 访问代码；Lua 内嵌数据附带访问 helper。各 backend 边界见 [能力矩阵](Documentation/Implementation/09-codegen-module-detail-design.md#941-codegen-backend-capability-matrix)。
- HTML/Markdown/JSON/CSV 验证报告与 Markdown/HTML schema 文档，包含表关系和 source map。

## 快速上手

在仓库根目录运行标准参考工程。Cargo 使用 `rust-toolchain.toml` 固定的 Rust 工具链，安装方式见 [开发环境](Documentation/Tech/development-environment.md)。

```bash
git clone https://github.com/watsonsong/ConfigForge.git
cd ConfigForge
cargo run -- --project tests/fixtures/standard_project/ConfigForge.toml --target client --dry-run
cargo run -- --project tests/fixtures/standard_project/ConfigForge.toml --target client
```

`client` target 将其配置的数据、代码、报告、文档、资源清单和 POT 写入 `tests/fixtures/standard_project/out/`。产物目录由 `[[targets.exports]].output` 声明，相对工程配置解析；`--report-output` 仅指定附加验证报告的文件。`--dry-run` 执行校验和生成，但不写产物。

仅生成 schema 文档，无需读取数据源：

```bash
cargo run -- --project tests/fixtures/standard_project/ConfigForge.toml --target schema_docs
```

运行 [XLSX 示例](tests/fixtures/xlsx/README.md)：

```bash
cargo run -- --project tests/fixtures/xlsx/ConfigForge.toml --target client
```

需要独立可执行文件时，使用 `cargo build --release` 或查看 [GitHub Releases](https://github.com/watsonsong/ConfigForge/releases)。`config-forge --help` 提供当前 CLI 帮助；构建通过 `--project` 和 `--target` 执行，无子命令。

## 文档与开发

- [用户工作流与 QA](Documentation/Design/07-user-manual-and-qa.md)、[配置说明](Documentation/Design/03-configuration.md)、[CLI 参考](Documentation/Design/06-export-cli.md#66-cli-总体形式)。
- [完整文档地图](Documentation/README.md)、[当前实现状态](Documentation/Design/README.md)、[Design HTML 阅读版](Documentation/Design/html/index.html)。
- [模块架构](Documentation/Implementation/01-module-outline.md)、[代码与 commit 规范](Documentation/code-style.md)、[测试/CI 矩阵](Documentation/Tech/test-ci-matrix.md)。
- [AGENTS.md](AGENTS.md) 是简短的 agent 入口索引；人工与 AI 辅助开发使用同一套设计与实现文档。

可通过 [GitHub Issues](https://github.com/watsonsong/ConfigForge/issues)反馈问题。代码变更提交前运行 `cargo fmt`、`cargo lint`、`cargo test` 和相关 runtime harness；纯文档变更遵循 [文档维护流程](Documentation/Tech/documentation-maintenance.md)。

## 开源协议

[Apache License 2.0](LICENSE)。
