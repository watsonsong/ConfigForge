# Diagnostics 修复验收记录（2026-10-31）

本记录是本地验证快照。当前契约见 [Diagnostic 实现设计](../Implementation/02-diagnostic-module-detail-design.md)和 [Designer Report](../Design/10-designer-validation-report.md)，后续行为变化不回写本快照。

## 修改范围

- 保留 validation 主错误和 report / trace 附加错误；export / sink finish 失败保留已有 warning。
- CLI 成功、业务失败和参数解析失败使用 version 1 JSON；I/O、TOML、CSV、序列化错误提供稳定 code。
- Asset regex warning 收集；regex/sum 的 current/expected；独立 group / asset / FK context。
- Meta 声明来源、关联位置、raw record key；报告可见性、根内相对路径和四格式传递。

## 本地验证

使用仓库固定的 Rust 1.99.1：

- `cargo fmt --check`、`cargo lint` 通过。
- `cargo test` 全部通过：570 个库测试、79 个 CLI integration 测试、5 个架构检查，以及其余 45 个 runtime compliance / metadata 测试；合计 699 个测试。跨语言工具要求按当前 local profile，不能据此替代 full CI 环境验收。
- 新增 diagnostics CLI 回归 8 个，覆盖主错误与附加写失败、warning 成功 / export 失败、I/O / TOML / argv、多文件声明、重复记录和 Asset warning。
- `npm run docs:build --prefix tools/docs` 与 `npm run docs:check --prefix tools/docs` 通过；Design HTML 同步，检查 52 份当前 Markdown 的 528 个本地链接。

覆盖率样本由库测试与上述 8 个 CLI 回归累计，不代表完整 runtime 测试覆盖率：

```powershell
cargo llvm-cov --all-features --no-report --lib
cargo llvm-cov --all-features --no-report --test cli_integration -- diagnostics::
cargo llvm-cov report --summary-only
cargo llvm-cov report --html --output-dir target/diagnostics-coverage
```

| 模块 | 行覆盖率 |
| --- | ---: |
| diagnostic/error.rs | 91.01% |
| diagnostic/mod.rs | 85.87% |
| meta/provenance.rs | 98.48% |
| validation/report/model.rs | 93.43% |
| validation/report/render.rs | 94.76% |

逐行检查 HTML 后，补充了 Meta/CSV parse 的结构化坐标和 fragment / 无 target / 未知路径的可见性测试。剩余未覆盖行主要包括 legacy 错误兼容、格式转换辅助分支、测试失败断言和 provenance 提取失败回退；后者在正常流程中由前置 schema parse 阻断。没有通过增加与实现镜像的测试追求全覆盖。

## 保留的边界

声明定位仍是 name 起始位置，不是完整 TOML token span；报告尚不统计 row_filtered 或已检查记录数。owner/domain 项目映射和可配置报告分组仍为设计扩展。共享 ArtifactSink 进入失败状态后仍拒绝写入，报告聚合不能绕过该约束。
